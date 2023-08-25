/*!
This is a resource-agent for provisioning an EKS Anywhere Bare Metal k8s cluster.
This agent is used to provision workload clusters.
https://anywhere.eks.amazonaws.com/docs/concepts/clusterworkflow/
Before using this agent, a management EKS Anywhere cluster needs to be created. Its kubeconfig will
be passed into the agent with `encodedMgmtClusterKubeconfig`.
Once the management cluster has been provisioned, an EKS Anywhere config file needs to be created
for the cluster that needs to be created.
Additionally, a hardware csv corresponding to the cluster config needs to be base64 encoded.

This agent utilizes workload cluster creation by invoking `eksctl anywhere create cluster` with the
management cluster's kubeconfig, the cluster config, and the hardware csv.
After the cluster has been created, an AWS SSM activation is created for each provisioned node.
The ssh key provided by EKS Anywhere is used to ssh to the admin container and set the ssm activation
with `api client set` allowing ssm docs to be run on the Bare Metal machines.
In addition to the ssm activation, additional userdata can used with `userdata`.
The userdata is accepted in the form of base64 encoded toml and is converted from toml to json and
applied using `apiclient set --json` over ssh via the admin container.

At deletion, each SSM activation is terminated and the cluster is cleaned up using
`eksctl anywhere delete cluster`. There is a chance that some of the artifacts created by EKS Anywhere
are not cleaned up completely, so `kubectl delete -f` is used on the management cluster to ensure all
k8s artifacts included in the workload cluster config are deleted.
!*/

use agent_utils::aws::aws_config;
use agent_utils::base64_decode_write_file;
use agent_utils::ssm::{create_ssm_activation, ensure_ssm_service_role, wait_for_ssm_ready};
use bottlerocket_agents::clusters::{
    retrieve_workload_cluster_kubeconfig, write_validate_mgmt_kubeconfig,
};
use bottlerocket_types::agent_config::{
    CustomUserData, MetalK8sClusterConfig, AWS_CREDENTIALS_SECRET_NAME,
};
use k8s_openapi::api::core::v1::Node;
use kube::config::Kubeconfig;
use kube::{Api, Config};
use log::{debug, info};
use openssh::{KnownHosts, SessionBuilder};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{env, fs};
use testsys_model::{Configuration, SecretName};

const WORKING_DIR: &str = "/local/eksa-work";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMemo {
    /// In this resource we put some traces here that describe what our provider is doing.
    pub current_status: String,

    pub ssm_activation_id: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The role that is being assumed.
    pub assume_role: Option<String>,

    /// The instance ids for machines provisioned by this agent.
    pub instance_ids: HashSet<String>,

    /// The name of the cluster.
    pub cluster_name: Option<String>,
}

impl Configuration for ProductionMemo {}

/// Once we have fulfilled the `Create` request, we return information about the metal K8s cluster
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedMetalK8sCluster {
    /// The base64 encoded kubeconfig for this cluster
    pub encoded_kubeconfig: String,

    /// The instance IDs of all SSM-registered machines
    pub instance_ids: HashSet<String>,
}

impl Configuration for CreatedMetalK8sCluster {}

pub struct MetalK8sClusterCreator {}

#[async_trait::async_trait]
impl Create for MetalK8sClusterCreator {
    type Config = MetalK8sClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedMetalK8sCluster;

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

        let mgmt_kubeconfig_path = format!("{}/mgmt.kubeconfig", WORKING_DIR);
        let eksa_config_path = format!("{}/cluster.yaml", WORKING_DIR);
        let hardware_csv_path = format!("{}/hardware.csv", WORKING_DIR);
        base64_decode_write_file(&spec.configuration.hardware_csv_base64, &hardware_csv_path)
            .await
            .context(
                resources,
                "Unable to decode and write hardware requirements",
            )?;

        let decoded_config = base64::decode(&spec.configuration.cluster_config_base64)
            .context(Resources::Clear, "Unable to decode eksctl configuration.")?;

        let cluster_name = serde_yaml::Deserializer::from_slice(decoded_config.as_slice())
            .map(|config| {
                serde_yaml::Value::deserialize(config)
                    .context(Resources::Clear, "Unable to deserialize eksa config file")
            })
            // Make sure all of the configs were deserializable
            .collect::<ProviderResult<Vec<_>>>()?
            .iter()
            // Find the `Cluster` config
            .find(|config| {
                config.get("kind") == Some(&serde_yaml::Value::String("Cluster".to_string()))
            })
            // Get the name from the metadata field in the `Cluster` config
            .and_then(|config| config.get("metadata"))
            .and_then(|config| config.get("name"))
            .and_then(|name| name.as_str())
            .context(
                Resources::Clear,
                "Unable to determine the cluster name from the config file ",
            )?
            .to_string();

        fs::write(&eksa_config_path, decoded_config).context(
            Resources::Clear,
            format!(
                "Unable to write EKS Anywhere configuration to '{}'",
                eksa_config_path
            ),
        )?;

