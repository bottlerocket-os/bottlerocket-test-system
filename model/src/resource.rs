use core::option::Option;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::map::Map;
use serde_json::value::Value;

/// A request for construction of a resource, as needed by a `Test`.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ResourceRequest {
    /// The name of the `ResourceProvider` CRD instance that can provide the needed resource.
    pub provider_name: String,

    /// Customized information about the resource that is to be created. The `ResourceProvider` may
    /// define a schema to be used here.
    pub configuration: Option<Map<String, Value>>,
}

/// The state that the resource agent is in.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
pub enum ResourceAgentState {
    Unknown,
    Creating,
    Created,
    CreateFailed,
    Destroying,
    Destroyed,
    DestroyFailed,
}

impl Default for ResourceAgentState {
    fn default() -> Self {
        Self::Unknown
    }
}

/// When a resource agent encounters an error, it uses this enum to tell us whether or not resources
/// were left behind.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
pub enum ErrorResources {
    /// The resource agent has left resources behind and has no way of destroying them. The
    /// controller will **not** run `destroy`.  
    Orphaned,

    /// The resource agent has left resources behind and may be able to clean them if `destroy` is
    /// called. The controller **will** run `destroy`.
    Remaining,

    /// The resource agent did not leave any resources behind. The controller will **not** run
    /// `destroy`.    
    Clear,

    /// The resource agent does not know whether or not resources were left behind. The controller
    /// **will** run `destroy`.
    Unknown,
}

/// A status struct to be used by a resource agent..
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ResourceStatus {
    /// The state that the resource agent is in.
    pub agent_state: ResourceAgentState,

    /// Any customized information that the resource agent wants to remember.
    pub agent_info: Option<Map<String, Value>>,

    /// If the resource agent encounters an error, it is written here.
    pub error: Option<String>,

    /// If the resource agent encounters an error, it tells us whether or not resources were left
    /// behind.
    pub error_resources: Option<ErrorResources>,

    /// When the resource agent is done creating the resource, it populates this field with a
    /// description of the resource.
    pub created_resource: Option<Map<String, Value>>,
}
