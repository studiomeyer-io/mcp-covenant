<!-- studiomeyer-mcp-stack-banner:start -->
> **Part of the [StudioMeyer MCP Stack](https://studiomeyer.io)** — Built in Mallorca 🌴 · ⭐ if you use it
<!-- studiomeyer-mcp-stack-banner:end -->

# mcp-covenant

[![crates.io](https://img.shields.io/crates/v/mcp-covenant.svg)](https://crates.io/crates/mcp-covenant)
[![CI](https://github.com/studiomeyer-io/mcp-covenant/actions/workflows/ci.yml/badge.svg)](https://github.com/studiomeyer-io/mcp-covenant/actions/workflows/ci.yml)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/studiomeyer-io/mcp-covenant/badge)](https://scorecard.dev/viewer/?uri=github.com/studiomeyer-io/mcp-covenant)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Contract & breaking-change detection for [Model Context Protocol](https://modelcontextprotocol.io) servers — semver for your MCP interface.**

When you ship a typed library you have a public API and tooling that screams when you break
it. MCP servers have neither: they just serve whatever `tools/list` returns *today*. Rename a
tool, add a required argument, narrow an enum — every agent built on your server breaks, and
nothing in your pipeline noticed.

`mcp-covenant` is one static binary that snapshots your server's interface into a committed
lockfile and **fails CI when a change would break existing clients** — classified the way a
human would: breaking / minor / patch.

```text
$ mcp-covenant check --baseline mcp-covenant.lock -- node dist/server.js
mcp-covenant: 3 change(s) — required version bump: MAJOR

  BREAKING (2)
    x tool:legacy — tool was removed  [tool.removed]
    x tool:search → inputSchema.properties.limit — new required property  [schema.property.required.added]
  MINOR (1)
    + tool:newtool — new tool  [tool.added]
# exit code 1 → the job fails
```

---

## Why a Rust one

The idea isn't new — there are already several schema-drift / contract tools for MCP, all in
Node, Python or the JVM. `mcp-covenant` is the one that's **a single static binary**: nothing
to `npm install`, no Python venv, no JVM in your CI image. Drop it into any pipeline — Go
shops, Rust shops, bare Alpine containers — and it talks MCP over **stdio** or
**Streamable HTTP**.

It's also direction-aware, which is the part most drift tools get wrong (see below).

> Companion to [`mcp-armor`](https://github.com/studiomeyer-io/mcp-armor) (runtime defense)
> and [`mcp-gauntlet`](https://github.com/studiomeyer-io/mcp-gauntlet) (pre-deploy fuzz +
> load). `mcp-covenant` is the **interface-stability** leg of the trio.

---

## Install

```sh
cargo install mcp-covenant
```

Or build from source:

```sh
git clone https://github.com/studiomeyer-io/mcp-covenant
cd mcp-covenant && cargo build --release
# binary at ./target/release/mcp-covenant
```

A leaner stdio-only build (no HTTP transport, fewer dependencies):

```sh
cargo install mcp-covenant --no-default-features
```

---

## Use it in three commands

**1. Snapshot** your current interface into a baseline you commit to git:

```sh
mcp-covenant snapshot -o mcp-covenant.lock -- node dist/server.js
# or against a running HTTP server:
mcp-covenant snapshot -o mcp-covenant.lock --http https://my-server.example/mcp
```

**2. Check** in CI — non-zero exit on a breaking change:

```sh
mcp-covenant check --fail-on breaking -- node dist/server.js
```

**3. Lint** the interface for schema hygiene (the things that quietly hurt tool selection):

```sh
$ mcp-covenant lint -- node dist/server.js
mcp-covenant lint: 2 finding(s) (0 error, 1 warning, 1 info)

  WARNING (1)
    tool:newtool — tool has no description; the model cannot tell when to call it  [tool.missing_description]
  INFO (1)
    tool:search → limit — parameter has no description  [tool.param.missing_description]
```

Everything also runs **fully offline** against two lockfiles — no server needed:

```sh
mcp-covenant check --baseline v1.lock --against v2.lock
mcp-covenant lint   --from v1.lock
```

---

## How it classifies changes

The interesting part: a tool's `inputSchema` is what a caller *sends*; its `outputSchema`
is what a caller *receives*. The **same** structural change has opposite blast radius
depending on direction, and `mcp-covenant` models both.

| change | input (caller sends) | output (caller receives) |
|---|---|---|
| tool / resource / prompt removed | **breaking** | — |
| new tool / resource / prompt | minor | — |
| new **required** field | **breaking** | minor (stronger guarantee) |
| new optional field | minor | minor |
| field removed | breaking if required, else minor | **breaking** (field is gone) |
| optional → required | **breaking** | minor |
| required → optional | minor | **breaking** (may now be absent) |
| type narrowed (e.g. `["string","number"]` → `string`) | **breaking** | minor |
| enum value added | minor (accepts more) | **breaking** (unknown value) |
| enum value removed | **breaking** (rejects it) | minor |
| `additionalProperties: true → false` | **breaking** | — |
| description / title changed | patch | patch |

Nested object properties and array `items` are diffed recursively. The overall result is the
most severe change found, which maps to the semver bump you owe your users.

---

## CI

### GitHub Action

```yaml
# .github/workflows/mcp-contract.yml
name: MCP contract
on: [pull_request]
jobs:
  contract:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: studiomeyer-io/mcp-covenant@v0.1.0
        with:
          command: "node dist/server.js"   # or: http: https://my-server/mcp
          baseline: mcp-covenant.lock
          fail-on: breaking
```

### Raw, any CI

```sh
mcp-covenant check --fail-on breaking -- node dist/server.js
```

### SARIF → GitHub code scanning

```sh
mcp-covenant check --format sarif -- node dist/server.js > covenant.sarif
# then upload with github/codeql-action/upload-sarif
```

Findings show up inline on the PR, anchored to your `mcp-covenant.lock`.

---

## The lockfile

`mcp-covenant.lock` is plain, pretty-printed, deterministically ordered JSON — commit it and
review it like any other lockfile. A regeneration with no interface change is byte-identical,
so a noisy diff *is* the signal.

```jsonc
{
  "covenant_version": "0.1.0",
  "captured_at_unix": 1750000000,
  "server": { "name": "demo", "version": "1.0.0", "protocolVersion": "2025-11-25" },
  "surface": {
    "tools": [ { "name": "search", "description": "Search the catalog.", "inputSchema": { … } } ],
    "resources": [],
    "prompts": []
  }
}
```

---

## Scope

`mcp-covenant` is **read-only**: it does the `initialize` handshake and calls `tools/list`,
`resources/list`, `prompts/list` — it never calls a tool. It models the slice of JSON Schema
that MCP tools actually use (type, enum, properties, required, items, additionalProperties),
not the entire spec. See [CONTRIBUTING.md](CONTRIBUTING.md) for the classification rules and
how to add one.

## License

MIT © [StudioMeyer](https://studiomeyer.io)
