use crate::pkg::policy;
use std::time;

#[derive(Default, Copy, Clone)]
pub struct Clock {}

impl policy::Clock for Clock {
    fn now_timestamp(&self) -> u64 {
        time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
