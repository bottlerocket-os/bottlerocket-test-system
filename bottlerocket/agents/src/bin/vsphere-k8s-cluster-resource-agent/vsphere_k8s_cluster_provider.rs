use agent_utils::base64_decode_write_file;
use bottlerocket_agents::clusters::{
    retrieve_workload_cluster_kubeconfig, write_validate_mgmt_kubeconfig,
};
use bottlerocket_agents::constants::TEST_CLUSTER_KUBECONFIG_PATH;
use bottlerocket_agents::is_cluster_creation_required;
use bottlerocket_agents::tuf::{download_target, tuf_repo_urls};
use bottlerocket_agents::vsphere::vsphere_credentials;
use bottlerocket_types::agent_config::{
    CreationPolicy, VSphereK8sClusterConfig, VSPHERE_CREDENTIALS_SECRET_NAME,
};
use k8s_openapi::api::core::v1::Node;
use kube::api::ListParams;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Config};
use log::{debug, info};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{env, fs};
use testsys_model::{Configuration, SecretName};

const WORKING_DIR: &str = "/local/eksa-work";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMemo {
    /// In this resource we put some traces here that describe what our provider is doing.
    pub current_status: String,

    /// The name of the cluster we created.
    pub cluster_name: Option<String>,

    /// Whether the agent was instructed to create the cluster or not.
    pub creation_policy: Option<CreationPolicy>,

    /// Base64 encoded clusterspec for the workload cluster
    pub encoded_clusterspec: String,

    /// Name of the VM template for the control plane VMs
    pub vm_template: String,

    /// The name of the secret containing vCenter credentials.
    pub vcenter_secret_name: Option<SecretName>,
}

impl Configuration for ProductionMemo {}

/// Once we have fulfilled the `Create` request, we return information about the cluster we've created
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedVSphereK8sCluster {
    /// The name of the cluster.
    pub cluster_name: String,

    /// The cluster server endpoint.
    pub endpoint: String,

    /// Base64 encoded Kubeconfig for the vSphere K8s cluster
    pub encoded_kubeconfig: String,
}

impl Configuration for CreatedVSphereK8sCluster {}

pub struct VSphereK8sClusterCreator {}

#[async_trait::async_trait]
impl Create for VSphereK8sClusterCreator {
    type Config = VSphereK8sClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedVSphereK8sCluster;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Unknown, "Unable to get info from info client")?;
        // Keep track of the state of resources
        let mut resources = Resources::Clear;

        info!("Getting vSphere secret");
        memo.current_status = "Getting vSphere secret".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Get vSphere credentials to authenticate to vCenter via govmomi
        let secret_name = spec
            .secrets
            .get(VSPHERE_CREDENTIALS_SECRET_NAME)
            .context(resources, "Unable to fetch vSphere credentials")?;
        vsphere_credentials(client, secret_name, &resources).await?;
        memo.vcenter_secret_name = Some(secret_name.clone());

        info!("Creating working directory");
        memo.current_status = "Creating working directory".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Set current directory to somewhere other than '/' so eksctl-anywhere won't try to mount
        // it in a container.
        fs::create_dir_all(WORKING_DIR).context(
            resources,
            format!("Failed to create working directory '{}'", WORKING_DIR),
        )?;
        env::set_current_dir(Path::new(WORKING_DIR)).context(
            resources,
            format!("Failed to change current directory to {}", WORKING_DIR),
        )?;

        info!("Checking existing cluster");
        memo.current_status = "Checking existing cluster".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Check whether cluster creation is necessary
        let (do_create, message) = is_vsphere_k8s_cluster_creation_required(
            &spec.configuration,
            &spec.configuration.creation_policy.unwrap_or_default(),
        )
        .await?;
        memo.current_status = message;
        info!("{}", memo.current_status);
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let mgmt_kubeconfig_path = format!("{}/mgmt.kubeconfig", WORKING_DIR);
        let encoded_kubeconfig = if do_create {
            info!("Creating cluster");
            memo.current_status = "Creating cluster".to_string();
            client
                .send_info(memo.clone())
                .await
                .context(resources, "Error sending cluster creation message")?;
            let mgmt_k8s_client = write_validate_mgmt_kubeconfig(
                &spec.configuration.mgmt_cluster_kubeconfig_base64,
                &mgmt_kubeconfig_path,
                &resources,
            )
            .await?;
            create_vsphere_k8s_cluster(
                &spec.configuration,
                &mgmt_kubeconfig_path,
                &mut resources,
                &mut memo,
            )
            .await?;
            retrieve_workload_cluster_kubeconfig(
                mgmt_k8s_client,
                &spec.configuration.name,
                &resources,
            )
            .await?
        } else {
            spec.configuration.kubeconfig_base64.to_owned().context(
                resources,
                "Kubeconfig for existing vSphere K8s cluster missing",
            )?
        };

        info!("Cluster created");
        // We are done, set our custom status to say so.
        memo.current_status = "Cluster created".into();

        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending final creation message")?;

        Ok(CreatedVSphereK8sCluster {
            cluster_name: spec.configuration.name,
            endpoint: spec.configuration.control_plane_endpoint_ip,
            encoded_kubeconfig,
        })
    }
}

