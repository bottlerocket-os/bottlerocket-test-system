/*!

This is the command line interface for setting up a TestSys Cluster and running tests in it.

!*/

mod add;
mod add_file;
mod error;
mod install;
mod k8s;
mod run;
mod run_file;
mod status;

use crate::k8s::k8s_client;
use env_logger::Builder;
use error::Result;
use log::LevelFilter;
use std::path::PathBuf;
use structopt::StructOpt;

/// The command line interface for setting up a Bottlerocket TestSys cluster and running tests.
#[derive(Debug, StructOpt)]
struct Args {
    /// Set logging verbosity [trace|debug|info|warn|error]. If the environment variable `RUST_LOG`
    /// is present, it overrides the default logging behavior. See https://docs.rs/env_logger/latest
    #[structopt(long = "log-level", default_value = "info")]
    log_level: LevelFilter,
    /// Path to the kubeconfig file. Also can be passed with the KUBECONFIG environment variable.
    #[structopt(long = "kubeconfig")]
    kubeconfig: Option<PathBuf>,
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Install TestSys components into the cluster.
    Install(install::Install),
    /// Run a TestSys test.
    Run(run::Run),
    /// Check the status of a TestSys test.
    Status(status::Status),
    /// Add various components to the cluster.
    Add(add::Add),
}

#[tokio::main]
async fn main() {
    let args = Args::from_args();
    init_logger(args.log_level);
    if let Err(e) = run(args).await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<()> {
    let k8s_client = k8s_client(&args.kubeconfig).await?;
    match args.command {
        Command::Install(install) => install.run(k8s_client).await,
        Command::Run(run) => run.run(k8s_client).await,
        Command::Status(status) => status.run(k8s_client).await,
        Command::Add(add) => add.run(k8s_client).await,
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
