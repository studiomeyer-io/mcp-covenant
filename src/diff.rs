//! The diff engine — the core of `mcp-covenant`.
//!
//! Compares two [`Surface`]s and classifies every change as **breaking**, **minor** or
//! **patch**, in the spirit of semantic versioning for an MCP server's *interface*.
//!
//! The interesting part is direction-awareness. A tool's `inputSchema` describes what a
//! caller *sends*; its `outputSchema` describes what a caller *receives*. The same
//! structural change has opposite blast radius depending on direction:
//!
//! | change                         | input (caller sends) | output (caller receives) |
//! |--------------------------------|----------------------|--------------------------|
//! | new required field             | **breaking**         | minor (stronger guarantee) |
//! | field removed                  | breaking / minor     | **breaking** (field gone)  |
//! | enum value added               | minor (accepts more) | **breaking** (unknown value) |
//! | enum value removed             | **breaking** (rejects)| minor                     |
//! | type narrowed                  | **breaking**         | minor                     |
//!
//! Pure, deterministic, and fully unit-tested — it never touches the network.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::lockfile::Surface;
use crate::protocol::{Prompt, Resource, Tool};

/// Severity of a single change, ordered so the overall version bump is `changes.max()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Cosmetic: descriptions, titles, annotations. No client breaks.
    Patch,
    /// Backward-compatible addition (a new tool, a new optional field, a widened input enum).
    Minor,
    /// Backward-incompatible: an existing client may break.
    Breaking,
}

impl Severity {
    /// The conventional semver bump this severity implies.
    pub fn bump(self) -> &'static str {
        match self {
            Severity::Patch => "patch",
            Severity::Minor => "minor",
            Severity::Breaking => "major",
        }
    }

    /// Short label for human output.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Patch => "patch",
            Severity::Minor => "minor",
            Severity::Breaking => "breaking",
        }
    }
}

/// One classified difference between baseline and current surface.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Change {
    /// How severe the change is for existing clients.
    pub severity: Severity,
    /// Stable machine code, e.g. `tool.removed`, `schema.property.required.added`.
    pub code: &'static str,
    /// Human-readable location, e.g. `tool:search → inputSchema.properties.query`.
    pub path: String,
    /// One-line explanation of what changed.
    pub detail: String,
}

/// The result of diffing two surfaces.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DiffReport {
    /// Every classified change, in discovery order.
    pub changes: Vec<Change>,
}

impl DiffReport {
    /// The highest severity present, if any.
    pub fn bump(&self) -> Option<Severity> {
        self.changes.iter().map(|c| c.severity).max()
    }

    /// Number of changes at exactly the given severity.
    pub fn count(&self, sev: Severity) -> usize {
        self.changes.iter().filter(|c| c.severity == sev).count()
    }

    /// Whether any change is at or above the given severity.
    pub fn has_at_least(&self, sev: Severity) -> bool {
        self.changes.iter().any(|c| c.severity >= sev)
    }

    fn push(&mut self, severity: Severity, code: &'static str, path: String, detail: String) {
        self.changes.push(Change {
            severity,
            code,
            path,
            detail,
        });
    }
}

/// Schema direction: who is affected by a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    /// Caller-provided (a tool's `inputSchema`, a prompt argument).
    Input,
    /// Caller-received (a tool's `outputSchema`).
    Output,
}

/// Diff two complete surfaces.
pub fn diff_surface(old: &Surface, new: &Surface) -> DiffReport {
    let mut r = DiffReport::default();
    diff_tools(&old.tools, &new.tools, &mut r);
    diff_resources(&old.resources, &new.resources, &mut r);
    diff_prompts(&old.prompts, &new.prompts, &mut r);
    r
}

fn diff_tools(old: &[Tool], new: &[Tool], r: &mut DiffReport) {
    for o in old {
        match new.iter().find(|n| n.name == o.name) {
            None => r.push(
                Severity::Breaking,
                "tool.removed",
                format!("tool:{}", o.name),
                "tool was removed".into(),
            ),
            Some(n) => diff_tool(o, n, r),
        }
    }
    for n in new {
        if !old.iter().any(|o| o.name == n.name) {
            r.push(
                Severity::Minor,
                "tool.added",
                format!("tool:{}", n.name),
                "new tool".into(),
            );
        }
    }
}

