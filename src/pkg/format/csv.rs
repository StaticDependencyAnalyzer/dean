use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use itertools::Itertools;
use tokio::io::AsyncWrite;
use tokio::sync::Mutex;

use crate::pkg::ResultReporter;
use crate::{Evaluation, Result};

pub struct Reporter<T>
where
    T: AsyncWrite,
{
    writer: Arc<Mutex<T>>,
}

impl<T> Reporter<T>
where
    T: AsyncWrite,
{
    pub fn new(writer: Arc<Mutex<T>>) -> Self {
        Self { writer }
    }

    fn headers<'a>(policies: &[&'a str]) -> Vec<&'a str> {
        let mut headers = ["name", "version", "latest_version", "repository", "score"].to_vec();
        headers.extend_from_slice(policies);
        headers
    }
}

#[async_trait]
impl<F> ResultReporter for Reporter<F>
where
    F: AsyncWrite + Unpin + Send,
{
    async fn report_results<T>(&mut self, result: T) -> Result<()>
    where
        T: IntoIterator<Item = Evaluation> + Send,
    {
        let arc = self.writer.clone();
        let wtr = &mut *arc.lock().await;

        let mut writer = csv_async::AsyncWriter::from_writer(wtr);

        let evaluations: Vec<Evaluation> = result.into_iter().collect();
        let policy_names: Vec<_> = evaluations
            .iter()
            .map(Evaluation::policy)
            .unique()
            .collect();

        let dependencies = evaluations
            .iter()
            .map(Evaluation::dependency)
            .unique()
            .collect::<Vec<_>>();

        writer
            .write_record(Self::headers(&policy_names))
            .await
            .context("unable to write record")?;

        for dependency in dependencies {
            let evaluations = evaluations
                .iter()
                .filter(|e| e.dependency() == dependency)
                .collect::<Vec<_>>();

            let mut row = [
                dependency.name.to_string(),
                dependency.version.to_string(),
                dependency
                    .latest_version
                    .as_ref()
                    .unwrap_or(&"unknown".to_string())
                    .clone(),
                dependency.repository.url().unwrap_or_default().to_string(),
                evaluations
                    .iter()
                    .map(|e| e.fail_score())
                    .sum::<f64>()
                    .to_string(),
            ]
            .to_vec();

            for policy in &policy_names {
                let policy_evaluation_for_dependency =
                    evaluations.iter().find(|e| e.policy() == *policy);

                if let Some(evaluation) = policy_evaluation_for_dependency {
                    match evaluation {
                        Evaluation::Pass { .. } => {
                            row.push("OK".to_string());
                        }
                        Evaluation::Fail { reason, .. } => {
                            row.push(reason.clone());
                        }
                    }
                } else {
                    row.push("Not evaluated".to_string());
                }
            }

            writer
                .write_record(row)
                .await
                .context("unable to write record")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::pkg::Repository::GitHub;
    use crate::{Dependency, Evaluation};

    #[tokio::test]
    async fn it_reports_to_csv_the_results() {
        let buffer = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        let mut reporter = Reporter::new(buffer.clone());

        let evaluations = vec![
            Evaluation::Pass {
                policy_name: "policy1".to_string(),
                dependency: Dependency {
                    name: "some_dep1".to_string(),
                    version: "1.2.3".to_string(),
                    latest_version: Some("1.2.3".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
            },
            Evaluation::Fail {
                policy_name: "policy1".to_string(),
                dependency: Dependency {
                    name: "some_dep2".to_string(),
                    version: "2.3.4".to_string(),
                    latest_version: Some("2.4.5".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
                reason: "failed because a reason".into(),
                fail_score: 1.5,
            },
            Evaluation::Fail {
                policy_name: "policy2".to_string(),
                dependency: Dependency {
                    name: "some_dep2".to_string(),
                    version: "2.3.4".to_string(),
                    latest_version: Some("2.4.5".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
                reason: "failed because a reason".into(),
                fail_score: 1.0,
            },
        ];

        reporter.report_results(evaluations).await.unwrap();

        assert_eq!(
            String::from_utf8_lossy(buffer.lock().await.get_ref()),
            r#"name,version,latest_version,repository,score,policy1,policy2
some_dep1,1.2.3,1.2.3,https://github.com/some_org/some_repo,0,OK,Not evaluated
some_dep2,2.3.4,2.4.5,https://github.com/some_org/some_repo,2.5,failed because a reason,failed because a reason
"#
        );
    }
}
