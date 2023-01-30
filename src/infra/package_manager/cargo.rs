use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use serde_json::{Map, Value};

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

    async fn make_request(&self, dependency: &str) -> Result<Map<String, Value>> {
        let result: Value = self
            .client
            .get(&format!("https://crates.io/api/v1/crates/{dependency}"))
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36")
            .send().await.context("unable to request crates.io")?
            .json().await.context("unable to parse crates.io response")?;

        if !result.is_object() {
            return Err(anyhow!(
                "unable to retrieve latest version for {}",
                dependency
            ));
        }

        Ok(result.as_object().unwrap().clone())
    }
}

#[async_trait]
impl crate::pkg::InfoRetriever for InfoRetriever {
    async fn latest_version(&self, dependency: &str) -> Result<String> {
        let response_object = self.make_request(dependency).await?;

        let crate_info = response_object
            .get("crate")
            .context("crate key is not present in the API response")?;

        let newest_version = crate_info
            .get("newest_version")
            .context("newest_version key is not present in the API response")?;

        newest_version
            .as_str()
            .context("newest_version is not a string")
            .map(std::string::ToString::to_string)
    }

    async fn repository(&self, dependency: &str) -> Result<Repository> {
        let response_object = self.make_request(dependency).await?;

        let crate_info = response_object
            .get("crate")
            .context("crate key is not present in the API response")?;

        let repository_info = crate_info
            .get("repository")
            .context("repository key is not present in the API response")?;

        let repository = repository_info
            .as_str()
            .context("repository is not a string")?;

        Ok(Repository::parse_url(repository))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::InfoRetriever as _;

    #[tokio::test]
    async fn it_retrieves_the_latest_version_of_yaml_rust() {
        let retriever = InfoRetriever::default();

        let result = retriever.latest_version("yaml-rust").await;

        assert_eq!(result.unwrap(), "0.4.5");
    }

    #[tokio::test]
    async fn it_retrieves_the_repository_of_serde() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("serde").await;

        assert_eq!(
            result.unwrap(),
            Repository::GitHub {
                organization: "serde-rs".into(),
                name: "serde".into(),
            }
        );
    }
}
