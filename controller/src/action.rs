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
    /// The test pod needs to be deleted.
    DeleteTestPod,
    /// The test pod is in the process of deletion, check if it has finished deleting.
    CheckTestPodDeletion,
    /// The test pod has been deleted, remove the pod finalizer.
    RemovePodFinalizer,
    /// When a `Test` has been marked for deletion, we need to clean up and remove finalizers.
    Delete,
    /// There is nothing to do.
    NoOp,
}

/// Inspect the `test` to determine which `Action` the controller should take.
pub(crate) fn determine_action(test: &TestInterface) -> Result<Action> {
    if test.is_delete_requested() {
        return Ok(determine_delete_action(test));
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
        Lifecycle::TestPodCreated | Lifecycle::TestPodStarting | Lifecycle::TestPodHealthy => {
            Ok(Action::CheckTestPod)
        }
        Lifecycle::TestPodDeleting => Ok(Action::CheckTestPodDeletion),
        Lifecycle::TestPodDeleted => Ok(Action::RemovePodFinalizer),
        // TODO - these are placeholders. see issues #14, #44, #47, etc
        Lifecycle::TestPodDone
        | Lifecycle::TestPodExited
        | Lifecycle::TestPodFailed
        | Lifecycle::TestPodError => Ok(Action::NoOp),
    }
}

/// Determines what we should do next if the TestSys `Test` CRD has been marked for deletion.
/// If we were already deleting the test pod then we need to wait for that complete. Otherwise,
/// we need to initiate the deletion of the test pod.
///
/// # Preconditions
///
/// This function assumes that the test has been marked for deletion. This is checked in debug
/// builds but not in release builds.
///
pub(crate) fn determine_delete_action(test: &TestInterface) -> Action {
    debug_assert!(test.is_delete_requested());
    if test.has_pod_finalizer() {
        match &test.controller_status().lifecycle {
            // If we are already deleting the test pod, then continue through that deletion process.
            Lifecycle::TestPodDeleting => return Action::CheckTestPodDeletion,
            Lifecycle::TestPodDeleted => return Action::RemovePodFinalizer,
            // If we have a pod finalizer and deletion is not already underway, we need to delete
            // the test pod.
            _ => return Action::DeleteTestPod,
        };
    }
    // There is no pod finalizer so we are ready to delete the TestSys `Test` CRD.
    Action::Delete
}