fn diff_tool(o: &Tool, n: &Tool, r: &mut DiffReport) {
    let base = format!("tool:{}", o.name);
    if o.title != n.title {
        r.push(
            Severity::Patch,
            "tool.title.changed",
            base.clone(),
            "title changed".into(),
        );
    }
    if o.description != n.description {
        r.push(
            Severity::Patch,
            "tool.description.changed",
            base.clone(),
            "description changed".into(),
        );
    }
    diff_schema(
        &format!("{base} → inputSchema"),
        &o.input_schema,
        &n.input_schema,
        Dir::Input,
        r,
    );
    match (&o.output_schema, &n.output_schema) {
        (None, Some(_)) => r.push(
            Severity::Minor,
            "tool.output_schema.added",
            base.clone(),
            "tool now declares a structured output schema".into(),
        ),
        (Some(_), None) => r.push(
            Severity::Breaking,
            "tool.output_schema.removed",
            base.clone(),
            "tool dropped its structured output schema; consumers lose the contract".into(),
        ),
        (Some(a), Some(b)) => diff_schema(&format!("{base} → outputSchema"), a, b, Dir::Output, r),
        (None, None) => {}
    }
}

fn diff_resources(old: &[Resource], new: &[Resource], r: &mut DiffReport) {
    for o in old {
        match new.iter().find(|n| n.uri == o.uri) {
            None => r.push(
                Severity::Breaking,
                "resource.removed",
                format!("resource:{}", o.uri),
                "resource was removed".into(),
            ),
            Some(n) => {
                let base = format!("resource:{}", o.uri);
                if o.description != n.description {
                    r.push(
                        Severity::Patch,
                        "resource.description.changed",
                        base.clone(),
                        "description changed".into(),
                    );
                }
                if o.mime_type != n.mime_type {
                    r.push(
                        Severity::Minor,
                        "resource.mime_type.changed",
                        base.clone(),
                        format!(
                            "mimeType changed {:?} → {:?}; consumers parsing it may break",
                            o.mime_type, n.mime_type
                        ),
                    );
                }
            }
        }
    }
    for n in new {
        if !old.iter().any(|o| o.uri == n.uri) {
            r.push(
                Severity::Minor,
                "resource.added",
                format!("resource:{}", n.uri),
                "new resource".into(),
            );
        }
    }
}

fn diff_prompts(old: &[Prompt], new: &[Prompt], r: &mut DiffReport) {
    for o in old {
        match new.iter().find(|n| n.name == o.name) {
            None => r.push(
                Severity::Breaking,
                "prompt.removed",
                format!("prompt:{}", o.name),
                "prompt was removed".into(),
            ),
            Some(n) => diff_prompt(o, n, r),
        }
    }
    for n in new {
        if !old.iter().any(|o| o.name == n.name) {
            r.push(
                Severity::Minor,
                "prompt.added",
                format!("prompt:{}", n.name),
                "new prompt".into(),
            );
        }
    }
}

fn diff_prompt(o: &Prompt, n: &Prompt, r: &mut DiffReport) {
    let base = format!("prompt:{}", o.name);
    if o.description != n.description {
        r.push(
            Severity::Patch,
            "prompt.description.changed",
            base.clone(),
            "description changed".into(),
        );
    }
    // Arguments are inputs (caller-supplied).
    for oa in &o.arguments {
        match n.arguments.iter().find(|na| na.name == oa.name) {
            None => {
                let sev = if oa.required {
                    Severity::Breaking
                } else {
                    Severity::Minor
                };
                r.push(
                    sev,
                    "prompt.arg.removed",
                    format!("{base}.{}", oa.name),
                    format!(
                        "{} argument removed",
                        if oa.required { "required" } else { "optional" }
                    ),
                );
            }
            Some(na) => {
                if !oa.required && na.required {
                    r.push(
                        Severity::Breaking,
                        "prompt.arg.required.added",
                        format!("{base}.{}", oa.name),
                        "argument became required".into(),
                    );
                } else if oa.required && !na.required {
                    r.push(
                        Severity::Minor,
                        "prompt.arg.required.relaxed",
                        format!("{base}.{}", oa.name),
                        "argument became optional".into(),
                    );
                }
                if oa.description != na.description {
                    r.push(
                        Severity::Patch,
                        "prompt.arg.description.changed",
                        format!("{base}.{}", oa.name),
                        "description changed".into(),
                    );
                }
            }
        }
    }
    for na in &n.arguments {
        if !o.arguments.iter().any(|oa| oa.name == na.name) {
            let sev = if na.required {
                Severity::Breaking
            } else {
                Severity::Minor
            };
            r.push(
                sev,
                if na.required {
                    "prompt.arg.required.added"
                } else {
                    "prompt.arg.added"
                },
                format!("{base}.{}", na.name),
                format!(
                    "new {} argument",
                    if na.required { "required" } else { "optional" }
                ),
            );
        }
    }
}