async fn is_vsphere_k8s_cluster_creation_required(
    config: &VSphereK8sClusterConfig,
    creation_policy: &CreationPolicy,
) -> ProviderResult<(bool, String)> {
    let cluster_exists = does_cluster_exist(config).await?;
    is_cluster_creation_required(&cluster_exists, &config.name, creation_policy).await
}

async fn does_cluster_exist(config: &VSphereK8sClusterConfig) -> ProviderResult<bool> {
    if let Some(kubeconfig_base64) = &config.kubeconfig_base64 {
        base64_decode_write_file(kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await
            .context(
                Resources::Clear,
                "Failed to write out kubeconfig for vSphere K8s cluster",
            )?;
        let kubeconfig = Kubeconfig::read_from(TEST_CLUSTER_KUBECONFIG_PATH)
            .context(Resources::Clear, "Unable to read kubeconfig")?;
        let config =
            Config::from_custom_kubeconfig(kubeconfig.to_owned(), &KubeConfigOptions::default())
                .await
                .context(Resources::Clear, "Unable load kubeconfig")?;
        let k8s_client = kube::client::Client::try_from(config)
            .context(Resources::Clear, "Unable create K8s client from kubeconfig")?;
        let k8s_nodes: Api<Node> = Api::all(k8s_client);
        let node_list = k8s_nodes.list(&ListParams::default()).await;
        if node_list.is_ok() {
            // If we can query nodes from the cluster, then we've validate that the kubeconfig is valid
            return Ok(true);
        }
    }
    Ok(false)
}

async fn create_vsphere_k8s_cluster(
    config: &VSphereK8sClusterConfig,
    mgmt_kubeconfig_path: &str,
    resources: &mut Resources,
    memo: &mut ProductionMemo,
) -> ProviderResult<()> {
    let (metadata_url, targets_url) = tuf_repo_urls(&config.tuf_repo, resources)?;

    // Set up environment variables for govc cli
    set_govc_env_vars(config);

    // Retrieve the OVA file
    let ova_name = config.ova_name.to_owned();
    info!("Downloading OVA '{}'", &config.ova_name);
    let outdir = Path::new("/local/");
    tokio::task::spawn_blocking(move || -> ProviderResult<()> {
        download_target(
            Resources::Clear,
            &metadata_url,
            &targets_url,
            outdir,
            &ova_name,
        )
    })
    .await
    .context(*resources, "Failed to join threads")??;

    // Update the import spec for the OVA
    let import_spec_output = Command::new("govc")
        .arg("import.spec")
        .arg(format!("/local/{}", &config.ova_name))
        .output()
        .context(*resources, "Failed to start govc")?;
    let mut import_spec: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&import_spec_output.stdout))
            .context(*resources, "Failed to deserialize govc import.spec output")?;
    let network_mappings = import_spec
        .get_mut("NetworkMapping")
        .and_then(|network_mapping| network_mapping.as_array_mut())
        .context(*resources, "Missing network mappings for VM network")?;
    network_mappings.clear();
    network_mappings.push(json!({"Name": "VM Network", "Network": &config.vcenter_network}));
    *import_spec
        .get_mut("MarkAsTemplate")
        .context(*resources, "Missing 'MarkAsTemplate' in Import spec")? = json!(true);
    let import_spec_file = File::create("/local/ova.importspec")
        .context(*resources, "Failed to create ova import spec file")?;
    serde_json::to_writer_pretty(&import_spec_file, &import_spec)
        .context(*resources, "Failed to write out OVA import spec file")?;

    // Import OVA and create a template out of it
    info!("Importing OVA and creating a VM template out of it");
    let vm_template_name = format!("{}-eksa-vmtemplate", &config.name);
    let import_ova_output = Command::new("govc")
        .arg("import.ova")
        .arg("-options=/local/ova.importspec")
        .arg(format!("-name={}", vm_template_name))
        .arg(format!("/local/{}", &config.ova_name))
        .output()
        .context(*resources, "Failed to launch govc process")?;
    *resources = Resources::Unknown;
    if !import_ova_output.status.success() {
        return Err(ProviderError::new_with_context(
            *resources,
            format!(
                "Failed to import OVA: {}",
                String::from_utf8_lossy(&import_ova_output.stderr)
            ),
        ));
    }
    *resources = Resources::Remaining;
    memo.vm_template = vm_template_name.to_owned();

    // EKS-A expects tags on the VM template
    let vm_full_path = format!(
        "{}/{}",
        config.vcenter_workload_folder.trim_end_matches('/'),
        vm_template_name
    );
    info!("Tagging VM template");
    // Create the 'os' tag category
    Command::new("govc")
        .args(["tags.category.create", "-m", "-t", "VirtualMachine", "os"])
        .output()
        .context(*resources, "Failed to launch govc process")?;
    // Create the 'os:bottlerocket' tag
    // We don't check the command status since we just need the tag to exist and we don't care
    // if it already exists. If it failed to create the tag, we'll find out when we try to tag.
    Command::new("govc")
        .args(["tags.create", "-c", "os", "os:bottlerocket"])
        .output()
        .context(*resources, "Failed to launch govc process")?;
    // Tag the VM template with the OS tag
    let tag_attach_output = Command::new("govc")
        .args(["tags.attach", "os:bottlerocket"])
        .arg(&vm_full_path)
        .output()
        .context(*resources, "Failed to launch govc process")?;
    if !tag_attach_output.status.success() {
        return Err(ProviderError::new_with_context(
            *resources,
            format!(
                "Failed to tag VM template '{}': {}",
                vm_template_name,
                String::from_utf8_lossy(&tag_attach_output.stderr)
            ),
        ));
    }
    // Create the 'eksdRelease' tag category
    Command::new("govc")
        .args([
            "tags.category.create",
            "-m",
            "-t",
            "VirtualMachine",
            "eksdRelease",
        ])
        .output()
        .context(*resources, "Failed to launch govc process")?;
    // Create the 'eksRelease' tag, e.g 'eksRelease:kubernetes-1-23-eks-6'
    // EKS-A needs to make sure the VM template is tagged with a compatible EKS-D release number.
    // Finding out which EKS-D release we're using in a given Bottlerocket OVA is hard and
    // we really really don't care which EKS-D release for a given K8s cluster version we're
    // testing here, we're just going to tag the VM template with a range of release numbers.
    // The tag just has to exist for EKS-A to be happy. We choose 50 since it's highly unlikely
    // a given K8s minor version will have 50 EKS-D releases.
    let k8s_ver_str = config
        .version
        .context(*resources, "K8s version missing from configuration")?
        .major_minor_without_v()
        .replace('.', "-");
    for release_number in 1..50 {
        let eksd_ver_str = format!("kubernetes-{}-eks-{}", k8s_ver_str, release_number);
        // Same reasons as above, we don't need to check the status here
        Command::new("govc")
            .args(["tags.create", "-c", "eksdRelease"])
            .arg(&format!("eksdRelease:{}", eksd_ver_str))
            .output()
            .context(*resources, "Failed to launch govc process")?;
        // Tag the VM template with the EKSD tag
        let tag_attach_output = Command::new("govc")
            .args(["tags.attach", &format!("eksdRelease:{}", eksd_ver_str)])
            .arg(&vm_full_path)
            .output()
            .context(*resources, "Failed to launch govc process")?;
        if !tag_attach_output.status.success() {
            return Err(ProviderError::new_with_context(
                *resources,
                format!(
                    "Failed to tag VM template '{}': {}",
                    vm_template_name,
                    String::from_utf8_lossy(&tag_attach_output.stderr)
                ),
            ));
        }
    }

    // Set the feature flag for potential new EKS version support in EKS-A
    env::set_var(
        format!(
            "K8S_{}_SUPPORT",
            config
                .version
                .context(*resources, "K8s version missing from configuration")?
                .major_minor_without_v()
                .replace('.', "_")
        ),
        "true",
    );

    // Set up EKS-A vSphere cluster spec file
    let mgmt_kubeconfig = Kubeconfig::read_from(mgmt_kubeconfig_path)
        .context(*resources, "Unable to read kubeconfig")?;
    let mgmt_cluster_name = &mgmt_kubeconfig
        .clusters
        .first()
        .context(*resources, "Missing clusters in Kubeconfig")?
        .name;
    let clusterspec_path = format!("{}/vsphere-k8s-clusterspec.yaml", WORKING_DIR);
    write_vsphere_clusterspec(
        config,
        vm_template_name,
        mgmt_cluster_name.to_string(),
        resources,
        &clusterspec_path,
        memo,
    )
    .await?;

    // Call eksctl-anywhere to create cluster with existing mgmt cluster in vsphere
    let status = Command::new("eksctl")
        .args(["anywhere", "create", "cluster"])
        .args(["--kubeconfig", mgmt_kubeconfig_path])
        .args(["-f", &clusterspec_path])
        .args(["-v", "4"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context(*resources, "Failed to launch eksctl process")?;
    if !status.success() {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!(
                "Failed to create EKS-A vSphere cluster with status code {}",
                status
            ),
        ));
    }

    // Scale the default NodeGroup's MachineDeployment to '0' since we're going to launch fresh
    // instances during tests.
    // FIXME: When Cluster API has a Rust library for its CRDs, switch to using that instead of 'kubectl'
    info!("Scaling default NodeGroup machinedeployments replicas to 0");
    let default_md = format!("machinedeployments/{}-md-0", config.name);
    let status = Command::new("kubectl")
        .args(["--kubeconfig", mgmt_kubeconfig_path])
        .args(["scale", &default_md, "--replicas=0"])
        .args(["-n", "eksa-system"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context(*resources, "Failed to launch kubectl process")?;
    if !status.success() {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!(
                "Failed to scale # of machines in NodeGroup '{}'",
                default_md
            ),
        ));
    }

    Ok(())
}

