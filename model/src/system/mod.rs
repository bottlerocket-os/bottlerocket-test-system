/// Encapsulates testsys related K8S object definitions
mod agent;
mod controller;
mod namespace;

pub use agent::{agent_cluster_role, agent_cluster_role_binding, agent_service_account, AgentType};
pub use controller::{
    controller_cluster_role, controller_cluster_role_binding, controller_deployment,
    controller_service_account, TESTSYS_CONTROLLER_ARCHIVE_LOGS,
};
pub use namespace::testsys_namespace;
