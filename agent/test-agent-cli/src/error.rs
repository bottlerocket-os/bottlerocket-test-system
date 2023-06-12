use snafu::Snafu;
use std::string::FromUtf8Error;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum Error {
    #[snafu(display("An error occurred while creating archive: {}", source))]
    Archive { source: std::io::Error },

    #[snafu(display("Unable to communicate with Kubernetes: {}", source))]
    Client { source: test_agent::ClientError },

    #[snafu(display("Configuration missing from the test secret data"))]
    ConfMissing,

    #[snafu(display("Could not convert {} secret to string: {}", what, source))]
    Conversion { what: String, source: FromUtf8Error },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Unable to get secret name for key '{}'", key))]
    SecretKeyFetch { key: String },

    #[snafu(display("Unable to get secret '{}'", source))]
    SecretMissing {
        source: agent_common::secrets::Error,
    },
}
