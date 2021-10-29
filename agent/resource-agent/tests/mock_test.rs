pub(crate) mod mock;

use mock::agent_client::MockAgentClient;
use mock::info_client::MockInfoClient;
use mock::{InstanceCreator, InstanceDestroyer};
use resource_agent::{Agent, BootstrapData, ResourceAction, Types};
use std::marker::PhantomData;

/// This test demonstrates the the use of mock clients so that [`Create`] and [`Destroy`] implementations can be  tested
/// in the absence of Kubernetes.
#[tokio::test]
async fn mock_test() {
    let types = Types {
        info_client: PhantomData::<MockInfoClient>::default(),
        agent_client: PhantomData::<MockAgentClient>::default(),
    };

    let agent = Agent::new(
        types,
        BootstrapData {
            resource_name: "some-instances".to_string(),
            action: ResourceAction::Create,
        },
        InstanceCreator {},
        InstanceDestroyer {},
    )
    .await
    .unwrap();

    agent.run().await.unwrap();

    let types = Types {
        info_client: PhantomData::<MockInfoClient>::default(),
        agent_client: PhantomData::<MockAgentClient>::default(),
    };

    let agent = Agent::new(
        types,
        BootstrapData {
            resource_name: "some-instances".to_string(),
            action: ResourceAction::Destroy,
        },
        InstanceCreator {},
        InstanceDestroyer {},
    )
    .await
    .unwrap();
    agent.run().await.unwrap();
}
