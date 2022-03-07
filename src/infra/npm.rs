use crate::infra::http;
use crate::pkg::npm::{InfoRetriever, Repository};
use regex::Regex;
use serde_json::Value;

pub struct DependencyInfoRetriever {
    client: http::Client,
    github_registry_regex: Regex,
    gitlab_registry_regex: Regex,
}

impl Default for DependencyInfoRetriever {
    fn default() -> Self {
        DependencyInfoRetriever {
            client: http::Client::new(),
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

impl InfoRetriever for DependencyInfoRetriever {
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

        let repository = response["repository"]["url"]
            .as_str()
            .or_else(|| response["homepage"].as_str())
            .map(|str| str.to_string());

        if repository.is_none() {
            return Ok(Repository::Unknown);
        }

        return if self
            .github_registry_regex
            .is_match(repository.as_ref().unwrap())
        {
            let captures = self
                .github_registry_regex
                .captures(repository.as_ref().unwrap())
                .ok_or_else(|| {
                    format!(
                        "repository '{}' does not match expression",
                        &repository.as_ref().unwrap()
                    )
                })?;

            Ok(Repository::GitHub {
                organization: captures["organization"].to_string(),
                name: captures["name"].to_string(),
            })
        } else if self
            .gitlab_registry_regex
            .is_match(repository.as_ref().unwrap())
        {
            let captures = self
                .gitlab_registry_regex
                .captures(repository.as_ref().unwrap())
                .ok_or_else(|| {
                    format!(
                        "repository '{}' does not match expression",
                        &repository.as_ref().unwrap()
                    )
                })?;

            Ok(Repository::GitLab {
                organization: captures["organization"].to_string(),
                name: captures["name"].to_string(),
            })
        } else {
            Ok(Repository::Raw {
                address: repository.as_ref().unwrap().to_string(),
            })
        };
    }
}

#[cfg(test)]
mod tests {
    use super::DependencyInfoRetriever;
    use crate::pkg::npm::{InfoRetriever, Repository};
    use expects::equal::{be_ok, equal};
    use expects::Subject;

    #[test]
    fn retrieves_the_latest_version_of_colors() {
        let retriever = DependencyInfoRetriever::default();

        let result = retriever.latest_version("colors");

        result.should(be_ok(equal("1.4.0")));
    }

    #[test]
    fn retrieves_the_repository_of_colors() {
        let retriever = DependencyInfoRetriever::default();

        let result = retriever.repository("colors");

        result.should(be_ok(equal(Repository::GitHub {
            organization: "Marak".into(),
            name: "colors.js".into(),
        })));
    }

    #[test]
    fn retrieves_the_repository_of_babel() {
        let retriever = DependencyInfoRetriever::default();

        let result = retriever.repository("babel");

        result.should(be_ok(equal(Repository::GitHub {
            organization: "babel".into(),
            name: "babel".into(),
        })))
    }

    #[test]
    fn retrieves_the_gitlab_repository_of_bfj() {
        let retriever = DependencyInfoRetriever::default();

        let result = retriever.repository("bfj");

        result.should(be_ok(equal(Repository::GitLab {
            organization: "philbooth".into(),
            name: "bfj".into(),
        })))
    }
}
