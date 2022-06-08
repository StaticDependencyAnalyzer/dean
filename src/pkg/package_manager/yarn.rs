use std::sync::{Arc, Mutex};

use itertools::Itertools;

use crate::pkg::{DependencyRetriever, InfoRetriever, Repository};
use crate::Dependency;

pub struct DependencyReader<T>
where
    T: std::io::Read + Send,
{
    npm_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
}

impl<T> DependencyReader<T>
where
    T: std::io::Read + Send,
{
    pub fn new<R>(reader: T, retriever: R) -> Self
    where
        R: Into<Arc<dyn InfoRetriever>>,
    {
        Self {
            reader: reader.into(),
            npm_info_retriever: retriever.into(),
        }
    }
}

impl<T> DependencyRetriever for DependencyReader<T>
where
    T: std::io::Read + Send,
{
    type Itr = Box<dyn Iterator<Item = Dependency> + Send>;

    fn dependencies(&self) -> Result<Self::Itr, String> {
        let lines = {
            let mut bytes = Vec::new();
            self.reader
                .lock()
                .unwrap()
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let str = String::from_utf8_lossy(&bytes).into_owned();
            str.lines()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        };

        let not_comment_lines = lines.iter().filter(|line| !line.trim().starts_with('#'));

        let dependency_lines_grouped = {
            let dependencies_grouped = not_comment_lines.group_by(|line| line.trim().is_empty());
            let dependency_groups = dependencies_grouped
                .into_iter()
                .filter_map(|(bool, group)| {
                    if bool {
                        None
                    } else {
                        let x: Vec<&String> = group.collect();
                        Some(x)
                    }
                });

            dependency_groups.collect::<Vec<_>>()
        };

        let dependencies = dependency_lines_grouped.into_iter().map(|lines| {
            let dependency_line: String = lines.get(0).unwrap().replace('\"', "");
            let mut dependency_name = dependency_line.split_once('@').unwrap().0.to_owned();
            if dependency_name.is_empty() {
                dependency_name = format!(
                    "@{}",
                    dependency_line
                        .replacen('@', "", 1)
                        .split_once('@')
                        .unwrap()
                        .0
                );
            }

            let dependency_version = lines.get(1).unwrap();
            let dependency_version = dependency_version
                .trim()
                .split_once(' ')
                .unwrap()
                .1
                .replace('\"', "");

            Dependency {
                latest_version: self
                    .npm_info_retriever
                    .latest_version(&dependency_name)
                    .ok(),
                repository: self
                    .npm_info_retriever
                    .repository(&dependency_name)
                    .unwrap_or(Repository::Unknown),
                name: dependency_name,
                version: dependency_version,
            }
        });

        Ok(Box::new(dependencies.collect::<Vec<_>>().into_iter()))
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use super::*;
    use crate::pkg::{MockInfoRetriever, Repository};

    #[test]
    fn it_retrieves_all_the_dependencies() {
        let retriever: Box<dyn InfoRetriever> = {
            let mut retriever = Box::new(MockInfoRetriever::new());
            retriever
                .expect_repository()
                .with(eq("webpack"))
                .return_once(|_| {
                    Ok(Repository::GitHub {
                        organization: "webpack".into(),
                        name: "webpack".into(),
                    })
                });
            retriever
                .expect_latest_version()
                .with(eq("webpack"))
                .return_once(|_| Ok("5.73.1".into()));
            retriever
                .expect_repository()
                .return_const(Ok(Repository::Unknown));
            retriever
                .expect_latest_version()
                .return_const(Ok("1.0.0".into()));
            retriever
        };

        let dependency_reader = DependencyReader::new(yarn_lock_file(), retriever);
        let dependencies = dependency_reader.dependencies();

        let deps = dependencies.unwrap().collect::<Vec<_>>();
        let webpack_dependency = deps.iter().find(|dep| dep.name == "webpack").unwrap();
        let gen_mapping_dependency = deps
            .iter()
            .find(|dep| dep.name == "@jridgewell/gen-mapping")
            .unwrap();

        assert_eq!(deps.len(), 76);
        assert_eq!(
            webpack_dependency,
            &Dependency {
                name: "webpack".to_string(),
                version: "5.73.0".to_string(),
                latest_version: Some("5.73.1".to_string()),
                repository: Repository::GitHub {
                    organization: "webpack".to_string(),
                    name: "webpack".to_string(),
                },
            }
        );
        assert_eq!(
            gen_mapping_dependency,
            &Dependency {
                name: "@jridgewell/gen-mapping".to_string(),
                version: "0.3.1".to_string(),
                latest_version: Some("1.0.0".to_string()),
                repository: Repository::Unknown,
            }
        );
    }

    fn yarn_lock_file() -> &'static [u8] {
        return include_bytes!("../../../tests/fixtures/yarn.lock");
    }
}
