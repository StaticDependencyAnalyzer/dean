use std::error::Error;
use std::sync::Arc;

use crate::infra::github;
use crate::pkg::policy::max_issue_lifespan::ContributionDataRetriever;
use crate::pkg::Repository;

pub struct Retriever {
    github_client: Arc<github::Client>,
}

impl Retriever {
    pub fn new<C>(github_client: C) -> Self
    where
        C: Into<Arc<github::Client>>,
    {
        Self {
            github_client: github_client.into(),
        }
    }

    fn get_github_issue_lifespan(&self, organization: &str, repo: &str) -> f64 {
        let issues = self.github_client.get_issues(organization, repo);

        let closed_issues = issues
            .filter(|issue| issue.get("state").is_some())
            .filter(|issue| issue.get("state").unwrap().as_str().is_some())
            .filter(|issue| issue.get("state").unwrap().as_str().unwrap() == "closed");

        let lifespan_per_issue = closed_issues.map(|issue| {
            let created_at_str = issue.get("created_at").unwrap().as_str().unwrap();
            let closed_at_str = issue.get("closed_at").unwrap().as_str().unwrap();

            let created_at = chrono::DateTime::parse_from_rfc3339(created_at_str).unwrap();
            let closed_at = chrono::DateTime::parse_from_rfc3339(closed_at_str).unwrap();

            let lifespan = closed_at.signed_duration_since(created_at);
            f64::from(i32::try_from(lifespan.num_seconds()).unwrap())
        });

        lifespan_per_issue.mean()
    }
}

trait Mean {
    fn mean(self) -> f64;
}

impl<F, T> Mean for T
where
    T: Iterator<Item = F>,
    F: std::borrow::Borrow<f64>,
{
    fn mean(self) -> f64 {
        self.zip(1..).fold(0., |s, (e, i)| {
            (*e.borrow() + s * f64::from(i - 1)) / f64::from(i)
        })
    }
}

impl ContributionDataRetriever for Retriever {
    fn get_issue_lifespan(&self, repository: &Repository) -> Result<f64, Box<dyn Error>> {
        match repository {
            Repository::Unknown => Err("unknown repository".into()),
            Repository::GitHub { name, organization } => {
                Ok(self.get_github_issue_lifespan(organization, name))
            }
            Repository::GitLab { .. } | Repository::Raw { .. } => Err("not implemented".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::github::Authentication;
    use crate::pkg::Repository;

    #[test]
    fn it_retrieves_the_issue_lifespan_of_dean() {
        let http_client = reqwest::blocking::Client::default();
        let github_client = github::Client::new(http_client, authentication());
        let retriever = Retriever::new(github_client);

        let issue_lifespan: f64 = retriever
            .get_issue_lifespan(&Repository::GitHub {
                organization: "StaticDependencyAnalyzer".to_string(),
                name: "dean".to_string(),
            })
            .unwrap();

        let two_months_in_seconds = 2.0 * 30.0 * 24.0 * 60.0 * 60.0;
        let three_months_in_seconds = 3.0 * 30.0 * 24.0 * 60.0 * 60.0;
        assert!(issue_lifespan > two_months_in_seconds);
        assert!(issue_lifespan < three_months_in_seconds);
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
