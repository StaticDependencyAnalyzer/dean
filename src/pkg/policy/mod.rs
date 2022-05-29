use std::collections::HashMap;
use std::error::Error;

mod contributors_ratio;
pub mod max_issue_lifespan;
mod min_number_of_releases_required;

pub use contributors_ratio::ContributorsRatio;
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
pub trait CommitRetriever {
    /// Retrieves the commits for each tag.
    fn commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Result<HashMap<String, Vec<Commit>>, Box<dyn Error>>;

    /// Retrieves all the tags from a repository ordered by time, where the latest one is the most recent.
    fn all_tags(&self, repository_url: &str) -> Result<Vec<Tag>, Box<dyn Error>>;
}

#[cfg_attr(test, mockall::automock)]
pub trait Clock {
    /// Retrieves the current timestamp
    fn now_timestamp(&self) -> u64;
}

#[derive(Clone, Debug)]
pub enum Evaluation {
    Pass(Dependency),
    Fail(Dependency, String),
}

impl PartialEq for Evaluation {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Evaluation::Pass(_), Evaluation::Pass(_))
                | (Evaluation::Fail(_, _), Evaluation::Fail(_, _))
        )
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait Policy: Send + Sync {
    /// Evaluates the policy.
    fn evaluate(&self, dependency: &Dependency) -> Result<Evaluation, Box<dyn Error>>;
}
