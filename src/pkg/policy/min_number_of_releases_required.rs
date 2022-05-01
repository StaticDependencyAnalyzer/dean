use super::{Clock, CommitRetriever, Evaluation, Repository};
use crate::pkg::policy::Policy;
use anyhow::Context;
use std::error::Error;
use std::time::Duration;

pub struct MinNumberOfReleasesRequired {
    retriever: Box<dyn CommitRetriever>,
    number_of_releases: usize,
    duration: Duration,
    clock: Box<dyn Clock>,
}

impl Policy for MinNumberOfReleasesRequired {
    fn evaluate(&self, repository: &Repository) -> Result<Evaluation, Box<dyn Error>> {
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

#[cfg(test)]
mod tests {
    use super::super::{MockClock, MockCommitRetriever, Tag};
    use super::*;
    use crate::pkg::policy::Evaluation;
    use expects::matcher::{be_ok, equal};
    use expects::Subject;
    use mockall::predicate::eq;

    use crate::pkg::Repository::GitHub;
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

        let result: Result<Evaluation, Box<dyn Error>> =
            number_of_releases_policy.evaluate(&GitHub {
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

        let result: Result<Evaluation, Box<dyn Error>> =
            number_of_releases_policy.evaluate(&GitHub {
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

        let result: Result<Evaluation, Box<dyn Error>> =
            number_of_releases_policy.evaluate(&GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            });

        result.should(be_ok(equal(Evaluation::Fail)));
    }
}
