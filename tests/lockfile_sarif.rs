//! Coverage for the lockfile model (`src/lockfile.rs`), the SARIF emitters
//! (`src/sarif.rs`), and a few Unicode / edge-case scenarios across the public API.

mod common;

use common::*;
use mcp_covenant::{diff_surface, lint_surface, sarif, Lockfile, ServerMeta, Severity, Surface};
use serde_json::json;

// ── lockfile roundtrip & determinism ────────────────────────────────────────

#[test]
fn lockfile_roundtrips_with_all_collections_and_optional_fields() {
    let surface = Surface {
        tools: vec![
            tool_raw(json!({
                "name":"b_tool","title":"B","description":"second",
                "inputSchema":{"type":"object","properties":{"x":{"type":"string","description":"d"}},"required":["x"]},
                "outputSchema":{"type":"object","properties":{"ok":{"type":"boolean"}}}
            })),
            tool_raw(
                json!({"name":"a_tool","description":"first","inputSchema":{"type":"object"}}),
            ),
        ],
        resources: vec![
            resource_raw(
                json!({"uri":"file://z","name":"Z","description":"zee","mimeType":"text/plain"}),
            ),
            resource_raw(json!({"uri":"file://a","name":"A"})),
        ],
        prompts: vec![
            prompt_raw(
                json!({"name":"q_prompt","description":"qp","arguments":[{"name":"who","required":true,"description":"w"}]}),
            ),
            prompt_raw(json!({"name":"a_prompt","description":"ap"})),
        ],
    };
    let lf = Lockfile::new(
        ServerMeta {
            name: Some("demo".into()),
            version: Some("1.2.3".into()),
            protocol_version: Some("2025-11-25".into()),
        },
        surface,
    );

    let text = lf.to_pretty().unwrap();
    assert!(
        text.ends_with('\n'),
        "pretty lockfile must end with a newline"
    );

    let back: Lockfile = serde_json::from_str(&text).unwrap();
    // sort() on construction → tools by name, resources by uri, prompts by name.
    assert_eq!(back.surface.tools[0].name, "a_tool");
    assert_eq!(back.surface.tools[1].name, "b_tool");
    assert_eq!(back.surface.resources[0].uri, "file://a");
    assert_eq!(back.surface.resources[1].uri, "file://z");
    assert_eq!(back.surface.prompts[0].name, "a_prompt");
    assert_eq!(back.surface.prompts[1].name, "q_prompt");

    // Optional fields survive the round-trip.
    assert_eq!(back.server.name.as_deref(), Some("demo"));
    assert_eq!(back.server.version.as_deref(), Some("1.2.3"));
    assert_eq!(back.server.protocol_version.as_deref(), Some("2025-11-25"));
    assert_eq!(back.surface.tools[1].title.as_deref(), Some("B"));
    assert!(back.surface.tools[1].output_schema.is_some());
    assert_eq!(
        back.surface.resources[1].mime_type.as_deref(),
        Some("text/plain")
    );
    assert!(back.surface.prompts[1].arguments[0].required);
}

