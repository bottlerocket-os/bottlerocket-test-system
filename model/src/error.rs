use snafu::Snafu;

#[derive(Debug, Snafu)]
pub struct Error(OpaqueError);
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum OpaqueError {
    #[snafu(display("Error deserializing configuration: {}", source))]
    ConfigDeserialization { source: serde_json::Error },

    #[snafu(display("Error serializing configuration: {}", source))]
    ConfigSerialization { source: serde_json::Error },

    #[snafu(display(
        "Error serializing configuration: expected Value::Object type but got something else."
    ))]
    ConfigWrongValueType {},

    #[snafu(display("Parse error: {}", source))]
    SerdePlain { source: serde_plain::Error },
}
