use std::path::PathBuf;

/// Returns the path to the `hello-example` test.
pub fn hello_example_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("testsys/tests/data/hello-example.yaml")
}

/// Returns the path to the `example-resource-provider` test.
pub fn example_resource_provider_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("testsys/tests/data/example-resource-provider.yaml")
}
