/*!

This library provides the Kubernetes custom resource definitions and their API clients.

!*/

#![deny(
    clippy::expect_used,
    clippy::get_unwrap,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::panicking_unwrap,
    clippy::unwrap_in_result,
    clippy::unwrap_used
)]

pub use agent::{Agent, SecretName, SecretType, TaskState};
pub use configuration::Configuration;
pub use crd_ext::CrdExt;
pub use error::{Error, Result};
use kube::ResourceExt;
pub use resource::{
    DestructionPolicy, ErrorResources, Resource, ResourceAction, ResourceError, ResourceSpec,
    ResourceStatus,
};
use serde::{Deserialize, Serialize};
pub use test::{
    AgentStatus, ControllerStatus, Outcome, Test, TestResults, TestSpec, TestStatus, TestUserState,
};

mod agent;
pub mod clients;
mod configuration;
pub mod constants;
mod crd_ext;
mod error;
mod resource;
mod schema_utils;
pub mod system;
mod test;
pub mod test_manager;

/// `CrdName` provides a way of determing which type of testsys object a name refers to.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum CrdName {
    Test(String),
    Resource(String),
}

impl CrdName {
    pub fn name(&self) -> &String {
        match self {
            CrdName::Test(name) => name,
            CrdName::Resource(name) => name,
        }
    }
}

/// `Crd` provides an interface to combine `Test` and `Resource` when actions can be performed on both.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Crd {
    Test(Test),
    Resource(Resource),
}

impl Crd {
    pub fn name(&self) -> Option<String> {
        match self {
            Self::Test(test) => test.metadata.name.to_owned(),
            Self::Resource(resource) => resource.metadata.name.to_owned(),
        }
    }
}

impl From<Crd> for CrdName {
    fn from(crd: Crd) -> Self {
        match crd {
            Crd::Test(test) => CrdName::Test(test.name()),
            Crd::Resource(resource) => CrdName::Resource(resource.name()),
        }
    }
}
