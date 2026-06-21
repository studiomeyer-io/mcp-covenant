# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-21

Initial release.

### Added

- `snapshot` — capture a server's `tools/list` + `resources/list` + `prompts/list` into a
  deterministic `mcp-covenant.lock` baseline (stdio subprocess or Streamable HTTP).
- `check` — diff the live (or a second lockfile) interface against the baseline, classify
  every change as breaking / minor / patch with direction-aware JSON-Schema semantics, and
  exit non-zero on a configurable severity threshold (`--fail-on`).
- `lint` — schema-hygiene rules over a single surface (missing descriptions, invalid tool
  names, `required` referencing undeclared properties, duplicate tool names, …).
- Output formats: human, **SARIF 2.1.0** (for GitHub code scanning), and JSON.
- Optional `http` feature (default on) for the Streamable HTTP transport; a leaner
  stdio-only build with `--no-default-features`.
- A reusable composite GitHub Action (`action.yml`).

[Unreleased]: https://github.com/studiomeyer-io/mcp-covenant/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/studiomeyer-io/mcp-covenant/releases/tag/v0.1.0
