use serde::de::DeserializeOwned;

#[derive(Debug)]
pub struct Response {
    inner: reqwest::blocking::Response,
}

impl Response {
    pub fn json<T: DeserializeOwned>(self) -> Result<T, String> {
        self.inner.json().map_err(|err| err.to_string())
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Client {}

impl Client {
    pub fn new() -> Self {
        Client {}
    }

    pub fn get(&self, url: &str) -> Result<Response, String> {
        reqwest::blocking::get(url)
            .map(|response| Response { inner: response })
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expects::{equal::equal, Subject};
    use serde_json::Value;

    #[test]
    fn performs_a_get_request() {
        let client = Client::new();

        let response: Value = client
            .get("https://registry.npmjs.org/")
            .unwrap()
            .json()
            .unwrap();

        response["db_name"].as_str().should(equal("registry"));
        response["engine"].as_str().should(equal("couch_bt_engine"));
    }
}
