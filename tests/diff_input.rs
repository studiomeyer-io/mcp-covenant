//! Diff-engine coverage for **input** direction (`inputSchema`, caller-sent data).
//!
//! Semver model (from `src/diff.rs`): tightening what a caller may *send* breaks
//! existing callers; loosening it is additive.

mod common;

use common::*;
use mcp_covenant::{diff_surface, Severity};
use serde_json::json;

// ── tool-level add/remove ───────────────────────────────────────────────────

#[test]
fn tool_removed_is_breaking() {
    // A caller that depended on the tool can no longer call it.
    let old = surface_tools(vec![tool("a", json!({})), tool("b", json!({}))]);
    let new = surface_tools(vec![tool("a", json!({}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.removed").expect("tool.removed present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
    assert!(c.path.contains("tool:b"));
}

#[test]
fn tool_added_is_minor() {
    // Purely additive: existing callers are unaffected.
    let old = surface_tools(vec![tool("a", json!({}))]);
    let new = surface_tools(vec![tool("a", json!({})), tool("b", json!({}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.added").expect("tool.added present");
    assert_eq!(c.severity, Severity::Minor);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn tool_title_changed_is_patch() {
    // Cosmetic metadata only.
    let old = surface_tools(vec![tool_raw(
        json!({"name":"a","title":"Old","inputSchema":{"type":"object"}}),
    )]);
    let new = surface_tools(vec![tool_raw(
        json!({"name":"a","title":"New","inputSchema":{"type":"object"}}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.title.changed").expect("tool.title.changed present");
    assert_eq!(c.severity, Severity::Patch);
    assert_eq!(r.bump(), Some(Severity::Patch));
}

#[test]
fn tool_description_changed_is_patch() {
    let old = surface_tools(vec![tool_raw(
        json!({"name":"a","description":"old","inputSchema":{"type":"object"}}),
    )]);
    let new = surface_tools(vec![tool_raw(
        json!({"name":"a","description":"new","inputSchema":{"type":"object"}}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.description.changed").expect("present");
    assert_eq!(c.severity, Severity::Patch);
}

// ── required-property additions / transitions ───────────────────────────────

#[test]
fn new_required_input_property_is_breaking() {
    // A new required field means every old call is now missing an argument.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(
            json!({"x":{"type":"string"},"y":{"type":"string"}}),
            json!(["y"]),
        ),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn new_optional_input_property_is_minor() {
    // A new optional field doesn't affect existing callers.
    let old = surface_tools(vec![tool("a", obj_schema(json!({}), json!([])))]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"y":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    // No required-add variant should appear for an optional new field.
    assert!(!has_code(&r, "schema.property.required.added"));
}

#[test]
fn input_optional_to_required_is_breaking() {
    // Promoting an existing optional field to required can break callers that omit it.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!(["x"])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn input_required_to_optional_is_minor() {
    // Relaxing required → optional only loosens the contract.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!(["x"])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.relaxed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

// ── property removal: required vs optional ──────────────────────────────────

#[test]
fn input_required_property_removed_is_breaking() {
    // Removing a field the caller still sends — but it was *required*, so the engine
    // classifies the removal of a required input property as breaking.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!(["x"])),
    )]);
    let new = surface_tools(vec![tool("a", obj_schema(json!({}), json!([])))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert!(c.detail.contains("required"));
}

#[test]
fn input_optional_property_removed_is_minor() {
    // Removing an optional input field is a relaxation for the input contract.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool("a", obj_schema(json!({}), json!([])))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.removed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    assert!(c.detail.contains("optional"));
}

// ── type narrowing / widening ───────────────────────────────────────────────

#[test]
fn input_type_narrowed_is_breaking() {
    // ["string","number"] -> "string": a caller that sent a number now fails.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":["string","number"]}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn input_type_widened_is_minor() {
    // "string" -> ["string","number"]: input accepts strictly more → minor.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":["string","number"]}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

#[test]
fn input_type_string_vs_single_element_union_is_equivalent() {
    // type_set() normalizes "string" and ["string"] to the same set → no change.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":["string"]}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    assert!(!has_code(&r, "schema.type.changed"), "{:?}", r.changes);
    assert_eq!(r.bump(), None);
}

#[test]
fn input_type_swapped_entirely_is_breaking() {
    // "string" -> "integer": a removed accepted type on input is breaking.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"type":"integer"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

// ── additionalProperties open/closed ────────────────────────────────────────

#[test]
fn additional_properties_true_to_false_is_breaking() {
    // Closing the object rejects callers that sent extra fields.
    let old = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{},"additionalProperties":true}),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{},"additionalProperties":false}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.additionalProperties.restricted").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn additional_properties_false_to_true_produces_no_ap_change() {
    // Opening the object (false -> true) only *relaxes* input; the engine emits no
    // additionalProperties change at all in that direction.
    let old = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{},"additionalProperties":false}),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{},"additionalProperties":true}),
    )]);
    let r = diff_surface(&old, &new);
    assert!(
        !has_code(&r, "schema.additionalProperties.restricted"),
        "{:?}",
        r.changes
    );
    assert_eq!(r.bump(), None);
}

#[test]
fn additional_properties_absent_to_false_is_breaking() {
    // Default (absent) is treated as "open"; tightening to false is breaking on input.
    let old = surface_tools(vec![tool("a", json!({"type":"object","properties":{}}))]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{},"additionalProperties":false}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.additionalProperties.restricted").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

// ── tool with no inputSchema, identical surfaces, empty ─────────────────────

#[test]
fn tool_without_input_schema_is_handled() {
    // A tool declared without inputSchema defaults to an empty object; comparing two
    // such tools must yield no changes (no panic, no spurious diff).
    let old = surface_tools(vec![tool_raw(json!({"name":"a"}))]);
    let new = surface_tools(vec![tool_raw(json!({"name":"a"}))]);
    let r = diff_surface(&old, &new);
    assert!(r.changes.is_empty(), "{:?}", r.changes);
}

#[test]
fn empty_surfaces_have_no_changes() {
    use mcp_covenant::Surface;
    let r = diff_surface(&Surface::default(), &Surface::default());
    assert!(r.changes.is_empty());
    assert_eq!(r.bump(), None);
    assert_eq!(r.count(Severity::Breaking), 0);
    assert!(!r.has_at_least(Severity::Patch));
}

#[test]
fn identical_non_empty_surface_has_no_changes() {
    let s = surface_tools(vec![tool(
        "a",
        obj_schema(
            json!({"x":{"type":"string","description":"d"}}),
            json!(["x"]),
        ),
    )]);
    let r = diff_surface(&s, &s);
    assert!(r.changes.is_empty(), "{:?}", r.changes);
}
