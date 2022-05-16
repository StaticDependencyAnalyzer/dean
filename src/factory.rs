use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::infra::clock::Clock;
use crate::infra::git::RepositoryRetriever;
use crate::infra::http;
use crate::infra::package_manager::cargo::InfoRetriever as CargoInfoRetriever;
use crate::infra::package_manager::npm::InfoRetriever as NpmInfoRetriever;
use crate::pkg::config::Config;
use crate::pkg::package_manager::{cargo, npm};
use crate::pkg::policy::{ContributorsRatio, MinNumberOfReleasesRequired, Policy};
use crate::pkg::recognizer::PackageManager;
use crate::pkg::{DependencyRetriever, InfoRetriever};

pub struct Factory {
    config: Rc<Config>,
}

impl Factory {
    pub fn dependency_reader<'a, T: std::io::Read + Send + 'a>(
        reader: T,
        lock_file: &str,
    ) -> Box<dyn DependencyRetriever + Send + 'a> {
        let retriever =
            Self::info_retriever(lock_file).expect("unable to create the info retriever");

        match Self::package_manager(lock_file) {
            PackageManager::Npm => Box::new(npm::DependencyReader::new(reader, retriever)),
            PackageManager::Cargo => Box::new(cargo::DependencyReader::new(reader, retriever)),
        }
    }

    fn info_retriever(
        lock_file: &str,
    ) -> Result<Box<dyn InfoRetriever + Sync + Send>, Box<dyn std::error::Error>> {
        let http_client = Self::http_client()?;
        match Self::package_manager(lock_file) {
            PackageManager::Npm => Ok(Box::new(NpmInfoRetriever::new(http_client))),
            PackageManager::Cargo => Ok(Box::new(CargoInfoRetriever::new(http_client))),
        }
    }

    fn http_client() -> Result<http::Client, Box<dyn std::error::Error>> {
        let reqwest_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;

        Ok(http::Client::new(reqwest_client))
    }

    fn package_manager(lock_file: &str) -> PackageManager {
        PackageManager::from_filename(lock_file).unwrap_or_else(|| {
            panic!(
                "unable to determine package manager for file: {}",
                lock_file
            )
        })
    }
    pub fn policies(&self) -> Vec<Box<dyn Policy + Send + Sync>> {
        let mut policies: Vec<Box<dyn Policy + Send + Sync>> = vec![];
        let retriever = Arc::new(RepositoryRetriever::new());

        if self.config.policies.contributors_ratio.enabled {
            policies.push(Box::new(ContributorsRatio::new(
                retriever.clone(),
                self.config
                    .policies
                    .contributors_ratio
                    .max_number_of_releases_to_check,
                self.config
                    .policies
                    .contributors_ratio
                    .max_contributor_ratio,
            )));
        }

        if self.config.policies.min_number_of_releases_required.enabled {
            let clock = Box::new(Clock::default());
            policies.push(Box::new(MinNumberOfReleasesRequired::new(
                retriever,
                self.config
                    .policies
                    .min_number_of_releases_required
                    .min_number_of_releases,
                Duration::from_secs(
                    self.config.policies.min_number_of_releases_required.days * 86400,
                ),
                clock,
            )));
        }

        policies
    }
}

impl Factory {
    pub fn new(config: Rc<Config>) -> Self {
        Self { config }
    }
}
