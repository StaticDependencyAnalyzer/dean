use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use anyhow::Context;
use async_recursion::async_recursion;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, error, trace, warn};
use serde_json::Value;
use tokio_stream::Stream;

use crate::infra::cached_issue_client::IssueClient;

#[derive(Clone)]
pub enum Authentication {
    None,
    Basic(String, Option<String>),
}

pub struct Client {
    client: Arc<reqwest::Client>,
    auth: Authentication,
}

pub struct IssuePullRequestStream {
    client: Arc<reqwest::Client>,
    next_page: Option<String>,
    buffer: Vec<Value>,
    auth: Authentication,
}

impl IssuePullRequestStream {
    #[async_recursion]
    async fn update_buffer(
        &mut self,
        backoff: Option<(usize, Duration)>,
    ) -> Result<(), Box<dyn Error>> {
        if self.next_page.is_none() {
            return Ok(());
        }

        let url = self.next_page.as_ref().unwrap().clone();

        debug!(target: "dean::github_client", "Fetching issues from {}", url);
        let mut request = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/81.0.4044.138 Safari/537.36")
            .header("Accept", "application/vnd.github.v3+json");

        if let Authentication::Basic(user, passwd) = &self.auth {
            request = request.basic_auth(user, passwd.as_ref());
        }

        trace!(target: "dean::github_client", "Request: {:?}", request);
        let response = request.send().await.context("Failed to get issues")?;
        self.next_page.take();
        trace!(target: "dean::github_client", "Response: {:?}", response);

        if response.status().as_u16() == 403 {
            let total_allowed_attempts = 8;
            self.next_page = Some(url);
            // FIXME: Naive implementation of a retry strategy, change this to use the x-ratelimit-reset header in the response.
            return match backoff {
                None => {
                    let initial_duration = Duration::from_secs(15);
                    warn!(target: "dean::github_client", "Github API rate limit exceeded, remaining attempts [{}/{}], retrying in {} seconds", total_allowed_attempts, total_allowed_attempts, initial_duration.as_secs());
                    tokio::time::sleep(initial_duration).await;
                    self.update_buffer(Some((total_allowed_attempts - 1, initial_duration * 2)))
                        .await
                }
                Some((remaining_attempts, duration)) => {
                    if remaining_attempts == 0 {
                        return Err("Github API rate limit exceeded, stopping iteration".into());
                    }

                    warn!(target: "dean::github_client", "Github API rate limit exceeded, remaining attempts [{}/{}], retrying in {} seconds", remaining_attempts, total_allowed_attempts, duration.as_secs());
                    tokio::time::sleep(duration).await;
                    self.update_buffer(Some((remaining_attempts - 1, duration * 2)))
                        .await
                }
            };
        }

        if let Some(link) = response.headers().get("link") {
            let link = link.to_str().unwrap_or("");
            let link = link
                .split(',')
                .find(|link| link.contains("rel=\"next\""))
                .unwrap_or("");
            let link = link.split(';').next().unwrap_or("");
            let link = link.trim().trim_start_matches('<').trim_end_matches('>');
            if !link.is_empty() {
                self.next_page = Some(link.to_string());
            }
        };

        let response_json = response
            .json::<Value>()
            .await
            .context("Failed to parse issues")?;

        let issues = response_json
            .as_array()
            .context("the response is not an array")?
            .clone();

        self.buffer.extend_from_slice(&issues);

        Ok(())
    }
}

impl Stream for IssuePullRequestStream {
    type Item = Value;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        trace!(target: "dean::github_client", "Checking if there are issues in the buffer");
        if self.buffer.is_empty() {
            debug!(target: "dean::github_client", "Buffer is empty, updating it");

            match self.as_mut().update_buffer(None).as_mut().poll(cx) {
                Poll::Ready(Err(e)) => {
                    error!(target: "dean::github_client", "Failed to update buffer: {}", e);
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    debug!(target: "dean::github_client", "Buffer update is pending");
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Poll::Ready(Ok(())) => {
                    debug!(target: "dean::github_client", "Buffer update is ready");
                }
            };
        }

        debug!(target: "dean::github_client", "Returning issue from buffer");
        Poll::Ready(self.buffer.pop())
    }
}

