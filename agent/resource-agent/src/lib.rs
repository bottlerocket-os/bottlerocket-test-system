/*!

The `resource-agent` library allows you do design custom resource providers for your TestSys Tests.
You do this by implementing the [`Create`] and [`Destroy`] traits, then handing these to an
[`Agent`] object, which you then package as a binary in a container to run in the cluster.

!*/

mod agent;
mod bootstrap;
pub mod clients;
pub mod error;
pub mod provider;

pub use agent::{Agent, Types};
pub use bootstrap::BootstrapData;
pub use testsys_model::{Configuration, ResourceAction};
