use super::{
    error, CrdState, CrdType, DeleteEvent, DockerConfigJson, ImageConfig, ResourceState, Result,
    SelectionParams, StatusSnapshot,
};
use crate::clients::{AllowNotFound, CrdClient, ResourceClient, TestClient};
use crate::constants::TESTSYS_RESULTS_FILE;
use crate::system::AgentType;
use crate::{Crd, CrdName, Resource, SecretName, TaskState, Test, TestUserState};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use k8s_openapi::api::core::v1::{Pod, Secret};
use kube::api::{ListParams, LogParams};
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config, Error, ResourceExt};
use serde::Deserialize;
use snafu::{OptionExt, ResultExt};
use std::{collections::BTreeMap, path::Path};
use tokio::io::AsyncWriteExt;

/// # Test Manager
///
/// The test manager provides operations for coordinating the creation, operation, deletion and
/// observation of `Test` and `Resource` objects. It understands the dependencies between `Test`s
/// and `Resource`s.
///
/// # Operations
///
/// Here are some of the things that you can do with the test manager:
/// - Delete a test and all of the tests and resources it depends on
/// - Get the logs from a test pod or one of its resource agent pods
/// - Get a structured summary of test results from multiple tests
///
/// # Clients
///
/// For direct, lower-level operations on the `Test` and `Resource` objects themselves, you can use
/// the [`TestClient`] and [`ResourceClient`] objects. These clients can be constructed
/// independently or obtained from the `TestManager` using `test_client()` and `resource_client()`
/// functions.
///
pub struct TestManager {
    pub k8s_client: Client,
}

impl TestManager {
    /// Create a `TestManager` from the path to a kubeconfig file.
    pub async fn new_from_kubeconfig_path(kubeconfig_path: &Path) -> Result<Self> {
        let kubeconfig = Kubeconfig::read_from(kubeconfig_path).context(error::ConfigReadSnafu)?;
        let config = Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
            .await
            .context(error::ClientCreateKubeconfigSnafu)?;
        Ok(TestManager {
            k8s_client: config.try_into().context(error::KubeSnafu {
                action: "create client from `Kubeconfig`",
            })?,
        })
    }

    /// Create a `TestManager` using the default `kube::Client`.
    pub async fn new() -> Result<Self> {
        Ok(TestManager {
            k8s_client: Client::try_default().await.context(error::KubeSnafu {
                action: "create client from `Kubeconfig`",
            })?,
        })
    }

    /// Create a `TestClient`
    pub fn test_client(&self) -> TestClient {
        TestClient::new_from_k8s_client(self.k8s_client.clone())
    }

    /// Create a `ResourceClient`
    pub fn resource_client(&self) -> ResourceClient {
        ResourceClient::new_from_k8s_client(self.k8s_client.clone())
    }

    /// Create a secret for image pulls using `DockerConfigJson`
    pub async fn create_image_pull_secret(
        &self,
        name: &str,
        username: &str,
        password: &str,
        image_url: &str,
    ) -> Result<Secret> {
        // Create docker config json for the image pull secret.
        let sec_str =
            serde_json::to_string_pretty(&DockerConfigJson::new(username, password, image_url))
                .context(error::JsonSerializeSnafu)?;
        let mut secret_tree = BTreeMap::new();
        secret_tree.insert(".dockerconfigjson".to_string(), sec_str);

        let object_meta = kube::api::ObjectMeta {
            name: Some(name.to_string()),
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

        self.create_or_update(self.namespaced_api(), &secret, "controller pull secret")
            .await?;
        Ok(secret)
    }

    pub async fn create_secret<I>(&self, name: &SecretName, data: I) -> Result<Secret>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let object_meta = kube::api::ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        };

