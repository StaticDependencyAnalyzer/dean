use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub max_lifespan_in_seconds: usize,
    pub last_pull_requests: usize,
}

impl Default for Config {
    fn default() -> Self {
        let month_in_seconds = 60 * 60 * 24 * 30;
        Self {
            max_lifespan_in_seconds: month_in_seconds,
            last_pull_requests: 300,
        }
    }
}
