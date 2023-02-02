use crate::error::{self, Result};
use kube::Client;
use snafu::ResultExt;
use structopt::StructOpt;
use testsys_model::clients::{CrdClient, TestClient};

/// Set the field of a testsys test.
#[derive(Debug, StructOpt)]
pub(crate) struct Set {
    /// The name of the test to change.
    name: String,

    /// Set the value of the `keep_running` field of a testsys test.
    #[structopt(long)]
    keep_running: Option<bool>,
}

impl Set {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let tests = TestClient::new_from_k8s_client(k8s_client);

        if let Some(keep_running) = &self.keep_running {
            tests
                .send_keep_running(&self.name, *keep_running)
                .await
                .context(error::SetSnafu {
                    name: self.name.clone(),
                    what: "keep_running",
                })?;
        }

        Ok(())
    }
}
