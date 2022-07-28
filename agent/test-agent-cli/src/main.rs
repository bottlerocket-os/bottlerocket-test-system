mod client;
mod error;
mod get_secret;
mod init;
mod retry_count;
mod send_results;
mod terminate;
mod test_error;

use crate::client::Client;
use argh::FromArgs;
use env_logger::Builder;
use error::Result;
use log::LevelFilter;
use model::constants::ENV_TEST_NAME;
use snafu::ResultExt;

#[derive(FromArgs)]
/// This command line interface is to send/receive information from K8s cluster when creating testcase using Bash script
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
    /// Send any error encountered
    TestError(test_error::TestError),
    /// Get secret  value for a key
    GetSecret(get_secret::GetSecret),
    /// Set the Task state running, return config details
    Init(init::Init),
    /// Get number of retries allowed
    RetryCount(retry_count::RetryCount),
    /// Send test results
    SendResults(send_results::SendResults),
    /// Mark Task state complete, handle keep running, save all results as tar
    Terminate(terminate::Terminate),
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
    let test_name = get_bootstrap_data().await?;
    let k8s_client = Client::new(test_name).await?;
    match args.command {
        Command::TestError(test_error) => test_error.run(k8s_client).await,
        Command::GetSecret(get_secret) => get_secret.run(k8s_client).await,
        Command::Init(init) => init.run(k8s_client).await,
        Command::RetryCount(retry_count) => retry_count.run(k8s_client).await,
        Command::SendResults(send_results) => send_results.run(k8s_client).await,
        Command::Terminate(terminate) => terminate.run(k8s_client).await,
    }
}

// Get the test name from environment variables
async fn get_bootstrap_data() -> Result<String> {
    std::env::var(ENV_TEST_NAME).context(error::EnvReadSnafu { key: ENV_TEST_NAME })
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
