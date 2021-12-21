use aws_sdk_ec2::SdkError;
use aws_sdk_ssm::error::DescribeDocumentErrorKind;
use aws_sdk_ssm::model::{
    CommandInvocation, CommandInvocationStatus, DocumentFormat, DocumentType,
    InstanceInformationStringFilter,
};
use bottlerocket_agents::error;
use log::{debug, info};
use maplit::hashmap;
use sha2::{Digest, Sha256};
use snafu::{ensure, OptionExt, ResultExt};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

/// Waits for the SSM agent to become ready on the instances
pub(crate) async fn wait_for_ssm_ready(
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: &HashSet<String>,
) -> Result<(), error::Error> {
    let mut num_ready = 0;
    let sec_between_checks = Duration::from_secs(6);
    while num_ready != instance_ids.len() {
        let instance_info = ssm_client
            .describe_instance_information()
            .filters(
                InstanceInformationStringFilter::builder()
                    .key("InstanceIds")
                    .set_values(Some(
                        instance_ids.to_owned().into_iter().collect::<Vec<_>>(),
                    ))
                    .build(),
            )
            .send()
            .await
            .context(error::SsmDescribeInstanceInfo)?;
        num_ready = instance_info
            .instance_information_list()
            .map(|list| list.len())
            .context(error::SsmInstanceInfo)?;
        sleep(sec_between_checks);
    }

    Ok(())
}

/// Creates an SSM document if it doesn't already exist with the given name; if
/// it does but doesn't match the SSM document at given file path, updates it.
pub(crate) async fn create_or_update_ssm_document(
    ssm_client: &aws_sdk_ssm::Client,
    document_name: &str,
    document_path: &Path,
) -> Result<(), error::Error> {
    // Get the hash of the SSM document (if it exists already)
    let ssm_doc_hash = match ssm_client
        .describe_document()
        .name(document_name)
        .send()
        .await
    {
        Ok(doc) => doc.document().and_then(|d| d.hash().map(|s| s.to_string())),
        Err(sdk_err) => {
            return match sdk_err {
                SdkError::ServiceError { err, .. } => {
                    match err.kind {
                        DescribeDocumentErrorKind::InvalidDocument(_) => {
                            // Document does not exist, we need to create it.
                            let file_doc_data =
                                fs::read_to_string(document_path).context(error::FileRead)?;
                            ssm_client
                                .create_document()
                                .content(file_doc_data)
                                .name(document_name)
                                .document_type(DocumentType::Command)
                                .document_format(DocumentFormat::Yaml)
                                .send()
                                .await
                                .context(error::SsmCreateDocument)?;
                            Ok(())
                        }
                        _ => error::SsmDescribeDocument {
                            message: err.to_string(),
                        }
                        .fail(),
                    }
                }
                _ => error::AwsSdk {
                    message: sdk_err.to_string(),
                }
                .fail(),
            };
        }
    };

    if let Some(ssm_doc_hash) = ssm_doc_hash {
        let file_doc_data = fs::read_to_string(document_path).context(error::FileRead)?;
        let mut d = Sha256::new();
        d.update(&file_doc_data);
        let file_sha256_digest = hex::encode(d.finalize());
        // If the document exists and the hash is the same, then we're done
        if file_sha256_digest == ssm_doc_hash {
            info!(
                "SSM Document '{}' already exists with same checksum as '{}'",
                document_name,
                document_path.display()
            );
            return Ok(());
        }
    }

    info!(
        "SSM Document '{}' exists but doesn't match '{}' exactly, updating it...",
        document_name,
        document_path.display()
    );
    // Update the SSM document
    ssm_client
        .update_document()
        .content(format!("file://{}", document_path.display()))
        .name(document_name)
        .document_version("$LATEST")
        .document_format(DocumentFormat::Yaml)
        .send()
        .await
        .context(error::SsmUpdateDocument)?;
    Ok(())
}

async fn wait_command_finish(
    ssm_client: &aws_sdk_ssm::Client,
    cmd_id: String,
) -> Result<Vec<CommandInvocation>, error::Error> {
    let seconds_between_checks = Duration::from_secs(2);
    loop {
        let cmd_status = ssm_client
            .list_command_invocations()
            .command_id(cmd_id.to_owned())
            .send()
            .await
            .context(error::SsmListCommandInvocations)?;
        if let Some(invocations) = cmd_status.command_invocations {
            if invocations.is_empty()
                || invocations.iter().any(|i| {
                    matches!(
                        i.status,
                        Some(CommandInvocationStatus::InProgress)
                            | Some(CommandInvocationStatus::Pending)
                            | Some(CommandInvocationStatus::Delayed)
                    )
                })
            {
                // Command not finished, wait then check again
                sleep(seconds_between_checks)
            } else {
                return Ok(invocations);
            }
        }
    }
}

