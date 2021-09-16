use snafu::Snafu;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Unable to check {}: {}", what, source))]
    Check { what: String, source: kube::Error },

    #[snafu(display("Unable to create client: {}", source))]
    Client { source: kube::Error },

    #[snafu(display("Error Creating {}: {}", what, source))]
    Creation { what: String, source: kube::Error },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Could not extract registry url from '{}'", uri))]
    MissingRegistry { uri: String },

    #[snafu(display("Error patching {}: {}", what, source))]
    Patch { what: String, source: kube::Error },
}
