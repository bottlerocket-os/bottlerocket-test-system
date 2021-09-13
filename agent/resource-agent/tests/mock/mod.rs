/*!

This test module provides mock implementations of the [`AgentClient`] and [`InfoClient`] that
demonstrate what can be done to test without Kubernetes.

Also provided here are a very simple mock implementations of the [`Create`] and [`Destroy`] traits.

!*/

pub(crate) mod agent_client;
pub(crate) mod info_client;

use client::model::Configuration;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, ProviderError, ProviderInfo, ProviderResult, Resources,
};
use serde::{Deserialize, Serialize};

/// InstanceCreator pretends to create instances for the sake demonstrating a mock test.
pub(crate) struct InstanceCreator {}

/// InstanceDestroyer pretends to destroy instances for the sake demonstrating a mock test.
pub(crate) struct InstanceDestroyer {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    default_region: String,
}

impl Configuration for ProviderConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Memo {
    information: String,
}

impl Configuration for Memo {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceRequest {
    num_instances: u32,
    instance_type: String,
}

impl Configuration for InstanceRequest {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreatedInstances {
    instance_ids: Vec<String>,
}

impl Configuration for CreatedInstances {}

#[async_trait::async_trait]
impl Create for InstanceCreator {
    type Config = ProviderConfig;
    type Info = Memo;
    type Request = InstanceRequest;
    type Resource = CreatedInstances;

    async fn new<I>(_info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient,
    {
        client
            .send_info(Memo {
                information: String::from("Create initializing"),
            })
            .await
            .map_err(|e| ProviderError::new_with_source(Resources::Clear, e))?;

        Ok(Self {})
    }

    async fn create<I>(&self, request: Self::Request, client: &I) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        client
            .send_info(Memo {
                information: format!("Create {} instances", request.num_instances),
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
    type Config = ProviderConfig;
    type Info = Memo;
    type Resource = CreatedInstances;

    async fn new<I>(_info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient,
    {
        client
            .send_info(Memo {
                information: String::from("Destroy initializing"),
            })
            .await
            .map_err(|e| ProviderError::new_with_source(Resources::Remaining, e))?;

        Ok(Self {})
    }

    async fn destroy<I>(&self, resource: Option<Self::Resource>, client: &I) -> ProviderResult<()>
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
