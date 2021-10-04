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
pub const CONTROLLER: &str = "testsys-controller";
pub const TEST_AGENT: &str = "testsys-test-agent";
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
pub const ENV_RESOURCE_PROVIDER_NAME: &str = "TESTSYS_RESOURCE_PROVIDER";
pub const ENV_TEST_NAME: &str = "TESTSYS_TEST_NAME";

// Standard tags https://kubernetes.io/docs/concepts/overview/working-with-objects/common-labels/
pub const APP_NAME: &str = "app.kubernetes.io/name";
pub const APP_INSTANCE: &str = "app.kubernetes.io/instance";
pub const APP_COMPONENT: &str = "app.kubernetes.io/component";
pub const APP_PART_OF: &str = "app.kubernetes.io/part-of";
pub const APP_MANAGED_BY: &str = "app.kubernetes.io/managed-by";
pub const APP_CREATED_BY: &str = "app.kubernetes.io/created-by";

#[test]
fn testsys_constants_macro_test() {
    assert_eq!("testsys.bottlerocket.aws", testsys!());
    assert_eq!("testsys.bottlerocket.aws/v1", API_VERSION);
    assert_eq!("testsys.bottlerocket.aws/foo", testsys!("foo"));
}
