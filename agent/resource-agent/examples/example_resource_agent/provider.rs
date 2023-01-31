/*!

This module demonstrates how to implement the `Create` and `Destroy` traits for a fictitious
resource provider. Our fictitious provider can create and destroy batches of robots in a given
color.

!*/

use log::{debug, info};
use nonzero_ext::nonzero;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{Create, Destroy, ProviderError, ProviderResult, Resources, Spec};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::num::NonZeroU16;
use testsys_model::Configuration;
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

/// While we are creating robots, we might need to remember some things in case we encounter an
/// error. We can define a struct for that purpose.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct RobotConfig {
    /// The color of robots to create in this batch.
    pub color: Color,

    /// How many robots to create.
    pub number_of_robots: NonZeroU16,
}

impl Default for RobotConfig {
    fn default() -> Self {
        Self {
            color: Default::default(),
            number_of_robots: nonzero!(1u16),
        }
    }
}

impl Configuration for RobotConfig {}

/// Once we have fulfilled the `Create` request, we return information about the batch of robots we
/// created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub struct RobotCreator {}

/// We need to implement the [`Create`] trait in order to provide robots for tests.
#[async_trait::async_trait]
impl Create for RobotCreator {
    /// The configuration for creating robots.
    type Config = RobotConfig;

    /// The struct we will use to remember things like the IDs of robots we have created so far.
    type Info = ProductionMemo;

    /// The response we will give back describing the batch of robots we have created.
    type Resource = CreatedRobotLot;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        info!("starting creation");

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

        // Create the robots.
        for id in 0..spec.configuration.number_of_robots.get() {
            let memo_text = format!("creating robot {}", id);
            debug!("{}", memo_text);
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
        info!("{}", memo.current_status);
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources_situation(&memo),
                "Error sending final creation message",
                e,
            )
        })?;

        // Return a description of the batch of robots that we created.
        Ok(CreatedRobotLot {
            color: spec.configuration.color,
            number_of_robots: spec.configuration.number_of_robots,
            ids: memo.existing_robot_ids,
        })
    }
}

/// This is the object that will destroy robots.
pub struct RobotDestroyer {}

/// We need to implement the `Destroy` trait so that our destroyer can destroy robots that have been
/// created for TestSys tests.
#[async_trait::async_trait]
impl Destroy for RobotDestroyer {
    /// The configuration that was used to create the robots.
    type Config = RobotConfig;

    /// The struct we will use to remember things like the IDs of robots we have created so far.
    type Info = ProductionMemo;

    /// The response we will give back describing the batch of robots we have created.
    type Resource = CreatedRobotLot;

    async fn destroy<I>(
        &self,
        _spec: Option<Spec<Self::Config>>,
        resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
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
