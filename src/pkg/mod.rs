use std::fmt::{Display, Formatter};

use lazy_static::lazy_static;
use regex::Regex;

use crate::Evaluation;

pub mod config;
pub mod csv;
pub mod engine;
pub mod iter;
pub mod package_manager;
pub mod policy;
pub mod recognizer;

#[cfg_attr(test, mockall::automock)]
pub trait InfoRetriever: Sync + Send {
    fn latest_version(&self, dependency: &str) -> Result<String, String>;
    fn repository(&self, dependency: &str) -> Result<Repository, String>;
}

pub trait DependencyRetriever: Sync + Send {
    type Itr: IntoIterator<Item = Dependency>;
    fn dependencies(&self) -> Result<Self::Itr, String>;
}

pub trait ResultReporter {
    fn report_results<T>(&mut self, result: T) -> Result<(), String>
    where
        T: IntoIterator<Item = Evaluation>;
}

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
pub enum Repository {
    Unknown,
    GitHub { organization: String, name: String },
    GitLab { organization: String, name: String },
    Raw { address: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub latest_version: Option<String>,
    pub repository: Repository,
}

impl Default for Repository {
    fn default() -> Self {
        Self::Unknown
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

    pub fn parse_url(repository: &str) -> Self {
        lazy_static! {
            static ref GITHUB_REGISTRY_REGEX: Regex =
                Regex::new(".*?github.com[:/](?P<organization>.*?)/(?P<name>.*?)(?:$|\\.git|/)")
                    .unwrap();
            static ref GITLAB_REGISTRY_REGEX: Regex =
                Regex::new(".*?gitlab.com[:/](?P<organization>.*?)/(?P<name>.*?)(?:$|\\.git|/)")
                    .unwrap();
        }

        if repository.trim().is_empty() {
            return Repository::Unknown;
        }

        if GITHUB_REGISTRY_REGEX.is_match(repository) {
            let captures = GITHUB_REGISTRY_REGEX.captures(repository).unwrap();

            return Repository::GitHub {
                organization: captures["organization"].to_string(),
                name: captures["name"].to_string(),
            };
        }

        if GITLAB_REGISTRY_REGEX.is_match(repository) {
            let captures = GITLAB_REGISTRY_REGEX.captures(repository).unwrap();

            return Repository::GitLab {
                organization: captures["organization"].to_string(),
                name: captures["name"].to_string(),
            };
        }

        Repository::Raw {
            address: repository.to_string(),
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
