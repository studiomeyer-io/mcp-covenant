//! Coverage for the lint engine (`src/lint.rs`): every rule fired positively, plus a
//! fully clean surface that yields zero findings, plus the level-aggregation helpers.

mod common;

use common::*;
use mcp_covenant::{lint_surface, LintLevel, Surface};
use serde_json::json;

// ── tool.missing_description (Warning) ──────────────────────────────────────

#[test]
fn flags_missing_tool_description() {
    let s = surface_tools(vec![tool_raw(
        json!({"name":"a","inputSchema":{"type":"object"}}),
    )]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.missing_description")
        .expect("present");
    assert_eq!(f.level, LintLevel::Warning);
    assert!(r.has_at_least(LintLevel::Warning));
}

#[test]
fn blank_whitespace_description_counts_as_missing() {
    // is_blank() trims — a whitespace-only description still trips the rule.
    let s = surface_tools(vec![tool_raw(
        json!({"name":"a","description":"   ","inputSchema":{"type":"object"}}),
    )]);
    assert!(has_lint_code(&lint_surface(&s), "tool.missing_description"));
}

// ── tool.invalid_name (Warning) ─────────────────────────────────────────────

#[test]
fn flags_invalid_tool_name() {
    let s = surface_tools(vec![tool_raw(
        json!({"name":"bad name!","description":"d","inputSchema":{"type":"object"}}),
    )]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.invalid_name")
        .expect("present");
    assert_eq!(f.level, LintLevel::Warning);
}

#[test]
fn valid_name_with_dash_and_underscore_is_accepted() {
    // ^[A-Za-z0-9_-]{1,128}$ — dashes, underscores and digits are fine.
    let s = surface_tools(vec![tool_raw(json!({
        "name":"get_thing-2","description":"d",
        "inputSchema":{"type":"object","properties":{},"required":[]}
    }))]);
    assert!(!has_lint_code(&lint_surface(&s), "tool.invalid_name"));
}

// ── tool.required_not_in_properties (Warning) ───────────────────────────────

#[test]
fn flags_required_not_in_properties() {
    let s = surface_tools(vec![tool_raw(json!({
        "name":"a","description":"d",
        "inputSchema":{"type":"object","properties":{"x":{"type":"string","description":"d"}},"required":["y"]}
    }))]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.required_not_in_properties")
        .expect("present");
    assert_eq!(f.level, LintLevel::Warning);
    assert!(f.path.contains("y"));
}

// ── tool.duplicate_name (Error) ─────────────────────────────────────────────

#[test]
fn flags_duplicate_tool_name() {
    let s = Surface {
        tools: vec![
            tool_raw(json!({"name":"a","description":"d","inputSchema":{"type":"object"}})),
            tool_raw(json!({"name":"a","description":"d","inputSchema":{"type":"object"}})),
        ],
        ..Default::default()
    };
    let r = lint_surface(&s);
    assert!(has_lint_code(&r, "tool.duplicate_name"));
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.duplicate_name")
        .unwrap();
    assert_eq!(f.level, LintLevel::Error);
    assert!(r.has_at_least(LintLevel::Error));
    // Exactly one duplicate finding for a single repeated name.
    assert_eq!(
        r.findings
            .iter()
            .filter(|f| f.code == "tool.duplicate_name")
            .count(),
        1
    );
}

// ── tool.param.missing_description (Info) ───────────────────────────────────

#[test]
fn flags_param_missing_description() {
    // properties present, but the parameter has no description → Info.
    let s = surface_tools(vec![tool_raw(json!({
        "name":"a","description":"d",
        "inputSchema":{"type":"object","properties":{"q":{"type":"string"}}}
    }))]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.param.missing_description")
        .expect("present");
    assert_eq!(f.level, LintLevel::Info);
    assert!(f.path.contains("q"));
}

// ── tool.input_schema.not_object (Warning) ──────────────────────────────────

#[test]
fn flags_non_object_input_schema() {
    // inputSchema is a JSON array, not an object → Warning.
    let s = surface_tools(vec![tool_raw(
        json!({"name":"a","description":"d","inputSchema":[1,2,3]}),
    )]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.input_schema.not_object")
        .expect("present");
    assert_eq!(f.level, LintLevel::Warning);
}

// ── tool.input_schema.not_object_type (Info) ────────────────────────────────

#[test]
fn flags_non_object_top_level_type() {
    // A non-empty schema whose top-level type isn't "object" → Info advisory.
    let s = surface_tools(vec![tool_raw(
        json!({"name":"a","description":"d","inputSchema":{"type":"string"}}),
    )]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "tool.input_schema.not_object_type")
        .expect("present");
    assert_eq!(f.level, LintLevel::Info);
}

// ── prompt.missing_description (Info) ───────────────────────────────────────

#[test]
fn flags_prompt_missing_description() {
    let s = surface_prompts(vec![prompt_raw(json!({"name":"p"}))]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "prompt.missing_description")
        .expect("present");
    assert_eq!(f.level, LintLevel::Info);
}

// ── resource.missing_label (Info) ───────────────────────────────────────────

#[test]
fn flags_resource_missing_label() {
    // Neither name nor description → Info.
    let s = surface_resources(vec![resource_raw(json!({"uri":"file://a"}))]);
    let r = lint_surface(&s);
    let f = r
        .findings
        .iter()
        .find(|f| f.code == "resource.missing_label")
        .expect("present");
    assert_eq!(f.level, LintLevel::Info);
}

#[test]
fn resource_with_only_name_is_not_flagged() {
    // A name alone satisfies the rule (it's `description AND name` both blank that trips it).
    let s = surface_resources(vec![resource_raw(json!({"uri":"file://a","name":"A"}))]);
    assert!(!has_lint_code(&lint_surface(&s), "resource.missing_label"));
}

// ── fully clean server: zero findings ───────────────────────────────────────

#[test]
fn fully_clean_surface_has_zero_findings() {
    let s = Surface {
        tools: vec![tool_raw(json!({
            "name":"search","description":"Search the catalog.",
            "inputSchema":{"type":"object","properties":{"q":{"type":"string","description":"the query"}},"required":["q"]}
        }))],
        resources: vec![resource_raw(
            json!({"uri":"file://readme","name":"Readme","description":"The project readme."}),
        )],
        prompts: vec![prompt_raw(
            json!({"name":"greet","description":"Greet a user.","arguments":[{"name":"who","required":true,"description":"who to greet"}]}),
        )],
    };
    let r = lint_surface(&s);
    assert!(
        r.findings.is_empty(),
        "expected zero findings, got: {:?}",
        r.findings
    );
    assert!(!r.has_at_least(LintLevel::Info));
    assert_eq!(r.count(LintLevel::Info), 0);
    assert_eq!(r.count(LintLevel::Warning), 0);
    assert_eq!(r.count(LintLevel::Error), 0);
}

#[test]
fn empty_surface_lints_clean() {
    let r = lint_surface(&Surface::default());
    assert!(r.findings.is_empty());
}

// ── aggregation helpers ─────────────────────────────────────────────────────

#[test]
fn lint_counts_and_has_at_least_thresholds() {
    // One tool: invalid name (Warning) + missing description (Warning) + (params none).
    let s = surface_tools(vec![tool_raw(
        json!({"name":"bad name!","inputSchema":{"type":"object"}}),
    )]);
    let r = lint_surface(&s);
    assert!(r.count(LintLevel::Warning) >= 2);
    assert!(r.has_at_least(LintLevel::Warning));
    // No error-level finding here.
    assert!(!r.has_at_least(LintLevel::Error));
    assert_eq!(r.count(LintLevel::Error), 0);
}

#[test]
fn lint_level_ordering_and_labels() {
    assert!(LintLevel::Info < LintLevel::Warning);
    assert!(LintLevel::Warning < LintLevel::Error);
    assert_eq!(LintLevel::Error.label(), "error");
    assert_eq!(LintLevel::Warning.label(), "warning");
    assert_eq!(LintLevel::Info.label(), "info");
}
