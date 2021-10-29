use crate::error::{self, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};
use snafu::ResultExt;
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
    fn into_map(self) -> Result<Map<String, Value>> {
        match self.into_value()? {
            Value::Object(map) => Ok(map),
            _ => Err(error::ConfigWrongValueType {}.build().into()),
        }
    }

    /// Convert the `Configuration` object to a serde `Value`.
    fn into_value(self) -> Result<Value> {
        Ok(serde_json::to_value(self).context(error::ConfigSerialization)?)
    }

    /// Deserialize the `Configuration` object from a serde `Map`.
    fn from_map(map: Map<String, Value>) -> Result<Self> {
        Self::from_value(Value::Object(map))
    }

    /// Deserialize the `Configuration` object from a serde `Value`.
    fn from_value(value: Value) -> Result<Self> {
        Ok(serde_json::from_value(value).context(error::ConfigDeserialization)?)
    }
}
