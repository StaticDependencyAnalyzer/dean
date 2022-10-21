use std::error::Error;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;

use anyhow::Context;
use async_recursion::async_recursion;
use async_trait::async_trait;
use futures_util::future::FutureExt;
use futures_util::StreamExt;
use log::{debug, error, warn};
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

        let url = self.next_page.take().unwrap();

        debug!(target: "dean::github_client", "Fetching issues from {}", url);
        let mut request = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/81.0.4044.138 Safari/537.36")
            .header("Accept", "application/vnd.github.v3+json");

        if let Authentication::Basic(user, passwd) = &self.auth {
            request = request.basic_auth(user, passwd.as_ref());
        }

        let response = request.send().await.context("Failed to get issues")?;

        if response.status().as_u16() == 403 {
            let total_allowed_attempts = 8;
            self.next_page = Some(url);
            // FIXME: Naive implementation of a retry strategy, change this to use the x-ratelimit-reset header in the response.
            return match backoff {
                None => {
                    let initial_duration = Duration::from_secs(15);
                    warn!(target: "dean::github_client", "Github API rate limit exceeded, remaining attempts [{}/{}], retrying in {} seconds", total_allowed_attempts, total_allowed_attempts, initial_duration.as_secs());
                    std::thread::sleep(initial_duration);
                    self.update_buffer(Some((total_allowed_attempts - 1, initial_duration * 2)))
                        .await
                }
                Some((remaining_attempts, duration)) => {
                    if remaining_attempts == 0 {
                        return Err("Github API rate limit exceeded, stopping iteration".into());
                    }

                    warn!(target: "dean::github_client", "Github API rate limit exceeded, remaining attempts [{}/{}], retrying in {} seconds", remaining_attempts, total_allowed_attempts, duration.as_secs());
                    std::thread::sleep(duration);
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
        if self.buffer.is_empty() {
            match self.as_mut().update_buffer(None).now_or_never() {
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    error!(target: "dean::github_client", "Failed to update buffer: {}", e);
                    return Poll::Ready(None);
                }
                None => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            }
        }

        Poll::Ready(self.buffer.pop())
    }
}

pub struct IssueStream {
    issue_pull_request_iterator: IssuePullRequestStream,
    issues_to_return: usize,
    returned_issues: usize,
}

impl Stream for IssueStream {
    type Item = Value;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let poll = self.as_mut().issue_pull_request_iterator.poll_next_unpin(cx);
        match poll {
            Poll::Ready(element) => {
                if self.as_ref().returned_issues >= self.as_ref().issues_to_return {
                    return Poll::Ready(None);
                }

                if let Some(inner) = element {
                    if inner.get("pull_request").is_none() {
                        self.returned_issues += 1;
                        Poll::Ready(Some(inner))
                    } else {
                        self.poll_next(cx)
                    }
                } else {
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct PullRequestStream {
    issue_pull_request_iterator: IssuePullRequestStream,
}

impl Stream for PullRequestStream {
    type Item = Value;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let poll = self.as_mut().issue_pull_request_iterator.poll_next_unpin(cx);
        match poll {
            Poll::Ready(element) => {
                if let Some(inner) = element {
                    if inner.get("pull_request").is_some() {
                        Poll::Ready(Some(inner))
                    } else {
                        self.poll_next(cx)
                    }
                } else {
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
        // return poll;
    }
}

#[async_trait]
impl IssueClient for Client {
    async fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
        last_issues: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        Box::new(IssueStream {
            issue_pull_request_iterator: self.all_issues_iterator(organization, repo),
            issues_to_return: last_issues,
            returned_issues: 0,
        })
    }

    async fn get_last_pull_requests(
        &self,
        organization: &str,
        repo: &str,
        _last_pull_requests: usize,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        Box::new(PullRequestStream {
            issue_pull_request_iterator: self.all_issues_iterator(organization, repo),
        })
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
        assert!(issues.count().await <= 150);

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
