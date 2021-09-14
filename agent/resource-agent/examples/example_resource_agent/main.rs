/*!

This program is an example implementation of the resource agent component of TestSys. See
`./provider.rs` for an example of how you can create and destroy resources by implementing the
`Create` and `Destroy` traits.

!*/

mod provider;

use crate::provider::{
    CreatedRobotLot, ProductionMemo, RobotCreator, RobotDestroyer, RobotProductionRequest,
    RobotProviderConfig,
};
use resource_agent::clients::{DefaultAgentClient, DefaultInfoClient};
use resource_agent::error::AgentResult;
use resource_agent::{Agent, BootstrapData, Types};
use std::marker::PhantomData;

#[tokio::main]
async fn main() {
    // This will get information that is provided to the container by the TestSys controller.
    let data = match BootstrapData::from_env() {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("Unable to get bootstrap data: {}", e);
            std::process::exit(1);
        }
    };

    // Pass the bootstrap data to a run function.
    if let Err(e) = run(data).await {
        eprintln!("{}", e);
        std::process::exit(1);
    };
}

async fn run(data: BootstrapData) -> AgentResult<()> {
    // We specify all of our custom types with this PhantomData struct.
    let types = Types {
        config: PhantomData::<RobotProviderConfig>::default(),
        info: PhantomData::<ProductionMemo>::default(),
        request: PhantomData::<RobotProductionRequest>::default(),
        resource: PhantomData::<CreatedRobotLot>::default(),
        info_client: PhantomData::<DefaultInfoClient>::default(),
        agent_client: PhantomData::<DefaultAgentClient>::default(),
        creator: PhantomData::<RobotCreator>::default(),
        destroyer: PhantomData::<RobotDestroyer>::default(),
    };

    // We build the agent component and use it to either create or destroy resources.
    let agent = Agent::new(data, types).await?;
    agent.run().await
}
