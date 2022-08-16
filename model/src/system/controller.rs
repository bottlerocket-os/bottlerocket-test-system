use crate::constants::{
    APP_COMPONENT, APP_MANAGED_BY, APP_PART_OF, LABEL_COMPONENT, NAMESPACE, TESTSYS,
};
use k8s_openapi::api::apps::v1::{
    Deployment, DeploymentSpec, DeploymentStrategy, RollingUpdateDeployment,
};
use k8s_openapi::api::core::v1::{
    Affinity, Container, LocalObjectReference, NodeAffinity, NodeSelector, NodeSelectorRequirement,
    NodeSelectorTerm, PodSpec, PodTemplateSpec, ServiceAccount,
};
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, PolicyRule, RoleRef, Subject};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::ObjectMeta;
use maplit::btreemap;

const TESTSYS_CONTROLLER_SERVICE_ACCOUNT: &str = "testsys-controller-service-account";
const TESTSYS_CONTROLLER_CLUSTER_ROLE: &str = "testsys-controller-role";

/// Defines the testsys-controller service account
pub fn controller_service_account() -> ServiceAccount {
    ServiceAccount {
        metadata: ObjectMeta {
            name: Some(TESTSYS_CONTROLLER_SERVICE_ACCOUNT.to_string()),
            namespace: Some(NAMESPACE.to_string()),
            annotations: Some(btreemap! {
                "kubernetes.io/service-account.name".to_string() => TESTSYS_CONTROLLER_SERVICE_ACCOUNT.to_string()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Defines the testsys-controller cluster role
pub fn controller_cluster_role() -> ClusterRole {
    ClusterRole {
        metadata: ObjectMeta {
            name: Some(TESTSYS_CONTROLLER_CLUSTER_ROLE.to_string()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        rules: Some(vec![
            PolicyRule {
                api_groups: Some(vec![TESTSYS.to_string()]),
                resources: Some(vec!["tests".to_string(), "tests/status".to_string()]),
                verbs: vec![
                    "create",
                    "delete",
                    "deletecollection",
                    "get",
                    "list",
                    "patch",
                    "update",
                    "watch",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                ..Default::default()
            },
            PolicyRule {
                api_groups: Some(vec![TESTSYS.to_string()]),
                resources: Some(vec![
                    "resources".to_string(),
                    "resources/status".to_string(),
                ]),
                verbs: vec![
                    "create",
                    "delete",
                    "deletecollection",
                    "get",
                    "list",
                    "patch",
                    "update",
                    "watch",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                ..Default::default()
            },
            PolicyRule {
                api_groups: Some(vec!["apps".to_string()]),
                resources: Some(vec!["deployments".to_string()]),
                verbs: vec![
                    "create",
                    "delete",
                    "deletecollection",
                    "get",
                    "list",
                    "patch",
                    "update",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                ..Default::default()
            },
            PolicyRule {
                api_groups: Some(vec!["batch".to_string()]),
                resources: Some(vec!["jobs".to_string()]),
                verbs: vec![
                    "create",
                    "delete",
                    "deletecollection",
                    "get",
                    "list",
                    "patch",
                    "update",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                ..Default::default()
            },
        ]),
        ..Default::default()
    }
}

/// Defines the testsys-controller cluster role binding
pub fn controller_cluster_role_binding() -> ClusterRoleBinding {
    ClusterRoleBinding {
        metadata: ObjectMeta {
            name: Some("testsys-controller-role-binding".to_string()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        role_ref: RoleRef {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "ClusterRole".to_string(),
            name: TESTSYS_CONTROLLER_CLUSTER_ROLE.to_string(),
        },
        subjects: Some(vec![Subject {
            kind: "ServiceAccount".to_string(),
            name: TESTSYS_CONTROLLER_SERVICE_ACCOUNT.to_string(),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        }]),
    }
}

/// Defines the testsys-controller deployment
pub fn controller_deployment(
    controller_image: String,
    image_pull_secret: Option<String>,
) -> Deployment {
    let image_pull_secrets =
        image_pull_secret.map(|secret| vec![LocalObjectReference { name: Some(secret) }]);

    Deployment {
        metadata: ObjectMeta {
            labels: Some(
                btreemap! {
                    APP_COMPONENT => "controller",
                    APP_MANAGED_BY => "testsys",
                    APP_PART_OF => "testsys",
                    LABEL_COMPONENT => "controller",
                }
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ),
            name: Some("testsys-controller".to_string()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector {
                match_labels: Some(
                    btreemap! { LABEL_COMPONENT.to_string() => "controller".to_string()},
                ),
                ..Default::default()
            },
            strategy: Some(DeploymentStrategy {
                rolling_update: Some(RollingUpdateDeployment {
                    max_unavailable: Some(IntOrString::String("100%".to_string())),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(btreemap! {
                        LABEL_COMPONENT.to_string() => "controller".to_string(),
                    }),
                    namespace: Some(NAMESPACE.to_string()),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    affinity: Some(Affinity {
                        node_affinity: Some(NodeAffinity {
                            required_during_scheduling_ignored_during_execution: Some(
                                NodeSelector {
                                    node_selector_terms: vec![NodeSelectorTerm {
                                        match_expressions: Some(vec![NodeSelectorRequirement {
                                            key: "kubernetes.io/os".to_string(),
                                            operator: "In".to_string(),
                                            values: Some(vec!["linux".to_string()]),
                                        }]),
                                        ..Default::default()
                                    }],
                                },
                            ),
                            ..Default::default()
                        }),
                        // TODO: Potentially add pods we want to avoid here, e.g. update operator agent pod
                        pod_anti_affinity: None,
                        ..Default::default()
                    }),
                    containers: vec![Container {
                        image: Some(controller_image),
                        image_pull_policy: None,
                        name: "controller".to_string(),
                        ..Default::default()
                    }],
                    image_pull_secrets,
                    service_account_name: Some(TESTSYS_CONTROLLER_SERVICE_ACCOUNT.to_string()),
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    }
}
