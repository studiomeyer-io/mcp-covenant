# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- `check` no longer reports "no changes" when one side of a schema omits `properties`
  ([#10](https://github.com/studiomeyer-io/mcp-covenant/issues/10)). An absent keyword is
  now compared as an empty property list: a new required input field is breaking, and a
  schema that drops its property list reports every removed field. Before, the whole
  properties and required comparison was skipped unless both schemas carried the keyword.
- `required` entries that name a property the schema does not declare are diffed as well.
  Declaring a property that was already required is no longer reported as a new requirement.

### Added

- `schema.items.added` and `schema.items.removed`. An array that gains an `items` schema is
  breaking for input and minor for output, one that drops it is minor for input and breaking
  for output. Until now `items` was only compared when both sides had it. An empty `items`
  schema counts as absent, the tuple and boolean forms stay unclassified.

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
