use crate::{ResourceRequest, ResourceStatus};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

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
    /// Information about resources this test needs.
    pub resources: BTreeMap<String, ResourceRequest>,
    /// Information about the test agent.
    pub agent: Agent,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct Agent {
    /// The name of the test agent.
    pub name: String,
    /// The URI of the test agent container image.
    pub image: String,
    /// The name of an image registry pull secret if one is needed to pull the test agent image.
    pub pull_secret: Option<String>,
    /// The configuration to pass to the test pod. This is 'open' to allow tests to define their own
    /// schemas.
    pub configuration: Option<Map<String, Value>>,
}

/// The status field of the TestSys Test CRD. This is where the controller and agents will write
/// information about the status of the test run.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct TestStatus {
    /// Information written by the controller.
    pub controller: Option<ControllerStatus>,
    /// Information written by the test agent.
    pub agent: Option<AgentStatus>,
    /// Information written by the resource agents.
    pub resources: Option<BTreeMap<String, ResourceStatus>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
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
    /// What phase of the TestSys `Test` lifecycle are we in.
    pub lifecycle: Lifecycle,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
pub enum Lifecycle {
    /// A newly created test that has not yet been seen by the controller.
    New,
    /// The test has been seen by the controller.
    Acknowledged,
    /// The controller has created the test pod.
    TestPodCreated,
    /// The controller is waiting for the test pod to be in the running state.
    TestPodStarting,
    /// The test pod is running.
    TestPodHealthy,
    /// The test pod is done with its test and is still running.
    TestPodDone,
    /// The test pod encountered an error that prevents tests from completing successfully.
    TestPodError,
    /// The test pod failed or exited with a non-zero exit code. It is not running.
    TestPodFailed,
    /// The test pod completed successfully. It is no longer running.
    TestPodExited,
    /// The test pod is being deleted.
    TestPodDeleting,
    /// The test pod has been deleted.
    TestPodDeleted,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Lifecycle::New
    }
}