        info!("Creating cluster");
        memo.current_status = "Creating cluster".to_string();
        memo.cluster_name = Some(cluster_name.clone());
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
        // Call eksctl-anywhere to create cluster with existing mgmt cluster
        let status = Command::new("eksctl")
            .args(["anywhere", "create", "cluster"])
            .args(["--kubeconfig", &mgmt_kubeconfig_path])
            .args(["-f", &eksa_config_path])
            .args(["--hardware-csv", &hardware_csv_path])
            .arg("--skip-ip-check")
            .args(["-v", "4"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context(resources, "Failed to launch eksctl process")?;
        resources = Resources::Remaining;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to create EKS-A workload cluster with status code {}",
                    status
                ),
            ));
        }
        let encoded_kubeconfig =
            retrieve_workload_cluster_kubeconfig(mgmt_k8s_client, &cluster_name, &resources)
                .await?;

        info!("Cluster created");

        // Now we need the nodes ip addresses to enable SSM.
        memo.current_status = "Getting node ips".into();

        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending final creation message")?;

        let k8s_client = kube::client::Client::try_from(
            Config::from_custom_kubeconfig(
                Kubeconfig::from_yaml(&String::from_utf8_lossy(
                    &base64::decode(&encoded_kubeconfig).context(
                        resources,
                        "Unable to decode encoded workload cluster kubeconfig",
                    )?,
                ))
                .context(
                    resources,
                    "Unable to create workload kubeconfig from encoded config",
                )?,
                &Default::default(),
            )
            .await
            .context(resources, "Unable to create `Config` from `Kubeconfig`")?,
        )
        .context(resources, "Unable create K8s client from kubeconfig")?;

        let machine_ips = Api::<Node>::all(k8s_client)
            .list(&Default::default())
            .await
            .context(resources, "Unable to list nodes from workload cluster")?
            .into_iter()
            .map(|node| {
                node.status
                    .and_then(|status| status.addresses)
                    .unwrap_or_default()
            })
            .map(|addresses| {
                addresses
                    .into_iter()
                    .find(|addr| addr.type_.as_str() == "InternalIP")
                    .map(|addr| addr.address)
                    .context(resources, "A node was missing an InternalIp address")
            })
            .collect::<ProviderResult<Vec<String>>>()?;

        // Generate SSM activation codes and IDs
        let activation =
            create_ssm_activation(&cluster_name, machine_ips.len() as i32, &ssm_client)
                .await
                .context(resources, "Unable to create SSM activation")?;
        memo.ssm_activation_id = activation.0.to_owned();
        let control_host_ctr_userdata = json!({"ssm":{"activation-id": activation.0.to_string(), "activation-code":activation.1.to_string(),"region":"us-west-2"}});
        debug!(
            "Control container host container userdata: {}",
            control_host_ctr_userdata
        );
        let ssm_json = json!({"host-containers":{"control":{"enabled":true, "user-data": base64::encode(control_host_ctr_userdata.to_string())}}});

        let custom_settings = &spec
            .configuration
            .custom_user_data
            .map(|userdata| match userdata {
                CustomUserData::Replace { encoded_userdata }
                | CustomUserData::Merge { encoded_userdata } => encoded_userdata,
            })
            .map(base64::decode)
            .transpose()
            .context(resources, "Unable to decode custom user data")?
            .map(|userdata| toml::from_slice::<serde_json::Value>(&userdata))
            .transpose()
            .context(resources, "Unable to deserialize custom userdata")?;

        custom_settings.iter().for_each(|settings| {
            info!(
                "Custom userdata was deserialized to the following settings:\n{}",
                settings.to_string()
            )
        });

        // Enable SSM on the nodes
        let mut instance_ids = HashSet::new();
        for machine_ip in machine_ips {
            info!("Starting session for {}", machine_ip);
            let session = SessionBuilder::default()
                .keyfile(
                    Path::new(WORKING_DIR)
                        .join(&cluster_name)
                        .join("eks-a-id_rsa"),
                )
                .user("ec2-user".to_string())
                .known_hosts_check(KnownHosts::Accept)
                .user_known_hosts_file("/dev/null")
                .connect_timeout(Duration::from_secs(5))
                .connect_mux(&machine_ip)
                .await
                .context(
                    resources,
                    format!("Unable to connect to machine with ip '{}'", machine_ip),
                )?;

            info!("Getting initial settings for {}", machine_ip);
            let status = session
                .command("apiclient")
                .args(["get", "settings"])
                .status()
                .await
                .context(resources, "Unable to call `apiclient get settings`")?;
            if !status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!("Failed to get settings with status code {}", status),
                ));
            }

            info!("Setting ssm activations for {}", machine_ip);
            let status = session
                .command("apiclient")
                .args(["set", "--json", &ssm_json.to_string()])
                .status()
                .await
                .context(resources, "Unable to call `apiclient set`")?;
            if !status.success() {
                return Err(ProviderError::new_with_context(
                    resources,
                    format!("Failed to set ssm activation with status code {}", status),
                ));
            }

            if let Some(settings) = &custom_settings {
                info!(
                    "Settings custom userdata as settings via `apiclient set` for {}",
                    machine_ip
                );

                let status = session
                    .command("apiclient")
                    .args(["set", "--json", &settings.to_string()])
                    .status()
                    .await
                    .context(resources, "Unable to call `apiclient set`")?;
                if !status.success() {
                    return Err(ProviderError::new_with_context(
                        resources,
                        format!("Failed to set custom settings with status code {}", status),
                    ));
                }
            }

            let instance_info = tokio::time::timeout(
                Duration::from_secs(60),
                wait_for_ssm_ready(&ssm_client, &memo.ssm_activation_id, &machine_ip),
            )
            .await
            .context(
                resources,
                format!(
                    "Timed out waiting for SSM agent to be ready on VM '{}'",
                    machine_ip
                ),
            )?
            .context(resources, "Unable to determine if SSM activation is ready")?;

            instance_ids.insert(
                instance_info
                    .instance_id()
                    .context(
                        resources,
                        format!(
                    "The instance id was missing for the machine with ip '{}' and activation '{}'",
                    machine_ip, &memo.ssm_activation_id
                ),
                    )?
                    .to_string(),
            );
        }

        // We are done, set our custom status to say so.
        memo.current_status = "Cluster created".into();
        memo.instance_ids = instance_ids.clone();

        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending final creation message")?;

        Ok(CreatedMetalK8sCluster {
            instance_ids,
            encoded_kubeconfig,
        })
    }
}

