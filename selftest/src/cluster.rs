use anyhow::{format_err, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    Api, Client, Config,
};
use model::constants::{LABEL_COMPONENT, LABEL_TEST_NAME, NAMESPACE};
use std::convert::TryInto;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const KUBECONFIG_FILENAME: &str = "kubeconfig.yaml";

/// Represents a `kind` cluster. The `Drop` trait is implemented deleting the `kind` cluster when it
/// goes out of scope.
#[derive(Debug)]
pub struct Cluster {
    name: String,
    kubeconfig_dir: TempDir,
}

impl Cluster {
    /// Creates a `Cluster` while initializing a kind cluster. If a cluster named `cluster_name`
    ///  already exists, it will be deleted.
    pub fn new(cluster_name: &str) -> Result<Cluster> {
        let kubeconfig_dir = TempDir::new()?;
        Self::delete_kind_cluster(cluster_name)?;
        Self::create_kind_cluster(
            cluster_name,
            &kubeconfig_dir.path().join(KUBECONFIG_FILENAME),
        )?;
        Ok(Self {
            name: cluster_name.into(),
            kubeconfig_dir,
        })
    }

    /// Returns the path to the kubeconfig file in the `TempDir` created for the cluster.
    pub fn kubeconfig(&self) -> PathBuf {
        self.kubeconfig_dir.path().join(KUBECONFIG_FILENAME)
    }

    /// Uses `kind load` to load an image from the machine to the kind cluster.
    pub fn load_image_to_cluster(&self, image_name: &str) -> Result<()> {
        use std::process::Command;
        let output = Command::new("kind")
            .arg("load")
            .arg("docker-image")
            .arg(image_name)
            .arg("--name")
            .arg(&self.name)
            .output()?;
        if !output.status.success() {
            return Err(format_err!(
                "'kind load docker-image failed' with exit status '{}'\n\n{}\n\n{}",
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(())
    }

    /// Create the k8s client for the cluster.
    pub async fn k8s_client(&self) -> Result<Client> {
        let kubeconfig = Kubeconfig::read_from(self.kubeconfig())?;
        let config =
            Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default()).await?;
        Ok(config.try_into()?)
    }

    /// Returns `true` if the controller is in the running state.
    pub async fn is_controller_running(&self) -> Result<bool> {
        let client = self.k8s_client().await?;
        let pod_api = Api::<Pod>::namespaced(client, NAMESPACE);
        let pods = pod_api
            .list(&ListParams {
                label_selector: Some(format!("{}=controller", LABEL_COMPONENT)),
                ..Default::default()
            })
            .await?;
        for pod in pods {
            if pod
                .status
                .unwrap_or_default()
                .phase
                .clone()
                .unwrap_or_default()
                == "Running"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns `true` if the `Test` named `test_name` is in the running state.
    pub async fn is_test_running(&self, test_name: &str) -> Result<bool> {
        let client = self.k8s_client().await?;
        let pod_api = Api::<Pod>::namespaced(client, NAMESPACE);
        let pods = pod_api
            .list(&ListParams {
                label_selector: Some(format!("{}={}", LABEL_TEST_NAME, test_name)),
                ..Default::default()
            })
            .await?;
        for pod in pods {
            if pod
                .status
                .unwrap_or_default()
                .phase
                .clone()
                .unwrap_or_default()
                == "Running"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn create_kind_cluster(name: &str, kubeconfig: &Path) -> Result<()> {
        use std::process::Command;
        let output = Command::new("kind")
            .arg("--kubeconfig")
            .arg(kubeconfig.to_str().ok_or_else(|| {
                format_err!(
                    "non utf-8 path '{}'",
                    kubeconfig.join(KUBECONFIG_FILENAME).to_string_lossy()
                )
            })?)
            .arg("create")
            .arg("cluster")
            .arg("--name")
            .arg(name)
            .output()?;
        if !output.status.success() {
            return Err(format_err!(
                "'kind create cluster failed' with exit status '{}'\n\n{}\n\n{}",
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(())
    }

    fn delete_kind_cluster(name: &str) -> Result<()> {
        use std::process::Command;
        let output = Command::new("kind")
            .arg("delete")
            .arg("cluster")
            .arg("--name")
            .arg(name)
            .output()?;
        if !output.status.success() {
            return Err(format_err!(
                "'kind delete cluster' failed with exit status '{}'\n\n{}\n\n{}",
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(())
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        if let Err(e) = Self::delete_kind_cluster(&self.name) {
            eprintln!("unable to delete kind cluster '{}': {}", self.name, e)
        }
    }
}
