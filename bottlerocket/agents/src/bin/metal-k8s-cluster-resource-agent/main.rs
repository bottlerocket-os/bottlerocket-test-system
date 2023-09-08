/*!

Provides Bottlerocket VMWare vSphere VMs to serve as Kubernetes nodes via `govc`.

!*/

mod metal_k8s_cluster_provider;

use crate::metal_k8s_cluster_provider::{MetalK8sClusterCreator, MetalK8sClusterDestroyer};
use agent_utils::init_agent_logger;
use resource_agent::clients::{DefaultAgentClient, DefaultInfoClient};
use resource_agent::error::AgentResult;
use resource_agent::{Agent, BootstrapData, Types};
use std::env;
use std::marker::PhantomData;

#[tokio::main]
async fn main() {
    init_agent_logger(env!("CARGO_CRATE_NAME"), None);
    let data = match BootstrapData::from_env() {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("Unable to get bootstrap data: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = run(data).await {
        eprintln!("{}", e);
        std::process::exit(1);
    };
}

async fn run(data: BootstrapData) -> AgentResult<()> {
    let types = Types {
        info_client: PhantomData::<DefaultInfoClient>,
        agent_client: PhantomData::<DefaultAgentClient>,
    };
    let agent = Agent::new(
        types,
        data,
        MetalK8sClusterCreator {},
        MetalK8sClusterDestroyer {},
    )
    .await?;
    agent.run().await
}
