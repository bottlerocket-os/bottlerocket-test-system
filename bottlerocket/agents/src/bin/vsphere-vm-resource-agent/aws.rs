use aws_sdk_iam::types::SdkError;
use aws_sdk_ssm::model::{InstanceInformation, InstanceInformationStringFilter, Tag};
use log::info;
use resource_agent::provider::{IntoProviderError, ProviderError, ProviderResult, Resources};
use serde_json::json;
use std::thread::sleep;
use std::time::Duration;

/// AWS Role to assign to the managed VM
const SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME: &str = "BR-SSMServiceRole";
/// ARN of the SSM amanged instance policy
const SSM_MANAGED_INSTANCE_POLICY_ARN: &str =
    "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore";

pub(crate) async fn ensure_ssm_service_role(
    iam_client: &aws_sdk_iam::Client,
) -> ProviderResult<()> {
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
                    .assume_role_policy_document(assume_role_doc)
                    .send()
                    .await
                    .context(
                        Resources::Clear,
                        "Could not create SSM managed instance service role",
                    )?;
            }
            _ => {
                return Err(ProviderError::new_with_source_and_context(
                    Resources::Clear,
                    format!(
                        "Failed to determine whether SSM '{}' service role exists",
                        SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME
                    ),
                    sdk_err,
                ));
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
        .context(
            Resources::Clear,
            "Could not attach SSM managed instance policy to the service role",
        )?;

    Ok(())
}

pub(crate) async fn create_ssm_activation(
    resources: Resources,
    cluster_name: &str,
    num_registration: i32,
    ssm_client: &aws_sdk_ssm::Client,
) -> ProviderResult<(String, String)> {
    let activations = ssm_client
        .create_activation()
        .iam_role(SSM_MANAGED_INSTANCE_SERVICE_ROLE_NAME)
        .registration_limit(num_registration)
        .tags(
            Tag::builder()
                .key("TESTSYS_VSPHERE_VMS")
                .value(cluster_name)
                .build(),
        )
        .send()
        .await
        .context(resources, "Failed to send SSM create activation command")?;
    let activation_id = activations
        .activation_id
        .context(resources, "Unable to fetch SSM activation ID")?;
    let activation_code = activations
        .activation_code
        .context(resources, "Unable to generate SSM activation code")?;
    Ok((activation_id, activation_code))
}

// Waits for the SSM agent to be ready for a particular instance, returns the instance information
pub(crate) async fn wait_for_ssm_ready(
    resources: Resources,
    ssm_client: &aws_sdk_ssm::Client,
    activation_id: &str,
    ip: &str,
) -> ProviderResult<InstanceInformation> {
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
            .context(
                resources,
                "Failed to get registered managed instance information",
            )?;
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
