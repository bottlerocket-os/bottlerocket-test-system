use agent_utils::aws::aws_config;
use agent_utils::base64_decode_write_file;
use agent_utils::ssm::{create_ssm_activation, ensure_ssm_service_role, wait_for_ssm_ready};
use base64::engine::general_purpose::STANDARD as base64_engine;
use base64::Engine as _;
use bottlerocket_agents::constants::TEST_CLUSTER_KUBECONFIG_PATH;
use bottlerocket_agents::tuf::{download_target, tuf_repo_urls};
use bottlerocket_agents::userdata::{decode_to_string, merge_values};
use bottlerocket_agents::vsphere::vsphere_credentials;
use bottlerocket_types::agent_config::{
    CustomUserData, VSphereVmConfig, AWS_CREDENTIALS_SECRET_NAME, VSPHERE_CREDENTIALS_SECRET_NAME,
};
use k8s_openapi::api::core::v1::{Node, Service};
use kube::api::{DeleteParams, ListParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Config};
use log::{debug, info};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use testsys_model::{Configuration, SecretName};
use toml::Value;

/// The default number of VMs to spin up.
const DEFAULT_VM_COUNT: i32 = 2;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VSphereVM {
    /// Name of the created VSphere VM.
    name: String,

    /// Instance IDs of the created VSphere VM.
    instance_id: String,

    /// The public IP addresses of the vSphere worker node
    ip_address: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMemo {
    /// In this resource we put some traces here that describe what our provider is doing.
    pub current_status: String,

    /// Name of the VM template used to create the worker nodes
    pub vm_template: String,

    /// Instance IDs of all created VSphere VMs.
    pub vms: Vec<VSphereVM>,

    pub ssm_activation_id: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The name of the secret containing vCenter credentials.
    pub vcenter_secret_name: Option<SecretName>,

    /// The role that is being assumed.
    pub assume_role: Option<String>,
}

impl Configuration for ProductionMemo {}

/// Once we have fulfilled the `Create` request, we return information about the batch of VSphere VMs
/// we've created
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedVSphereVms {
    /// The instance IDs of all SSM-registered VMs
    pub instance_ids: HashSet<String>,
}

impl Configuration for CreatedVSphereVms {}

pub struct VMCreator {}

#[async_trait::async_trait]
impl Create for VMCreator {
    type Config = VSphereVmConfig;
    type Info = ProductionMemo;
    type Resource = CreatedVSphereVms;

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

        info!("Initializing agent");
        memo.current_status = "Initializing agent".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let (metadata_url, targets_url) = tuf_repo_urls(&spec.configuration.tuf_repo, &resources)?;

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role = spec.configuration.assume_role.clone();

        let shared_config = aws_config(
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &None,
            &None,
            false,
        )
        .await
        .context(Resources::Clear, "Error creating config")?;

        let ssm_client = aws_sdk_ssm::Client::new(&shared_config);
        let iam_client = aws_sdk_iam::Client::new(&shared_config);

        info!("Checking SSM service role");
        memo.current_status = "Checking SSM service role".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Ensure we have a SSM service role we can attach to the VMs
        ensure_ssm_service_role(&iam_client)
            .await
            .context(Resources::Clear, "Unable to check for SSM service role")?;

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

        info!("Setting GOVC env");
        memo.current_status = "Setting GOVC env".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Set up environment variables for govc cli
        set_govc_env_vars(&spec.configuration);

        info!("Writing kubeconfig");
        memo.current_status = "Writing kubeconfig".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let vsphere_cluster = spec.configuration.cluster.clone();
        debug!("Decoding and writing out kubeconfig for vSphere cluster");
        base64_decode_write_file(
            &vsphere_cluster.kubeconfig_base64,
            TEST_CLUSTER_KUBECONFIG_PATH,
        )
        .await
        .context(
            Resources::Clear,
            "Failed to write out kubeconfig for vSphere cluster",
        )?;
        let kubeconfig_arg = vec!["--kubeconfig", TEST_CLUSTER_KUBECONFIG_PATH];
        let kubeconfig = Kubeconfig::read_from(TEST_CLUSTER_KUBECONFIG_PATH)
            .context(resources, "Unable to read kubeconfig")?;
        let config =
            Config::from_custom_kubeconfig(kubeconfig.to_owned(), &KubeConfigOptions::default())
                .await
                .context(resources, "Unable load kubeconfig")?;

        info!("Creating K8s client");
        memo.current_status = "Creating K8s client".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let k8s_client = kube::client::Client::try_from(config)
            .context(resources, "Unable create K8s client from kubeconfig")?;

        info!("Downloading OVA");
        // Retrieve the OVA file
        memo.current_status = "Downloading OVA".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        let ova_name = spec.configuration.ova_name.to_owned();
        info!("Downloading OVA '{}'", &spec.configuration.ova_name);
        let outdir = Path::new("/local/");
        tokio::task::spawn_blocking(move || -> ProviderResult<()> {
            download_target(resources, &metadata_url, &targets_url, outdir, &ova_name)
        })
        .await
        .context(resources, "Failed to join threads")??;

        info!("Retrieving K8s cluster info");
        memo.current_status = "Retrieving K8s cluster info".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        // Get necessary information for Bottlerocket nodes to join the test cluster
        let k8s_services: Api<Service> = Api::namespaced(k8s_client, "kube-system");
        let list = k8s_services
            .list(&ListParams::default().labels("k8s-app=kube-dns"))
            .await
            .context(resources, "Failed to query for K8s cluster services")?;
        let cluster_dns_ip = list
            .items
            .first()
            .and_then(|item| item.spec.as_ref())
            .and_then(|spec| spec.cluster_ip.to_owned())
            .context(resources, "Missing Cluster DNS IP")?;
        debug!("Got cluster-dns-ip '{}'", &cluster_dns_ip);

        let cluster_certificate = &kubeconfig
            .clusters
            .first()
            .and_then(|cluster| cluster.cluster.as_ref())
            .and_then(|cluster| cluster.certificate_authority_data.as_ref())
            .context(resources, "Missing Cluster certificate authority data")?;
        debug!("Got certificate-authority-data '{}'", &cluster_certificate);

        info!("Creating bootstrap token");
        memo.current_status = "Creating bootstrap token".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        info!("Create a bootstrap token for node registrations");
        let token_create_output = Command::new("kubeadm")
            .args(&kubeconfig_arg)
            .arg("token")
            .arg("create")
            .output()
            .context(resources, "Failed to start kubeadm")?;
        let bootstrap_token = String::from_utf8_lossy(&token_create_output.stdout);
        let bootstrap_token = bootstrap_token.trim_end();

        info!("Updating import spec");
        memo.current_status = "Updating import spec".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        // Update the import spec for the OVA
        let import_spec_output = Command::new("govc")
            .arg("import.spec")
            .arg(format!("/local/{}", &spec.configuration.ova_name))
            .output()
            .context(resources, "Failed to start govc")?;
        let mut import_spec: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&import_spec_output.stdout))
                .context(resources, "Failed to deserialize govc import.spec output")?;
        let network_mappings = import_spec
            .get_mut("NetworkMapping")
            .and_then(|network_mapping| network_mapping.as_array_mut())
            .context(resources, "Missing network mappings for VM network")?;
        network_mappings.clear();
        network_mappings
            .push(json!({"Name": "VM Network", "Network": &spec.configuration.vcenter_network}));
        let import_spec_file = File::create("/local/ova.importspec")
            .context(resources, "Failed to create ova import spec file")?;
        serde_json::to_writer_pretty(&import_spec_file, &import_spec)
            .context(resources, "Failed to write out OVA import spec file")?;

        info!("Importing OVA");
        memo.current_status = "Importing OVA".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        // Import OVA and create a template out of it
        info!("Importing OVA and creating a VM template out of it");
        let vm_template_name = format!("{}-node-vmtemplate", vsphere_cluster.name);
        let import_ova_output = Command::new("govc")
            .arg("import.ova")
            .arg("-options=/local/ova.importspec")
            .arg(format!("-name={}", vm_template_name))
            .arg(format!("/local/{}", &spec.configuration.ova_name))
            .output()
            .context(resources, "Failed to start govc")?;
        resources = Resources::Unknown;
        if !import_ova_output.status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to import OVA: {}",
                    String::from_utf8_lossy(&import_ova_output.stderr)
                ),
            ));
        }

        info!("Creating template from OVA");
        memo.current_status = "Creating template from OVA".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        resources = Resources::Remaining;
        let markastemplate_output = Command::new("govc")
            .arg("vm.markastemplate")
            .arg(&vm_template_name)
            .output()
            .context(resources, "Failed to start govc")?;
        if !markastemplate_output.status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to mark VM as template: {}",
                    String::from_utf8_lossy(&markastemplate_output.stderr)
                ),
            ));
        }
        memo.vm_template = vm_template_name.to_owned();

        let vm_count = spec.configuration.vm_count.unwrap_or(DEFAULT_VM_COUNT);
        // Generate SSM activation codes and IDs
        let activation = create_ssm_activation(&vsphere_cluster.name, vm_count, &ssm_client)
            .await
            .context(resources, "Unable to create SSM activation")?;
        memo.ssm_activation_id = activation.0.to_owned();
        let control_host_ctr_userdata = json!({"ssm":{"activation-id": activation.0.to_string(), "activation-code":activation.1.to_string(),"region":"us-west-2"}});
        debug!(
            "Control container host container userdata: {}",
            control_host_ctr_userdata
        );
        let encoded_control_host_ctr_userdata =
            base64_engine.encode(control_host_ctr_userdata.to_string());

        let custom_user_data = spec.configuration.custom_user_data;

        // Base64 encode userdata
        let userdata = userdata(
            &vsphere_cluster.control_plane_endpoint_ip,
            &cluster_dns_ip,
            bootstrap_token,
            cluster_certificate,
            &encoded_control_host_ctr_userdata,
            &custom_user_data,
        )?;

        info!("Launching worker nodes");
        memo.current_status = "Launching worker nodes".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        info!("Launching {} Bottlerocket worker nodes", vm_count);
        for i in 0..vm_count {
            let node_name = format!("{}-node-{}", vsphere_cluster.name, i + 1);
            info!("Cloning VM for worker node '{}'", node_name);
            let vm_clone_output = Command::new("govc")
                .arg("vm.clone")
                .arg("-vm")
                .arg(&vm_template_name)
                .arg("-on=false")
                .arg(&node_name)
                .output()
                .context(resources, "Failed to start govc")?;
            if !vm_clone_output.status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!(
                        "Failed to clone VM from template: {}",
                        String::from_utf8_lossy(&vm_clone_output.stderr)
                    ),
                ));
            }
            // Inject encoded userdata
            let vm_change_output = Command::new("govc")
                .arg("vm.change")
                .arg("-vm")
                .arg(&node_name)
                .arg("-e")
                .arg(format!("guestinfo.userdata={}", userdata))
                .arg("-e")
                .arg("guestinfo.userdata.encoding=base64")
                .output()
                .context(resources, "Failed to start govc")?;
            if !vm_change_output.status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!(
                        "Failed to inject user-data for '{}': {}",
                        node_name,
                        String::from_utf8_lossy(&vm_change_output.stderr)
                    ),
                ));
            }
            info!("Powering on '{}'...", node_name);
            let vm_power_output = Command::new("govc")
                .arg("vm.power")
                .arg("-wait=true")
                .arg("-on")
                .arg(&node_name)
                .output()
                .context(resources, "Failed to start govc")?;
            if !vm_power_output.status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!(
                        "Failed to power on '{}': {}",
                        node_name,
                        String::from_utf8_lossy(&vm_power_output.stderr)
                    ),
                ));
            }

            // Grab the IP address of the VM
            // Note that the DNS name of the VM is the same as its IP address.
            // The hostname of the VM takes a while to show up and we can't wait for it
            // So we just take its IP address as soon as the VM gets one.
            let vm_info_output = Command::new("govc")
                .arg("vm.info")
                .arg("-waitip=true")
                .arg("-json=true")
                .arg(&node_name)
                .output()
                .context(resources, "Failed to start govc")?;
            let vm_info: serde_json::Value =
                serde_json::from_str(&String::from_utf8_lossy(&vm_info_output.stdout))
                    .context(resources, "Failed to deserialize govc vm.info output")?;
            let ip = vm_info
                .get("VirtualMachines")
                .and_then(|vms| vms.as_array())
                .and_then(|vms| vms.first())
                .and_then(|vm| vm.get("Guest"))
                .and_then(|guest| guest.get("IpAddress"))
                .and_then(|ip| ip.as_str())
                .context(
                    resources,
                    format!("Failed to get ip address for '{}'", node_name),
                )?;

            let instance_info = tokio::time::timeout(
                Duration::from_secs(60),
                wait_for_ssm_ready(&ssm_client, &memo.ssm_activation_id, ip),
            )
            .await
            .context(
                resources,
                format!("Timed out waiting for SSM agent to be ready on VM '{}'", ip),
            )?
            .context(resources, "Unable to determine if SSM activation is ready")?;

            memo.vms.push(VSphereVM {
                name: node_name,
                instance_id: instance_info.instance_id.context(
                    resources,
                    format!("Missing managed instance information for VM '{}'", ip),
                )?,
                ip_address: ip.to_string(),
            });
        }

        info!("VM(s) created");
        // We are done, set our custom status to say so.
        memo.current_status = "VM(s) created".into();

        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending final creation message")?;

        Ok(CreatedVSphereVms {
            instance_ids: memo
                .vms
                .iter()
                .map(|vm| vm.instance_id.to_owned())
                .collect(),
        })
    }
}

