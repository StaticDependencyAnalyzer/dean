use std::collections::VecDeque;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures_util::FutureExt;
use log::error;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use toml::Value;

use crate::pkg::Repository;
use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever};

pub struct DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    cargo_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
}

pub struct DependencyStream {
    pub(crate) name_and_version_iter: VecDeque<(String, String)>,
    pub(crate) next_name_and_version: Option<(String, String)>,

    pub(crate) retriever: Arc<dyn InfoRetriever>,
    pub(crate) latest_version_retrieved: Option<Result<String, String>>,
    pub(crate) latest_repository_retrieved: Option<Result<Repository, String>>,
}

impl Stream for DependencyStream {
    type Item = Dependency;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if self.as_ref().next_name_and_version.is_some() {
            let mut next_name_and_version = self.as_mut().name_and_version_iter.pop_front();
            if next_name_and_version.is_none() {
                return Poll::Ready(None);
            }

            self.as_mut().next_name_and_version =
                Some(next_name_and_version.take().unwrap());
        }
        let (name, version) = self.as_ref().next_name_and_version.clone().unwrap();

        if self.as_ref().latest_version_retrieved.is_none() {
            if let Some(result) = self.as_mut().retriever.latest_version(&name).now_or_never() {
                self.as_mut().latest_version_retrieved = Some(result);
            }
        }

        if self.as_ref().latest_repository_retrieved.is_none() {
            if let Some(result) = self.as_mut().retriever.repository(&name).now_or_never() {
                self.as_mut().latest_repository_retrieved = Some(result);
            }
        }

        if self.as_ref().latest_version_retrieved.is_some()
            && self.as_ref().latest_repository_retrieved.is_some()
        {
            let latest_version = self.as_mut().latest_version_retrieved.take().unwrap();
            let latest_repository = self.as_mut().latest_repository_retrieved.take().unwrap();
            self.as_mut().next_name_and_version = None;

            if latest_version.is_err() {
                error!("Failed to retrieve latest version for dependency {}", name);
                return Poll::Ready(None);
            }
            if latest_repository.is_err() {
                error!(
                    "Failed to retrieve latest repository for dependency {}",
                    name
                );
                return Poll::Ready(None);
            }

            return Poll::Ready(Some(Dependency {
                name,
                version,
                latest_version: Some(latest_version.unwrap()),
                repository: latest_repository.unwrap(),
            }));
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[async_trait]
impl<T> DependencyRetriever for DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    type Itr = Box<dyn Stream<Item = Dependency> + Unpin + Send>;
    async fn dependencies(&self) -> Result<Self::Itr, String> {
        let contents = self.contents_from_reader().await?;
        let result: Value = toml::from_slice(&contents).map_err(|e| e.to_string())?;

        let packages = result
            .get("package")
            .ok_or_else(|| "No package section found".to_string())?
            .clone();

        let package_list = packages
            .as_array()
            .ok_or_else(|| "Packages section is not an array".to_string())?
            .clone();

        let name_and_version_from_packages = package_list
            .into_iter()
            .map(|package| {
                let name = package
                    .get("name")
                    .ok_or_else(|| "no name found".to_string())?
                    .as_str()
                    .ok_or_else(|| "name is not a string".to_string())?
                    .to_string();

                let version = package
                    .get("version")
                    .ok_or_else(|| "no version found".to_string())?
                    .as_str()
                    .ok_or_else(|| "version is not a string".to_string())?
                    .to_string();

                Ok((name, version))
            })
            .into_iter()
            .filter_map(|result: Result<(String, String), String>| {
                result.map_err(|e| error!("{}", e)).ok()
            });

        let dependencies = DependencyStream {
            name_and_version_iter: name_and_version_from_packages.collect(),
            next_name_and_version: None,
            retriever: self.cargo_info_retriever.clone(),
            latest_version_retrieved: None,
            latest_repository_retrieved: None,
        };
        Ok(Box::new(dependencies))
    }
}

impl<T> DependencyReader<T>
where
    T: Unpin + tokio::io::AsyncRead + Send,
{
    async fn contents_from_reader(&self) -> Result<Vec<u8>, String> {
        let mut contents = Vec::new();
        self.reader
            .lock()
            .await
            .read_to_end(&mut contents)
            .await
            .map_err(|e| e.to_string())?;
        Ok(contents)
    }
}

impl<T> DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    pub fn new<R>(reader: T, retriever: R) -> Self
    where
        R: Into<Arc<dyn InfoRetriever>>,
    {
        Self {
            reader: reader.into(),
            cargo_info_retriever: retriever.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::pkg::{MockInfoRetriever, Repository};
    use crate::Dependency;

    #[tokio::test]
    async fn retrieves_all_dependencies_from_cargo() {
        let retriever = {
            let mut retriever = Box::new(MockInfoRetriever::new());
            retriever
                .expect_latest_version()
                .with(eq("serde"))
                .return_once(|_| Ok("1.0.138".into()))
                .times(1);
            retriever
                .expect_repository()
                .with(eq("serde"))
                .return_once(|_| {
                    Ok(Repository::GitHub {
                        organization: "serde-rs".into(),
                        name: "serde".into(),
                    })
                })
                .times(1);
            retriever as Box<dyn InfoRetriever>
        };

        let dependency_reader = DependencyReader::new(cargo_lock_file_contents(), retriever);
        let mut dependencies = dependency_reader.dependencies().await.unwrap();

        assert_eq!(
            dependencies.next().await.unwrap(),
            Dependency {
                name: "serde".into(),
                version: "1.0.137".into(),
                latest_version: Some("1.0.138".into()),
                repository: Repository::GitHub {
                    organization: "serde-rs".into(),
                    name: "serde".into(),
                },
            }
        );
    }

    fn cargo_lock_file_contents() -> &'static [u8] {
        r#"# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "serde"
version = "1.0.137"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "61ea8d54c77f8315140a05f4c7237403bf38b72704d031543aa1d16abbf517d1"
dependencies = [
 "serde_derive",
]
"#
        .as_bytes()
    }
}
