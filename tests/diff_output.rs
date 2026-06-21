//! Diff-engine coverage for **output** direction (`outputSchema`, caller-received data).
//!
//! Mirrored semver model (from `src/diff.rs`): tightening what a caller *receives*
//! is additive (a stronger guarantee), while loosening / removing it breaks consumers.

mod common;

use common::*;
use mcp_covenant::{diff_surface, Severity};
use serde_json::json;

// ── presence of the output schema itself ────────────────────────────────────

#[test]
fn output_schema_added_is_minor() {
    // Adding a structured output schema is a stronger guarantee → additive.
    let old = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.output_schema.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn output_schema_removed_is_breaking() {
    // Dropping the output schema removes the contract consumers relied on.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "tool.output_schema.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn output_schema_none_to_none_is_noop() {
    // Neither side declares an output schema → nothing emitted for it.
    let old = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let new = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let r = diff_surface(&old, &new);
    assert!(!has_code(&r, "tool.output_schema.added"));
    assert!(!has_code(&r, "tool.output_schema.removed"));
}

// ── output field add / remove ───────────────────────────────────────────────

#[test]
fn output_field_added_required_is_minor() {
    // A new *required* output field is a stronger guarantee for the consumer → minor.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(
            json!({"x":{"type":"string"},"y":{"type":"string"}}),
            json!(["y"]),
        ),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    assert_eq!(r.bump(), Some(Severity::Minor));
}

#[test]
fn output_field_removed_is_breaking() {
    // A removed output field is simply gone for the consumer → breaking, regardless of
    // whether it was required.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert_eq!(r.bump(), Some(Severity::Breaking));
}

#[test]
fn output_optional_field_removed_is_still_breaking() {
    // Even an optional output field, once removed, is missing for the consumer.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

// ── output required transitions (mirrored) ──────────────────────────────────

#[test]
fn output_required_to_optional_is_breaking() {
    // A field the consumer could always count on is now only sometimes present → breaking.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!(["x"])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.relaxed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn output_optional_to_required_is_minor() {
    // Promoting an output field to always-present is a stronger guarantee → minor.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!(["x"])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

// ── output type narrow / widen (mirrored) ───────────────────────────────────

#[test]
fn output_type_widened_is_breaking() {
    // "string" -> ["string","number"]: the consumer may now receive a number it can't
    // handle → breaking on output (an *added* emitted type narrows the consumer contract).
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":["string","number"]}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn output_type_narrowed_is_minor() {
    // ["string","number"] -> "string": the consumer now receives a strict subset → minor.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":["string","number"]}}), json!([])),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        obj_schema(json!({"x":{"type":"string"}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

// ── additionalProperties is input-only ──────────────────────────────────────

#[test]
fn output_additional_properties_tightening_emits_nothing() {
    // The additionalProperties open->closed rule is gated to `Dir::Input`; on output it
    // must not produce a `schema.additionalProperties.restricted` change.
    let old = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        json!({"type":"object","properties":{},"additionalProperties":true}),
    )]);
    let new = surface_tools(vec![tool_io(
        "a",
        json!({"type":"object"}),
        json!({"type":"object","properties":{},"additionalProperties":false}),
    )]);
    let r = diff_surface(&old, &new);
    assert!(
        !has_code(&r, "schema.additionalProperties.restricted"),
        "{:?}",
        r.changes
    );
}
