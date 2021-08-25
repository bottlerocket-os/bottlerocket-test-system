/*!

This library provides the Kubernetes custom resource definitions and their API clients.

!*/

pub mod model;
pub mod system;
mod test_client;

pub use test_client::{Error, TestClient};
