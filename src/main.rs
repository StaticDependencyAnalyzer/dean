#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod infra;
mod pkg;

use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use log::LevelFilter;

use crate::cmd::parse_args;
use crate::http::Client;
use crate::infra::git::RepositoryRetriever;

use crate::infra::http;
use crate::infra::npm::DependencyInfoRetriever;
use crate::pkg::config::Config;
use crate::pkg::npm::{Dependency, DependencyReader};
use crate::pkg::policy::{ContributorsRatio, Evaluation, MinNumberOfReleasesRequired, Policy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    load_logger()?;
    let config = load_config_from_file();

    let http_client = create_http_client()?;
    let retriever = Box::new(DependencyInfoRetriever::new(http_client));
    let reader = DependencyReader::new(retriever);

    let file = File::open(&args.lock_file)
        .map_err(|err| format!("file {} could not be opened: {}", &args.lock_file, err))?;

    let policies = policies_from_config(&config);

    reader.retrieve_from_reader(file).map(|x| {
        for dep in &x {
            println!(
                "{}: {} (latest: {}) - {} - {}",
                dep.name,
                dep.version,
                dep.latest_version
                    .as_ref()
                    .unwrap_or(&"unknown".to_string()),
                dep.repository,
                check_if_dependency_is_okay(&policies, dep)
            );
        }
    })?;

    Ok(())
}

fn policies_from_config(config: &Config) -> Vec<Box<dyn Policy>> {
    let mut policies: Vec<Box<dyn Policy>> = vec![];
    let retriever = Arc::new(RwLock::new(RepositoryRetriever::new()));

    if config.policies.contributors_ratio.enabled {
        policies.push(Box::new(ContributorsRatio::new(
            retriever.clone(),
            config
                .policies
                .contributors_ratio
                .max_number_of_releases_to_check,
            config.policies.contributors_ratio.max_contributor_ratio,
        )));
    }

    if config.policies.min_number_of_releases_required.enabled {
        let clock = Box::new(infra::clock::Clock::default());
        policies.push(Box::new(MinNumberOfReleasesRequired::new(
            retriever,
            config
                .policies
                .min_number_of_releases_required
                .min_number_of_releases,
            Duration::from_secs(config.policies.min_number_of_releases_required.days * 86400),
            clock,
        )));
    }

    policies
}

fn check_if_dependency_is_okay(policies: &[Box<dyn Policy>], dep: &Dependency) -> String {
    for policy in policies {
        match policy.evaluate(dep) {
            Ok(result) => match result {
                Evaluation::Pass => continue,
                Evaluation::Fail(reason) => {
                    return format!("Fail due to: {}", reason);
                }
            },
            Err(error) => {
                return format!("Error: {}", error);
            }
        }
    }
    "PASS".to_owned()
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
        .timeout(Duration::from_secs(600))
        .build()?;

    Ok(http::Client::new(reqwest_client))
}

fn load_config_from_file() -> Config {
    match default_config_file() {
        Ok(config_file) => match File::open(&config_file) {
            Ok(mut file) => match Config::load_from_reader(&mut file) {
                Ok(config) => {
                    return config;
                }
                Err(err) => {
                    log::warn!("could not load config from file: {}", err);
                }
            },
            Err(err) => {
                log::warn!(
                    "could not open config file {}: {}",
                    &config_file.display(),
                    err
                );
            }
        },
        Err(err) => {
            log::warn!("could not determine default config file: {}", err);
        }
    }
    log::info!("using default config");
    Config::default()
}

fn default_config_file() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = home_directory().ok_or_else(|| { "Could not find home directory. Please set the environment variable HOME to your home directory.".to_string() })?;
    Ok(home.join(".config/dean.yaml"))
}

fn home_directory() -> Option<PathBuf> {
    dirs_next::home_dir()
}
