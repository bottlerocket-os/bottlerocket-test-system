use aws_sdk_ssm::error::{
    CreateDocumentError, ListCommandInvocationsError, SendCommandError, UpdateDocumentError,
};
use aws_sdk_ssm::SdkError;
use snafu::Snafu;
use std::string::FromUtf8Error;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub")]
pub enum Error {
    #[snafu(display("Failed to base64-decode kubeconfig for test cluster: {}", source))]
    Base64Decode { source: base64::DecodeError },

    #[snafu(display("Could not convert '{}' secret to string: {}", what, source))]
    Conversion { what: String, source: FromUtf8Error },

    #[snafu(display("Failed to setup environment variables: {}", what))]
    EnvSetup { what: String },

    #[snafu(display("Failed to write kubeconfig for test cluster: {}", source))]
    KubeconfigWrite { source: std::io::Error },

    #[snafu(display("Secret was missing: {}", source))]
    SecretMissing {
        source: agent_common::secrets::Error,
    },

    #[snafu(display("Failed to create sonobuoy process: {}", source))]
    SonobuoyProcess { source: std::io::Error },

    #[snafu(display("Failed to run conformance test"))]
    SonobuoyRun,

    #[snafu(display("Failed to clean-up sonobuoy resources"))]
    SonobuoyDelete,

    #[snafu(display("{}", source))]
    DeserializeJson { source: serde_json::Error },

    #[snafu(display("Missing '{}' field from sonobuoy status", field))]
    MissingSonobuoyStatusField { field: String },

    #[snafu(display("AWS SDK failed: {}", message))]
    AwsSdk { message: String },

    #[snafu(display("SSM Create Document failed: {}", source))]
    SsmCreateDocument {
        source: SdkError<CreateDocumentError>,
    },

    #[snafu(display("SSM Describe Document failed: {}", message))]
    SsmDescribeDocument { message: String },

    #[snafu(display("SSM Update Document failed: {}", source))]
    SsmUpdateDocument {
        source: SdkError<UpdateDocumentError>,
    },

    #[snafu(display("SSM Send Command failed: {}", source))]
    SsmSendCommand { source: SdkError<SendCommandError> },

    #[snafu(display("SSM List Command Invocations failed: {}", source))]
    SsmListCommandInvocations {
        source: SdkError<ListCommandInvocationsError>,
    },

    #[snafu(display("No command ID in SSM send command response"))]
    SsmCommandId,

    #[snafu(display("Timed-out waiting for commands to finish running"))]
    SsmWaitCommandTimeout,

    #[snafu(display("Failed to run '{}' in '{:?}'", document_name, instance_ids))]
    SsmRunCommand {
        document_name: String,
        instance_ids: Vec<String>,
    },

    #[snafu(display(
        "The following Bottlerocket hosts failed to update to '{}': {:?}",
        target_version,
        instance_ids
    ))]
    FailUpdates {
        target_version: String,
        instance_ids: Vec<String>,
    },

    #[snafu(display("One or more hosts failed to report their OS version"))]
    OsVersionCheck,

    #[snafu(display("Failed to read file: {}", source))]
    FileRead { source: std::io::Error },

    #[snafu(display("Results location is invalid"))]
    ResultsLocation,
}
