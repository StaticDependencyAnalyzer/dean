use serde_json::Value;
use serde_json::Value::Object;

#[derive(Clone, PartialEq, Debug)]
pub enum Repository {
    GitHub { organization: String, name: String },
}

#[cfg_attr(test, mockall::automock)]
pub trait InfoRetriever {
    fn latest_version(&self, package_name: &str) -> Result<String, String>;
    fn repository(&self, package_name: &str) -> Result<Repository, String>;
}

pub struct DependencyReader {
    npm_info_retriever: Box<dyn InfoRetriever>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub latest_version: String,
    pub repository: Repository,
}

impl DependencyReader {
    pub fn retrieve_from_reader<T>(&self, reader: T) -> Result<Vec<Dependency>, String>
    where
        T: std::io::Read,
    {
        let result: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
        if let Object(dependencies) = &result["dependencies"] {
            dependencies
                .iter()
                .map(|(name, value)| {
                    if let Some(version) = value["version"].as_str() {
                        self.get_dependency_info(name, version)
                    } else {
                        Err("version not found in map".to_string())
                    }
                })
                .collect()
        } else {
            Err("dependencies not found".into())
        }
    }

    fn get_dependency_info(&self, name: &str, version: &str) -> Result<Dependency, String> {
        Ok(Dependency {
            name: name.into(),
            version: version.into(),
            latest_version: self.npm_info_retriever.latest_version(name)?,
            repository: self.npm_info_retriever.repository(name)?,
        })
    }
}

impl DependencyReader {
    pub fn new(retriever: Box<dyn InfoRetriever>) -> Self {
        DependencyReader {
            npm_info_retriever: retriever,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expects::{equal::be_ok, iter::consist_of, Subject};
    use mockall::predicate::eq;

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
            latest_version: "1.4.1".into(),
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
