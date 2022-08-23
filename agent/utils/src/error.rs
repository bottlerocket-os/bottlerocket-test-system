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

    #[snafu(display("Failed to decode base64 blob: {}", source))]
    Base64Decode { source: base64::DecodeError },

    #[snafu(display("Failed to setup environment variables: {}", what))]
    EnvSetup { what: String },

    #[snafu(display("Could not convert '{}' secret to string: {}", what, source))]
    Conversion { what: String, source: FromUtf8Error },

    #[snafu(display("Credentials were missing for assumed role '{}'", role_arn))]
    CredentialsMissing { role_arn: String },

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
