use crate::constants::requeue;
use crate::error::ReconciliationError;
use crate::test_controller::context::{new_context, Context};
use crate::test_controller::reconcile::reconcile;
use futures::StreamExt;
use kube::api::ListParams;
use kube_runtime::controller::Action as RequeueAction;
use kube_runtime::{controller, Controller};
use log::{debug, error};

mod action;
mod context;
mod reconcile;

pub(super) async fn run_test_controller(client: kube::Client) {
    let context = new_context(client);
    Controller::new(context.api().clone(), ListParams::default())
        .run(reconcile, handle_reconciliation_error, context)
        .for_each(|reconciliation_result| async move {
            if let Err(reconciliation_err) = reconciliation_result {
                match &reconciliation_err {
                    controller::Error::ObjectNotFound { .. } => {
                        debug!("Object is gone: {}", reconciliation_err)
                    }
                    _ => error!("Error during reconciliation: {}", reconciliation_err),
                }
            }
        })
        .await;
}

/// `handle_reconciliation_error` is called when `reconcile` returns an error.
fn handle_reconciliation_error(e: &ReconciliationError, _: Context) -> RequeueAction {
    error!("Reconciliation error: {}", e);
    requeue()
}
