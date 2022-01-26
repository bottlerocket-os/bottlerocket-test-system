use crate::PathBuf;
use snafu::Snafu;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum Error {
    #[snafu(display(
        "Unable to parse argument '{}' as key value pair, expected key=value syntax",
        arg
    ))]
    ArgumentMissing { arg: String },

    #[snafu(display("Unable to create client: {}", source))]
    ClientCreate { source: kube::Error },

    #[snafu(display("Unable to create client: {}", source))]
    ClientCreateKubeconfig {
        source: kube::config::KubeconfigError,
    },

    #[snafu(display("Unable to read kubeconfig: {}", source))]
    ConfigRead {
        source: kube::config::KubeconfigError,
    },

    #[snafu(display("Error Creating {}: {}", what, source))]
    Creation { what: String, source: kube::Error },

    #[snafu(display("Error creating test: {}", source))]
    CreateTest { source: model::clients::Error },

    #[snafu(display("Error creating resource: {}", source))]
    CreateResource { source: model::clients::Error },

    #[snafu(display("Unable to delete '{}': {}", what, source))]
    Delete {
        what: String,
        source: model::clients::Error,
    },

    #[snafu(display("Error deleting {} '{}': {}", what, name, source))]
    DeleteObject {
        what: String,
        name: String,
        source: kube::Error,
    },

    #[snafu(display("The following tests failed to run '{:?}'", tests))]
    FailedTest { tests: Vec<String> },

    #[snafu(display("Unable to open file '{}': {}", path.display(), source))]
    File {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Unable to get pod for '{}': {}", test_name, source))]
    GetPod {
        test_name: String,
        source: kube::Error,
    },

    #[snafu(display("Unable to get logs for '{}': {}", pod, source))]
    Logs { pod: String, source: kube::Error },

    #[snafu(display("Unable to get next log from stream: {}", source))]
    LogsStream { source: kube::Error },

    #[snafu(display("Unable to get '{}': {}", what, source))]
    Get {
        what: String,
        source: model::clients::Error,
    },

    #[snafu(display("The arguments given were invalid: {}", why))]
    InvalidArguments { why: String },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Could not create map: {}", source))]
    ConfigMap { source: model::Error },

    #[snafu(display("Could not extract registry url from '{}'", uri))]
    MissingRegistry { uri: String },

    #[snafu(display("{}: {}", message, source))]
    ModelClient {
        message: String,
        source: model::clients::Error,
    },

    #[snafu(display("No stdout from request"))]
    NoOut,

    #[snafu(display("Error patching {}: {}", what, source))]
    Patch { what: String, source: kube::Error },

    #[snafu(display("Error getting data from reader: {}", source))]
    Read { source: std::io::Error },

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

    #[snafu(display("Could not retrieve test '{}'", test_name))]
    TestMissing { test_name: String },

    #[snafu(display("Unable to write data: {}", source))]
    Write { source: std::io::Error },
}
