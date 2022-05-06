use crate::pkg::Repository::Unknown;
use std::fmt::{Display, Formatter};

pub mod config;
pub mod npm;
pub mod policy;
mod recognizer;

#[cfg_attr(test, derive(Clone, PartialEq, Debug))]
pub enum Repository {
    Unknown,
    GitHub { organization: String, name: String },
    GitLab { organization: String, name: String },
    Raw { address: String },
}

impl Default for Repository {
    fn default() -> Self {
        Unknown
    }
}

impl Repository {
    pub fn url(&self) -> Option<String> {
        match self {
            Repository::GitHub { name, organization } => {
                Some(format!("https://github.com/{}/{}", organization, name))
            }
            Repository::GitLab { name, organization } => {
                Some(format!("https://gitlab.com/{}/{}", organization, name))
            }
            Repository::Raw { address } => Some(address.clone()),
            Repository::Unknown => None,
        }
    }
}

impl Display for Repository {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.url() {
            None => f.write_str("Unknown repository"),
            Some(url) => f.write_str(&url),
        }
    }
}
