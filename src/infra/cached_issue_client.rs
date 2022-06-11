use std::error::Error;
use std::sync::Arc;

use log::error;
use serde_json::Value;

#[cfg_attr(test, mockall::automock)]
pub trait IssueClient: Send + Sync {
    fn get_issues(&self, organization: &str, repo: &str) -> Box<dyn Iterator<Item = Value>>;
    fn get_pull_requests(&self, organization: &str, repo: &str) -> Box<dyn Iterator<Item = Value>>;
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

pub struct CachedClient {
    provider: String,
    inner: Arc<dyn IssueClient>,
    store: Arc<dyn IssueStore>,
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
        }
    }

    pub fn get_issues(&self, organization: &str, repo: &str) -> Box<dyn Iterator<Item = Value>> {
        if let Some(issues) = self.store.get_issues(&self.provider, organization, repo) {
            return Box::new(issues.into_iter());
        }

        let issues = self.inner.get_issues(organization, repo);
        let issue_vec: Vec<Value> = issues.collect();
        if let Err(err) = self
            .store
            .save_issues(&self.provider, organization, repo, &issue_vec)
        {
            error!("error saving issues: {:?}", err);
        }

        Box::new(issue_vec.into_iter())
    }

    pub fn get_pull_requests(
        &self,
        organization: &str,
        repo: &str,
    ) -> Box<dyn Iterator<Item = Value>> {
        if let Some(pull_requests) =
            self.store
                .get_pull_requests(&self.provider, organization, repo)
        {
            return Box::new(pull_requests.into_iter());
        }

        let pull_requests = self.inner.get_pull_requests(organization, repo);
        let pull_request_vec: Vec<Value> = pull_requests.collect();
        if let Err(err) =
            self.store
                .save_pull_requests(&self.provider, organization, repo, &pull_request_vec)
        {
            error!("error saving pull requests: {:?}", err);
        }

        Box::new(pull_request_vec.into_iter())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn it_retrieves_issues_if_not_present_in_store_exactly_once() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store.expect_get_issues().return_const(None).once();
            issue_store
                .expect_save_issues()
                .once()
                .return_once(|_, _, _, _| Ok(()));
            issue_store
                .expect_get_issues()
                .return_const(Some(issues_in_repo()))
                .once();
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = {
            let mut issue_client = Box::new(MockIssueClient::new());
            issue_client
                .expect_get_issues()
                .return_once(|_, _| Box::new(issues_in_repo().into_iter()))
                .once();
            issue_client
        };

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_issues = cached_client.get_issues("some_org", "some_repo");
        let second_call_issues = cached_client.get_issues("some_org", "some_repo");

        assert!(first_call_issues.eq(issues_in_repo()));
        assert!(second_call_issues.eq(issues_in_repo()));
    }

    #[test]
    fn if_the_issues_are_already_present_in_the_store_it_retrieves_them_from_the_store() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store
                .expect_get_issues()
                .return_const(Some(issues_in_repo()))
                .times(2);
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = Box::new(MockIssueClient::new());

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_issues = cached_client.get_issues("some_org", "some_repo");
        let second_call_issues = cached_client.get_issues("some_org", "some_repo");

        assert!(first_call_issues.eq(issues_in_repo()));
        assert!(second_call_issues.eq(issues_in_repo()));
    }

    #[test]
    fn it_retrieves_pull_requests_if_not_present_in_store_exactly_once() {
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
                .expect_get_pull_requests()
                .return_const(Some(pull_requests_in_repo()))
                .once();
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = {
            let mut issue_client = Box::new(MockIssueClient::new());
            issue_client
                .expect_get_pull_requests()
                .return_once(|_, _| Box::new(pull_requests_in_repo().into_iter()))
                .once();
            issue_client
        };

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_pull_requests = cached_client.get_pull_requests("some_org", "some_repo");
        let second_call_pull_requests = cached_client.get_pull_requests("some_org", "some_repo");

        assert!(first_call_pull_requests.eq(pull_requests_in_repo()));
        assert!(second_call_pull_requests.eq(pull_requests_in_repo()));
    }

    #[test]
    fn if_the_pull_requests_are_already_present_in_the_store_it_retrieves_them_from_the_store() {
        let issue_store: Box<dyn IssueStore> = {
            let mut issue_store = Box::new(MockIssueStore::new());
            issue_store
                .expect_get_pull_requests()
                .return_const(Some(pull_requests_in_repo()))
                .times(2);
            issue_store
        };
        let issue_client: Box<dyn IssueClient> = Box::new(MockIssueClient::new());

        let cached_client = CachedClient::new("github", issue_client, issue_store);

        let first_call_pull_requests = cached_client.get_pull_requests("some_org", "some_repo");
        let second_call_pull_requests = cached_client.get_pull_requests("some_org", "some_repo");

        assert!(first_call_pull_requests.eq(pull_requests_in_repo()));
        assert!(second_call_pull_requests.eq(pull_requests_in_repo()));
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
