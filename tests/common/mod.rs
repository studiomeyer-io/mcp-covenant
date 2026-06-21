//! Shared builders for the mcp-covenant integration test-suite.
//!
//! Everything here goes through the *public* API only:
//! `serde_json::from_value::<Tool|Prompt|Resource>(json!({...}))` plus
//! `Surface { .. }`. No `src/*` internals are touched.
//!
//! `#![allow(dead_code)]` because each test crate pulls in this module
//! independently and only uses the subset of helpers it needs.
#![allow(dead_code)]

use mcp_covenant::{Prompt, Resource, Surface, Tool};
use serde_json::{json, Value};

/// Build a [`Tool`] from a name and an `inputSchema` value.
pub fn tool(name: &str, input: Value) -> Tool {
    serde_json::from_value(json!({ "name": name, "inputSchema": input })).unwrap()
}

/// Build a [`Tool`] with both an input and an output schema.
pub fn tool_io(name: &str, input: Value, output: Value) -> Tool {
    serde_json::from_value(json!({
        "name": name,
        "inputSchema": input,
        "outputSchema": output,
    }))
    .unwrap()
}

/// Build a [`Tool`] from an arbitrary raw JSON object (full control).
pub fn tool_raw(v: Value) -> Tool {
    serde_json::from_value(v).unwrap()
}

/// Build a [`Resource`] from a raw JSON object.
pub fn resource_raw(v: Value) -> Resource {
    serde_json::from_value(v).unwrap()
}

/// Build a [`Prompt`] from a raw JSON object.
pub fn prompt_raw(v: Value) -> Prompt {
    serde_json::from_value(v).unwrap()
}

/// A `{"type":"object","properties":..,"required":..}` schema.
pub fn obj_schema(props: Value, required: Value) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

/// Surface containing only the given tools.
pub fn surface_tools(tools: Vec<Tool>) -> Surface {
    Surface {
        tools,
        ..Default::default()
    }
}

/// Surface containing only the given resources.
pub fn surface_resources(resources: Vec<Resource>) -> Surface {
    Surface {
        resources,
        ..Default::default()
    }
}

/// Surface containing only the given prompts.
pub fn surface_prompts(prompts: Vec<Prompt>) -> Surface {
    Surface {
        prompts,
        ..Default::default()
    }
}

/// Look up the (first) change with a given code in a diff report.
/// Returns `None` if the code is absent.
pub fn find_code<'a>(
    report: &'a mcp_covenant::DiffReport,
    code: &str,
) -> Option<&'a mcp_covenant::Change> {
    report.changes.iter().find(|c| c.code == code)
}

/// True iff the report contains at least one change with the given code.
pub fn has_code(report: &mcp_covenant::DiffReport, code: &str) -> bool {
    report.changes.iter().any(|c| c.code == code)
}

/// True iff the lint report contains at least one finding with the given code.
pub fn has_lint_code(report: &mcp_covenant::LintReport, code: &str) -> bool {
    report.findings.iter().any(|f| f.code == code)
}
