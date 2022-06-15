/*!

This is the command line interface for setting up a TestSys Cluster and running tests in it.

!*/

mod add_secret;
mod delete;
mod install;
mod logs;
mod restart;
mod restart_test;
mod results;
mod run;
mod run_file;
mod status;
mod uninstall;

use anyhow::{Context, Result};
use clap::Parser;
use env_logger::Builder;
use log::LevelFilter;
use model::test_manager::TestManager;
use std::path::PathBuf;

/// The command line interface for setting up a Bottlerocket TestSys cluster and running tests.
#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    /// Set logging verbosity [trace|debug|info|warn|error]. If the environment variable `RUST_LOG`
    /// is present, it overrides the default logging behavior. See https://docs.rs/env_logger/latest
    #[clap(long = "log-level", default_value = "info")]
    log_level: LevelFilter,
    /// Path to the kubeconfig file. Also can be passed with the KUBECONFIG environment variable.
    #[clap(long = "kubeconfig")]
    kubeconfig: Option<PathBuf>,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
enum Command {
    /// Install testsys components into the cluster.
    Install(install::Install),
    /// Uninstall all components from a testsys cluster.
    Uninstall(uninstall::Uninstall),
    /// Restart a test.
    Restart(restart::Restart),
    /// Run a testsys test.
    Run(run::Run),
    /// Get logs from testsys objects.
    Logs(logs::Logs),
    /// Add a secret to a cluster.
    AddSecret(add_secret::AddSecret),
    /// Get the status of testsys objects.
    Status(status::Status),
    /// Get the result files from a test.
    Results(results::Results),
    /// Delete objects from a testsys cluster.
    Delete(delete::Delete),
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    init_logger(args.log_level);
    if let Err(e) = run(args).await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<()> {
    let client = match args.kubeconfig {
        Some(path) => TestManager::new_from_kubeconfig_path(&path)
            .await
            .context(format!(
                "Unable to create testsys client from path '{:?}'",
                path
            ))?,
        None => TestManager::new()
            .await
            .context("Unable to create default testsys client")?,
    };
    match args.command {
        Command::Install(install) => install.run(client).await,
        Command::Uninstall(uninstall) => uninstall.run(client).await,
        Command::Restart(restart) => restart.run(client).await,
        Command::Run(run) => run.run(client).await,
        Command::Logs(logs) => logs.run(client).await,
        Command::AddSecret(add_secret) => add_secret.run(client).await,
        Command::Status(status) => status.run(client).await,
        Command::Results(results) => results.run(client).await,
        Command::Delete(delete) => delete.run(client).await,
    }
}

/// Initialize the logger with the value passed by `--log-level` (or its default) when the
/// `RUST_LOG` environment variable is not present. If present, the `RUST_LOG` environment variable
/// overrides `--log-level`/`level`.
fn init_logger(level: LevelFilter) {
    match std::env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level for this crate only.
            Builder::new()
                .filter(Some(env!("CARGO_CRATE_NAME")), level)
                .init();
        }
    }
}
