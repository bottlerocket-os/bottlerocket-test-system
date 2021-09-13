use crate::error::{self, Result};
use apiexts::CustomResourceDefinition;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1 as apiexts;
use kube::{
    api::{Api, Patch, PatchParams, PostParams, ResourceExt},
    Client, CustomResourceExt,
};
use model::model::{Test, NAMESPACE};
use model::system::{
    agent_cluster_role, agent_cluster_role_binding, agent_service_account, controller_cluster_role,
    controller_cluster_role_binding, controller_deployment, controller_service_account,
    testsys_namespace,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use structopt::StructOpt;

const CONTROLLER_SECRET: &str = "testsys-controller-pull-cred";

const DEFAULT_TESTSYS_CONTROLLER_IMAGE: &str =
    "334716814390.dkr.ecr.us-west-2.amazonaws.com/controller:2";

/// The install subcommand is responsible for putting all of the necessary components for testsys in
/// a k8s cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct Install {
    /// Controller image pull username
    #[structopt(
        long = "controller-pull-username",
        short = "u",
        requires("pull-password")
    )]
    pull_username: Option<String>,

    /// Controller image pull password
    #[structopt(
        long = "controller-pull-password",
        short = "p",
        requires("pull-username")
    )]
    pull_password: Option<String>,

    /// Controller image uri
    #[structopt(long = "controller-uri", default_value = DEFAULT_TESTSYS_CONTROLLER_IMAGE)]
    controller_uri: String,
}

impl Install {
    pub(crate) async fn run(&self) -> Result<()> {
        // Initialize the k8s client from in-cluster variables or KUBECONFIG.
        let client = Client::try_default().await.context(error::Client)?;

        create_namespace(&client).await?;
        create_crd(&client).await?;
        create_roles(&client).await?;
        create_service_accts(&client).await?;

        let mut controller_image_pull_secret = None;

        // Create the secret.
        if let (Some(username), Some(password)) =
            (self.pull_username.as_ref(), self.pull_password.as_ref())
        {
            create_secret(
                &client,
                username,
                password,
                self.controller_uri
                    .split('/')
                    .next()
                    .ok_or(error::Error::MissingRegistry {
                        uri: self.controller_uri.clone(),
                    })?,
            )
            .await?;
            // Use the secret we just created.
            controller_image_pull_secret = Some(CONTROLLER_SECRET.to_string());
        }

        create_deployment(
            &client,
            self.controller_uri.clone(),
            controller_image_pull_secret,
        )
        .await?;

        Ok(())
    }
}

async fn create_namespace(client: &Client) -> Result<()> {
    // Add the namespace to the cluster.
    let api: Api<k8s_openapi::api::core::v1::Namespace> = Api::all(client.clone());
    let ns = testsys_namespace();

    create_or_update(&api, ns, "namespace").await?;

    // Give the object enough time to settle.
    let mut sleep_count = 0;
    while api.get(NAMESPACE).await.is_err() && sleep_count < 20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        sleep_count += 1;
    }

    api.get(NAMESPACE)
        .await
        .context(error::Creation { what: "namespace" })?;

    Ok(())
}

async fn create_crd(client: &Client) -> Result<()> {
    // Manage the cluster CRDs.
    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    // Create the CRD.
    let testcrd = Test::crd();

    create_or_update(&crds, testcrd, "Test CRD").await
}

