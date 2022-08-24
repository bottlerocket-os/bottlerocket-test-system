pub use error::{Error, Result};
pub use resource_client::ResourceClient;
pub use test_client::TestClient;

mod crd_client;
mod error;
mod http_status_code;
mod resource_client;
mod test_client;

pub use crd_client::CrdClient;
pub use http_status_code::{AllowNotFound, HttpStatusCode, StatusCode};
pub use resource_client::create_resource_crd;
pub use test_client::create_test_crd;
