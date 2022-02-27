use serde_json::Value;
use serde_json::Value::Object;

pub trait InfoRetriever {
    fn latest_version(&self, package_name: &str) -> Result<String, String>;
}

pub struct DependencyReader {
    npm_info_retriever: Box<dyn InfoRetriever>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub latest_version: String,
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
        let latest_version = self.npm_info_retriever.latest_version(name)?;
        Ok(Dependency {
            name: name.into(),
            version: version.into(),
            latest_version,
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
