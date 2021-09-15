use crate::error::{self, Result};
use kube::Client;
use model::clients::TestClient;
use snafu::ResultExt;
use std::path::PathBuf;
use structopt::StructOpt;

/// Run a test stored in a YAML file at `path`.
#[derive(Debug, StructOpt)]
pub(crate) struct RunFile {
    /// Path to test crd YAML file.
    #[structopt(parse(from_os_str))]
    path: PathBuf,
}

impl RunFile {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        // Create the test object from its path.
        let test_file =
            std::fs::File::open(&self.path).context(error::File { path: &self.path })?;
        let test = serde_yaml::from_reader(test_file)
            .context(error::TestFileParse { path: &self.path })?;

        let tests = TestClient::new_from_k8s_client(k8s_client);

        tests.create_test(test).await.context(error::CreateTest)?;

        Ok(())
    }
}
