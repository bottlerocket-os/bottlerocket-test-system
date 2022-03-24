use crate::constants::FINALIZER_MAIN;
use crate::crd_ext::CrdExt;
use crate::{Agent, TaskState};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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
    version = "v1",
    printcolumn = r#"{"name":"State", "type":"string", "jsonPath":".status.agent.taskState"}"#,
    printcolumn = r#"{"name":"Result", "type":"string", "jsonPath":".status.agent.results.outcome"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct TestSpec {
    /// The list of resources required by this test. The test controller will wait for these
    /// resources to become ready before running the test agent.
    pub resources: Vec<String>,
    /// Other tests that must be completed before this one can be run.
    pub depends_on: Option<Vec<String>>,
    /// Information about the test agent.
    pub agent: Agent,
    /// The number of retries the agent is allowed to perform after a failed test.
    pub retries: Option<u32>,
}

/// The status field of the TestSys Test CRD. This is where the controller and agents will write
/// information about the status of the test run.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestStatus {
    /// Information written by the controller.
    pub controller: ControllerStatus,
    /// Information written by the test agent.
    pub agent: AgentStatus,
}

/// The `Outcome` of a test run, reported by the test agent.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Copy, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Pass,
    Fail,
    Timeout,
    Unknown,
}

impl Default for Outcome {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestResults {
    pub outcome: Outcome,
    pub num_passed: u64,
    pub num_failed: u64,
    pub num_skipped: u64,
    pub other_info: Option<String>,
}

impl TestResults {
    /// The sum of all tests counted, whether passed, failed or skipped.
    pub fn total(&self) -> u64 {
        self.num_passed + self.num_failed + self.num_skipped
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub task_state: TaskState,
    /// Due to structural OpenAPI constraints, the error message must be provided separately instead
    /// of as a value within the `RunState::Error` variant. If the `run_state` is `Error` then there
    /// *may* be an error message here. If there is an error message here and the `run_state` is
    /// *not* `Error`, the this is a bad state and the `error_message` should be ignored.
    pub error: Option<String>,
    pub results: Vec<TestResults>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ControllerStatus {
    pub resource_error: Option<String>,
}

/// A simplified summary of the test's current state. This can be used by a user interface to
/// describe what is happening with the test. This is not included in the model, but is derived
/// from the state of the `Test` CRD. Note that resource state cannot be represented here
/// because the `Resource` CRDs would need to be queried.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum TestUserState {
    /// The test state cannot be determined.
    Unknown,
    /// The test has not yet started its test agent, it might be waiting for resources.
    Starting,
    /// The test agent container is running.
    Running,
    /// The test agent container finished successfully but reported no tests.
    NoTests,
    /// The test agent reported no failing tests.
    Passed,
    /// The test agent reported one or more test failures.
    Failed,
    /// The test agent reported an error.
    Error,
    /// Resource creation failed and the test will not be started.
    ResourceError,
    /// The test is in the process of being deleted.
    Deleting,
}

impl Default for TestUserState {
    fn default() -> Self {
        Self::Unknown
    }
}

serde_plain::derive_display_from_serialize!(TestUserState);

impl Test {
    pub fn agent_status(&self) -> Cow<'_, AgentStatus> {
        match self.status.as_ref() {
            None => Cow::Owned(AgentStatus::default()),
            Some(status) => Cow::Borrowed(&status.agent),
        }
    }

    pub fn agent_error(&self) -> Option<&str> {
        self.status
            .as_ref()
            .map(|test_status| &test_status.agent)
            .and_then(|agent_status| agent_status.error.as_deref())
    }

    pub fn resource_error(&self) -> Option<&String> {
        self.status
            .as_ref()
            .map(|some| &some.controller)
            .and_then(|some| some.resource_error.as_ref())
    }

    pub fn test_user_state(&self) -> TestUserState {
        let agent_status = self.agent_status();
        if self.is_delete_requested() && !matches!(agent_status.task_state, TaskState::Unknown) {
            return TestUserState::Deleting;
        }
        if self.resource_error().is_some() {
            return TestUserState::ResourceError;
        }
        match agent_status.task_state {
            TaskState::Unknown => {
                if self.has_finalizer(FINALIZER_MAIN) {
                    TestUserState::Starting
                } else {
                    TestUserState::Unknown
                }
            }
            TaskState::Running => TestUserState::Running,
            TaskState::Completed => {
                if let Some(results) = agent_status.results.last() {
                    match results.outcome {
                        Outcome::Pass => TestUserState::Passed,
                        Outcome::Fail => TestUserState::Failed,
                        Outcome::Timeout => TestUserState::Failed,
                        Outcome::Unknown => {
                            if results.total() == 0 {
                                TestUserState::NoTests
                            } else if results.num_failed == 0 {
                                TestUserState::Passed
                            } else {
                                TestUserState::Failed
                            }
                        }
                    }
                } else {
                    TestUserState::NoTests
                }
            }
            TaskState::Error => TestUserState::Error,
        }
    }
}

impl CrdExt for Test {
    fn object_meta(&self) -> &ObjectMeta {
        &self.metadata
    }
}
