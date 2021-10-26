use snafu::Snafu;

#[derive(Debug, Snafu)]
pub struct Error(OpaqueError);
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub(crate) enum OpaqueError {
    #[snafu(display("Parse error: {}", source))]
    SerdePlain { source: serde_plain::Error },
}
