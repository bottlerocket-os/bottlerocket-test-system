use crate::context::TestInterface;
use crate::error::Result;
use client::model::Lifecycle;

/// The action that the controller needs to take in order to reconcile the `Test`.
pub(crate) enum Action {
    /// A new `Test`, not yet seen, will be acknowledged by setting its lifecycle value.
    Acknowledge,
    /// A test that has been acknowledged needs to have its main finalizer added.
    AddMainFinalizer,
    /// An acknowledged test that has its main finalizer added can proceed to start its test pod.
    // TODO - resources https://github.com/bottlerocket-os/bottlerocket-test-system/issues/19
    CreateTestPod,
    /// If a `Test` pod has been started, we need to check its status.
    CheckTestPod,
    /// When a `Test` has been marked for deletion, we need to clean up and remove finalizers.
    Delete,
    /// There is nothing to do.
    NoOp,
}

/// Inspect the `test` to determine which `Action` the controller should take.
pub(crate) fn determine_action(test: &TestInterface) -> Result<Action> {
    if test.is_delete_requested() {
        return Ok(Action::Delete);
    }
    let lifecycle = test.controller_status().lifecycle;
    match lifecycle {
        Lifecycle::New => Ok(Action::Acknowledge),
        Lifecycle::Acknowledged => {
            if test.has_finalizers() {
                Ok(Action::CreateTestPod)
            } else {
                Ok(Action::AddMainFinalizer)
            }
        }
        Lifecycle::TestPodCreated | Lifecycle::TestPodHealthy => Ok(Action::CheckTestPod),
        // TODO - these are placeholders. see issues #14, #44, #47, etc
        Lifecycle::TestPodDone => Ok(Action::NoOp),
        Lifecycle::TestPodExited => Ok(Action::NoOp),
    }
}
