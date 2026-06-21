//! Error type shared across transports and the higher-level client.

use crate::protocol::RpcError;
use thiserror::Error;

/// Errors produced while talking to an MCP server or handling lockfiles.
#[derive(Debug, Error)]
pub enum Error {
    /// The server connection closed unexpectedly (subprocess exited / socket dropped).
    #[error("connection closed by server{}", .stderr.as_ref().map(|s| format!(" — stderr tail:\n{s}")).unwrap_or_default())]
    ConnectionClosed {
        /// Last lines the server wrote to stderr before dying, if captured.
        stderr: Option<String>,
    },

    /// The request did not receive a response within the configured timeout.
    #[error("request timed out")]
    Timeout,

    /// The server returned a well-formed JSON-RPC error object.
    #[error("JSON-RPC error {}: {}", .0.code, .0.message)]
    Rpc(RpcError),

    /// Low-level transport I/O failure.
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// (De)serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// HTTP-transport-specific failure.
    #[error("HTTP transport error: {0}")]
    Http(String),

    /// The peer violated the protocol in a way we cannot recover from.
    #[error("protocol error: {0}")]
    Protocol(String),

    /// The requested transport was compiled out (e.g. `http` feature disabled).
    #[error("{0}")]
    Unsupported(String),
}
