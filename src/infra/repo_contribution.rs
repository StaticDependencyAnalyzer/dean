use std::error::Error;
use std::sync::Arc;

use crate::infra::cached_issue_client::{CachedClient, IssueClient, IssueStore};
use crate::infra::github;
use crate::pkg::policy::ContributionDataRetriever;
use crate::pkg::Repository;

pub struct Retriever {
    github_cached_client: Box<CachedClient>,
}

impl Retriever {
    pub fn new<C, S>(github_client: C, issue_store: S) -> Self
    where
        C: Into<Arc<github::Client>>,
        S: Into<Arc<dyn IssueStore>>,
    {
        let client = CachedClient::new(
            "github",
            github_client.into() as Arc<dyn IssueClient>,
            issue_store.into(),
        );
        Self {
            github_cached_client: Box::new(client),
        }
    }

    fn get_github_issue_lifespan(&self, organization: &str, repo: &str, last_issues: usize) -> f64 {
        let issues = self
            .github_cached_client
            .get_last_issues(organization, repo, last_issues);

        let closed_issues =
            issues.filter(|issue| issue.get("state").unwrap().as_str().unwrap() == "closed");

        let lifespan_per_issue = closed_issues.map(|issue| {
            let created_at_str = issue.get("created_at").unwrap().as_str().unwrap();
            let closed_at_str = issue.get("closed_at").unwrap().as_str().unwrap();

            let created_at = chrono::DateTime::parse_from_rfc3339(created_at_str).unwrap();
            let closed_at = chrono::DateTime::parse_from_rfc3339(closed_at_str).unwrap();

            let lifespan = closed_at.signed_duration_since(created_at);
            f64::from(i32::try_from(lifespan.num_seconds()).unwrap())
        });

        lifespan_per_issue.mean()
    }

    fn get_github_pull_request_lifespan(
        &self,
        organization: &str,
        repo: &str,
        last_pull_requests: usize,
    ) -> f64 {
        let prs =
            self.github_cached_client
                .get_pull_requests(organization, repo, last_pull_requests);

        let closed_prs = prs.filter(|pr| pr.get("state").unwrap().as_str().unwrap() == "closed");

        let lifespan_per_pr = closed_prs.map(|pr| {
            let created_at_str = pr.get("created_at").unwrap().as_str().unwrap();
            let closed_at_str = pr.get("closed_at").unwrap().as_str().unwrap();

            let created_at = chrono::DateTime::parse_from_rfc3339(created_at_str).unwrap();
            let closed_at = chrono::DateTime::parse_from_rfc3339(closed_at_str).unwrap();

            let lifespan = closed_at.signed_duration_since(created_at);
            f64::from(i32::try_from(lifespan.num_seconds()).unwrap())
        });

        lifespan_per_pr.mean()
    }
}

trait Mean {
    fn mean(self) -> f64;
}

impl<F, T> Mean for T
where
    T: Iterator<Item = F>,
    F: std::borrow::Borrow<f64>,
{
    fn mean(self) -> f64 {
        self.zip(1..).fold(0., |s, (e, i)| {
            (*e.borrow() + s * f64::from(i - 1)) / f64::from(i)
        })
    }
}

impl ContributionDataRetriever for Retriever {
    fn get_issue_lifespan(
        &self,
        repository: &Repository,
        last_issues: usize,
    ) -> Result<f64, Box<dyn Error>> {
        match repository {
            Repository::Unknown => Err("unknown repository".into()),
            Repository::GitHub { name, organization } => {
                Ok(self.get_github_issue_lifespan(organization, name, last_issues))
            }
            Repository::GitLab { .. } | Repository::Raw { .. } => Err("not implemented".into()),
        }
    }

    fn get_pull_request_lifespan(
        &self,
        repository: &Repository,
        last_pull_requests: usize,
    ) -> Result<f64, Box<dyn Error>> {
        match repository {
            Repository::Unknown => Err("unknown repository".into()),
            Repository::GitHub { name, organization } => {
                Ok(self.get_github_pull_request_lifespan(organization, name, last_pull_requests))
            }
            Repository::GitLab { .. } | Repository::Raw { .. } => Err("not implemented".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::cached_issue_client::MockIssueStore;
    use crate::infra::github::Authentication;
    use crate::pkg::Repository;

    #[test]
    fn it_retrieves_the_issue_lifespan_of_dean() {
        let http_client = reqwest::blocking::Client::default();
        let github_client = github::Client::new(http_client, authentication());
        let issue_store = mock_issue_store();
        let retriever = Retriever::new(github_client, issue_store);

        let issue_lifespan: f64 = retriever
            .get_issue_lifespan(
                &Repository::GitHub {
                    organization: "StaticDependencyAnalyzer".to_string(),
                    name: "dean".to_string(),
                },
                10,
            )
            .unwrap();

        let two_months_in_seconds = 1.0 * 30.0 * 24.0 * 60.0 * 60.0;
        let three_months_in_seconds = 3.0 * 30.0 * 24.0 * 60.0 * 60.0;
        assert!(issue_lifespan > two_months_in_seconds);
        assert!(issue_lifespan < three_months_in_seconds);
    }

    #[test]
    fn it_retrieves_the_pull_request_lifespan_of_dean() {
        let http_client = reqwest::blocking::Client::default();
        let github_client = github::Client::new(http_client, authentication());
        let issue_store = mock_issue_store();
        let retriever = Retriever::new(github_client, issue_store);

        let pr_lifespan: f64 = retriever
            .get_pull_request_lifespan(
                &Repository::GitHub {
                    organization: "StaticDependencyAnalyzer".to_string(),
                    name: "dean".to_string(),
                },
                10,
            )
            .unwrap();

        let two_minutes_in_seconds = 2.0 * 60.0;
        let two_hours_in_seconds = 2.0 * 60.0 * 60.0;
        assert!(pr_lifespan > two_minutes_in_seconds);
        assert!(pr_lifespan < two_hours_in_seconds);
    }

    fn mock_issue_store() -> Box<dyn IssueStore> {
        let mut issue_store = Box::new(MockIssueStore::new());
        issue_store.expect_get_issues().return_const(None);
        issue_store.expect_get_pull_requests().return_const(None);
        issue_store
            .expect_save_issues()
            .return_once(|_, _, _, _| Ok(()));
        issue_store
            .expect_save_pull_requests()
            .return_once(|_, _, _, _| Ok(()));
        issue_store
    }

    fn authentication() -> Authentication {
        let github_username = std::env::var("GITHUB_USERNAME").ok();
        let github_password = std::env::var("GITHUB_PASSWORD").ok();

        match github_username {
            None => Authentication::None,
            Some(github_username) => Authentication::Basic(github_username, github_password),
        }
    }
}