// ── JSON Schema diff ────────────────────────────────────────────────────────

fn obj(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}

/// Normalize a schema's `type` keyword to a set of type names.
/// Accepts both `"string"` and `["string","null"]`. Returns `None` when absent.
fn type_set(schema: &Value) -> Option<BTreeSet<String>> {
    match schema.get("type") {
        Some(Value::String(s)) => Some([s.clone()].into_iter().collect()),
        Some(Value::Array(a)) => Some(
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        ),
        _ => None,
    }
}

fn enum_set(schema: &Value) -> Option<BTreeSet<String>> {
    schema.get("enum").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            // Compare by canonical JSON so non-string enum values are handled too.
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect()
    })
}

fn required_set(schema: &Value) -> BTreeSet<String> {
    schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `additionalProperties` as a bool, when it is one (the object form means "a schema",
/// which we treat as "extra allowed" for the purpose of the open/closed transition).
fn additional_props_closed(schema: &Value) -> bool {
    matches!(schema.get("additionalProperties"), Some(Value::Bool(false)))
}

/// Diff two JSON (sub)schemas, pushing classified changes. `dir` flips the semantics of
/// narrowing vs widening between caller-sent (input) and caller-received (output) data.
fn diff_schema(path: &str, old: &Value, new: &Value, dir: Dir, r: &mut DiffReport) {
    // type
    if let (Some(ot), Some(nt)) = (type_set(old), type_set(new)) {
        if ot != nt {
            let removed: Vec<_> = ot.difference(&nt).cloned().collect();
            let added: Vec<_> = nt.difference(&ot).cloned().collect();
            // Removing an accepted input type, or adding an emitted output type, narrows
            // the contract for the affected side.
            let breaking = match dir {
                Dir::Input => !removed.is_empty(),
                Dir::Output => !added.is_empty(),
            };
            r.push(
                if breaking {
                    Severity::Breaking
                } else {
                    Severity::Minor
                },
                "schema.type.changed",
                path.to_string(),
                format!("type changed {ot:?} → {nt:?}"),
            );
        }
    }

    // enum — both membership changes AND the presence transitions (constraint added or
    // dropped entirely). Dropping an enum on an OUTPUT field, or adding one to an INPUT
    // field, is a breaking change that an "only when both have enum" check would miss.
    match (enum_set(old), enum_set(new)) {
        (Some(oe), Some(ne)) => {
            let removed = oe.difference(&ne).count();
            let added = ne.difference(&oe).count();
            if removed > 0 {
                // Input: fewer accepted values = breaking. Output: fewer emitted = minor.
                r.push(
                    if dir == Dir::Input {
                        Severity::Breaking
                    } else {
                        Severity::Minor
                    },
                    "schema.enum.value.removed",
                    path.to_string(),
                    format!("{removed} enum value(s) removed"),
                );
            }
            if added > 0 {
                // Input: more accepted values = minor. Output: a new value a consumer may
                // not handle = breaking.
                r.push(
                    if dir == Dir::Output {
                        Severity::Breaking
                    } else {
                        Severity::Minor
                    },
                    "schema.enum.value.added",
                    path.to_string(),
                    format!("{added} enum value(s) added"),
                );
            }
        }
        (Some(_), None) => {
            // Constraint dropped: input now accepts anything (relaxation = minor); output
            // may now emit anything a consumer didn't expect (breaking).
            r.push(
                if dir == Dir::Output {
                    Severity::Breaking
                } else {
                    Severity::Minor
                },
                "schema.enum.removed",
                path.to_string(),
                "enum constraint removed (field is now unconstrained)".into(),
            );
        }
        (None, Some(_)) => {
            // Constraint added: input is now restricted (callers may break); output is now a
            // known subset (minor).
            r.push(
                if dir == Dir::Input {
                    Severity::Breaking
                } else {
                    Severity::Minor
                },
                "schema.enum.added",
                path.to_string(),
                "enum constraint added (field is now restricted to a fixed set)".into(),
            );
        }
        (None, None) => {}
    }

    // additionalProperties: open → closed restricts what an input caller may send.
    if dir == Dir::Input && !additional_props_closed(old) && additional_props_closed(new) {
        r.push(
            Severity::Breaking,
            "schema.additionalProperties.restricted",
            path.to_string(),
            "additionalProperties tightened to false; callers sending extra fields now fail".into(),
        );
    }

    // object properties + required
    if let (Some(oo), Some(no)) = (obj(old), obj(new)) {
        let old_props = oo.get("properties").and_then(|v| v.as_object());
        let new_props = no.get("properties").and_then(|v| v.as_object());
        if let (Some(op), Some(np)) = (old_props, new_props) {
            let old_req = required_set(old);
            let new_req = required_set(new);

            for (name, oschema) in op {
                let ppath = format!("{path}.properties.{name}");
                match np.get(name) {
                    None => {
                        let was_required = old_req.contains(name);
                        let sev = match dir {
                            // Output: a removed field is gone for the consumer → breaking.
                            Dir::Output => Severity::Breaking,
                            // Input: a removed required field breaks the contract; an
                            // optional one is a relaxation.
                            Dir::Input if was_required => Severity::Breaking,
                            Dir::Input => Severity::Minor,
                        };
                        r.push(
                            sev,
                            "schema.property.removed",
                            ppath,
                            format!(
                                "{} property removed",
                                if was_required { "required" } else { "optional" }
                            ),
                        );
                    }
                    Some(nschema) => {
                        // Required transition for a property present on both sides.
                        let was_req = old_req.contains(name);
                        let now_req = new_req.contains(name);
                        if !was_req && now_req {
                            r.push(
                                match dir {
                                    Dir::Input => Severity::Breaking,
                                    Dir::Output => Severity::Minor,
                                },
                                "schema.property.required.added",
                                ppath.clone(),
                                "property became required".into(),
                            );
                        } else if was_req && !now_req {
                            r.push(
                                match dir {
                                    Dir::Input => Severity::Minor,
                                    Dir::Output => Severity::Breaking,
                                },
                                "schema.property.required.relaxed",
                                ppath.clone(),
                                "property is no longer required".into(),
                            );
                        }
                        diff_schema(&ppath, oschema, nschema, dir, r);
                    }
                }
            }
            for name in np.keys() {
                if !op.contains_key(name) {
                    let now_req = new_req.contains(name);
                    let ppath = format!("{path}.properties.{name}");
                    let sev = match dir {
                        // Input: a new required field breaks existing callers.
                        Dir::Input if now_req => Severity::Breaking,
                        // Output additions and optional input additions are additive.
                        _ => Severity::Minor,
                    };
                    let code = if dir == Dir::Input && now_req {
                        "schema.property.required.added"
                    } else {
                        "schema.property.added"
                    };
                    r.push(
                        sev,
                        code,
                        ppath,
                        format!(
                            "new {} property",
                            if now_req { "required" } else { "optional" }
                        ),
                    );
                }
            }
        }

        // array items (single-schema form)
        if let (Some(oi), Some(ni)) = (
            oo.get("items").filter(|v| v.is_object()),
            no.get("items").filter(|v| v.is_object()),
        ) {
            diff_schema(&format!("{path}.items"), oi, ni, dir, r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, input: Value) -> Tool {
        serde_json::from_value(json!({"name": name, "inputSchema": input})).unwrap()
    }
    fn surface(tools: Vec<Tool>) -> Surface {
        Surface {
            tools,
            ..Default::default()
        }
    }
    fn obj_schema(props: Value, required: Value) -> Value {
        json!({"type": "object", "properties": props, "required": required})
    }

    #[test]
    fn identical_surfaces_have_no_changes() {
        let s = surface(vec![tool("a", json!({"type": "object"}))]);
        let r = diff_surface(&s, &s);
        assert!(r.changes.is_empty());
        assert_eq!(r.bump(), None);
    }

    #[test]
    fn removed_tool_is_breaking() {
        let old = surface(vec![tool("a", json!({})), tool("b", json!({}))]);
        let new = surface(vec![tool("a", json!({}))]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "tool.removed");
    }

    #[test]
    fn added_tool_is_minor() {
        let old = surface(vec![tool("a", json!({}))]);
        let new = surface(vec![tool("a", json!({})), tool("b", json!({}))]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Minor));
        assert_eq!(r.changes[0].code, "tool.added");
    }

    #[test]
    fn new_required_input_property_is_breaking() {
        let old = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!([])),
        )]);
        let new = surface(vec![tool(
            "a",
            obj_schema(
                json!({"x": {"type": "string"}, "y": {"type": "string"}}),
                json!(["y"]),
            ),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "schema.property.required.added");
    }

    #[test]
    fn new_optional_input_property_is_minor() {
        let old = surface(vec![tool("a", obj_schema(json!({}), json!([])))]);
        let new = surface(vec![tool(
            "a",
            obj_schema(json!({"y": {"type": "string"}}), json!([])),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Minor));
        assert_eq!(r.changes[0].code, "schema.property.added");
    }

    #[test]
    fn optional_to_required_input_is_breaking() {
        let old = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!([])),
        )]);
        let new = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!(["x"])),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "schema.property.required.added");
    }

    #[test]
    fn required_to_optional_input_is_minor() {
        let old = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!(["x"])),
        )]);
        let new = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!([])),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Minor));
        assert_eq!(r.changes[0].code, "schema.property.required.relaxed");
    }

    #[test]
    fn input_type_narrowed_is_breaking() {
        let old = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": ["string", "number"]}}), json!([])),
        )]);
        let new = surface(vec![tool(
            "a",
            obj_schema(json!({"x": {"type": "string"}}), json!([])),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "schema.type.changed");
    }

    #[test]
    fn input_enum_widened_is_minor_but_narrowed_is_breaking() {
        let base = |vals: Value| {
            surface(vec![tool(
                "a",
                obj_schema(json!({"x": {"enum": vals}}), json!([])),
            )])
        };
        // widened: a,b -> a,b,c  (input accepts more)
        let r = diff_surface(&base(json!(["a", "b"])), &base(json!(["a", "b", "c"])));
        assert_eq!(r.bump(), Some(Severity::Minor));
        // narrowed: a,b -> a  (input rejects b)
        let r = diff_surface(&base(json!(["a", "b"])), &base(json!(["a"])));
        assert_eq!(r.bump(), Some(Severity::Breaking));
    }

    #[test]
    fn output_semantics_are_mirrored() {
        // Adding a required field to OUTPUT is a stronger guarantee → minor.
        let mk = |out: Value| -> Tool {
            serde_json::from_value(
                json!({"name": "a", "inputSchema": {"type":"object"}, "outputSchema": out}),
            )
            .unwrap()
        };
        let old = surface(vec![mk(obj_schema(
            json!({"x": {"type": "string"}}),
            json!([]),
        ))]);
        let new = surface(vec![mk(obj_schema(
            json!({"x": {"type": "string"}, "y": {"type": "string"}}),
            json!(["y"]),
        ))]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Minor));

        // Removing an output field is breaking for consumers.
        let old = surface(vec![mk(obj_schema(
            json!({"x": {"type": "string"}}),
            json!([]),
        ))]);
        let new = surface(vec![mk(obj_schema(json!({}), json!([])))]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "schema.property.removed");
    }

    #[test]
    fn additional_properties_tightened_is_breaking() {
        let old = surface(vec![tool(
            "a",
            json!({"type": "object", "properties": {}, "additionalProperties": true}),
        )]);
        let new = surface(vec![tool(
            "a",
            json!({"type": "object", "properties": {}, "additionalProperties": false}),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "schema.additionalProperties.restricted");
    }

    #[test]
    fn description_change_is_patch() {
        let old = surface(vec![serde_json::from_value(
            json!({"name": "a", "description": "old", "inputSchema": {"type": "object"}}),
        )
        .unwrap()]);
        let new = surface(vec![serde_json::from_value(
            json!({"name": "a", "description": "new", "inputSchema": {"type": "object"}}),
        )
        .unwrap()]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Patch));
        assert_eq!(r.changes[0].code, "tool.description.changed");
    }

    #[test]
    fn nested_object_property_is_diffed_recursively() {
        let old = surface(vec![tool(
            "a",
            json!({"type":"object","properties":{"cfg":{"type":"object","properties":{"a":{"type":"string"}},"required":[]}}}),
        )]);
        let new = surface(vec![tool(
            "a",
            json!({"type":"object","properties":{"cfg":{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}},"required":["b"]}}}),
        )]);
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert!(r.changes[0].path.contains("cfg"));
    }

    #[test]
    fn removed_required_prompt_arg_is_breaking() {
        let mk = |args: Value| -> Prompt {
            serde_json::from_value(json!({"name": "p", "arguments": args})).unwrap()
        };
        let old = Surface {
            prompts: vec![mk(json!([{"name": "who", "required": true}]))],
            ..Default::default()
        };
        let new = Surface {
            prompts: vec![mk(json!([]))],
            ..Default::default()
        };
        let r = diff_surface(&old, &new);
        assert_eq!(r.bump(), Some(Severity::Breaking));
        assert_eq!(r.changes[0].code, "prompt.arg.removed");
    }
}
