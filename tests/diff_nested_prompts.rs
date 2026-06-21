//! Coverage for recursive schema diffing (nested objects + array items), prompt
//! arguments, resources, and the determinism / aggregation guarantees of `DiffReport`.

mod common;

use common::*;
use mcp_covenant::{diff_surface, DiffReport, Severity};
use serde_json::json;

// ── nested objects (properties in properties) ───────────────────────────────

#[test]
fn nested_object_required_field_is_recursive_and_breaking() {
    // tool a → inputSchema.properties.cfg.properties.b becomes a new required field.
    let old = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{
            "cfg":{"type":"object","properties":{"a":{"type":"string"}},"required":[]}
        }}),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{
            "cfg":{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}},"required":["b"]}
        }}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    // The path must point inside the nested cfg object.
    assert!(c.path.contains("cfg"), "path was {}", c.path);
    assert!(c.path.contains("properties.b"), "path was {}", c.path);
}

#[test]
fn deeply_nested_three_levels_type_narrowing_is_breaking() {
    // depth > 2: a.properties.b.properties.c type narrows on input.
    let mk = |inner_type: serde_json::Value| {
        surface_tools(vec![tool(
            "t",
            json!({"type":"object","properties":{
                "a":{"type":"object","properties":{
                    "b":{"type":"object","properties":{
                        "c":{"type": inner_type}
                    }}
                }}
            }}),
        )])
    };
    let old = mk(json!(["string", "number"]));
    let new = mk(json!("string"));
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.type.changed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert!(
        c.path.contains("a.properties.b.properties.c"),
        "path was {}",
        c.path
    );
}

#[test]
fn array_items_are_diffed_recursively() {
    // properties.tags.items: a new required field inside the item schema (input) is breaking.
    let old = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{
            "tags":{"type":"array","items":{"type":"object","properties":{"k":{"type":"string"}},"required":[]}}
        }}),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{
            "tags":{"type":"array","items":{"type":"object","properties":{"k":{"type":"string"},"v":{"type":"string"}},"required":["v"]}}
        }}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.property.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert!(c.path.contains("items"), "path was {}", c.path);
}

#[test]
fn array_items_enum_added_on_input_is_breaking() {
    // items.enum introduced on an input array element → restriction → breaking.
    let old = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}}}),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        json!({"type":"object","properties":{"tags":{"type":"array","items":{"type":"string","enum":["x","y"]}}}}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "schema.enum.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert!(c.path.contains("items"));
}

// ── prompts ─────────────────────────────────────────────────────────────────

#[test]
fn prompt_removed_is_breaking_and_added_is_minor() {
    let old = surface_prompts(vec![
        prompt_raw(json!({"name":"p","description":"d"})),
        prompt_raw(json!({"name":"q","description":"d"})),
    ]);
    let new = surface_prompts(vec![
        prompt_raw(json!({"name":"p","description":"d"})),
        prompt_raw(json!({"name":"r","description":"d"})),
    ]);
    let r = diff_surface(&old, &new);
    assert_eq!(
        find_code(&r, "prompt.removed").unwrap().severity,
        Severity::Breaking
    ); // q gone
    assert_eq!(
        find_code(&r, "prompt.added").unwrap().severity,
        Severity::Minor
    ); // r new
}

#[test]
fn prompt_description_change_is_patch() {
    let old = surface_prompts(vec![prompt_raw(json!({"name":"p","description":"old"}))]);
    let new = surface_prompts(vec![prompt_raw(json!({"name":"p","description":"new"}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.description.changed").expect("present");
    assert_eq!(c.severity, Severity::Patch);
}

#[test]
fn prompt_required_arg_removed_is_breaking() {
    let old = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true}]}),
    )]);
    let new = surface_prompts(vec![prompt_raw(json!({"name":"p","arguments":[]}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.removed").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
    assert!(c.detail.contains("required"));
}

#[test]
fn prompt_optional_arg_removed_is_minor() {
    let old = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"opt","required":false}]}),
    )]);
    let new = surface_prompts(vec![prompt_raw(json!({"name":"p","arguments":[]}))]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.removed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
    assert!(c.detail.contains("optional"));
}

#[test]
fn prompt_new_required_arg_is_breaking() {
    // A new required prompt argument breaks existing callers; code is required.added.
    let old = surface_prompts(vec![prompt_raw(json!({"name":"p","arguments":[]}))]);
    let new = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true}]}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn prompt_new_optional_arg_is_minor() {
    let old = surface_prompts(vec![prompt_raw(json!({"name":"p","arguments":[]}))]);
    let new = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"opt","required":false}]}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.added").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

#[test]
fn prompt_arg_optional_to_required_is_breaking() {
    let old = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":false}]}),
    )]);
    let new = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true}]}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.required.added").expect("present");
    assert_eq!(c.severity, Severity::Breaking);
}

#[test]
fn prompt_arg_required_to_optional_is_minor() {
    let old = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true}]}),
    )]);
    let new = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":false}]}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.required.relaxed").expect("present");
    assert_eq!(c.severity, Severity::Minor);
}

