use std::sync::{Arc, Mutex};

use log::{error, info};
use toml::Value;

use crate::pkg::Repository;
use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever};

pub struct DependencyReader<T>
where
    T: std::io::Read + Send,
{
    cargo_info_retriever: Arc<dyn InfoRetriever>,
    reader: Mutex<T>,
}

impl<T> DependencyRetriever for DependencyReader<T>
where
    T: std::io::Read + Send,
{
    type Itr = Box<dyn Iterator<Item = Dependency> + Send>;
    fn dependencies(&self) -> Result<Self::Itr, String> {
        let mut contents = Vec::new();
        self.reader
            .lock()
            .unwrap()
            .read_to_end(&mut contents)
            .map_err(|e| e.to_string())?;
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

        let info_retriever = self.cargo_info_retriever.clone();
        let dependencies = name_and_version_from_packages.map(move |(name, version)| {
            info!(
                target: "dean::cargo-dependency-retriever",
                "retrieving information for dependency [name={}, version={}]",
                name, &version
            );

            let retriever = info_retriever.clone();
            Dependency {
                latest_version: retriever.latest_version(&name).ok(),
                repository: retriever.repository(&name).unwrap_or(Repository::Unknown),
                name,
                version,
            }
        });
        Ok(Box::new(dependencies))
    }
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
            cargo_info_retriever: retriever.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use super::*;
    use crate::pkg::{MockInfoRetriever, Repository};
    use crate::Dependency;

    #[test]
    fn retrieves_all_dependencies_from_cargo() {
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
        let mut dependencies = dependency_reader.dependencies().unwrap();

        assert_eq!(
            dependencies.next().unwrap(),
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
