use super::{error, Result};
use crate::constants::NAMESPACE;
use crate::system::{
    agent_cluster_role, agent_cluster_role_binding, agent_service_account, controller_cluster_role,
    controller_cluster_role_binding, controller_deployment, controller_service_account,
    testsys_namespace, AgentType,
};
use crate::test_manager::TestManager;
use crate::{Resource, Test};
use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, CustomResourceExt};
use snafu::ResultExt;

impl TestManager {
    /// Create the testsys namespace
    pub(super) async fn create_namespace(&self) -> Result<()> {
        // Add the namespace to the cluster.
        let ns = testsys_namespace();

        self.create_or_update(false, &ns, "namespace").await?;

        // Give the object enough time to settle.
        let mut sleep_count = 0;
        let api = self.api::<Namespace>();
        while api.get(NAMESPACE).await.is_err() && sleep_count < 20 {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            sleep_count += 1;
        }

        api.get(NAMESPACE)
            .await
            .context(error::CreateSnafu { what: "namespace" })?;

        Ok(())
    }

    pub(super) async fn create_crd(&self) -> Result<()> {
        // Create the `Test` crd.
        let testcrd = Test::crd();
        // Create the `Resource` crd.
        let resourcecrd = Resource::crd();

        self.create_or_update(false, &testcrd, "Test CRD").await?;
        self.create_or_update(false, &resourcecrd, "Resource Provider CRD")
            .await
    }

    pub(super) async fn create_roles(&self, agent_type: AgentType) -> Result<()> {
        // If the role exists merge the new role, if not create the role.
        let test_agent_cluster_role = agent_cluster_role(agent_type);
        self.create_or_update(false, &test_agent_cluster_role, "Agent Cluster Role")
            .await?;

        // If the role already exists, update it with the new one using Patch. If not create a new
        // role.
        let controller_cluster_role = controller_cluster_role();
        self.create_or_update(false, &controller_cluster_role, "Controller Cluster Role")
            .await?;

        // If the cluster role binding already exists, update it with the new one using Patch. If
        // not create a new cluster role binding.
        let agent_cluster_role_binding = agent_cluster_role_binding(agent_type);
        self.create_or_update(
            false,
            &agent_cluster_role_binding,
            "Agent Cluster Role Binding",
        )
        .await?;

        // If the cluster role binding already exists, update it with the new one using Patch. If
        // not create a new cluster role binding.
        let controller_cluster_role_binding = controller_cluster_role_binding();
        self.create_or_update(
            false,
            &controller_cluster_role_binding,
            "Controller Cluster Role Binding",
        )
        .await?;

        Ok(())
    }

    pub(super) async fn create_service_accts(&self, agent_type: AgentType) -> Result<()> {
        // If the service accounts already exist, update them with the new ones using Patch. If not
        // create new service accounts.
        let agent_service_account = agent_service_account(agent_type);
        self.create_or_update(true, &agent_service_account, "Agent Service Account")
            .await?;

        Ok(())
    }

    pub(super) async fn create_controller_service_acct(&self) -> Result<()> {
        let controller_service_account = controller_service_account();
        self.create_or_update(
            true,
            &controller_service_account,
            "Controller Service Accout",
        )
        .await?;

        Ok(())
    }

    pub(super) async fn create_deployment(
        &self,
        uri: String,
        secret: Option<String>,
    ) -> Result<()> {
        let controller_deployment = controller_deployment(uri, secret);

        // If the controller deployment already exists, update it with the new one using Patch. If
        // not create a new controller deployment.
        self.create_or_update(true, &controller_deployment, "namespace")
            .await
    }

    pub(super) async fn uninstall_testsys(&self) -> Result<()> {
        let namespace_api: Api<Namespace> = self.api();
        namespace_api
            .delete(NAMESPACE, &Default::default())
            .await
            .context(error::KubeSnafu {
                action: "delete testsys namespace",
            })?;
        Ok(())
    }
}
