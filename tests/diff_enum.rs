//! Diff-engine coverage for **enum** changes — both membership deltas and the presence
//! transitions (enum dropped entirely / added entirely). This is the freshly-fixed
//! `schema.enum.removed` / `schema.enum.added` area, tested exhaustively in both directions.
//!
//! Model (from `src/diff.rs`):
//! - value removed: input=breaking (rejects), output=minor (emits fewer)
//! - value added:   input=minor (accepts more), output=breaking (unknown value)
//! - enum removed (Some->None): input=minor (now unconstrained), output=breaking
//! - enum added   (None->Some): input=breaking (now restricted), output=minor

mod common;

use common::*;
use mcp_covenant::{diff_surface, DiffReport, Severity};
use serde_json::{json, Value};

// Helpers that wrap a single property `x` carrying the given schema, on the chosen side.

fn input_prop(x: Value) -> Vec<mcp_covenant::Tool> {
    vec![tool("a", obj_schema(json!({ "x": x }), json!([])))]
}
fn output_prop(x: Value) -> Vec<mcp_covenant::Tool> {
    vec![tool_io(
        "a",
        json!({ "type": "object" }),
        obj_schema(json!({ "x": x }), json!([])),
    )]
}

fn diff_input(old_x: Value, new_x: Value) -> DiffReport {
    diff_surface(
        &surface_tools(input_prop(old_x)),
        &surface_tools(input_prop(new_x)),
    )
}
fn diff_output(old_x: Value, new_x: Value) -> DiffReport {
    diff_surface(
        &surface_tools(output_prop(old_x)),
        &surface_tools(output_prop(new_x)),
    )
}

// ── enum value added / removed (both enums present) ─────────────────────────

#[test]
fn input_enum_value_added_is_minor() {
    // a,b -> a,b,c : input accepts more values.
    let r = diff_input(json!({"enum":["a","b"]}), json!({"enum":["a","b","c"]}));
    let c = find_code(&r, "schema.enum.value.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    assert!(!has_code(&r, "schema.enum.value.removed"));
}

#[test]
fn input_enum_value_removed_is_breaking() {
    // a,b -> a : a caller that sent "b" is now rejected.
    let r = diff_input(json!({"enum":["a","b"]}), json!({"enum":["a"]}));
    let c = find_code(&r, "schema.enum.value.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn output_enum_value_added_is_breaking() {
    // a,b -> a,b,c : consumer may receive an unhandled "c".
    let r = diff_output(json!({"enum":["a","b"]}), json!({"enum":["a","b","c"]}));
    let c = find_code(&r, "schema.enum.value.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn output_enum_value_removed_is_minor() {
    // a,b -> a : consumer simply receives a subset of what it already handled.
    let r = diff_output(json!({"enum":["a","b"]}), json!({"enum":["a"]}));
    let c = find_code(&r, "schema.enum.value.removed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

#[test]
fn input_enum_value_added_and_removed_together() {
    // a,b -> a,c : one removed (b, breaking) + one added (c, minor). The removal dominates
    // the overall bump for an input field.
    let r = diff_input(json!({"enum":["a","b"]}), json!({"enum":["a","c"]}));
    assert!(has_code(&r, "schema.enum.value.removed"));
    assert!(has_code(&r, "schema.enum.value.added"));
    assert_eq!(
        find_code(&r, "schema.enum.value.removed").unwrap().severity,
        Severity::Breaking
    );
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

// ── enum presence transitions: removed entirely (Some -> None) ──────────────

#[test]
fn input_enum_removed_entirely_is_minor() {
    // enum dropped on input → field now unconstrained → relaxation = minor.
    let r = diff_input(json!({"enum":["a","b"]}), json!({"type":"string"}));
    let c = find_code(&r, "schema.enum.removed").expect("schema.enum.removed present");
    assert_eq!(c.severity, Severity::Minor);
    // It must NOT be reported as a per-value removal.
    assert!(!has_code(&r, "schema.enum.value.removed"));
}

#[test]
fn output_enum_removed_entirely_is_breaking() {
    // enum dropped on output → field may now emit anything → breaking for consumers.
    let r = diff_output(json!({"enum":["a","b"]}), json!({"type":"string"}));
    let c = find_code(&r, "schema.enum.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

// ── enum presence transitions: added entirely (None -> Some) ────────────────

#[test]
fn input_enum_added_entirely_is_breaking() {
    // enum introduced on input → field is now restricted → callers may break.
    let r = diff_input(json!({"type":"string"}), json!({"enum":["a","b"]}));
    let c = find_code(&r, "schema.enum.added").expect("schema.enum.added present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
    assert!(!has_code(&r, "schema.enum.value.added"));
}

#[test]
fn output_enum_added_entirely_is_minor() {
    // enum introduced on output → emitted values now a known fixed subset → minor.
    let r = diff_output(json!({"type":"string"}), json!({"enum":["a","b"]}));
    let c = find_code(&r, "schema.enum.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

// ── unchanged enum & non-string enum values ─────────────────────────────────

#[test]
fn identical_enum_yields_no_change() {
    let r = diff_input(json!({"enum":["a","b"]}), json!({"enum":["a","b"]}));
    assert!(!has_code(&r, "schema.enum.value.added"));
    assert!(!has_code(&r, "schema.enum.value.removed"));
    assert!(!has_code(&r, "schema.enum.added"));
    assert!(!has_code(&r, "schema.enum.removed"));
}

#[test]
fn enum_reordering_is_not_a_change() {
    // enum_set() is order-independent (BTreeSet of canonical JSON).
    let r = diff_input(json!({"enum":["a","b","c"]}), json!({"enum":["c","a","b"]}));
    assert!(r.changes.is_empty(), "{:?}", r.changes);
}

#[test]
fn non_string_enum_values_are_compared_canonically() {
    // enum values may be numbers/objects; they are compared by canonical JSON.
    // 1,2 -> 1,2,3 on input = a widened input enum = minor.
    let r = diff_input(json!({"enum":[1,2]}), json!({"enum":[1,2,3]}));
    let c = find_code(&r, "schema.enum.value.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

#[test]
fn non_string_enum_value_removed_on_input_is_breaking() {
    let r = diff_input(json!({"enum":[1,2,3]}), json!({"enum":[1,2]}));
    let c = find_code(&r, "schema.enum.value.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}
