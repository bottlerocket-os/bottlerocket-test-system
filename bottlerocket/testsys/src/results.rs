use crate::error::{self, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{AttachParams, ListParams},
    Api, Client, ResourceExt,
};
use model::constants::{NAMESPACE, TESTSYS_RESULTS_FILE};
use snafu::ResultExt;
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
        let pods: Api<Pod> = Api::namespaced(k8s_client.clone(), NAMESPACE);
        let pod_name = pods
            .list(&ListParams {
                label_selector: Some(format!("job-name={}", &self.test_name)),
                ..Default::default()
            })
            .await
            .context(error::GetPodSnafu {
                test_name: self.test_name.clone(),
            })?
            .iter()
            .next()
            .ok_or(error::Error::TestMissing {
                test_name: self.test_name.clone(),
            })?
            .name_any();

        let ap = AttachParams::default();
        let mut cat = pods
            .exec(&pod_name, vec!["cat", TESTSYS_RESULTS_FILE], &ap)
            .await
            .context(error::CreationSnafu {
                what: "sonobuoy results file",
            })?;
        let mut cat_out =
            tokio_util::io::ReaderStream::new(cat.stdout().ok_or(error::Error::NoOut)?);

        let mut out_file =
            tokio::fs::File::create(&self.destination)
                .await
                .context(error::FileSnafu {
                    path: &self.destination,
                })?;
        while let Some(data) = cat_out.next().await {
            out_file
                .write(&data.context(error::ReadSnafu)?)
                .await
                .context(error::WriteSnafu)?;
        }
        out_file.flush().await.context(error::WriteSnafu)?;

        Ok(())
    }
}
