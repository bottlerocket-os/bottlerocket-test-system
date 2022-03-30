use kube_runtime::controller::Action;
use std::time::Duration;

/// Tell the controller to reconcile the object again after some duration.
pub(crate) fn requeue() -> Action {
    Action::requeue(Duration::from_secs(5))
}

/// Requeue just in case, but we don't expect anything to happen.
pub(crate) fn requeue_slow() -> Action {
    Action::requeue(Duration::from_secs(30))
}

/// Do not requeue the object.
pub(crate) fn no_requeue() -> Action {
    Action::await_change()
}
