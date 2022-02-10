use serde_json::Value;
use serde_json::Value::Object;

#[derive(Default)]
pub struct NpmDependencyRetriever {}

#[derive(Clone, PartialEq, Debug)]
pub struct NpmDependency {
    pub name: String,
}

impl NpmDependencyRetriever {
    pub fn retrieve_from_reader<T>(&self, reader: T) -> Result<Vec<NpmDependency>, String>
    where
        T: std::io::Read,
    {
        let result: Value = serde_json::from_reader(reader).map_err(|e| e.to_string())?;
        if let Object(dependencies) = &result["dependencies"] {
            Ok(dependencies
                .keys()
                .map(|key| NpmDependency { name: key.into() })
                .collect())
        } else {
            Err("nono".into())
        }
    }
}

impl NpmDependencyRetriever {
    pub fn new() -> Self {
        NpmDependencyRetriever {}
    }
}
