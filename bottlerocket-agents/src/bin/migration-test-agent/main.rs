/*!

This is a test-agent for upgrade/downgrading a set of Bottlerocket instances to a target version and
ensure the instances successfully migrate to the target version.
It needs to run in a pod in a K8s cluster containing all the testsys-related CRDs.
(See yamlgen/deploy/testsys.yaml)

See `MigrateConfig` for different configuration values.

To build the container for the migration test agent, run `make migration-test-agent` from the
root directory of this repository.

Here is an example manifest for deploying the test definition for the migration test agent to a K8s cluster:


```yaml
apiVersion: testsys.bottlerocket.aws/v1
kind: Test
metadata:
  name: upgrade-ec2-test
  namespace: testsys-bottlerocket-aws
spec:
  agent:
    configuration:
      aws_region: us-west-2
      instance_ids: ${x86-aws-k8s-1-21-ec2-instances.ids}
      migrate_to_version: v1.4.0
      tuf_repo:
        metadata-url: https://example.net/testing/metadata/aws-k8s-1.21/x86_64/
        targets-url: https://example.net/testing/targets/
    image: <your migration-test-agent image URI>
    name: migration-test-agent
    keep_running: true
  resources: [x86-aws-k8s-1-21-ec2-instances, eks-1-21-ipv4]
```

!*/

mod ssm;

use crate::ssm::{
    create_or_update_ssm_document, ssm_run_command, wait_for_os_version_change, wait_for_ssm_ready,
};
use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ssm::Region;
use bottlerocket_agents::error;
use bottlerocket_agents::error::Error;
use bottlerocket_agents::{
    init_agent_logger, setup_env, MigrationConfig, AWS_CREDENTIALS_SECRET_NAME,
};
use log::{error, info};
use maplit::hashmap;
use model::{Outcome, SecretName, TestResults};
use snafu::ResultExt;
use std::path::Path;
use std::time::Duration;
use test_agent::{BootstrapData, ClientError, DefaultClient, Spec, TestAgent};

const BR_CHANGE_UPDATE_REPO_DOCUMENT_NAME: &str = "BR-ChangeUpdateRepo";
const BR_CHANGE_UPDATE_REPO_DOCUMENT_PATH: &str = "/local/ssm-documents/ssm-change-update-repo.yml";
const BR_UPDATE_DOCUMENT_PATH: &str = "/local/ssm-documents/update-api-update.yml";
const BR_UPDATE_DOCUMENT_NAME: &str = "BR-Update";

struct MigrationTestRunner {
    config: MigrationConfig,
    aws_secret_name: Option<SecretName>,
}

