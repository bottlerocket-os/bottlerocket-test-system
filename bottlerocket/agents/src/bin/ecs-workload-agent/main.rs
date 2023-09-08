/*!

This is a test-agent for running workload tests on ECS.
It expects to be run in a pod launched by the TestSys controller.

To build the container for the workload test agent, run `make ecs-workload-agent` from the
root directory of this repository.

Here is an example manifest for deploying the test definition for the workload test agent:

```yaml
apiVersion: testsys.system/v1
kind: Test
metadata:
  name: ecs-workload
  namespace: testsys
spec:
  agent:
    configuration:
      region: "us-east-2"
      clusterName: "bottlerocket"
      assumeRole: "arn:aws:sts::999807767999:assumed-role/InstanceAdmin/i-89898f9c69b228989"
      tests:
      - name: nvidia-workload
        image: testsys-nvidia-workload-test:v0.0.3
        gpu: true
      - name: webserver
        image: nginx:latest
    image: <your ecs-workload-agent image URI>
    name: ecs-workload-test-agent
    keepRunning: true
    timeout: "5000"
  resources: []
```

!*/

use agent_utils::aws::aws_config;
use agent_utils::init_agent_logger;
use async_trait::async_trait;
use aws_sdk_ec2::types::SdkError;
use aws_sdk_ecs::error::DescribeTaskDefinitionErrorKind;
use aws_sdk_ecs::model::{
    Compatibility, ContainerDefinition, LaunchType, ResourceRequirement, ResourceType, SortOrder,
    TaskDefinition, TaskStopCode,
};
use bottlerocket_agents::error::{self, Error};
use bottlerocket_types::agent_config::{
    EcsWorkloadTestConfig, WorkloadTest, AWS_CREDENTIALS_SECRET_NAME,
};
use log::info;
use snafu::{OptionExt, ResultExt};
use std::time::Duration;
use test_agent::{
    BootstrapData, ClientError, DefaultClient, DefaultInfoClient, InfoClient, Runner, Spec,
    TestAgent,
};
use testsys_model::{Outcome, SecretName, TestResults};

pub const DEFAULT_TASK_DEFINITION_PREFIX: &str = "testsys-bottlerocket-";

struct EcsWorkloadTestRunner {
    config: EcsWorkloadTestConfig,
    aws_secret_name: Option<SecretName>,
}

