use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio_stream::StreamExt;

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

    async fn get_github_issue_lifespan(
        &self,
        organization: &str,
        repo: &str,
        last_issues: usize,
    ) -> f64 {
        let mut issues = self
            .github_cached_client
            .get_last_issues(organization, repo, last_issues)
            .await;

        let mut closed_issues = Vec::new();
        while let Some(issue) = issues.next().await {
            if issue.get("state").unwrap().as_str().unwrap() == "closed" {
                closed_issues.push(issue);
            }
        }

        let lifespan_per_issue = closed_issues.into_iter().map(|issue| {
            let created_at_str = issue.get("created_at").unwrap().as_str().unwrap();
            let closed_at_str = issue.get("closed_at").unwrap().as_str().unwrap();

            let created_at = OffsetDateTime::parse(created_at_str, &Rfc3339).unwrap();
            let closed_at = OffsetDateTime::parse(closed_at_str, &Rfc3339).unwrap();
            let lifespan = closed_at - created_at;

            lifespan.as_seconds_f64()
        });

        lifespan_per_issue.mean()
    }

    async fn get_github_pull_request_lifespan(
        &self,
        organization: &str,
        repo: &str,
        last_pull_requests: usize,
    ) -> f64 {
        let mut prs = self
            .github_cached_client
            .get_pull_requests(organization, repo, last_pull_requests)
            .await;

        let mut closed_prs = Vec::new();
        while let Some(pr) = prs.next().await {
            if pr.get("state").unwrap().as_str().unwrap() == "closed" {
                closed_prs.push(pr);
            }
        }

        let lifespan_per_pr = closed_prs.into_iter().map(|pr| {
            let created_at_str = pr.get("created_at").unwrap().as_str().unwrap();
            let closed_at_str = pr.get("closed_at").unwrap().as_str().unwrap();

            let created_at = OffsetDateTime::parse(created_at_str, &Rfc3339).unwrap();
            let closed_at = OffsetDateTime::parse(closed_at_str, &Rfc3339).unwrap();
            let lifespan = closed_at - created_at;

            lifespan.as_seconds_f64()
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

#[async_trait]
impl ContributionDataRetriever for Retriever {
    async fn get_issue_lifespan(
        &self,
        repository: &Repository,
        last_issues: usize,
    ) -> Result<f64, Box<dyn Error>> {
        match repository {
            Repository::Unknown => Err("unknown repository".into()),
            Repository::GitHub { name, organization } => Ok(self
                .get_github_issue_lifespan(organization, name, last_issues)
                .await),
            Repository::GitLab { .. } | Repository::Raw { .. } => Err("not implemented".into()),
        }
    }

    async fn get_pull_request_lifespan(
        &self,
        repository: &Repository,
        last_pull_requests: usize,
    ) -> Result<f64, Box<dyn Error>> {
        match repository {
            Repository::Unknown => Err("unknown repository".into()),
            Repository::GitHub { name, organization } => Ok(self
                .get_github_pull_request_lifespan(organization, name, last_pull_requests)
                .await),
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

    #[tokio::test]
    async fn it_retrieves_the_issue_lifespan_of_dean() {
        let http_client = reqwest::Client::default();
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
            .await
            .unwrap();

        let two_months_in_seconds = 1.0 * 30.0 * 24.0 * 60.0 * 60.0;
        let three_months_in_seconds = 3.0 * 30.0 * 24.0 * 60.0 * 60.0;
        assert!(issue_lifespan > two_months_in_seconds);
        assert!(issue_lifespan < three_months_in_seconds);
    }

    #[tokio::test]
    async fn it_retrieves_the_pull_request_lifespan_of_dean() {
        let http_client = reqwest::Client::default();
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
            .await
            .unwrap();

        let two_minutes_in_seconds = 2.0 * 60.0;
        let three_days_in_seconds = 3.0 * 24.0 * 60.0 * 60.0;
        assert!(pr_lifespan > two_minutes_in_seconds);
        assert!(pr_lifespan < three_days_in_seconds);
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
