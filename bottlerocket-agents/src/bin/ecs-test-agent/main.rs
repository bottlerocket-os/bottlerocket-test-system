use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::Region;
use aws_sdk_ecs::model::LaunchType;
use bottlerocket_agents::error::{self, Error};
use bottlerocket_agents::{
    init_agent_logger, setup_test_env, EcsTestConfig, AWS_CREDENTIALS_SECRET_NAME,
};
use log::info;
use model::{Outcome, SecretName, TestResults};
use snafu::{OptionExt, ResultExt};
use std::time::Duration;
use test_agent::{BootstrapData, ClientError, DefaultClient, Runner, Spec, TestAgent};

const DEFAULT_REGION: &str = "us-west-2";

struct EcsTestRunner {
    config: EcsTestConfig,
    aws_secret_name: Option<SecretName>,
}

#[async_trait]
impl Runner for EcsTestRunner {
    type C = EcsTestConfig;
    type E = Error;

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        info!("Initializing Ecs test agent...");
        Ok(Self {
            config: spec.configuration,
            aws_secret_name: spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned(),
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        // Set up the aws credentials if they were provided.
        if let Some(aws_secret_name) = &self.aws_secret_name {
            setup_test_env(self, aws_secret_name).await?;
        }

        let region_provider = RegionProviderChain::first_try(Some(Region::new(
            self.config
                .region
                .clone()
                .unwrap_or_else(|| DEFAULT_REGION.to_string()),
        )));
        let config = aws_config::from_env().region(region_provider).load().await;
        let ecs_client = aws_sdk_ecs::Client::new(&config);

        info!("Waiting for registered container instances...");

        tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_registered_containers(&ecs_client, &self.config.cluster_name),
        )
        .await
        .context(error::InstanceTimeoutSnafu)??;

        info!("Running task definition...");

        let run_task_output = ecs_client
            .run_task()
            .cluster(&self.config.cluster_name)
            .task_definition(&self.config.task_definition)
            .count(self.config.task_count)
            .launch_type(LaunchType::Ec2)
            .send()
            .await
            .context(error::TaskRunCreationSnafu)?;
        let task_arns: Vec<String> = run_task_output
            .tasks()
            .map(|tasks| {
                tasks
                    .iter()
                    .filter_map(|task| task.task_arn().map(|arn| arn.to_string()))
                    .collect()
            })
            .unwrap();

        info!("Waiting for tasks to complete...");

        match tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_test_running(
                &ecs_client,
                &self.config.cluster_name,
                &task_arns,
                self.config.task_count,
            ),
        )
        .await
        {
            Ok(results) => results,
            Err(_) => {
                test_results(
                    &ecs_client,
                    &self.config.cluster_name,
                    &task_arns,
                    self.config.task_count,
                )
                .await
            }
        }
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        Ok(())
    }
}

async fn wait_for_test_running(
    ecs_client: &aws_sdk_ecs::Client,
    cluster_name: &str,
    task_arns: &[String],
    task_count: i32,
) -> Result<TestResults, Error> {
    loop {
        let results = test_results(ecs_client, cluster_name, task_arns, task_count).await?;
        if results.outcome == Outcome::Pass {
            return Ok(results);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn test_results(
    ecs_client: &aws_sdk_ecs::Client,
    cluster_name: &str,
    task_arns: &[String],
    task_count: i32,
) -> Result<TestResults, Error> {
    let tasks = ecs_client
        .describe_tasks()
        .cluster(cluster_name)
        .set_tasks(Some(task_arns.to_vec()))
        .send()
        .await
        .context(error::TaskDescribeSnafu)?
        .tasks()
        .map(|tasks| tasks.to_owned())
        .context(error::NoTaskSnafu)?;
    let running_count = tasks
        .iter()
        .filter_map(|task| task.last_status())
        .filter(|status| status == &"Running")
        .count() as i32;
    Ok(TestResults {
        outcome: if task_count == running_count {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        num_passed: running_count as u64,
        num_failed: (task_count - running_count) as u64,
        num_skipped: 0,
        other_info: None,
    })
}

async fn wait_for_registered_containers(
    ecs_client: &aws_sdk_ecs::Client,
    cluster: &str,
) -> Result<(), Error> {
    loop {
        let cluster = ecs_client
            .describe_clusters()
            .clusters(cluster)
            .send()
            .await
            .context(error::ClusterDescribeSnafu)?
            .clusters()
            .context(error::NoTaskSnafu)?
            .first()
            .context(error::NoTaskSnafu)?
            .clone();

        if cluster.registered_container_instances_count() != 0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::main]
async fn main() {
    init_agent_logger(env!("CARGO_CRATE_NAME"), None);
    if let Err(e) = run().await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), test_agent::error::Error<ClientError, Error>> {
    let mut agent = TestAgent::<DefaultClient, EcsTestRunner>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "ecs_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
