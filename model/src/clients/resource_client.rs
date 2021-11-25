use super::error::{self, Result};
use crate::clients::crd_client::JsonPatch;
use crate::clients::CrdClient;
use crate::resource::{ResourceAction, ResourceError};
use crate::{Configuration, Resource, ResourceStatus, TaskState};
use async_recursion::async_recursion;
use futures::stream::{self, StreamExt};
use kube::Api;
use log::trace;
use regex::Regex;
use serde_json::{Map, Value};
use snafu::{OptionExt, ResultExt};

const TEMPLATE_PATTERN_REGEX: &str = r"^\$\{(.+)\.(.+)\}$";

lazy_static::lazy_static! {

    static ref REGEX: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(TEMPLATE_PATTERN_REGEX).unwrap()
    };
}

/// An API Client for TestSys Resource CRD objects.
///
/// # Example
///
/// ```
///# use model::clients::{CrdClient, ResourceClient};
///# async fn no_run() {
/// let resource_client = ResourceClient::new().await.unwrap();
/// let test = resource_client.get("my-resource").await.unwrap();
///# }
/// ```
#[derive(Clone)]
pub struct ResourceClient {
    api: Api<Resource>,
}

impl ResourceClient {
    pub async fn get_agent_info<C>(&self, name: &str) -> Result<C>
    where
        C: Configuration,
    {
        let status = self.get_status(name).await?;
        let info = match status.agent_info {
            None => return Ok(C::default()),
            Some(some) => some,
        };
        Ok(C::from_map(info).context(error::ConfigSerde)?)
    }

    pub async fn send_agent_info<C>(&self, name: &str, info: C) -> Result<Resource>
    where
        C: Configuration,
    {
        trace!("patching agent info for resource '{}'", name);
        self.patch_status(
            name,
            vec![JsonPatch::new_add_operation("/status/agent_info", info)],
            "send agent info",
        )
        .await
    }

    pub async fn get_resource_request<R>(&self, name: &str) -> Result<R>
    where
        R: Configuration,
    {
        let resource = self.get(name).await?;
        let map = match resource.spec.agent.configuration {
            None => return Ok(R::default()),
            Some(some) => some,
        };
        // Add test results here.
        let map = self.resolve_templated_config(map).await?;

        Ok(R::from_map(map).context(error::ConfigSerde)?)
    }

    /// This function resolves an agents config by populating it's templated fields.
    /// An agent may use the syntax `${resource_name.field_name}` to have the field
    /// named `field_name` of the resource named `resource_name` populated in the
    /// configuration map.
    pub async fn resolve_templated_config(
        &self,
        raw_config: Map<String, Value>,
    ) -> Result<Map<String, Value>> {
        stream::iter(raw_config)
            .then(|(k, v)| async move { self.resolve_input(v).await.map(|v| (k, v)) })
            .collect::<Vec<Result<(_, _)>>>()
            .await
            .into_iter()
            .collect::<Result<Map<String, Value>>>()
    }

    pub async fn send_creation_success<R>(
        &self,
        name: &str,
        created_resource: R,
    ) -> Result<Resource>
    where
        R: Configuration,
    {
        trace!("patching creation success for resource '{}'", name);
        self.patch_status(
            name,
            vec![
                JsonPatch::new_add_operation("/status/creation/task_state", TaskState::Completed),
                JsonPatch::new_add_operation("/status/created_resource", created_resource),
            ],
            "send creation success",
        )
        .await
    }

    pub async fn get_created_resource<R>(&self, name: &str) -> Result<Option<R>>
    where
        R: Configuration,
    {
        let status = self.get_status(name).await?;
        let map = match status.created_resource {
            None => return Ok(None),
            Some(some) => some,
        };
        Ok(Some(R::from_map(map).context(error::ConfigSerde)?))
    }