#[async_trait]
impl test_agent::Runner for MigrationTestRunner {
    type C = MigrationConfig;
    type E = Error;

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        info!("Initializing migration test agent...");
        Ok(Self {
            config: spec.configuration,
            aws_secret_name: spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned(),
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        // Set up the aws credentials if they were provided.
        if let Some(aws_secret_name) = &self.aws_secret_name {
            setup_env(self, aws_secret_name).await?;
        }
        let region_provider =
            RegionProviderChain::first_try(Region::new(self.config.aws_region.clone()));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        let ssm_client = aws_sdk_ssm::Client::new(&shared_config);

        // Ensure the SSM agents on the instances are ready, wait up to 5 minutes
        tokio::time::timeout(
            Duration::from_secs(300),
            wait_for_ssm_ready(&ssm_client, &self.config.instance_ids),
        )
        .await
        .context(error::SsmWaitInstanceReadyTimeout)??;

        // Optional step to change the update repository before proceeding to update
        if let Some(tuf_repo) = &self.config.tuf_repo {
            // Check if the SSM document to change Bottlerocket update repository exists, create it if it does not.
            create_or_update_ssm_document(
                &ssm_client,
                BR_CHANGE_UPDATE_REPO_DOCUMENT_NAME,
                Path::new(BR_CHANGE_UPDATE_REPO_DOCUMENT_PATH),
            )
            .await?;
            let parameters = hashmap! {
            "MetadataBaseUrl".to_string() => vec![tuf_repo.metadata_url.to_string()],
                "TargetsBaseUrl".to_string() => vec![tuf_repo.targets_url.to_string()],
            };
            info!("Changing TUF repository endpoints via the Bottlerocket API, Metadata url: {}, Targets url: {}", tuf_repo.metadata_url, tuf_repo.targets_url);
            ssm_run_command(
                &ssm_client,
                &self.config.instance_ids,
                BR_CHANGE_UPDATE_REPO_DOCUMENT_NAME.to_string(),
                &parameters,
            )
            .await?;
        }

        // Check if the SSM document to update Bottlerocket hosts exists, create it if it does not.
        create_or_update_ssm_document(
            &ssm_client,
            BR_UPDATE_DOCUMENT_NAME,
            Path::new(BR_UPDATE_DOCUMENT_PATH),
        )
        .await?;
        // Update to the requested version in the TUF update repository
        info!(
            "Initiating migration to {} on {:?}",
            self.config.migrate_to_version, self.config.instance_ids
        );
        let upgrade_parameters = hashmap! {
        "TargetVersion".to_string() => vec![self.config.migrate_to_version.to_string()],
        };
        ssm_run_command(
            &ssm_client,
            &self.config.instance_ids,
            BR_UPDATE_DOCUMENT_NAME.to_string(),
            &upgrade_parameters,
        )
        .await?;
        let reboot_parameters = hashmap! {
        "commands".to_string() => vec![r#"apiclient -u /actions/reboot -m POST"#.to_string()],
            "executionTimeout".to_string() => vec!["10".to_string()]
        };
        // Reboot with a separate SSM run command
        // We don't float-up errors here because SSM will occasionally not be able to report
        // success due to the host shutting down too fast.
        // We catch update failures by checking OS version changes when the hosts come back up.
        let _ = ssm_run_command(
            &ssm_client,
            &self.config.instance_ids,
            "AWS-RunShellScript".to_string(),
            &reboot_parameters,
        )
        .await;

        info!(
            "Waiting for hosts to reboot into {}",
            self.config.migrate_to_version
        );
        match wait_for_os_version_change(
            &ssm_client,
            &self.config.instance_ids,
            &self.config.migrate_to_version,
        )
        .await
        {
            Ok(_) => {
                info!(
                    "All instances successfully migrated to {}",
                    self.config.migrate_to_version
                );
                Ok(TestResults {
                    outcome: Outcome::Pass,
                    num_passed: self.config.instance_ids.len() as u64,
                    num_failed: 0,
                    num_skipped: 0,
                    other_info: Some(format!(
                        "Instances '{:?}' successfully migrated to {}",
                        &self.config.instance_ids, &self.config.migrate_to_version
                    )),
                })
            }
            Err(e) => match e {
                Error::FailUpdates {
                    target_version,
                    instance_ids,
                } => {
                    error!(
                        "Instance(s) '{:?}' failed to migrate to {}",
                        instance_ids, target_version
                    );
                    Ok(TestResults {
                        outcome: Outcome::Fail,
                        num_passed: (self.config.instance_ids.len() - instance_ids.len()) as u64,
                        num_failed: instance_ids.len() as u64,
                        num_skipped: 0,
                        other_info: Some(format!(
                            "Instance(s) '{:?}' successfully migrated to {}; Instance(s) '{:?}' failed to migrate",
                            &self.config.instance_ids, target_version, instance_ids
                        )),
                    })
                }
                _ => Err(e),
            },
        }
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        // Nothing to clean-up
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    init_agent_logger();
    if let Err(e) = run().await {
        error!("{}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), test_agent::error::Error<ClientError, Error>> {
    let mut agent = TestAgent::<DefaultClient, MigrationTestRunner>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "migration_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
