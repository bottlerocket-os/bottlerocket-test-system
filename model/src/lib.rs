/*!

This library provides the Kubernetes custom resource definitions and their API clients.

!*/

pub mod clients;
pub mod model;
mod resource_provider_client;
pub mod system;
mod test_client;

pub use resource_provider_client::Error as ResourceProviderClientError;
pub use resource_provider_client::ResourceProviderClient;
pub use resource_provider_client::Result as ResourceProviderClientResult;
pub use test_client::{Error, TestClient};
