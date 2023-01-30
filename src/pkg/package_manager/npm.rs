use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use futures::Stream;
use itertools::Itertools;
use log::error;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Mutex;

use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever, Repository};
use crate::Result;

pub struct DependencyReader<T>
where
    T: AsyncRead + Unpin + Send,
{
    npm_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
}

#[async_trait]
impl<T> DependencyRetriever for DependencyReader<T>
where
    T: AsyncRead + Unpin + Send,
{
    type Itr = Box<dyn Stream<Item = Dependency> + Unpin + Send>;
    async fn dependencies(&self) -> Result<Self::Itr> {
        let content = {
            let mut content = String::new();
            self.reader
                .lock()
                .await
                .read_to_string(&mut content)
                .await
                .context("unable to read contents from reader")?;
            content
        };
        let result: Value =
            serde_json::from_str(&content).context("unable to retrieve json from string")?;

        let value = result["dependencies"].clone();
        if !value.is_object() {
            return Err(anyhow!("dependencies not found in lock file"));
        }

        let dependencies = value
            .as_object()
            .context("unable to extract dependency as object")?
            .clone();

        let deps = dependencies
            .into_iter()
            .map(|(name, value)| (name, value["version"].as_str().map(ToString::to_string)))
            .filter_map(|(name, version)| {
                if let Some(version) = version {
                    Some((name, version))
                } else {
                    error!("no version found for dependency {}", &name);
                    None
                }
            });

        let futures = deps
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
    T: AsyncRead + Unpin + Send,
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

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use mockall::predicate::eq;

    use super::*;
    use crate::pkg::{MockInfoRetriever, Repository};
    use crate::Dependency;

    #[tokio::test]
    async fn retrieves_all_dependencies() {
        let retriever = {
            let mut retriever = Box::new(MockInfoRetriever::new());
            retriever
                .expect_latest_version()
                .with(eq("colors"))
                .return_once(|_| Ok("1.4.1".into()))
                .times(1);
            retriever
                .expect_repository()
                .with(eq("colors"))
                .return_once(|_| {
                    Ok(Repository::GitHub {
                        organization: "org".into(),
                        name: "name".into(),
                    })
                })
                .times(1);
            retriever as Box<dyn InfoRetriever>
        };

        let dependency_reader = DependencyReader::new(npm_package_lock(), retriever);
        let dependencies = dependency_reader.dependencies().await;

        assert_eq!(
            dependencies.unwrap().next().await.unwrap(),
            Dependency {
                name: "colors".into(),
                version: "1.4.0".into(),
                latest_version: Some("1.4.1".into()),
                repository: Repository::GitHub {
                    organization: "org".into(),
                    name: "name".into(),
                },
            }
        );
    }

    fn npm_package_lock() -> &'static [u8] {
        r#"{
  "name": "foo",
  "version": "1.0.0",
  "lockfileVersion": 2,
  "requires": true,
  "packages": {
    "": {
      "name": "foo",
      "version": "1.0.0",
      "license": "ISC",
      "dependencies": {
        "colors": "^1.4.0",
        "faker": "^5.5.3"
      }
    },
    "node_modules/colors": {
      "version": "1.4.0",
      "resolved": "https://registry.npmjs.org/colors/-/colors-1.4.0.tgz",
      "integrity": "sha512-a+UqTh4kgZg/SlGvfbzDHpgRu7AAQOmmqRHJnxhRZICKFUT91brVhNNt58CMWU9PsBbv3PDCZUHbVxuDiH2mtA==",
      "engines": {
        "node": ">=0.1.90"
      }
    }
  },
  "dependencies": {
    "colors": {
      "version": "1.4.0",
      "resolved": "https://registry.npmjs.org/colors/-/colors-1.4.0.tgz",
      "integrity": "sha512-a+UqTh4kgZg/SlGvfbzDHpgRu7AAQOmmqRHJnxhRZICKFUT91brVhNNt58CMWU9PsBbv3PDCZUHbVxuDiH2mtA=="
    }
  }
}"#.as_bytes()
    }
}
