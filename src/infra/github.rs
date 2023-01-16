use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_recursion::async_recursion;
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, trace};
use reqwest::header::{HeaderMap, HeaderValue};
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
    async fn update_buffer(&mut self) -> Result<(), Box<dyn Error>> {
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
            self.next_page = Some(url);

            let rate_limit_timestamp_seconds = response
                .headers()
                .get("x-ratelimit-reset")
                .unwrap_or(&HeaderValue::from(0))
                .to_str()
                .context("unable to convert rate limit reset header to str")?
                .parse::<u64>()?;

            let rate_limit_sleep_duration = Duration::from_secs(rate_limit_timestamp_seconds)
                .checked_sub(SystemTime::now().duration_since(UNIX_EPOCH)?)
                .context("unable to substract dates")?
                .checked_add(Duration::from_secs(5))
                .context("unable to add duration increment")?;

            tokio::time::sleep(rate_limit_sleep_duration).await;
            return self.update_buffer().await;
        }

        if let Some(next_page) = Self::extract_next_page_from_headers(response.headers()) {
            self.next_page = Some(next_page.to_string());
        }

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

    fn extract_next_page_from_headers(headers: &HeaderMap) -> Option<&str> {
        let link_header = headers.get("link")?;
        let link_header_as_str = link_header.to_str().ok()?;
        let next_rel_link = link_header_as_str
            .split(',')
            .find(|link| link.contains("rel=\"next\""))?;
        let first_next_rel_link = next_rel_link.split(';').next()?;
        let first_next_rel_link_without_weird_characters = first_next_rel_link
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>');

        if first_next_rel_link_without_weird_characters.is_empty() {
            None
        } else {
            Some(first_next_rel_link_without_weird_characters)
        }
    }
}

async fn fetch_value_from_stream(mut stream: IssuePullRequestStream) -> Option<(Value, IssuePullRequestStream)> {
    trace!(target: "dean::github_client::func", "Polling stream");
    if stream.buffer.is_empty() {
        trace!(target: "dean::github_client::func", "Buffer is empty, updating it");
        match stream.update_buffer().await {
            Ok(_) => {
                trace!(target: "dean::github_client::func", "Buffer update is ready");
            }
            Err(e) => {
                trace!(target: "dean::github_client", "Failed to update buffer: {}", e);
                return None;
            }
        };
    };
    if let Some(value_from_buffer) = stream.buffer.pop() {
        trace!(target: "dean::github_client::func", "Returning issue from buffer");
        Some((value_from_buffer, stream))
    } else {
        trace!(target: "dean::github_client::func", "Buffer is empty, stopping");
        None
    }
}

#[async_trait]
impl IssueClient for Client {
    async fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let stream = self.all_issues_iterator(organization, repo);

        let values_from_the_stream = futures::stream::unfold(stream, fetch_value_from_stream);
        let issues =
            values_from_the_stream.filter(|value| futures::future::ready(value.get("pull_request").is_none()));
        Box::new(Box::pin(issues))
    }

    async fn get_last_pull_requests(
        &self,
        organization: &str,
        repo: &str,
    ) -> Box<dyn Stream<Item = Value> + Unpin + Send> {
        let stream = self.all_issues_iterator(organization, repo);

        let values_from_the_stream = futures::stream::unfold(stream, fetch_value_from_stream);
        let pull_requests =
            values_from_the_stream.filter(|value| futures::future::ready(value.get("pull_request").is_some()));
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
            .get_last_issues("StaticDependencyAnalyzer", "dean")
            .await
            .take(100)
            .collect::<Vec<_>>()
            .await;

        assert!(issues.len() >= 6);
        assert!(creation_timestamp(&issues[0]) > creation_timestamp(&issues[1]));
    }

    #[tokio::test]
    async fn it_retrieves_the_pull_requests_from_dean_from_newer_to_older() {
        let client = Client::new(reqwest::Client::new(), authentication());

        let prs = client
            .get_last_pull_requests("StaticDependencyAnalyzer", "dean")
            .await
            .take(100)
            .collect::<Vec<_>>()
            .await;

        assert!(prs.len() > 10);
        assert!(creation_timestamp(&prs[0]) > creation_timestamp(&prs[1]));
    }

    #[tokio::test]
    async fn it_retrieves_150_issues_from_rust_lang() {
        let client = Client::new(reqwest::Client::new(), authentication());

        let issues = client.get_last_issues("rust-lang", "rust").await.take(150);
        let issue_count = issues.take(150).count().await;
        assert!(issue_count > 0);
        assert!(issue_count <= 150);

        info!("New issues!");
        let mut issues = client.get_last_issues("rust-lang", "rust").await.take(150);
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
