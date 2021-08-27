use crate::model::{NAMESPACE, TESTSYS, TEST_AGENT_SERVICE_ACCOUNT};
use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, PolicyRule, RoleRef, Subject};
use kube::api::ObjectMeta;
use maplit::btreemap;

const TESTSYS_AGENT_ROLE: &str = "testsys-agent-role";

/// Defines the testsys-agent service account
pub fn agent_service_account() -> ServiceAccount {
    ServiceAccount {
        metadata: ObjectMeta {
            name: Some(TEST_AGENT_SERVICE_ACCOUNT.to_string()),
            namespace: Some(NAMESPACE.to_string()),
            annotations: Some(btreemap! {
                "kubernetes.io/service-account.name".to_string() => TEST_AGENT_SERVICE_ACCOUNT.to_string()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Defines the testsys-agent cluster role
pub fn agent_cluster_role() -> ClusterRole {
    ClusterRole {
        metadata: ObjectMeta {
            name: Some(TESTSYS_AGENT_ROLE.to_string()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        rules: Some(vec![PolicyRule {
            api_groups: Some(vec![TESTSYS.to_string()]),
            resources: Some(vec!["tests".to_string(), "tests/status".to_string()]),
            verbs: vec!["get", "list", "patch", "update", "watch"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ..Default::default()
        }]),
        ..Default::default()
    }
}

/// Defines the testsys-agent cluster role binding
pub fn agent_cluster_role_binding() -> ClusterRoleBinding {
    ClusterRoleBinding {
        metadata: ObjectMeta {
            name: Some("testsys-agent-role-binding".to_string()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        role_ref: RoleRef {
            kind: "ClusterRole".to_string(),
            name: TESTSYS_AGENT_ROLE.to_string(),
            api_group: "rbac.authorization.k8s.io".to_string(),
        },
        subjects: Some(vec![Subject {
            kind: "ServiceAccount".to_string(),
            name: TEST_AGENT_SERVICE_ACCOUNT.to_string(),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        }]),
    }
}
