use crate::action::{determine_action, Action};
use crate::context::{Context, TestInterface};
use crate::error::{self, Error, Result};
use crate::test_pod::{
    check_test_pod, check_test_pod_deletion, create_test_pod, delete_test_pod, get_job,
};
use kube_runtime::controller::ReconcilerAction;
use log::{error, trace};
use model::{Lifecycle, Test};
use snafu::ensure;
use std::time::Duration;

/// Tell the controller to reconcile the object again after some duration.
pub(crate) const REQUEUE: ReconcilerAction = ReconcilerAction {
    requeue_after: Some(Duration::from_secs(5)),
};

/// Requeue the object for immediate follow-up reconciliation.
pub(crate) const REQUEUE_IMMEDIATE: ReconcilerAction = ReconcilerAction {
    requeue_after: Some(Duration::from_millis(10)),
};

/// Do not requeue the object for further reconciliation.
pub(crate) const NO_REQUEUE: ReconcilerAction = ReconcilerAction {
    requeue_after: None,
};

/// `handle_reconciliation_error` is called when `reconcile` returns an error.
pub(crate) fn handle_reconciliation_error(e: &Error, _: Context) -> ReconcilerAction {
    error!("Reconciliation error: {}", e);
    REQUEUE
}

/// `reconcile` is called when a new `Test` object arrives, or when a `Test` object has been
/// re-queued. This is the entrypoint to the controller logic.
pub(crate) async fn reconcile(t: Test, context: Context) -> Result<ReconcilerAction> {
    let mut test = TestInterface::new(t, context)?;
    trace!("Reconciling test: {}", test.name());

    let action = determine_action(&test)?;
    match action {
        Action::Acknowledge => acknowledge_new_test(&mut test).await,
        Action::AddMainFinalizer => add_main_finalizer(&mut test).await,
        Action::CreateTestPod => create_test_pod(&mut test).await,
        Action::CheckTestPod => check_test_pod(&mut test).await,
        Action::DeleteTestPod => delete_test_pod(&mut test).await,
        Action::CheckTestPodDeletion => check_test_pod_deletion(&mut test).await,
        Action::RemovePodFinalizer => remove_pod_finalizer(&mut test).await,
        Action::Delete => delete_test(&mut test).await,
        Action::NoOp => Ok(REQUEUE),
    }
}

async fn acknowledge_new_test(test: &mut TestInterface) -> Result<ReconcilerAction> {
    ensure!(
        !test.has_finalizers(),
        error::NewTestWithFinalizers {
            test_name: test.name()
        }
    );
    let mut status = test.controller_status().into_owned();
    status.lifecycle = Lifecycle::Acknowledged;
    test.set_controller_status(status).await?;
    Ok(REQUEUE_IMMEDIATE)
}

async fn add_main_finalizer(test: &mut TestInterface) -> Result<ReconcilerAction> {
    test.add_main_finalizer().await?;
    Ok(REQUEUE_IMMEDIATE)
}

/// Removes the main finalizer allowing k8s to proceed with deletion of the TestSys `Test` CRD.
///
/// # Precondition
///
/// The only remaining finalizer on the object should be the 'main' finalizer. Anything else is an
/// error.
///
async fn delete_test(test: &mut TestInterface) -> Result<ReconcilerAction> {
    ensure!(
        test.is_safe_to_delete(),
        error::UnsafeDelete {
            test_name: test.name()
        }
    );
    test.remove_main_finalizer().await?;

    // Make sure we got the job done and that the test will be deleted by k8s.
    ensure!(
        !test.has_finalizers(),
        error::DanglingFinalizers {
            test_name: test.name()
        }
    );

    Ok(NO_REQUEUE)
}

/// Removes the pod finalizer.
///
/// # Precondition
///
/// It is assumed that the test pod no longer exists. This is checked in debug builds only.
///
async fn remove_pod_finalizer(test: &mut TestInterface) -> Result<ReconcilerAction> {
    debug_assert!(get_job(test).await?.is_none());
    if !test.has_pod_finalizer() {
        return Ok(REQUEUE);
    }
    test.remove_pod_finalizer().await?;
    Ok(REQUEUE)
}
