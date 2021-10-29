use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The states that an agent declares about its task (e.g. running tests or creating/destroying
/// resources).
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
pub enum TaskState {
    Unknown,
    Running,
    Completed,
    Error,
}

impl Default for TaskState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct Agent {
    /// The name of the agent.
    pub name: String,
    /// The URI of the agent container image.
    pub image: String,
    /// The name of an image registry pull secret if one is needed to pull the agent image.
    pub pull_secret: Option<String>,
    /// Determine if the pod should keep running after it has finished or encountered and error.
    pub keep_running: bool,
    /// The configuration to pass to the agent. This is 'open' to allow agents to define their own
    /// schemas.
    pub configuration: Option<Map<String, Value>>,
}
