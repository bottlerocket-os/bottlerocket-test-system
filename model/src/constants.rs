/// Helper macro to avoid retyping the base domain-like name of our system when creating further
/// string constants from it. When given no parameters, this returns the base domain-like name of
/// the system. When given a string literal parameter it adds `/parameter` to the end.
macro_rules! testsys {
    () => {
        "testsys.bottlerocket.aws"
    };
    ($s:literal) => {
        concat!(testsys!(), "/", $s)
    };
}

// System identifiers
pub const API_VERSION: &str = testsys!("v1");
pub const NAMESPACE: &str = "testsys-bottlerocket-aws";
pub const TESTSYS: &str = testsys!();

// Component names
pub const CONTROLLER: &str = "controller";
pub const RESOURCE_AGENT: &str = "testsys-resource-agent";
pub const RESOURCE_AGENT_BINDING: &str = "testsys-resource-agent-role-binding";
pub const RESOURCE_AGENT_ROLE: &str = "testsys-resource-agent-role";
pub const RESOURCE_AGENT_SERVICE_ACCOUNT: &str = "testsys-resource-agent-account";
pub const TEST_AGENT: &str = "testsys-test-agent";
pub const TEST_AGENT_BINDING: &str = "testsys-test-agent-role-binding";
pub const TEST_AGENT_ROLE: &str = "testsys-test-agent-role";
pub const TEST_AGENT_SERVICE_ACCOUNT: &str = "testsys-test-agent-account";

// Label keys
pub const LABEL_TEST_NAME: &str = testsys!("test-name");
pub const LABEL_TEST_UID: &str = testsys!("test-uid");
pub const LABEL_PROVIDER_NAME: &str = testsys!("provider-name");
pub const LABEL_COMPONENT: &str = testsys!("component");

// Environment variables
pub const ENV_PROVIDER_NAME: &str = "TESTSYS_PROVIDER_NAME";
pub const ENV_RESOURCE_ACTION: &str = "TESTSYS_RESOURCE_ACTION";
pub const ENV_RESOURCE_NAME: &str = "TESTSYS_RESOURCE_NAME";
pub const ENV_TEST_NAME: &str = "TESTSYS_TEST_NAME";

// Paths
pub const SECRETS_PATH: &str = "/secrets";

// Standard tags https://kubernetes.io/docs/concepts/overview/working-with-objects/common-labels/
pub const APP_NAME: &str = "app.kubernetes.io/name";
pub const APP_INSTANCE: &str = "app.kubernetes.io/instance";
pub const APP_COMPONENT: &str = "app.kubernetes.io/component";
pub const APP_PART_OF: &str = "app.kubernetes.io/part-of";
pub const APP_MANAGED_BY: &str = "app.kubernetes.io/managed-by";
pub const APP_CREATED_BY: &str = "app.kubernetes.io/created-by";

// Names of finalizers used by the controller/CRDs
pub const FINALIZER_CREATION_JOB: &str = testsys!("resource-creation-job");
pub const FINALIZER_MAIN: &str = testsys!("controlled");
pub const FINALIZER_RESOURCE: &str = testsys!("resources-exist");
pub const FINALIZER_TEST_JOB: &str = testsys!("test-job");

pub const TESTSYS_RESULTS_FILE: &str = "/output.tar.gz";
pub const TESTSYS_RESULTS_DIRECTORY: &str = "/output";

// Used by the controller to truncate resource names
pub const TRUNC_LEN: usize = 15;

#[test]
fn testsys_constants_macro_test() {
    assert_eq!("testsys.bottlerocket.aws", testsys!());
    assert_eq!("testsys.bottlerocket.aws/v1", API_VERSION);
    assert_eq!("testsys.bottlerocket.aws/foo", testsys!("foo"));
}
