//! The `mcp-covenant.lock` model: a captured snapshot of a server's interface.
//!
//! A lockfile is the *baseline* you commit to your repo. `check` re-captures the live
//! surface and diffs it against this file. The format is plain, pretty-printed JSON with
//! deterministic ordering (tools by name, resources by uri, prompts by name) so that an
//! unchanged server produces a byte-identical lockfile and git diffs stay clean.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::client::McpClient;
use crate::error::Error;
use crate::protocol::{Prompt, Resource, Tool};

/// The captured interface of an MCP server: everything a client can discover up front.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Surface {
    /// Tools, sorted by name.
    #[serde(default)]
    pub tools: Vec<Tool>,
    /// Resources, sorted by uri.
    #[serde(default)]
    pub resources: Vec<Resource>,
    /// Prompts, sorted by name.
    #[serde(default)]
    pub prompts: Vec<Prompt>,
}

impl Surface {
    /// Sort all collections into the canonical order used by lockfiles.
    pub fn sort(&mut self) {
        self.tools.sort_by(|a, b| a.name.cmp(&b.name));
        self.resources.sort_by(|a, b| a.uri.cmp(&b.uri));
        self.prompts.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// Server identity captured from the `initialize` handshake.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ServerMeta {
    /// Server name (`serverInfo.name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Server version (`serverInfo.version`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Negotiated MCP protocol version.
    #[serde(
        rename = "protocolVersion",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub protocol_version: Option<String>,
}

/// The committed baseline file (`mcp-covenant.lock`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Version of `mcp-covenant` that produced this file.
    pub covenant_version: String,
    /// Capture time, seconds since the Unix epoch. Informational only.
    #[serde(default)]
    pub captured_at_unix: u64,
    /// Captured server identity.
    #[serde(default)]
    pub server: ServerMeta,
    /// The captured interface.
    pub surface: Surface,
}

impl Lockfile {
    /// Build a fresh lockfile from a captured server identity + surface.
    pub fn new(server: ServerMeta, mut surface: Surface) -> Lockfile {
        surface.sort();
        Lockfile {
            covenant_version: env!("CARGO_PKG_VERSION").to_string(),
            captured_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            server,
            surface,
        }
    }

    /// Read and parse a lockfile from disk.
    pub fn read(path: impl AsRef<Path>) -> Result<Lockfile, Error> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Serialize the lockfile to a stable, pretty-printed JSON string (trailing newline).
    pub fn to_pretty(&self) -> Result<String, Error> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }

    /// Write the lockfile to disk as pretty JSON.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        std::fs::write(path, self.to_pretty()?)?;
        Ok(())
    }
}

/// Initialize the connection and capture the full discoverable surface.
///
/// `resources/list` and `prompts/list` are best-effort: a server that doesn't advertise
/// those capabilities simply yields empty collections (see [`McpClient::list_resources`]).
pub async fn capture(client: &McpClient) -> Result<(ServerMeta, Surface), Error> {
    let init = client.initialize().await?;
    let server = ServerMeta {
        name: init
            .server_info
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        version: init
            .server_info
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        protocol_version: if init.protocol_version.is_empty() {
            None
        } else {
            Some(init.protocol_version.clone())
        },
    };

    let tools: Vec<Tool> = client.list_tools().await?;
    let resources: Vec<Resource> = client.list_resources().await?;
    let prompts: Vec<Prompt> = client.list_prompts().await?;

    let mut surface = Surface {
        tools,
        resources,
        prompts,
    };
    surface.sort();
    Ok((server, surface))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str) -> Tool {
        serde_json::from_value(json!({"name": name, "inputSchema": {"type": "object"}})).unwrap()
    }

    #[test]
    fn surface_sorts_deterministically() {
        let mut s = Surface {
            tools: vec![tool("zeta"), tool("alpha")],
            ..Default::default()
        };
        s.sort();
        assert_eq!(s.tools[0].name, "alpha");
        assert_eq!(s.tools[1].name, "zeta");
    }

    #[test]
    fn lockfile_roundtrips() {
        let lf = Lockfile::new(
            ServerMeta {
                name: Some("demo".into()),
                version: Some("1.0.0".into()),
                protocol_version: Some("2025-11-25".into()),
            },
            Surface {
                tools: vec![tool("b"), tool("a")],
                ..Default::default()
            },
        );
        let text = lf.to_pretty().unwrap();
        assert!(text.ends_with('\n'));
        let back: Lockfile = serde_json::from_str(&text).unwrap();
        assert_eq!(back.surface.tools[0].name, "a"); // sorted on construction
        assert_eq!(back.server.name.as_deref(), Some("demo"));
    }
}
