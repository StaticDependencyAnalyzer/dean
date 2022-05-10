use std::sync::{Arc, RwLock};

use log::info;
use rayon::prelude::*;
use serde_json::Value;
use serde_json::Value::Object;

use crate::pkg::Repository;

#[cfg_attr(test, mockall::automock)]
pub trait InfoRetriever {
    fn latest_version(&self, dependency: &str) -> Result<String, String>;
    fn repository(&self, dependency: &str) -> Result<Repository, String>;
}

pub struct DependencyReader {
    npm_info_retriever: Arc<RwLock<Box<dyn InfoRetriever + Send + Sync>>>,
}

#[cfg_attr(test, derive(Clone, PartialEq, Debug, Default))]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub latest_version: Option<String>,
    pub repository: Repository,
}

impl DependencyReader {
    pub fn retrieve_from_reader<T>(&self, reader: T) -> Result<Vec<Dependency>, String>
    where
        T: std::io::Read,
    {
        let result: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
        if let Object(dependencies) = &result["dependencies"] {
            let deps: Vec<_> = dependencies
                .into_iter()
                .map(|(name, value)| (name, value["version"].as_str().map(ToString::to_string)))
                .collect();

            deps.into_par_iter()
                .map(|(name, version)| {
                    let retriever = self.npm_info_retriever.clone();
                    if let Some(version) = version {
                        info!(
                            target: "npm-dependency-retriever",
                            "retrieving information for dependency [name={}, version={}]",
                            name, &version
                        );
                        Ok(Dependency {
                            name: name.into(),
                            version,
                            latest_version: retriever.read().unwrap().latest_version(name).ok(),
                            repository: retriever
                                .read()
                                .unwrap()
                                .repository(name)
                                .unwrap_or(Repository::Unknown),
                        })
                    } else {
                        Err("version not found in map".to_string())
                    }
                })
                .collect()
        } else {
            Err("dependencies not found in lock file".into())
        }
    }
}

impl DependencyReader {
    pub fn new(retriever: Box<dyn InfoRetriever + Send + Sync>) -> Self {
        DependencyReader {
            npm_info_retriever: Arc::new(RwLock::new(retriever)),
        }
    }
}

#[cfg(test)]
mod tests {
    use expects::{
        matcher::{be_ok, consist_of},
        Subject,
    };
    use mockall::predicate::eq;

    use super::*;

    #[test]
    fn retrieves_all_dependencies() {
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
        let dependency_reader = DependencyReader::new(retriever);

        let dependencies = dependency_reader.retrieve_from_reader(npm_package_lock().as_bytes());

        dependencies.should(be_ok(consist_of(&[Dependency {
            name: "colors".into(),
            version: "1.4.0".into(),
            latest_version: Some("1.4.1".into()),
            repository: Repository::GitHub {
                organization: "org".into(),
                name: "name".into(),
            },
        }])));
    }

    fn npm_package_lock() -> String {
        String::from(
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
}"#,
        )
    }
}
