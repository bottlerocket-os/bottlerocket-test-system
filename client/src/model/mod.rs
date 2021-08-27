mod constants;
mod resource_provider;
mod test;

pub use constants::{
    API_VERSION, APP_COMPONENT, APP_CREATED_BY, APP_INSTANCE, APP_MANAGED_BY, APP_NAME,
    APP_PART_OF, CONTROLLER, ENV_TEST_NAME, LABEL_COMPONENT, LABEL_TEST_NAME, LABEL_TEST_UID,
    NAMESPACE, TESTSYS, TEST_AGENT, TEST_AGENT_SERVICE_ACCOUNT,
};
pub use resource_provider::{ResourceProvider, ResourceProviderSpec, ResourceProviderStatus};
pub use test::{
    Agent, AgentStatus, ControllerStatus, Lifecycle, ResourceStatus, RunState, Test, TestResults,
    TestSpec, TestStatus,
};

use serde::de::DeserializeOwned;
use serde::Serialize;
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
}
