//! Human-readable terminal rendering for diff and lint reports.
//!
//! Plain text by design — no color dependency, pipe-friendly, deterministic. Returns a
//! `String` so it is trivially unit-testable.

use crate::diff::{DiffReport, Severity};
use crate::lint::{LintLevel, LintReport};

/// Render a diff report. Groups changes by severity, most severe first, with a summary.
pub fn render_diff(r: &DiffReport) -> String {
    if r.changes.is_empty() {
        return "mcp-covenant: no interface changes — compatible. [OK]\n".to_string();
    }
    let mut out = String::new();
    let bump = r.bump().map(Severity::bump).unwrap_or("none");
    out.push_str(&format!(
        "mcp-covenant: {} change(s) — required version bump: {}\n\n",
        r.changes.len(),
        bump.to_uppercase()
    ));
    for sev in [Severity::Breaking, Severity::Minor, Severity::Patch] {
        let group: Vec<_> = r.changes.iter().filter(|c| c.severity == sev).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "  {} ({})\n",
            sev.label().to_uppercase(),
            group.len()
        ));
        for c in group {
            out.push_str(&format!(
                "    {} {} — {}  [{}]\n",
                marker(sev),
                c.path,
                c.detail,
                c.code
            ));
        }
    }
    out
}

fn marker(s: Severity) -> char {
    match s {
        Severity::Breaking => 'x',
        Severity::Minor => '+',
        Severity::Patch => '.',
    }
}

/// Render a lint report. Groups findings by level, most severe first, with a summary.
pub fn render_lint(r: &LintReport) -> String {
    if r.findings.is_empty() {
        return "mcp-covenant lint: no issues found. [OK]\n".to_string();
    }
    let mut out = String::new();
    out.push_str(&format!(
        "mcp-covenant lint: {} finding(s) ({} error, {} warning, {} info)\n\n",
        r.findings.len(),
        r.count(LintLevel::Error),
        r.count(LintLevel::Warning),
        r.count(LintLevel::Info),
    ));
    for level in [LintLevel::Error, LintLevel::Warning, LintLevel::Info] {
        let group: Vec<_> = r.findings.iter().filter(|f| f.level == level).collect();
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "  {} ({})\n",
            level.label().to_uppercase(),
            group.len()
        ));
        for f in group {
            out.push_str(&format!("    {} — {}  [{}]\n", f.path, f.message, f.code));
        }
    }
    out
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
    fn no_changes_message() {
        let out = render_diff(&DiffReport::default());
        assert!(out.contains("no interface changes"));
    }

    #[test]
    fn renders_breaking_group_and_bump() {
        let old = Surface {
            tools: vec![tool("a"), tool("b")],
            ..Default::default()
        };
        let new = Surface {
            tools: vec![tool("a")],
            ..Default::default()
        };
        let out = render_diff(&crate::diff::diff_surface(&old, &new));
        assert!(out.contains("MAJOR"));
        assert!(out.contains("BREAKING"));
        assert!(out.contains("tool.removed"));
    }

    #[test]
    fn lint_clean_message() {
        let out = render_lint(&LintReport::default());
        assert!(out.contains("no issues"));
    }
}
