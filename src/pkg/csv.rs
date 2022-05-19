use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

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

        writer
            .write_record(&[
                "name",
                "version",
                "latest_version",
                "repository",
                "evaluation",
            ])
            .map_err(|e| format!("unable to write record: {}", e))?;

        for evaluation in result {
            let record = match evaluation {
                Evaluation::Pass(dep) => [
                    dep.name,
                    dep.version,
                    dep.latest_version.unwrap_or_else(|| "unknown".to_string()),
                    dep.repository.to_string(),
                    "OK".to_string(),
                ],
                Evaluation::Fail(dep, reason) => [
                    dep.name,
                    dep.version,
                    dep.latest_version.unwrap_or_else(|| "unknown".to_string()),
                    dep.repository.to_string(),
                    reason,
                ],
            };

            writer
                .write_record(record)
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
            Evaluation::Pass(Dependency {
                name: "some_dep1".to_string(),
                version: "1.2.3".to_string(),
                latest_version: Some("1.2.3".to_string()),
                repository: GitHub {
                    organization: "some_org".to_string(),
                    name: "some_repo".to_string(),
                },
            }),
            Evaluation::Fail(
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
            ),
        ];

        reporter.report_results(evaluations).unwrap();

        assert_eq!(
            String::from_utf8_lossy(buffer.borrow().get_ref()),
            r#"name,version,latest_version,repository,evaluation
some_dep1,1.2.3,1.2.3,https://github.com/some_org/some_repo,OK
some_dep2,2.3.4,2.4.5,https://github.com/some_org/some_repo,failed because a reason
"#
        );
    }
}