#[test]
fn lockfile_serialization_is_byte_stable() {
    // Same input (already in canonical order) must serialize identically twice.
    let mk = || {
        Lockfile::new(
            ServerMeta {
                name: Some("s".into()),
                version: Some("1.0.0".into()),
                protocol_version: None,
            },
            Surface {
                tools: vec![tool_raw(
                    json!({"name":"a","inputSchema":{"type":"object"}}),
                )],
                ..Default::default()
            },
        )
    };
    // captured_at_unix is a timestamp, so compare everything *except* that line.
    let strip_ts = |s: String| {
        s.lines()
            .filter(|l| !l.contains("captured_at_unix"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        strip_ts(mk().to_pretty().unwrap()),
        strip_ts(mk().to_pretty().unwrap())
    );
}

#[test]
fn lockfile_records_covenant_version() {
    let lf = Lockfile::new(ServerMeta::default(), Surface::default());
    // Must match the crate version baked in at build time.
    assert_eq!(lf.covenant_version, env!("CARGO_PKG_VERSION"));
}

#[test]
fn lockfile_write_then_read_from_disk() {
    let lf = Lockfile::new(
        ServerMeta {
            name: Some("disk".into()),
            ..Default::default()
        },
        Surface {
            tools: vec![tool_raw(
                json!({"name":"a","inputSchema":{"type":"object"}}),
            )],
            ..Default::default()
        },
    );
    let path = std::env::temp_dir().join(format!("covenant-test-{}.lock", std::process::id()));
    lf.write(&path).expect("write");
    let back = Lockfile::read(&path).expect("read");
    assert_eq!(back.server.name.as_deref(), Some("disk"));
    assert_eq!(back.surface.tools[0].name, "a");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn surface_sort_is_idempotent_and_total() {
    let mut s = Surface {
        tools: vec![
            tool_raw(json!({"name":"zeta","inputSchema":{"type":"object"}})),
            tool_raw(json!({"name":"alpha","inputSchema":{"type":"object"}})),
        ],
        resources: vec![
            resource_raw(json!({"uri":"u://z"})),
            resource_raw(json!({"uri":"u://a"})),
        ],
        prompts: vec![
            prompt_raw(json!({"name":"y"})),
            prompt_raw(json!({"name":"b"})),
        ],
    };
    s.sort();
    assert_eq!(s.tools[0].name, "alpha");
    assert_eq!(s.resources[0].uri, "u://a");
    assert_eq!(s.prompts[0].name, "b");
    // sorting again changes nothing.
    let before = serde_json::to_value(&s).unwrap();
    s.sort();
    assert_eq!(before, serde_json::to_value(&s).unwrap());
}

// ── SARIF: diff → sarif ─────────────────────────────────────────────────────

#[test]
fn diff_sarif_top_level_shape() {
    let old = surface_tools(vec![
        tool("a", json!({"type":"object"})),
        tool("b", json!({"type":"object"})),
    ]);
    let new = surface_tools(vec![tool("a", json!({"type":"object"}))]);
    let rep = diff_surface(&old, &new);
    let s = sarif::diff_to_sarif(&rep, "mcp-covenant.lock");

    assert_eq!(s["version"], "2.1.0");
    assert_eq!(s["runs"][0]["tool"]["driver"]["name"], "mcp-covenant");
    assert_eq!(
        s["runs"][0]["tool"]["driver"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(s["runs"][0]["tool"]["driver"]["informationUri"].is_string());

    let res0 = &s["runs"][0]["results"][0];
    assert_eq!(res0["ruleId"], "tool.removed");
    assert_eq!(res0["level"], "error"); // breaking → error
                                        // physical + logical location both present
    assert_eq!(
        res0["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "mcp-covenant.lock"
    );
    assert!(res0["locations"][0]["logicalLocations"][0]["name"].is_string());

    // A rule descriptor exists for the emitted rule id.
    let rules = s["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
    assert!(rules.iter().any(|r| r["id"] == "tool.removed"));
}

#[test]
fn diff_sarif_level_mapping_breaking_minor_patch() {
    // Build a surface change that yields all three severities at once:
    // - tool removed (breaking → error)
    // - tool added (minor → warning)
    // - description changed (patch → note)
    let old = surface_tools(vec![
        tool_raw(json!({"name":"keep","description":"old","inputSchema":{"type":"object"}})),
        tool("gone", json!({"type":"object"})),
    ]);
    let new = surface_tools(vec![
        tool_raw(json!({"name":"keep","description":"new","inputSchema":{"type":"object"}})),
        tool("fresh", json!({"type":"object"})),
    ]);
    let rep = diff_surface(&old, &new);
    let s = sarif::diff_to_sarif(&rep, "x.lock");
    let results = s["runs"][0]["results"].as_array().unwrap();

    let level_for = |code: &str| -> String {
        results
            .iter()
            .find(|r| r["ruleId"] == code)
            .and_then(|r| r["level"].as_str())
            .unwrap_or("MISSING")
            .to_string()
    };
    assert_eq!(level_for("tool.removed"), "error"); // breaking
    assert_eq!(level_for("tool.added"), "warning"); // minor
    assert_eq!(level_for("tool.description.changed"), "note"); // patch
}

#[test]
fn empty_diff_sarif_has_no_results() {
    let s = sarif::diff_to_sarif(&mcp_covenant::DiffReport::default(), "x.lock");
    assert_eq!(s["version"], "2.1.0");
    assert_eq!(s["runs"][0]["results"].as_array().unwrap().len(), 0);
    assert_eq!(
        s["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

// ── SARIF: lint → sarif ─────────────────────────────────────────────────────

#[test]
fn lint_sarif_level_mapping_error_warning_info() {
    // duplicate name (Error → error) + missing description (Warning → warning)
    // + param missing description (Info → note). One surface, all three.
    let s_surface = Surface {
        tools: vec![
            tool_raw(
                json!({"name":"dup","description":"d","inputSchema":{"type":"object","properties":{"q":{"type":"string"}}}}),
            ),
            tool_raw(json!({"name":"dup","description":"d","inputSchema":{"type":"object"}})),
            tool_raw(json!({"name":"nodesc","inputSchema":{"type":"object"}})),
        ],
        ..Default::default()
    };
    let rep = lint_surface(&s_surface);
    let s = sarif::lint_to_sarif(&rep, "mcp-covenant.lock");
    assert_eq!(s["version"], "2.1.0");
    assert_eq!(s["runs"][0]["tool"]["driver"]["name"], "mcp-covenant");

    let results = s["runs"][0]["results"].as_array().unwrap();
    let level_for = |code: &str| -> String {
        results
            .iter()
            .find(|r| r["ruleId"] == code)
            .and_then(|r| r["level"].as_str())
            .unwrap_or("MISSING")
            .to_string()
    };
    assert_eq!(level_for("tool.duplicate_name"), "error"); // Error
    assert_eq!(level_for("tool.missing_description"), "warning"); // Warning
    assert_eq!(level_for("tool.param.missing_description"), "note"); // Info
                                                                     // physical location present on every lint result too
    assert_eq!(
        results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "mcp-covenant.lock"
    );
}

#[test]
fn lint_sarif_clean_surface_is_empty() {
    let clean = Surface {
        tools: vec![tool_raw(json!({
            "name":"ok","description":"fine",
            "inputSchema":{"type":"object","properties":{"q":{"type":"string","description":"d"}},"required":["q"]}
        }))],
        ..Default::default()
    };
    let s = sarif::lint_to_sarif(&lint_surface(&clean), "x.lock");
    assert_eq!(s["runs"][0]["results"].as_array().unwrap().len(), 0);
}

// ── Unicode handling across the API ─────────────────────────────────────────

#[test]
fn unicode_tool_name_and_description_roundtrip_and_diff() {
    // Names/descriptions with non-ASCII + emoji must survive serde and diff cleanly.
    let old = surface_tools(vec![tool_raw(json!({
        "name":"búsqueda","description":"Búsqueda en el catálogo 🔎",
        "inputSchema":{"type":"object"}
    }))]);
    // Same tool, only the description changes (different emoji) → patch.
    let new = surface_tools(vec![tool_raw(json!({
        "name":"búsqueda","description":"Suche im Katalog 🔍",
        "inputSchema":{"type":"object"}
    }))]);
    let r = diff_surface(&old, &new);
    assert_eq!(
        find_code(&r, "tool.description.changed").unwrap().severity,
        Severity::Patch
    );

    // And it roundtrips through a lockfile intact.
    let lf = Lockfile::new(ServerMeta::default(), old.clone());
    let back: Lockfile = serde_json::from_str(&lf.to_pretty().unwrap()).unwrap();
    assert_eq!(back.surface.tools[0].name, "búsqueda");
    assert!(back.surface.tools[0]
        .description
        .as_deref()
        .unwrap()
        .contains('🔎'));
}

#[test]
fn unicode_name_is_flagged_invalid_by_lint() {
    // The MCP name shape is ASCII-only (^[A-Za-z0-9_-]{1,128}$); a Unicode name is invalid.
    let s = surface_tools(vec![tool_raw(
        json!({"name":"búsqueda","description":"d","inputSchema":{"type":"object","properties":{},"required":[]}}),
    )]);
    assert!(has_lint_code(&lint_surface(&s), "tool.invalid_name"));
}

#[test]
fn unicode_enum_values_diff_by_canonical_json() {
    // Emoji enum values: adding one on input is a widening = minor.
    let old = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"enum":["🔴","🟢"]}}), json!([])),
    )]);
    let new = surface_tools(vec![tool(
        "a",
        obj_schema(json!({"x":{"enum":["🔴","🟢","🔵"]}}), json!([])),
    )]);
    let r = diff_surface(&old, &new);
    assert_eq!(
        find_code(&r, "schema.enum.value.added").unwrap().severity,
        Severity::Minor
    );
}
