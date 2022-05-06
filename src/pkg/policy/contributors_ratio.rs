use crate::pkg::policy::{CommitRetriever, Evaluation, Policy};

use crate::Dependency;
use anyhow::Context;
use itertools::Itertools;
use std::collections::HashSet;
use std::error::Error;
use std::sync::{Arc, RwLock};

pub struct ContributorsRatio {
    retriever: Arc<RwLock<dyn CommitRetriever>>,
    max_number_of_releases_to_check: usize,
    max_contributor_ratio: f64,
}

impl Policy for ContributorsRatio {
    #[allow(clippy::cast_precision_loss)]
    fn evaluate(&self, dependency: &Dependency) -> Result<Evaluation, Box<dyn Error>> {
        let repo_url = dependency
            .repository
            .url()
            .context("the repository doesn't have a URL")?;

        let all_tags = self
            .retriever
            .read()
            .map_err(|_| "failed to retrieve read lock")?
            .all_tags(&repo_url)
            .map_err(|e| format!("unable to retrieve all tags for repo {}: {}", &repo_url, e))?
            .into_iter();
        let tags_to_check = all_tags.rev().take(self.max_number_of_releases_to_check);
        let tag_names = tags_to_check.map(|tag| tag.name).collect::<HashSet<_>>();

        let all_commits_for_each_tag = self
            .retriever
            .read()
            .map_err(|_| "failed to retrieve read lock")?
            .commits_for_each_tag(&repo_url)
            .map_err(|e| {
                format!(
                    "unable to retrieve commits for each tag for repo {}: {}",
                    &repo_url, e
                )
            })?;

        let commits_to_check = all_commits_for_each_tag
            .into_iter()
            .filter(|(key, _)| tag_names.contains(key))
            .flat_map(|(_, value)| value)
            .unique_by(|commit| commit.id.clone());

        let authors_in_all_releases = commits_to_check.map(|commit| commit.author_email);

        let number_of_different_authors = authors_in_all_releases
            .dedup_with_count()
            .collect::<Vec<_>>();

        let all_authors_count: usize = number_of_different_authors
            .iter()
            .map(|(count, _)| count)
            .sum();

        let authors_with_rate = number_of_different_authors
            .into_iter()
            .map(|(count, author)| (count as f64 / all_authors_count as f64, author));

        for (rate, author) in authors_with_rate {
            if rate > self.max_contributor_ratio {
                return Ok(Evaluation::Fail(format!(
                    "the rate of contribution is too high ({} > {}) for author {}",
                    rate, self.max_contributor_ratio, author
                )));
            }
        }
        Ok(Evaluation::Pass)
    }
}

impl ContributorsRatio {
    pub fn new(
        retriever: Arc<RwLock<dyn CommitRetriever>>,
        max_number_of_releases_to_check: usize,
        max_contributor_ratio: f64,
    ) -> Self {
        Self {
            retriever,
            max_number_of_releases_to_check,
            max_contributor_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Commit, MockCommitRetriever, Tag};
    use super::*;
    use crate::pkg::Repository::GitHub;
    use expects::matcher::{be_ok, equal};
    use expects::Subject;
    use mockall::predicate::eq;
    use std::collections::HashMap;

    #[test]
    fn if_the_contributor_ratio_for_the_latest_release_is_lower_than_90_percent_it_should_pass() {
        let retriever = Arc::new(RwLock::new(MockCommitRetriever::new()));
        retriever
            .write()
            .expect("unable to retrieve write lock")
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
        retriever
            .write()
            .expect("unable to retrieve write lock")
            .expect_commits_for_each_tag()
            .returning(|_| {
                Ok({
                    let mut map = HashMap::new();
                    map.insert(
                        "v0.1.4".to_string(),
                        vec![
                            Commit {
                                id: "2134324".to_string(),
                                author_name: "SomeName".to_string(),
                                author_email: "SomeAuthor".to_string(),
                                creation_timestamp: 0,
                            },
                            Commit {
                                id: "324213432".to_string(),
                                author_name: "SomeOtherName".to_string(),
                                author_email: "SomeOtherAuthor".to_string(),
                                creation_timestamp: 0,
                            },
                        ],
                    );
                    map
                })
            });
        let contributors_ratio_policy = ContributorsRatio::new(retriever, 1, 0.9);

        let mut dependency = Dependency::default();
        dependency.repository = GitHub {
            organization: "some_org".to_string(),
            name: "some_repo".to_string(),
        };
        let result = contributors_ratio_policy.evaluate(&dependency);

        result.should(be_ok(equal(Evaluation::Pass)));
    }
    #[test]
    fn if_the_contributor_ratio_for_the_latest_release_is_higher_than_90_percent_it_should_fail() {
        let retriever = Arc::new(RwLock::new(MockCommitRetriever::new()));
        retriever
            .write()
            .expect("unable to retrieve write lock")
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
        retriever
            .write()
            .expect("unable to retrieve write lock")
            .expect_commits_for_each_tag()
            .returning(|_| {
                Ok({
                    let mut map = HashMap::new();
                    map.insert(
                        "v0.1.4".to_string(),
                        vec![Commit {
                            id: "2134324".to_string(),
                            author_name: "SomeName".to_string(),
                            author_email: "SomeAuthor".to_string(),
                            creation_timestamp: 0,
                        }],
                    );
                    map
                })
            });
        let contributors_ratio_policy = ContributorsRatio::new(retriever, 1, 0.9);

        let mut dependency = Dependency::default();
        dependency.repository = GitHub {
            organization: "some_org".to_string(),
            name: "some_repo".to_string(),
        };
        let result = contributors_ratio_policy.evaluate(&dependency);

        result.should(be_ok(equal(Evaluation::Fail(
            "the rate of contribution is too high (1 > 0.9) for author SomeAuthor".to_owned(),
        ))));
    }
}
