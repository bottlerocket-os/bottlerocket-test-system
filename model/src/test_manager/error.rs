use snafu::Snafu;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

/// The error type for `TestManager`
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(super)))]
pub enum Error {
    #[snafu(display("Unable to {}: {}", action, source))]
    Client {
        action: String,
        source: crate::clients::Error,
    },

    #[snafu(display("Unable to create client: {}", source))]
    ClientCreateKubeconfig {
        source: kube::config::KubeconfigError,
    },

    #[snafu(display("Unable to read kubeconfig: {}", source))]
    ConfigRead {
        source: kube::config::KubeconfigError,
    },

    #[snafu(display("Error Creating {}: {}", what, source))]
    Create { what: String, source: kube::Error },

    #[snafu(display("Unable to send delete event"))]
    DeleteEvent,

    #[snafu(display("Unable to read file '{}': {}", path.display(), source))]
    File {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Unable to {}: {}", action, source))]
    Io {
        action: String,
        source: std::io::Error,
    },

    #[snafu(display("Unable to {}: {}", action, source))]
    Kube { action: String, source: kube::Error },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("Unable to find {}", what))]
    NotFound { what: String },

    #[snafu(display("Some resources are still in the cluster"))]
    ResourceExisting,

    #[snafu(display("Unable to send event: {}", source))]
    Sender {
        source: futures::channel::mpsc::SendError,
    },

    #[snafu(display("Unable to {}: {}", action, source))]
    SerdeYaml {
        action: String,
        source: serde_yaml::Error,
    },
}
