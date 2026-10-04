//! Diff-engine coverage for keywords that are **absent on one side** (issue #10).
//!
//! `properties`, `required` and `items` are independent JSON Schema keywords: a schema may
//! carry any of them without the others. The diff therefore must not depend on a keyword
//! being present on both sides. An absent `properties` declares no fields, an absent
//! `items` leaves elements unconstrained.

mod common;

use common::*;
use mcp_covenant::{diff_surface, DiffReport, Severity};
use serde_json::{json, Value};
use std::process::Command;

fn bare() -> Value {
    json!({"type": "object"})
}

fn with_required_p() -> Value {
    obj_schema(json!({"p": {"type": "string"}}), json!(["p"]))
}

fn input_diff(old: Value, new: Value) -> DiffReport {
    diff_surface(
        &surface_tools(vec![tool("demo", old)]),
        &surface_tools(vec![tool("demo", new)]),
    )
}

fn output_diff(old: Value, new: Value) -> DiffReport {
    diff_surface(
        &surface_tools(vec![tool_io("demo", bare(), old)]),
        &surface_tools(vec![tool_io("demo", bare(), new)]),
    )
}

fn codes(r: &DiffReport) -> Vec<&'static str> {
    r.changes.iter().map(|c| c.code).collect()
}

// ── properties absent on one side ───────────────────────────────────────────