async fn write_vsphere_clusterspec(
    config: &VSphereK8sClusterConfig,
    vm_template_name: String,
    mgmt_cluster_name: String,
    resources: &Resources,
    clusterspec_path: &str,
    memo: &mut ProductionMemo,
) -> ProviderResult<()> {
    let cluster_name = config.name.to_owned();
    let cluster_version = config.version.unwrap_or_default().major_minor_without_v();
    let endpoint_ip = config.control_plane_endpoint_ip.to_owned();
    let vcenter_datacenter = config.vcenter_datacenter.to_owned();
    let vcenter_network = config.vcenter_network.to_owned();
    let vcenter_host_url = config.vcenter_host_url.to_owned();
    let vcenter_datastore = config.vcenter_datastore.to_owned();
    let vcenter_workload_folder = config.vcenter_workload_folder.to_owned();
    let vcenter_resource_pool = config.vcenter_resource_pool.to_owned();

    let about_cert_output = Command::new("govc")
        .args(["about.cert", "-k", "-json"])
        .output()
        .context(*resources, "Failed to launch govc process")?;
    if !about_cert_output.status.success() {
        return Err(ProviderError::new_with_context(
            resources,
            format!(
                "Failed to query vCenter server certificate for '{}': {}",
                vcenter_host_url,
                String::from_utf8_lossy(&about_cert_output.stderr)
            ),
        ));
    }
    let about_cert: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&about_cert_output.stdout))
            .context(*resources, "Failed to deserialize 'govc about.cert' output")?;
    let vcenter_thumbprint = about_cert
        .get("ThumbprintSHA1")
        .and_then(|t| t.as_str())
        .context(
            resources,
            format!(
                "Failed to get the SHA1 thumbprint from vCenter server certificate for '{}'",
                vcenter_host_url
            ),
        )?;

    let clusterspec = format!(
        r###"apiVersion: anywhere.eks.amazonaws.com/v1alpha1
kind: Cluster
metadata:
  name: {cluster_name}
spec:
  managementCluster:
    name: {mgmt_cluster_name}
  clusterNetwork:
    cniConfig:
      cilium: {{}}
    pods:
      cidrBlocks:
      - 192.168.0.0/16
    services:
      cidrBlocks:
      - 10.96.0.0/12
  controlPlaneConfiguration:
    count: 2
    endpoint:
      host: "{endpoint_ip}"
    machineGroupRef:
      kind: VSphereMachineConfig
      name: {cluster_name}-node
  datacenterRef:
    kind: VSphereDatacenterConfig
    name: {cluster_name}
  externalEtcdConfiguration:
    count: 3
    machineGroupRef:
      kind: VSphereMachineConfig
      name: {cluster_name}-node
  kubernetesVersion: "{cluster_version}"
  workerNodeGroupConfigurations:
  - count: 1
    machineGroupRef:
      kind: VSphereMachineConfig
      name: {cluster_name}-node
    name: md-0
---
apiVersion: anywhere.eks.amazonaws.com/v1alpha1
kind: VSphereDatacenterConfig
metadata:
  name: {cluster_name}
spec:
  datacenter: "{vcenter_datacenter}"
  insecure: false
  network: "{vcenter_network}"
  server: "{vcenter_host_url}"
  thumbprint: "{vcenter_thumbprint}"
---
apiVersion: anywhere.eks.amazonaws.com/v1alpha1
kind: VSphereMachineConfig
metadata:
  name: {cluster_name}-node
spec:
  diskGiB: 25
  memoryMiB: 8192
  numCPUs: 2
  osFamily: bottlerocket
  datastore: "{vcenter_datastore}"
  folder: "{vcenter_workload_folder}"
  template: "{vm_template_name}"
  resourcePool: "{vcenter_resource_pool}"
"###
    );
    debug!("{}", &clusterspec);
    memo.encoded_clusterspec = base64::encode(&clusterspec);
    fs::write(clusterspec_path, clusterspec).context(
        resources,
        format!(
            "Failed to write vSphere cluster spec to '{}'",
            clusterspec_path
        ),
    )
}

