mod action;
mod context;

use crate::constants::requeue;
use crate::error::{ReconciliationError, ReconciliationResult, Result};
use crate::resource_controller::action::{
    action, Action, CreationAction, DestructionAction, ErrorState,
};
use crate::resource_controller::context::{new_context, Context, ResourceInterface};
use anyhow::Context as AnyhowContext;
use futures::StreamExt;
use kube::api::ListParams;
use kube::{Api, Client};
use kube_runtime::controller::Action as RequeueAction;
use kube_runtime::{controller, Controller};
use log::{debug, error, trace, warn};
use model::clients::CrdClient;
use model::constants::{FINALIZER_CREATION_JOB, FINALIZER_MAIN, FINALIZER_RESOURCE, NAMESPACE};
use model::{CrdExt, ErrorResources, Resource, ResourceAction, ResourceError};
use std::ops::Deref;
use std::sync::Arc;

pub(crate) async fn run_resource_controller(client: Client) {
    let context = new_context(client.clone());
    Controller::new(
        Api::<Resource>::namespaced(client, NAMESPACE),
        ListParams::default(),
    )
    .run(reconcile, handle_reconciliation_error, context)
    .for_each(|reconciliation_result| async move {
        if let Err(reconciliation_err) = reconciliation_result {
            match &reconciliation_err {
                controller::Error::ObjectNotFound { .. } => {
                    // TODO - not sure why we get this after object deletion
                    debug!("Object is gone: {}", reconciliation_err)
                }
                _ => error!("Error during reconciliation: {}", reconciliation_err),
            }
        }
    })
    .await;
}

pub(super) async fn reconcile(
    r: Arc<Resource>,
    ctx: Context,
) -> ReconciliationResult<RequeueAction> {
    let interface = ResourceInterface::new(r.deref().clone(), ctx)?;
    trace!(
        "Reconciling resource: {}",
        interface.resource().object_name()
    );

    let action = action(&interface).await?;
    trace!("Action: {:?}", action);
    match action {
        Action::Creation(creation_action) => do_creation_action(interface, creation_action).await?,
        Action::Destruction(destruction_action) => {
            do_destruction_action(interface, destruction_action).await?
        }
    }
    Ok(requeue())
}

async fn do_creation_action(r: ResourceInterface, action: CreationAction) -> Result<()> {
    match action {
        CreationAction::Initialize => {
            let _ = r
                .resource_client()
                .initialize_status(r.resource().object_name())
                .await
                .with_context(|| format!("Unable to initialize '{}'", r.name()))?;
        }
        CreationAction::AddMainFinalizer => {
            let _ = r
                .resource_client()
                .add_finalizer(FINALIZER_MAIN, r.resource())
                .await
                .with_context(|| format!("Unable to add main finalizer to '{}'", r.name()))?;
        }
        CreationAction::AddJobFinalizer => {
            let _ = r
                .resource_client()
                .add_finalizer(FINALIZER_CREATION_JOB, r.resource())
                .await
                .with_context(|| format!("Unable to creation job finalizer to '{}'", r.name()))?;
        }
        CreationAction::StartJob => r.start_job(ResourceAction::Create).await?,
        CreationAction::WaitForCreation => {
            debug!("waiting for creation of resource '{}'", r.name())
        }
        CreationAction::WaitForDependency(dependency) => {
            debug!(
                "'{}' is waiting for dependency '{}' to be created",
                r.name(),
                dependency
            );
        }
        CreationAction::WaitForConflict(conflict) => {
            debug!(
                "'{}' is waiting for conflicting resource '{}' to be destroyed",
                r.name(),
                conflict
            );
        }
        CreationAction::AddResourceFinalizer => {
            let _ = r
                .resource_client()
                .add_finalizer(FINALIZER_RESOURCE, r.resource())
                .await
                .with_context(|| format!("Unable to add resource finalizer to '{}'", r.name()))?;
        }
        CreationAction::Done => {}
        CreationAction::Error(error_state) => {
            handle_error_state(&r, ResourceAction::Create, error_state).await?
        }
    }
    Ok(())
}

async fn do_destruction_action(r: ResourceInterface, action: DestructionAction) -> Result<()> {
    match action {
        DestructionAction::StartResourceDeletion => {
            r.resource_client().delete(r.name()).await?;
        }
        DestructionAction::RemoveCreationJob => {
            r.remove_job(ResourceAction::Create).await?;
        }
        DestructionAction::RemoveCreationJobFinalizer => {
            r.resource_client()
                .remove_finalizer(FINALIZER_CREATION_JOB, r.resource())
                .await
                .with_context(|| {
                    format!(
                        "Unable to remove creation job finalizer from '{}'",
                        r.name()
                    )
                })?;
        }
        DestructionAction::StartDestructionJob => {
            r.start_job(ResourceAction::Destroy).await?;
        }
        DestructionAction::Wait => {}
        DestructionAction::RemoveDestructionJob => {
            r.remove_job(ResourceAction::Destroy).await?;
        }
        DestructionAction::RemoveResourceFinalizer => {
            r.resource_client()
                .remove_finalizer(FINALIZER_RESOURCE, r.resource())
                .await
                .with_context(|| {
                    format!("Unable to remove resource finalizer from '{}'", r.name())
                })?;
        }
        DestructionAction::RemoveMainFinalizer => {
            r.resource_client()
                .remove_finalizer(FINALIZER_MAIN, r.resource())
                .await
                .with_context(|| format!("Unable to remove main finalizer from '{}'", r.name()))?;
        }
        DestructionAction::Error(error_state) => {
            handle_error_state(&r, ResourceAction::Destroy, error_state).await?
        }
    };
    Ok(())
}

async fn handle_error_state(r: &ResourceInterface, a: ResourceAction, e: ErrorState) -> Result<()> {
    let message = format!(
        "{} error state for resource '{}': {}",
        match a {
            ResourceAction::Create => "Creation",
            ResourceAction::Destroy => "Destruction",
        },
        r.name(),
        match e {
            ErrorState::JobStart => "Timeout before resource started",
            ErrorState::JobExited => "Container exited before it was done",
            ErrorState::JobFailed => "Container exited with an error",
            ErrorState::JobRemoved => "Container was killed before it was done",
            ErrorState::TaskFailed => "Task failed",
            ErrorState::JobTimeout => "Job did not complete within time limit",
            ErrorState::Zombie => {
                warn!("Resource still exists after main finalizer was removed");
                return Ok(());
            }
        }
    );
    error!("{}", message);
    if r.resource().error(a).is_none() {
        let resource_error = ResourceError {
            error: message,
            error_resources: ErrorResources::Unknown,
        };
        r.resource_client()
            .send_error(r.name(), a, &resource_error)
            .await
            .with_context(|| {
                format!(
                    "Unable to send error for job '{}': {}",
                    r.name(),
                    resource_error.error
                )
            })?;
    }
    Ok(())
}

/// `handle_reconciliation_error` is called when `reconcile` returns an error.
pub(crate) fn handle_reconciliation_error(e: &ReconciliationError, _: Context) -> RequeueAction {
    error!("Resource reconciliation error: {}", e);
    requeue()
}
