use model::Configuration;
use serde::{Deserialize, Serialize};

pub const SONOBUOY_AWS_SECRET_NAME: &str = "aws-credentials";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct ClusterInfo {
    pub name: String,
    pub region: String,
    pub endpoint: String,
    pub certificate: String,
    pub public_subnet_ids: Vec<String>,
    pub private_subnet_ids: Vec<String>,
    pub nodegroup_sg: Vec<String>,
    pub controlplane_sg: Vec<String>,
    pub clustershared_sg: Vec<String>,
    pub iam_instance_profile_arn: String,
}

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