#[test]
fn required_property_added_to_bare_schema_is_breaking() {
    // The report in issue #10: `{}` satisfied the old schema and fails the new one.
    let r = input_diff(bare(), with_required_p());
    assert_eq!(codes(&r), ["schema.property.required.added"]);
    let c = &r.changes[0];
    assert_eq!(c.severity, Severity::Breaking);
    assert!(c.path.contains("tool:demo"));
    assert!(c.path.ends_with("inputSchema.properties.p"));
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn optional_property_added_to_bare_schema_is_minor() {
    let new = obj_schema(json!({"p": {"type": "string"}}), json!([]));
    let r = input_diff(bare(), new);
    assert_eq!(codes(&r), ["schema.property.added"]);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn dropped_properties_report_every_removed_field() {
    // The mirror: the new schema drops its property list. A required field that
    // disappears is breaking for input, an optional one is a relaxation.
    let old = obj_schema(
        json!({"p": {"type": "string"}, "q": {"type": "string"}}),
        json!(["p"]),
    );
    let r = input_diff(old, bare());
    assert_eq!(
        codes(&r),
        ["schema.property.removed", "schema.property.removed"]
    );
    let sev = |name: &str| {
        r.changes
            .iter()
            .find(|c| c.path.ends_with(name))
            .map(|c| c.severity)
    };
    assert_eq!(sev("properties.p"), Some(Severity::Breaking));
    assert_eq!(sev("properties.q"), Some(Severity::Minor));
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn bare_schema_on_both_sides_has_no_changes() {
    // Control: comparing an absent keyword as empty must not invent changes.
    assert!(input_diff(bare(), bare()).changes.is_empty());
    assert!(output_diff(bare(), bare()).changes.is_empty());
}

#[test]
fn absent_and_empty_properties_are_equivalent() {
    let empty = obj_schema(json!({}), json!([]));
    assert!(input_diff(bare(), empty.clone()).changes.is_empty());
    assert!(input_diff(empty, bare()).changes.is_empty());
}

#[test]
fn nested_bare_object_gaining_a_required_field_is_breaking() {
    let old = obj_schema(json!({"filter": {"type": "object"}}), json!([]));
    let new = obj_schema(
        json!({"filter": obj_schema(json!({"x": {"type": "string"}}), json!(["x"]))}),
        json!([]),
    );
    let r = input_diff(old, new);
    assert_eq!(codes(&r), ["schema.property.required.added"]);
    assert!(r.changes[0]
        .path
        .ends_with("inputSchema.properties.filter.properties.x"));
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

// ── the same transitions on an output schema ────────────────────────────────

#[test]
fn output_gaining_a_required_field_is_minor() {
    // Output direction: a field the caller can now rely on is a stronger guarantee.
    let r = output_diff(bare(), with_required_p());
    assert_eq!(codes(&r), ["schema.property.added"]);
    assert!(r.changes[0].path.ends_with("outputSchema.properties.p"));
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn output_dropping_its_properties_is_breaking() {
    // Output direction: a field the caller read is no longer promised.
    let r = output_diff(with_required_p(), bare());
    assert_eq!(codes(&r), ["schema.property.removed"]);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

// ── `required` naming a property the schema does not declare ────────────────

#[test]
fn required_without_a_declaration_is_breaking_for_input() {
    let new = json!({"type": "object", "required": ["p"]});
    let r = input_diff(bare(), new);
    assert_eq!(codes(&r), ["schema.property.required.added"]);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn dropping_an_undeclared_requirement_is_direction_aware() {
    let old = json!({"type": "object", "required": ["p"]});
    // Input: the caller no longer has to send the field.
    let r = input_diff(old.clone(), bare());
    assert_eq!(codes(&r), ["schema.property.required.relaxed"]);
    assert_eq!(r.bump(), Some(Severity::Minor));
    // Output: the field may now be absent.
    let r = output_diff(old, bare());
    assert_eq!(codes(&r), ["schema.property.required.relaxed"]);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn declaring_an_already_required_property_is_not_a_new_requirement() {
    // The caller had to send `p` before. Declaring it adds no obligation.
    let old = json!({"type": "object", "required": ["p"]});
    let r = input_diff(old, with_required_p());
    assert_eq!(codes(&r), ["schema.property.added"]);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

// ── items absent on one side ────────────────────────────────────────────────

/// An object schema with one array property `tags`, with or without an `items` keyword.
fn tags(items: Option<Value>) -> Value {
    let mut tags = json!({"type": "array"});
    if let Some(i) = items {
        tags["items"] = i;
    }
    obj_schema(json!({"tags": tags}), json!([]))
}

fn typed() -> Option<Value> {
    Some(json!({"type": "string"}))
}

#[test]
fn items_added_is_breaking_for_input_and_minor_for_output() {
    let r = input_diff(tags(None), tags(typed()));
    assert_eq!(codes(&r), ["schema.items.added"]);
    assert!(r.changes[0]
        .path
        .ends_with("inputSchema.properties.tags.items"));
    assert_eq!(r.bump(), Some(Severity::Breaking));

    let r = output_diff(tags(None), tags(typed()));
    assert_eq!(codes(&r), ["schema.items.added"]);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn items_removed_is_minor_for_input_and_breaking_for_output() {
    let r = input_diff(tags(typed()), tags(None));
    assert_eq!(codes(&r), ["schema.items.removed"]);
    assert_eq!(r.bump(), Some(Severity::Minor));

    let r = output_diff(tags(typed()), tags(None));
    assert_eq!(codes(&r), ["schema.items.removed"]);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn empty_items_schema_is_the_same_as_none() {
    // `items: {}` accepts every element, so it constrains nothing.
    let open = || Some(json!({}));
    assert!(input_diff(tags(None), tags(open())).changes.is_empty());
    assert!(input_diff(tags(open()), tags(None)).changes.is_empty());
    // Going from the empty schema to a real one is the same narrowing as from none.
    assert_eq!(
        codes(&input_diff(tags(open()), tags(typed()))),
        ["schema.items.added"]
    );
}

#[test]
fn unchanged_items_have_no_changes() {
    assert!(input_diff(tags(typed()), tags(typed())).changes.is_empty());
}

#[test]
fn changed_items_are_still_diffed_recursively() {
    let wide = Some(json!({"type": ["string", "number"]}));
    let r = input_diff(tags(wide), tags(typed()));
    assert_eq!(codes(&r), ["schema.type.changed"]);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn tuple_and_boolean_items_stay_unclassified() {
    // Only "absent vs. single schema" is modelled. The other forms must not be reported
    // as an added or removed constraint.
    for other in [json!([{"type": "string"}]), json!(true), json!(false)] {
        assert!(input_diff(tags(Some(other.clone())), tags(typed()))
            .changes
            .is_empty());
        assert!(input_diff(tags(typed()), tags(Some(other)))
            .changes
            .is_empty());
    }
}

// ── end to end, with the lockfiles from the report ──────────────────────────

fn lock(input_schema: Value) -> String {
    json!({
        "covenant_version": "0.1.0",
        "captured_at_unix": 0,
        "server": {"name": "example", "version": "1", "protocolVersion": "2025-11-25"},
        "surface": {
            "tools": [{"name": "demo", "inputSchema": input_schema}],
            "resources": [],
            "prompts": []
        }
    })
    .to_string()
}

/// Run `mcp-covenant check --baseline <old> --against <new> --format json`, the command
/// from the report, and return the exit code plus the parsed report.
fn check(tag: &str, old: Value, new: Value) -> (Option<i32>, Value) {
    let dir = std::env::temp_dir();
    let stem = format!("mcp-covenant-{}-{tag}", std::process::id());
    let old_path = dir.join(format!("{stem}-old.lock"));
    let new_path = dir.join(format!("{stem}-new.lock"));
    std::fs::write(&old_path, lock(old)).unwrap();
    std::fs::write(&new_path, lock(new)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mcp-covenant"))
        .arg("check")
        .arg("--baseline")
        .arg(&old_path)
        .arg("--against")
        .arg(&new_path)
        .args(["--format", "json"])
        .output()
        .expect("run mcp-covenant");
    let _ = std::fs::remove_file(&old_path);
    let _ = std::fs::remove_file(&new_path);
    let report = serde_json::from_slice(&out.stdout).expect("json report on stdout");
    (out.status.code(), report)
}

#[test]
fn cli_exits_non_zero_on_the_reported_lockfiles() {
    let (code, report) = check("added", bare(), with_required_p());
    assert_eq!(code, Some(1));
    assert_eq!(
        report["changes"][0]["code"],
        "schema.property.required.added"
    );
    assert_eq!(report["changes"][0]["severity"], "breaking");

    let (code, report) = check("mirror", with_required_p(), bare());
    assert_eq!(code, Some(1));
    assert_eq!(report["changes"][0]["code"], "schema.property.removed");

    let (code, report) = check("control", bare(), bare());
    assert_eq!(code, Some(0));
    assert_eq!(report["changes"], json!([]));
}
