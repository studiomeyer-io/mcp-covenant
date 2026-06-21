# Contributing to mcp-covenant

Thanks for considering a contribution. `mcp-covenant` decides whether a change to an MCP
server's interface is **breaking, minor or patch** — so the bar for new code is "it encodes
a real compatibility rule, and it ships with a test that pins the classification".

## Quick Start

```sh
git clone https://github.com/studiomeyer-io/mcp-covenant
cd mcp-covenant
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings   # http off
cargo test --all-features
```

MSRV is **Rust 1.86** — CI checks it on a pinned 1.86 toolchain plus stable. Your patch
needs to compile on the floor.

## What we accept

- **New compatibility rules.** A rule in `src/diff.rs` that classifies a schema change,
  plus a unit test in the same file that pins it (one `assert_eq!(r.bump(), ...)`). State
  which direction (input vs output) it applies to and why — the semantics are mirrored.
- **New lint rules.** A schema-hygiene check in `src/lint.rs` with a positive and a clean
  test case. Keep noise low: a rule that fires on healthy real-world servers is a bug.
- **Wire-format compatibility.** If a real server's `tools/list` shape isn't parsed
  leniently, a fix in `src/protocol.rs` with a fixture from the actual server.

## What we don't accept

- Full JSON Schema validation. `mcp-covenant` models the slice of JSON Schema that MCP
  tools actually use (type / enum / properties / required / items / additionalProperties).
  A new construct is welcome only when a real server uses it.
- Rules that depend on calling tools. The whole tool is read-only by design.

## Classification cheatsheet

A change is **breaking** when an existing client could stop working: a removed tool, a new
required input field, a narrowed input type/enum, a removed output field, a widened output
enum. It is **minor** when it is purely additive for the affected side, and **patch** when
it is cosmetic (descriptions, titles). When in doubt, classify conservatively (more severe)
and write the test.