#[async_trait]
impl<I> Runner<I> for EcsWorkloadTestRunner
where
    I: InfoClient,
{
    type C = EcsWorkloadTestConfig;
    type E = Error;

    /// Initialize a new instance of our workload test runner.
    async fn new(spec: Spec<Self::C>, _info_client: &I) -> Result<Self, Self::E> {
        info!("Initializing ECS workload test agent...");
        Ok(Self {
            config: spec.configuration,
            aws_secret_name: spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned(),
        })
    }

    /// Run the set of configured workload tests and return the results.
    async fn run(&mut self, info_client: &I) -> Result<TestResults, Self::E> {
        let config = aws_config(
            &self.aws_secret_name.as_ref(),
            &self.config.assume_role,
            &None,
            &self.config.region,
            &None,
            false,
        )
        .await?;
        let ecs_client = aws_sdk_ecs::Client::new(&config);

        info!("Waiting for registered container instances...");

        tokio::time::timeout(
            Duration::from_secs(120),
            wait_for_cluster_ready(&ecs_client, &self.config.cluster_name),
        )
        .await
        .context(error::InstanceTimeoutSnafu)??;

        // First loop through and make sure we have task definitions for each of our tests.
        // We don't want to starts tasks if one of the defined tets is not able to be run.
        let mut task_def_arns = Vec::new();
        for test_def in &self.config.tests {
            let task_def_arn = create_or_find_task_definition(&ecs_client, test_def).await?;

            task_def_arns.push(task_def_arn);
        }

        let mut results = TestResults {
            outcome: Outcome::InProgress,
            other_info: Some("Starting test".to_string()),
            ..Default::default()
        };
        info_client
            .send_test_update(results.clone())
            .await
            .err()
            .iter()
            .for_each(|e| log::error!("Unable to send test update: {}", e));

        // Now that we have the full set of task definitions to run, loop through and kick them all off.
        let mut task_arns = Vec::new();
        for task_def_arn in task_def_arns {
            info!("Running task '{}'", task_def_arn);
            let run_task_output = ecs_client
                .run_task()
                .cluster(&self.config.cluster_name)
                .task_definition(task_def_arn)
                .launch_type(LaunchType::Ec2)
                .send()
                .await
                .context(error::TaskRunCreationSnafu)?;
            let run_task_arns: Vec<String> = run_task_output
                .tasks()
                .map(|tasks| {
                    tasks
                        .iter()
                        .filter_map(|task| task.task_arn().map(|arn| arn.to_string()))
                        .collect()
                })
                .unwrap();
            for task_arn in &run_task_arns {
                task_arns.push(task_arn.clone());
            }
        }

        info!("Waiting for tasks to complete...");
        results.other_info = Some("Checking status".to_string());
        info_client
            .send_test_update(results)
            .await
            .err()
            .iter()
            .for_each(|e| log::error!("Unable to send test update: {}", e));

        // Some workload tests could take awhile to complete. This is a long timeout, but we need to make sure there is
        // enough time for any larger tests to finish.
        // Note: may want to make this configurable by test definition yaml so it can be customized by the actual tests
        // being run.
        match tokio::time::timeout(
            Duration::from_secs(1800),
            wait_for_test_completion(
                &ecs_client,
                &self.config.cluster_name,
                &task_arns,
                info_client,
            ),
        )
        .await
        {
            Ok(results) => results,
            Err(_) => {
                let mut res =
                    test_results(&ecs_client, &self.config.cluster_name, &task_arns).await;

                // We've timed out, but there is a window where our final check could actually report completion.
                // To guard against this, check if still in progress and mark failed only if so.
                if let Ok(mut result) = res {
                    if result.outcome == Outcome::InProgress {
                        result.outcome = Outcome::Timeout;
                    }
                    res = Ok(result);
                }

                res
            }
        }
    }

    /// Terminate the test runs. This is a noop for this agent.
    async fn terminate(&mut self) -> Result<(), Self::E> {
        Ok(())
    }
}

