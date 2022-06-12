use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use log::{debug, error, warn};
use serde_json::Value;

use crate::infra::cached_issue_client::IssueClient;

#[derive(Clone)]
pub enum Authentication {
    None,
    Basic(String, Option<String>),
}

pub struct Client {
    client: Arc<reqwest::blocking::Client>,
    auth: Authentication,
}

pub struct IssuePullRequestIterator {
    client: Arc<reqwest::blocking::Client>,
    next_page: Option<String>,
    buffer: Vec<Value>,
    auth: Authentication,
}

impl IssuePullRequestIterator {
    fn update_buffer(
        &mut self,
        backoff: Option<(usize, std::time::Duration)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let response = request.send().context("Failed to get issues")?;

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
                }
                Some((remaining_attempts, duration)) => {
                    if remaining_attempts == 0 {
                        return Err("Github API rate limit exceeded, stopping iteration".into());
                    }

                    warn!(target: "dean::github_client", "Github API rate limit exceeded, remaining attempts [{}/{}], retrying in {} seconds", remaining_attempts, total_allowed_attempts, duration.as_secs());
                    std::thread::sleep(duration);
                    self.update_buffer(Some((remaining_attempts - 1, duration * 2)))
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

        let response_json = response.json::<Value>().context("Failed to parse issues")?;

        let issues = response_json
            .as_array()
            .context("the response is not an array")?
            .clone();

        self.buffer.extend_from_slice(&issues);

        Ok(())
    }
}

impl Iterator for IssuePullRequestIterator {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.buffer.is_empty() {
            return self.buffer.pop();
        };

        match self.update_buffer(None) {
            Ok(_) => self.buffer.pop(),
            Err(error) => {
                error!("error retrieving issues: {:?}", error);
                None
            }
        }
    }
}

impl IssueClient for Client {
    fn get_last_issues(
        &self,
        organization: &str,
        repo: &str,
        last_issues: usize,
    ) -> Box<dyn Iterator<Item = Value>> {
        let iter = self
            .all_issues_iterator(organization, repo)
            .take(last_issues)
            .filter(|issue| issue.get("pull_request").is_none());
        Box::new(iter)
    }

    fn get_last_pull_requests(
        &self,
        organization: &str,
        repo: &str,
        last_pull_requests: usize,
    ) -> Box<dyn Iterator<Item = Value>> {
        let iter = self
            .all_issues_iterator(organization, repo)
            .take(last_pull_requests)
            .filter(|issue| issue.get("pull_request").is_some());
        Box::new(iter)
    }
}

impl Client {
    pub fn new<C>(client: C, auth: Authentication) -> Self
    where
        C: Into<Arc<reqwest::blocking::Client>>,
    {
        Self {
            client: client.into(),
            auth,
        }
    }

    fn all_issues_iterator(&self, organization: &str, repo: &str) -> IssuePullRequestIterator {
        IssuePullRequestIterator {
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
    use super::*;

    #[test]
    fn it_retrieves_the_issues_from_dean_from_newer_to_older() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let issues = client
            .get_last_issues("StaticDependencyAnalyzer", "dean", 100)
            .collect::<Vec<_>>();

        assert!(issues.len() >= 6);
        assert!(creation_timestamp(&issues[0]) > creation_timestamp(&issues[1]));
    }

    #[test]
    fn it_retrieves_the_pull_requests_from_dean_from_newer_to_older() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let prs = client
            .get_last_pull_requests("StaticDependencyAnalyzer", "dean", 100)
            .collect::<Vec<_>>();

        assert!(prs.len() > 10);
        assert!(creation_timestamp(&prs[0]) > creation_timestamp(&prs[1]));
    }

    #[test]
    fn it_retrieves_150_issues_from_rust_lang() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let issues = client.get_last_issues("rust-lang", "rust", 150);
        assert!(issues.count() <= 150);

        let mut issues = client.get_last_issues("rust-lang", "rust", 150);
        assert_eq!(
            issues
                .next()
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
        let time = chrono::DateTime::parse_from_rfc3339(created_at_str).unwrap();
        time.timestamp()
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
