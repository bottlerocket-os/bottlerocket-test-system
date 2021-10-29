pub use error::{Error, Result};
pub use resource_client::ResourceClient;
pub use test_client::TestClient;

mod crd_client;
mod error;
mod resource_client;
mod test_client;

pub use crd_client::CrdClient;
