/*!

This is the [controller] for managing TestSys tests. It runs in a Kubernetes cluster and is
responsible for running resource provider pods and test agent pods when a TestSys [`Test`] CRD
instance is added to the cluster.

[controller]: https://kubernetes.io/docs/concepts/architecture/controller/

!*/

mod action;
mod context;
mod error;
mod reconcile;
mod test_pod;

use crate::reconcile::handle_reconciliation_error;
use context::new_context;
use env_logger::Builder;
use futures::stream::StreamExt;
use kube::api::ListParams;
use kube::Client;
use kube_runtime::{controller, Controller};
use log::{debug, error, info, LevelFilter};
use reconcile::reconcile;

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

    // Run the controller.
    run(client).await
}

async fn run(client: Client) {
    let context = new_context(client);
    Controller::new(context.get_ref().api(), ListParams::default())
        .run(reconcile, handle_reconciliation_error, context)
        .for_each(|reconciliation_result| async move {
            if let Err(reconciliation_err) = reconciliation_result {
                match &reconciliation_err {
                    controller::Error::ObjectNotFound { .. } => {
                        // TODO - not sure why we get this after test deletion
                        debug!("Object is gone: {}", reconciliation_err)
                    }
                    _ => error!("Error during reconciliation: {}", reconciliation_err),
                }
            }
        })
        .await;
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
                .init();
        }
    }
}