        // Create the secret we are going to add.
        let secret = Secret {
            data: None,
            immutable: None,
            metadata: object_meta,
            string_data: Some(data.into_iter().collect()),
            type_: None,
        };
        self.create_or_update(self.namespaced_api(), &secret, "secret")
            .await?;
        Ok(secret)
    }

    /// Install testsys to a cluster.
    pub async fn install(&self, controller_config: ImageConfig, store_logs: bool) -> Result<()> {
        self.create_namespace().await?;
        self.create_crd().await?;
        self.create_roles(AgentType::Test).await?;
        self.create_roles(AgentType::Resource).await?;
        self.create_service_accts(AgentType::Test).await?;
        self.create_service_accts(AgentType::Resource).await?;
        self.create_controller_service_acct().await?;

        // Add the controller to the cluster
        let (image, secret) = match controller_config {
            ImageConfig::WithCreds { secret, image } => (image, Some(secret)),
            ImageConfig::Image(image) => (image, None),
        };
        self.create_deployment(image, secret, store_logs).await?;

        Ok(())
    }

    /// Uninstall testsys from a cluster.
    pub async fn uninstall(&self) -> Result<()> {
        if !self
            .resource_client()
            .get_all()
            .await
            .allow_not_found(|_| ())
            .context(error::ClientSnafu {
                action: "get all resources",
            })?
            .unwrap_or_default()
            .is_empty()
        {
            return Err(error::Error::ResourceExisting);
        }
        self.uninstall_testsys().await?;
        self.wait_for_namespace_deletion().await
    }

    /// Restart a crd object by deleting the crd from the cluster and adding a copy of it with its
    /// status cleared.
    pub async fn restart_test(&self, name: &str) -> Result<()> {
        let test_client = TestClient::new_from_k8s_client(self.k8s_client.clone());
        let mut test = test_client
            .get(name)
            .await
            .context(error::ClientSnafu { action: "get test" })?;
        // Created objects are not allowed to have `resource_version` set.
        test.metadata.resource_version = None;
        test.status = None;
        test_client.delete(name).await.context(error::ClientSnafu {
            action: "delete test",
        })?;
        test_client.wait_for_deletion(name).await;
        test_client.create(test).await.context(error::ClientSnafu {
            action: "create new test",
        })?;
        Ok(())
    }

    /// Add a testsys crd (`Test`, `Resource`) to the cluster.
    pub async fn create_object(&self, crd: Crd) -> Result<Crd> {
        match &crd {
            Crd::Test(test) => self.create_test(test.clone()).await?,
            Crd::Resource(resource) => self.create_resource(resource.clone()).await?,
        }
        Ok(crd)
    }

    /// List all testsys objects following `SelectionParams`
    pub async fn list(&self, selection_params: &SelectionParams) -> Result<Vec<Crd>> {
        let mut list_params = ListParams::default();
        if let Some(labels) = &selection_params.labels {
            list_params = list_params.labels(labels)
        }
        if let Some(name) = &selection_params.name {
            list_params = list_params.fields(&format!("metadata.name=={}", name));
        }
        let mut objects = Vec::new();
        if matches!(selection_params.crd_type, Some(CrdType::Test) | None) {
            objects.extend(
                self.test_client()
                    .api()
                    .list(&list_params)
                    .await
                    .context(error::KubeSnafu {
                        action: "list tests from label params",
                    })?
                    .into_iter()
                    .filter(|test| filter_test_by_state(test, &selection_params.state))
                    .map(Crd::Test),
            );
        }
        if matches!(selection_params.crd_type, Some(CrdType::Resource) | None) {
            objects.extend(
                self.resource_client()
                    .api()
                    .list(&list_params)
                    .await
                    .context(error::KubeSnafu {
                        action: "list tests from label params",
                    })?
                    .into_iter()
                    .filter(|resource| filter_resource_by_state(resource, &selection_params.state))
                    .map(Crd::Resource),
            );
        }

        Ok(objects)
    }

    /// Delete all testsys `Test`s and `Resource`s from a cluster.
    pub async fn delete_all(&self) -> Result<impl Stream<Item = Result<DeleteEvent>>> {
        let deletion_order = self.all_objects_deletion_order().await?;
        Ok(self.delete_sorted_resources(deletion_order))
    }

    /// Delete resources from a testsys cluster based on `SelectionParams`. If
    /// `include_dependencies` all objects that each item depends on will also be deleted.
    pub async fn delete(
        &self,
        selection_params: &SelectionParams,
        include_dependencies: bool,
    ) -> Result<impl Stream<Item = Result<DeleteEvent>>> {
        let mut objects = self.list(selection_params).await?;
        if include_dependencies {
            objects = self.add_dependencies_to_vec(objects).await?;
        }
        Ok(self.delete_sorted_resources(Self::vec_to_deletion_order(objects)))
    }

    /// Delete the resource after a failed deletion attempt.
    /// Warning: the physical resources may not be deleted.
    /// The finalizers will be removed from the resource and the resource will be deleted.
    /// The k8s job for resource deletion will also be deleted.
    /// This should only be used if a resource has already failed to delete.
    /// All tests will be deleted normally.
    pub async fn force_delete_resource(&self, selection_params: &SelectionParams) -> Result<()> {
        let objects = self.list(selection_params).await?;
        for object in objects {
            match object {
                Crd::Test(test) => {
                    self.test_client().delete(test.name_any()).await.context(
                        error::ClientSnafu {
                            action: "delete test",
                        },
                    )?;
                }
                Crd::Resource(resource) => {
                    self.resource_client()
                        .force_delete(resource.name_any())
                        .await
                        .context(error::ClientSnafu {
                            action: "delete test",
                        })?;
                }
            };
        }
        Ok(())
    }

    /// Collect the status of all testsys objects meeting `selection_params`. If
    /// `include_controller` the status of the controller will also be recorded. The `Status`
    /// returned can be used to print a table containing each objects status (including rerun tests)
    /// or to print a json representation containing all included objects as well as the controller
    /// status.
    pub async fn status(&self, selection_params: &SelectionParams) -> Result<StatusSnapshot> {
        let crds = self.list(selection_params).await?;

        Ok(StatusSnapshot::new(crds))
    }

    /// Retrieve the logs of a test.
    pub async fn test_logs<S>(
        &self,
        test_name: S,
        follow: bool,
    ) -> Result<impl Stream<Item = Result<Bytes>>>
    where
        S: Into<String>,
    {
        let pod = self.test_pod(test_name).await?;
        let log_params = LogParams {
            follow,
            pretty: true,
            ..Default::default()
        };
        let pod_api: Api<Pod> = self.namespaced_api();
        pod_api
            .log_stream(&pod.name_any(), &log_params)
            .await
            .context(error::KubeSnafu {
                action: "stream logs",
            })
            .map(|stream| {
                stream.map(|res| {
                    res.context(error::KubeSnafu {
                        action: "stream logs",
                    })
                })
            })
    }

    /// Retrieve the logs of a resource.
    pub async fn resource_logs<S>(
        &self,
        resource_name: S,
        state: ResourceState,
        follow: bool,
    ) -> Result<impl Stream<Item = Result<Bytes>>>
    where
        S: Into<String>,
    {
        let pod = self.resource_pod(resource_name, state).await?;
        let log_params = LogParams {
            follow,
            pretty: true,
            ..Default::default()
        };
        let pod_api: Api<Pod> = self.namespaced_api();
        pod_api
            .log_stream(&pod.name_any(), &log_params)
            .await
            .context(error::KubeSnafu {
                action: "stream logs",
            })
            .map(|stream| {
                stream.map(|res| {
                    res.context(error::KubeSnafu {
                        action: "stream logs",
                    })
                })
            })
    }

    /// Retrieve the logs of the controller.
    pub async fn controller_logs(
        &self,
        follow: bool,
    ) -> Result<impl Stream<Item = core::result::Result<Bytes, Error>>> {
        let pod_api: Api<Pod> = self.namespaced_api();
        let pod = self.controller_pod().await?;
        let log_params = LogParams {
            follow,
            pretty: true,
            ..Default::default()
        };
        pod_api
            .log_stream(&pod.name_any(), &log_params)
            .await
            .context(error::KubeSnafu {
                action: "stream logs",
            })
    }

    /// Write the results from a testsys `Test` to a given `destination`. The results are in the
    /// form of a tarball containing all files placed in the test agents output directory.
    pub async fn write_test_results(&self, test_name: &str, destination: &Path) -> Result<()> {
        let pod_name = self
            .get_pods(&CrdName::Test(test_name.to_string()))
            .await?
            .pop()
            .context(error::NotFoundSnafu { what: "test" })?
            .name_any();

        let pods: Api<Pod> = self.namespaced_api();
        let mut cat = pods
            .exec(
                &pod_name,
                vec!["cat", TESTSYS_RESULTS_FILE],
                &Default::default(),
            )
            .await
            .context(error::KubeSnafu {
                action: "get results tarball",
            })?;
        let mut cat_out =
            tokio_util::io::ReaderStream::new(cat.stdout().context(error::NotFoundSnafu {
                what: "results stdout",
            })?);

        let mut out_file = tokio::fs::File::create(destination)
            .await
            .context(error::FileSnafu { path: destination })?;
        while let Some(data) = cat_out.next().await {
            out_file
                .write(&data.context(error::IoSnafu {
                    action: "get results line",
                })?)
                .await
                .context(error::IoSnafu {
                    action: "write results",
                })?;
        }
        out_file.flush().await.context(error::IoSnafu {
            action: "flush results",
        })?;
        Ok(())
    }
}

