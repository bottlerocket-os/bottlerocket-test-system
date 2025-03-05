/*!

`bottlerocket-agents` is a collection of test agent and resource agent implementations used to test
Bottlerocket instances.
This `lib.rs` provides code that is used by multiple agent binaries or used by the `testsys` CLI.

!*/

use bottlerocket_types::agent_config::CreationPolicy;
use resource_agent::provider::{IntoProviderError, ProviderError, ProviderResult, Resources};

pub mod clusters;
pub mod constants;
pub mod error;
pub mod sonobuoy;
pub mod tuf;
pub mod userdata;
pub mod vsphere;
pub mod workload;

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
                        "The cluster '{}' already exists and creation policy '{:?}' requires that it does not exist",
                        cluster_name,
                        creation_policy
                    )
                )
            ),
        CreationPolicy::Never if !*cluster_exists =>
            Err(
                ProviderError::new_with_context(
                    Resources::Clear, format!(
                        "The cluster '{}' does not exist and creation policy '{:?}' requires that it exists",
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

/// Retrieve secret value from AWS Secrets Manager
pub async fn get_secret_values(
    secretsmanager_client: &aws_sdk_secretsmanager::Client,
    secret_name: &str,
) -> ProviderResult<serde_json::Value> {
    let secret_value = secretsmanager_client
        .get_secret_value()
        .secret_id(secret_name)
        .send()
        .await
        .context(Resources::Clear, "Unable to get info from client")?
        .secret_string
        .context(Resources::Clear, format!("{} not found", secret_name))?;

    // Parse the secret values into a serde_json::Value
    let parsed_secret_value: serde_json::Value = serde_json::from_str(&secret_value)
        .context(Resources::Clear, "Unable to parse secret values")?;
    Ok(parsed_secret_value)
}
