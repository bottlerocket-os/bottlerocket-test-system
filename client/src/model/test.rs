use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A TestSys Test. The `CustomResource` derive also produces a struct named `Test` which represents
/// a test CRD object in the k8s API.
#[derive(Clone, CustomResource, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[kube(
    derive = "Default",
    derive = "PartialEq",
    group = "testsys.bottlerocket.aws",
    kind = "Test",
    namespaced,
    plural = "tests",
    singular = "test",
    status = "TestStatus",
    version = "v1"
)]
pub struct TestSpec {
    /// The URI of the test agent container image.
    pub image: String,
    /// The configuration to pass to the test pod. This is 'open' to allow tests to define their own
    /// schemas.
    pub configuration: Option<Map<String, Value>>,
}

/// The status field of the TestSys Test CRD. This is where the controller and agents will write
/// information about the status of the test run.
// TODO - these fields are strings, define appropriate objects and enums
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct TestStatus {
    /// Information written by the controller.
    pub controller: Option<ControllerStatus>,
    /// Information written by the test agent.
    pub agent: Option<AgentStatus>,
    /// Information written by the resource agents.
    pub resources: Option<HashMap<String, ResourceStatus>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, JsonSchema)]
pub enum RunState {
    Unknown,
    Running,
    Done,
    Error,
}

impl Default for RunState {
    fn default() -> Self {
        RunState::Unknown
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct TestResults {
    // TODO - create this schema
    pub whatever: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct AgentStatus {
    pub run_state: RunState,
    /// Due to structural OpenAPI constraints, the error message must be provided separately instead
    /// of as a value within the `RunState::Error` variant. If the `run_state` is `Error` then there
    /// *may* be an error message here. If there is an error message here and the `run_state` is
    /// *not* `Error`, the this is a bad state and the `error_message` should be ignored.
    pub error_message: Option<String>,
    pub results: Option<TestResults>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ControllerStatus {
    // TODO - create this schema
    pub whatever: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ResourceStatus {
    // TODO - create this schema
    pub whatever: String,
}
