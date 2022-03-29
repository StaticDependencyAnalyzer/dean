use crate::pkg::npm::Repository;
use anyhow::Context;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

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

pub struct MinNumberOfReleasesRequired {
    retriever: Box<dyn CommitRetriever>,
    number_of_releases: usize,
    duration: Duration,
    clock: Box<dyn Clock>,
}

impl MinNumberOfReleasesRequired {
    pub fn check(&self, repository: &Repository) -> Result<Evaluation, Box<dyn Error>> {
        let repository_url = repository
            .url()
            .context("the repository did not contain a URL")?;
        let all_tags = self.retriever.all_tags(&repository_url)?;

        let now = self.clock.now_timestamp();
        let num_tags_in_range = all_tags
            .iter()
            .rev()
            .take(self.number_of_releases)
            .filter(|&tag| tag.commit_timestamp >= now - self.duration.as_secs())
            .count();

        if num_tags_in_range == self.number_of_releases {
            Ok(Evaluation::Pass)
        } else {
            Ok(Evaluation::Fail)
        }
    }
}

impl MinNumberOfReleasesRequired {
    pub fn new(
        retriever: Box<dyn CommitRetriever>,
        number_of_releases: usize,
        duration: Duration,
        clock: Box<dyn Clock>,
    ) -> Self {
        Self {
            retriever,
            number_of_releases,
            duration,
            clock,
        }
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Evaluation {
    Pass,
    Fail,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::npm::Repository::GitHub;
    use crate::pkg::policy::Evaluation;
    use expects::matcher::{be_ok, equal};
    use expects::Subject;
    use mockall::predicate::eq;

    use std::time::Duration;

    #[test]
    fn when_there_are_more_than_2_releases_in_last_6_months_it_should_pass_the_policy_evaluation() {
        let mut retriever = Box::new(MockCommitRetriever::new());
        retriever
            .expect_all_tags()
            .with(eq("https://github.com/some_org/some_repo"))
            .returning(|_| {
                Ok(vec![
                    Tag {
                        name: "v0.1.2".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_640_477_360,
                    },
                    Tag {
                        name: "v0.1.3".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_641_477_360,
                    },
                    Tag {
                        name: "v0.1.4".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_642_477_360,
                    },
                ])
            });
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );

        let result: Result<Evaluation, Box<dyn Error>> = number_of_releases_policy.check(&GitHub {
            organization: "some_org".to_string(),
            name: "some_repo".to_string(),
        });

        result.should(be_ok(equal(Evaluation::Pass)));
    }

    #[test]
    fn when_there_are_less_than_2_releases_in_last_6_months_it_should_pass_the_policy_evaluation() {
        let mut retriever = Box::new(MockCommitRetriever::new());
        retriever
            .expect_all_tags()
            .with(eq("https://github.com/some_org/some_repo"))
            .returning(|_| {
                Ok(vec![Tag {
                    name: "v0.1.2".to_string(),
                    commit_id: "234234231".to_string(),
                    commit_timestamp: 1_640_477_360,
                }])
            });
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );

        let result: Result<Evaluation, Box<dyn Error>> = number_of_releases_policy.check(&GitHub {
            organization: "some_org".to_string(),
            name: "some_repo".to_string(),
        });

        result.should(be_ok(equal(Evaluation::Fail)));
    }

    #[test]
    fn when_the_releases_are_too_old_it_should_pass_the_policy_evaluation() {
        let mut retriever = Box::new(MockCommitRetriever::new());
        retriever
            .expect_all_tags()
            .with(eq("https://github.com/some_org/some_repo"))
            .returning(|_| {
                Ok(vec![
                    Tag {
                        name: "v0.1.2".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_440_477_360,
                    },
                    Tag {
                        name: "v0.1.3".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_441_477_360,
                    },
                    Tag {
                        name: "v0.1.4".to_string(),
                        commit_id: "234234231".to_string(),
                        commit_timestamp: 1_442_477_360,
                    },
                ])
            });
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );

        let result: Result<Evaluation, Box<dyn Error>> = number_of_releases_policy.check(&GitHub {
            organization: "some_org".to_string(),
            name: "some_repo".to_string(),
        });

        result.should(be_ok(equal(Evaluation::Fail)));
    }
}
