mod error;
mod get_secret;
mod init;
mod retry_count;
mod save_results;
mod send_results;
mod terminate;
mod test_error;

use argh::FromArgs;
use env_logger::Builder;
use error::Result;
use log::LevelFilter;
use snafu::ResultExt;
use test_agent::BootstrapData;
use test_agent::{Client, DefaultClient};

#[derive(FromArgs)]
/// The command line interface help in receiving and sending information from/to the Testsys cluster for Bash test scripts.
struct Args {
    /// set logging verbosity [trace|debug|info|warn|error]. If the environment variable `RUST_LOG`
    /// is present, it overrides the default logging behavior. See https://docs.rs/env_logger/latest
    #[argh(option, default = "LevelFilter::Info")]
    log_level: LevelFilter,

    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Command {
    /// Get secret value for a key
    GetSecret(get_secret::GetSecret),
    /// Set the Task state running, return config details
    Init(init::Init),
    /// Get number of retries allowed
    RetryCount(retry_count::RetryCount),
    /// Save test results
    SaveResults(save_results::SaveResults),
    /// Send test results
    SendResults(send_results::SendResults),
    /// Mark Task state complete, handle keep running, save all results to a tar archive.
    Terminate(terminate::Terminate),
    /// Send any error encountered
    TestError(test_error::TestError),
}

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();
    init_logger(args.log_level);
    if let Err(e) = run(args).await {
        eprintln!("{:?}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<()> {
    let bootstrap_data = BootstrapData::from_env().unwrap();
    let client = DefaultClient::new(bootstrap_data)
        .await
        .context(error::ClientSnafu)?;

    match args.command {
        Command::GetSecret(get_secret) => get_secret.run(client).await,
        Command::Init(init) => init.run(client).await,
        Command::RetryCount(retry_count) => retry_count.run(client).await,
        Command::SaveResults(save_results) => save_results.run(client).await,
        Command::SendResults(send_results) => send_results.run(client).await,
        Command::Terminate(terminate) => terminate.run(client).await,
        Command::TestError(test_error) => test_error.run(client).await,
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
