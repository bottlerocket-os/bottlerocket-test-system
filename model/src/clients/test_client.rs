use crate::model::{
    AgentStatus, Configuration, ConfigurationError, ControllerStatus, ErrorResources,
    ResourceAgentState, ResourceRequest, ResourceStatus, Test, API_VERSION, NAMESPACE, TESTSYS,
};
use kube::api::{Patch, PatchParams};
use kube::{Api, Resource};
use log::trace;
use serde::Serialize;
use serde_json::{json, Map, Value};
use snafu::{ResultExt, Snafu};

/// An API Client for TestSys Test CRD objects.
///
/// # Example
///
/// ```
///# use model::clients::TestClient;
///# async fn no_run() {
/// let test_client = TestClient::new().await.unwrap();
/// let test = test_client.get_test("my-test").await.unwrap();
///# }
/// ```
#[derive(Clone)]
pub struct TestClient {
    api: Api<Test>,
}

/// The `Result` type returned by [`TestClient`].
pub type Result<T> = std::result::Result<T, Error>;

/// The public error type for `TestClient`.
#[derive(Debug, Snafu)]
pub struct Error(InnerError);

/// The private error type for `TestClient`.
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    #[snafu(display("{}", source))]
    ConfigSerde { source: ConfigurationError },

    #[snafu(display("Error serializing object '{}': {}", what, source))]
    Serde {
        what: String,
        source: serde_json::Error,
    },

    #[snafu(display("Error initializing the Kubernetes client: {}", source))]
    Initialization { source: kube::Error },

    #[snafu(display("Unable to {} {}: {}", method, what, source))]
    KubeApiCall {
        method: String,
        what: String,
        source: kube::Error,
    },
}

impl From<ConfigurationError> for Error {
    fn from(e: ConfigurationError) -> Self {
        Error(InnerError::ConfigSerde { source: e })
    }
}

impl TestClient {
    /// Create a new [`TestClient`] using either `KUBECONFIG` or the in-cluster environment
    /// variables.
    pub async fn new() -> Result<Self> {
        let k8s_client = kube::Client::try_default().await.context(Initialization)?;
        Ok(Self::new_from_k8s_client(k8s_client))
    }

    /// Create a new [`TestClient`] from an existing k8s client.
    pub fn new_from_k8s_client(k8s_client: kube::Client) -> Self {
        Self {
            api: Api::<Test>::namespaced(k8s_client, NAMESPACE),
        }
    }

    /// Get the TestSys [`Test`].
    pub async fn get_test<S>(&self, name: S) -> Result<Test>
    where
        S: AsRef<str>,
    {
        Ok(self.api.get(name.as_ref()).await.context(KubeApiCall {
            method: "get",
            what: "test",
        })?)
    }

    pub async fn get_resource_request(
        &self,
        test_name: &str,
        resource_name: &str,
    ) -> Result<Option<ResourceRequest>> {
        let mut test = self.get_test(test_name).await?;
        Ok(test.spec.resources.remove(resource_name))
    }

    /// Get the TestSys [`Test`]'s `status.agent` field.
    pub async fn get_agent_status<S>(&self, name: S) -> Result<AgentStatus>
    where
        S: AsRef<str>,
    {
        Ok(self
            .get_test(name)
            .await?
            .status
            .unwrap_or_else(|| Default::default())
            .agent
            .unwrap_or_else(|| Default::default()))
    }

