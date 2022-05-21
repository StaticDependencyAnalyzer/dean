use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Config {
    pub min_number_of_releases: usize,
    pub days: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_number_of_releases: 3,
            days: 365,
        }
    }
}
