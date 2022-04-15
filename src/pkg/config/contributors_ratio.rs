use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Config {
    pub max_number_of_releases_to_check: usize,
    pub max_contributor_ratio: f64,
    pub skip: Vec<String>,
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_number_of_releases_to_check: 3,
            max_contributor_ratio: 0.5,
            skip: vec![],
            enabled: true,
        }
    }
}
