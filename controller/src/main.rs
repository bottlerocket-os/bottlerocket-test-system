/*!

This [controller] runs in a Kubernetes cluster and is responsible for running resource provider pods
and test agent pods when TestSys [`Test`] and [`Resource`] CRD instance is added to the cluster.

[controller]: https://kubernetes.io/docs/concepts/architecture/controller/

!*/

#![deny(
    clippy::expect_used,
    clippy::get_unwrap,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::panicking_unwrap,
    clippy::unwrap_in_result,
    clippy::unwrap_used
)]

use crate::resource_controller::run_resource_controller;
use crate::test_controller::run_test_controller;
use env_logger::Builder;
use futures::join;
use kube::Client;
use log::{error, info, LevelFilter};

mod constants;
mod error;
mod job;
mod resource_controller;
mod test_controller;

#[tokio::main]
async fn main() {
    init_logger();
    info!("Starting");

    // Initialize the k8s client from in-cluster variables or KUBECONFIG.
    let client = match Client::try_default().await {
        Ok(client) => client,
        Err(e) => {
            error!("Unable to create k8s client: {}", e);
            std::process::exit(1);
        }
    };

    // Run the controllers.
    let future_1 = run_test_controller(client.clone());
    let future_2 = run_resource_controller(client);

    let _ = join!(future_1, future_2);
}

/// The log level used when the `RUST_LOG` environment variable does not exist.
const DEFAULT_LEVEL_FILTER: LevelFilter = LevelFilter::Trace;

/// Extract the value of `RUST_LOG` if it exists, otherwise log this crate at
/// `DEFAULT_LEVEL_FILTER`.
fn init_logger() {
    match std::env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level for this crate only.
            Builder::new()
                .filter(Some(env!("CARGO_CRATE_NAME")), DEFAULT_LEVEL_FILTER)
                .filter(Some("model"), DEFAULT_LEVEL_FILTER)
                .init();
        }
    }
}
