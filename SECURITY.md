# Security Policy

## Reporting a vulnerability

Please report security issues privately to **security@studiomeyer.io** or via GitHub's
private vulnerability reporting ("Report a vulnerability" in the Security tab). We aim to
acknowledge within 72 hours.

## Scope & intent

`mcp-covenant` is a **read-only** client. It performs the MCP `initialize` handshake and
then calls `tools/list`, `resources/list` and `prompts/list` — it **never calls a tool**,
never writes to the server, and never executes anything it reads. Connecting `mcp-covenant`
to a server has the same side effects as a client opening its tool picker.

Only point it at servers you own or are authorized to inspect.

## Safety properties

- `#![forbid(unsafe_code)]` across the crate.
- Read-only protocol surface: `initialize` + the three `*/list` methods, nothing else.
- A hard 16 MiB per-line cap on the stdio transport prevents a hostile server from driving
  the client to unbounded memory use.
- Spawned stdio servers are killed on drop (`kill_on_drop`).
- Lockfiles are plain JSON written only to the path you choose; no schema content is ever
  executed.
