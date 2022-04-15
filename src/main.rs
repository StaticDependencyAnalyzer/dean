#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod infra;
mod pkg;

use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use log::LevelFilter;

use crate::cmd::parse_args;
use crate::http::Client;
use crate::infra::http;
use crate::infra::npm::DependencyInfoRetriever;
use crate::pkg::config::Config;
use crate::pkg::npm::DependencyReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    load_logger()?;

    let http_client = create_http_client()?;
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

fn load_logger() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .with_colors(true)
        .env()
        .init()?;
    Ok(())
}

fn create_http_client() -> Result<Client, Box<dyn std::error::Error>> {
    let reqwest_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    Ok(http::Client::new(reqwest_client))
}

fn load_default_config_from_file() -> Result<Config, Box<dyn std::error::Error>> {
    let config_file = default_config_file()?;
    let mut file = File::open(&config_file).map_err(|err| {
        if let Some(config_file_path) = config_file.to_str() {
            format!("file {} could not be opened: {}", config_file_path, err)
        } else {
            "unable to retrieve config file path".to_string()
        }
    })?;

    let config = Config::load_from_reader(&mut file)?;
    Ok(config)
}

fn default_config_file() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = home_directory().ok_or_else(|| { "Could not find home directory. Please set the environment variable HOME to your home directory.".to_string() })?;
    Ok(home.join(".config/dean.yaml"))
}

fn home_directory() -> Option<PathBuf> {
    dirs_next::home_dir()
}
