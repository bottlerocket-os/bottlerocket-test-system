use crate::error::{self, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{AttachParams, ListParams},
    Api, Client, ResourceExt,
};
use model::{
    clients::TestClient,
    constants::{LABEL_TEST_NAME, NAMESPACE},
    TestType,
};
use snafu::ResultExt;
use sonobuoy_test_agent::SONOBUOY_TEST_RESULTS_LOCATION;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::io::AsyncWriteExt;

/// Retrieve the results of a test.
#[derive(Debug, StructOpt)]
pub(crate) struct Results {
    /// Name of the sonobuoy test.
    #[structopt(short = "n", long)]
    test_name: String,
    /// The place the test results should be written
    #[structopt(long, parse(from_os_str))]
    destination: PathBuf,
}

impl Results {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let tests_api = TestClient::new_from_k8s_client(k8s_client.clone());
        let test = tests_api
            .get_test(&self.test_name)
            .await
            .context(error::GetTest)?;
        match test.spec.test_type {
            TestType::Sonobuoy => {
                Self::sonobuoy_test_results(k8s_client, &self.test_name, &self.destination).await
            }
            test_type => Self::default_test_results(test_type),
        }
    }

    async fn sonobuoy_test_results(
        k8s_client: Client,
        test_name: &str,
        destination: &PathBuf,
    ) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(k8s_client.clone(), NAMESPACE);
        let pod_name = pods
            .list(&ListParams {
                label_selector: Some(format!("{}={}", LABEL_TEST_NAME, test_name)),
                ..Default::default()
            })
            .await
            .context(error::GetPod {
                test_name: test_name.to_string(),
            })?
            .iter()
            .next()
            .ok_or(error::Error::TestMissing {
                test_name: test_name.to_string(),
            })?
            .name();

        let ap = AttachParams::default();
        let mut cat = pods
            .exec(&pod_name, vec!["cat", SONOBUOY_TEST_RESULTS_LOCATION], &ap)
            .await
            .context(error::Creation {
                what: "sonobuoy results file",
            })?;
        let mut cat_out =
            tokio_util::io::ReaderStream::new(cat.stdout().ok_or(error::Error::NoOut)?);

        let mut out_file = tokio::fs::File::create(destination)
            .await
            .context(error::File { path: destination })?;
        while let Some(data) = cat_out.next().await {
            out_file
                .write(&data.context(error::Read)?)
                .await
                .context(error::Write)?;
        }
        out_file.flush().await.context(error::Write)?;

        Ok(())
    }

    fn default_test_results(test_type: TestType) -> Result<()> {
        println!("There are no results for tests of type '{:?}'", test_type);
        Ok(())
    }
}