    pub async fn send_error(
        &self,
        name: &str,
        resource_action: ResourceAction,
        error: &ResourceError,
    ) -> Result<Resource> {
        trace!(
            "patching {:?} error for resource '{}'",
            resource_action,
            name
        );
        let path_prefix = match resource_action {
            ResourceAction::Create => "/status/creation",
            ResourceAction::Destroy => "/status/destruction",
        };
        let error_path = format!("{}/error", path_prefix);
        let task_state_path = format!("{}/task_state", path_prefix);
        self.patch_status(
            name,
            vec![
                JsonPatch::new_add_operation(error_path, error),
                JsonPatch::new_add_operation(task_state_path, TaskState::Error),
            ],
            "send error",
        )
        .await
    }

    pub async fn send_task_state(
        &self,
        name: &str,
        op: ResourceAction,
        state: TaskState,
    ) -> Result<Resource> {
        trace!(
            "patching {:?} task state to '{:?}' for resource '{}'",
            op,
            state,
            name
        );
        let path = match op {
            ResourceAction::Create => "/status/creation/task_state",
            ResourceAction::Destroy => "/status/destruction/task_state",
        }
        .to_string();

        self.patch_status(
            name,
            vec![JsonPatch::new_add_operation(path, state)],
            "send task state",
        )
        .await
    }

    /// Get the `status` field of the `Resource`. Returns a default-constructed `ResourceStatus` if
    /// the `status` field is null.
    async fn get_status(&self, name: &str) -> Result<ResourceStatus> {
        Ok(self.get(name).await?.status.unwrap_or_default())
    }

    #[async_recursion]
    async fn resolve_input(&self, input: Value) -> Result<Value> {
        match input {
            Value::String(input_string) => self.resolve_input_string(input_string).await,
            Value::Object(map) => self
                .resolve_templated_config(map)
                .await
                .map(|map| Value::Object(map)),
            non_string_input => Ok(non_string_input),
        }
    }

    async fn resolve_input_string(&self, input: String) -> Result<Value> {
        if let Some((resource_name, field_name)) = resource_name_and_field_name(&input)? {
            let resource = self.get(resource_name).await?;
            let results = resource
                .created_resource()
                .context(error::ConfigResolution {
                    what: "Created resource missing from resource.".to_string(),
                })?;
            let updated_value = results.get(&field_name).context(error::ConfigResolution {
                what: format!("No field '{}' in created resource", field_name),
            })?;
            Ok(updated_value.to_owned())
        } else {
            Ok(Value::String(input))
        }
    }
}

fn resource_name_and_field_name(input: &str) -> Result<Option<(String, String)>> {
    let captures = match REGEX.captures(&input) {
        None => return Ok(None),
        Some(some) => some,
    };
    let resource_name = captures
        .get(1)
        .context(error::ConfigResolution {
            what: "Resource name could not be extracted from capture.".to_string(),
        })?
        .as_str();
    let field_name = captures
        .get(2)
        .context(error::ConfigResolution {
            what: "Resource value could not be extracted from capture.".to_string(),
        })?
        .as_str();
    Ok(Some((resource_name.to_string(), field_name.to_string())))
}

#[test]
fn test_pattern1() {
    let (resource_name, field_name) = resource_name_and_field_name(r"${dup1.info}")
        .unwrap()
        .unwrap();
    assert_eq!(resource_name, "dup1");
    assert_eq!(field_name, "info");
    assert!(resource_name_and_field_name(r"hello").unwrap().is_none());
    assert!(resource_name_and_field_name(r"${hello}").unwrap().is_none());
    assert!(resource_name_and_field_name(r"foo${x.y}")
        .unwrap()
        .is_none());
    assert!(resource_name_and_field_name(r"${x.y}foo")
        .unwrap()
        .is_none());
    assert!(resource_name_and_field_name(r"foo${x.y}bar")
        .unwrap()
        .is_none());
    assert!(resource_name_and_field_name(r"${.x}").unwrap().is_none());
    assert!(resource_name_and_field_name(r"${x.}").unwrap().is_none());
    assert!(resource_name_and_field_name(r"${.}").unwrap().is_none());
    assert!(resource_name_and_field_name(r"${.}").unwrap().is_none());
}

