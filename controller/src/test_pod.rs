use crate::context::TestInterface;
use crate::error::Result;
use crate::reconcile::REQUEUE;
use client::model::Lifecycle;
use kube_runtime::controller::ReconcilerAction;
use log::{debug, trace};

pub(crate) async fn create_test_pod(test: &mut TestInterface) -> Result<ReconcilerAction> {
    debug!("Creating test pod for '{}'", test.name());
    // TODO - create pod https://github.com/bottlerocket-os/bottlerocket-test-system/issues/14
    test.add_pod_finalizer().await?;
    let mut status = test.controller_status().into_owned();
    status.lifecycle = Lifecycle::TestPodCreated;
    test.set_controller_status(status).await?;
    Ok(REQUEUE)
}

pub(crate) async fn delete_test_pod(test: &mut TestInterface) -> Result<()> {
    if !test.has_pod_finalizer() {
        return Ok(());
    }
    debug!("Deleting test pod for '{}'", test.name());
    // TODO - delete pod https://github.com/bottlerocket-os/bottlerocket-test-system/issues/14
    test.remove_pod_finalizer().await?;
    Ok(())
}

pub(crate) async fn check_test_pod(test: &mut TestInterface) -> Result<ReconcilerAction> {
    trace!("Checking test pod for '{}'", test.name());
    let status = test.controller_status();
    if matches!(status.lifecycle, Lifecycle::TestPodCreated) {
        // TODO - actually check the status of the test pod
        let mut status = status.into_owned();
        status.lifecycle = Lifecycle::TestPodHealthy;
        test.set_controller_status(status).await?;
    }
    Ok(REQUEUE)
}
