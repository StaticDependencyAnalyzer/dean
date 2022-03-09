#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style, clippy::cargo)]
#![deny(unused)]

mod cmd;
mod infra;
mod pkg;

use std::error::Error;
use std::fs::File;
use std::time::Duration;

use crate::http::Client;
use crate::infra::http;
use cmd::parse_args;
use infra::npm::DependencyInfoRetriever;
use pkg::npm::DependencyReader;

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();

    let http_client = http_client();
    let retriever = Box::new(DependencyInfoRetriever::new(http_client));
    let reader = DependencyReader::new(retriever);

    let file = File::open(&args.lock_file)
        .map_err(|err| format!("file {} could not be opened: {}", &args.lock_file, err))?;

    reader.retrieve_from_reader(file).map(|x| {
        for dep in &x {
            println!(
                "{}: {} ({} latest: {}) - {}",
                dep.name,
                dep.version,
                if dep.version == dep.latest_version {
                    "✅"
                } else {
                    "️⚠️"
                },
                dep.latest_version,
                dep.repository
            );
        }
    })?;

    Ok(())
}

fn http_client() -> Client {
    let reqwest_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    http::Client::new(reqwest_client)
}
