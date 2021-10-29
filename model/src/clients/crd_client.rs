use crate::clients::error::{self, Result};
use crate::constants::NAMESPACE;
use crate::CrdExt;
use core::fmt::Debug;
use json_patch::{AddOperation, PatchOperation, ReplaceOperation, TestOperation};
use kube::api::{ListParams, Patch, PatchParams, PostParams};
use kube::Api;
use log::trace;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::{ensure, ResultExt};
use std::collections::HashSet;

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
            .context(error::Initialization)?;
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
        Ok(self.api().get(name).await.context(error::KubeApiCall {
            method: "get",
            what: self.kind(),
        })?)
    }

    async fn get_all(&self) -> Result<Vec<Self::Crd>> {
        Ok(self
            .api()
            .list(&ListParams::default())
            .await
            .context(error::KubeApiCallFor {
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
            .context(error::KubeApiCall {
                method: "create",
                what: self.kind(),
            })?)
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

        let mut finalizers = crd.finalizer_set();
        ensure!(
            finalizers.insert(finalizer.to_owned()),
            error::DuplicateFinalizer { finalizer }
        );

        self.patch(
            crd.object_name(),
            vec![JsonPatch::new_replace_operation(
                "/metadata/finalizers",
                finalizers,
            )],
            "add finalizer",
        )
        .await
    }

    /// Remove a finalizer. Checks `crd` to make sure the finalizer actually existed. Replaces the
    /// finalizer array with those found in `crd` minus the removed `finalizer`.
    async fn remove_finalizer(&self, finalizer: &str, crd: &Self::Crd) -> Result<Self::Crd> {
        trace!("removing finalizer {} for {}", finalizer, crd.object_name());

        let mut finalizers: HashSet<String> = crd.finalizer_set();
        ensure!(
            finalizers.remove(finalizer),
            error::DeleteMissingFinalizer { finalizer }
        );

        self.patch(
            crd.object_name(),
            vec![JsonPatch::new_replace_operation(
                "/metadata/finalizers",
                finalizers,
            )],
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
            .context(error::KubeApiCallFor {
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
            .context(error::KubeApiCallFor {
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
            PatchOp::Test => PatchOperation::Test(TestOperation {
                path: self.path,
                value: self.value,
            }),
        }
    }
}