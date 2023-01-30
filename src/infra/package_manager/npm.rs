use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use serde_json::Value;

use crate::pkg::Repository;
use crate::Result;

#[derive(Default)]
pub struct InfoRetriever {
    client: Arc<reqwest::Client>,
}

impl InfoRetriever {
    pub fn new<C>(client: C) -> Self
    where
        C: Into<Arc<reqwest::Client>>,
    {
        Self {
            client: client.into(),
        }
    }
}

#[async_trait]
impl crate::pkg::InfoRetriever for InfoRetriever {
    async fn latest_version(&self, package_name: &str) -> Result<String> {
        let response: Value = self
            .client
            .get(format!("https://registry.npmjs.org/{package_name}").as_str())
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36")
            .send().await.context("unable to request npmjs.org")?
            .json().await.context("unable to parse npmjs.org response")?;

        Ok(response["dist-tags"]["latest"]
            .as_str()
            .context("latest is not a string")?
            .to_string())
    }

    async fn repository(&self, package_name: &str) -> Result<Repository> {
        let response: Value = self
            .client
            .get(format!("https://registry.npmjs.org/{package_name}").as_str())
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36")
            .send().await.context("unable to request npmjs.org")?
            .json().await.context("unable to parse npmjs.org response")?;

        let possible_repository = response["repository"]["url"]
            .as_str()
            .or_else(|| response["homepage"].as_str())
            .map(ToString::to_string);

        if possible_repository.is_none() {
            return Ok(Repository::Unknown);
        }

        let repository = possible_repository.as_ref().unwrap();

        Ok(Repository::parse_url(repository))
    }
}

#[cfg(test)]
mod tests {
    use super::InfoRetriever;
    use crate::pkg::InfoRetriever as _;
    use crate::pkg::Repository;

    #[tokio::test]
    async fn retrieves_the_latest_version_of_colors() {
        let retriever = InfoRetriever::default();

        let result = retriever.latest_version("colors").await;

        assert_eq!(result.unwrap(), "1.4.0");
    }

    #[tokio::test]
    async fn retrieves_the_repository_of_colors() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("colors").await;

        assert_eq!(
            result.unwrap(),
            Repository::GitHub {
                organization: "Marak".into(),
                name: "colors.js".into(),
            }
        );
    }

    #[tokio::test]
    async fn retrieves_the_repository_of_babel() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("babel").await;

        assert_eq!(
            result.unwrap(),
            Repository::GitHub {
                organization: "babel".into(),
                name: "babel".into(),
            }
        );
    }

    #[tokio::test]
    async fn retrieves_the_gitlab_repository_of_bfj() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("bfj").await;

        assert_eq!(
            result.unwrap(),
            Repository::GitLab {
                organization: "philbooth".into(),
                name: "bfj".into(),
            }
        );
    }

    #[tokio::test]
    async fn retrieves_the_raw_repository_of_atob() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("atob").await;

        assert_eq!(
            result.unwrap(),
            Repository::Raw {
                address: "git://git.coolaj86.com/coolaj86/atob.js.git".into(),
            }
        );
    }

    #[tokio::test]
    async fn retrieves_unknown_repository_of_json5() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("@types/json5").await;

        assert_eq!(result.unwrap(), Repository::Unknown);
    }
}