fn userdata(
    endpoint: &str,
    cluster_dns_ip: &str,
    bootstrap_token: &str,
    certificate: &str,
    control_container_userdata: &str,
    custom_user_data: &Option<CustomUserData>,
) -> ProviderResult<String> {
    let default_userdata = default_userdata(
        endpoint,
        cluster_dns_ip,
        bootstrap_token,
        certificate,
        control_container_userdata,
    );

    let custom_userdata = if let Some(value) = custom_user_data {
        value
    } else {
        return Ok(default_userdata);
    };

    match custom_userdata {
        CustomUserData::Replace { encoded_userdata } => Ok(encoded_userdata.to_string()),
        CustomUserData::Merge { encoded_userdata } => {
            let merge_into = &mut decode_to_string(&default_userdata)?
                .parse::<Value>()
                .context(Resources::Clear, "Failed to parse TOML")?;
            let merge_from = decode_to_string(encoded_userdata)?
                .parse::<Value>()
                .context(Resources::Clear, "Failed to parse TOML")?;
            merge_values(&merge_from, merge_into)
                .context(Resources::Clear, "Failed to merge TOML")?;
            Ok(base64_engine.encode(
                toml::to_string(merge_into)
                    .context(Resources::Clear, "Failed to serialize merged TOML")?,
            ))
        }
    }
}

