use std::path::PathBuf;

pub fn integ_test_dependent_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("testsys/tests/data/integ-test-dependent.yaml")
}

pub fn integ_test_depended_on_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("testsys/tests/data/integ-test-depended-on.yaml")
}
