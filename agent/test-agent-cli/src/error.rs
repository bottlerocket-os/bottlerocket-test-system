use snafu::Snafu;

/// The crate-wide result type.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The crate-wide error type.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum Error {
    #[snafu(display("Unable to parse argument '{}'", arg))]
    ArgumentMissing { arg: String },

    #[snafu(display("The arguments given were invalid: {}", why))]
    InvalidArguments { why: String },

    #[snafu(display("Unable to read environment variable: '{}': {}", key, source))]
    EnvRead {
        key: String,
        source: std::env::VarError,
    },

    #[snafu(display("Unable to resolve config templates: {}", source))]
    ResolveConfig { source: model::clients::Error },

    #[snafu(display("Unable to deserialize test configuration: {}", source))]
    Deserialization { source: serde_json::Error },

    #[snafu(display("{}", source))]
    K8s { source: model::clients::Error },

    #[snafu(display("An error occured while creating a `TempDir`: {}", source))]
    TempDirCreate { source: std::io::Error },

    #[snafu(display("Unable to create resource client: {}", source))]
    ResourceClientCreate { source: model::clients::Error },

    #[snafu(display("Could not serialize object: {}", source))]
    JsonSerialize { source: serde_json::Error },

    #[snafu(display("An error occured while creating archive: {}", source))]
    Archive { source: std::io::Error },

    #[snafu(display(
        "An error occurred while trying to find the value for given key: {}",
        key
    ))]
    SecretKeyFetch { key: String },
}
