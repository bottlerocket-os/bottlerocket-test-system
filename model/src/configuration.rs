use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use snafu::{ResultExt, Snafu};
use std::fmt::Debug;

/// The `Configuration` trait is for structs that can be used for custom data, which is represented
/// in a CRD model like this:
///
/// ```yaml
/// configuration:
///   additionalProperties: true
///   nullable: true
///   type: object
/// ```
///
/// The traits aggregated by the `Configuration` trait are typical of "plain old data" types and
/// provide a way for clients to strongly type this data which is otherwise unconstrained by the
/// API.
///
pub trait Configuration:
    Serialize + DeserializeOwned + Clone + Debug + Default + Send + Sync + Sized + 'static
{
    /// Convert the `Configuration` object to a serde `Map`.
    fn into_map(self) -> Result<Map<String, Value>, ConfigurationError> {
        match self.into_value()? {
            Value::Object(map) => Ok(map),
            _ => Err(WrongValueType {}.build().into()),
        }
    }

    /// Convert the `Configuration` object to a serde `Value`.
    fn into_value(self) -> std::result::Result<Value, ConfigurationError> {
        Ok(serde_json::to_value(self).context(Serialization)?)
    }

    /// Deserialize the `Configuration` object from a serde `Map`.
    fn from_map(map: Map<String, Value>) -> std::result::Result<Self, ConfigurationError> {
        Self::from_value(Value::Object(map))
    }

    /// Deserialize the `Configuration` object from a serde `Value`.
    fn from_value(value: Value) -> std::result::Result<Self, ConfigurationError> {
        Ok(serde_json::from_value(value).context(Deserialization)?)
    }
}

/// The error type that can occur when serializing or deserializing a `Configuration` object.
#[derive(Debug, Snafu)]
pub struct ConfigurationError(InnerError);

/// The hidden error type that produces `ConfigurationError` context messages.
#[derive(Debug, Snafu)]
enum InnerError {
    #[snafu(display("Error deserializing configuration: {}", source))]
    Deserialization { source: serde_json::Error },

    #[snafu(display("Error serializing configuration: {}", source))]
    Serialization { source: serde_json::Error },

    #[snafu(display(
        "Error serializing configuration: expected Value::Object type but got something else."
    ))]
    WrongValueType {},
}
