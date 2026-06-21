//! Minimal JSON-RPC 2.0 + MCP wire types.
//!
//! Only the slice of the protocol `mcp-covenant` needs is modelled: `initialize`,
//! `tools/list`, `resources/list`, `prompts/list`. Deserialization is deliberately
//! lenient (unknown fields ignored, sensible defaults) so we stay compatible across MCP
//! spec revisions (2025-06-18 -> 2025-11-25 -> the 2026-07-28 RC).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC protocol version string.
pub const JSONRPC_VERSION: &str = "2.0";

/// Latest stable MCP protocol version this client advertises by default.
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// A decoded JSON-RPC response (or notification echo) from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    /// Request id this response correlates to. May be absent for server notifications.
    #[serde(default)]
    pub id: Option<Value>,
    /// Result payload on success.
    #[serde(default)]
    pub result: Option<Value>,
    /// Error object on failure.
    #[serde(default)]
    pub error: Option<RpcError>,
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    /// Numeric error code (e.g. `-32602` invalid params, `-32601` method not found).
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// One tool exposed by an MCP server (`tools/list` entry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    /// Unique tool name.
    pub name: String,
    /// Optional human/LLM-facing title (`title` or legacy `annotations.title`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional human/LLM-facing description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's arguments.
    #[serde(rename = "inputSchema", default = "empty_object")]
    pub input_schema: Value,
    /// Optional JSON Schema for the tool's structured output.
    #[serde(
        rename = "outputSchema",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<Value>,
}

/// One resource exposed by an MCP server (`resources/list` entry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resource {
    /// Resource URI (the identity key).
    pub uri: String,
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional MIME type.
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// One argument of a prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptArgument {
    /// Argument name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the argument is required. Absent is treated as `false`.
    #[serde(default)]
    pub required: bool,
}

/// One prompt exposed by an MCP server (`prompts/list` entry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Prompt {
    /// Unique prompt name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<PromptArgument>,
}

/// Result of a `tools/list` call.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListToolsResult {
    /// Tools advertised by the server.
    #[serde(default)]
    pub tools: Vec<Tool>,
}

/// Result of a `resources/list` call.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListResourcesResult {
    /// Resources advertised by the server.
    #[serde(default)]
    pub resources: Vec<Resource>,
}

/// Result of a `prompts/list` call.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListPromptsResult {
    /// Prompts advertised by the server.
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

/// Result of the `initialize` handshake.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InitializeResult {
    /// Protocol version the server negotiated.
    #[serde(rename = "protocolVersion", default)]
    pub protocol_version: String,
    /// Server capability advertisement.
    #[serde(default)]
    pub capabilities: Value,
    /// Server name/version metadata.
    #[serde(rename = "serverInfo", default)]
    pub server_info: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tools_list_result() {
        let raw = r#"{"tools":[{"name":"echo","description":"e","inputSchema":{"type":"object","properties":{"m":{"type":"string"}},"required":["m"]}}]}"#;
        let r: ListToolsResult = serde_json::from_str(raw).unwrap();
        assert_eq!(r.tools.len(), 1);
        assert_eq!(r.tools[0].name, "echo");
        assert_eq!(r.tools[0].input_schema["type"], "object");
    }

    #[test]
    fn tool_without_input_schema_defaults_to_empty_object() {
        let r: Tool = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
        assert!(r.input_schema.is_object());
        assert!(r.description.is_none());
    }

    #[test]
    fn parses_resources_and_prompts() {
        let r: ListResourcesResult =
            serde_json::from_str(r#"{"resources":[{"uri":"file://a","name":"A"}]}"#).unwrap();
        assert_eq!(r.resources[0].uri, "file://a");
        let p: ListPromptsResult = serde_json::from_str(
            r#"{"prompts":[{"name":"greet","arguments":[{"name":"who","required":true}]}]}"#,
        )
        .unwrap();
        assert_eq!(p.prompts[0].arguments[0].name, "who");
        assert!(p.prompts[0].arguments[0].required);
    }

    #[test]
    fn parses_rpc_error_response() {
        let r: JsonRpcResponse = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32602,"message":"bad"}}"#,
        )
        .unwrap();
        assert!(r.result.is_none());
        let e = r.error.unwrap();
        assert_eq!(e.code, -32602);
    }

    #[test]
    fn ignores_unknown_fields() {
        let r: InitializeResult = serde_json::from_str(
            r#"{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"s"},"_extra":42}"#,
        )
        .unwrap();
        assert_eq!(r.protocol_version, "2025-11-25");
    }

    #[test]
    fn tool_roundtrips_through_serde() {
        // The lockfile stores tools verbatim; serialize -> deserialize must be stable.
        let raw = r#"{"name":"echo","description":"e","inputSchema":{"type":"object"}}"#;
        let t: Tool = serde_json::from_str(raw).unwrap();
        let back = serde_json::to_value(&t).unwrap();
        let t2: Tool = serde_json::from_value(back).unwrap();
        assert_eq!(t, t2);
    }
}
