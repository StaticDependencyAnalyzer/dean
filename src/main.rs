#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod infra;
mod pkg;

use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use log::{error, info, LevelFilter};
use rayon::prelude::*;

use crate::cmd::parse_args;
use crate::http::Client;
use crate::infra::git::RepositoryRetriever;
use crate::infra::http;
use crate::infra::package_manager::npm::InfoRetriever as NpmInfoRetriever;
use crate::pkg::config::Config;
use crate::pkg::package_manager::npm;
use crate::pkg::policy::{ContributorsRatio, Evaluation, MinNumberOfReleasesRequired, Policy};
use crate::pkg::recognizer::{package_manager_from_filename, PackageManager};
use crate::pkg::{Dependency, DependencyRetriever, InfoRetriever};

fn info_retriever_from_package_manager(
    package_manager: PackageManager,
) -> Result<Box<dyn InfoRetriever + Sync + Send>, Box<dyn std::error::Error>> {
    let http_client = create_http_client()?;
    match package_manager {
        PackageManager::Npm => Ok(Box::new(NpmInfoRetriever::new(http_client))),
        PackageManager::Cargo => Err("Cargo is not supported yet".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    load_logger()?;
    let config = load_config_from_file();

    let package_manager = package_manager_from_filename(&args.lock_file).with_context(|| {
        format!(
            "unable to determine package manager for file: {}",
            &args.lock_file
        )
    })?;

    let file = File::open(&args.lock_file)
        .map_err(|err| format!("file {} could not be opened: {}", &args.lock_file, err))?;

    let reader = dependency_reader_from_package_manager(package_manager, file)?;

    let policies = policies_from_config(&config);

    let dependencies = reader.dependencies()?;
    dependencies.into_par_iter().for_each(|dep| {
            let evaluation = check_if_dependency_is_okay(&policies, &dep);
            match evaluation {
                Evaluation::Pass => {
                    info!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is okay",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository
                    );
                }
                Evaluation::Fail(reason) => {
                    error!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is not okay: {}",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository, reason
                    );
                }
            }
        });

    Ok(())
}

fn dependency_reader_from_package_manager(
    package_manager: PackageManager,
    reader: impl std::io::Read + Send + 'static,
) -> Result<Box<dyn DependencyRetriever + Send>, Box<dyn std::error::Error>> {
    let retriever = info_retriever_from_package_manager(package_manager)?;
    match package_manager {
        PackageManager::Npm => Ok(Box::new(npm::DependencyReader::new(reader, retriever))),
        PackageManager::Cargo => Err("Cargo is not supported yet".into()),
    }
}

fn policies_from_config(config: &Config) -> Vec<Box<dyn Policy + Send + Sync>> {
    let mut policies: Vec<Box<dyn Policy + Send + Sync>> = vec![];
    let retriever = Arc::new(RepositoryRetriever::new());

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

fn check_if_dependency_is_okay(
    policies: &[Box<dyn Policy + Send + Sync>],
    dep: &Dependency,
) -> Evaluation {
    for policy in policies.iter() {
        match policy.evaluate(dep) {
            Ok(result) => match result {
                Evaluation::Pass => continue,
                Evaluation::Fail(reason) => {
                    return Evaluation::Fail(reason);
                }
            },
            Err(error) => {
                return Evaluation::Fail(error.to_string());
            }
        }
    }
    Evaluation::Pass
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

    Ok(Client::new(reqwest_client))
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