async fn create_roles(client: &Client) -> Result<()> {
    let roles: Api<k8s_openapi::api::rbac::v1::ClusterRole> = Api::all(client.clone());

    // If the role exists merge the new role, if not create the role.
    let agent_cluster_role = agent_cluster_role();
    create_or_update(&roles, agent_cluster_role, "Agent Cluster Role").await?;

    // If the role already exists, update it with the new one using Patch. If not create a new role.
    let controller_cluster_role = controller_cluster_role();
    create_or_update(&roles, controller_cluster_role, "Controller Cluster Role").await?;

    let rolesbinding: Api<k8s_openapi::api::rbac::v1::ClusterRoleBinding> =
        Api::all(client.clone());

    // If the cluster role binding already exists, update it with the new one using Patch. If not
    // create a new cluster role binding.
    let agent_cluster_role_binding = agent_cluster_role_binding();
    create_or_update(
        &rolesbinding,
        agent_cluster_role_binding,
        "Agent Cluster Role Binding",
    )
    .await?;

    // If the cluster role binding already exists, update it with the new one using Patch. If not
    // create a new cluster role binding.
    let controller_cluster_role_binding = controller_cluster_role_binding();
    create_or_update(
        &rolesbinding,
        controller_cluster_role_binding,
        "Controller Cluster Role Binding",
    )
    .await?;

    Ok(())
}

async fn create_service_accts(client: &Client) -> Result<()> {
    let service_accts: Api<k8s_openapi::api::core::v1::ServiceAccount> =
        Api::namespaced(client.clone(), NAMESPACE);

    // If the service accounts already exist, update them with the new ones using Patch.
    // If not create new service accounts.
    let agent_service_account = agent_service_account();
    create_or_update(
        &service_accts,
        agent_service_account,
        "Agent Service Account",
    )
    .await?;

    let controller_service_account = controller_service_account();
    create_or_update(
        &service_accts,
        controller_service_account,
        "Controller Service Accout",
    )
    .await?;

    Ok(())
}

async fn create_secret(
    client: &Client,
    username: &str,
    password: &str,
    registry_url: &str,
) -> Result<()> {
    // Create secret for controller image pull.
    let sec_str =
        serde_json::to_string_pretty(&DockerConfigJson::new(username, password, registry_url))
            .context(error::JsonSerialize)?;
    let mut secret_tree = BTreeMap::new();
    secret_tree.insert(".dockerconfigjson".to_string(), sec_str);

    let secrets: Api<k8s_openapi::api::core::v1::Secret> =
        Api::namespaced(client.clone(), NAMESPACE);

    let object_meta = kube::api::ObjectMeta {
        name: Some(CONTROLLER_SECRET.to_string()),
        ..Default::default()
    };

    // Create the secret we are going to add.
    let secret = Secret {
        data: None,
        immutable: None,
        metadata: object_meta,
        string_data: Some(secret_tree),
        type_: Some("kubernetes.io/dockerconfigjson".to_string()),
    };

    create_or_update(&secrets, secret, "Secret").await
}

async fn create_deployment(client: &Client, uri: String, secret: Option<String>) -> Result<()> {
    let deps: Api<k8s_openapi::api::apps::v1::Deployment> =
        Api::namespaced(client.clone(), NAMESPACE);

    let controller_deployment = controller_deployment(uri, secret);

    // If the controller deployment already exists, update it with the new one using Patch.
    // If not create a new controller deployment.
    create_or_update(&deps, controller_deployment, "namespace").await
}

async fn create_or_update<T>(api: &Api<T>, data: T, what: &str) -> Result<()>
where
    T: Clone + DeserializeOwned + Debug + kube::Resource + Serialize,
{
    // If the data already exists, update it with the new one using Patch. If not create a new one.
    match api.get(&data.name()).await {
        Ok(deployment) => {
            api.patch(
                &deployment.name(),
                &PatchParams::default(),
                &Patch::Merge(data),
            )
            .await
        }
        Err(_err) => api.create(&PostParams::default(), &data).await,
    }
    .context(error::Creation { what })?;

    Ok(())
}

#[derive(Serialize)]
struct DockerConfigJson {
    auths: HashMap<String, DockerConfigAuth>,
}

#[derive(Serialize)]
struct DockerConfigAuth {
    auth: String,
}

impl DockerConfigJson {
    fn new(username: &str, password: &str, registry: &str) -> DockerConfigJson {
        let mut auths = HashMap::new();
        let auth = base64::encode(format!("{}:{}", username, password));
        auths.insert(registry.to_string(), DockerConfigAuth { auth });
        DockerConfigJson { auths }
    }
}
