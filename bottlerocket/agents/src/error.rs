use aws_sdk_ecs::error::SdkError as EcsSdkError;
use aws_sdk_ssm::error::SdkError as SsmSdkError;
use snafu::Snafu;
use std::num::TryFromIntError;
use test_agent::error::InfoClientError;
use tokio::time::error::Elapsed;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    #[snafu(display("Unable to build resource requirement: {}", source))]
    BuildResourceRequirement {
        source: aws_sdk_ecs::error::BuildError,
    },

    #[snafu(display("Unable to build instance information string filter: {}", source))]
    BuildInstanceInformationStringFilter {
        source: aws_sdk_ssm::error::BuildError,
    },

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

    #[snafu(display("{}", source))]
    SerializeJson { source: serde_json::Error },

    #[snafu(display("{}", source))]
    DeserializeYaml { source: serde_yaml::Error },

    #[snafu(display("{}", source))]
    SerializeYaml { source: serde_yaml::Error },

    #[snafu(display("Missing '{}' field from sonobuoy status", field))]
    MissingSonobuoyStatusField { field: String },

    #[snafu(display("AWS SDK failed: {}", message))]
    AwsSdk { message: String },

    #[snafu(display("SSM Create Document failed: {}", source))]
    SsmCreateDocument {
        source: SsmSdkError<aws_sdk_ssm::operation::create_document::CreateDocumentError>,
    },

    #[snafu(display("SSM Describe Document failed: {}", message))]
    SsmDescribeDocument { message: String },

    #[snafu(display("SSM Update Document failed: {}", source))]
    SsmUpdateDocument {
        source: SsmSdkError<aws_sdk_ssm::operation::update_document::UpdateDocumentError>,
    },

    #[snafu(display("SSM Send Command failed: {}", source))]
    SsmSendCommand {
        source: SsmSdkError<aws_sdk_ssm::operation::send_command::SendCommandError>,
    },

    #[snafu(display("SSM List Command Invocations failed: {}", source))]
    SsmListCommandInvocations {
        source: SsmSdkError<
            aws_sdk_ssm::operation::list_command_invocations::ListCommandInvocationsError,
        >,
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
        source: SsmSdkError<
            aws_sdk_ssm::operation::describe_instance_information::DescribeInstanceInformationError,
        >,
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

    #[snafu(display("Failed to write file: {path}"))]
    FileWrite {
        source: std::io::Error,
        path: String,
    },

    #[snafu(display("Results location is invalid"))]
    ResultsLocation,

    #[snafu(display("Unable to create task definition: {}", source))]
    TaskDefinitionCreation {
        source: EcsSdkError<
            aws_sdk_ecs::operation::register_task_definition::RegisterTaskDefinitionError,
        >,
    },

    #[snafu(display("Unable to describe task definition: {}", source))]
    TaskDefinitionDescribe {
        source: EcsSdkError<
            aws_sdk_ecs::operation::describe_task_definition::DescribeTaskDefinitionError,
        >,
    },

    #[snafu(display("Unable to list task definitions: {}", source))]
    TaskDefinitionList {
        source:
            EcsSdkError<aws_sdk_ecs::operation::list_task_definitions::ListTaskDefinitionsError>,
    },

    #[snafu(display("Unable to run task: {}", source))]
    TaskRunCreation {
        source: EcsSdkError<aws_sdk_ecs::operation::run_task::RunTaskError>,
    },

    #[snafu(display("Unable to update the service: {}", source))]
    TaskServiceUpdate {
        source: EcsSdkError<aws_sdk_ecs::operation::update_service::UpdateServiceError>,
    },

    #[snafu(display("Unable to delete service: {}", source))]
    TaskServiceDelete {
        source: EcsSdkError<aws_sdk_ecs::operation::delete_service::DeleteServiceError>,
    },

    #[snafu(display("Unable to get task description: {}", source))]
    TaskDescribe {
        source: EcsSdkError<aws_sdk_ecs::operation::describe_tasks::DescribeTasksError>,
    },

    #[snafu(display("Unable to get cluster description: {}", source))]
    ClusterDescribe {
        source: EcsSdkError<aws_sdk_ecs::operation::describe_clusters::DescribeClustersError>,
    },

    #[snafu(display("Unable to deregister task description: {}", source))]
    DeregisterTask {
        source: EcsSdkError<
            aws_sdk_ecs::operation::deregister_task_definition::DeregisterTaskDefinitionError,
        >,
    },

    #[snafu(display("No task running tasks in cluster"))]
    NoTask,

    #[snafu(display(
        "Sonobuoy status could not be retrieved within the given time: {}",
        source
    ))]
    SonobuoyTimeout { source: tokio::time::error::Elapsed },

    #[snafu(display("Failed to retrieve sonobuoy status after '{}' retries", retries))]
    SonobuoyStatus { retries: i32 },

    #[snafu(display("The task did not complete in time"))]
    TaskTimeout,

    #[snafu(display("Registered container instances did not start in time: {}", source))]
    InstanceTimeout { source: tokio::time::error::Elapsed },

    #[snafu(display("The default task does not exist"))]
    TaskExist,

    #[snafu(display("The task definition is missing"))]
    TaskDefinitionMissing,

    #[snafu(context(false))]
    #[snafu(display("{}", source))]
    Utils { source: agent_utils::Error },

    #[snafu(display("Failed to create workload process: {}", source))]
    WorkloadProcess { source: std::io::Error },

    #[snafu(display(
        "Failed to run workload test\nCode: {exit_code}\nStdout:\n{stdout}\nStderr:\n{stderr}"
    ))]
    WorkloadRun {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },

    #[snafu(display("Failed to initialize workload test plugin: {}", plugin))]
    WorkloadTest { plugin: String },

    #[snafu(display(
        "Failed to write workload test plugin configuration yaml for: {}",
        plugin
    ))]
    WorkloadWritePlugin { plugin: String },

    #[snafu(display("Failed to clean-up workload resources"))]
    WorkloadDelete,

    #[snafu(display("Missing '{}' field from workload status", field))]
    MissingWorkloadStatusField { field: String },

    #[snafu(display("Unable to send test update: {}", source), context(false))]
    InfoClient { source: InfoClientError },

    #[snafu(display("Unable convert usize to u64: {}", source), context(false))]
    Conversion { source: TryFromIntError },
}
