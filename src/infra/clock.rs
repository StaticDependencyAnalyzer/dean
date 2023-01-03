use std::time;

use crate::pkg::policy;

#[derive(Default, Copy, Clone)]
pub struct System {}

impl System {
    pub fn new() -> Self {
        Self {}
    }
}

impl policy::Clock for System {
    fn now_timestamp(&self) -> u64 {
        time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
