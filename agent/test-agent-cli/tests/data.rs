#![allow(unused)]

use std::path::PathBuf;

pub fn integ_test_dependent_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("test-agent-cli/tests/data/deploy_test.yaml")
}
