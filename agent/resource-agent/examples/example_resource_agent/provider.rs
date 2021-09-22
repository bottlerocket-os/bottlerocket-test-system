/*!

This module demonstrates how to implement the `Create` and `Destroy` traits for a fictitious
resource provider. Our fictitious provider can create and destroy batches of robots in a given
color.

!*/

use model::Configuration;
use nonzero_ext::nonzero;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, ProviderError, ProviderInfo, ProviderResult, Resources,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroU16;
use tokio::time::{sleep, Duration};

/// The color of a robot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub enum Color {
    Gray,
    Purple,
    Silver,
}

impl Default for Color {
    fn default() -> Self {
        Self::Gray
    }
}

/// The configuration information for a robot provider. This configuration determines what color of
/// robots the provider can create, and how many it can create in a single batch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RobotProviderConfig {
    /// The colors of which this provider can create robots.
    available_colors: HashSet<Color>,

    /// The maximum batch size of robots this provider can create.
    max_order_size: NonZeroU16,
}

impl Default for RobotProviderConfig {
    fn default() -> Self {
        Self {
            available_colors: Default::default(),
            max_order_size: nonzero!(1u16),
        }
    }
}

/// We need to specify that our configuration struct implements the `Configuration` trait. Doing
/// so also requires that we implement a few things like `Default`, `serde::Serialize` and
/// `serde::Deserialize`.
impl Configuration for RobotProviderConfig {}

/// While we are creating robots, we might need to remember some things in case we encounter an
/// error. We can define a struct for that purpose.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProductionMemo {
    /// In this example we put some traces here that describe what our provider is doing.
    pub current_status: String,

    /// If we have created any robots, we can remember their IDs here.
    pub existing_robot_ids: HashSet<u64>,
}

impl Configuration for ProductionMemo {}

/// When a TestSys test needs some robots, it needs to tell us how many and what color they should
/// be.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RobotProductionRequest {
    /// The color of robots to create in this batch.
    pub color: Color,

    /// How many robots to create.
    pub number_of_robots: NonZeroU16,
}

impl Default for RobotProductionRequest {
    fn default() -> Self {
        Self {
            color: Default::default(),
            number_of_robots: nonzero!(1u16),
        }
    }
}

impl Configuration for RobotProductionRequest {}

/// Once we have fulfilled the `Create` request, we return information about the batch of robots we
/// created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreatedRobotLot {
    /// The color of robots we created.
    pub color: Color,

    /// The number of robots we created.
    pub number_of_robots: NonZeroU16,

    /// The IDs of the robots we created.
    pub ids: HashSet<u64>,
}

impl Default for CreatedRobotLot {
    fn default() -> Self {
        Self {
            color: Default::default(),
            number_of_robots: nonzero!(1u16),
            ids: Default::default(),
        }
    }
}

impl Configuration for CreatedRobotLot {}

/// This is the object that will create robots.
pub struct RobotCreator {
    /// The configuration of this robot provider.
    pub config: RobotProviderConfig,
}

/// We need to implement the [`Create`] trait in order to provide robots for tests.
#[async_trait::async_trait]
impl Create for RobotCreator {
    /// The struct we will use to configure out provider.
    type Config = RobotProviderConfig;

    /// The struct we will use to remember things like the IDs of robots we have created so far.
    type Info = ProductionMemo;

    /// The request for robots that a test will have when it needs us to create them.
    type Request = RobotProductionRequest;

    /// The response we will give back describing the batch of robots we have created.
    type Resource = CreatedRobotLot;