    /// Get the TestSys [`Test`] `status.resource` field for `resource_name`.
    pub async fn get_resource_status<S1, S2>(
        &self,
        test: S1,
        resource: S2,
    ) -> Result<Option<ResourceStatus>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        Ok(self
            .get_test(test)
            .await?
            .status
            .unwrap_or_else(|| Default::default())
            .resources
            .and_then(|mut some| some.remove(resource.as_ref())))
    }

    /// Set the TestSys [`Test`]'s `status.agent` field. Returns the updated [`Test`].
    pub async fn set_agent_status<S>(&self, name: S, status: AgentStatus) -> Result<Test>
    where
        S: AsRef<str>,
    {
        let json = Self::create_patch("status", "agent", status);
        self.send_merge_patch(&name, json, "agent status").await
    }

    /// Set the TestSys [`Test`]'s `status.controller` field. Returns the updated [`Test`].
    pub async fn set_controller_status<S>(&self, name: S, status: ControllerStatus) -> Result<Test>
    where
        S: AsRef<str>,
    {
        let json = Self::create_patch("status", "controller", status);
        self.send_merge_patch(&name, json, "controller status")
            .await
    }

    /// Set the `agent_info` field of the given `resource`'s status entry.
    pub async fn set_resource_agent_info<I>(
        &self,
        name: &str,
        resource: &str,
        info: I,
    ) -> Result<Test>
    where
        I: Configuration,
    {
        self.ensure_resource_status_init(&name, &resource).await?;
        self.patch_resource_status_fields(
            &name,
            &resource,
            None,
            Some(info.into_map()?),
            None,
            None,
            None,
        )
        .await
    }

    /// Set the `agent_state` field of the given `resource`'s status entry.
    pub async fn set_resource_agent_state<S1, S2>(
        &self,
        name: S1,
        resource: S2,
        state: ResourceAgentState,
    ) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        self.ensure_resource_status_init(&name, &resource).await?;
        self.patch_resource_status_fields(
            name.as_ref(),
            resource.as_ref(),
            Some(state),
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Set the `agent_state` field to `Created` and the `created_resource` field of the given `resource`'s status entry.
    pub async fn set_resource_created<S1, S2, R>(
        &self,
        test_name: S1,
        resource_name: S2,
        created_resource: R,
    ) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
        R: Configuration,
    {
        self.ensure_resource_status_init(&test_name, &resource_name)
            .await?;
        self.patch_resource_status_fields(
            test_name.as_ref(),
            resource_name.as_ref(),
            Some(ResourceAgentState::Created),
            None,
            None,
            None,
            Some(created_resource.into_map()?),
        )
        .await
    }

    /// Set the `agent_state` and `error_message` fields of the given `resource`'s status entry.
    pub async fn set_resource_agent_error<S1, S2, S3>(
        &self,
        test_name: S1,
        resource_name: S2,
        state: ResourceAgentState,
        error_message: S3,
        error_resources: ErrorResources,
    ) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
        S3: AsRef<str>,
    {
        self.ensure_resource_status_init(&test_name, &resource_name)
            .await?;
        self.patch_resource_status_fields(
            test_name.as_ref(),
            resource_name.as_ref(),
            Some(state),
            None,
            Some(error_message.as_ref()),
            Some(error_resources),
            None,
        )
        .await
    }

    /// Add a k8s 'finalizer' to the [`Test`] object's metadata.
    pub async fn add_finalizer<S1, S2>(&self, test_name: S1, finalizer_name: S2) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let finalizer = Self::create_finalizer(finalizer_name);
        let mut finalizers: Vec<String> = self
            .get_test(&test_name)
            .await?
            .meta()
            .finalizers
            .to_owned()
            .unwrap_or(Vec::new());
        finalizers.push(finalizer);
        let json = Self::create_patch("metadata", "finalizers", &finalizers);
        let patch: Patch<&Value> = Patch::Merge(&json);
        self.patch(&test_name, patch, "finalizers").await
    }

    pub async fn remove_finalizer<S1, S2>(&self, test_name: S1, finalizer_name: S2) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let finalizer = Self::create_finalizer(&finalizer_name);
        trace!("removing finalizer: {}", finalizer);
        let mut finalizers: Vec<String> = self
            .get_test(&test_name)
            .await?
            .meta()
            .finalizers
            .to_owned()
            .unwrap_or(Vec::new());
        finalizers.retain(|item| item.as_str() != finalizer.as_str());
        let json = Self::create_patch("metadata", "finalizers", &finalizers);
        let patch: Patch<&Value> = Patch::Merge(&json);
        self.patch(&test_name, patch, "finalizers").await
    }

    /// Helper for finding a given finalizer in a `Test` object.
    pub fn has_finalizer<S: AsRef<str>>(test: &Test, finalizer_name: S) -> bool {
        test.meta()
            .finalizers
            .as_ref()
            .unwrap_or(&Vec::new())
            .contains(&Self::create_finalizer(finalizer_name))
    }

    /// Get a clone of the [`kube::Api`].
    pub fn api(&self) -> kube::Api<Test> {
        self.api.clone()
    }

    /// Private helper to create a domain-like finalizer name. For example, given "foo", returns
    /// "foo.finalizers.bottlerocket.aws".
    fn create_finalizer<S: AsRef<str>>(finalizer_name: S) -> String {
        format!("{}.finalizers.{}", finalizer_name.as_ref(), TESTSYS)
    }

    /// Creates the JSON object used in an HTTP PATCH operation.
    fn create_patch<K1, K2, T>(top_key: K1, sub_key: K2, value: T) -> Value
    where
        K1: AsRef<str>,
        K2: AsRef<str>,
        T: Serialize,
    {
        json!({
            "apiVersion": API_VERSION,
            "kind": "Test",
            top_key.as_ref(): {
                sub_key.as_ref(): value
            }
        })
    }

    /// Applies a change to a `Test` object with an HTTP PATCH operation.
    async fn patch<S>(&self, test_name: S, patch: Patch<&Value>, what: &str) -> Result<Test>
    where
        S: AsRef<str>,
    {
        Ok(self
            .api
            .patch(test_name.as_ref(), &PatchParams::default(), &patch)
            .await
            .context(KubeApiCall {
                method: "patch",
                what,
            })?)
    }

    /// Applies a patch to the `Test` using the `Merge` method.
    async fn send_merge_patch<S>(&self, test_name: S, json: Value, what: &str) -> Result<Test>
    where
        S: AsRef<str>,
    {
        Ok(self
            .api
            .patch_status(
                test_name.as_ref(),
                &PatchParams::default(),
                &Patch::Merge(json),
            )
            .await
            .context(KubeApiCall {
                method: "patch",
                what,
            })?)
    }

    /// This function makes sure the `status.resources.[resource]` is initialized.
    ///
    /// Some of the `status.resources.[resource]` fields are required. Because of this, we must
    /// first initialize the required fields before we can surgically patch the desired fields.
    async fn ensure_resource_status_init<S1, S2>(&self, name: S1, resource: S2) -> Result<()>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        if self.get_resource_status(&name, &resource).await?.is_none() {
            // The map entry for our resource does not exist, instantiate it with default values.
            let json = resources_status_patch_initialize(resource.as_ref())?;
            self.send_merge_patch(name.as_ref(), json, "resource status")
                .await?;
        }
        Ok(())
    }

    /// Surgically patch a resource status updating only the desired fields. Fields left as `None`
    /// will not be updated.
    async fn patch_resource_status_fields(
        &self,
        test_name: &str,
        resource_name: &str,
        agent_state: Option<ResourceAgentState>,
        agent_info: Option<Map<String, Value>>,
        error: Option<&str>,
        error_resources: Option<ErrorResources>,
        created_resource: Option<Map<String, Value>>,
    ) -> Result<Test> {
        self.ensure_resource_status_init(test_name, resource_name)
            .await?;
        let json = resource_status_patch_surgical(
            resource_name,
            agent_state,
            agent_info,
            error,
            error_resources,
            created_resource,
        )?;
        self.send_merge_patch(test_name, json, "resource status")
            .await
    }
}

