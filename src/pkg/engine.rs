use std::cmp::Ordering;
use std::sync::Arc;

use futures::future::join_all;
use itertools::Itertools;

use crate::{Dependency, Evaluation, Policy, Result};

pub struct ExecutionConfig {
    regex: Option<regex::Regex>,
    policies: Vec<Arc<dyn Policy>>,
}

impl ExecutionConfig {
    pub fn new(
        policies: Vec<Box<dyn Policy>>,
        dependency_name_regex: Option<&str>,
    ) -> Result<Self> {
        Ok(Self {
            regex: match dependency_name_regex {
                Some(regex) => Some(regex::Regex::new(regex)?),
                None => None,
            },
            policies: policies.into_iter().map(std::convert::Into::into).collect(),
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

impl PolicyExecutor {
    pub fn new(execution_configs: Vec<ExecutionConfig>) -> Self {
        Self {
            execution_configs: execution_configs
                .into_iter()
                .sorted_by(|a, b| some_options_first(&a.regex, &b.regex))
                .collect(),
        }
    }

    pub async fn evaluate(&self, dependency: &Dependency) -> Result<Vec<Evaluation>> {
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
                let policy = policy.clone();
                let dependency = dependency.clone();
                evaluations.push(tokio::spawn(
                    async move { policy.evaluate(&dependency).await },
                ));
            }
        }

        let evaluations_resolved = join_all(evaluations).await;
        let mut evaluations = vec![];
        for evaluation in &evaluations_resolved {
            evaluations.push(evaluation.as_ref().unwrap().as_ref().unwrap().clone());
        }

        Ok(evaluations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::policy::MockPolicy;
    use crate::pkg::Repository;
    use crate::{Dependency, Evaluation, Policy};

    #[tokio::test]
    async fn it_executes_all_policies_for_a_dependency_if_they_pass() {
        let policies = vec![
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass {
                        policy_name: "some_policy_name".to_string(),
                        dependency: dep.clone(),
                    })
                });
                policy as Box<dyn Policy>
            },
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass {
                        policy_name: "some_policy_name2".to_string(),
                        dependency: dep.clone(),
                    })
                });
                policy as Box<dyn Policy>
            },
        ];
        let config = vec![ExecutionConfig::new(policies, None).unwrap()];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).await.unwrap();

        assert_eq!(
            evaluation,
            &[
                Evaluation::Pass {
                    policy_name: "some_policy_name".to_string(),
                    dependency: dependency()
                },
                Evaluation::Pass {
                    policy_name: "some_policy_name2".to_string(),
                    dependency: dependency()
                },
            ]
        );
    }

    #[tokio::test]
    async fn it_executes_both_policies_even_if_one_fails() {
        let policies = vec![
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Fail {
                        policy_name: "some_policy_name".to_string(),
                        dependency: dep.clone(),
                        reason: "some_reason".into(),
                        fail_score: 1.0,
                    })
                });
                policy as Box<dyn Policy>
            },
            {
                let mut policy = mock_policy();
                policy.expect_evaluate().once().return_once(|dep| {
                    Ok(Evaluation::Pass {
                        policy_name: "some_policy_name2".to_string(),
                        dependency: dep.clone(),
                    })
                });
                policy as Box<dyn Policy>
            },
        ];
        let config = vec![ExecutionConfig::new(policies, None).unwrap()];

        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).await.unwrap();

        assert_eq!(evaluation.len(), 2);
        match evaluation.get(0).unwrap() {
            Evaluation::Fail {
                policy_name,
                dependency: dep,
                reason,
                fail_score,
            } => {
                assert_eq!(policy_name, "some_policy_name");
                assert_eq!(dep, &dependency());
                assert_eq!(reason, "some_reason");
                assert!((fail_score - 1.0).abs() < f64::EPSILON);
            }
            Evaluation::Pass { .. } => {
                unreachable!()
            }
        }
        match evaluation.get(1).unwrap() {
            Evaluation::Pass {
                policy_name: policy,
                dependency: dep,
            } => {
                assert_eq!(policy, "some_policy_name2");
                assert_eq!(dep, &dependency());
            }
            Evaluation::Fail { .. } => {
                unreachable!()
            }
        };
    }

    #[tokio::test]
    async fn it_executes_only_the_second_policy_because_it_doesnt_match_the_first() {
        let non_matching_policies = vec![{ mock_policy() as Box<dyn Policy> }];
        let matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Pass {
                    policy_name: "some_policy_name2".to_string(),
                    dependency: dep.clone(),
                })
            });
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(matching_policies, Some("foo")).unwrap(),
            ExecutionConfig::new(non_matching_policies, Some("bar")).unwrap(),
        ];

        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).await.unwrap();

        assert_eq!(
            evaluation,
            &[Evaluation::Pass {
                policy_name: "some_policy_name2".to_string(),
                dependency: dependency()
            }]
        );
    }

    #[tokio::test]
    async fn if_the_dependency_doesnt_match_it_is_evaluated_with_the_default_policies() {
        let default_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Fail {
                    policy_name: "some_policy_name".to_string(),
                    dependency: dep.clone(),
                    reason: "some_reason".into(),
                    fail_score: 1.0,
                })
            });
            policy as Box<dyn Policy>
        }];
        let non_matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().never();
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(non_matching_policies, Some("bar")).unwrap(),
            ExecutionConfig::new(default_policies, None).unwrap(),
        ];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).await.unwrap();

        assert_eq!(evaluation.len(), 1);
        match evaluation.get(0).unwrap() {
            Evaluation::Fail {
                policy_name,
                dependency: dep,
                reason,
                fail_score,
            } => {
                assert_eq!(policy_name, "some_policy_name");
                assert_eq!(dep, &dependency());
                assert_eq!(reason, "some_reason");
                assert!((fail_score - 1.0) < f64::EPSILON);
            }
            Evaluation::Pass { .. } => {
                unreachable!()
            }
        }
    }

    #[tokio::test]
    async fn if_the_dependency_matches_it_is_evaluated_with_the_specified_policy_regardless_of_the_default_ones(
    ) {
        let default_policies = vec![{ mock_policy() as Box<dyn Policy> }];
        let matching_policies = vec![{
            let mut policy = mock_policy();
            policy.expect_evaluate().once().return_once(|dep| {
                Ok(Evaluation::Pass {
                    policy_name: "some_policy_name2".to_string(),
                    dependency: dep.clone(),
                })
            });
            policy as Box<dyn Policy>
        }];
        let config = vec![
            ExecutionConfig::new(matching_policies, Some("foo")).unwrap(),
            ExecutionConfig::new(default_policies, None).unwrap(),
        ];
        let policy_executor = PolicyExecutor::new(config);

        let evaluation = policy_executor.evaluate(&dependency()).await.unwrap();

        assert_eq!(
            evaluation,
            &[Evaluation::Pass {
                policy_name: "some_policy_name2".to_string(),
                dependency: dependency()
            }]
        );
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
