use aws_sdk_iam::error::{AttachRolePolicyError, CreateRoleError, GetRoleError};
use aws_sdk_ssm::error::{CreateActivationError, DescribeInstanceInformationError};
use aws_sdk_sts::error::AssumeRoleError;
use aws_sdk_sts::types::SdkError;
use snafu::Snafu;
use std::string::FromUtf8Error;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
#[allow(clippy::large_enum_variant)]
pub enum Error {
    AssumeRole {
        role_arn: String,
        source: SdkError<AssumeRoleError>,
    },

    #[snafu(display(
        "Failed to attach policy '{}' to role '{}': {}",
        policy_arn,
        role_name,
        source
    ))]
    AttachRolePolicy {
        role_name: String,
        policy_arn: String,
        source: SdkError<AttachRolePolicyError>,
    },

    #[snafu(display("Failed to decode base64 blob: {}", source))]
    Base64Decode { source: base64::DecodeError },

    #[snafu(display("Could not convert '{}' secret to string: {}", what, source))]
    Conversion { what: String, source: FromUtf8Error },

    #[snafu(display("Failed to send create SSM command: {}", source))]
    CreateSsmActivation {
        source: SdkError<CreateActivationError>,
    },

    #[snafu(display(
        "Unable to create role '{}' with policy '{}': {}",
        role_name,
        role_policy,
        source
    ))]
    CreateRole {
        role_name: String,
        role_policy: String,
        source: SdkError<CreateRoleError>,
    },

    #[snafu(display("Credentials were missing for assumed role '{}'", role_arn))]
    CredentialsMissing { role_arn: String },

    #[snafu(display("Failed to setup environment variables: {}", what))]
    EnvSetup { what: String },

    #[snafu(display("Unable to get managed instance information: {}", source))]
    GetManagedInstanceInfo {
        source: SdkError<DescribeInstanceInformationError>,
    },

    #[snafu(display("Unable to get SSM role '{}': {}", role_name, source))]
    GetSSMRole {
        role_name: String,
        source: SdkError<GetRoleError>,
    },

    #[snafu(display("{} was missing from {}", what, from))]
    Missing { what: String, from: String },

    #[snafu(display("Secret was missing: {}", source))]
    SecretMissing {
        source: agent_common::secrets::Error,
    },

    #[snafu(display("Failed to write file at '{}': {}", path, source))]
    WriteFile {
        path: String,
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
