use crate::error::{self, Result};
use kube::Client;
use model::clients::{CrdClient, TestClient};
use snafu::ResultExt;
use structopt::StructOpt;

/// Restart an object from a testsys cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct RestartTest {
    /// The name of the test to be restarted.
    #[structopt()]
    test_name: String,
}

impl RestartTest {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
        let mut test = test_client
            .get(&self.test_name)
            .await
            .context(error::GetSnafu {
                what: self.test_name.clone(),
            })?;
        // Created objects are not allowed to have `resource_version` set.
        test.metadata.resource_version = None;
        test.status = None;
        test_client
            .delete(&self.test_name)
            .await
            .context(error::DeleteSnafu {
                what: self.test_name.clone(),
            })?;
        test_client.wait_for_deletion(&self.test_name).await;
        test_client
            .create(test)
            .await
            .context(error::CreateTestSnafu)?;
        Ok(())
    }
}
