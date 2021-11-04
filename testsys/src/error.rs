use crate::PathBuf;
use snafu::Snafu;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum Error {
    #[snafu(display(
        "Unable to parse argument '{}' as key value pair, expected key=value syntax",
        arg
    ))]
    ArgumentMissing { arg: String },

    #[snafu(display("Unable to create client: {}", source))]
    ClientCreate { source: kube::Error },

    #[snafu(display("Unable to read kubeconfig: {}", source))]
    ConfigRead { source: kube::Error },

    #[snafu(display("Error Creating {}: {}", what, source))]
    Creation { what: String, source: kube::Error },

    #[snafu(display("Error creating test: {}", source))]
    CreateTest { source: model::clients::Error },

    #[snafu(display("The following tests failed to run '{:?}'", tests))]
    FailedTest { tests: Vec<String> },

    #[snafu(display("Unable to open file '{}': {}", path.display(), source))]
    File {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Unable to get test: {}", source))]
    GetTest { source: model::clients::Error },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Could not create map: {}", source))]
    ConfigMap { source: model::Error },

    #[snafu(display("Could not extract registry url from '{}'", uri))]
    MissingRegistry { uri: String },

    #[snafu(display("Error patching {}: {}", what, source))]
    Patch { what: String, source: kube::Error },

    #[snafu(display("Unable to create ResourceProvider CRD from '{}': {}", path.display(), source))]
    ResourceProviderFileParse {
        path: PathBuf,
        source: serde_yaml::Error,
    },

    #[snafu(display("Unable to set {} field of '{}': {}", what, name, source))]
    Set {
        name: String,
        what: String,
        source: model::clients::Error,
    },

    #[snafu(display("Unable to create client: {}", source))]
    TestClientNew { source: model::clients::Error },

    #[snafu(display("Unable to create Test CRD from '{}': {}", path.display(), source))]
    TestFileParse {
        path: PathBuf,
        source: serde_yaml::Error,
    },
}
