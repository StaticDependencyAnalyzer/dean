use std::sync::{Arc, Mutex};

use log::info;
use rayon::prelude::*;
use serde_json::Value;

use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever, Repository};

pub struct DependencyReader<T>
where
    T: std::io::Read,
{
    npm_info_retriever: Arc<dyn InfoRetriever + Send + Sync>,
    reader: Mutex<T>,
}

impl<T> DependencyRetriever for DependencyReader<T>
where
    T: std::io::Read,
{
    fn dependencies(&self) -> Result<Vec<Dependency>, String> {
        let result: Value = serde_json::from_reader(&mut *self.reader.lock().unwrap())
            .map_err(|e| e.to_string())?;

        let value = &result["dependencies"];
        if !value.is_object() {
            return Err("dependencies not found in lock file".into());
        }

        let dependencies = value.as_object().unwrap();

        let deps = dependencies
            .into_iter()
            .map(|(name, value)| (name, value["version"].as_str().map(ToString::to_string)));

        let npm_info_retriever = self.npm_info_retriever.clone();
        deps.par_bridge()
            .map(|(name, version)| {
                let retriever = npm_info_retriever.clone();
                if let Some(version) = version {
                    info!(
                        target: "npm-dependency-retriever",
                        "retrieving information for dependency [name={}, version={}]",
                        name, &version
                    );
                    Ok(Dependency {
                        name: name.into(),
                        version,
                        latest_version: retriever.latest_version(name).ok(),
                        repository: retriever.repository(name).unwrap_or(Repository::Unknown),
                    })
                } else {
                    Err("version not found in map".to_string())
                }
            })
            .collect()
    }
}

impl<T> DependencyReader<T>
where
    T: std::io::Read,
{
    pub fn new(reader: T, retriever: Box<dyn InfoRetriever + Send + Sync>) -> Self {
        Self {
            reader: reader.into(),
            npm_info_retriever: retriever.into(),
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
    use crate::pkg::{MockInfoRetriever, Repository};

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

        let dependency_reader = DependencyReader::new(npm_package_lock(), retriever);
        let dependencies = dependency_reader.dependencies();

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