fn default_userdata(
    endpoint: &str,
    cluster_dns_ip: &str,
    bootstrap_token: &str,
    certificate: &str,
    control_container_userdata: &str,
) -> String {
    base64_engine.encode(format!(
        r#"[settings.updates]
ignore-waves = true

[settings.host-containers.control]
enabled = true
user-data = "{}"

[settings.kubernetes]
api-server = "https://{}:6443"
cluster-dns-ip = "{}"
bootstrap-token = "{}"
cluster-certificate = "{}""#,
        control_container_userdata, endpoint, cluster_dns_ip, bootstrap_token, certificate
    ))
}

/// This is the object that will destroy ec2 instances.
pub struct VMDestroyer {}

#[async_trait::async_trait]
impl Destroy for VMDestroyer {
    type Config = VSphereVmConfig;
    type Info = ProductionMemo;
    type Resource = CreatedVSphereVms;

    async fn destroy<I>(
        &self,
        spec: Option<Spec<Self::Config>>,
        _resource: Option<Self::Resource>,
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
        let resources = if !memo.vms.is_empty()
            || !memo.ssm_activation_id.is_empty()
            || !memo.vm_template.is_empty()
        {
            Resources::Remaining
        } else {
            Resources::Clear
        };
        let spec = spec.context(resources, "Missing vSphere resource agent spec")?;

        let shared_config = aws_config(
            &memo.aws_secret_name.as_ref(),
            &memo.assume_role,
            &None,
            &None,
            false,
        )
        .await
        .context(Resources::Clear, "Error creating config")?;
        let ssm_client = aws_sdk_ssm::Client::new(&shared_config);

        let vsphere_cluster = spec.configuration.cluster.clone();
        debug!("Decoding and writing out kubeconfig for vSphere cluster");
        base64_decode_write_file(
            &vsphere_cluster.kubeconfig_base64,
            TEST_CLUSTER_KUBECONFIG_PATH,
        )
        .await
        .context(
            Resources::Clear,
            "Failed to write out kubeconfig for vSphere cluster",
        )?;
        let kubeconfig = Kubeconfig::read_from(TEST_CLUSTER_KUBECONFIG_PATH)
            .context(resources, "Unable to read kubeconfig")?;
        let config =
            Config::from_custom_kubeconfig(kubeconfig.to_owned(), &KubeConfigOptions::default())
                .await
                .context(resources, "Unable load kubeconfig")?;
        info!("Creating K8s client");
        let k8s_client = kube::client::Client::try_from(config)
            .context(resources, "Unable create K8s client from kubeconfig")?;

        // Get vSphere credentials to authenticate to vCenter via govmomi
        let secret_name = spec
            .secrets
            .get(VSPHERE_CREDENTIALS_SECRET_NAME)
            .context(resources, "Unable to fetch vSphere credentials")?;
        vsphere_credentials(client, secret_name, &resources).await?;

        // Set up environment variables for govc cli
        set_govc_env_vars(&spec.configuration);

        // Get the list of VMs to destroy
        for vm in &memo.vms {
            info!("Deregistering node '{}' from cluster", &vm.name);
            Api::<Node>::all(k8s_client.to_owned())
                .delete(&vm.ip_address, &DeleteParams::default())
                .await
                .context(
                    resources,
                    format!("Unable delete node '{}' from cluster", &vm.name),
                )?;

            info!("Destroying VM '{}'...", &vm.name);
            let vm_destroy_output = Command::new("govc")
                .arg("vm.destroy")
                .arg(format!("-vm.ip={}", vm.ip_address))
                .arg(&vm.name)
                .output()
                .context(resources, "Failed to start govc")?;
            if !vm_destroy_output.status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!(
                        "Failed to destroy '{}': {}",
                        &vm.name,
                        String::from_utf8_lossy(&vm_destroy_output.stderr)
                    ),
                ));
            }

            // Deregister managed instances
            ssm_client
                .deregister_managed_instance()
                .instance_id(&vm.instance_id)
                .send()
                .await
                .context(
                    resources,
                    format!("Failed deregister managed instance '{}'", &vm.instance_id),
                )?;
        }

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

        // Delete the SSM activation
        ssm_client
            .delete_activation()
            .activation_id(&memo.ssm_activation_id)
            .send()
            .await
            .context(
                resources,
                format!("Failed delete SSM activation '{}'", &memo.ssm_activation_id),
            )?;

        memo.vms.clear();
        info!("VM(s) deleted");
        memo.current_status = "VM(s) deleted".into();
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

pub fn set_govc_env_vars(config: &VSphereVmConfig) {
    env::set_var("GOVC_URL", &config.vcenter_host_url);
    env::set_var("GOVC_DATACENTER", &config.vcenter_datacenter);
    env::set_var("GOVC_DATASTORE", &config.vcenter_datastore);
    env::set_var("GOVC_NETWORK", &config.vcenter_network);
    env::set_var("GOVC_RESOURCE_POOL", &config.vcenter_resource_pool);
    env::set_var("GOVC_FOLDER", &config.vcenter_workload_folder);
}
