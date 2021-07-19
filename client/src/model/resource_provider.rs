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
    /// The configuration to send to the resource pod. This is 'open' to allow resource providers to
    /// define their own schemas.
    pub configuration: Option<Map<String, Value>>,
}

/// The status field of the ResourceProvider CRD.
#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
pub struct ResourceProviderStatus {
    // TODO
}