impl CrdClient for ResourceClient {
    type Crd = Resource;
    type CrdStatus = ResourceStatus;

    fn new_from_api(api: Api<Self::Crd>) -> Self {
        Self { api }
    }

    fn kind(&self) -> &'static str {
        "resource"
    }

    fn api(&self) -> &Api<Self::Crd> {
        &self.api
    }
}

#[cfg(test)]
#[cfg(feature = "integ")]
mod test {
    use super::*;
    use crate::constants::NAMESPACE;
    use crate::{Agent, CrdExt, ErrorResources, ResourceSpec};
    use k8s_openapi::api::core::v1::Namespace;
    use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
    use kube::api::PostParams;
    use kube::CustomResourceExt;
    use selftest::Cluster;
    use serde::{Deserialize, Serialize};
    use std::fmt::Debug;

    const CLUSTER_NAME: &str = "resource-client";
    const RESOURCE_NAME: &str = "my-resource";

    #[derive(Default, Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
    struct AgentInfo {
        field_a: String,
        field_b: u64,
    }

    impl Configuration for AgentInfo {}

    #[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
    struct RobotRequest {
        robot_lucky_number: u64,
        robot_unlucky_number: u64,
    }

    impl Configuration for RobotRequest {}

    const ROBOT_REQUEST: RobotRequest = RobotRequest {
        robot_lucky_number: 7,
        robot_unlucky_number: 13,
    };

    #[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
    struct CreatedRobot {
        instance_id: u64,
    }

    impl Configuration for CreatedRobot {}

    const CREATED_ROBOT: CreatedRobot = CreatedRobot {
        instance_id: 123456,
    };