/// Build a patch for a resource status including only the fields that we desire to update.
fn resource_status_patch_surgical(
    resource_name: &str,
    agent_state: Option<ResourceAgentState>,
    agent_info: Option<Map<String, Value>>,
    error: Option<&str>,
    error_resources: Option<ErrorResources>,
    created_resource: Option<Map<String, Value>>,
) -> Result<Value> {
    let mut map = Map::new();

    if let Some(agent_state) = agent_state {
        map.insert(
            "agent_state".into(),
            serde_json::to_value(agent_state).context(Serde {
                what: "ResourceAgentState",
            })?,
        );
    }

    if let Some(agent_info) = agent_info {
        map.insert("agent_info".into(), Value::Object(agent_info));
    }

    if let Some(error) = error {
        map.insert("error".into(), Value::String(error.into()));
    }

    if let Some(error_resources) = error_resources {
        map.insert(
            "error_resources".into(),
            serde_json::to_value(error_resources).context(Serde {
                what: "ErrorResources",
            })?,
        );
    }

    if let Some(created_resource) = created_resource {
        map.insert("created_resource".into(), Value::Object(created_resource));
    }
    Ok(create_resource_status_patch_json(
        resource_name,
        Value::Object(map),
    ))
}

/// Create the JSON needed to initialize a resource status.
fn resources_status_patch_initialize(resource_name: &str) -> Result<Value> {
    Ok(create_resource_status_patch_json(
        resource_name,
        serde_json::to_value(ResourceStatus::default()).context(Serde {
            what: "ResourceStatus",
        })?,
    ))
}

/// Place the `resource_status` JSON into the right place in order to patch a `Test` object.
fn create_resource_status_patch_json(resource_name: &str, resource_status: Value) -> Value {
    let json = json!({
        "apiVersion": API_VERSION,
        "kind": "Test",
        "status": {
            "resources": {
                resource_name: resource_status
            }
        }
    });
    println!("\n{},", json.to_string());
    json
}
