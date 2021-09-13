use super::error::ClientResult;
use crate::clients::{AgentClient, ClientError, DefaultAgentClient, DefaultInfoClient, InfoClient};
use crate::provider::{ProviderError, ProviderInfo, Resources};
use crate::{Action, BootstrapData};
use model::clients::{ResourceProviderClient, TestClient};
use model::model::{Configuration, ConfigurationError, ErrorResources, ResourceAgentState};

impl From<model::clients::Error> for ClientError {
    fn from(e: model::clients::Error) -> Self {
        ClientError::RequestFailed(Some(Box::new(e)))
    }
}

impl From<model::clients::ResourceProviderClientError> for ClientError {
    fn from(e: model::clients::ResourceProviderClientError) -> Self {
        ClientError::RequestFailed(Some(Box::new(e)))
    }
}

impl From<ConfigurationError> for ClientError {
    fn from(e: ConfigurationError) -> Self {
        ClientError::Serialization(Some(Box::new(e)))
    }
}

impl From<Resources> for ErrorResources {
    fn from(r: Resources) -> Self {
        match r {
            Resources::Orphaned => ErrorResources::Orphaned,
            Resources::Remaining => ErrorResources::Remaining,
            Resources::Clear => ErrorResources::Clear,
            Resources::Unknown => ErrorResources::Unknown,
        }
    }
}

#[async_trait::async_trait]
impl InfoClient for DefaultInfoClient {
    async fn new(data: BootstrapData) -> ClientResult<Self> {
        let client = TestClient::new()
            .await
            .map_err(|e| ClientError::InitializationFailed(Some(Box::new(e))))?;
        Ok(Self { data, client })
    }

    async fn get_info<Info>(&self) -> ClientResult<Info>
    where
        Info: Configuration,
    {
        let maybe_info = if let Some(status) = self
            .client
            .get_resource_status(&self.data.test_name, &self.data.resource_name)
            .await?
        {
            status.agent_info
        } else {
            return Ok(Info::default());
        };
        let map = if let Some(map) = maybe_info {
            map
        } else {
            return Ok(Info::default());
        };
        let info: Info = Configuration::from_map(map)?;
        Ok(info)
    }

    async fn send_info<Info>(&self, info: Info) -> ClientResult<()>
    where
        Info: Configuration,
    {
        self.client
            .set_resource_agent_info(&self.data.test_name, &self.data.resource_name, info)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentClient for DefaultAgentClient {
    async fn new(data: BootstrapData) -> ClientResult<Self> {
        Ok(Self {
            data,
            test_client: TestClient::new()
                .await
                .map_err(|e| ClientError::InitializationFailed(Some(Box::new(e))))?,
            resource_provider_client: ResourceProviderClient::new()
                .await
                .map_err(|e| ClientError::InitializationFailed(Some(Box::new(e))))?,
        })
    }

    async fn send_initialization_error(&self, action: Action, error: &str) -> ClientResult<()> {
        let state = match action {
            Action::Create => ResourceAgentState::CreateFailed,
            Action::Destroy => ResourceAgentState::DestroyFailed,
        };

        let _ = self
            .test_client
            .set_resource_agent_error(
                &self.data.test_name,
                &self.data.resource_name,
                state,
                error,
                ErrorResources::Unknown,
            )
            .await?;
        Ok(())
    }

    async fn get_provider_info<Config>(&self) -> ClientResult<ProviderInfo<Config>>
    where
        Config: Configuration,
    {
        let resource_provider = self
            .resource_provider_client
            .get_resource_provider(&self.data.resource_provider_name)
            .await?;

        let configuration: Config = match resource_provider.spec.configuration {
            Some(map) => Configuration::from_map(map)?,
            None => Config::default(),
        };
        Ok(ProviderInfo { configuration })
    }

    async fn get_request<Request>(&self) -> ClientResult<Request>
    where
        Request: Configuration,
    {
        let request = self
            .test_client
            .get_resource_request(&self.data.test_name, &self.data.resource_name)
            .await?
            .ok_or_else(|| {
                ClientError::MissingData(Some(
                    format!("the resource '{}' was not found", self.data.resource_name).into(),
                ))
            })?;
        let config_map = match request.configuration {
            Some(map) => map,
            None => return Ok(Request::default()),
        };
        Ok(Configuration::from_map(config_map)?)
    }

    async fn get_resource<Resource>(&self) -> ClientResult<Option<Resource>>
    where
        Resource: Configuration,
    {
        let resource_status = if let Some(rs) = self
            .test_client
            .get_resource_status(&self.data.test_name, &self.data.resource_name)
            .await?
        {
            rs
        } else {
            return Ok(None);
        };
        Ok(match resource_status.created_resource {
            None => None,
            Some(resource) => {
                let resource: Resource = Configuration::from_map(resource)?;
                Some(resource)
            }
        })
    }

    async fn send_create_starting(&self) -> ClientResult<()> {
        self.test_client
            .set_resource_agent_state(
                &self.data.test_name,
                &self.data.resource_name,
                ResourceAgentState::Creating,
            )
            .await?;
        Ok(())
    }

    async fn send_create_succeeded<Resource>(&self, resource: Resource) -> ClientResult<()>
    where
        Resource: Configuration,
    {
        let _ = self
            .test_client
            .set_resource_created(&self.data.test_name, &self.data.resource_name, resource)
            .await?;
        Ok(())
    }

    async fn send_create_failed(&self, error: &ProviderError) -> ClientResult<()> {
        let _ = self
            .test_client
            .set_resource_agent_error(
                &self.data.test_name,
                &self.data.resource_name,
                ResourceAgentState::CreateFailed,
                error.to_string(),
                error.resources().into(),
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_starting(&self) -> ClientResult<()> {
        self.test_client
            .set_resource_agent_state(
                &self.data.test_name,
                &self.data.resource_name,
                ResourceAgentState::Destroying,
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_succeeded(&self) -> ClientResult<()> {
        self.test_client
            .set_resource_agent_state(
                &self.data.test_name,
                &self.data.resource_name,
                ResourceAgentState::Destroyed,
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_failed(&self, error: &ProviderError) -> ClientResult<()> {
        let _ = self
            .test_client
            .set_resource_agent_error(
                &self.data.test_name,
                &self.data.resource_name,
                ResourceAgentState::DestroyFailed,
                error.to_string(),
                error.resources().into(),
            )
            .await?;
        Ok(())
    }
}
