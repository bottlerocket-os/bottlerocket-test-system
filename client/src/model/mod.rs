mod resource_provider;
mod test;

pub use resource_provider::{ResourceProvider, ResourceProviderSpec, ResourceProviderStatus};
pub use test::{
    AgentStatus, ControllerStatus, ResourceStatus, RunState, Test, TestResults, TestSpec,
    TestStatus,
};

pub const TESTSYS_NAMESPACE: &str = "testsys-bottlerocket-aws";
pub const TESTSYS_API: &str = "testsys.bottlerocket.aws/v1";
