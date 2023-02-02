/*!

This program is an example implementation of the resource agent component of TestSys. See
`./provider.rs` for an example of how you can create and destroy resources by implementing the
`Create` and `Destroy` traits.

!*/

mod provider;

use crate::provider::{RobotCreator, RobotDestroyer};
use env_logger::Builder;
use log::LevelFilter;
use resource_agent::clients::{DefaultAgentClient, DefaultInfoClient};
use resource_agent::error::AgentResult;
use resource_agent::{Agent, BootstrapData, Types};
use std::marker::PhantomData;

#[tokio::main]
async fn main() {
    init_logger();
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
        info_client: PhantomData::<DefaultInfoClient>::default(),
        agent_client: PhantomData::<DefaultAgentClient>::default(),
    };

    // We build the agent component and use it to either create or destroy resources.
    let agent = Agent::new(types, data, RobotCreator {}, RobotDestroyer {}).await?;
    agent.run().await
}

/// Extract the value of `RUST_LOG` if it exists, otherwise log this crate at
/// `DEFAULT_LEVEL_FILTER`.
fn init_logger() {
    match std::env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level for this crate only.
            Builder::new()
                .filter(Some(env!("CARGO_CRATE_NAME")), LevelFilter::Trace)
                .filter(Some("resource_agent"), LevelFilter::Trace)
                .filter(Some("testsys_model"), LevelFilter::Trace)
                .init();
        }
    }
}
