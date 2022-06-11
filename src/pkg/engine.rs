use std::cmp::Ordering;
use std::error::Error;

use itertools::Itertools;

use crate::{Dependency, Evaluation, Policy};

pub struct ExecutionConfig {
    regex: Option<regex::Regex>,
    policies: Vec<Box<dyn Policy>>,
}

#[cfg(test)]
impl PartialEq for ExecutionConfig {
    fn eq(&self, other: &Self) -> bool {
        if self.regex.is_none() && other.regex.is_none() {
            return self.policies.len() == other.policies.len();
        }
        if self.regex.is_some() && other.regex.is_some() {
            return self.regex.as_ref().unwrap().as_str() == other.regex.as_ref().unwrap().as_str()
                && self.policies.len() == other.policies.len();
        }
        false
    }
}

impl ExecutionConfig {
    pub fn new(
        policies: Vec<Box<dyn Policy>>,
        dependency_name_regex: Option<&str>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            regex: match dependency_name_regex {
                Some(regex) => Some(regex::Regex::new(regex)?),
                None => None,
            },
            policies,
        })
    }
}

pub struct PolicyExecutor {
    execution_configs: Vec<ExecutionConfig>,
}

fn some_options_first<T>(a: &Option<T>, b: &Option<T>) -> Ordering {
    match (a, b) {
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), Some(_)) | (None, None) => Ordering::Equal,
    }
}

impl<'a> PolicyExecutor {
    pub fn new(execution_configs: Vec<ExecutionConfig>) -> Self {
        Self {
            execution_configs: execution_configs
                .into_iter()
                .sorted_by(|a, b| some_options_first(&a.regex, &b.regex))
                .collect(),
        }
    }

    pub fn evaluate(&self, dependency: &Dependency) -> Result<Vec<Evaluation>, Box<dyn Error>> {
        let mut has_matched_regex_previously = false;
        let mut evaluations = vec![];

        for execution_config in &self.execution_configs {
            if let Some(regex) = &execution_config.regex {
                if !regex.is_match(&dependency.name) {
                    continue;
                }
                has_matched_regex_previously = true;
            } else if has_matched_regex_previously {
                continue;
            }

            for policy in &execution_config.policies {
                evaluations.push(policy.evaluate(dependency)?);
            }
        }

        Ok(evaluations)
    }
}

#[cfg(test)]
mod tests {

    use expects::matcher::consist_of;
    use expects::Subject;

    use super::*;
    use crate::pkg::policy::MockPolicy;
    use crate::pkg::Repository;
    use crate::{Dependency, Evaluation, Policy};

    #[test]
    fn it_executes_all_policies_for_a_dependency_if_they_pass() {
        let policies = vec![
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass(
                        "some_policy_name".to_string(),
                        dep.clone(),
                    ))
                });
                policy as Box<dyn Policy>
            },
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass(
                        "some_policy_name2".to_string(),
                        dep.clone(),
                    ))
                });
                policy as Box<dyn Policy>
            },
        ];
        let config = vec![ExecutionConfig::new(policies, None).unwrap()];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).unwrap();

        evaluation.should(consist_of(&[
            Evaluation::Pass("some_policy_name".to_string(), dependency()),
            Evaluation::Pass("some_policy_name2".to_string(), dependency()),
        ]));
    }

    #[test]
    fn it_executes_both_policies_even_if_one_fails() {
        let policies = vec![
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Fail(
                        "some_policy_name".to_string(),
                        dep.clone(),
                        "some_reason".into(),
                    ))
                });
                policy as Box<dyn Policy>
            },
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass(
                        "some_policy_name2".to_string(),
                        dep.clone(),
                    ))
                });
                policy as Box<dyn Policy>
            },
        ];
        let config = vec![ExecutionConfig::new(policies, None).unwrap()];

        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).unwrap();

        evaluation.should(consist_of(&[
            Evaluation::Fail(
                "some_policy_name".to_string(),
                dependency(),
                "some_reason".into(),
            ),
            Evaluation::Pass("some_policy_name2".to_string(), dependency()),
        ]));
    }

    #[test]
    fn it_executes_only_the_second_policy_because_it_doesnt_match_the_first() {
        let non_matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().never().return_once(|dep| {
                Ok(Evaluation::Fail(
                    "some_policy_name".to_string(),
                    dep.clone(),
                    "some_reason".into(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Pass(
                    "some_policy_name2".to_string(),
                    dep.clone(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(matching_policies, Some("foo")).unwrap(),
            ExecutionConfig::new(non_matching_policies, Some("bar")).unwrap(),
        ];

        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).unwrap();

        evaluation.should(consist_of(&[Evaluation::Pass(
            "some_policy_name2".to_string(),
            dependency(),
        )]));
    }

    #[test]
    fn if_the_dependency_doesnt_match_it_is_evaluated_with_the_default_policies() {
        let default_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Fail(
                    "some_policy_name".to_string(),
                    dep.clone(),
                    "some_reason".into(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let non_matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().never().return_once(|dep| {
                Ok(Evaluation::Pass(
                    "some_policy_name2".to_string(),
                    dep.clone(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(non_matching_policies, Some("bar")).unwrap(),
            ExecutionConfig::new(default_policies, None).unwrap(),
        ];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).unwrap();

        evaluation.should(consist_of(&[Evaluation::Fail(
            "some_policy_name".to_string(),
            dependency(),
            "some_reason".into(),
        )]));
    }

    #[test]
    fn if_the_dependency_matches_it_is_evaluated_with_the_specified_policy_regardless_of_the_default_ones(
    ) {
        let default_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().never().return_once(|dep| {
                Ok(Evaluation::Pass(
                    "some_policy_name".to_string(),
                    dep.clone(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Pass(
                    "some_policy_name2".to_string(),
                    dep.clone(),
                ))
            });
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(matching_policies, Some("foo")).unwrap(),
            ExecutionConfig::new(default_policies, None).unwrap(),
        ];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).unwrap();

        evaluation.should(consist_of(&[Evaluation::Pass(
            "some_policy_name2".to_string(),
            dependency(),
        )]));
    }

    fn dependency() -> Dependency {
        Dependency {
            name: "foo".to_string(),
            version: "1.0.0".to_string(),
            repository: Repository::GitHub {
                organization: "some_org".to_string(),
                name: "some_name".to_string(),
            },
            latest_version: Some("1.0.1".to_string()),
        }
    }

    fn mock_policy() -> Box<MockPolicy> {
        Box::new(MockPolicy::new())
    }
}
