//! End-to-end test: launch a real stdio MCP server (a tiny python3 mock), capture its
//! surface over the wire, write + read the lockfile, and diff it.
//!
//! Skips cleanly (passes) when `python3` is not on PATH, so the suite stays green on
//! minimal environments while still proving the live path everywhere python is present
//! (including GitHub `ubuntu-latest`).

use std::process::Command;
use std::time::Duration;

use mcp_covenant::{capture, diff_surface, Lockfile, McpClient};

/// A minimal line-delimited JSON-RPC MCP server. Answers `initialize`, `tools/list` and
/// `prompts/list`; replies *method not found* to `resources/list` to exercise the
/// "capability not advertised -> empty" path in the client.
const MOCK: &str = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except Exception:
        continue
    mid = msg.get("id")
    method = msg.get("method")
    if mid is None:
        continue  # notification, no reply
    if method == "initialize":
        res = {"protocolVersion": "2025-11-25", "capabilities": {},
               "serverInfo": {"name": "mock", "version": "0.0.1"}}
    elif method == "tools/list":
        res = {"tools": [{"name": "echo", "description": "Echo input.",
               "inputSchema": {"type": "object", "properties": {"m": {"type": "string"}},
               "required": ["m"]}}]}
    elif method == "prompts/list":
        res = {"prompts": [{"name": "greet", "description": "Greet someone.",
               "arguments": [{"name": "who", "required": True}]}]}
    else:
        print(json.dumps({"jsonrpc": "2.0", "id": mid,
              "error": {"code": -32601, "message": "method not found"}}), flush=True)
        continue
    print(json.dumps({"jsonrpc": "2.0", "id": mid, "result": res}), flush=True)
"#;

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn captures_live_surface_and_roundtrips() {
    if !python3_available() {
        eprintln!("skipping: python3 not available");
        return;
    }

    let client = McpClient::connect_stdio(
        "python3",
        &["-c".to_string(), MOCK.to_string()],
        Duration::from_secs(10),
    )
    .await
    .expect("spawn mock server");

    let (meta, surface) = capture(&client).await.expect("capture surface");

    assert_eq!(meta.name.as_deref(), Some("mock"));
    assert_eq!(meta.protocol_version.as_deref(), Some("2025-11-25"));
    assert_eq!(surface.tools.len(), 1);
    assert_eq!(surface.tools[0].name, "echo");
    // resources/list returned -32601 -> treated as no resources, not an error.
    assert!(surface.resources.is_empty());
    assert_eq!(surface.prompts.len(), 1);
    assert!(surface.prompts[0].arguments[0].required);

    // snapshot -> write -> read -> diff: an unchanged surface yields zero changes.
    let lf = Lockfile::new(meta, surface);
    let tmp = std::env::temp_dir().join(format!("covenant-it-{}.lock", std::process::id()));
    lf.write(&tmp).expect("write lockfile");
    let reread = Lockfile::read(&tmp).expect("read lockfile");
    let report = diff_surface(&lf.surface, &reread.surface);
    assert!(report.changes.is_empty(), "unexpected changes: {report:?}");

    let _ = std::fs::remove_file(&tmp);
}

#[tokio::test]
async fn detects_breaking_change_end_to_end() {
    if !python3_available() {
        eprintln!("skipping: python3 not available");
        return;
    }

    let client = McpClient::connect_stdio(
        "python3",
        &["-c".to_string(), MOCK.to_string()],
        Duration::from_secs(10),
    )
    .await
    .expect("spawn mock server");
    let (_, baseline) = capture(&client).await.expect("capture surface");

    // Simulate a new release that dropped the only tool.
    let mut changed = baseline.clone();
    changed.tools.clear();

    let report = diff_surface(&baseline, &changed);
    assert!(report.has_at_least(mcp_covenant::Severity::Breaking));
    assert_eq!(report.changes[0].code, "tool.removed");
}