/// Takes a path to a yaml manifest of testsys crds (`Test` and `Resource`) and creates a set of
/// `Crd`s through deserialization. These can be added using `TestManager::create_object`
pub fn read_manifest(path: &Path) -> Result<Vec<Crd>> {
    // Create the resource objects from its path.
    let manifest_string = std::fs::read_to_string(path).context(error::FileSnafu { path })?;
    convert_manifest(manifest_string)
}

/// Takes a `String` containing a yaml manifest of testsys crds (`Test` and `Resource`) and creates
/// a set of `Crd`s through deserialization. These can be added using `TestManager::create_object`
pub fn convert_manifest(manifest_string: String) -> Result<Vec<Crd>> {
    let mut crds = Vec::new();
    for crd_doc in serde_yaml::Deserializer::from_str(&manifest_string) {
        let value = serde_yaml::Value::deserialize(crd_doc).context(error::SerdeYamlSnafu {
            action: "deserialize manifest",
        })?;
        let crd: Crd = serde_yaml::from_value(value).context(error::SerdeYamlSnafu {
            action: "deserialize manifest",
        })?;
        crds.push(crd);
    }
    Ok(crds)
}

fn filter_test_by_state(test: &Test, state: &Option<CrdState>) -> bool {
    if let Some(state) = state {
        match state {
            CrdState::Running => {
                matches!(test.test_user_state(), TestUserState::Running)
            }
            CrdState::Completed => matches!(
                test.test_user_state(),
                TestUserState::NoTests
                    | TestUserState::Passed
                    | TestUserState::Failed
                    | TestUserState::Error
                    | TestUserState::ResourceError
            ),
            CrdState::Passed => {
                matches!(test.test_user_state(), TestUserState::Passed)
            }
            CrdState::Failed => {
                matches!(
                    test.test_user_state(),
                    TestUserState::Failed | TestUserState::Error | TestUserState::ResourceError
                )
            }
            CrdState::NotFinished => matches!(
                test.test_user_state(),
                TestUserState::Running | TestUserState::Waiting | TestUserState::Unknown
            ),
        }
    } else {
        true
    }
}

fn filter_resource_by_state(resource: &Resource, state: &Option<CrdState>) -> bool {
    if let Some(state) = state {
        match state {
            CrdState::Running => {
                matches!(resource.creation_task_state(), TaskState::Running)
                    || matches!(resource.destruction_task_state(), TaskState::Running)
            }
            CrdState::Completed => {
                matches!(
                    resource.creation_task_state(),
                    TaskState::Completed | TaskState::Error
                ) && !matches!(resource.destruction_task_state(), TaskState::Running)
            }
            CrdState::NotFinished => {
                matches!(
                    resource.creation_task_state(),
                    TaskState::Running | TaskState::Unknown
                ) || matches!(
                    resource.creation_task_state(),
                    TaskState::Running | TaskState::Unknown
                )
            }
            _ => false,
        }
    } else {
        true
    }
}
