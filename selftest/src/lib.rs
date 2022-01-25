/*!

Provides utilities for testing the TestSys system using `kind` and `docker`.
We call this testing modality `selftest` to distinguish it from the `Test` CRDs and `TestSys Tests`.

!*/

pub mod cluster;
mod test_settings;

pub use cluster::Cluster;
