use agent_utils::base64_decode_write_file;
use flate2::read::GzDecoder;
use k8s_openapi::api::core::v1::Secret;
use kube::config::Kubeconfig;
use kube::{Api, Config};
use log::{debug, info};
use reqwest::IntoUrl;
use resource_agent::provider::{IntoProviderError, ProviderError, ProviderResult, Resources};
use serde::Deserialize;
use std::convert::TryFrom;
use std::env;
use std::fmt::Display;
use std::path::PathBuf;

/// Write out and check CAPI management cluster is accessible and valid
pub async fn write_validate_mgmt_kubeconfig(
    mgmt_cluster_kubeconfig_base64: &str,
    mgmt_kubeconfig_path: &str,
    resources: &Resources,
) -> ProviderResult<kube::client::Client> {
    debug!("Decoding and writing out kubeconfig for the CAPI management cluster");
    base64_decode_write_file(mgmt_cluster_kubeconfig_base64, mgmt_kubeconfig_path)
        .await
        .context(
            resources,
            "Failed to write out kubeconfig for the CAPI management cluster",
        )?;
    let mgmt_kubeconfig = Kubeconfig::read_from(mgmt_kubeconfig_path)
        .context(resources, "Unable to read kubeconfig")?;
    let mgmt_config =
        Config::from_custom_kubeconfig(mgmt_kubeconfig.to_owned(), &Default::default())
            .await
            .context(resources, "Unable load kubeconfig")?;
    kube::client::Client::try_from(mgmt_config)
        .context(resources, "Unable create K8s client from kubeconfig")
}

/// Retrieve the kubeconfig for the K8s workload cluster from the CAPI mgmt cluster
pub async fn retrieve_workload_cluster_kubeconfig(
    mgmt_k8s_client: kube::client::Client,
    cluster_name: &str,
    resources: &Resources,
) -> ProviderResult<String> {
    let k8s_secrets: Api<Secret> = Api::namespaced(mgmt_k8s_client, "eksa-system");
    let kubeconfig_secret = k8s_secrets
        .get(&format!("{}-kubeconfig", cluster_name))
        .await
        .context(
            resources,
            format!(
                "K8s cluster '{}' does not exist in CAPI mgmt cluster",
                cluster_name
            ),
        )?;
    let encoded_kubeconfig = kubeconfig_secret
        .data
        .context(resources, "Missing kubeconfig secret")?
        .get("value")
        .context(resources, "Missing base64-encoded kubeconfig secret value")?
        .to_owned();
    Ok(serde_json::to_string(&encoded_kubeconfig)
        .context(
            resources,
            "Unable to serialize kubeconfig secret ByteString to String",
        )?
        .trim_matches('"')
        .to_string())
}

pub async fn get_eks_a_archive_url<S>(
    eks_a_release_manifest_url: S,
    resources: &Resources,
) -> ProviderResult<String>
where
    S: AsRef<str> + IntoUrl + Display,
{
    let manifest = reqwest::get(eks_a_release_manifest_url.to_string())
        .await
        .context(
            resources,
            format!(
                "Unable to request EKS-A release manifest '{}'",
                eks_a_release_manifest_url
            ),
        )?
        .text()
        .await
        .context(
            resources,
            format!(
                "Unable to retrieve EKS-A release manifest at '{}'",
                eks_a_release_manifest_url
            ),
        )?;
    let deserialized_manifest = serde_yaml::Deserializer::from_str(&manifest)
        .map(|config| {
            serde_yaml::Value::deserialize(config)
                .context(resources, "Unable to deserialize eksa config file")
        })
        .collect::<ProviderResult<Vec<_>>>()?;

    let latest_release_version = deserialized_manifest
        .iter()
        .find(|config| {
            config.get("kind") == Some(&serde_yaml::Value::String("Release".to_string()))
        })
        .and_then(|release| release.get("spec"))
        .and_then(|spec| spec.get("latestVersion"))
        .and_then(|ver| ver.as_str())
        .context(resources, "Unable to get latest version for EKS-A")?;

    let arch = match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        rest => rest,
    };
    deserialized_manifest
        .iter()
        .find(|manifests| {
            manifests.get("kind") == Some(&serde_yaml::Value::String("Release".to_string()))
        })
        .and_then(|manifest| manifest.get("spec"))
        .and_then(|spec| spec.get("releases"))
        .and_then(|list| list.as_sequence())
        .and_then(|releases| {
            releases.iter().find(|release| {
                release.get("version")
                    == Some(&serde_yaml::Value::String(
                        latest_release_version.to_string(),
                    ))
            })
        })
        .and_then(|release| release.get("eksACLI"))
        .and_then(|list| list.get(env::consts::OS.to_ascii_lowercase()))
        .and_then(|binaries| binaries.get(arch.to_ascii_lowercase()))
        .and_then(|binaries| binaries.get("uri"))
        .and_then(|uri| uri.as_str())
        .context(
            resources,
            format!(
                "Unable to get the URL for the latest EKS-A version ({})",
                latest_release_version
            ),
        )
        .map(|s| s.to_string())
}

pub async fn fetch_eks_a_binary(
    archive_url: String,
    dest: PathBuf,
    resources: &Resources,
) -> ProviderResult<()> {
    if !archive_url.ends_with("tar.gz") {
        return Err(ProviderError::new_with_context(
            resources,
            format!(
                "EKS-A binary archive at '{}' is not tar.gz compressed",
                archive_url
            ),
        ));
    }
    let tar_bytes = reqwest::get(&archive_url)
        .await
        .context(
            resources,
            format!("Unable to request binary at '{}'", archive_url),
        )?
        .bytes()
        .await
        .context(resources, "Unable to retrieve binary archive bytes.")?;

    // Decompress tar.gz archive
    let decoder = GzDecoder::new(&tar_bytes[..]);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(dest)
        .context(resources, "Failed to unpack EKS-A binary archive.")
}

const USR_LOCAL_BIN: &str = "/usr/local/bin/";
const DEFAULT_EKS_A_RELEASE_MANIFEST: &str =
    "https://anywhere-assets.eks.amazonaws.com/releases/eks-a/manifest.yaml";

pub async fn install_eks_a_binary(
    eks_a_release_manifest_url: &Option<String>,
    resources: &Resources,
) -> ProviderResult<()> {
    let eks_a_release_manifest_url = eks_a_release_manifest_url
        .to_owned()
        .unwrap_or(DEFAULT_EKS_A_RELEASE_MANIFEST.to_string());
    info!(
        "Using EKS-A release manifest '{}'",
        eks_a_release_manifest_url
    );
    let eks_a_archive_url = get_eks_a_archive_url(eks_a_release_manifest_url, resources).await?;
    info!("Fetching EKS-A binary archive from '{}'", eks_a_archive_url);
    fetch_eks_a_binary(eks_a_archive_url, PathBuf::from(USR_LOCAL_BIN), resources).await
}
