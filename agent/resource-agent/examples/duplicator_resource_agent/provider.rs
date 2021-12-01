/*!
 *
This program takes its input (the "spec") and writes it to its output (the "created resource"). The
purpose of this program is to test the resources that depend on other resources for their inputs,
and tests that depend on resources for their inputs.

!*/

use model::Configuration;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    info: Option<DuplicationConfig>,
}

impl Configuration for Memo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuplicationConfig {
    /// The info that will be copied to `DuplicatedData`.
    pub info: Value,
}

impl Configuration for DuplicationConfig {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatedData {
    /// The info we have duplicated.
    info: Value,
}

impl Configuration for DuplicatedData {}

pub struct DuplicationCreator {}

#[async_trait::async_trait]
impl Create for DuplicationCreator {
    type Config = DuplicationConfig;
    type Info = Memo;
    type Resource = DuplicatedData;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        let mut memo: Memo = client
            .get_info()
            .await
            .context(Resources::Clear, "Unable to get info from client")?;
        memo.info = Some(spec.configuration.clone());
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster created message",
        )?;
        Ok(DuplicatedData {
            info: spec.configuration.info.clone(),
        })
    }
}

pub struct DuplicationDestroyer {}
#[async_trait::async_trait]
impl Destroy for DuplicationDestroyer {
    type Config = DuplicationConfig;
    type Info = Memo;
    type Resource = DuplicatedData;

    async fn destroy<I>(
        &self,
        _spec: Option<Spec<Self::Config>>,
        _resource: Option<Self::Resource>,
        _client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        // Nothing to destroy.
        Ok(())
    }
}
