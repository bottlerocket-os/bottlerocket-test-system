use http::StatusCode;
use snafu::Snafu;

pub(crate) type JobResult<T> = std::result::Result<T, JobError>;

#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(super)")]
pub(crate) enum JobError {
    #[snafu(display("Job already exists: {}", source))]
    AlreadyExists { source: kube::error::ErrorResponse },

    #[snafu(display("Unable to create job: {}", source))]
    Create { source: kube::Error },

    #[snafu(display("Unable to delete job: {}", source))]
    Delete { source: kube::Error },

    #[snafu(display("Unable to get job: {}", source))]
    Get { source: kube::Error },

    #[snafu(display("Job does not exist: {}", source))]
    NotFound { source: kube::error::ErrorResponse },

    #[snafu(display(
        "There should be only one container for job '{}' but found {} running, {} succeeded, and {} failed",
        job_name,
        running,
        succeeded,
        failed,
    ))]
    TooManyJobContainers {
        job_name: String,
        running: i32,
        succeeded: i32,
        failed: i32,
    },
}

impl JobError {
    /// Check if the error is a 409 (`conflict`, which happens when the job already exists),
    /// otherwise return a `Create` error.
    pub(super) fn create(e: kube::Error) -> Self {
        match e {
            kube::Error::Api(response) if response.code == StatusCode::CONFLICT => {
                JobError::AlreadyExists { source: response }
            }
            _ => JobError::Create { source: e },
        }
    }

    /// Check if the error is a 404 (`not found`), otherwise return a `Delete` error.
    pub(super) fn delete(e: kube::Error) -> Self {
        match e {
            kube::Error::Api(response) if response.code == StatusCode::NOT_FOUND => {
                JobError::NotFound { source: response }
            }
            _ => JobError::Delete { source: e },
        }
    }

    /// Check if the error is a 404 (`not found`), otherwise return a `Get` error.
    pub(super) fn get(e: kube::Error) -> Self {
        match e {
            kube::Error::Api(response) if response.code == StatusCode::NOT_FOUND => {
                JobError::NotFound { source: response }
            }
            _ => JobError::Get { source: e },
        }
    }
}
