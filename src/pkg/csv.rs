use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use itertools::Itertools;

use crate::pkg::ResultReporter;
use crate::Evaluation;

pub struct Reporter<T>
where
    T: Write,
{
    writer: Rc<RefCell<T>>,
}

impl<T> Reporter<T>
where
    T: Write,
{
    pub fn new(writer: Rc<RefCell<T>>) -> Self {
        Self { writer }
    }

    fn headers<'a>(policies: &[&'a str]) -> Vec<&'a str> {
        let mut headers = ["name", "version", "latest_version", "repository", "score"].to_vec();
        headers.extend_from_slice(policies);
        headers
    }
}

impl<F> ResultReporter for Reporter<F>
where
    F: Write,
{
    fn report_results<T>(&mut self, result: T) -> Result<(), String>
    where
        T: IntoIterator<Item = Evaluation>,
    {
        let wtr = &mut *self.writer.borrow_mut();
        let mut writer = csv::Writer::from_writer(wtr);

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
            .map_err(|e| format!("unable to write record: {}", e))?;

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
                        Evaluation::Pass(_, _) => {
                            row.push("OK".to_string());
                        }
                        Evaluation::Fail(_, _, reason, _) => {
                            row.push(reason.clone());
                        }
                    }
                } else {
                    row.push("Not evaluated".to_string());
                }
            }

            writer
                .write_record(row)
                .map_err(|e| format!("unable to write record: {}", e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::Cursor;
    use std::rc::Rc;

    use super::*;
    use crate::pkg::Repository::GitHub;
    use crate::{Dependency, Evaluation};

    #[test]
    fn it_reports_to_csv_the_results() {
        let buffer = Rc::new(RefCell::new(Cursor::new(Vec::new())));
        let mut reporter = Reporter::new(buffer.clone());

        let evaluations = vec![
            Evaluation::Pass(
                "policy1".to_string(),
                Dependency {
                    name: "some_dep1".to_string(),
                    version: "1.2.3".to_string(),
                    latest_version: Some("1.2.3".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
            ),
            Evaluation::Fail(
                "policy1".to_string(),
                Dependency {
                    name: "some_dep2".to_string(),
                    version: "2.3.4".to_string(),
                    latest_version: Some("2.4.5".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
                "failed because a reason".into(),
                1.5,
            ),
            Evaluation::Fail(
                "policy2".to_string(),
                Dependency {
                    name: "some_dep2".to_string(),
                    version: "2.3.4".to_string(),
                    latest_version: Some("2.4.5".to_string()),
                    repository: GitHub {
                        organization: "some_org".to_string(),
                        name: "some_repo".to_string(),
                    },
                },
                "failed because a reason".into(),
                1.0,
            ),
        ];

        reporter.report_results(evaluations).unwrap();

        assert_eq!(
            String::from_utf8_lossy(buffer.borrow().get_ref()),
            r#"name,version,latest_version,repository,score,policy1,policy2
some_dep1,1.2.3,1.2.3,https://github.com/some_org/some_repo,0,OK,Not evaluated
some_dep2,2.3.4,2.4.5,https://github.com/some_org/some_repo,2.5,failed because a reason,failed because a reason
"#
        );
    }
}