    #[tokio::test]
    async fn test() {
        let cluster = Cluster::new(CLUSTER_NAME).unwrap();
        let k8s_client = cluster.k8s_client().await.unwrap();
        let ns_api: Api<Namespace> = Api::all(k8s_client.clone());
        ns_api
            .create(&PostParams::default(), &crate::system::testsys_namespace())
            .await
            .unwrap();
        cluster
            .wait_for_object::<Namespace>(NAMESPACE, None, tokio::time::Duration::from_secs(10))
            .await
            .unwrap();
        let crd_api: Api<CustomResourceDefinition> = Api::all(k8s_client.clone());
        crd_api
            .create(&PostParams::default(), &Resource::crd())
            .await
            .unwrap();
        cluster
            .wait_for_object::<CustomResourceDefinition>(
                "resources.testsys.bottlerocket.aws",
                None,
                tokio::time::Duration::from_secs(10),
            )
            .await
            .unwrap();
        let rc = ResourceClient::new_from_k8s_client(cluster.k8s_client().await.unwrap());

        rc.create(Resource {
            metadata: ObjectMeta {
                name: Some(RESOURCE_NAME.into()),
                ..ObjectMeta::default()
            },
            spec: ResourceSpec {
                agent: Agent {
                    name: "my-agent".into(),
                    image: "foo:v0.1.0".into(),
                    configuration: Some(ROBOT_REQUEST.into_map().unwrap()),
                    ..Agent::default()
                },
                ..Default::default()
            },
            ..Resource::default()
        })
        .await
        .unwrap();

        rc.initialize_status(RESOURCE_NAME).await.unwrap();

        // If status is already initialized, it should be an error to do so again.
        assert!(rc.initialize_status(RESOURCE_NAME).await.is_err());

        assert!(matches!(
            rc.get_status(RESOURCE_NAME)
                .await
                .unwrap()
                .creation
                .task_state,
            TaskState::Unknown
        ));

        assert_eq!(
            rc.get_agent_info::<AgentInfo>(RESOURCE_NAME).await.unwrap(),
            AgentInfo::default()
        );
        let expected_agent_info = AgentInfo {
            field_a: "foo".to_string(),
            field_b: 42,
        };
        rc.send_agent_info(RESOURCE_NAME, expected_agent_info.clone())
            .await
            .unwrap();
        let actual_agent_info: AgentInfo = rc.get_agent_info(RESOURCE_NAME).await.unwrap();
        assert_eq!(expected_agent_info, actual_agent_info);

        assert_eq!(
            ROBOT_REQUEST,
            rc.get_resource_request(RESOURCE_NAME).await.unwrap()
        );

        rc.send_task_state(RESOURCE_NAME, ResourceAction::Create, TaskState::Running)
            .await
            .unwrap();
        assert!(matches!(
            rc.get_status(RESOURCE_NAME)
                .await
                .unwrap()
                .creation
                .task_state,
            TaskState::Running,
        ));

        rc.send_task_state(RESOURCE_NAME, ResourceAction::Destroy, TaskState::Running)
            .await
            .unwrap();
        assert!(matches!(
            rc.get_status(RESOURCE_NAME)
                .await
                .unwrap()
                .destruction
                .task_state,
            TaskState::Running,
        ));

        assert!(rc
            .get_created_resource::<CreatedRobot>(RESOURCE_NAME)
            .await
            .unwrap()
            .is_none());

        rc.send_creation_success(RESOURCE_NAME, CREATED_ROBOT)
            .await
            .unwrap();

        assert_eq!(
            rc.get_created_resource::<CreatedRobot>(RESOURCE_NAME)
                .await
                .unwrap()
                .unwrap(),
            CREATED_ROBOT
        );

        let status = rc.get_status(RESOURCE_NAME).await.unwrap();
        assert!(status.creation.error.is_none());
        assert!(matches!(status.creation.task_state, TaskState::Completed));
        assert!(status.destruction.error.is_none());
        assert!(matches!(status.destruction.task_state, TaskState::Running));

        let create_error = ResourceError {
            error: "c".to_string(),
            error_resources: ErrorResources::Clear,
        };
        rc.send_error(RESOURCE_NAME, ResourceAction::Create, &create_error)
            .await
            .unwrap();
        let status = rc.get_status(RESOURCE_NAME).await.unwrap();
        assert_eq!(status.creation.error.unwrap(), create_error);
        assert!(matches!(status.creation.task_state, TaskState::Error));
        assert!(status.destruction.error.is_none());
        assert!(matches!(status.destruction.task_state, TaskState::Running));

        let destroy_error = ResourceError {
            error: "d".to_string(),
            error_resources: ErrorResources::Orphaned,
        };
        rc.send_error(RESOURCE_NAME, ResourceAction::Destroy, &destroy_error)
            .await
            .unwrap();
        let status = rc.get_status(RESOURCE_NAME).await.unwrap();
        assert_eq!(status.creation.error.unwrap(), create_error);
        assert!(matches!(status.creation.task_state, TaskState::Error));
        assert_eq!(status.destruction.error.unwrap(), destroy_error);
        assert!(matches!(status.destruction.task_state, TaskState::Error));

        // Add a finalizer
        rc.add_finalizer("foobar", &rc.get(RESOURCE_NAME).await.unwrap())
            .await
            .unwrap();

        // The finalizer is present
        assert!(rc.get(RESOURCE_NAME).await.unwrap().has_finalizer("foobar"));

        // Cannot add the finalizer twice
        assert!(rc
            .add_finalizer("foobar", &rc.get(RESOURCE_NAME).await.unwrap())
            .await
            .is_err());

        // Remove the finalizer
        rc.remove_finalizer("foobar", &rc.get(RESOURCE_NAME).await.unwrap())
            .await
            .unwrap();

        // No longer present
        assert!(!rc.get(RESOURCE_NAME).await.unwrap().has_finalizer("foobar"));

        // Cannot remove it if it is not present
        assert!(rc
            .remove_finalizer("foobar", &rc.get(RESOURCE_NAME).await.unwrap())
            .await
            .is_err());
    }
}
