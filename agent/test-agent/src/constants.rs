use std::time::Duration;

pub(crate) const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const STATUS_CHECK_WAIT: Duration = Duration::from_secs(2);
pub(crate) const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const TERMINATE_TIMEOUT: Duration = Duration::from_secs(30);
