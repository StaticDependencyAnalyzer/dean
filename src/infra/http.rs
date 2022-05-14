use std::io::Read;

use serde::de::DeserializeOwned;

#[derive(Debug)]
pub struct Response {
    inner: reqwest::blocking::Response,
}

impl Response {
    pub fn json<T: DeserializeOwned>(self) -> Result<T, String> {
        let bytes = self.inner.bytes().map_err(|e| e.to_string())?;
        let contents = String::from_utf8_lossy(&bytes);
        serde_json::from_str(&contents).map_err(|e| format!("error: {}, contents: {}", e, contents))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Client {
    client: reqwest::blocking::Client,
}

impl Client {
    pub fn new(client: reqwest::blocking::Client) -> Self {
        Client { client }
    }

    pub fn get(&self, url: &str) -> Result<Response, String> {
        self.client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/80.0.3987.149 Safari/537.36")
            .send()
            .map(|response| Response { inner: response })
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use expects::{matcher::equal, Subject};
    use serde_json::Value;

    use super::*;

    #[test]
    fn performs_a_get_request() {
        let client = Client::new(reqwest::blocking::Client::new());

        let response: Value = client
            .get("https://registry.npmjs.org/")
            .unwrap()
            .json()
            .unwrap();

        response["db_name"].as_str().should(equal("registry"));
        response["engine"].as_str().should(equal("couch_bt_engine"));
    }
}
