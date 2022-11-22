use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use log::error;
use moka::future::{Cache, CacheBuilder};
use serde_json::Value;
use tokio::pin;
use tokio_stream::{Stream, StreamExt};

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait IssueClient: Send + Sync {
    async fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
        last_issues: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send>;
    async fn get_last_pull_requests(
        &self,
        organization: &str,
        repo: &str,
        last_pull_requests: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send>;
}

#[cfg_attr(test, mockall::automock)]
pub trait IssueStore: Send + Sync {
    fn get_issues(&self, provider: &str, organization: &str, repo: &str) -> Option<Vec<Value>>;
    fn save_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        issues: &[Value],
    ) -> Result<(), Box<dyn Error>>;
    fn get_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>>;
    fn save_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        pull_requests: &[Value],
    ) -> Result<(), Box<dyn Error>>;
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct CacheKey {
    organization: String,
    repo: String,
}

pub struct CachedClient {
    provider: String,
    inner: Arc<dyn IssueClient>,
    store: Arc<dyn IssueStore>,
    issue_cache: Cache<CacheKey, Vec<Value>>,
    pull_request_cache: Cache<CacheKey, Vec<Value>>,
}

impl CachedClient {
    pub fn new<G, C>(provider: &str, inner: G, store: C) -> Self
    where
        G: Into<Arc<dyn IssueClient>>,
        C: Into<Arc<dyn IssueStore>>,
    {
        Self {
            provider: provider.to_string(),
            inner: inner.into(),
            store: store.into(),
            issue_cache: CacheBuilder::default().build(),
            pull_request_cache: CacheBuilder::default().build(),
        }
    }

    pub async fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
        last_issues: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let key = CacheKey {
            organization: organization.to_string(),
            repo: repo.to_string(),
        };

        let issues = self.issue_cache.try_get_with(key, async {
            if let Some(issues) = self.store.get_issues(&self.provider, organization, repo) {
                return Ok(issues);
            }

            let issues = self
                .inner
                .get_last_issues(organization, repo, last_issues)
                .await;
            pin!(issues);
            let mut issue_vec: Vec<_> = Vec::new();
            while let Some(issue) = issues.next().await {
                issue_vec.push(issue);
            }

            match self
                .store
                .save_issues(&self.provider, organization, repo, &issue_vec)
            {
                Ok(_) => Ok(issue_vec),
                Err(inner) => Err(inner.to_string()),
            }
        });

        match issues.await {
            Ok(issues) => Box::new(tokio_stream::iter(issues)),
            Err(err) => {
                error!(
                    "failed to get issues for {}/{}: {}",
                    organization, repo, err
                );
                Box::new(tokio_stream::empty())
            }
        }
    }

    pub async fn get_pull_requests(
        &self,
        organization: &str,
        repo: &str,
        last_pull_requests: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let key = CacheKey {
            organization: organization.to_string(),
            repo: repo.to_string(),
        };

        let pull_requests = self.pull_request_cache.try_get_with(key, async {
            if let Some(pull_requests) =
                self.store
                    .get_pull_requests(&self.provider, organization, repo)
            {
                return Ok(pull_requests);
            }

            let mut pull_requests = self
                .inner
                .get_last_pull_requests(organization, repo, last_pull_requests)
                .await;
            let mut pull_request_vec = Vec::new();
            while let Some(pull_request) = pull_requests.next().await {
                pull_request_vec.push(pull_request);
            }

            match self.store.save_pull_requests(
                &self.provider,
                organization,
                repo,
                &pull_request_vec,
            ) {
                Ok(_) => Ok(pull_request_vec),
                Err(inner) => Err(inner.to_string()),
            }
        });

        match pull_requests.await {
            Ok(pull_requests) => Box::new(tokio_stream::iter(pull_requests)),
            Err(err) => {
                error!(
                    "failed to get pull requests for {}/{}: {}",
                    organization, repo, err
                );
                Box::new(tokio_stream::empty())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn it_retrieves_issues_if_not_present_in_store_exactly_once() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store.expect_get_issues().return_const(None).once();
            issue_store
                .expect_save_issues()
                .once()
                .return_once(|_, _, _, _| Ok(()));
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = {
            let mut issue_client = Box::new(MockIssueClient::new());
            issue_client
                .expect_get_last_issues()
                .return_once(|_, _, _| Box::new(tokio_stream::iter(issues_in_repo())))
                .once();
            issue_client
        };

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_issues = cached_client
            .get_last_issues("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;
        let second_call_issues = cached_client
            .get_last_issues("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;

        assert!(first_call_issues.eq(&issues_in_repo()));
        assert!(second_call_issues.eq(&issues_in_repo()));
    }

    #[tokio::test]
    async fn if_the_issues_are_already_present_in_the_store_it_retrieves_them_from_the_store() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store
                .expect_get_issues()
                .return_const(Some(issues_in_repo()))
                .times(1);
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = Box::new(MockIssueClient::new());

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_issues = cached_client
            .get_last_issues("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;
        let second_call_issues = cached_client
            .get_last_issues("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;

        assert!(first_call_issues.eq(&issues_in_repo()));
        assert!(second_call_issues.eq(&issues_in_repo()));
    }

    #[tokio::test]
    async fn it_retrieves_pull_requests_if_not_present_in_store_exactly_once() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store
                .expect_get_pull_requests()
                .return_const(None)
                .once();
            issue_store
                .expect_save_pull_requests()
                .once()
                .return_once(|_, _, _, _| Ok(()));
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = {
            let mut issue_client = Box::new(MockIssueClient::new());
            issue_client
                .expect_get_last_pull_requests()
                .return_once(|_, _, _| Box::new(tokio_stream::iter(pull_requests_in_repo())))
                .once();
            issue_client
        };

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_pull_requests = cached_client
            .get_pull_requests("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;
        let second_call_pull_requests = cached_client
            .get_pull_requests("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;

        println!("{:?}", first_call_pull_requests);
        println!("{:?}", second_call_pull_requests);
        println!("{:?}", &pull_requests_in_repo());
        assert!(first_call_pull_requests.eq(&pull_requests_in_repo()));
        assert!(second_call_pull_requests.eq(&pull_requests_in_repo()));
    }

    #[tokio::test]
    async fn if_the_pull_requests_are_already_present_in_the_store_it_retrieves_them_from_the_store(
    ) {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store
                .expect_get_pull_requests()
                .return_const(Some(pull_requests_in_repo()))
                .times(1);
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = Box::new(MockIssueClient::new());

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_pull_requests = cached_client
            .get_pull_requests("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;
        let second_call_pull_requests = cached_client
            .get_pull_requests("some_org", "some_repo", 10)
            .await
            .collect::<Vec<_>>()
            .await;

        assert!(first_call_pull_requests.eq(&pull_requests_in_repo()));
        assert!(second_call_pull_requests.eq(&pull_requests_in_repo()));
    }

    fn pull_requests_in_repo() -> Vec<Value> {
        vec![
            Value::String("pull_request_1".to_string()),
            Value::String("pull_request_2".to_string()),
        ]
    }

    fn issues_in_repo() -> Vec<Value> {
        vec![
            Value::String("issue1".into()),
            Value::String("issue2".into()),
        ]
    }
}
