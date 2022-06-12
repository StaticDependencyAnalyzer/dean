use std::collections::HashMap;
use std::error::Error;

use crate::pkg::Repository;

mod contributors_ratio;
mod max_issue_lifespan;
mod max_pull_request_lifespan;
mod min_number_of_releases_required;

pub use contributors_ratio::ContributorsRatio;
pub use max_issue_lifespan::MaxIssueLifespan;
pub use max_pull_request_lifespan::MaxPullRequestLifespan;
pub use min_number_of_releases_required::MinNumberOfReleasesRequired;

use crate::Dependency;

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub author_name: String,
    pub author_email: String,
    pub creation_timestamp: i64,
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Tag {
    pub name: String,
    pub commit_id: String,
    pub commit_timestamp: u64,
}

#[cfg_attr(test, mockall::automock)]
pub trait CommitRetriever: Sync + Send {
    /// Retrieves the commits for each tag.
    fn commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Result<HashMap<String, Vec<Commit>>, Box<dyn Error>>;

    /// Retrieves all the tags from a repository ordered by time, where the latest one is the most recent.
    fn all_tags(&self, repository_url: &str) -> Result<Vec<Tag>, Box<dyn Error>>;
}

#[cfg_attr(test, mockall::automock)]
pub trait ContributionDataRetriever: Send + Sync {
    fn get_issue_lifespan(
        &self,
        repository: &Repository,
        last_issues: usize,
    ) -> Result<f64, Box<dyn Error>>;
    fn get_pull_request_lifespan(
        &self,
        repository: &Repository,
        last_pull_requests: usize,
    ) -> Result<f64, Box<dyn Error>>;
}

#[cfg_attr(test, mockall::automock)]
pub trait Clock: Sync + Send {
    /// Retrieves the current timestamp
    fn now_timestamp(&self) -> u64;
}

#[derive(Clone, Debug)]
pub enum Evaluation {
    Pass(String, Dependency),
    Fail(String, Dependency, String),
}

impl PartialEq for Evaluation {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Evaluation::Fail(policy, _, _), Evaluation::Fail(policy2, _, _))
            | (Evaluation::Pass(policy, _), Evaluation::Pass(policy2, _)) => policy == policy2,
            _ => false,
        }
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait Policy: Send + Sync {
    /// Evaluates the policy.
    fn evaluate(&self, dependency: &Dependency) -> Result<Evaluation, Box<dyn Error>>;
}
