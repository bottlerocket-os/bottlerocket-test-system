/*!

This library provides the Kubernetes custom resource definitions and their API clients.

!*/

pub mod clients;
mod configuration;
pub mod constants;
mod resource;
mod resource_provider;
pub mod system;
mod test;

pub use configuration::{Configuration, ConfigurationError};
pub use resource::{ErrorResources, ResourceAgentState, ResourceRequest, ResourceStatus};
pub use resource_provider::{ResourceProvider, ResourceProviderSpec, ResourceProviderStatus};
pub use test::{
    Agent, AgentStatus, ControllerStatus, Lifecycle, RunState, Test, TestResults, TestSpec,
    TestStatus,
};
