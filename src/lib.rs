//! # mcp-covenant
//!
//! Contract & breaking-change detection for [Model Context Protocol](https://modelcontextprotocol.io)
//! servers — *semver for your MCP interface*.
//!
//! Capture a server's discoverable surface (`tools/list` + `resources/list` +
//! `prompts/list`) into a committed [`Lockfile`], then in CI diff the live surface against
//! it and fail the build on **breaking** changes. The [`diff`] engine is direction-aware:
//! it understands that tightening an *input* schema and loosening an *output* schema are
//! the changes that break callers, and classifies each difference as breaking / minor /
//! patch accordingly.
//!
//! The library is usable on its own; the `mcp-covenant` binary is a thin CLI over it.
//!
//! Companion to [`mcp-armor`](https://github.com/studiomeyer-io/mcp-armor) (runtime
//! defense) and [`mcp-gauntlet`](https://github.com/studiomeyer-io/mcp-gauntlet)
//! (pre-deploy fuzz + load). `mcp-covenant` is the *interface-stability* leg.
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod client;
pub mod diff;
pub mod error;
pub mod lint;
pub mod lockfile;
pub mod protocol;
pub mod report;
pub mod sarif;

pub use client::McpClient;
pub use diff::{diff_surface, Change, DiffReport, Severity};
pub use error::Error;
pub use lint::{lint_surface, LintFinding, LintLevel, LintReport};
pub use lockfile::{capture, Lockfile, ServerMeta, Surface};
pub use protocol::{Prompt, Resource, Tool};
