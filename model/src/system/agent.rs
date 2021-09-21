use crate::constants::{
    NAMESPACE, RESOURCE_AGENT_BINDING, RESOURCE_AGENT_ROLE, RESOURCE_AGENT_SERVICE_ACCOUNT,
    TESTSYS, TEST_AGENT_BINDING, TEST_AGENT_ROLE, TEST_AGENT_SERVICE_ACCOUNT,
};
use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, PolicyRule, RoleRef, Subject};
use kube::api::ObjectMeta;
use maplit::btreemap;

#[derive(Debug, Clone, Copy)]
pub enum AgentType {
    Test,
    Resource,
}

/// Defines the service account for an agent of type `agent_type`.
pub fn agent_service_account(agent_type: AgentType) -> ServiceAccount {
    ServiceAccount {
        metadata: ObjectMeta {
            name: Some(agent_type.service_account_name()),
            namespace: Some(NAMESPACE.to_string()),
            annotations: Some(btreemap! {
                "kubernetes.io/service-account.name".to_string() => agent_type.service_account_name()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Defines the cluster role for an agent of type `agent_type`.
pub fn agent_cluster_role(agent_type: AgentType) -> ClusterRole {
    ClusterRole {
        metadata: ObjectMeta {
            name: Some(agent_type.role_name()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        rules: Some(agent_type.policy_rules()),
        ..Default::default()
    }
}

/// Defines the cluster role binding for an agent of type `agent_type`.
pub fn agent_cluster_role_binding(agent_type: AgentType) -> ClusterRoleBinding {
    ClusterRoleBinding {
        metadata: ObjectMeta {
            name: Some(agent_type.binding_name()),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        },
        role_ref: RoleRef {
            kind: "ClusterRole".to_string(),
            name: agent_type.role_name(),
            api_group: "rbac.authorization.k8s.io".to_string(),
        },
        subjects: Some(vec![Subject {
            kind: "ServiceAccount".to_string(),
            name: agent_type.service_account_name(),
            namespace: Some(NAMESPACE.to_string()),
            ..Default::default()
        }]),
    }
}

impl AgentType {
    fn role_name(&self) -> String {
        match self {
            AgentType::Test => TEST_AGENT_ROLE.to_string(),
            AgentType::Resource => RESOURCE_AGENT_ROLE.to_string(),
        }
    }

    fn service_account_name(&self) -> String {
        match self {
            AgentType::Test => TEST_AGENT_SERVICE_ACCOUNT.to_string(),
            AgentType::Resource => RESOURCE_AGENT_SERVICE_ACCOUNT.to_string(),
        }
    }

    fn binding_name(&self) -> String {
        match self {
            AgentType::Test => TEST_AGENT_BINDING.to_string(),
            AgentType::Resource => RESOURCE_AGENT_BINDING.to_string(),
        }
    }

    fn managed_resource_plural_name(&self) -> &'static str {
        match self {
            AgentType::Test => "tests",
            AgentType::Resource => "resources",
        }
    }

    fn policy_rules(&self) -> Vec<PolicyRule> {
        let managed_resource = self.managed_resource_plural_name().to_string();
        let managed_status = format!("{}/status", managed_resource);

        // TODO - make two policy rules, remove patch/update for `managed_resource` (i.e. status only)
        vec![PolicyRule {
            api_groups: Some(vec![TESTSYS.to_string()]),
            resources: Some(vec![managed_resource, managed_status]),
            verbs: vec!["get", "list", "patch", "update", "watch"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ..Default::default()
        }]
    }
}
