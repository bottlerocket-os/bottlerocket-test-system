use aws_sdk_ecs::error::{
    DeleteServiceError, DeregisterTaskDefinitionError, DescribeClustersError,
    DescribeTaskDefinitionError, DescribeTasksError, RegisterTaskDefinitionError, RunTaskError,
    UpdateServiceError,
};
use aws_sdk_ssm::error::{
    CreateDocumentError, DescribeInstanceInformationError, ListCommandInvocationsError,
    SendCommandError, UpdateDocumentError,
};
use aws_sdk_ssm::SdkError;
use snafu::Snafu;
use std::string::FromUtf8Error;
use tokio::time::error::Elapsed;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    #[snafu(display("Failed to base64-decode '{}' for test cluster: {}", what, source))]
    Base64Decode {
        what: String,
        source: base64::DecodeError,
    },

    #[snafu(display("Could not convert '{}' secret to string: {}", what, source))]
    Conversion { what: String, source: FromUtf8Error },

    #[snafu(display("Failed to setup environment variables: {}", what))]
    EnvSetup { what: String },

    #[snafu(display("Failed to write '{}': {}", what, source))]
    Write {
        what: String,
        source: std::io::Error,
    },

    #[snafu(display("Secret was missing: {}", source))]
    SecretMissing {
        source: agent_common::secrets::Error,
    },

    #[snafu(display("Wireguard configuration missing from wireguard secret data"))]
    WireguardConfMissing,

    #[snafu(display("Failed to run wireguard to set up wireguard VPN tunnel: {}", stderr))]
    WireguardRun { stderr: String },

    #[snafu(display("Failed to create sonobuoy process: {}", source))]
    SonobuoyProcess { source: std::io::Error },

    #[snafu(display("Failed to create '{}' process: {}", what, source))]
    Process {
        what: String,
        source: std::io::Error,
    },

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

    #[snafu(display("Timed-out waiting for SSM agents to become ready: {}", source))]
    SsmWaitInstanceReadyTimeout { source: Elapsed },

    #[snafu(display("Failed to run '{}' in '{:?}'", document_name, instance_ids))]
    SsmRunCommand {
        document_name: String,
        instance_ids: Vec<String>,
    },

    #[snafu(display("SSM Describe Instance Information failed: {}", source))]
    SsmDescribeInstanceInfo {
        source: SdkError<DescribeInstanceInformationError>,
    },

    #[snafu(display("Missing instance information from describe-instance-info output"))]
    SsmInstanceInfo,

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

    #[snafu(display("Unable to create task defininition: {}", source))]
    TaskDefinitionCreation {
        source: SdkError<RegisterTaskDefinitionError>,
    },

    #[snafu(display("Unable to describe task definition: {}", source))]
    TaskDefinitionDescribe {
        source: SdkError<DescribeTaskDefinitionError>,
    },

    #[snafu(display("Unable to run task: {}", source))]
    TaskRunCreation { source: SdkError<RunTaskError> },

    #[snafu(display("Unable to update the service: {}", source))]
    TaskServiceUpdate {
        source: SdkError<UpdateServiceError>,
    },

    #[snafu(display("Unable to delete service: {}", source))]
    TaskServiceDelete {
        source: SdkError<DeleteServiceError>,
    },

    #[snafu(display("Unable to get task description: {}", source))]
    TaskDescribe {
        source: SdkError<DescribeTasksError>,
    },

    #[snafu(display("Unable to get cluster description: {}", source))]
    ClusterDescribe {
        source: SdkError<DescribeClustersError>,
    },

    #[snafu(display("Unable to deregister task description: {}", source))]
    DeregisterTask {
        source: SdkError<DeregisterTaskDefinitionError>,
    },

    #[snafu(display("No task running tasks in cluster"))]
    NoTask,

    #[snafu(display("The task did not complete in time"))]
    TaskTimeout,

    #[snafu(display("Registered container instances did not start in time: {}", source))]
    InstanceTimeout { source: tokio::time::error::Elapsed },

    #[snafu(display("The default task does not exist"))]
    TaskExist,

    #[snafu(display("The task definition is missing"))]
    TaskDefinitionMissing,
}
