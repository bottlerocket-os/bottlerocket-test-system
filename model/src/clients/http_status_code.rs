pub use http::StatusCode;
use kube::Error;
use std::fmt::Display;

pub trait HttpStatusCode {
    fn status_code(&self) -> Option<StatusCode>;

    fn is_status_code(&self, status_code: StatusCode) -> bool {
        self.status_code()
            .map(|some| some == status_code)
            .unwrap_or_default()
    }
}

impl HttpStatusCode for kube::Error {
    fn status_code(&self) -> Option<StatusCode> {
        if let Error::Api(error_response) = self {
            StatusCode::from_u16(error_response.code).ok()
        } else {
            None
        }
    }
}

impl<T, E> HttpStatusCode for std::result::Result<T, E>
where
    E: HttpStatusCode,
{
    fn status_code(&self) -> Option<StatusCode> {
        self.as_ref().err().and_then(|e| e.status_code())
    }
}

pub trait AllowNotFound<T, E>
where
    E: HttpStatusCode + Display,
{
    /// When an operation returns a `Result`, sometimes it is ok if that result is a `404`. For
    /// example, if you are deleting something and if it is fine for the object you were trying to
    /// delete to not exist.
    ///
    /// In this case you can call `.is_found()` to transform the `Result` into a `bool` with the
    /// following logic:
    ///
    /// Returns `Ok(true)` if the the result was `Ok`. Returns `Ok(false)` if the result was `Err`
    /// but the error was a `404`. Returns `Err(e)` for any error that is not a `404`.
    ///
    /// If you want to log the error or do anything else with it in the case of a `404` then you can
    /// do so in `handle_not_found`.
    ///
    #[allow(clippy::wrong_self_convention)]
    fn allow_not_found<O>(self, handle_not_found: O) -> std::result::Result<Option<T>, E>
    where
        O: FnOnce(E);
}

impl<T, E> AllowNotFound<T, E> for std::result::Result<T, E>
where
    E: HttpStatusCode + Display,
{
    fn allow_not_found<O>(self, handle_not_found: O) -> Result<Option<T>, E>
    where
        O: FnOnce(E),
    {
        match self {
            Ok(obj) => Ok(Some(obj)),
            Err(e) => {
                if e.is_status_code(StatusCode::NOT_FOUND) {
                    handle_not_found(e);
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}
