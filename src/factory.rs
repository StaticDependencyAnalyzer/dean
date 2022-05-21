use std::cell::RefCell;
use std::error::Error;
use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::infra::clock::Clock;
use crate::infra::git::RepositoryRetriever;
use crate::infra::http;
use crate::infra::package_manager::cargo::InfoRetriever as CargoInfoRetriever;
use crate::infra::package_manager::npm::InfoRetriever as NpmInfoRetriever;
use crate::pkg::config::Config;
use crate::pkg::csv::Reporter;
use crate::pkg::engine::{ExecutionConfig, PolicyExecutor};
use crate::pkg::package_manager::{cargo, npm};
use crate::pkg::policy::{CommitRetriever, ContributorsRatio, MinNumberOfReleasesRequired, Policy};
use crate::pkg::recognizer::PackageManager;
use crate::pkg::{DependencyRetriever, InfoRetriever};
use crate::Dependency;

pub struct Factory {
    config: Rc<Config>,

    info_retriever: Option<Arc<dyn InfoRetriever + Sync + Send>>,
    http_client: Option<Arc<http::Client>>,
    repository_retriever: Option<Arc<dyn CommitRetriever + Send + Sync>>,
}

const DAYS_TO_SECONDS: u64 = 86400;

impl Factory {
    pub fn dependency_reader<'a, T: std::io::Read + Send + 'a>(
        &mut self,
        reader: T,
        lock_file: &str,
    ) -> Box<dyn Iterator<Item = Dependency> + Send + 'a> {
        let retriever = self
            .info_retriever(lock_file)
            .expect("unable to create the info retriever");

        match Self::package_manager(lock_file) {
            PackageManager::Npm => Box::new(
                npm::DependencyReader::new(reader, retriever)
                    .dependencies()
                    .unwrap(),
            ),
            PackageManager::Cargo => Box::new(
                cargo::DependencyReader::new(reader, retriever)
                    .dependencies()
                    .unwrap(),
            ),
        }
    }

    fn execution_configs(&mut self) -> Result<Vec<ExecutionConfig>, Box<dyn Error>> {
        let mut execution_configs = vec![];
        let repository_retriever = self.repository_retriever();

        for dependency_config in &self.config.dependency_config {
            let mut policies: Vec<Box<dyn Policy>> = vec![];

            if let Some(policy) = &dependency_config.policies.min_number_of_releases_required {
                policies.push(Box::new(MinNumberOfReleasesRequired::new(
                    repository_retriever.clone(),
                    policy.min_number_of_releases,
                    Duration::from_secs(policy.days * DAYS_TO_SECONDS),
                    Box::new(Clock::default()),
                )));
            }
            if let Some(policy) = &dependency_config.policies.contributors_ratio {
                policies.push(Box::new(ContributorsRatio::new(
                    repository_retriever.clone(),
                    policy.max_number_of_releases_to_check,
                    policy.max_contributor_ratio,
                )));
            }

            execution_configs.push(ExecutionConfig::new(
                policies,
                Some(&dependency_config.name),
            )?);
        }

        let mut policies: Vec<Box<dyn Policy>> = vec![];
        if let Some(policy) = &self.config.default_policies.min_number_of_releases_required {
            policies.push(Box::new(MinNumberOfReleasesRequired::new(
                repository_retriever.clone(),
                policy.min_number_of_releases,
                Duration::from_secs(policy.days * DAYS_TO_SECONDS),
                Box::new(Clock::default()),
            )));
        }
        if let Some(policy) = &self.config.default_policies.contributors_ratio {
            policies.push(Box::new(ContributorsRatio::new(
                repository_retriever.clone(),
                policy.max_number_of_releases_to_check,
                policy.max_contributor_ratio,
            )));
        }
        if !policies.is_empty() {
            execution_configs.push(ExecutionConfig::new(policies, None)?);
        }

        Ok(execution_configs)
    }

    fn info_retriever(
        &mut self,
        lock_file: &str,
    ) -> Result<Arc<dyn InfoRetriever + Sync + Send>, Box<dyn Error>> {
        if let Some(ref retriever) = self.info_retriever {
            return Ok(retriever.clone());
        }

        let http_client = self.http_client()?;
        let retriever: Arc<dyn InfoRetriever + Sync + Send> = match Self::package_manager(lock_file)
        {
            PackageManager::Npm => Arc::new(NpmInfoRetriever::new(http_client)),
            PackageManager::Cargo => Arc::new(CargoInfoRetriever::new(http_client)),
        };
        self.info_retriever = Some(retriever.clone());
        Ok(retriever)
    }

    fn http_client(&mut self) -> Result<Arc<http::Client>, Box<dyn std::error::Error>> {
        if let Some(http_client) = &self.http_client {
            return Ok(http_client.clone());
        }

        let reqwest_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;

        let client = Arc::new(http::Client::new(reqwest_client));
        self.http_client = Some(client.clone());
        Ok(client)
    }

    fn package_manager(lock_file: &str) -> PackageManager {
        PackageManager::from_filename(lock_file).unwrap_or_else(|| {
            panic!(
                "unable to determine package manager for file: {}",
                lock_file
            )
        })
    }

    fn repository_retriever(&mut self) -> Arc<dyn CommitRetriever + Send + Sync> {
        if let Some(retriever) = &self.repository_retriever {
            return retriever.clone();
        }

        let retriever = Arc::new(RepositoryRetriever::new());
        self.repository_retriever = Some(retriever.clone());
        retriever
    }

    pub fn result_reporter() -> Reporter<File> {
        let reader = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open("result.csv")
            .expect("unable to open result.csv");
        Reporter::new(Rc::new(RefCell::new(reader)))
    }

    pub fn engine(&mut self) -> Result<PolicyExecutor, Box<dyn Error>> {
        Ok(PolicyExecutor::new(self.execution_configs()?))
    }
}

impl Factory {
    pub fn new(config: Rc<Config>) -> Self {
        Self {
            config,

            info_retriever: None,
            http_client: None,
            repository_retriever: None,
        }
    }
}
