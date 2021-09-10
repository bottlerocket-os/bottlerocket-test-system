mod constants;
mod resource;
mod resource_provider;
mod test;

// TODO - pub mod constants https://github.com/bottlerocket-os/bottlerocket-test-system/issues/91
pub use constants::{
    API_VERSION, APP_COMPONENT, APP_CREATED_BY, APP_INSTANCE, APP_MANAGED_BY, APP_NAME,
    APP_PART_OF, CONTROLLER, ENV_PROVIDER_NAME, ENV_RESOURCE_ACTION, ENV_RESOURCE_NAME,
    ENV_RESOURCE_PROVIDER_NAME, ENV_TEST_NAME, LABEL_COMPONENT, LABEL_TEST_NAME, LABEL_TEST_UID,
    NAMESPACE, TESTSYS, TEST_AGENT, TEST_AGENT_SERVICE_ACCOUNT,
};
pub use resource::{ErrorResources, ResourceAgentState, ResourceRequest, ResourceStatus};
pub use resource_provider::{ResourceProvider, ResourceProviderSpec, ResourceProviderStatus};
pub use test::{
    Agent, AgentStatus, ControllerStatus, Lifecycle, RunState, Test, TestResults, TestSpec,
    TestStatus,
};

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
