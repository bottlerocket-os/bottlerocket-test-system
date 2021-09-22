use crate::PathBuf;
use snafu::Snafu;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display("Unable to create client: {}", source))]
    Client { source: kube::Error },

    #[snafu(display("Error creating {}: {}", what, source))]
    Creation { what: String, source: kube::Error },

    #[snafu(display("Error creating test: {}", source))]
    CreateTest { source: model::clients::Error },

    #[snafu(display("Unable to open file '{}': {}", path.display(), source))]
    File {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Could not extract registry url from '{}'", uri))]
    MissingRegistry { uri: String },

    #[snafu(display("Error patching {}: {}", what, source))]
    Patch { what: String, source: kube::Error },

    #[snafu(display("Unable to create client: {}", source))]
    TestClientNew { source: model::clients::Error },

    #[snafu(display("Unable to create Test CRD from '{}': {}", path.display(), source))]
    TestFileParse {
        path: PathBuf,
        source: serde_yaml::Error,
    },
}
