use crate::CrdName;
pub use delete::DeleteEvent;
pub use error::{Error, Result};
pub use manager::{read_manifest, TestManager};
use serde::{Deserialize, Serialize};
use serde_plain::derive_fromstr_from_deserialize;
pub use status::Status;
use std::collections::HashMap;

mod delete;
mod error;
mod install;
mod manager;
mod manager_impl;
mod status;

/// `SelectionParams` are used to select a group (or single) object from a testsys cluster.
pub enum SelectionParams {
    // TODO add field selectors (Think kube-rs `ListParams`)
    Label(String),
    Name(CrdName),
    All,
}

impl Default for SelectionParams {
    fn default() -> Self {
        Self::All
    }
}

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