/// Loops waiting for the workload test tasks to complete.
async fn wait_for_test_completion<I>(
    ecs_client: &aws_sdk_ecs::Client,
    cluster_name: &str,
    task_arns: &[String],
    info_client: &I,
) -> Result<TestResults, Error>
where
    I: InfoClient,
{
    let mut retries: u64 = 0;
    loop {
        let mut results = test_results(ecs_client, cluster_name, task_arns).await?;
        // If tests have all passed or failed, return. Otherwise still in progress so wait and retry.
        if results.outcome == Outcome::Pass || results.outcome == Outcome::Fail {
            info_client
                .send_test_update(results.clone())
                .await
                .err()
                .iter()
                .for_each(|e| log::error!("Unable to send test update: {}", e));
            return Ok(results);
        }

        retries += 1;
        results.other_info = Some(format!("Waiting for test completion: attempt {}", retries));
        info_client
            .send_test_update(results.clone())
            .await
            .err()
            .iter()
            .for_each(|e| log::error!("Unable to send test update: {}", e));
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Gets the status from our test tasks and populates a `TestResults` struct with the outcome.
async fn test_results(
    ecs_client: &aws_sdk_ecs::Client,
    cluster_name: &str,
    task_arns: &[String],
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
    let passed_count = tasks
        .iter()
        .filter(|task| task.last_status() == Some("STOPPED"))
        .filter(|task| task.stop_code() == Some(&TaskStopCode::EssentialContainerExited))
        .filter(|task| {
            task.containers()
                .unwrap_or_default()
                .iter()
                .filter(|container| container.exit_code() != Some(0))
                .count()
                == 0
        })
        .count() as i32;
    let failed_count = tasks
        .iter()
        .filter(|task| task.last_status() == Some("STOPPED"))
        .filter(|task| task.stop_code() == Some(&TaskStopCode::EssentialContainerExited))
        .filter(|task| {
            task.containers()
                .unwrap_or_default()
                .iter()
                .filter(|container| container.exit_code() != Some(0))
                .count()
                != 0
        })
        .count() as i32;
    let task_count = task_arns.len() as i32;
    Ok(TestResults {
        outcome: if task_count == passed_count {
            // All tests have passed
            Outcome::Pass
        } else if task_count == (passed_count + failed_count) {
            // All tests have completed, but some have failed
            Outcome::Fail
        } else {
            // Still tests running
            Outcome::InProgress
        },
        num_passed: passed_count as u64,
        num_failed: failed_count as u64,
        num_skipped: 0,
        other_info: None,
    })
}

/// Waits for the cluster to have registered instances.
async fn wait_for_cluster_ready(
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

        info!("Waiting for cluster to have registered instances");
        if cluster.registered_container_instances_count() != 0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Retrieves the task_definition and revision of a matching test task definition. If the
/// task definition doesn't exist, it will be created. If the task definition exists but
/// the container definitions doesn't match the current settings in the config, a new task
/// definition revision will be created.
async fn create_or_find_task_definition(
    ecs_client: &aws_sdk_ecs::Client,
    test_def: &WorkloadTest,
) -> Result<String, Error> {
    let task_def_name = format!("{}{}", DEFAULT_TASK_DEFINITION_PREFIX, test_def.name);
    let task_info = ecs_client
        .describe_task_definition()
        .task_definition(task_def_name.clone())
        .send()
        .await;

    if let Err(SdkError::ServiceError(service_error)) = &task_info {
        // If we get an error and it's a ClientException, that means the call worked
        // but the identifier was not found. Anything else and it will be handled
        // below with lookup calls.
        if matches!(
            &service_error.err().kind,
            DescribeTaskDefinitionErrorKind::ClientException(_)
        ) {
            return create_task_definition(ecs_client, &task_def_name, test_def).await;
        }
    }

    // There is a task definition with the right name, now we need to find the right
    // revision that matches our image:tag.
    if let task_arn @ Ok(_) = find_task_rev(ecs_client, &task_def_name, test_def).await {
        return task_arn;
    }

    // No existing revision matches our current settings, create a new one.
    create_task_definition(ecs_client, &task_def_name, test_def).await
}

/// Attempts to find a task definition revision that matches our test requirements.
async fn find_task_rev(
    ecs_client: &aws_sdk_ecs::Client,
    task_def_name: &str,
    task_def: &WorkloadTest,
) -> Result<String, Error> {
    let task_revisions = ecs_client
        .list_task_definitions()
        .sort(SortOrder::Desc)
        .family_prefix(task_def_name)
        .send()
        .await
        .context(error::TaskDefinitionListSnafu)?;
    let task_revisions = task_revisions.task_definition_arns().unwrap_or_default();

    for task_rev_arn in task_revisions {
        let task_rev_details = ecs_client
            .describe_task_definition()
            .task_definition(task_rev_arn.clone())
            .send()
            .await;

        if let Ok(task_rev_details) = task_rev_details {
            if let Some(task_rev) = task_rev_details.task_definition() {
                if is_matching_definition(task_def, task_rev) {
                    return Ok(task_rev_arn.to_string());
                }
            }
        }
    }

    Err(Error::TaskDefinitionMissing)
}

/// Compares an individual task definition to see if its settings match the test settings.
fn is_matching_definition(test_def: &WorkloadTest, task_def: &TaskDefinition) -> bool {
    if let Some(containers) = task_def.container_definitions() {
        if containers.len() != 1 {
            return false;
        }

        let container_def = &containers[0];
        if container_def.name().unwrap_or_default() == test_def.name
            && container_def.image().unwrap_or_default() == test_def.image
        {
            // We have the right name and image, now see if there are any resource requirements
            let res_reqs = container_def.resource_requirements();
            match res_reqs {
                Some(reqs) => {
                    // If the test needs GPU support, then it needs to be one of the resource requirements.
                    let mut gpu_set = false;
                    for requirement in reqs {
                        if matches!(requirement.r#type(), Some(&ResourceType::Gpu))
                            && requirement.value().unwrap_or_default() == "1"
                        {
                            gpu_set = true;
                        }
                    }

                    if test_def.gpu != gpu_set {
                        // Mismatch on GPU settings between this rev and our test
                        return false;
                    }
                }
                None => {
                    // If the test needs GPU support but there are no resource requirements, then it is missing from
                    // the revision and is not a match.
                    if test_def.gpu {
                        return false;
                    }
                }
            }
            return true;
        }
    }
    false
}

/// Creates a container definition based on the test settings.
fn create_container_definition(test_def: &WorkloadTest) -> ContainerDefinition {
    let mut builder = ContainerDefinition::builder()
        .name(test_def.name.clone())
        .image(test_def.image.clone())
        .essential(true)
        .memory_reservation(300);
    // TODO: See if we want to set up CloudWatch log capture
    // .log_configuration(
    //     LogConfiguration::builder()
    //         .log_driver(aws_sdk_ecs::model::LogDriver::Awslogs)
    //         .options("awslogs-group", "path")
    //         .options("awslogs-region", "region")
    //         .options("awslogs-stream-prefix", "ecs")

    if test_def.gpu {
        builder = builder.resource_requirements(
            ResourceRequirement::builder()
                .r#type(aws_sdk_ecs::model::ResourceType::Gpu)
                .value("1")
                .build(),
        );
    }

    builder.build()
}

/// Creates a new task definition for an individual workload test.
async fn create_task_definition(
    ecs_client: &aws_sdk_ecs::Client,
    task_def_name: &str,
    test_def: &WorkloadTest,
) -> Result<String, Error> {
    let task_info = ecs_client
        .register_task_definition()
        .family(task_def_name)
        .requires_compatibilities(Compatibility::Ec2)
        .container_definitions(create_container_definition(test_def))
        .send()
        .await
        .context(error::TaskDefinitionCreationSnafu)?;
    if let Some(task_arn) = task_info
        .task_definition()
        .context(error::TaskDefinitionMissingSnafu)?
        .task_definition_arn()
    {
        Ok(task_arn.to_string())
    } else {
        Err(Error::TaskDefinitionMissing)
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
    let mut agent = TestAgent::<DefaultClient, EcsWorkloadTestRunner, DefaultInfoClient>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "ecs_workload_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}

#[cfg(test)]
mod test_workload_agent {
    use super::*;
    use bottlerocket_types::agent_config::WorkloadTest;

    #[test]
    fn test_is_matching_definition_matches() {
        let test_def = WorkloadTest {
            image: "image_name:latest".to_string(),
            name: "test".to_string(),
            gpu: true,
        };

        let task_def = fake_task_def(true);
        assert!(is_matching_definition(&test_def, &task_def));
    }

    #[test]
    fn test_is_matching_definition_mismatch() {
        let test_def = WorkloadTest {
            image: "image_name:1.1".to_string(),
            name: "test".to_string(),
            gpu: true,
        };

        let task_def = fake_task_def(true);
        assert!(!is_matching_definition(&test_def, &task_def));
    }

    #[test]
    fn test_is_matching_definition_mismatch_gpu() {
        let test_def = WorkloadTest {
            image: "image_name:latest".to_string(),
            name: "test".to_string(),
            gpu: true,
        };

        let task_def = fake_task_def(false);
        assert!(!is_matching_definition(&test_def, &task_def));
    }

    fn fake_task_def(include_gpu: bool) -> TaskDefinition {
        let revision;
        if include_gpu {
            revision = 2;
        } else {
            revision = 1;
        }

        let mut builder = ContainerDefinition::builder()
            .name("test".to_string())
            .image("image_name:latest".to_string());

        if include_gpu {
            builder = builder.resource_requirements(
                ResourceRequirement::builder()
                    .r#type(aws_sdk_ecs::model::ResourceType::Gpu)
                    .value("1")
                    .build(),
            );
        }

        TaskDefinition::builder()
            .family("test".to_string())
            .task_definition_arn(format!("test.task.def.arn:{}", revision))
            .container_definitions(builder.build())
            .revision(revision)
            .build()
    }
}
