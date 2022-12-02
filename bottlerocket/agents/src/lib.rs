/*!

`bottlerocket-agents` is a collection of test agent and resource agent implementations used to test
Bottlerocket instances.
This `lib.rs` provides code that is used by multiple agent binaries or used by the `testsys` CLI.

!*/

use bottlerocket_types::agent_config::CreationPolicy;
use resource_agent::provider::{ProviderError, ProviderResult, Resources};

pub mod constants;
pub mod error;
pub mod sonobuoy;
pub mod tuf;
pub mod userdata;
pub mod vsphere;

/// Determines whether a cluster resource needs to be created given its creation policy
pub async fn is_cluster_creation_required(
    cluster_exists: &bool,
    cluster_name: &str,
    creation_policy: &CreationPolicy,
) -> ProviderResult<(bool, String)> {
    match creation_policy {
        CreationPolicy::Create if *cluster_exists =>
            Err(
                ProviderError::new_with_context(
                    Resources::Clear, format!(
                        "The cluster '{}' already existed and creation policy '{:?}' requires that it not exist",
                        cluster_name,
                        creation_policy
                    )
                )
            ),
        CreationPolicy::Never if !*cluster_exists =>
            Err(
                ProviderError::new_with_context(
                    Resources::Clear, format!(
                        "The cluster '{}' does not exist and creation policy '{:?}' requires that it exist",
                        cluster_name,
                        creation_policy
                    )
                )
            ),
        CreationPolicy::Create  =>{
            Ok((true, format!("Creation policy is '{:?}' and cluster '{}' does not exist: creating cluster", creation_policy, cluster_name)))
        },
        CreationPolicy::IfNotExists if !*cluster_exists => {
            Ok((true, format!("Creation policy is '{:?}' and cluster '{}' does not exist: creating cluster", creation_policy, cluster_name)))
        },
        CreationPolicy::IfNotExists |
        CreationPolicy::Never => {
            Ok((false, format!("Creation policy is '{:?}' and cluster '{}' exists: not creating cluster", creation_policy, cluster_name)))
        },
    }
}
