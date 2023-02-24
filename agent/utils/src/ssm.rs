use crate::error::{self, Result};
use aws_sdk_iam::types::SdkError;
use aws_sdk_ssm::model::{InstanceInformation, InstanceInformationStringFilter, Tag};
use log::info;
use serde_json::json;
use snafu::{OptionExt, ResultExt};
use std::thread::sleep;
use std::time::Duration;

/// AWS Role to assign to the managed VM
const SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME: &str = "BR-SSMServiceRole";
/// ARN of the SSM amanged instance policy
const SSM_MANAGED_INSTANCE_POLICY_ARN: &str =
    "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore";

pub async fn ensure_ssm_service_role(iam_client: &aws_sdk_iam::Client) -> Result<()> {
    let get_role_result = iam_client
        .get_role()
        .role_name(SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME)
        .send()
        .await;
    if let Err(sdk_err) = get_role_result {
        match sdk_err {
            SdkError::ServiceError { .. } => {
                info!(
                    "'{}' service role does not exist, creating the service role",
                    SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME
                );
                let ssm_service_trust = json!({"Version": "2012-10-17", "Statement": {
                    "Effect": "Allow",
                    "Principal": {
                        "Service": "ssm.amazonaws.com"
                    },
                    "Action": "sts:AssumeRole"
                }});
                let assume_role_doc = ssm_service_trust.to_string();
                iam_client
                    .create_role()
                    .role_name(SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME)
                    .assume_role_policy_document(&assume_role_doc)
                    .send()
                    .await
                    .context(error::CreateRoleSnafu {
                        role_name: SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME,
                        role_policy: assume_role_doc,
                    })?;
            }
            e => {
                return Err(error::Error::GetSSMRole {
                    role_name: SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME.to_string(),
                    source: e,
                });
            }
        }
    }

    // Attach SSM managed instance policy to the service role
    iam_client
        .attach_role_policy()
        .role_name(SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME)
        .policy_arn(SSM_MANAGED_INSTANCE_POLICY_ARN)
        .send()
        .await
        .context(error::AttachRolePolicySnafu {
            role_name: SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME,
            policy_arn: SSM_MANAGED_INSTANCE_POLICY_ARN,
        })?;

    Ok(())
}

pub async fn create_ssm_activation(
    cluster_name: &str,
    num_registration: i32,
    ssm_client: &aws_sdk_ssm::Client,
) -> Result<(String, String)> {
    let activations = ssm_client
        .create_activation()
        .iam_role(SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME)
        .registration_limit(num_registration)
        .tags(
            Tag::builder()
                .key("TESTSYS_MANAGED_INSTANCE")
                .value(cluster_name)
                .build(),
        )
        .send()
        .await
        .context(error::CreateSsmActivationSnafu {})?;
    let activation_id = activations.activation_id.context(error::MissingSnafu {
        what: "activation id",
        from: "activations",
    })?;
    let activation_code = activations.activation_code.context(error::MissingSnafu {
        what: "activation code",
        from: "activations",
    })?;
    Ok((activation_id, activation_code))
}

// Waits for the SSM agent to be ready for a particular instance, returns the instance information
pub async fn wait_for_ssm_ready(
    ssm_client: &aws_sdk_ssm::Client,
    activation_id: &str,
    ip: &str,
) -> Result<InstanceInformation> {
    let seconds_between_checks = Duration::from_secs(5);
    loop {
        let instance_info = ssm_client
            .describe_instance_information()
            .filters(
                InstanceInformationStringFilter::builder()
                    .key("ActivationIds")
                    .values(activation_id)
                    .build(),
            )
            .send()
            .await
            .context(error::GetManagedInstanceInfoSnafu {})?;
        if let Some(info) = instance_info.instance_information_list().and_then(|list| {
            list.iter()
                .find(|info| info.ip_address == Some(ip.to_string()))
        }) {
            return Ok(info.to_owned());
        } else {
            // SSM agent not ready on instance, wait then check again
            sleep(seconds_between_checks)
        }
    }
}
