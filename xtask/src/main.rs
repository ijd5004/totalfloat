//! TotalFloat developer tooling, run as `cargo xtask <command>`.
//!
//! Corpus management, fixtures and release helpers live here rather than in
//! shell scripts so they run on every OS (AGENTS.md "Conventions").

#![forbid(unsafe_code)]

mod anonymize_fixture;
#[cfg(test)]
mod architecture;
mod corpus;
mod fixtures;
mod fresh_fixture;
mod generated;
mod history_cost;
mod rules;
#[cfg(test)]
mod skills;
mod ui;
mod ui_bench;
mod validate;
mod xer_fields;
mod xertools_fixtures;

use std::error::Error;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

/// TotalFloat developer tooling.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate authored supplemental anonymizer validation fixtures.
    AnonymizeFixture {
        #[arg(long)]
        out: PathBuf,
    },
    /// Print a greeting (placeholder until real tasks exist).
    Hello,
    /// Inventory corpus tables, fields and value shapes, never cell values.
    XerFields {
        /// Corpus directory (defaults to `TOTALFLOAT_CORPUS`).
        #[arg(long)]
        corpus: Option<PathBuf>,
    },
    /// Fetch and manage the corpus: the public schedule files
    /// corpus/manifest.toml lists (corpus/README.md).
    Corpus {
        /// Local corpus directory the files are kept in (defaults to
        /// `TOTALFLOAT_CORPUS`).
        #[arg(long, global = true)]
        corpus: Option<PathBuf>,
        #[command(subcommand)]
        action: corpus::Action,
    },
    /// Build authored synthetic fixture batches (docs/FIXTURE-SPECS.md).
    Fixtures {
        #[command(subcommand)]
        action: fixtures::Action,
    },
    /// Generate authored patch fixtures for external application validation.
    XertoolsFixtures {
        #[arg(long)]
        out: PathBuf,
    },
    /// Time recording edits in a history store on a large synthetic schedule,
    /// against the same edits file to file, and check the guard.
    HistoryCost {
        /// Activities in the synthetic schedule.
        #[arg(long, default_value_t = 50_000)]
        activities: u64,
        /// Edits to time each way.
        #[arg(long, default_value_t = 20)]
        edits: usize,
    },
    /// Time the web app on a large synthetic schedule: first paint, scrolling,
    /// an edit, a branch switch and a proposal. Needs Node.js, pnpm
    /// and playwright-cli.
    UiBench {
        /// Activities in the synthetic schedule.
        #[arg(long, default_value_t = 50_000)]
        activities: u64,
    },
    /// Check a file TotalFloat wrote with independent software: XER Tools and
    /// MPXJ for XER; Microsoft's published schema and MPXJ for MS Project XML
    /// (docs/validation/validators.md). Missing tools are skipped.
    Validate {
        #[command(subcommand)]
        action: validate::Action,
    },
    /// Generate the authored synthetic fresh-writer fixture, without scheduling.
    FreshFixture {
        /// New output path; existing files are never replaced.
        #[arg(long)]
        output: PathBuf,
    },
    /// Build the web app in `ui/` into `ui/dist/`, which `cargo build` then
    /// compiles into the binary. Needs Node.js and pnpm.
    Ui {
        /// Type check, lint and test the app instead of building it.
        #[arg(long)]
        check: bool,
        /// Refuse production dependencies whose licenses deny.toml doesn't
        /// allow (ARCHITECTURE.md, License and dependencies).
        #[arg(long)]
        licenses: bool,
    },
    /// Check that every rule in docs/rules/ records its sources: a valid
    /// class on each, anchors that exist in docs/rules/sources.md, and the
    /// tests it lists (docs/rules/README.md). Run by `just check`.
    Rules,
}

fn corpus_root(corpus: Option<PathBuf>) -> Result<PathBuf, &'static str> {
    corpus
        .or_else(|| std::env::var_os("TOTALFLOAT_CORPUS").map(PathBuf::from))
        .filter(|root| !root.as_os_str().is_empty())
        .ok_or("set TOTALFLOAT_CORPUS or pass --corpus <directory>")
}

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::AnonymizeFixture { out } => anonymize_fixture::run(&out)?,
        Command::XertoolsFixtures { out } => xertools_fixtures::run(&out)?,
        Command::Validate { action } => validate::run(&action)?,
        Command::Fixtures { action } => fixtures::run(action)?,
        Command::FreshFixture { output } => fresh_fixture::run(&output)?,
        Command::HistoryCost { activities, edits } => history_cost::run(activities, edits)?,
        Command::UiBench { activities } => ui_bench::run(activities)?,
        Command::Hello => println!("Hello from cargo xtask."),
        Command::Rules => rules::run()?,
        Command::Ui { check, licenses } => ui::run(ui::Asked { check, licenses })?,
        Command::XerFields { corpus } => {
            xer_fields::run(&corpus_root(corpus)?)?;
        }
        Command::Corpus { corpus, action } => corpus::run(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join(corpus::MANIFEST),
            &corpus_root(corpus)?,
            &action,
        )?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
