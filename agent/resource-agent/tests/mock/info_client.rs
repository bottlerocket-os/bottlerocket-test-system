use model::model::Configuration;
use resource_agent::clients::{ClientResult, InfoClient};
use resource_agent::BootstrapData;

/// Create an [`InfoClient`] that does nothing so that we can test without Kubernetes.
pub(crate) struct MockInfoClient {}

#[async_trait::async_trait]
impl InfoClient for MockInfoClient {
    async fn new(_data: BootstrapData) -> ClientResult<Self> {
        Ok(Self {})
    }

    async fn get_info<Info>(&self) -> ClientResult<Info>
    where
        Info: Configuration,
    {
        Ok(Info::default())
    }

    async fn send_info<Info>(&self, _info: Info) -> ClientResult<()>
    where
        Info: Configuration,
    {
        Ok(())
    }
}
