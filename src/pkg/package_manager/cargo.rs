use std::sync::Arc;

use async_trait::async_trait;
use log::error;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use toml::Value;

use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever, Repository};

pub struct DependencyReader<T>
where
    T: tokio::io::AsyncRead + Unpin + Send,
{
    cargo_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
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

        struct StreamStatus {
            name_and_versions_to_retrieve: Vec<(String, String)>,
            retriever : Arc<dyn InfoRetriever>,
        }

        let status = StreamStatus {
            name_and_versions_to_retrieve: name_and_version_from_packages.collect(),
            retriever: self.cargo_info_retriever.clone(),
        };

        let unfold = futures::stream::unfold(status, |mut status| async move {
            if let Some((name, version)) = status.name_and_versions_to_retrieve.pop() {
                let dependency = Dependency {
                    name: name.clone(),
                    version: version.clone(),
                    latest_version: status.retriever.latest_version(&name).await.ok(),
                    repository: status.retriever.repository(&name).await.unwrap_or(Repository::Unknown),
                };
                Some((dependency, status))
            } else {
                None
            }
        });
        Ok(Box::new(Box::pin(unfold)))
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
