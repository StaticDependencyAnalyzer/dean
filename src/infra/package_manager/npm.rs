use regex::Regex;
use serde_json::Value;

use crate::infra::http;
use crate::pkg::Repository;

pub struct InfoRetriever {
    client: http::Client,
    github_registry_regex: Regex,
    gitlab_registry_regex: Regex,
}

impl InfoRetriever {
    pub fn new(client: http::Client) -> Self {
        Self {
            client,
            github_registry_regex: Regex::new(
                ".*?github.com[:/](?P<organization>.*?)/(?P<name>.*?)(?:$|\\.git|/)",
            )
            .unwrap(),
            gitlab_registry_regex: Regex::new(
                ".*?gitlab.com[:/](?P<organization>.*?)/(?P<name>.*?)(?:$|\\.git|/)",
            )
            .unwrap(),
        }
    }
}

impl Default for InfoRetriever {
    fn default() -> Self {
        InfoRetriever::new(http::Client::default())
    }
}

impl crate::InfoRetriever for InfoRetriever {
    fn latest_version(&self, package_name: &str) -> Result<String, String> {
        let response: Value = self
            .client
            .get(format!("https://registry.npmjs.org/{}", package_name).as_str())?
            .json()?;

        Ok(response["dist-tags"]["latest"]
            .as_str()
            .ok_or_else(|| "latest is not a string".to_string())?
            .to_string())
    }

    fn repository(&self, package_name: &str) -> Result<Repository, String> {
        let response: Value = self
            .client
            .get(format!("https://registry.npmjs.org/{}", package_name).as_str())?
            .json()?;

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
    use expects::matcher::{be_ok, equal};
    use expects::Subject;

    use super::InfoRetriever;
    use crate::pkg::Repository;
    use crate::InfoRetriever as _;

    #[test]
    fn retrieves_the_latest_version_of_colors() {
        let retriever = InfoRetriever::default();

        let result = retriever.latest_version("colors");

        result.should(be_ok(equal("1.4.0")));
    }

    #[test]
    fn retrieves_the_repository_of_colors() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("colors");

        result.should(be_ok(equal(Repository::GitHub {
            organization: "Marak".into(),
            name: "colors.js".into(),
        })));
    }

    #[test]
    fn retrieves_the_repository_of_babel() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("babel");

        result.should(be_ok(equal(Repository::GitHub {
            organization: "babel".into(),
            name: "babel".into(),
        })));
    }

    #[test]
    fn retrieves_the_gitlab_repository_of_bfj() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("bfj");

        result.should(be_ok(equal(Repository::GitLab {
            organization: "philbooth".into(),
            name: "bfj".into(),
        })));
    }

    #[test]
    fn retrieves_the_raw_repository_of_atob() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("atob");

        result.should(be_ok(equal(Repository::Raw {
            address: "git://git.coolaj86.com/coolaj86/atob.js.git".into(),
        })));
    }

    #[test]
    fn retrieves_unknown_repository_of_json5() {
        let retriever = InfoRetriever::default();

        let result = retriever.repository("@types/json5");

        result.should(be_ok(equal(Repository::Unknown)));
    }
}
