use snafu::Snafu;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display(
        "Unable to add finalizer '{}' for test '{}': {}",
        finalizer,
        test_name,
        source
    ))]
    AddFinalizer {
        test_name: String,
        finalizer: String,
        source: model::Error,
    },

    #[snafu(display(
        "Unable to remove all finalizers for test '{}', zombie cannot be deleted.",
        test_name
    ))]
    DanglingFinalizers { test_name: String },

    #[snafu(display(
        "Kubernetes client error trying to {} for test '{}': {}",
        action,
        test_name,
        source
    ))]
    KubeClient {
        test_name: String,
        action: String,
        source: kube::Error,
    },

    #[snafu(display(
        "Test '{}' is in a bad state, it should not be created with finalizers.",
        test_name
    ))]
    NewTestWithFinalizers { test_name: String },

    #[snafu(display(
        "Unable to remove finalizer '{}' for test '{}': {}",
        finalizer,
        test_name,
        source
    ))]
    RemoveFinalizer {
        test_name: String,
        finalizer: String,
        source: model::Error,
    },

    #[snafu(display("Unable to set controller status for test '{}': {}", test_name, source))]
    SetControllerStatus {
        test_name: String,
        source: model::Error,
    },

    #[snafu(display(
        "There should be only one k8s job but found {} running, {} succeeded, and {} failed for test '{}'",
        running,
        succeeded,
        failed,
        test_name
    ))]
    TooManyJobContainers {
        test_name: String,
        running: i32,
        succeeded: i32,
        failed: i32,
    },

    #[snafu(display(
        "The controller tried to delete test '{}' before cleaning up finalizers.",
        test_name
    ))]
    UnsafeDelete { test_name: String },
}
