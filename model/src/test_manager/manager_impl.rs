use super::{error, ResourceState, Result, TestManager};
use crate::clients::{AllowNotFound, CrdClient};
use crate::constants::{LABEL_COMPONENT, NAMESPACE};
use crate::{Crd, CrdName, Resource, Test};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{ListParams, Patch, PatchParams, PostParams};
use kube::{Api, Resource as KubeResource, ResourceExt};
use serde::{de::DeserializeOwned, Serialize};
use snafu::{OptionExt, ResultExt};
use std::fmt::Debug;

impl TestManager {
    /// Retry attempts for creating or updating an object.
    const MAX_RETRIES: i32 = 3;
    /// Timeout for object creation/update retries.
    const BACKOFF_MS: u64 = 500;

    /// Create or update an existing k8s object
    pub(super) async fn create_or_update<T>(
        &self,
        namespaced: bool,
        data: &T,
        what: &str,
    ) -> Result<()>
    where
        T: KubeResource + Clone + DeserializeOwned + Serialize + Debug,
        <T as KubeResource>::DynamicType: Default,
    {
        let mut error = None;

        for _ in 0..Self::MAX_RETRIES {
            match self.create_or_update_internal(namespaced, data, what).await {
                Ok(()) => return Ok(()),
                Err(e) => error = Some(e),
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(Self::BACKOFF_MS)).await;
        }
        match error {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    pub(super) async fn create_or_update_internal<T>(
        &self,
        namespaced: bool,
        data: &T,
        what: &str,
    ) -> Result<()>
    where
        T: KubeResource + Clone + DeserializeOwned + Serialize + Debug,
        <T as KubeResource>::DynamicType: Default,
    {
        let api = if namespaced {
            self.namespaced_api::<T>()
        } else {
            self.api::<T>()
        };
        // If the data already exists, update it with the new one using a `Patch`. If not create a
        // new one.
        match api.get(&data.name()).await {
            Ok(deployment) => {
                api.patch(
                    &deployment.name(),
                    &PatchParams::default(),
                    &Patch::Merge(data),
                )
                .await
            }
            Err(_err) => api.create(&PostParams::default(), data).await,
        }
        .context(error::CreateSnafu { what })?;

        Ok(())
    }

    /// Creates a non namespaced api of type `T`
    pub(super) fn api<T>(&self) -> Api<T>
    where
        T: KubeResource,
        <T as KubeResource>::DynamicType: Default,
    {
        Api::<T>::all(self.k8s_client.clone())
    }

    /// Creates a namespaced api of type `T`
    pub(super) fn namespaced_api<T>(&self) -> Api<T>
    where
        T: KubeResource,
        <T as KubeResource>::DynamicType: Default,
    {
        Api::<T>::namespaced(self.k8s_client.clone(), NAMESPACE)
    }

    /// Returns a list containing all dependencies for each object in a `Vec<Crd>` including the
    /// objects themselves
    pub(super) async fn add_dependencies_to_vec(&self, objects: Vec<Crd>) -> Result<Vec<Crd>> {
        let mut dependencies = Vec::new();
        let mut to_be_visited = objects;
        while let Some(crd) = to_be_visited.pop() {
            dependencies.push(crd.clone());
            let resources = match crd {
                Crd::Test(test) => test.spec.resources,
                Crd::Resource(resource) => resource.spec.depends_on.unwrap_or_default(),
            };
            for resource in resources {
                if let Some(resource_spec) = self
                    .resource_client()
                    .get(resource)
                    .await
                    .allow_not_found(|_| ())
                    .context(error::ClientSnafu {
                        action: "get resource",
                    })?
                {
                    to_be_visited.push(Crd::Resource(resource_spec));
                }
            }
        }

        Ok(dependencies)
    }

    /// Get all pods in a cluster that are doing work for a testsys crd.
    pub(super) async fn get_pods(&self, crd: &CrdName) -> Result<Vec<Pod>> {
        let pod_api: Api<Pod> = self.namespaced_api();
        Ok(match crd {
            CrdName::Test(test) => {
                pod_api
                    .list(&ListParams {
                        label_selector: Some(format!("job-name={}", test)),
                        ..Default::default()
                    })
                    .await
                    .context(error::KubeSnafu { action: "get pods" })?
                    .items
            }
            CrdName::Resource(resource) => {
                let mut pods = Vec::new();
                pods.append(
                    &mut pod_api
                        .list(&ListParams {
                            label_selector: Some(format!("job-name={}-creation", resource)),
                            ..Default::default()
                        })
                        .await
                        .context(error::KubeSnafu { action: "get pods" })?
                        .items,
                );
                pods.append(
                    &mut pod_api
                        .list(&ListParams {
                            label_selector: Some(format!("job-name={}-destruction", resource)),
                            ..Default::default()
                        })
                        .await
                        .context(error::KubeSnafu { action: "get pods" })?
                        .items,
                );
                pods
            }
        })
    }

    /// Add a testsys test to the cluster.
    pub(super) async fn create_test(&self, test: Test) -> Result<()> {
        let test_client = self.test_client();
        test_client.create(test).await.context(error::ClientSnafu {
            action: "create new test",
        })?;
        Ok(())
    }

    /// Add a testsys resource to the cluster.
    pub(super) async fn create_resource(&self, resource: Resource) -> Result<()> {
        let resource_client = self.resource_client();
        resource_client
            .create(resource)
            .await
            .context(error::ClientSnafu {
                action: "create new resource",
            })?;
        Ok(())
    }

    /// Get a pod for a testsys test.
    pub(super) async fn test_pod<S>(&self, test: S) -> Result<Pod>
    where
        S: Into<String>,
    {
        let pod_api: Api<Pod> = self.namespaced_api();
        pod_api
            .list(&ListParams {
                label_selector: Some(format!("job-name={}", test.into())),
                ..Default::default()
            })
            .await
            .context(error::KubeSnafu { action: "get pods" })?
            .items
            .first()
            .context(error::NotFoundSnafu {
                what: "pod for test",
            })
            .map(|pod| pod.clone())
    }

    /// Get a pod for a testsys resource.
    pub(super) async fn resource_pod<S>(&self, resource: S, state: ResourceState) -> Result<Pod>
    where
        S: Into<String>,
    {
        let pod_api: Api<Pod> = self.namespaced_api();
        let suffix = match state {
            ResourceState::Creation => "creation",
            ResourceState::Destruction => "destruction",
        };
        pod_api
            .list(&ListParams {
                label_selector: Some(format!("job-name={}-{}", resource.into(), suffix)),
                ..Default::default()
            })
            .await
            .context(error::KubeSnafu { action: "get pods" })?
            .items
            .first()
            .context(error::NotFoundSnafu {
                what: "pod for test",
            })
            .map(|pod| pod.clone())
    }

    /// Get a pod for the testsys controller.
    pub(super) async fn controller_pod(&self) -> Result<Pod> {
        let pod_api: Api<Pod> = self.namespaced_api();
        pod_api
            .list(&ListParams {
                label_selector: Some(format!("{}={}", LABEL_COMPONENT, "controller")),
                ..Default::default()
            })
            .await
            .context(error::KubeSnafu {
                action: "get controller pod",
            })?
            .items
            .first()
            .context(error::NotFoundSnafu {
                what: "controller pod for test",
            })
            .map(|pod| pod.clone())
    }
}
