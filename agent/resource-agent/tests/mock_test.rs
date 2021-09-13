pub(crate) mod mock;

use mock::agent_client::MockAgentClient;
use mock::info_client::MockInfoClient;
use mock::{
    CreatedInstances, InstanceCreator, InstanceDestroyer, InstanceRequest, Memo, ProviderConfig,
};
use resource_agent::{Action, Agent, BootstrapData, Types};
use std::marker::PhantomData;

/// This test demonstrates the the use of mock clients so that [`Create`] and [`Destroy`] implementations can be  tested
/// in the absence of Kubernetes.
#[tokio::test]
async fn mock_test() {
    let types = Types {
        config: PhantomData::<ProviderConfig>::default(),
        info: PhantomData::<Memo>::default(),
        request: PhantomData::<InstanceRequest>::default(),
        resource: PhantomData::<CreatedInstances>::default(),
        info_client: PhantomData::<MockInfoClient>::default(),
        agent_client: PhantomData::<MockAgentClient>::default(),
        creator: PhantomData::<InstanceCreator>::default(),
        destroyer: PhantomData::<InstanceDestroyer>::default(),
    };

    let agent = Agent::new(
        BootstrapData {
            test_name: "mock-test".to_string(),
            resource_provider_name: "mock-instance-provider".to_string(),
            resource_name: "some-instances".to_string(),
            action: Action::Create,
        },
        types,
    )
    .await
    .unwrap();

    agent.run().await.unwrap();

    let types = Types {
        config: PhantomData::<ProviderConfig>::default(),
        info: PhantomData::<Memo>::default(),
        request: PhantomData::<InstanceRequest>::default(),
        resource: PhantomData::<CreatedInstances>::default(),
        info_client: PhantomData::<MockInfoClient>::default(),
        agent_client: PhantomData::<MockAgentClient>::default(),
        creator: PhantomData::<InstanceCreator>::default(),
        destroyer: PhantomData::<InstanceDestroyer>::default(),
    };

    let agent = Agent::new(
        BootstrapData {
            test_name: "mock-test".to_string(),
            resource_provider_name: "mock-instance-provider".to_string(),
            resource_name: "some-instances".to_string(),
            action: Action::Destroy,
        },
        types,
    )
    .await
    .unwrap();
    agent.run().await.unwrap();
}
