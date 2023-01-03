#![allow(unused)]

use std::collections::BTreeMap;
use std::error::Error;

use async_trait::async_trait;
use futures::executor::block_on;
use futures::future::ok;
use futures::{StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use surrealdb::engine::any::Any;
use surrealdb::{sql, Surreal};

use crate::infra::cached_issue_client::IssueStore;

struct SurrealDB {
    client: Surreal<Any>,
}

impl SurrealDB {
    fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }
}

#[derive(Serialize, Deserialize)]
struct Issue {
    provider: String,
    organization: String,
    repository: String,
    issues: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
struct PullRequest {
    provider: String,
    organization: String,
    repository: String,
    pull_requests: Vec<Value>,
}

#[async_trait]
impl IssueStore for SurrealDB {
    async fn get_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>> {
        let mut response = self.client.query("SELECT * FROM issue WHERE provider = $provider AND organization = $organization AND repository = $repository")
            .bind(("provider", provider))
            .bind(("organization", organization))
            .bind(("repository", repo))
            .await
            .ok()?;

        let issues: Vec<Issue> = response
            .take(0)
            .expect("failed to take response from query, this should never happen");

        let issues: Vec<Value> = issues.into_iter().map(|i| i.issues).flatten().collect();

        if issues.is_empty() {
            None
        } else {
            Some(issues)
        }
    }

    async fn save_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        issues: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let _: Issue = self
            .client
            .create("issue")
            .content(Issue {
                provider: provider.to_string(),
                organization: organization.to_string(),
                repository: repo.to_string(),
                issues: issues.to_vec(),
            })
            .await?;

        Ok(())
    }

    async fn get_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>> {
        let mut response = self.client.query("SELECT * FROM pull_request WHERE provider = $provider AND organization = $organization AND repository = $repository")
            .bind(("provider", provider))
            .bind(("organization", organization))
            .bind(("repository", repo))
            .await
            .ok()?;

        let pull_requests: Vec<PullRequest> = response
            .take(0)
            .expect("failed to take response from query, this should never happen");

        let pull_requests: Vec<Value> = pull_requests
            .into_iter()
            .map(|i| i.pull_requests)
            .flatten()
            .collect();

        if pull_requests.is_empty() {
            None
        } else {
            Some(pull_requests)
        }
    }

    async fn save_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        pull_requests: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let _: PullRequest = self
            .client
            .create("pull_request")
            .content(PullRequest {
                provider: provider.to_string(),
                organization: organization.to_string(),
                repository: repo.to_string(),
                pull_requests: pull_requests.to_vec(),
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use surrealdb::engine::any::{connect, Any};
    use surrealdb::opt::auth::{Database, Namespace, Root, Scope};
    use surrealdb::Surreal;

    use super::*;

    #[tokio::test]
    async fn it_stores_and_retrieves_the_issues() {
        let client = actual_surreal_client().await;
        let issue_store = SurrealDB::new(client);

        issue_store
            .save_issues("github", "rust-lang", "rust", &issues_in_repo())
            .await
            .expect("Failed to save issues");
        let issues = issue_store
            .get_issues("github", "rust-lang", "rust")
            .await
            .expect("Failed to retrieve issues");

        assert_eq!(issues, issues_in_repo());
    }

    #[tokio::test]
    async fn it_stores_and_retrieves_the_pull_requests() {
        let client = actual_surreal_client().await;
        let issue_store = SurrealDB::new(client);

        issue_store
            .save_pull_requests("github", "rust-lang", "rust", &pull_requests_in_repo())
            .await
            .unwrap();
        let pull_requests = issue_store
            .get_pull_requests("github", "rust-lang", "rust")
            .await
            .unwrap();

        assert_eq!(pull_requests, pull_requests_in_repo());
    }

    #[tokio::test]
    async fn if_there_are_no_issues_it_returns_none() {
        let client = actual_surreal_client().await;
        let issue_store = SurrealDB::new(client);

        let issues = issue_store.get_issues("github", "unknown", "unknown").await;

        assert_eq!(issues, None);
    }

    #[tokio::test]
    async fn if_there_are_no_pull_requests_it_returns_none() {
        let client = actual_surreal_client().await;
        let issue_store = SurrealDB::new(client);

        let pull_requests = issue_store
            .get_pull_requests("github", "unknown", "unknown")
            .await;

        assert_eq!(pull_requests, None);
    }

    async fn actual_surreal_client() -> Surreal<Any> {
        let client = connect("mem://")
            .await
            .expect("unable to connect to surreal instance");
        client
            .use_ns("ns")
            .use_db("db")
            .await
            .expect("unable to specify ns and db");
        client
    }

    fn issues_in_repo() -> Vec<Value> {
        vec![
            Value::String("issue1".to_string()),
            Value::String("issue2".to_string()),
        ]
    }

    fn pull_requests_in_repo() -> Vec<Value> {
        vec![
            Value::String("pull_request1".to_string()),
            Value::String("pull_request2".to_string()),
        ]
    }
}
