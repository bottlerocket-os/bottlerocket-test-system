/*!

`bottlerocket-agents` is a collection of test agent and resource agent implementations used to test
Bottlerocket instances.
This `lib.rs` provides code that is used by multiple agent binaries or used by the `testsys` CLI.

!*/

pub mod constants;
pub mod error;
pub mod sonobuoy;
pub mod tuf;
pub mod vsphere;