/// This is the object that will destroy vSphere K8s clusters.
pub struct VSphereK8sClusterDestroyer {}

#[async_trait::async_trait]
impl Destroy for VSphereK8sClusterDestroyer {
    type Config = VSphereK8sClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedVSphereK8sCluster;

    async fn destroy<I>(
        &self,
        spec: Option<Spec<Self::Config>>,
        resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client.get_info().await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                Resources::Unknown,
                "Unable to get info from client",
                e,
            )
        })?;
        let resources = if memo.cluster_name.is_some() || !memo.vm_template.is_empty() {
            Resources::Remaining
        } else {
            Resources::Clear
        };
        let spec = spec.context(resources, "Missing vSphere K8s cluster resource agent spec")?;
        let resource = resource.context(resources, "Missing created resource information")?;

        // Get vSphere credentials to authenticate to vCenter via govmomi
        let secret_name = spec
            .secrets
            .get(VSPHERE_CREDENTIALS_SECRET_NAME)
            .context(resources, "Unable to fetch vSphere credentials")?;
        vsphere_credentials(client, secret_name, &resources).await?;

        // Set up environment variables for govc cli
        set_govc_env_vars(&spec.configuration);

        // Set current directory to somewhere other than '/' so eksctl-anywhere won't try to mount
        // it in a container.
        fs::create_dir_all(WORKING_DIR).context(
            resources,
            format!("Failed to create working directory '{}'", WORKING_DIR),
        )?;
        env::set_current_dir(Path::new(WORKING_DIR)).context(
            resources,
            format!("Failed to change current directory to {}", WORKING_DIR),
        )?;

        // Delete the VM template
        let vm_destroy_output = Command::new("govc")
            .arg("vm.destroy")
            .arg(&memo.vm_template)
            .output()
            .context(resources, "Failed to start govc")?;
        if !vm_destroy_output.status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to VM template '{}': {}",
                    &memo.vm_template,
                    String::from_utf8_lossy(&vm_destroy_output.stderr)
                ),
            ));
        }
        memo.vm_template = "".to_string();

        let mgmt_kubeconfig_path = format!("{}/mgmt.kubeconfig", WORKING_DIR);
        debug!("Decoding and writing out kubeconfig for the CAPI management cluster");
        base64_decode_write_file(
            &spec.configuration.mgmt_cluster_kubeconfig_base64,
            &mgmt_kubeconfig_path,
        )
        .await
        .context(
            resources,
            "Failed to write out kubeconfig for the CAPI management cluster",
        )?;

        // For cluster deletion, EKS-A needs the workload cluster's kubeconfig at
        // './${CLUSTER_NAME}/${CLUSTER_NAME}-eks-a-cluster.kubeconfig'
        let cluster_dir = format!("{}/{}/", WORKING_DIR, &spec.configuration.name);
        fs::create_dir_all(&cluster_dir).context(
            resources,
            format!(
                "Failed to create EKS-A cluster directory '{}'",
                &cluster_dir
            ),
        )?;
        debug!("Decoding and writing out kubeconfig for workload cluster");
        base64_decode_write_file(
            &resource.encoded_kubeconfig,
            &format!(
                "{}/{}-eks-a-cluster.kubeconfig",
                &cluster_dir, &spec.configuration.name
            ),
        )
        .await
        .context(
            resources,
            "Failed to write out kubeconfig for workload cluster",
        )?;

        debug!("Decoding and writing out EKS-A clusterspec");
        base64_decode_write_file(
            &memo.encoded_clusterspec,
            &format!(
                "{}/{}-eks-a-cluster.yaml",
                &cluster_dir, &spec.configuration.name
            ),
        )
        .await
        .context(resources, "Failed to write out EKS-A clusterspec")?;

        // Delete the cluster with EKS-A
        info!(
            "Deleting vSphere K8s cluster '{}'",
            &spec.configuration.name
        );
        let status = Command::new("eksctl")
            .args(["anywhere", "delete", "cluster"])
            .args(["--kubeconfig", &mgmt_kubeconfig_path])
            .arg(spec.configuration.name)
            .args(["-v", "4"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context(resources, "Failed to run eksctl-anywhere delete command")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Orphaned,
                format!("Failed to delete cluster with status code {}", status),
            ));
        }
        memo.cluster_name = None;

        info!("vSphere K8s cluster deleted");
        memo.current_status = "Cluster deleted".into();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                Resources::Clear,
                "Error sending final destruction message",
                e,
            )
        })?;

        Ok(())
    }
}

pub fn set_govc_env_vars(config: &VSphereK8sClusterConfig) {
    env::set_var("GOVC_URL", &config.vcenter_host_url);
    env::set_var("GOVC_DATACENTER", &config.vcenter_datacenter);
    env::set_var("GOVC_DATASTORE", &config.vcenter_datastore);
    env::set_var("GOVC_NETWORK", &config.vcenter_network);
    env::set_var("GOVC_RESOURCE_POOL", &config.vcenter_resource_pool);
    env::set_var("GOVC_FOLDER", &config.vcenter_workload_folder);
}
