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

// Environment variables
pub const ENV_TEST_NAME: &str = "TESTSYS_TEST_NAME";

#[test]
fn testsys_constants_macro_test() {
    assert_eq!("testsys.bottlerocket.aws", testsys!());
    assert_eq!("testsys.bottlerocket.aws/v1", API_VERSION);
    assert_eq!("testsys.bottlerocket.aws/foo", testsys!("foo"));
}
