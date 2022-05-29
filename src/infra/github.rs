use std::sync::Arc;

use anyhow::Context;
use log::{error, trace};
use serde_json::Value;

#[derive(Clone)]
pub enum Authentication {
    None,
    Basic(String, Option<String>),
}

pub struct Client {
    client: Arc<reqwest::blocking::Client>,
    auth: Authentication,
}

#[derive(Clone)]
pub struct IssueIterator {
    client: Arc<reqwest::blocking::Client>,
    next_page: Option<String>,
    buffer: Vec<Value>,
    auth: Authentication,
}

impl IssueIterator {
    fn update_buffer(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.next_page.is_none() {
            return Ok(());
        }

        let url = self.next_page.take().unwrap();

        trace!("Fetching issues from {}", url);
        let mut request = self
            .client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/81.0.4044.138 Safari/537.36")
            .header("Accept", "application/vnd.github.v3+json");

        if let Authentication::Basic(user, passwd) = &self.auth {
            request = request.basic_auth(user, passwd.as_ref());
        }

        let response = request.send().context("Failed to get issues")?;

        if let Some(link) = response.headers().get("link") {
            let link = link.to_str().unwrap();
            let link = link
                .split(',')
                .find(|link| link.contains("rel=\"next\""))
                .unwrap();
            let link = link.split(';').next().unwrap();
            let link = link.trim().trim_start_matches('<').trim_end_matches('>');
            self.next_page = Some(link.to_string());
        };

        let response_json = response.json::<Value>().context("Failed to parse issues")?;

        let issues = response_json
            .as_array()
            .context("the response is not an array")?
            .iter()
            .filter(|issue| issue.get("pull_request").is_none())
            .cloned()
            .collect::<Vec<_>>();

        self.buffer.extend_from_slice(&issues);

        Ok(())
    }
}

impl Iterator for IssueIterator {
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

impl Client {
    pub fn new<C: Into<Arc<reqwest::blocking::Client>>>(client: C, auth: Authentication) -> Self {
        Self {
            client: client.into(),
            auth,
        }
    }

    pub fn get_issues(&self, organization: &str, repo: &str) -> IssueIterator {
        IssueIterator {
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
    fn it_retrieves_150_issues_from_rust_lang() {
        let client = Client::new(reqwest::blocking::Client::new(), authentication());

        let mut issues = client.get_issues("rust-lang", "rust");

        assert_eq!(issues.clone().take(150).count(), 150);
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
