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
        source: client::Error,
    },

    #[snafu(display(
        "Unable to remove finalizer '{}' for test '{}': {}",
        finalizer,
        test_name,
        source
    ))]
    RemoveFinalizer {
        test_name: String,
        finalizer: String,
        source: client::Error,
    },

    #[snafu(display(
        "Unable to remove all finalizers for test '{}', zombie cannot be deleted.",
        test_name
    ))]
    DanglingFinalizers { test_name: String },

    #[snafu(display(
        "Test '{}' is in a bad state, it should not be created with finalizers.",
        test_name
    ))]
    NewTestWithFinalizers { test_name: String },

    #[snafu(display("Unable to set controller status for test '{}': {}", test_name, source))]
    SetControllerStatus {
        test_name: String,
        source: client::Error,
    },
}
