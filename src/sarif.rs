//! SARIF 2.1.0 output, for GitHub code scanning / any SARIF viewer.
//!
//! Both `check` and `lint` can emit SARIF so their findings show up inline on pull
//! requests via `github/codeql-action/upload-sarif`.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::diff::{DiffReport, Severity};
use crate::lint::{LintLevel, LintReport};

const INFO_URI: &str = "https://github.com/studiomeyer-io/mcp-covenant";

/// A single SARIF result row, transport-agnostic.
struct Row {
    rule_id: &'static str,
    level: &'static str, // "error" | "warning" | "note"
    message: String,
    location: String,
}

fn severity_level(s: Severity) -> &'static str {
    match s {
        Severity::Breaking => "error",
        Severity::Minor => "warning",
        Severity::Patch => "note",
    }
}

fn lint_level(l: LintLevel) -> &'static str {
    match l {
        LintLevel::Error => "error",
        LintLevel::Warning => "warning",
        LintLevel::Info => "note",
    }
}

fn build(rows: &[Row], artifact_uri: &str) -> Value {
    // Distinct rule descriptors, required by stricter SARIF consumers.
    let rule_ids: BTreeSet<&str> = rows.iter().map(|r| r.rule_id).collect();
    let rules: Vec<Value> = rule_ids
        .iter()
        .map(|id| json!({ "id": id, "name": id }))
        .collect();

    let results: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "ruleId": r.rule_id,
                "level": r.level,
                "message": { "text": r.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": artifact_uri }
                    },
                    "logicalLocations": [{ "name": r.location, "kind": "member" }]
                }]
            })
        })
        .collect();

    json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "mcp-covenant",
                    "informationUri": INFO_URI,
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rules
                }
            },
            "results": results
        }]
    })
}

/// Render a diff report as SARIF. `artifact_uri` is the baseline lockfile path, used as the
/// physical location so GitHub anchors results to a file in the repo.
pub fn diff_to_sarif(report: &DiffReport, artifact_uri: &str) -> Value {
    let rows: Vec<Row> = report
        .changes
        .iter()
        .map(|c| Row {
            rule_id: c.code,
            level: severity_level(c.severity),
            message: format!("[{}] {} — {}", c.severity.label(), c.path, c.detail),
            location: c.path.clone(),
        })
        .collect();
    build(&rows, artifact_uri)
}

/// Render a lint report as SARIF.
pub fn lint_to_sarif(report: &LintReport, artifact_uri: &str) -> Value {
    let rows: Vec<Row> = report
        .findings
        .iter()
        .map(|f| Row {
            rule_id: f.code,
            level: lint_level(f.level),
            message: format!("[{}] {} — {}", f.level.label(), f.path, f.message),
            location: f.path.clone(),
        })
        .collect();
    build(&rows, artifact_uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::Surface;
    use crate::protocol::Tool;
    use serde_json::json;

    fn tool(name: &str) -> Tool {
        serde_json::from_value(json!({"name": name, "inputSchema": {"type": "object"}})).unwrap()
    }

    #[test]
    fn diff_sarif_has_required_shape() {
        let old = Surface {
            tools: vec![tool("a"), tool("b")],
            ..Default::default()
        };
        let new = Surface {
            tools: vec![tool("a")],
            ..Default::default()
        };
        let rep = crate::diff::diff_surface(&old, &new);
        let s = diff_to_sarif(&rep, "mcp-covenant.lock");
        assert_eq!(s["version"], "2.1.0");
        assert_eq!(s["runs"][0]["tool"]["driver"]["name"], "mcp-covenant");
        assert_eq!(s["runs"][0]["results"][0]["ruleId"], "tool.removed");
        assert_eq!(s["runs"][0]["results"][0]["level"], "error");
        // physical location present for GitHub ingestion
        assert_eq!(
            s["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
                ["uri"],
            "mcp-covenant.lock"
        );
    }

    #[test]
    fn lint_sarif_maps_levels() {
        let s = Surface {
            tools: vec![tool("a")], // no description -> warning
            ..Default::default()
        };
        let rep = crate::lint::lint_surface(&s);
        let sarif = lint_to_sarif(&rep, "mcp-covenant.lock");
        let lvl = &sarif["runs"][0]["results"][0]["level"];
        assert!(lvl == "warning" || lvl == "note");
    }
}
