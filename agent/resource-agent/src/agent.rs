/*!

The `agent` module defines the `Agent` object which provides the end-to-end program of a resource
provider.

!*/

use crate::clients::{AgentClient, InfoClient};
use crate::error::AgentResult;
use crate::provider::{Create, Destroy};
use crate::{BootstrapData, Configuration, ResourceAction};
use log::{debug, error, info, trace};
use std::marker::PhantomData;

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
pub struct Agent<Info, Request, Resource, IClient, AClient, Creator, Destroyer>
where
    Info: Configuration,
    Request: Configuration,
    Resource: Configuration,
    IClient: InfoClient,
    AClient: AgentClient,
    Creator: Create<Request = Request, Info = Info, Resource = Resource>,
    Destroyer: Destroy<Request = Request, Info = Info, Resource = Resource>,
{
    /// This field ensures that we are using all of the generic types in the struct's signature
    /// which helps us resolve the agent's generic types during construction.
    _types: Types<IClient, AClient>,

    /// The client that we will pass to the `Creator` and `Destroyer`.
    info_client: IClient,

    /// The client that the agent will use.
    agent_client: AClient,

    /// The user's custom `Create` and `Destroy` implementations.
    creator: Creator,
    destroyer: Destroyer,
    action: ResourceAction,
}

/// The `Agent` requires specifying a lot of data types. The `Types` struct makes specifying these
/// a bit easier when constructing the `Agent`.
#[derive(Clone)]
pub struct Types<IClient, AClient>
where
    IClient: InfoClient,
    AClient: AgentClient,
{
    pub info_client: PhantomData<IClient>,
    pub agent_client: PhantomData<AClient>,
}

impl<Info, Request, Resource, IClient, AClient, Creator, Destroyer>
    Agent<Info, Request, Resource, IClient, AClient, Creator, Destroyer>
where
    Info: Configuration,
    Request: Configuration,
    Resource: Configuration,
    IClient: InfoClient,
    AClient: AgentClient,
    Creator: Create<Request = Request, Info = Info, Resource = Resource>,
    Destroyer: Destroy<Request = Request, Info = Info, Resource = Resource>,
{
    /// Create a new `Agent` by providing the necessary bootstrapping data and all of the specific
    /// types that we will be using.
    pub async fn new(
        types: Types<IClient, AClient>,
        bootstrap_data: BootstrapData,
        creator: Creator,
        destroyer: Destroyer,
        // t: Types<Info, Request, Resource, IClient, AClient, Creator, Destroyer>,
    ) -> AgentResult<Self> {
        info!("Initializing Agent");
        // Initialize the clients.
        trace!("Creating agent client");
        let agent_client = AClient::new(bootstrap_data.clone()).await?;
        trace!("Creating info client");
        let info_client = match IClient::new(bootstrap_data.clone()).await {
            Ok(ok) => ok,
            Err(e) => {
                if let Err(send_error) = agent_client
                    .send_init_error(bootstrap_data.action, &e.to_string())
                    .await
                {
                    error!("Unable to send error '{}' to Kubernetes: {}", e, send_error);
                }
                return Err(e.into());
            }
        };

        trace!("Successfully constructed agent");
        Ok(Self {
            _types: types,
            info_client,
            agent_client,
            creator,
            destroyer,
            action: bootstrap_data.action,
        })
    }

    /// Either create or destroy resources based on which operation was requested when the `Agent`
    /// was instantiated.
    pub async fn run(&self) -> AgentResult<()> {
        debug!("Agent::run starting");
        match &self.action {
            ResourceAction::Create => self.create().await,
            ResourceAction::Destroy => self.destroy().await,
        }
    }

    /// Create resources.
    async fn create(&self) -> AgentResult<()> {
        trace!("sending create start signal");
        self.agent_client.send_create_starting().await?;
        debug!("Getting configuration (i.e. request)");
        let request = self.agent_client.get_request().await?;
        trace!("request\n{:?}", request);
        match self.creator.create(request, &self.info_client).await {
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
    async fn destroy(&self) -> AgentResult<()> {
        self.agent_client.send_destroy_starting().await?;
        let resource = match self.agent_client.get_created_resource::<Resource>().await {
            Ok(r) => r,
            Err(e) => {
                error!("Unable to obtain resource info from Kubernetes: {}", e);
                None
            }
        };

        let request = match self.agent_client.get_request::<Request>().await {
            Ok(r) => Some(r),
            Err(e) => {
                error!("Unable to obtain resource request from Kubernetes: {}", e);
                None
            }
        };

        match self
            .destroyer
            .destroy(request, resource, &self.info_client)
            .await
        {
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
