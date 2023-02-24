use agent_utils::base64_decode_write_file;
use k8s_openapi::api::core::v1::Secret;
use kube::config::Kubeconfig;
use kube::{Api, Config};
use log::debug;
use resource_agent::provider::{IntoProviderError, ProviderResult, Resources};
use std::convert::TryFrom;

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
