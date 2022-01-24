/*!

This module provides clients that the resource agent uses to talk to Kubernetes.

!*/

mod agent_client;
mod error;
mod implementation;
mod info_client;

pub use agent_client::{AgentClient, DefaultAgentClient};
pub use error::{ClientError, ClientResult};
pub use info_client::{DefaultInfoClient, InfoClient};
