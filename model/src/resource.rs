use crate::{agent::config_schema, Agent, CrdExt, TaskState};
use core::option::Option;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{CustomResource, Resource as Kresource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_plain::{derive_display_from_serialize, derive_fromstr_from_deserialize};
use std::fmt::{Display, Formatter};

/// A resource required by a test. For example, a compute instance or cluster. The `CustomResource`
/// derive also produces a struct named `Resource` which represents a resource CRD object in the k8s
/// API.
#[derive(Clone, CustomResource, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[kube(
    derive = "Default",
    derive = "PartialEq",
    group = "testsys.bottlerocket.aws",
    kind = "Resource",
    namespaced,
    plural = "resources",
    singular = "resource",
    status = "ResourceStatus",
    version = "v1",
    printcolumn = r#"{"name":"DestructionPolicy", "type":"string", "jsonPath":".spec.destructionPolicy"}"#,
    printcolumn = r#"{"name":"CreationState", "type":"string", "jsonPath":".status.creation.taskState"}"#,
    printcolumn = r#"{"name":"DestructionState", "type":"string", "jsonPath":".status.destruction.taskState"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpec {
    /// Other resources that must to be created before this one can be created.
    pub depends_on: Option<Vec<String>>,
    /// Creation of this resource will not begin until all conflicting resources have been deleted.
    pub conflicts_with: Option<Vec<String>>,
    /// Information about the resource agent.
    pub agent: Agent,
    /// Whether/when the resource controller will destroy the resource (`OnDeletion` is the
    /// default).
    #[serde(deserialize_with = "crate::schema_utils::null_to_default")]
    #[serde(default)]
    #[schemars(schema_with = "crate::schema_utils::nullable_enum::<DestructionPolicy>")]
    pub destruction_policy: DestructionPolicy,
}

impl Resource {
    /// Gets the information for the resource created.
    pub fn created_resource(&self) -> Option<&Map<String, Value>> {
        self.status
            .as_ref()
            .and_then(|s| s.created_resource.as_ref())
    }

    /// Gets the error that occurred during resource creation (if any).
    pub fn creation_error(&self) -> Option<&ResourceError> {
        self.status.as_ref().and_then(|s| s.creation.error.as_ref())
    }

    /// Gets the current state of the creation task. Defaults to `Unknown` if not present.
    pub fn creation_task_state(&self) -> TaskState {
        self.status
            .as_ref()
            .map(|s| s.creation.task_state)
            .unwrap_or_default()
    }

    /// Gets the error that occurred during resource destruction (if any).
    pub fn destruction_error(&self) -> Option<&ResourceError> {
        self.status
            .as_ref()
            .and_then(|s| s.destruction.error.as_ref())
    }

    /// Gets the current state of the destruction task. Defaults to `Unknown` if not present.
    pub fn destruction_task_state(&self) -> TaskState {
        self.status
            .as_ref()
            .map(|s| s.destruction.task_state)
            .unwrap_or_default()
    }

    /// Gets either the creation error (if there is one) or the destruction error (if there is one)
    /// depending on the given `resource_action`.
    pub fn error(&self, resource_action: ResourceAction) -> Option<&ResourceError> {
        match resource_action {
            ResourceAction::Create => self.creation_error(),
            ResourceAction::Destroy => self.destruction_error(),
        }
    }

    /// Gets either the current creation task state or the destruction task state based on
    /// `resource_action`. `Unknown` is returned if the desired `resource_action` task state does
    /// not exist.
    pub fn task_state(&self, resource_action: ResourceAction) -> TaskState {
        match resource_action {
            ResourceAction::Create => self.creation_task_state(),
            ResourceAction::Destroy => self.destruction_task_state(),
        }
    }
}

/// The action taken by a resource agent, which can create and destroy resources. This is not used
/// in the CRD model, but is populated in an environment variable for the resource agent and is
/// useful for function parameters, etc.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum ResourceAction {
    Create,
    Destroy,
}

serde_plain::derive_fromstr_from_deserialize!(ResourceAction, |e| -> crate::Error {
    crate::error::OpaqueError::SerdePlain { source: e }.into()
});
serde_plain::derive_display_from_serialize!(ResourceAction);

/// When a resource agent encounters an error, it uses this enum to tell us whether or not resources
/// were left behind.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ErrorResources {
    /// The resource agent has left resources behind and has no way of destroying them. The
    /// controller will **not** run `destroy`.  
    Orphaned,

    /// The resource agent has left resources behind and may be able to clean them if `destroy` is
    /// called. The controller **will** run `destroy` if the creation task declares that resources
    /// are `Remaining`.
    Remaining,

    /// The resource agent did not leave any resources behind. The controller will **not** run
    /// `destroy`.    
    Clear,

    /// The resource agent does not know whether or not resources were left behind. The controller
    /// **will** run `destroy` if the creation task declares that resources are `Unknown` (or if the
    /// value is obtained by default because the creation task was unable to declare
    /// `ErrorResources`).
    Unknown,
}

impl Default for ErrorResources {
    fn default() -> Self {
        Self::Unknown
    }
}

impl ErrorResources {
    /// A description of the error resources variant value.
    fn description(&self) -> &'static str {
        match self {
            ErrorResources::Orphaned => "An error left resources that cannot be destroyed",
            ErrorResources::Remaining => "An error left resources that can be destroyed",
            ErrorResources::Clear => "An error occurred but no resources were left behind",
            ErrorResources::Unknown => "An error occurred but it is unknown if resources exist",
        }
    }
}

/// A status struct to be used by a resource agent.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceStatus {
    /// The state or the resource agent when creating resources.
    pub creation: ResourceAgentState,

    /// The state of the resource agent when destroying resources.
    pub destruction: ResourceAgentState,

    /// Open content to be used by the resource agent to store state.
    #[schemars(schema_with = "config_schema")]
    pub agent_info: Option<Map<String, Value>>,

    /// A description of the resource that has been created by the resource agent.
    #[schemars(schema_with = "config_schema")]
    pub created_resource: Option<Map<String, Value>>,
}

impl CrdExt for Resource {
    fn object_meta(&self) -> &ObjectMeta {
        self.meta()
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAgentState {
    pub task_state: TaskState,
    pub error: Option<ResourceError>,
}

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceError {
    /// The error message.
    pub error: String,
    /// The status of left-behind resources, if any.
    pub error_resources: ErrorResources,
}

impl Display for ResourceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} resources error: {}: {}",
            self.error_resources,
            self.error_resources.description(),
            self.error
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DestructionPolicy {
    /// The controller will delete this resource when the Kubernetes object is marked for deletion.
    OnDeletion,
    /// The controller will not delete this resource even when the Kubernetes object is deleted.
    Never,
    /// The controller will delete this resource when all tests requiring it have passed.
    OnTestSuccess,
    /// The controller will delete this resource when all tests requiring it have finished.
    OnTestCompletion,
}

impl Default for DestructionPolicy {
    fn default() -> Self {
        Self::OnDeletion
    }
}

derive_display_from_serialize!(DestructionPolicy);
derive_fromstr_from_deserialize!(DestructionPolicy);
