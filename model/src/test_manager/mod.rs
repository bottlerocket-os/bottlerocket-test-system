pub use delete::DeleteEvent;
pub use error::{Error, Result};
pub use manager::{read_manifest, TestManager};
use serde::{Deserialize, Serialize};
use serde_plain::derive_fromstr_from_deserialize;
pub use status::StatusSnapshot;
use std::collections::HashMap;

mod delete;
mod error;
mod install;
mod manager;
mod manager_impl;
mod status;

#[derive(Default, Debug, Clone)]
/// `SelectionParams` are used to select a group (or single) object from a testsys cluster. For any
/// of the filters, None is equivalent to all.
pub struct SelectionParams {
    /// Filter based on the type of the CRD
    pub crd_type: Option<CrdType>,
    /// Filter based on the crd labels
    pub labels: Option<String>,
    /// Filter based on the name of the CRD
    pub name: Option<String>,
    /// Filter based on the state of the CRD
    pub state: Option<CrdState>,
}

#[derive(Debug, Clone)]
/// Filter based on the type of the CRD
pub enum CrdType {
    Test,
    Resource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Filter based on the state of the CRD
pub enum CrdState {
    /// All `Test`s and `Resource`s that are not finished.
    NotFinished,
    /// All `Test`s and `Resource`s that are currently running.
    Running,
    /// All `Test`s and `Resource`s that are finished.
    Completed,
    /// All `Test`s that passed.
    Passed,
    /// All `Test`s that failed.
    Failed,
}

derive_fromstr_from_deserialize!(CrdState);

#[derive(Serialize)]
pub(crate) struct DockerConfigJson {
    auths: HashMap<String, DockerConfigAuth>,
}

#[derive(Serialize)]
struct DockerConfigAuth {
    auth: String,
}

impl DockerConfigJson {
    pub(crate) fn new(username: &str, password: &str, registry: &str) -> DockerConfigJson {
        let mut auths = HashMap::new();
        let auth = base64::encode(format!("{}:{}", username, password));
        auths.insert(registry.to_string(), DockerConfigAuth { auth });
        DockerConfigJson { auths }
    }
}

/// `ImageConfig` represents an image uri, and the name of a pull secret (if needed).
pub enum ImageConfig {
    WithCreds { image: String, secret: String },
    Image(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceState {
    Creation,
    Destruction,
}

derive_fromstr_from_deserialize!(ResourceState);

/// `StatusProgress` represents whether a `Test`'s `other_info` should be included or not.
#[derive(Debug)]
pub enum StatusProgress {
    WithTests,
    Resources,
}
