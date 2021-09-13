use crate::ConfigurationError;
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
    ConfigSerde { source: ConfigurationError },

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
}

impl From<ConfigurationError> for Error {
    fn from(e: ConfigurationError) -> Self {
        Error(InnerError::ConfigSerde { source: e })
    }
}
