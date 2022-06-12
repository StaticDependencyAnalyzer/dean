use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;

use anyhow::Context;
use itertools::Itertools;

use crate::pkg::policy::{CommitRetriever, Evaluation, Policy};
use crate::Dependency;

pub struct ContributorsRatio {
    retriever: Arc<dyn CommitRetriever>,
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
            .all_tags(&repo_url)
            .map_err(|e| format!("unable to retrieve all tags for repo {}: {}", &repo_url, e))?
            .into_iter();
        let tags_to_check = all_tags.rev().take(self.max_number_of_releases_to_check);
        let tag_names = tags_to_check.map(|tag| tag.name).collect::<HashSet<_>>();

        let all_commits_for_each_tag =
            self.retriever
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
            if rate > self.max_contributor_ratio && self.max_contributor_ratio > 0.0 {
                let fail_score = rate / self.max_contributor_ratio;
                return Ok(Evaluation::Fail(
                    "contributors_ratio".to_string(),
                    dependency.clone(),
                    format!(
                        "the rate of contribution is too high ({} > {}) for author {}",
                        rate, self.max_contributor_ratio, author
                    ),
                    fail_score,
                ));
            }
        }
        Ok(Evaluation::Pass(
            "contributors_ratio".to_string(),
            dependency.clone(),
        ))
    }
}

impl ContributorsRatio {
    pub fn new<R: Into<Arc<dyn CommitRetriever>>>(
        retriever: R,
        max_number_of_releases_to_check: usize,
        max_contributor_ratio: f64,
    ) -> Self {
        Self {
            retriever: retriever.into(),
            max_number_of_releases_to_check,
            max_contributor_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use expects::matcher::{be_ok, equal};
    use expects::Subject;
    use mockall::predicate::eq;

    use super::super::{Commit, MockCommitRetriever, Tag};
    use super::*;
    use crate::pkg::Repository::GitHub;

    #[test]
    fn if_the_contributor_ratio_for_the_latest_release_is_lower_than_90_percent_it_should_pass() {
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
            retriever.expect_commits_for_each_tag().returning(|_| {
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
            Box::new(retriever) as Box<dyn CommitRetriever>
        };
        let contributors_ratio_policy = ContributorsRatio::new(retriever, 1, 0.9);

        let dependency = Dependency {
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            },
            ..Dependency::default()
        };
        let result = contributors_ratio_policy.evaluate(&dependency);

        result.should(be_ok(equal(Evaluation::Pass(
            "contributors_ratio".to_string(),
            dependency,
        ))));
    }
    #[test]
    fn if_the_contributor_ratio_for_the_latest_release_is_higher_than_90_percent_it_should_fail() {
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
            retriever.expect_commits_for_each_tag().returning(|_| {
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
            Box::new(retriever) as Box<dyn CommitRetriever>
        };
        let contributors_ratio_policy = ContributorsRatio::new(retriever, 1, 0.9);

        let dependency = Dependency {
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_repo".to_string(),
            },
            ..Dependency::default()
        };

        let result = contributors_ratio_policy.evaluate(&dependency);

        match result.unwrap() {
            Evaluation::Fail(policy, dep, reason, score) => {
                policy.should(equal("contributors_ratio".to_string()));
                dep.should(equal(dependency));
                reason.should(equal(
                    "the rate of contribution is too high (1 > 0.9) for author SomeAuthor"
                        .to_owned(),
                ));
                println!("{}", score);
                assert!((score - 1.111_111_111_111_111_2).abs() < f64::EPSILON);
            }
            Evaluation::Pass(_, _) => {
                unreachable!()
            }
        }
    }
}
