use std::fmt::{Display, Formatter};

/// An error that can occur when parsing a string into an enum.
#[derive(derive_more::Error, Debug)]
pub struct ParseError(serde_plain::Error);

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl ParseError {
    pub(crate) fn new(e: serde_plain::Error) -> Self {
        Self(e)
    }
}
