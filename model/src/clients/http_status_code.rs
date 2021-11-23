pub use http::StatusCode;
use kube::Error;

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
