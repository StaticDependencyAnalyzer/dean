use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use futures::Stream;
use itertools::Itertools;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;

use crate::pkg::{DependencyRetriever, InfoRetriever, Repository};
use crate::{Dependency, Result};

pub struct DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin,
{
    npm_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
}

impl<T> DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin,
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

#[async_trait]
impl<T> DependencyRetriever for DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    type Itr = Box<dyn Stream<Item = Dependency> + Unpin + Send>;

    async fn dependencies(&self) -> Result<Self::Itr> {
        let content = self.content_from_reader().await?;

        let not_comment_lines = content.lines().filter(|line| !line.trim().starts_with('#'));

        let dependency_lines_grouped = not_comment_lines.group_by(|line| line.trim().is_empty());
        let dependency_lines_grouped =
            dependency_lines_grouped
                .into_iter()
                .filter_map(|(bool, group)| {
                    if bool {
                        None
                    } else {
                        Some(group.collect_vec())
                    }
                });

        let dependency_info_tuples = dependency_lines_grouped
            .map(|lines| {
                let dependency_line: String = lines
                    .first()
                    .context("there was no first line")?
                    .replace('\"', "");
                let mut dependency_name = dependency_line
                    .split_once('@')
                    .context("unable to split dependency line by @")?
                    .0
                    .to_owned();
                if dependency_name.is_empty() {
                    dependency_name = format!(
                        "@{}",
                        dependency_line
                            .replacen('@', "", 1)
                            .split_once('@')
                            .context("unable to split the dependency line by @")?
                            .0
                    );
                }

                let dependency_version = lines.get(1).context("no lines to retrieve")?;
                let dependency_version: String = dependency_version
                    .trim()
                    .split_once(' ')
                    .context("unable to split by space the dependency version")?
                    .1
                    .replace('\"', "");

                Ok((dependency_name, dependency_version))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter();

        let futures = dependency_info_tuples
            .map(|(name, version)| {
                let retriever = self.npm_info_retriever.clone();

                tokio::spawn(async move {
                    let (latest_version, repository) = futures::future::join(
                        retriever.latest_version(&name),
                        retriever.repository(&name),
                    )
                    .await;

                    Dependency {
                        name: name.clone(),
                        version: version.clone(),
                        latest_version: latest_version.ok(),
                        repository: repository.unwrap_or(Repository::Unknown),
                    }
                })
            })
            .collect_vec();

        let unfold =
            futures::stream::unfold(futures, |mut name_and_versions_to_retrieve| async move {
                let next = name_and_versions_to_retrieve.pop();
                let dependency = next?.await.ok()?;
                Some((dependency, name_and_versions_to_retrieve))
            });
        Ok(Box::new(Box::pin(unfold)))
    }
}

impl<T> DependencyReader<T>
where
    T: Unpin + tokio::io::AsyncRead + Send,
{
    async fn content_from_reader(&self) -> Result<String> {
        let mut bytes = Vec::new();
        self.reader.lock().await.read_to_end(&mut bytes).await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::pkg::{MockInfoRetriever, Repository};

    #[tokio::test]
    async fn it_retrieves_all_the_dependencies() {
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
                .returning(|_| Ok(Repository::Unknown));
            retriever
                .expect_latest_version()
                .returning(|_| Ok("1.0.0".into()));
            retriever
        };

        let dependency_reader = DependencyReader::new(yarn_lock_file(), retriever);
        let dependencies = dependency_reader.dependencies().await;

        let deps = dependencies.unwrap().collect::<Vec<_>>().await;
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
        include_bytes!("../../../tests/fixtures/yarn.lock")
    }
}