#[test]
fn prompt_arg_description_change_is_patch() {
    let old = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true,"description":"old"}]}),
    )]);
    let new = surface_prompts(vec![prompt_raw(
        json!({"name":"p","arguments":[{"name":"who","required":true,"description":"new"}]}),
    )]);
    let r = diff_surface(&old, &new);
    let c = find_code(&r, "prompt.arg.description.changed").expect("present");
    assert_eq!(c.severity, Severity::Patch);
}

// ── resources ───────────────────────────────────────────────────────────────

#[test]
fn resource_removed_is_breaking_added_is_minor() {
    let old = surface_resources(vec![
        resource_raw(json!({"uri":"file://a"})),
        resource_raw(json!({"uri":"file://b"})),
    ]);
    let new = surface_resources(vec![
        resource_raw(json!({"uri":"file://a"})),
        resource_raw(json!({"uri":"file://c"})),
    ]);
    let r = diff_surface(&old, &new);
    assert_eq!(
        find_code(&r, "resource.removed").unwrap().severity,
        Severity::Breaking
    );
    assert_eq!(
        find_code(&r, "resource.added").unwrap().severity,
        Severity::Minor
    );
}

#[test]
fn resource_description_change_is_patch_mime_change_is_minor() {
    let old = surface_resources(vec![resource_raw(
        json!({"uri":"file://a","description":"old","mimeType":"text/plain"}),
    )]);
    let new = surface_resources(vec![resource_raw(
        json!({"uri":"file://a","description":"new","mimeType":"application/json"}),
    )]);
    let r = diff_surface(&old, &new);
    assert_eq!(
        find_code(&r, "resource.description.changed")
            .unwrap()
            .severity,
        Severity::Patch
    );
    // mimeType change is classified Minor (consumers parsing it may break).
    assert_eq!(
        find_code(&r, "resource.mime_type.changed")
            .unwrap()
            .severity,
        Severity::Minor
    );
    assert_eq!(r.bump(), Some(Severity::Minor));
}

// ── determinism, idempotence, aggregation ───────────────────────────────────

fn breaking_and_minor_pair() -> (mcp_covenant::Surface, mcp_covenant::Surface) {
    // old: tool x (optional p), tool z (to be removed)
    // new: tool x (p now required = breaking), tool y added (minor)
    let old = surface_tools(vec![
        tool("x", obj_schema(json!({"p":{"type":"string"}}), json!([]))),
        tool("z", json!({"type":"object"})),
    ]);
    let new = surface_tools(vec![
        tool(
            "x",
            obj_schema(json!({"p":{"type":"string"}}), json!(["p"])),
        ),
        tool("y", json!({"type":"object"})),
    ]);
    (old, new)
}

#[test]
fn diff_is_idempotent() {
    let (old, new) = breaking_and_minor_pair();
    let a = diff_surface(&old, &new);
    let b = diff_surface(&old, &new);
    let codes = |r: &DiffReport| r.changes.iter().map(|c| c.code).collect::<Vec<_>>();
    assert_eq!(codes(&a), codes(&b));
    assert_eq!(a.changes.len(), b.changes.len());
}

#[test]
fn tool_order_in_surface_is_irrelevant() {
    // Tools are matched by name, so shuffling the vectors must not change the verdict.
    let a = surface_tools(vec![
        tool("a", json!({"type":"object"})),
        tool("b", json!({"type":"object"})),
    ]);
    let b = surface_tools(vec![
        tool("b", json!({"type":"object"})),
        tool("a", json!({"type":"object"})),
    ]);
    let r = diff_surface(&a, &b);
    assert!(
        r.changes.is_empty(),
        "order should not matter: {:?}",
        r.changes
    );
}

#[test]
fn bump_is_max_severity_and_counts_are_correct() {
    let (old, new) = breaking_and_minor_pair();
    let r = diff_surface(&old, &new);
    // Highest severity present is Breaking (z removed / p became required).
    assert_eq!(r.bump(), Some(Severity::Breaking));
    assert!(r.count(Severity::Breaking) >= 1);
    assert!(r.count(Severity::Minor) >= 1);
    assert!(r.has_at_least(Severity::Minor));
    assert!(r.has_at_least(Severity::Breaking));
    assert!(r.has_at_least(Severity::Patch));
    // total = sum across the three buckets
    let total = r.count(Severity::Patch) + r.count(Severity::Minor) + r.count(Severity::Breaking);
    assert_eq!(total, r.changes.len());
}

#[test]
fn severity_ordering_and_bump_labels() {
    // Enum ordering: Patch < Minor < Breaking.
    assert!(Severity::Patch < Severity::Minor);
    assert!(Severity::Minor < Severity::Breaking);
    // bump() maps to semver words; label() to the human form.
    assert_eq!(Severity::Breaking.bump(), "major");
    assert_eq!(Severity::Minor.bump(), "minor");
    assert_eq!(Severity::Patch.bump(), "patch");
    assert_eq!(Severity::Breaking.label(), "breaking");
}

#[test]
fn swapping_old_and_new_inverts_add_remove() {
    // diff(a,b) reporting "removed" implies diff(b,a) reports "added" for the same tool.
    let s1 = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let s2 = surface_tools(vec![
        tool("a", json!({"type":"object"})),
        tool("b", json!({"type":"object"})),
    ]);
    let forward = diff_surface(&s1, &s2);
    let backward = diff_surface(&s2, &s1);
    assert!(has_code(&forward, "tool.added"));
    assert!(has_code(&backward, "tool.removed"));
}