/// Runs a specified SSM document with specified parameters on provided list of instances
pub(crate) async fn ssm_run_command(
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: &HashSet<String>,
    document_name: String,
    parameters: &HashMap<String, Vec<String>>,
) -> Result<Vec<CommandInvocation>, error::Error> {
    let cmd_id = ssm_client
        .send_command()
        .set_instance_ids(Some(instance_ids.iter().map(|i| i.to_owned()).collect()))
        .document_name(document_name.to_owned())
        .set_parameters(Some(parameters.to_owned()))
        .timeout_seconds(30)
        .send()
        .await
        .context(error::SsmSendCommand)?
        .command()
        .and_then(|c| c.command_id().map(|s| s.to_string()))
        .context(error::SsmCommandId)?;

    debug!("############## Sent command, command ID: {}", cmd_id);
    // Wait for the command to finish
    if let Ok(invocations_result) = tokio::time::timeout(
        Duration::from_secs(60),
        wait_command_finish(ssm_client, cmd_id),
    )
    .await
    {
        let invocations = invocations_result?;
        for i in &invocations {
            debug!(
                "Instance: {}, Command Status: {}, Command Output: {:?}",
                i.instance_id.to_owned().unwrap_or_default(),
                i.status.as_ref().map(|s| s.as_str()).unwrap_or_default(),
                i.command_plugins
                    .to_owned()
                    .unwrap_or_default()
                    .iter()
                    .map(|c| c.output.as_ref().map(|s| s.to_string()).unwrap_or_default())
                    .collect::<Vec<String>>()
            )
        }
        let failed_invocations: Vec<_> = invocations
            .iter()
            .filter(|i| i.status != Some(CommandInvocationStatus::Success))
            .collect();
        if !failed_invocations.is_empty() {
            return error::SsmRunCommand {
                document_name,
                instance_ids: failed_invocations
                    .iter()
                    .map(|i| i.instance_id.to_owned().unwrap_or_default())
                    .collect::<Vec<String>>(),
            }
            .fail();
        }
        Ok(invocations)
    } else {
        // Timed-out waiting for commands to finish
        error::SsmWaitCommandTimeout.fail()
    }
}

/// Waits for the OS version of the instances to change to the target version.
pub(crate) async fn wait_for_os_version_change(
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: &HashSet<String>,
    target_version: &str,
) -> Result<(), error::Error> {
    let target_version = target_version.trim_start_matches('v');
    let check_os_parameters = hashmap! {
        "commands".to_string() => vec![r#"apiclient -u /os"#.to_string()],
        "executionTimeout".to_string() => vec!["10".to_string()],
    };
    let mut tries = 0;
    let max_tries = 12;
    let seconds_between_checks = Duration::from_secs(15);
    let mut unchanged_hosts = Vec::new();
    while tries < max_tries {
        sleep(seconds_between_checks);
        // Check the hosts' OS info
        if let Ok(invocations) = ssm_run_command(
            ssm_client,
            instance_ids,
            "AWS-RunShellScript".to_string(),
            &check_os_parameters,
        )
        .await
        {
            unchanged_hosts = invocations
                .iter()
                .filter(|i| {
                    i.command_plugins
                        .to_owned()
                        .unwrap_or_default()
                        .iter()
                        .any(|plugin| {
                            // Parse the JSON output of the 'apiclient get /os' call and compare versions
                            let os_info: serde_json::Value =
                                serde_json::from_str(&plugin.output.to_owned().unwrap_or_default())
                                    .unwrap_or_default();
                            if let Some(version_id) =
                                os_info.get("version_id").and_then(|v| v.as_str())
                            {
                                version_id != target_version
                            } else {
                                false
                            }
                        })
                })
                .map(|i| i.instance_id.to_owned().unwrap_or_default())
                .collect();
            if unchanged_hosts.is_empty() && invocations.len() == instance_ids.len() {
                // All hosts have updated to the target version
                return Ok(());
            }
        }
        tries += 1;
    }
    // This should technically never happen, check just in case
    ensure!(!unchanged_hosts.is_empty(), error::OsVersionCheck);
    // One or more hosts failed to update
    error::FailUpdates {
        target_version,
        instance_ids: unchanged_hosts,
    }
    .fail()
}
