use serde::Deserialize;

/// Test settings provides a way to send arguments into the Rust tests using environment variables.
pub(super) struct TestSettings {}

impl TestSettings {
    /// The path or name of the `kind` binary.
    pub(super) fn kind_path() -> &'static str {
        TEST_SETTINGS.kind_path.as_str()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "SCREAMING_SNAKE_CASE")]
struct Inner {
    /// The path to the [kind] binary. Defaults to `kind` (i.e. by default the kind binary is
    /// expected to be found via `$PATH`).
    ///
    /// # Example
    ///
    /// ```text
    /// TESTSYS_SELFTEST_KIND_PATH=/wherever/kind
    /// ```
    ///
    /// [kind]: https://kind.sigs.k8s.io/
    #[serde(default = "kind")]
    kind_path: String,
}

lazy_static::lazy_static! {
    static ref TEST_SETTINGS: Inner =
        envy::prefixed("TESTSYS_SELFTEST_")
            .from_env::<Inner>()
            .expect("Error parsing TestSettings environment variables");
}

/// We need this to provide a default for serde.
fn kind() -> String {
    String::from("kind")
}
