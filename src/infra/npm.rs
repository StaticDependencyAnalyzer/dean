use crate::pkg::npm::{InfoRetriever, Repository};
use regex::Regex;
use serde_json::Value;

pub struct DependencyInfoRetriever {
    pub github_registry_regex: Regex,
}

impl Default for DependencyInfoRetriever {
    fn default() -> Self {
        DependencyInfoRetriever {
            github_registry_regex: Regex::new(
                ".*?github.com[:/](?P<organization>.*?)/(?P<name>.*?)(?:$|\\.git)",
            )
            .unwrap(),
        }
    }
}

impl InfoRetriever for DependencyInfoRetriever {
    fn latest_version(&self, package_name: &str) -> Result<String, String> {
        let response = ureq::get(format!("https://registry.npmjs.org/{}", package_name).as_str())
            .call()
            .map_err(|err| err.to_string())?;

        let json: Value = response.into_json().map_err(|err| err.to_string())?;
        Ok(json["dist-tags"]["latest"]
            .as_str()
            .ok_or_else(|| "latest is not a string".to_string())?
            .to_string())
    }

    fn repository(&self, package_name: &str) -> Result<Repository, String> {
        let response = ureq::get(format!("https://registry.npmjs.org/{}", package_name).as_str())
            .call()
            .map_err(|err| err.to_string())?;

        let json: Value = response.into_json().map_err(|err| err.to_string())?;
        let repository = json["homepage"]
            .as_str()
            .ok_or("homepage is not a string")?
            .to_string();

        return if self.github_registry_regex.is_match(&repository) {
            let captures = self
                .github_registry_regex
                .captures(&repository)
                .ok_or_else(|| format!("repository '{}' does not match expression", &repository))?;

            Ok(Repository::GitHub {
                organization: captures["organization"].to_string(),
                name: captures["name"].to_string(),
            })
        } else {
            Err("no repository matched".into())
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
}
