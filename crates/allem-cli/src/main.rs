//! `allem` — the single-command surface. Thin binary over `allem-core`: parses args,
//! runs the engine, formats output, sets the exit code for CI gating. Errors are wrapped
//! with `anyhow` at this boundary (klayer `rust-dev`).

use allem_core::{Config, FindingStatus, Report, Severity};
use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "allem",
    version,
    about = "Polyglot codebase & dependency intelligence"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze a project and print findings (code + dependency intelligence).
    Analyze {
        /// Project root to analyze.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Pretty)]
        format: Format,
    },
    /// CI gate: exit non-zero if any actionable finding meets/exceeds the gate severity.
    Audit {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Mark a finding as a confirmed, real issue (triage).
    Confirm {
        /// The finding id (see `analyze`).
        id: String,
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Dismiss a finding as a false positive — excluded from gating (triage).
    Ignore {
        id: String,
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Apply a bounded, single-finding fix (e.g. dependency upgrade), then mark it fixed.
    Fix {
        id: String,
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show recorded triage verdicts for a project.
    Triage {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Start the MCP server (stdio) exposing the engine to agents and editors.
    Mcp,
}

#[derive(Copy, Clone, ValueEnum)]
enum Format {
    Pretty,
    Json,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("allem: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Analyze { path, format } => {
            let report = analyze(&path)?;
            match format {
                Format::Json => {
                    let json = serde_json::to_string_pretty(&report)?;
                    println!("{json}");
                }
                Format::Pretty => print_pretty(&report),
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Audit { path } => {
            let config = Config::load(&path).context("loading config")?;
            let report = analyze(&path)?;
            print_pretty(&report);
            let failed = report
                .worst_actionable_severity()
                .is_some_and(|w| w >= config.gate_severity);
            if failed {
                eprintln!(
                    "\naudit: FAIL — actionable findings at or above {:?}",
                    config.gate_severity
                );
                Ok(ExitCode::FAILURE)
            } else {
                println!("\naudit: PASS");
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::Confirm { id, path } => {
            allem_engine::set_verdict(&path, &id, FindingStatus::Confirmed)
                .context("recording verdict")?;
            println!("confirmed: {id}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Ignore { id, path } => {
            allem_engine::set_verdict(&path, &id, FindingStatus::FalsePositive)
                .context("recording verdict")?;
            println!("dismissed as false positive: {id}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Fix { id, path } => {
            let config = Config::load(&path).context("loading config")?;
            let outcome = allem_engine::apply_fix(&path, &config, &id).context("applying fix")?;
            println!("{}", outcome.message);
            Ok(if outcome.applied {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Command::Triage { path } => {
            let store = allem_core::TriageStore::load(&path).context("loading triage")?;
            if store.entries().is_empty() {
                println!("no triage verdicts recorded");
            } else {
                for (id, status) in store.entries() {
                    println!("  {status:?}\t{id}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Mcp => {
            // `allem mcp` is a stdio JSON-RPC server, not an interactive command. When run by
            // hand in a terminal it would just block on stdin forever (looks like a hang/crash),
            // so detect a TTY and print setup guidance instead. MCP clients pipe stdin (not a
            // TTY), so they serve normally.
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() {
                eprintln!("allem mcp is a stdio MCP server — your MCP client launches it; you don't run it by hand.");
                eprintln!();
                eprintln!(
                    "Add this to your MCP config (e.g. .mcp.json or claude_desktop_config.json):"
                );
                eprintln!(
                    "  {{\"mcpServers\":{{\"allem\":{{\"command\":\"npx\",\"args\":[\"-y\",\"allem\",\"mcp\"]}}}}}}"
                );
                eprintln!();
                eprintln!("For a one-off CLI report instead, run: allem analyze .");
                return Ok(ExitCode::SUCCESS);
            }
            allem_mcp::serve_stdio().context("MCP server")?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Run the engine: load config, then polyglot code intelligence + dependency safety, merged
/// into one report via `allem-engine`.
fn analyze(root: &Path) -> anyhow::Result<Report> {
    let config = Config::load(root).context("loading config")?;
    let report = allem_engine::analyze_report(root, &config).context("analysis")?;
    Ok(report)
}

fn print_pretty(report: &Report) {
    println!("Allem report for {}", report.root);
    println!(
        "  languages: {}",
        if report.languages.is_empty() {
            "(none detected)".to_string()
        } else {
            report.languages.join(", ")
        }
    );
    println!(
        "  ecosystems: {}",
        if report.ecosystems.is_empty() {
            "(none detected)".to_string()
        } else {
            report.ecosystems.join(", ")
        }
    );
    let s = &report.summary;
    println!(
        "  findings: {} total ({} critical, {} high, {} medium, {} low, {} info)",
        s.total, s.critical, s.high, s.medium, s.low, s.info
    );
    for f in &report.findings {
        let sev = severity_label(f.severity);
        let tag = match f.status {
            FindingStatus::Candidate => String::new(),
            FindingStatus::Confirmed => " {confirmed}".to_string(),
            FindingStatus::FalsePositive => " {false-positive}".to_string(),
            FindingStatus::Fixed => " {fixed}".to_string(),
        };
        println!("  - [{sev}] {}{tag} ({})", f.title, f.id);
    }
}

fn severity_label(sev: Severity) -> &'static str {
    match sev {
        Severity::Critical => "CRIT",
        Severity::High => "HIGH",
        Severity::Medium => "MED ",
        Severity::Low => "LOW ",
        Severity::Info => "INFO",
    }
}
