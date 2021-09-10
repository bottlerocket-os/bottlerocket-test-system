/*!

The `agent` module defines the `Agent` object which provides the end-to-end program of a resource
provider.

!*/

use crate::bootstrap::Action;
use crate::clients::{AgentClient, InfoClient};
use crate::error::AgentResult;
use crate::provider::{Create, Destroy};
use crate::BootstrapData;
use client::model::Configuration;
use log::error;
use std::marker::PhantomData;

enum Provider<C, D>
where
    C: Create,
    D: Destroy,
{
    Create(C),
    Destroy(D),
}

/// The `Agent` drives the main program of a resource provider. It takes several injected types.
///
/// ## Configuration Types
///
/// These are "plain old data" structs that you define to carry custom information needed to create
/// and destroy your resources. These types are `Config`, `Info`, `Request` and `Resource`. See
/// the [`Create`] and [`Destroy`] traits for more information on these.
///
/// ## Dependency Injection for Testing
///
/// The `IClient` and `AClient` types are available so that you can inject mock clients and test
/// your code in the absence of Kubernetes. In practice you will use the  [`DefaultInfoClient`] and
/// [`DefaultAgentClient`] which implement Kubernetes API communication.
///
/// ## Your Custom Implementation
///
/// You implement the `Creator` (see [`Create`]) and `Destroyer` (see [`Destroy`]) types to create
/// and destroy resources.
///
pub struct Agent<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>
where
    Config: Configuration,
    Info: Configuration,
    Request: Configuration,
    Resource: Configuration,
    IClient: InfoClient,
    AClient: AgentClient,
    Creator: Create<Config = Config, Info = Info, Resource = Resource>,
    Destroyer: Destroy<Config = Config, Info = Info, Resource = Resource>,
{
    /// This field ensures that we are using all of the generic types in the struct's signature
    /// which helps us resolve the agent's generic types during construction.
    _types: Types<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>,

    /// The client that we will pass to the `Creator` and `Destroyer`.
    info_client: IClient,

    /// The client that the agent will use.
    agent_client: AClient,

    /// The user's custom `Create` and `Destroy` implementations.
    provider: Provider<Creator, Destroyer>,
}

/// The `Agent` requires specifying a lot of data types. The `Types` struct makes specifying these
/// a bit easier when constructing the `Agent`.
#[derive(Clone)]
pub struct Types<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>
where
    Config: Configuration,
    Info: Configuration,
    Resource: Configuration,
    IClient: InfoClient,
    AClient: AgentClient,
    Creator: Create<Config = Config, Info = Info, Resource = Resource>,
    Destroyer: Destroy<Config = Config, Info = Info, Resource = Resource>,
{
    pub config: PhantomData<Config>,
    pub info: PhantomData<Info>,
    pub request: PhantomData<Request>,
    pub resource: PhantomData<Resource>,
    pub info_client: PhantomData<IClient>,
    pub agent_client: PhantomData<AClient>,
    pub creator: PhantomData<Creator>,
    pub destroyer: PhantomData<Destroyer>,
}

impl<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>
    Agent<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>
where
    Config: Configuration,
    Info: Configuration,
    Request: Configuration,
    Resource: Configuration,
    IClient: InfoClient,
    AClient: AgentClient,
    Creator: Create<Config = Config, Info = Info, Resource = Resource>,
    Destroyer: Destroy<Config = Config, Info = Info, Resource = Resource>,
{
    /// Create a new `Agent` by providing the necessary bootstrapping data and all of the specific
    /// types that we will be using.
    pub async fn new(
        b: BootstrapData,
        t: Types<Config, Info, Request, Resource, IClient, AClient, Creator, Destroyer>,
    ) -> AgentResult<Self> {
        // Initialize the clients.
        let agent_client = AClient::new(b.clone()).await?;
        let info_client = match IClient::new(b.clone()).await {
            Ok(ok) => ok,
            Err(e) => {
                if let Err(send_error) = agent_client
                    .send_initialization_error(b.action, &e.to_string())
                    .await
                {
                    error!("Unable to send error '{}' to Kubernetes: {}", e, send_error);
                }
                return Err(e.into());
            }
        };

        // Get information about this resource provider from Kubernetes.
        let provider_info = agent_client.get_provider_info().await?;

        // Instantiate either `Create` or `Destroy` based on which operation we are doing.
        let provider = match b.action {
            Action::Create => Provider::Create(Creator::new(provider_info, &info_client).await?),
            Action::Destroy => {
                Provider::Destroy(Destroyer::new(provider_info, &info_client).await?)
            }
        };

        Ok(Self {
            _types: t,
            info_client,
            agent_client,
            provider,
        })
    }

    /// Either create or destroy resources based on which operation was requested when the `Agent`
    /// was instantiated.
    pub async fn run(&self) -> AgentResult<()> {
        match &self.provider {
            Provider::Create(c) => self.create(c).await,
            Provider::Destroy(d) => self.destroy(d).await,
        }
    }

    /// Create resources.
    async fn create<C>(&self, creator: &C) -> AgentResult<()>
    where
        C: Create,
    {
        self.agent_client.send_create_starting().await?;
        let request = self.agent_client.get_request().await?;
        match creator.create(request, &self.info_client).await {
            Ok(resource) => Ok(self.agent_client.send_create_succeeded(resource).await?),
            Err(e) => {
                if let Err(client_error) = self.agent_client.send_create_failed(&e).await {
                    error!("Unable to send error to Kubernetes: {}", client_error);
                    error!("The error we failed to send is: {}", e);
                }
                Err(e.into())
            }
        }
    }

    /// Destroy resources.
    async fn destroy<D>(&self, destroyer: &D) -> AgentResult<()>
    where
        D: Destroy,
    {
        self.agent_client.send_destroy_starting().await?;
        let resource = match self.agent_client.get_resource().await {
            Ok(r) => r,
            Err(e) => {
                error!("Unable to obtain resource from Kubernetes: {}", e);
                None
            }
        };

        match destroyer.destroy(resource, &self.info_client).await {
            Ok(()) => Ok(self.agent_client.send_destroy_succeeded().await?),
            Err(e) => {
                if let Err(client_error) = self.agent_client.send_destroy_failed(&e).await {
                    error!("Unable to send error to Kubernetes: {}", client_error);
                    error!("The error we failed to send is: {}", e);
                }
                Err(e.into())
            }
        }
    }
}
