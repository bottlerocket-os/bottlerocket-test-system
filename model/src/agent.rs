use crate::error::{self, Error, Result};
use k8s_openapi::serde::Deserializer;
use regex::Regex;
use schemars::gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject, StringValidation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use snafu::ensure;
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

/// The states that an agent declares about its task (e.g. running tests or creating/destroying
/// resources).
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone, Copy, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Unknown,
    Running,
    Completed,
    Error,
}

impl Default for TaskState {
    fn default() -> Self {
        Self::Unknown
    }
}

serde_plain::derive_display_from_serialize!(TaskState);

#[derive(Serialize, Deserialize, Debug, Default, Eq, PartialEq, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// The name of the agent.
    pub name: String,
    /// The URI of the agent container image.
    pub image: String,
    /// The name of an image registry pull secret if one is needed to pull the agent image.
    pub pull_secret: Option<String>,
    /// Determine if the pod should keep running after it has finished or encountered and error.
    pub keep_running: bool,
    /// The maximum amount of time an agent should be left to run.
    #[schemars(schema_with = "timeout_schema")]
    pub timeout: Option<String>,
    /// The configuration to pass to the agent. This is 'open' to allow agents to define their own
    /// schemas.
    #[schemars(schema_with = "config_schema")]
    pub configuration: Option<Map<String, Value>>,
    /// A map of `SecretType` -> `SecretName` where `SecretType` is defined by the agent that will
    /// use it, and `SecretName` is provided by the user. `SecretName` is constrained to ascii
    /// alphanumerics plus underscores and dashes.
    pub secrets: Option<BTreeMap<SecretType, SecretName>>,
    /// Linux capabilities to add for the agent container, e.g. NET_ADMIN
    pub capabilities: Option<Vec<String>>,
    /// Whether the agent container needs to be privileged or not
    pub privileged: Option<bool>,
}

impl Agent {
    pub fn secret_names(&self) -> BTreeSet<&SecretName> {
        self.secrets
            .as_ref()
            .map(|secrets_map| secrets_map.values().collect::<BTreeSet<&SecretName>>())
            .unwrap_or_default()
    }
}

pub fn config_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    let mut extensions = BTreeMap::<String, Value>::new();
    extensions.insert("nullable".to_string(), Value::Bool(true));
    extensions.insert(
        "x-kubernetes-preserve-unknown-fields".to_string(),
        Value::Bool(true),
    );
    let schema = SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        extensions,
        ..SchemaObject::default()
    };
    schema.into()
}

pub fn timeout_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
    let mut extensions = BTreeMap::<String, Value>::new();
    extensions.insert("nullable".to_string(), Value::Bool(true));
    let schema = SchemaObject {
        string: Some(Box::new(StringValidation {
            max_length: Some(253),
            min_length: Some(1),
            pattern: Some(r"^((([0-9]+)d)?(([0-9]+)h)?(([0-9]+)m)?(([0-9]+)s)?|\d+)$".to_string()),
        })),
        instance_type: Some(InstanceType::String.into()),
        extensions,
        ..SchemaObject::default()
    };
    schema.into()
}

/// The type of a secret, as defined and required by an agent. Possible examples: `foo-credentials`,
/// `bar-api-key`, etc.
pub type SecretType = String;

/// The name of secret. This may be used as a file name and thus has a pattern restriction allowing
/// only ascii alphanumeric, underscore and dash characters.
#[derive(Serialize, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
#[serde(transparent)]
pub struct SecretName(String);

impl SecretName {
    pub fn new<S>(value: S) -> Result<Self>
    where
        S: Into<String>,
    {
        let s = Self::validate(value)?;
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    const PATTERN_REGEX: &'static str = "^[a-zA-Z0-9_-]{1,253}$";

    fn validate<S>(value: S) -> Result<String>
    where
        S: Into<String>,
    {
        let s = value.into();
        ensure!(
            REGEX.is_match(&s),
            error::SecretNameValidationSnafu {
                secret_name: s.as_str(),
                regex: Self::PATTERN_REGEX
            }
        );
        Ok(s)
    }
}

lazy_static::lazy_static! {

    static ref REGEX: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(SecretName::PATTERN_REGEX).unwrap()
    };
}

impl JsonSchema for SecretName {
    fn schema_name() -> String {
        "secret_type".into()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        let schema = SchemaObject {
            metadata: None,
            instance_type: Some(InstanceType::String.into()),
            string: Some(Box::new(StringValidation {
                max_length: Some(253),
                min_length: Some(1),
                pattern: Some(Self::PATTERN_REGEX.into()),
            })),
            ..SchemaObject::default()
        };
        schema.into()
    }
}

impl AsRef<str> for SecretName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<String> for SecretName {
    fn as_ref(&self) -> &String {
        &self.0
    }
}

impl Deref for SecretName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl Borrow<String> for SecretName {
    fn borrow(&self) -> &String {
        &self.0
    }
}

impl Borrow<str> for SecretName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Display for SecretName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for SecretName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl FromStr for SecretName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        SecretName::new(s)
    }
}

impl TryFrom<&str> for SecretName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        SecretName::new(value)
    }
}

impl TryFrom<&String> for SecretName {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self> {
        SecretName::new(value)
    }
}

impl TryFrom<String> for SecretName {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        SecretName::new(value)
    }
}

impl<'de> Deserialize<'de> for SecretName {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SecretName::new(s).map_err(|e| serde::de::Error::custom(format!("{}", e)))
    }
}

#[test]
fn bad_secret_name_1() {
    let input = "bad/name/1";
    assert!(SecretName::new(input).err().is_some())
}

#[test]
fn bad_secret_name_2() {
    let input = "";
    assert!(SecretName::new(input).err().is_some())
}

#[test]
fn good_secret_name_1() {
    let input = "-";
    let secret_name = SecretName::new(input).unwrap();
    assert_eq!(secret_name.as_str(), input);
}

#[test]
fn good_secret_name_2() {
    let input = "0-1_foO";
    let secret_name = SecretName::new(input).unwrap();
    assert_eq!(secret_name.as_str(), input);
}

#[test]
fn secret_name_deserialize() {
    use serde_json::json;
    #[derive(Deserialize)]
    struct Something {
        foo: SecretName,
    }
    let bad_json = json!({ "foo": "/" });
    assert!(serde_json::from_value::<Something>(bad_json).is_err());
    let good_json = json!({ "foo": "bar-baz" });
    let deserialized = serde_json::from_value::<Something>(good_json).unwrap();
    assert_eq!(deserialized.foo.as_str(), "bar-baz");
}
