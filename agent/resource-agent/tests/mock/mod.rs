/*!

This test module provides mock implementations of the [`AgentClient`] and [`InfoClient`] that
demonstrate what can be done to test without Kubernetes.

Also provided here are a very simple mock implementations of the [`Create`] and [`Destroy`] traits.

!*/

pub(crate) mod agent_client;
pub(crate) mod info_client;

use resource_agent::clients::InfoClient;
use resource_agent::provider::{Create, Destroy, ProviderError, ProviderResult, Resources, Spec};
use serde::{Deserialize, Serialize};
use testsys_model::Configuration;

/// InstanceCreator pretends to create instances for the sake demonstrating a mock resource provider.
pub(crate) struct InstanceCreator {}

/// InstanceDestroyer pretends to destroy instances for the sake demonstrating a mock resource
/// provider.
pub(crate) struct InstanceDestroyer {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    information: String,
}

impl Configuration for Memo {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    num_instances: u32,
    instance_type: String,
}

impl Configuration for InstanceConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedInstances {
    instance_ids: Vec<String>,
}

impl Configuration for CreatedInstances {}

#[async_trait::async_trait]
impl Create for InstanceCreator {
    type Config = InstanceConfig;
    type Info = Memo;
    type Resource = CreatedInstances;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        client
            .send_info(Memo {
                information: format!("Create {} instances", spec.configuration.num_instances),
            })
            .await
            .map_err(|e| ProviderError::new_with_source(Resources::Clear, e))?;
        Ok(CreatedInstances {
            instance_ids: vec!["123".to_string(), "456".to_string()],
        })
    }
}

#[async_trait::async_trait]
impl Destroy for InstanceDestroyer {
    type Config = InstanceConfig;
    type Info = Memo;
    type Resource = CreatedInstances;

    async fn destroy<I>(
        &self,
        _spec: Option<Spec<Self::Config>>,
        resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let resource = match resource {
            Some(some) => some,
            None => {
                return Err(ProviderError::new_with_context(
                    Resources::Unknown,
                    "Resource was 'None', unable to destroy resources.",
                ));
            }
        };

        for instance_id in resource.instance_ids {
            client
                .send_info(Memo {
                    information: format!("Destroying instance '{}'", instance_id),
                })
                .await
                .map_err(|e| ProviderError::new_with_source(Resources::Clear, e))?;
        }

        client
            .send_info(Memo {
                information: "Done destroying resources".into(),
            })
            .await
            .map_err(|e| ProviderError::new_with_source(Resources::Clear, e))?;

        Ok(())
    }
}
