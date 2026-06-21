//! `mcp-covenant` — a thin CLI over the [`mcp_covenant`] library.
//!
//! ```text
//! mcp-covenant snapshot -o mcp-covenant.lock -- my-server --flag
//! mcp-covenant check --fail-on breaking      -- my-server --flag
//! mcp-covenant lint                          -- my-server --flag
//! mcp-covenant check --against new.lock       # offline: diff two lockfiles
//! ```

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Args, Parser, Subcommand, ValueEnum};

use mcp_covenant::diff::{diff_surface, DiffReport, Severity};
use mcp_covenant::lint::{lint_surface, LintLevel, LintReport};
use mcp_covenant::report::{render_diff, render_lint};
use mcp_covenant::{capture, sarif, Lockfile, McpClient, ServerMeta, Surface};

#[derive(Parser, Debug)]
#[command(
    name = "mcp-covenant",
    version,
    about = "Contract & breaking-change detector for MCP servers — semver for your interface."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Capture a server's interface into a lockfile (the committed baseline).
    Snapshot(SnapshotArgs),
    /// Diff the current interface against a baseline and exit non-zero on breaking change.
    Check(CheckArgs),
    /// Lint a server's interface for schema-hygiene issues.
    Lint(LintArgs),
}

/// How to reach the server: a Streamable HTTP URL, or a stdio command after `--`.
#[derive(Args, Debug)]
struct Target {
    /// Connect to a Streamable HTTP MCP endpoint.
    #[arg(long, value_name = "URL")]
    http: Option<String>,
    /// Per-request timeout in seconds.
    #[arg(long, default_value_t = 30, value_name = "SECS")]
    timeout: u64,
    /// Launch a stdio MCP server: everything after `--`.
    #[arg(last = true, value_name = "CMD")]
    command: Vec<String>,
}

#[derive(Args, Debug)]
struct SnapshotArgs {
    /// Where to write the lockfile.
    #[arg(
        short = 'o',
        long,
        default_value = "mcp-covenant.lock",
        value_name = "FILE"
    )]
    out: PathBuf,
    #[command(flatten)]
    target: Target,
}

#[derive(Args, Debug)]
struct CheckArgs {
    /// Baseline lockfile to diff against.
    #[arg(long, default_value = "mcp-covenant.lock", value_name = "FILE")]
    baseline: PathBuf,
    /// Diff against this lockfile instead of a live server (fully offline).
    #[arg(long, value_name = "FILE")]
    against: Option<PathBuf>,
    /// Exit non-zero when a change at or above this severity is found.
    #[arg(long, value_enum, default_value_t = FailOn::Breaking)]
    fail_on: FailOn,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,
    #[command(flatten)]
    target: Target,
}

#[derive(Args, Debug)]
struct LintArgs {
    /// Lint this lockfile instead of a live server (fully offline).
    #[arg(long, value_name = "FILE")]
    from: Option<PathBuf>,
    /// Exit non-zero when a finding at or above this level is found.
    #[arg(long, value_enum, default_value_t = LintFailOn::Error)]
    fail_on: LintFailOn,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    format: Format,
    #[command(flatten)]
    target: Target,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Format {
    Human,
    Sarif,
    Json,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum FailOn {
    Breaking,
    Minor,
    Patch,
    Never,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum LintFailOn {
    Error,
    Warning,
    Info,
    Never,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(2);
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    match cli.cmd {
        Cmd::Snapshot(a) => snapshot(a).await,
        Cmd::Check(a) => check(a).await,
        Cmd::Lint(a) => lint(a).await,
    }
}

async fn snapshot(a: SnapshotArgs) -> anyhow::Result<i32> {
    let (meta, surface) = live_surface(&a.target).await?;
    let lf = Lockfile::new(meta, surface);
    lf.write(&a.out)
        .with_context(|| format!("writing {}", a.out.display()))?;
    eprintln!(
        "mcp-covenant: captured {} tool(s), {} resource(s), {} prompt(s) → {}",
        lf.surface.tools.len(),
        lf.surface.resources.len(),
        lf.surface.prompts.len(),
        a.out.display()
    );
    Ok(0)
}

async fn check(a: CheckArgs) -> anyhow::Result<i32> {
    let baseline = Lockfile::read(&a.baseline)
        .with_context(|| format!("reading baseline {}", a.baseline.display()))?;
    let new_surface = match &a.against {
        Some(f) => {
            Lockfile::read(f)
                .with_context(|| format!("reading {}", f.display()))?
                .surface
        }
        None => live_surface(&a.target).await?.1,
    };

    let report = diff_surface(&baseline.surface, &new_surface);
    match a.format {
        Format::Human => print!("{}", render_diff(&report)),
        Format::Sarif => {
            let s = sarif::diff_to_sarif(&report, &a.baseline.display().to_string());
            println!("{}", serde_json::to_string_pretty(&s)?);
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }
    Ok(diff_exit_code(&report, a.fail_on))
}

async fn lint(a: LintArgs) -> anyhow::Result<i32> {
    let surface = match &a.from {
        Some(f) => {
            Lockfile::read(f)
                .with_context(|| format!("reading {}", f.display()))?
                .surface
        }
        None => live_surface(&a.target).await?.1,
    };

    let report = lint_surface(&surface);
    match a.format {
        Format::Human => print!("{}", render_lint(&report)),
        Format::Sarif => {
            let src = a.from.as_ref().map(|p| p.display().to_string());
            let s = sarif::lint_to_sarif(&report, src.as_deref().unwrap_or("mcp-covenant.lock"));
            println!("{}", serde_json::to_string_pretty(&s)?);
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }
    Ok(lint_exit_code(&report, a.fail_on))
}

fn diff_exit_code(report: &DiffReport, fail_on: FailOn) -> i32 {
    let threshold = match fail_on {
        FailOn::Breaking => Severity::Breaking,
        FailOn::Minor => Severity::Minor,
        FailOn::Patch => Severity::Patch,
        FailOn::Never => return 0,
    };
    if report.has_at_least(threshold) {
        1
    } else {
        0
    }
}

fn lint_exit_code(report: &LintReport, fail_on: LintFailOn) -> i32 {
    let threshold = match fail_on {
        LintFailOn::Error => LintLevel::Error,
        LintFailOn::Warning => LintLevel::Warning,
        LintFailOn::Info => LintLevel::Info,
        LintFailOn::Never => return 0,
    };
    if report.has_at_least(threshold) {
        1
    } else {
        0
    }
}

async fn live_surface(target: &Target) -> anyhow::Result<(ServerMeta, Surface)> {
    let client = connect(target).await?;
    let captured = capture(&client)
        .await
        .context("capturing server interface")?;
    Ok(captured)
}

async fn connect(target: &Target) -> anyhow::Result<McpClient> {
    let timeout = Duration::from_secs(target.timeout);
    if let Some(url) = &target.http {
        return connect_http_target(url, timeout).await;
    }
    let (prog, args) = target.command.split_first().with_context(|| {
        "no target given — pass --http <URL>, or a stdio server after `--` (e.g. `-- my-server`)"
            .to_string()
    })?;
    McpClient::connect_stdio(prog, args, timeout)
        .await
        .with_context(|| format!("launching `{prog}`"))
}

#[cfg(feature = "http")]
async fn connect_http_target(url: &str, timeout: Duration) -> anyhow::Result<McpClient> {
    Ok(McpClient::connect_http(url, timeout).await?)
}

#[cfg(not(feature = "http"))]
async fn connect_http_target(_url: &str, _timeout: Duration) -> anyhow::Result<McpClient> {
    anyhow::bail!("--http requires the `http` feature, which is disabled in this build")
}
