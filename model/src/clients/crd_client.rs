use super::HttpStatusCode;
use crate::clients::error::{self, Result};
use crate::constants::NAMESPACE;
use crate::CrdExt;
use core::fmt::Debug;
use http::StatusCode;
use json_patch::{AddOperation, PatchOperation, RemoveOperation, ReplaceOperation, TestOperation};
use kube::api::{ListParams, Patch, PatchParams, PostParams};
use kube::{Api, Resource};
use log::trace;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use snafu::{ensure, OptionExt, ResultExt};
use std::time::Duration;

/// A trait with implementations of code that is shared between more than one CRD object.
#[async_trait::async_trait]
pub trait CrdClient: Sized {
    type Crd: kube::Resource<DynamicType = ()>
        + Serialize
        + DeserializeOwned
        + Debug
        + Clone
        + Send
        + Sync
        + CrdExt;
    type CrdStatus: Serialize + Default + Send;

    // The following need to be implemented which allows the rest of the functions to have
    // default implementations.

    fn new_from_api(api: Api<Self::Crd>) -> Self;
    fn kind(&self) -> &'static str;
    fn api(&self) -> &Api<Self::Crd>;

    async fn new() -> Result<Self> {
        let k8s_client = kube::Client::try_default()
            .await
            .context(error::InitializationSnafu)?;
        Ok(Self::new_from_k8s_client(k8s_client))
    }

    fn new_from_k8s_client(k8s_client: kube::Client) -> Self {
        Self::new_from_api(Self::create_api(k8s_client))
    }

    fn create_api(k8s_client: kube::Client) -> Api<Self::Crd> {
        Api::<Self::Crd>::namespaced(k8s_client, NAMESPACE)
    }

    async fn get<S>(&self, name: S) -> Result<Self::Crd>
    where
        S: AsRef<str> + Send,
    {
        let name: &str = name.as_ref();
        Ok(self
            .api()
            .get(name)
            .await
            .context(error::KubeApiCallSnafu {
                method: "get",
                what: self.kind(),
            })?)
    }

    async fn get_all(&self) -> Result<Vec<Self::Crd>> {
        Ok(self
            .api()
            .list(&ListParams::default())
            .await
            .context(error::KubeApiCallForSnafu {
                operation: "get all",
                name: format!("{}s", self.kind()),
            })?
            .items)
    }

    async fn create(&self, crd: Self::Crd) -> Result<Self::Crd> {
        Ok(self
            .api()
            .create(&PostParams::default(), &crd)
            .await
            .context(error::KubeApiCallSnafu {
                method: "create",
                what: self.kind(),
            })?)
    }

    async fn delete<S>(&self, name: S) -> Result<Option<Self::Crd>>
    where
        S: AsRef<str> + Send,
    {
        let name: &str = name.as_ref();
        Ok(self
            .api()
            .delete(name, &Default::default())
            .await
            .context(error::KubeApiCallSnafu {
                method: "delete",
                what: self.kind(),
            })?
            .map_right(|_| None)
            .map_left(Some)
            .into_inner())
    }

    async fn delete_all(&self) -> Result<Option<Vec<Self::Crd>>> {
        Ok(self
            .api()
            .delete_collection(&Default::default(), &Default::default())
            .await
            .context(error::KubeApiCallSnafu {
                method: "delete_collection",
                what: self.kind(),
            })?
            .map_right(|_| None)
            .map_left(|deleted_test| Some(deleted_test.items))
            .into_inner())
    }

