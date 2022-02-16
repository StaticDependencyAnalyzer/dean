use serde_json::Value;
use serde_json::Value::Object;

#[derive(Default)]
pub struct NpmDependencyRetriever {}

#[derive(Clone, PartialEq, Debug)]
pub struct NpmDependency {
    pub name: String,
    pub version: String,
}

impl NpmDependencyRetriever {
    pub fn retrieve_from_reader<T>(&self, reader: T) -> Result<Vec<NpmDependency>, String>
    where
        T: std::io::Read,
    {
        let result: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
        if let Object(dependencies) = &result["dependencies"] {
            dependencies
                .iter()
                .map(|(key, value)| {
                    if let Some(version) = value["version"].as_str() {
                        Ok(NpmDependency {
                            name: key.into(),
                            version: version.to_string(),
                        })
                    } else {
                        Err("version not found in map".to_string())
                    }
                })
                .collect()
        } else {
            Err("dependencies not found".into())
        }
    }
}

impl NpmDependencyRetriever {
    pub fn new() -> Self {
        NpmDependencyRetriever {}
    }
}
