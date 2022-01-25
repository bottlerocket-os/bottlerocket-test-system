/*!

Provides Bottlerocket VMWare vSphere VMs to serve as Kubernetes nodes via `govc`.

!*/

mod aws;
mod tuf;
mod vsphere_vm_provider;

use crate::vsphere_vm_provider::{VMCreator, VMDestroyer};
use bottlerocket_agents::DEFAULT_AGENT_LEVEL_FILTER;
use env_logger::Builder;
use log::LevelFilter;
use resource_agent::clients::{DefaultAgentClient, DefaultInfoClient};
use resource_agent::error::AgentResult;
use resource_agent::{Agent, BootstrapData, Types};
use std::env;
use std::marker::PhantomData;

/// Extract the value of `RUST_LOG` if it exists, otherwise log this application at
/// `DEFAULT_AGENT_LEVEL_FILTER`.
pub fn init_agent_logger() {
    match env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level except AWS SDK.
            Builder::new()
                .filter_level(DEFAULT_AGENT_LEVEL_FILTER)
                .filter(Some("aws_"), LevelFilter::Error)
                .filter(Some("tracing"), LevelFilter::Error)
                .init();
        }
    }
}

#[tokio::main]
async fn main() {
    init_agent_logger();
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
        info_client: PhantomData::<DefaultInfoClient>::default(),
        agent_client: PhantomData::<DefaultAgentClient>::default(),
    };
    let agent = Agent::new(types, data, VMCreator {}, VMDestroyer {}).await?;
    agent.run().await
}
