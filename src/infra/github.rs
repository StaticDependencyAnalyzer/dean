use std::sync::Arc;

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
    fn update_buffer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.next_page.is_none() {
            return Ok(());
        }

        let url = self.next_page.take().unwrap();

        debug!(target: "dean::github_client", "Fetching issues from {}", url);
        let mut request = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/81.0.4044.138 Safari/537.36")
            .header("Accept", "application/vnd.github.v3+json");

        if let Authentication::Basic(user, passwd) = &self.auth {
            request = request.basic_auth(user, passwd.as_ref());
        }

        let response = request.send().context("Failed to get issues")?;

        if response.status().as_u16() == 403 {
            warn!("Github API rate limit exceeded, retrying in 5 minutes");
            std::thread::sleep(std::time::Duration::from_secs(5 * 60));
            return self.update_buffer();
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

        match self.update_buffer() {
            Ok(_) => self.buffer.pop(),
            Err(error) => {
                error!("error retrieving issues: {:?}", error);
                None
            }
        }
    }
}

impl IssueClient for Client {
    fn get_issues(&self, organization: &str, repo: &str) -> Box<dyn Iterator<Item = Value>> {
        let iter = self
            .all_issues_iterator(organization, repo)
            .filter(|issue| issue.get("pull_request").is_none());
        Box::new(iter)
    }

    fn get_pull_requests(&self, organization: &str, repo: &str) -> Box<dyn Iterator<Item = Value>> {
        let iter = self
            .all_issues_iterator(organization, repo)
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
                "https://api.github.com/repos/{}/{}/issues?state=all&per_page=100&page=1",
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
    fn it_retrieves_all_the_issues_from_dean() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let issues = client.get_issues("StaticDependencyAnalyzer", "dean");

        assert!(issues.count() >= 6);
    }

    #[test]
    fn it_retrieves_all_prs_from_dean() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let prs = client.get_pull_requests("StaticDependencyAnalyzer", "dean");

        assert!(prs.count() >= 6);
    }

    #[test]
    fn it_retrieves_150_issues_from_rust_lang() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let issues = client.get_issues("rust-lang", "rust");
        assert_eq!(issues.take(150).count(), 150);

        let mut issues = client.get_issues("rust-lang", "rust");
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

    fn authentication() -> Authentication {
        let github_username = std::env::var("GITHUB_USERNAME").ok();
        let github_password = std::env::var("GITHUB_PASSWORD").ok();

        match github_username {
            None => Authentication::None,
            Some(github_username) => Authentication::Basic(github_username, github_password),
        }
    }
}