    /// Loop until `get(name)` returns `StatusCode::NOT_FOUND`
    async fn wait_for_deletion<S>(&self, name: S) -> ()
    where
        S: AsRef<str> + Send,
    {
        let name: &str = name.as_ref();
        loop {
            if let Err(err) = self.api().get(name).await {
                if err.status_code() == Some(StatusCode::NOT_FOUND) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// If the `status` field is null, this will populate it with a default-constructed
    /// instantiation of the `CrdStatus` type. This is helpful so that subsequent status patches can
    /// assume `status` and its required sub-paths are not null. This will return an error if the
    /// `status` field is not `null`.
    async fn initialize_status(&self, name: &str) -> Result<Self::Crd> {
        trace!("initializing status for {} '{}'", self.kind(), name);
        self.patch_status(
            name,
            vec![
                JsonPatch::new_test_operation("/status", Option::<Self::CrdStatus>::None),
                JsonPatch::new_add_operation("/status", Self::CrdStatus::default()),
            ],
            "initialize status",
        )
        .await
    }

    /// Add a finalizer. Checks `crd` to make sure the finalizer is not a duplicate. Replaces the
    /// finalizer array with those found in `crd` plus the new `finalizer`.
    async fn add_finalizer(&self, finalizer: &str, crd: &Self::Crd) -> Result<Self::Crd> {
        trace!("adding finalizer {} for {}", finalizer, crd.object_name());

        // Initialize finalizer array if it doesn't exist.
        if !crd.has_finalizers() {
            self.patch(
                crd.object_name(),
                vec![
                    JsonPatch::new_test_operation("/metadata/finalizers", Value::Null),
                    JsonPatch::new_add_operation("/metadata/finalizers", vec![finalizer]),
                ],
                "add finalizer",
            )
            .await
        } else {
            ensure!(
                !crd.has_finalizer(finalizer),
                error::DuplicateFinalizerSnafu { finalizer }
            );
            self.patch(
                crd.object_name(),
                vec![
                    JsonPatch::new_test_operation(
                        "/metadata/finalizers",
                        crd.meta().finalizers.clone(),
                    ),
                    JsonPatch::new_add_operation("/metadata/finalizers/-", finalizer),
                ],
                "add finalizer",
            )
            .await
        }
    }

    /// Remove a finalizer. Checks `crd` to make sure the finalizer actually existed. Replaces the
    /// finalizer array with those found in `crd` minus the removed `finalizer`.
    async fn remove_finalizer(&self, finalizer: &str, crd: &Self::Crd) -> Result<Self::Crd> {
        trace!("removing finalizer {} for {}", finalizer, crd.object_name());

        let finalizer_idx = crd
            .finalizer_position(finalizer)
            .context(error::DeleteMissingFinalizerSnafu { finalizer })?;

        self.patch(
            crd.object_name(),
            vec![
                JsonPatch::new_test_operation(
                    format!("/metadata/finalizers/{}", finalizer_idx),
                    finalizer,
                ),
                JsonPatch::new_remove_operation(format!("/metadata/finalizers/{}", finalizer_idx)),
            ],
            "remove finalizer",
        )
        .await
    }

    /// Apply JSON patches to the object anywhere that is not in the `/status` path.
    async fn patch<I, S1, S2>(&self, name: S1, patches: I, description: S2) -> Result<Self::Crd>
    where
        S1: AsRef<str> + Send,
        S2: Into<String> + Send,
        I: IntoIterator<Item = JsonPatch> + Send,
    {
        let name = name.as_ref();
        let patch = json_patch::Patch(
            patches
                .into_iter()
                .map(|item| item.into_json_patch_operation())
                .collect(),
        );
        Ok(self
            .api()
            .patch(
                name,
                &PatchParams::default(),
                &Patch::<Self::Crd>::Json(patch),
            )
            .await
            .context(error::KubeApiCallForSnafu {
                operation: description,
                name,
            })?)
    }

    /// Apply JSON patches that apply to the `/status` path.
    async fn patch_status<I, S1, S2>(
        &self,
        name: S1,
        patches: I,
        description: S2,
    ) -> Result<Self::Crd>
    where
        S1: AsRef<str> + Send,
        S2: Into<String> + Send,
        I: IntoIterator<Item = JsonPatch> + Send,
    {
        let name = name.as_ref();
        let patch = json_patch::Patch(
            patches
                .into_iter()
                .map(|item| item.into_json_patch_operation())
                .collect(),
        );
        Ok(self
            .api()
            .patch_status(
                name,
                &PatchParams::default(),
                &Patch::<Self::Crd>::Json(patch),
            )
            .await
            .context(error::KubeApiCallForSnafu {
                operation: description,
                name,
            })?)
    }
}

/// The JSON patch operation type.
#[derive(Debug, Copy, Clone)]
pub(super) enum PatchOp {
    Add,
    Replace,
    Remove,
    Test,
}

/// Information for constructing a JSON patch.
pub struct JsonPatch {
    op: PatchOp,
    path: String,
    value: serde_json::Value,
}

impl JsonPatch {
    pub fn new_add_operation<S, V>(path: S, value: V) -> Self
    where
        S: Into<String>,
        V: Serialize,
    {
        Self {
            op: PatchOp::Add,
            path: path.into(),
            value: serde_json::json!(value),
        }
    }

    pub fn new_replace_operation<S, V>(path: S, value: V) -> Self
    where
        S: Into<String>,
        V: Serialize,
    {
        Self {
            op: PatchOp::Replace,
            path: path.into(),
            value: serde_json::json!(value),
        }
    }

    pub fn new_remove_operation<S>(path: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            op: PatchOp::Remove,
            path: path.into(),
            value: Default::default(),
        }
    }

    pub fn new_test_operation<S, V>(path: S, value: V) -> Self
    where
        S: Into<String>,
        V: Serialize,
    {
        Self {
            op: PatchOp::Test,
            path: path.into(),
            value: serde_json::json!(value),
        }
    }

    pub(super) fn into_json_patch_operation(self) -> PatchOperation {
        match self.op {
            PatchOp::Add => PatchOperation::Add(AddOperation {
                path: self.path,
                value: self.value,
            }),
            PatchOp::Replace => PatchOperation::Replace(ReplaceOperation {
                path: self.path,
                value: self.value,
            }),
            PatchOp::Remove => PatchOperation::Remove(RemoveOperation { path: self.path }),
            PatchOp::Test => PatchOperation::Test(TestOperation {
                path: self.path,
                value: self.value,
            }),
        }
    }
}
