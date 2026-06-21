//! Schema-hygiene linting for an MCP surface.
//!
//! Independent of the diff engine: `lint` judges a *single* surface against
//! best-practice rules (good descriptions, sane tool names, well-formed schemas). It fills
//! the "there is no Rust linter for MCP tool schemas" gap — the kind of issues that quietly
//! degrade how well an LLM can pick and call your tools.

use serde_json::Value;

use crate::lockfile::Surface;

/// Severity of a lint finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LintLevel {
    /// Advisory: improves model accuracy or hygiene, but nothing is broken.
    Info,
    /// Likely to hurt tool selection or callers; should be fixed.
    Warning,
    /// A real defect (e.g. duplicate tool name, malformed schema).
    Error,
}

impl LintLevel {
    /// Short label for human output.
    pub fn label(self) -> &'static str {
        match self {
            LintLevel::Info => "info",
            LintLevel::Warning => "warning",
            LintLevel::Error => "error",
        }
    }
}

/// One lint finding.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LintFinding {
    /// Severity.
    pub level: LintLevel,
    /// Stable rule id, e.g. `tool.missing_description`.
    pub code: &'static str,
    /// Where it was found, e.g. `tool:search`.
    pub path: String,
    /// What to do about it.
    pub message: String,
}

/// Result of linting a surface.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LintReport {
    /// All findings, in rule order.
    pub findings: Vec<LintFinding>,
}

impl LintReport {
    fn push(&mut self, level: LintLevel, code: &'static str, path: String, message: String) {
        self.findings.push(LintFinding {
            level,
            code,
            path,
            message,
        });
    }

    /// Whether any finding is at or above the given level.
    pub fn has_at_least(&self, level: LintLevel) -> bool {
        self.findings.iter().any(|f| f.level >= level)
    }

    /// Number of findings at exactly the given level.
    pub fn count(&self, level: LintLevel) -> usize {
        self.findings.iter().filter(|f| f.level == level).count()
    }
}

/// MCP tool/prompt names must match this shape (`^[a-zA-Z0-9_-]{1,128}$`).
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn is_blank(s: &Option<String>) -> bool {
    s.as_deref().map(|d| d.trim().is_empty()).unwrap_or(true)
}

/// Lint a full surface.
pub fn lint_surface(s: &Surface) -> LintReport {
    let mut r = LintReport::default();

    // Duplicate tool names (a server bug — the second shadows the first for most clients).
    for (i, t) in s.tools.iter().enumerate() {
        if s.tools.iter().take(i).any(|o| o.name == t.name) {
            r.push(
                LintLevel::Error,
                "tool.duplicate_name",
                format!("tool:{}", t.name),
                "duplicate tool name; clients will see only one of them".into(),
            );
        }
    }

    for t in &s.tools {
        let base = format!("tool:{}", t.name);
        if !is_valid_name(&t.name) {
            r.push(
                LintLevel::Warning,
                "tool.invalid_name",
                base.clone(),
                "name should match ^[A-Za-z0-9_-]{1,128}$ for broad client compatibility".into(),
            );
        }
        if is_blank(&t.description) {
            r.push(
                LintLevel::Warning,
                "tool.missing_description",
                base.clone(),
                "tool has no description; the model cannot tell when to call it".into(),
            );
        }
        lint_input_schema(&base, &t.input_schema, &mut r);
    }

    for p in &s.prompts {
        if is_blank(&p.description) {
            r.push(
                LintLevel::Info,
                "prompt.missing_description",
                format!("prompt:{}", p.name),
                "prompt has no description".into(),
            );
        }
    }

    for res in &s.resources {
        if is_blank(&res.description) && is_blank(&res.name) {
            r.push(
                LintLevel::Info,
                "resource.missing_label",
                format!("resource:{}", res.uri),
                "resource has neither a name nor a description".into(),
            );
        }
    }

    r
}

fn lint_input_schema(base: &str, schema: &Value, r: &mut LintReport) {
    let Some(o) = schema.as_object() else {
        r.push(
            LintLevel::Warning,
            "tool.input_schema.not_object",
            base.to_string(),
            "inputSchema should be a JSON Schema object".into(),
        );
        return;
    };

    // type should be "object" at the top level.
    if o.get("type").and_then(|v| v.as_str()) != Some("object") && !o.is_empty() {
        r.push(
            LintLevel::Info,
            "tool.input_schema.not_object_type",
            base.to_string(),
            "top-level inputSchema type is conventionally \"object\"".into(),
        );
    }

    let props = o.get("properties").and_then(|v| v.as_object());
    let required: Vec<&str> = o
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // required must reference declared properties.
    if let Some(props) = props {
        for req in &required {
            if !props.contains_key(*req) {
                r.push(
                    LintLevel::Warning,
                    "tool.required_not_in_properties",
                    format!("{base} → {req}"),
                    "required lists a property that isn't declared in properties".into(),
                );
            }
        }
        // Per-parameter descriptions help the model fill arguments correctly.
        for (name, pschema) in props {
            let described = pschema
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if !described {
                r.push(
                    LintLevel::Info,
                    "tool.param.missing_description",
                    format!("{base} → {name}"),
                    "parameter has no description".into(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Tool;
    use serde_json::json;

    fn surface_with(tool: Value) -> Surface {
        Surface {
            tools: vec![serde_json::from_value::<Tool>(tool).unwrap()],
            ..Default::default()
        }
    }

    #[test]
    fn flags_missing_description() {
        let r = lint_surface(&surface_with(
            json!({"name": "a", "inputSchema": {"type": "object"}}),
        ));
        assert!(r
            .findings
            .iter()
            .any(|f| f.code == "tool.missing_description"));
        assert!(r.has_at_least(LintLevel::Warning));
    }

    #[test]
    fn flags_invalid_name() {
        let r = lint_surface(&surface_with(
            json!({"name": "bad name!", "description": "d", "inputSchema": {"type": "object"}}),
        ));
        assert!(r.findings.iter().any(|f| f.code == "tool.invalid_name"));
    }

    #[test]
    fn flags_required_not_in_properties() {
        let r = lint_surface(&surface_with(json!({
            "name": "a", "description": "d",
            "inputSchema": {"type": "object", "properties": {"x": {"type": "string"}}, "required": ["y"]}
        })));
        assert!(r
            .findings
            .iter()
            .any(|f| f.code == "tool.required_not_in_properties"));
    }

    #[test]
    fn flags_duplicate_tool_name() {
        let s = Surface {
            tools: vec![
                serde_json::from_value(
                    json!({"name": "a", "description": "d", "inputSchema": {"type":"object"}}),
                )
                .unwrap(),
                serde_json::from_value(
                    json!({"name": "a", "description": "d", "inputSchema": {"type":"object"}}),
                )
                .unwrap(),
            ],
            ..Default::default()
        };
        let r = lint_surface(&s);
        assert!(r.has_at_least(LintLevel::Error));
        assert!(r.findings.iter().any(|f| f.code == "tool.duplicate_name"));
    }

    #[test]
    fn clean_tool_has_no_warnings_or_errors() {
        let r = lint_surface(&surface_with(json!({
            "name": "search", "description": "Search the catalog.",
            "inputSchema": {"type": "object", "properties": {"q": {"type": "string", "description": "query"}}, "required": ["q"]}
        })));
        assert!(!r.has_at_least(LintLevel::Warning), "{:?}", r.findings);
    }
}