    async fn new<I>(info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient,
    {
        // Here we record our status in Kubernetes. In a real-world case you might want to check
        // here to see if the provider had been running before now.
        client
            .send_info(ProductionMemo {
                current_status: "initializing creator".to_string(),
                existing_robot_ids: Default::default(),
            })
            .await
            .map_err(|e| {
                ProviderError::new_with_source_and_context(
                    Resources::Clear,
                    "Error initializing creator",
                    e,
                )
            })?;

        Ok(Self {
            config: info.configuration,
        })
    }

    async fn create<I>(&self, request: Self::Request, client: &I) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        // We get our custom state information from Kubernetes.
        let mut memo: ProductionMemo = client.get_info().await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                // Since we can't get our custom state information, we don't know whether or not
                // this provider has previously created robots that need to be destroyed.
                Resources::Unknown,
                "Unable to get info from client",
                e,
            )
        })?;

        // We pretend here like it's important that are robot provider is in the correct state. This
        // is an example of using the custom `Info` for some purpose.
        if memo.current_status != "initializing creator" {
            return Err(ProviderError::new_with_context(
                resources_situation(&memo),
                format!("Unexpected state: {}", memo.current_status),
            ));
        }

        // If the request is for more robots than our maximum batch size, then we have a problem.
        if request.number_of_robots > self.config.max_order_size {
            return Err(ProviderError::new_with_context(
                resources_situation(&memo),
                format!(
                    "Production request too large. {} robots requested, but max size is {}",
                    request.number_of_robots, self.config.max_order_size
                ),
            ));
        }

        // If the request is for robots of a color we cannot provide, then we have a problem.
        if !self.config.available_colors.contains(&request.color) {
            return Err(ProviderError::new_with_context(
                resources_situation(&memo),
                format!(
                    "Production request for unavailable color. '{:?}' requested, but I don't have it.",
                    request.color
                ),
            ));
        }

        // Create the robots.
        for id in 0..request.number_of_robots.get() {
            let memo_text = format!("creating robot {}", id);
            memo.current_status = memo_text.clone();
            memo.existing_robot_ids.insert(id.into());

            // We record the robot ID before we actually create it in case something goes wrong.
            client.send_info(memo.clone()).await.map_err(|e| {
                ProviderError::new_with_source_and_context(
                    resources_situation(&memo),
                    format!("Error creating robot id {}", id),
                    e,
                )
            })?;

            // We would actually create the robot here.
            sleep(Duration::from_millis(500)).await;
        }

        // We are done, set our custom status to say so.
        memo.current_status = "All robots created".into();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources_situation(&memo),
                "Error sending final creation message",
                e,
            )
        })?;

        // Return a description of the batch of robots that we created.
        Ok(CreatedRobotLot {
            color: request.color,
            number_of_robots: request.number_of_robots,
            ids: memo.existing_robot_ids,
        })
    }
}

/// This is the object that will destroy robots.
pub struct RobotDestroyer {
    /// We aren't actually using this configuration in the destroyer, but we could if we needed to.
    pub config: RobotProviderConfig,
}

/// We need to implement the `Destroy` trait so that our destroyer can destroy robots that have been
/// created for TestSys tests.
#[async_trait::async_trait]
impl Destroy for RobotDestroyer {
    /// The struct we will use to configure out provider.
    type Config = RobotProviderConfig;

    /// The struct we will use to remember things like the IDs of robots we have created so far.
    type Info = ProductionMemo;

    /// The response we will give back describing the batch of robots we have created.
    type Resource = CreatedRobotLot;

    async fn new<I>(info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient,
    {
        // Here we record our status in Kubernetes. In a real-world case you might want to use this
        // status information to inform what the destroyer does.
        client
            .send_info(ProductionMemo {
                current_status: "initializing destroyer".to_string(),
                existing_robot_ids: Default::default(),
            })
            .await
            .map_err(|e| {
                ProviderError::new_with_source_and_context(
                    Resources::Clear,
                    "Error initializing destroyer",
                    e,
                )
            })?;

        Ok(Self {
            config: info.configuration,
        })
    }

    async fn destroy<I>(&self, resource: Option<Self::Resource>, client: &I) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        // We get our custom state information from Kubernetes.
        let mut memo: ProductionMemo = client.get_info().await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                Resources::Unknown,
                "Unable to get info from client",
                e,
            )
        })?;

        // We pretend here like it's important that are robot provider is in the correct state. This
        // is an example of using the custom `Info` for some purpose.
        if memo.current_status != "initializing destroyer" {
            return Err(ProviderError::new_with_context(
                resources_situation(&memo),
                format!("Unexpected state: {}", memo.current_status),
            ));
        }

        // Create a set of IDs to iterate over and destroy. Also ensure that the memo's IDs match.
        let ids = if let Some(resource) = resource {
            memo.existing_robot_ids = resource.ids.clone();
            resource.ids
        } else {
            memo.clone().existing_robot_ids
        };

        for id in ids {
            let memo_text = format!("destroying robot {}", id);
            memo.current_status = memo_text.clone();
            memo.existing_robot_ids.remove(&id);
            client.send_info(memo.clone()).await.map_err(|e| {
                ProviderError::new_with_source_and_context(
                    resources_situation(&memo),
                    format!("Error destroying robot id {}", id),
                    e,
                )
            })?;
            // We would actually destroy the robot here.
            sleep(Duration::from_millis(500)).await;
        }

        memo.current_status = "All robots destroyed".into();
        memo.existing_robot_ids = HashSet::new();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources_situation(&memo),
                "Error sending final destruction message",
                e,
            )
        })?;

        Ok(())
    }
}

/// When something goes wrong, we need to let the controller know whether or not we have existing
/// robots out there that need to be destroyed. We can do this by checking our `ProductionMemo`.
fn resources_situation(memo: &ProductionMemo) -> Resources {
    if memo.existing_robot_ids.is_empty() {
        Resources::Clear
    } else {
        Resources::Remaining
    }
}
