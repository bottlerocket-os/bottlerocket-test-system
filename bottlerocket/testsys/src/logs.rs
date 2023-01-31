use crate::error::{self, Result};
use futures::{stream::select_all, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{ListParams, LogParams},
    Api, Client, ResourceExt,
};
use snafu::ResultExt;
use structopt::StructOpt;
use testsys_model::{
    clients::{CrdClient, TestClient},
    constants::NAMESPACE,
};

/// Retrieve the logs for a testsys test and all resources it depends on.
#[derive(Debug, StructOpt)]
pub(crate) struct Logs {
    /// The name of the test.
    #[structopt()]
    test_name: String,

    /// Include logs for the resource this test depends on.
    #[structopt(long)]
    include_resources: bool,

    /// Keep and updated stream of logs.
    #[structopt(long)]
    follow: bool,
}

impl Logs {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
        let test = test_client
            .get(&self.test_name)
            .await
            .context(error::GetSnafu {
                what: self.test_name.clone(),
            })?;
        let resources = test.spec.resources;

        let pod_api = Api::<Pod>::namespaced(k8s_client, NAMESPACE);
        let test_pods = pod_api
            .list(&ListParams {
                label_selector: Some(format!("job-name={}", &self.test_name)),
                ..Default::default()
            })
            .await
            .context(error::GetPodSnafu {
                test_name: self.test_name.clone(),
            })?;

        let log_params = LogParams {
            follow: self.follow,
            pretty: true,
            ..Default::default()
        };

        let mut streams = Vec::new();

        if self.include_resources {
            let mut pods = Vec::new();
            for resource in resources {
                pods.append(
                    &mut pod_api
                        .list(&ListParams {
                            label_selector: Some(format!("job-name={}-creation", resource)),
                            ..Default::default()
                        })
                        .await
                        .context(error::GetPodSnafu {
                            test_name: resource.clone(),
                        })?
                        .items,
                );
                pods.append(
                    &mut pod_api
                        .list(&ListParams {
                            label_selector: Some(format!("job-name={}-destruction", resource)),
                            ..Default::default()
                        })
                        .await
                        .context(error::GetPodSnafu {
                            test_name: resource.clone(),
                        })?
                        .items,
                );
            }
            for pod in pods {
                streams.push(
                    pod_api
                        .log_stream(&pod.name_any(), &log_params)
                        .await
                        .context(error::LogsSnafu {
                            pod: pod.name_any(),
                        })?,
                );
            }
        }
        for pod in test_pods {
            streams.push(
                pod_api
                    .log_stream(&pod.name_any(), &log_params)
                    .await
                    .context(error::LogsSnafu {
                        pod: pod.name_any(),
                    })?,
            );
        }

        let mut single_stream = select_all(streams);

        while let Some(line) = single_stream
            .try_next()
            .await
            .context(error::LogsStreamSnafu)?
        {
            println!("{:?}", String::from_utf8_lossy(&line));
        }

        Ok(())
    }
}
