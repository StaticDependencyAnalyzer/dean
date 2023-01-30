use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use async_trait::async_trait;

use super::{Clock, CommitRetriever, Evaluation};
use crate::pkg::policy::Policy;
use crate::Dependency;

pub struct MinNumberOfReleasesRequired {
    retriever: Arc<dyn CommitRetriever>,
    number_of_releases: usize,
    duration: Duration,
    clock: Box<dyn Clock>,
}

#[async_trait]
impl Policy for MinNumberOfReleasesRequired {
    async fn evaluate(&self, dependency: &Dependency) -> Result<Evaluation, anyhow::Error> {
        let repository_url = dependency
            .repository
            .url()
            .context("the repository did not contain a URL")?;
        let all_tags = self
            .retriever
            .all_tags(&repository_url)
            .await
            .map_err(|e| anyhow!("error retrieving all tags: {}", e))?;

        let now = self.clock.now_timestamp();
        let num_tags_in_range = all_tags
            .iter()
            .rev()
            .take(self.number_of_releases)
            .filter(|&tag| tag.commit_timestamp >= now - self.duration.as_secs())
            .count();

        if num_tags_in_range == self.number_of_releases {
            Ok(Evaluation::Pass {
                policy_name: "min_number_of_releases_required".to_string(),
                dependency: dependency.clone(),
            })
        } else {
            #[allow(clippy::cast_precision_loss)]
            let fail_score = (self.number_of_releases as f64 - num_tags_in_range as f64)
                / self.number_of_releases as f64;
            Ok(Evaluation::Fail {
                policy_name: "min_number_of_releases_required".to_string(),
                dependency: dependency.clone(),
                reason: format!(
                    "expected {} releases in the last {} days, but found {}",
                    self.number_of_releases,
                    self.duration.as_secs() / (24 * 60 * 60),
                    num_tags_in_range
                ),
                fail_score,
            })
        }
    }
}

impl MinNumberOfReleasesRequired {
    pub fn new<R>(
        retriever: R,
        number_of_releases: usize,
        duration: Duration,
        clock: Box<dyn Clock>,
    ) -> Self
    where
        R: Into<Arc<dyn CommitRetriever>>,
    {
        Self {
            retriever: retriever.into(),
            number_of_releases,
            duration,
            clock,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mockall::predicate::eq;

    use super::super::{MockClock, MockCommitRetriever, Tag};
    use super::*;
    use crate::pkg::policy::Evaluation;
    use crate::pkg::Repository::GitHub;
    use crate::Dependency;

    #[tokio::test]
    async fn when_there_are_more_than_2_releases_in_last_6_months_it_should_pass_the_policy_evaluation(
    ) {
        let retriever = {
            let mut retriever = MockCommitRetriever::new();
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
            Box::new(retriever) as Box<dyn CommitRetriever>
        };
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );

        let dependency = Dependency {
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            },
            ..Dependency::default()
        };
        let result: Result<Evaluation, anyhow::Error> =
            number_of_releases_policy.evaluate(&dependency).await;

        assert_eq!(
            result.unwrap(),
            Evaluation::Pass {
                policy_name: "min_number_of_releases_required".to_string(),
                dependency
            }
        );
    }

    #[tokio::test]
    async fn when_there_are_less_than_2_releases_in_last_6_months_it_should_pass_the_policy_evaluation(
    ) {
        let retriever = {
            let mut retriever = MockCommitRetriever::new();
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
            Box::new(retriever) as Box<dyn CommitRetriever>
        };
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );
        let dependency = Dependency {
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            },
            ..Dependency::default()
        };
        let result: Result<Evaluation, anyhow::Error> =
            number_of_releases_policy.evaluate(&dependency).await;

        assert_eq!(
            result.unwrap(),
            Evaluation::Fail {
                policy_name: "min_number_of_releases_required".to_string(),
                dependency,
                reason: "expected 2 releases in the last 1260 days, but found 1".to_string(),
                fail_score: 0.5,
            }
        );
    }

    #[tokio::test]
    async fn when_the_releases_are_too_old_it_should_pass_the_policy_evaluation() {
        let retriever = {
            let mut retriever = MockCommitRetriever::new();
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
            Box::new(retriever) as Box<dyn CommitRetriever>
        };
        let mut clock = Box::new(MockClock::new());
        clock.expect_now_timestamp().return_const(1_648_583_009_u64);

        let months_in_seconds = 30 * 7 * 24 * 60 * 60;
        let number_of_releases_policy = MinNumberOfReleasesRequired::new(
            retriever,
            2,
            Duration::from_secs(6 * months_in_seconds),
            clock,
        );

        let dependency = Dependency {
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            },
            ..Dependency::default()
        };
        let result: Result<Evaluation, anyhow::Error> =
            number_of_releases_policy.evaluate(&dependency).await;

        assert_eq!(
            result.unwrap(),
            Evaluation::Fail {
                policy_name: "min_number_of_releases_required".to_string(),
                dependency,
                reason: "expected 2 releases in the last 1260 days, but found 0".to_string(),
                fail_score: 1.0,
            }
        );
    }
}
