use kube_runtime::controller::ReconcilerAction;
use std::time::Duration;

/// Tell the controller to reconcile the object again after some duration.
pub(crate) const REQUEUE: ReconcilerAction = ReconcilerAction {
    requeue_after: Some(Duration::from_secs(5)),
};

/// Requeue just in case, but we don't expect anything to happen.
pub(crate) const REQUEUE_SLOW: ReconcilerAction = ReconcilerAction {
    requeue_after: Some(Duration::from_secs(30)),
};

/// Do not requeue the object for further reconciliation.
pub(crate) const NO_REQUEUE: ReconcilerAction = ReconcilerAction {
    requeue_after: None,
};
