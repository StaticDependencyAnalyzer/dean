use std::sync::Arc;

use anyhow::Context;
use serde_json::{Map, Value};

use crate::pkg::Repository;

#[derive(Default)]
pub struct InfoRetriever {
    client: Arc<reqwest::blocking::Client>,
}

impl InfoRetriever {
    pub fn new<C>(client: C) -> Self
    where
        C: Into<Arc<reqwest::blocking::Client>>,
    {
        Self {
            client: client.into(),
        }
    }

    fn make_request(&self, dependency: &str) -> Result<Map<String, Value>, String> {
        let result: Value = self
            .client
            .get(&format!("https://crates.io/api/v1/crates/{}", dependency))
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36")
            .send().context("unable to request crates.io").map_err(|e| e.to_string())?
            .json().context("unable to parse crates.io response").map_err(|e| e.to_string())?;

        if !result.is_object() {
            return Err(format!(
                "unable to retrieve latest version for {}",
                dependency
            ));
        }

        Ok(result.as_object().unwrap().clone())
    }
}

impl crate::pkg::InfoRetriever for InfoRetriever {
    fn latest_version(&self, dependency: &str) -> Result<String, String> {
        let response_object = self.make_request(dependency)?;

        let crate_info = response_object
            .get("crate")
            .ok_or("crate key is not present in the API response")?;

        let newest_version = crate_info
            .get("newest_version")
            .ok_or("newest_version key is not present in the API response")?;

        newest_version
            .as_str()
            .ok_or_else(|| "newest_version is not a string".to_string())
            .map(std::string::ToString::to_string)
    }

    fn repository(&self, dependency: &str) -> Result<Repository, String> {
        let response_object = self.make_request(dependency)?;

        let crate_info = response_object
            .get("crate")
            .ok_or("crate key is not present in the API response")?;

        let repository_info = crate_info
            .get("repository")
            .ok_or("repository key is not present in the API response")?;

        let repository = repository_info
            .as_str()
            .ok_or_else(|| "repository is not a string".to_string())?;

        Ok(Repository::parse_url(repository))
    }
}

#[cfg(test)]
mod tests {
    use expects::matcher::equal;
    use expects::Subject;

    use super::*;
    use crate::pkg::InfoRetriever as _;

    #[test]
    fn it_retrieves_the_latest_version_of_serde() {
        let retriever = InfoRetriever::default();

        let result = retriever.latest_version("serde");

        result.unwrap().should(equal("1.0.137"));
    }

    #[test]
    fn it_retrieves_the_repository_of_serde() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("serde");

        result.unwrap().should(equal(Repository::GitHub {
            organization: "serde-rs".to_string(),
            name: "serde".to_string(),
        }));
    }
}
