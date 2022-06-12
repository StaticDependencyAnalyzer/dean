use std::error::Error;
use std::sync::Arc;

use crate::pkg::policy::ContributionDataRetriever;
use crate::{Dependency, Evaluation, Policy};

pub struct MaxIssueLifespan {
    max_issue_lifespan: f64,
    last_issues: usize,
    contribution_data_retriever: Arc<dyn ContributionDataRetriever>,
}

impl Policy for MaxIssueLifespan {
    fn evaluate(&self, dependency: &Dependency) -> Result<Evaluation, Box<dyn Error>> {
        let issue_lifespan = self
            .contribution_data_retriever
            .get_issue_lifespan(&dependency.repository, self.last_issues)?;

        if issue_lifespan > self.max_issue_lifespan {
            let fail_score = if self.max_issue_lifespan == 0.0 {
                1.0
            } else {
                issue_lifespan / self.max_issue_lifespan
            };
            Ok(Evaluation::Fail("max_issue_lifespan".to_string(),dependency.clone(), format!("the issue lifespan is {} seconds, which is greater than the maximum allowed lifespan of {} seconds", issue_lifespan, self.max_issue_lifespan), fail_score))
        } else {
            Ok(Evaluation::Pass(
                "max_issue_lifespan".to_string(),
                dependency.clone(),
            ))
        }
    }
}

impl MaxIssueLifespan {
    pub fn new<C: Into<Arc<dyn ContributionDataRetriever>>>(
        contribution_data_retriever: C,
        max_issue_lifespan: f64,
        last_issues: usize,
    ) -> Self {
        Self {
            contribution_data_retriever: contribution_data_retriever.into(),
            max_issue_lifespan,
            last_issues,
        }
    }
}

#[cfg(test)]
mod tests {
    use expects::matcher::equal;
    use expects::Subject;

    use super::super::{ContributionDataRetriever, MockContributionDataRetriever, Policy};
    use super::*;
    use crate::pkg::Repository::GitHub;
    use crate::{Dependency, Evaluation};

    #[test]
    fn it_passes_if_the_issue_lifetime_is_lower_than_the_maximum_allowed() {
        let retriever = {
            let mut retriever = MockContributionDataRetriever::new();
            retriever
                .expect_get_issue_lifespan()
                .return_once(|_, _| Ok(42_f64));
            Box::new(retriever) as Box<dyn ContributionDataRetriever>
        };

        let max_allowed_issue_lifespan = 100_f64;
        let issue_lifespan = MaxIssueLifespan::new(retriever, max_allowed_issue_lifespan, 100);

        let evaluation = issue_lifespan.evaluate(&dependency());
        evaluation.unwrap().should(equal(Evaluation::Pass(
            "max_issue_lifespan".to_string(),
            dependency(),
        )));
    }

    #[test]
    fn it_fails_if_the_issue_lifetime_is_higher_than_the_maximum_expected() {
        let retriever = {
            let mut retriever = MockContributionDataRetriever::new();
            retriever
                .expect_get_issue_lifespan()
                .return_once(|_, _| Ok(102_f64));
            Box::new(retriever) as Box<dyn ContributionDataRetriever>
        };

        let max_allowed_issue_lifespan = 100_f64;
        let issue_lifespan = MaxIssueLifespan::new(retriever, max_allowed_issue_lifespan, 100);

        let evaluation = issue_lifespan.evaluate(&dependency());
        match evaluation.unwrap() {
            Evaluation::Fail(policy, dep, reason, score) => {
                policy.should(equal("max_issue_lifespan".to_string()));
                dep.should(equal(dependency()));
                reason.should(equal("the issue lifespan is 102 seconds, which is greater than the maximum allowed lifespan of 100 seconds"));
                assert!((score - 1.02).abs() < f64::EPSILON);
            }
            Evaluation::Pass(_, _) => {
                unreachable!()
            }
        }
    }

    fn dependency() -> Dependency {
        Dependency {
            name: "foo".to_string(),
            version: "1.2.3".to_string(),
            latest_version: Some("1.2.4".to_string()),
            repository: GitHub {
                organization: "some_org".to_string(),
                name: "some_name".to_string(),
            },
        }
    }
}
