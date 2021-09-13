use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An object representing a container that can be run to create and destroy resources. The
/// `CustomResource` derive also produces a struct named `ResourceProvider` which represents a
/// resource provider object in the k8s API.
#[derive(Clone, CustomResource, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[kube(
    derive = "Default",
    derive = "PartialEq",
    group = "testsys.bottlerocket.aws",
    kind = "ResourceProvider",
    namespaced,
    plural = "resource-providers",
    shortname = "rp",
    singular = "resource-provider",
    status = "ResourceProviderStatus",
    version = "v1"
)]
pub struct ResourceProviderSpec {
    /// The URI of the resource agent container image.
    pub image: String,
    /// The name of an image registry pull secret if one is needed to pull the resource agent image.
    pub pull_secret: Option<String>,
    /// The configuration to pass to the resource agent pod. This is 'open' to allow resource
    /// agents to define their own schemas.
    pub configuration: Option<Map<String, Value>>,
}

/// The status field of the ResourceProvider CRD.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ResourceProviderStatus {
    // TODO
}
