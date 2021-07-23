mod resource_provider;
mod test;

pub use resource_provider::{ResourceProvider, ResourceProviderSpec, ResourceProviderStatus};
pub use test::{
    AgentStatus, ControllerStatus, Lifecycle, ResourceStatus, RunState, Test, TestResults,
    TestSpec, TestStatus,
};

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

pub const TESTSYS: &str = "testsys.bottlerocket.aws";
pub const TESTSYS_API: &str = "testsys.bottlerocket.aws/v1";
pub const TESTSYS_NAMESPACE: &str = "testsys-bottlerocket-aws";

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
