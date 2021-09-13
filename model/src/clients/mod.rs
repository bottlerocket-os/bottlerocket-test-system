mod resource_provider_client;
mod test_client;

pub use resource_provider_client::{
    Error as ResourceProviderClientError, ResourceProviderClient,
    Result as ResourceProviderClientResult,
};
pub use test_client::{Error, Result, TestClient};
