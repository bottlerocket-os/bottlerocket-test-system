use model::Configuration;
use serde::{Deserialize, Serialize};

pub const SONOBUOY_TEST_RESULTS_LOCATION: &str = "testsys_sonobuoy_results.tar.gz";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SonobuoyConfig {
    // FIXME: need a better way of passing test cluster information
    pub kubeconfig_base64: String,
    pub plugin: String,
    pub mode: String,
    pub kubernetes_version: Option<String>,
    pub kube_conformance_image: Option<String>,
}

impl Configuration for SonobuoyConfig {}
