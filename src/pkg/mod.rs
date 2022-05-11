use std::fmt::{Display, Formatter};

use crate::pkg::Repository::Unknown;

pub mod config;
pub mod npm;
pub mod policy;
pub mod recognizer;

#[cfg_attr(test, mockall::automock)]
pub trait InfoRetriever {
    fn latest_version(&self, dependency: &str) -> Result<String, String>;
    fn repository(&self, dependency: &str) -> Result<Repository, String>;
}

pub trait DependencyRetriever {
    fn dependencies(&self) -> Result<Vec<Dependency>, String>;
}

#[cfg_attr(test, derive(Clone, PartialEq, Debug))]
pub enum Repository {
    Unknown,
    GitHub { organization: String, name: String },
    GitLab { organization: String, name: String },
    Raw { address: String },
}

#[cfg_attr(test, derive(Clone, PartialEq, Debug, Default))]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub latest_version: Option<String>,
    pub repository: Repository,
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
