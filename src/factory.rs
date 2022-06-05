use std::cell::RefCell;
use std::error::Error;
use std::fs::File;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::infra::clock::Clock;
use crate::infra::git::RepositoryRetriever;
use crate::infra::package_manager::cargo::InfoRetriever as CargoInfoRetriever;
use crate::infra::package_manager::npm::InfoRetriever as NpmInfoRetriever;
use crate::infra::repo_contribution;
use crate::infra::{github, http};
use crate::lazy::Lazy;
use crate::pkg::config::Config;
use crate::pkg::csv::Reporter;
use crate::pkg::engine::{ExecutionConfig, PolicyExecutor};
use crate::pkg::package_manager::{cargo, npm};
use crate::pkg::policy::{
    CommitRetriever, ContributionDataRetriever, ContributorsRatio, MaxIssueLifespan,
    MaxPullRequestLifespan, MinNumberOfReleasesRequired, Policy,
};
use crate::pkg::recognizer::PackageManager;
use crate::pkg::{DependencyRetriever, InfoRetriever};
use crate::Dependency;

pub struct Factory {
    config: Rc<Config>,

    info_retriever: Lazy<Arc<dyn InfoRetriever>>,
    http_client: Lazy<Arc<http::Client>>,
    repository_retriever: Lazy<Arc<dyn CommitRetriever>>,
    contribution_retriever: Lazy<Arc<dyn ContributionDataRetriever>>,
    github_client: Lazy<Arc<github::Client>>,
}

const DAYS_TO_SECONDS: u64 = 86400;

impl Factory {
    pub fn dependency_reader<'a, T: std::io::Read + Send + 'a>(
        &self,
        reader: T,
        lock_file: &str,
    ) -> Box<dyn Iterator<Item = Dependency> + Send + 'a> {
        let retriever = self.info_retriever(lock_file);

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

    fn execution_configs(&self) -> Result<Vec<ExecutionConfig>, Box<dyn Error>> {
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
            if let Some(policy) = &dependency_config.policies.max_issue_lifespan {
                policies.push(Box::new(
                    #[allow(clippy::cast_precision_loss)]
                    MaxIssueLifespan::new(
                        self.contribution_retriever(),
                        policy.max_lifespan_in_seconds as f64,
                    ),
                ));
            }
            if let Some(policy) = &dependency_config.policies.max_pull_request_lifespan {
                policies.push(Box::new(
                    #[allow(clippy::cast_precision_loss)]
                    MaxPullRequestLifespan::new(
                        self.contribution_retriever(),
                        policy.max_lifespan_in_seconds as f64,
                    ),
                ));
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
        if let Some(policy) = &self.config.default_policies.max_issue_lifespan {
            policies.push(Box::new(
                #[allow(clippy::cast_precision_loss)]
                MaxIssueLifespan::new(
                    self.contribution_retriever(),
                    policy.max_lifespan_in_seconds as f64,
                ),
            ));
        }
        if let Some(policy) = &self.config.default_policies.max_pull_request_lifespan {
            policies.push(Box::new(
                #[allow(clippy::cast_precision_loss)]
                MaxPullRequestLifespan::new(
                    self.contribution_retriever(),
                    policy.max_lifespan_in_seconds as f64,
                ),
            ));
        }
        if !policies.is_empty() {
            execution_configs.push(ExecutionConfig::new(policies, None)?);
        }

        Ok(execution_configs)
    }

    fn info_retriever(&self, lock_file: &str) -> Arc<dyn InfoRetriever> {
        let info_retriever = &self.info_retriever;
        info_retriever
            .get(|| {
                let http_client = self.http_client();

                match Self::package_manager(lock_file) {
                    PackageManager::Npm => Arc::new(NpmInfoRetriever::new(http_client)),
                    PackageManager::Cargo => Arc::new(CargoInfoRetriever::new(http_client)),
                }
            })
            .clone()
    }

    fn http_client(&self) -> Arc<http::Client> {
        self.http_client
            .get(|| {
                let reqwest_client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(600))
                    .build()
                    .expect("unable to create the reqwest client");

                Arc::new(http::Client::new(reqwest_client))
            })
            .clone()
    }

    fn package_manager(lock_file: &str) -> PackageManager {
        PackageManager::from_filename(lock_file).unwrap_or_else(|| {
            panic!(
                "unable to determine package manager for file: {}",
                lock_file
            )
        })
    }

    fn repository_retriever(&self) -> Arc<dyn CommitRetriever> {
        self.repository_retriever
            .get(|| {
                let git_repository_retriever = RepositoryRetriever::new();

                Arc::new(git_repository_retriever)
            })
            .clone()
    }

    fn contribution_retriever(&self) -> Arc<dyn ContributionDataRetriever> {
        self.contribution_retriever
            .get(|| {
                let git_contributor_retriever =
                    repo_contribution::Retriever::new(self.github_client());

                Arc::new(git_contributor_retriever)
            })
            .clone()
    }

    fn github_authentication() -> github::Authentication {
        let github_username = std::env::var("GITHUB_USERNAME").ok();
        let github_password = std::env::var("GITHUB_PASSWORD").ok();

        match github_username {
            None => github::Authentication::None,
            Some(github_username) => {
                github::Authentication::Basic(github_username, github_password)
            }
        }
    }

    fn github_client(&self) -> Arc<github::Client> {
        self.github_client
            .get(|| {
                let github_client = github::Client::new(
                    reqwest::blocking::Client::new(),
                    Self::github_authentication(),
                );

                Arc::new(github_client)
            })
            .clone()
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

            info_retriever: Lazy::new(),
            http_client: Lazy::new(),
            repository_retriever: Lazy::new(),
            contribution_retriever: Lazy::new(),
            github_client: Lazy::new(),
        }
    }
}
