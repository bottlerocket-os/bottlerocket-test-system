use crate::clients::{HttpStatusCode, StatusCode};
use crate::Error as ModelError;
use snafu::Snafu;

/// The `Result` type returned by `clients`.
pub type Result<T> = std::result::Result<T, Error>;

/// The public error type returned by `clients`.
#[derive(Debug, Snafu)]
pub struct Error(InnerError);

/// The private error type returned by `clients`.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(super)")]
pub(crate) enum InnerError {
    #[snafu(display("{}", source))]
    ConfigSerde { source: ModelError },

    #[snafu(display("An error occured while resolving the config: {}", what))]
    ConfigResolution { what: String },

    #[snafu(display("Error serializing object '{}': {}", what, source))]
    Serde {
        what: String,
        source: serde_json::Error,
    },

    #[snafu(display("Error initializing the Kubernetes client: {}", source))]
    Initialization { source: kube::Error },

    #[snafu(display("Unable to {} {}: {}", method, what, source))]
    KubeApiCall {
        method: String,
        what: String,
        source: kube::Error,
    },

    #[snafu(display("Unable to {} for '{}': {}", operation, name, source))]
    KubeApiCallFor {
        /// What we were trying to do, e.g. 'initialize status field'.
        operation: String,
        /// The name of the k8s object we were trying to do this for, e.g. 'my-test'.
        name: String,
        /// The error from kube-rs.
        source: kube::Error,
    },

    #[snafu(display(
        "An attempt was made to add the finalizer '{}' more than once",
        finalizer,
    ))]
    DuplicateFinalizer { finalizer: String },

    #[snafu(display(
        "An attempt was made to delete the non-existant finalizer '{}'",
        finalizer,
    ))]
    DeleteMissingFinalizer { finalizer: String },
}

impl From<ModelError> for Error {
    fn from(e: ModelError) -> Self {
        Error(InnerError::ConfigSerde { source: e })
    }
}

impl HttpStatusCode for InnerError {
    fn status_code(&self) -> Option<StatusCode> {
        match self {
            InnerError::ConfigSerde { .. }
            | InnerError::ConfigResolution { .. }
            | InnerError::Serde { .. }
            | InnerError::Initialization { .. } => None,
            InnerError::KubeApiCall {
                method: _,
                source: e,
                what: _,
            } => e.status_code(),
            InnerError::KubeApiCallFor {
                operation: _,
                name: _,
                source: e,
            } => e.status_code(),
            InnerError::DuplicateFinalizer { .. } | InnerError::DeleteMissingFinalizer { .. } => {
                None
            }
        }
    }
}

impl HttpStatusCode for Error {
    fn status_code(&self) -> Option<StatusCode> {
        self.0.status_code()
    }
}