pub struct MetalK8sClusterDestroyer {}

#[async_trait::async_trait]
impl Destroy for MetalK8sClusterDestroyer {
    type Config = MetalK8sClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedMetalK8sCluster;

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
        let resources = if !memo.instance_ids.is_empty() || !memo.ssm_activation_id.is_empty() {
            Resources::Remaining
        } else {
            Resources::Clear
        };

        let cluster_name = memo.cluster_name.as_ref().context(
            resources,
            "The cluster name was missing from this agents production memo",
        )?;

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

        // Set the cluster deletion configs paths
        let mgmt_kubeconfig_path = format!("{}/mgmt.kubeconfig", WORKING_DIR);
        let eksa_config_path = format!("{}/cluster.yaml", WORKING_DIR);
        let workload_kubeconfig_path =
            format!("{}/{}-eks-a-cluster.kubeconfig", WORKING_DIR, cluster_name);

        // Populate each file that is needed for cluster deletion
        let configuration = spec
            .context(resources, "The spec was not provided for destruction")?
            .configuration;
        base64_decode_write_file(
            &configuration.mgmt_cluster_kubeconfig_base64,
            &mgmt_kubeconfig_path,
        )
        .await
        .context(
            resources,
            "Unable to decode and write hardware requirements",
        )?;
        base64_decode_write_file(&configuration.cluster_config_base64, &eksa_config_path)
            .await
            .context(
                resources,
                "Unable to decode and write hardware requirements",
            )?;
        let encoded_kubeconfig = resource
            .context(
                resources,
                "The created resource was not provided for destruction",
            )?
            .encoded_kubeconfig;
        base64_decode_write_file(&encoded_kubeconfig, &workload_kubeconfig_path)
            .await
            .context(
                resources,
                "Unable to decode and write hardware requirements",
            )?;

        info!("Deleting cluster");
        memo.current_status = "Deleting cluster".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;
        // Call eksctl-anywhere to delete cluster with mgmt cluster
        let status = Command::new("eksctl")
            .args(["anywhere", "delete", "cluster"])
            .args(["--kubeconfig", &mgmt_kubeconfig_path])
            .args(["-f", &eksa_config_path])
            .args(["--w-config", &workload_kubeconfig_path])
            .args(["-v", "4"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context(resources, "Failed to launch eksctl process")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to delete EKS-A workload cluster with status code {}",
                    status
                ),
            ));
        }

        info!("Cleaning up leftover EKSA artifacts.");
        // Clean up leftover EKSA Templates
        Command::new("kubectl")
            .args(["delete"])
            .args(["--kubeconfig", &mgmt_kubeconfig_path])
            .args(["-f", &eksa_config_path])
            .args(["--ignore-not-found"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context(resources, "Failed to launch eksctl process")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!(
                    "Failed to delete EKS-A workload cluster with status code {}",
                    status
                ),
            ));
        }

        // Deregister managed instances
        for instance_id in &memo.instance_ids {
            info!("Deregistering {}", instance_id);
            ssm_client
                .deregister_managed_instance()
                .instance_id(instance_id)
                .send()
                .await
                .context(
                    resources,
                    format!("Failed deregister managed instance '{}'", instance_id),
                )?;
        }

        memo.instance_ids.clear();
        info!("Cluster deleted");
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