#[async_recursion]
async fn func(mut stream: IssuePullRequestStream) -> Option<(Value, IssuePullRequestStream)> {
    trace!(target: "dean::github_client::func", "Polling stream");
    if stream.buffer.is_empty() {
        trace!(target: "dean::github_client::func", "Buffer is empty, updating it");
        match stream.update_buffer(None).await {
            Ok(()) => {
                trace!(target: "dean::github_client::func", "Buffer update is ready");
            }
            Err(e) => {
                trace!(target: "dean::github_client", "Failed to update buffer: {}", e);
                return None;
            }
        };
    };
    match stream.buffer.pop() {
        None => {
            trace!(target: "dean::github_client::func", "Buffer is empty, stopping");
            None
        }
        Some(value) => {
            trace!(target: "dean::github_client::func", "Returning issue from buffer");
            Some((value, stream))
        }
    }
}

#[async_trait]
impl IssueClient for Client {
    async fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
        _last_issues: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let stream = self.all_issues_iterator(organization, repo);

        let unfold = futures::stream::unfold(stream, func);
        let issues = unfold.filter(|value| futures::future::ready(value.get("pull_request").is_none()));
        Box::new(Box::pin(issues))
    }

    async fn get_last_pull_requests(
        &self,
        organization: &str,
        repo: &str,
        _last_pull_requests: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let stream = self.all_issues_iterator(organization, repo);

        let unfold = futures::stream::unfold(stream, func);
        let pull_requests =
            unfold.filter(|value| futures::future::ready(value.get("pull_request").is_some()));
        Box::new(Box::pin(pull_requests))
    }
}

impl Client {
    pub fn new<C>(client: C, auth: Authentication) -> Self
    where
        C: Into<Arc<reqwest::Client>>,
    {
        Self {
            client: client.into(),
            auth,
        }
    }

    fn all_issues_iterator(&self, organization: &str, repo: &str) -> IssuePullRequestStream {
        IssuePullRequestStream {
            client: self.client.clone(),
            next_page: Some(format!(
                "https://api.github.com/repos/{}/{}/issues?state=all&direction=asc&sort=created&per_page=100&page=1",
                organization, repo
            )),
            buffer: vec![],
            auth: self.auth.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use log::info;
    use time::format_description::well_known::Rfc3339;

    use super::*;

    #[tokio::test]
    async fn it_retrieves_the_issues_from_dean_from_newer_to_older() {
        let client = Client::new(reqwest::Client::new(), authentication());

        let issues = client
            .get_last_issues("StaticDependencyAnalyzer", "dean", 100)
            .await
            .collect::<Vec<_>>()
            .await;

        assert!(issues.len() >= 6);
        assert!(creation_timestamp(&issues[0]) > creation_timestamp(&issues[1]));
    }

    #[tokio::test]
    async fn it_retrieves_the_pull_requests_from_dean_from_newer_to_older() {
        let client = Client::new(reqwest::Client::new(), authentication());

        let prs = client
            .get_last_pull_requests("StaticDependencyAnalyzer", "dean", 100)
            .await
            .collect::<Vec<_>>()
            .await;

        assert!(prs.len() > 10);
        assert!(creation_timestamp(&prs[0]) > creation_timestamp(&prs[1]));
    }

    #[tokio::test]
    async fn it_retrieves_150_issues_from_rust_lang() {
        let client = Client::new(reqwest::Client::new(), authentication());

        let issues = client.get_last_issues("rust-lang", "rust", 150).await;
        let issue_count = issues.take(150).count().await;
        assert!(issue_count > 0);
        assert!(issue_count <= 150);

        info!("New issues!");
        let mut issues = client.get_last_issues("rust-lang", "rust", 150).await;
        assert_eq!(
            issues
                .next()
                .await
                .as_ref()
                .unwrap()
                .get("repository_url")
                .unwrap()
                .as_str()
                .unwrap(),
            "https://api.github.com/repos/rust-lang/rust"
        );
    }

    fn creation_timestamp(issue_or_pr: &Value) -> i64 {
        let created_at_str = issue_or_pr["created_at"].as_str().unwrap();
        time::OffsetDateTime::parse(created_at_str, &Rfc3339)
            .unwrap()
            .unix_timestamp()
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
