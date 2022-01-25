use model::constants::SECRETS_PATH;
use model::SecretName;
use snafu::{OptionExt, ResultExt};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::PathBuf;

/// Reads the keys (which become files) and values of a Kubernetes generic/[opaque] secret.
/// [opaque]: https://kubernetes.io/docs/concepts/configuration/secret/#opaque-secrets
pub struct SecretsReader {
    /// The directory where secrets are mounted.
    dir: PathBuf,
}

#[derive(Debug)]
pub struct Error {
    name: SecretName,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl Error {
    pub fn new(name: SecretName) -> Self {
        Self { name, source: None }
    }

    pub fn new_with_source<E>(name: SecretName, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            name,
            source: Some(source.into()),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            None => write!(f, "Unable to read secret '{}'", self.name),
            Some(e) => write!(f, "Unable to read secret '{}': {}", self.name, e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|some| some.as_ref() as &(dyn std::error::Error + 'static))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type SecretData = BTreeMap<String, Vec<u8>>;

impl SecretsReader {
    /// Create a new `SecretsReader` that looks for secrets in the secrets directory that TestSys
    /// expects for agent containers.
    pub fn new() -> SecretsReader {
        Self {
            dir: PathBuf::from(SECRETS_PATH),
        }
    }

    /// Get the key/value pairs from a Kubernetes generic/[opaque] secret.
    /// [opaque]: https://kubernetes.io/docs/concepts/configuration/secret/#opaque-secrets
    pub fn get_secret(&self, secret_name: &SecretName) -> Result<SecretData> {
        let mut map = SecretData::new();
        let directory = self.dir.join(secret_name.as_str());
        let read_dir = fs::read_dir(&directory).with_context(|_| error::ListDirectorySnafu {
            name: secret_name.to_owned(),
            directory: &directory,
        })?;
        for entry in read_dir.map(|result| {
            result.with_context(|_| error::ListDirectorySnafu {
                name: secret_name.to_owned(),
                directory: &directory,
            })
        }) {
            let entry = entry?;
            if entry.path().is_file() {
                let path = entry.path();
                let key = path
                    .file_name()
                    .with_context(|| error::MissingFilenameSnafu {
                        name: secret_name.to_owned(),
                        path: &path,
                    })?
                    .to_str()
                    .with_context(|| error::NonUtf8FilenameSnafu {
                        name: secret_name.to_owned(),
                        path: &path,
                    })?;
                let value = fs::read(&path).with_context(|_| error::ReadFileSnafu {
                    name: secret_name.to_owned(),
                    path: &path,
                })?;
                map.insert(key.into(), value);
            }
        }
        Ok(map)
    }
}

impl Default for SecretsReader {
    fn default() -> Self {
        SecretsReader::new()
    }
}

mod error {
    use model::SecretName;
    use snafu::Snafu;
    use std::path::PathBuf;

    #[derive(Debug, Snafu)]
    #[snafu(visibility(pub(super)))]
    pub enum OpaqueError {
        #[snafu(display("Unable to list contents of directory '{}': {}", directory.display(), source))]
        ListDirectory {
            name: SecretName,
            directory: PathBuf,
            source: std::io::Error,
        },

        #[snafu(display("Unable to get filename from path '{}'", path.display()))]
        MissingFilename { name: SecretName, path: PathBuf },

        #[snafu(display("Non-UTF8 filename in path '{}'", path.display()))]
        NonUtf8Filename { name: SecretName, path: PathBuf },

        #[snafu(display("Unable to read file '{}': {}", path.display(), source))]
        ReadFile {
            name: SecretName,
            path: PathBuf,
            source: std::io::Error,
        },
    }

    impl OpaqueError {
        fn secret_name(&self) -> &SecretName {
            match self {
                OpaqueError::ListDirectory { name, .. } => name,
                OpaqueError::MissingFilename { name, .. } => name,
                OpaqueError::NonUtf8Filename { name, .. } => name,
                OpaqueError::ReadFile { name, .. } => name,
            }
        }
    }

    impl From<OpaqueError> for super::Error {
        fn from(e: OpaqueError) -> Self {
            let name = e.secret_name().to_owned();
            super::Error::new_with_source(name, e)
        }
    }
}

#[cfg(test)]
impl SecretsReader {
    /// Create a new `SecretsReader` that looks for secrets in a custom directory.
    pub fn new_custom_directory<P>(directory: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            dir: directory.into(),
        }
    }
}

#[test]
fn test() {
    let tempdir = tempfile::TempDir::new().unwrap();
    let dir = tempdir.path();
    let key1 = "piano";
    let value1 = "lake";
    let key2 = "bread";
    let value2 = "mall";
    let secret_name = SecretName::new("poet").unwrap();
    let secret_dir = dir.join(secret_name.as_str());
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(secret_dir.join(key1), &value1).unwrap();
    fs::write(secret_dir.join(key2), &value2).unwrap();
    let secrets = SecretsReader::new_custom_directory(&dir);
    let data = secrets.get_secret(&secret_name).unwrap();
    assert_eq!(
        String::from_utf8(data.get(key1).unwrap().to_owned()).unwrap(),
        value1
    );
    assert_eq!(
        String::from_utf8(data.get(key2).unwrap().to_owned()).unwrap(),
        value2
    );
}
