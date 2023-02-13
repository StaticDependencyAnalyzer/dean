use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use log::info;
use tokio::fs::File;
use tokio::sync::Mutex;
use tokio_stream::Stream;

use crate::infra::cached_issue_client::IssueStore;
use crate::infra::clock;
use crate::infra::git::{CommitStore, RepositoryRetriever};
use crate::infra::github;
use crate::infra::package_manager::cargo::InfoRetriever as CargoInfoRetriever;
use crate::infra::package_manager::npm::InfoRetriever as NpmInfoRetriever;
use crate::infra::repo_contribution;
use crate::infra::{commit_store, issue_store};
use crate::lazy::{AsyncLazy, Lazy};
use crate::pkg::config::{Config, Policies};
use crate::pkg::engine::{ExecutionConfig, PolicyExecutor};
use crate::pkg::format::csv::Reporter;
use crate::pkg::package_manager::{cargo, npm, yarn};
use crate::pkg::policy::{
    CommitRetriever, ContributionDataRetriever, ContributorsRatio, MaxIssueLifespan,
    MaxPullRequestLifespan, MinNumberOfReleasesRequired, Policy,
};
use crate::pkg::recognizer::PackageManager;
use crate::pkg::{DependencyRetriever, InfoRetriever};
use crate::{Dependency, Result};

pub struct Factory {
    config: Rc<Config>,

    info_retriever: Lazy<Arc<dyn InfoRetriever>>,
    http_client: Lazy<Arc<reqwest::Client>>,
    repository_retriever: Lazy<Arc<dyn CommitRetriever>>,
    contribution_retriever: AsyncLazy<Arc<dyn ContributionDataRetriever>>,
    github_client: Lazy<Arc<github::Client>>,
    commit_store: Lazy<Arc<dyn CommitStore>>,
    issue_store: AsyncLazy<Arc<dyn IssueStore>>,
}

const DAYS_TO_SECONDS: u64 = 86400;

impl Factory {
    pub async fn dependency_reader<'a, T: tokio::io::AsyncRead + Unpin + Send + 'a>(
        &self,
        reader: T,
        lock_file: &str,
    ) -> impl Stream<Item = Dependency> + Unpin + Send {
        let retriever = self.info_retriever(lock_file);

        match Self::package_manager(lock_file) {
            PackageManager::Npm => Box::new(
                npm::DependencyReader::new(reader, retriever)
                    .dependencies()
                    .await
                    .expect("failed to retrieve npm dependencies from reader"),
            ),
            PackageManager::Cargo => Box::new(
                cargo::DependencyReader::new(reader, retriever)
                    .dependencies()
                    .await
                    .expect("failed to retrieve cargo dependencies from reader"),
            ),
            PackageManager::Yarn => Box::new(
                yarn::DependencyReader::new(reader, retriever)
                    .dependencies()
                    .await
                    .expect("failed to retrieve yarn dependencies from reader"),
            ),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    async fn config_policies_to_vector(&self, config_policies: &Policies) -> Vec<Box<dyn Policy>> {
        let repository_retriever = self.repository_retriever();
        let mut policies: Vec<Box<dyn Policy>> = Vec::new();

        if let Some(policy) = &config_policies.min_number_of_releases_required {
            policies.push(Box::new(MinNumberOfReleasesRequired::new(
                repository_retriever.clone(),
                policy.min_number_of_releases,
                Duration::from_secs(policy.days * DAYS_TO_SECONDS),
                Box::new(clock::System::new()),
            )));
        }
        if let Some(policy) = &config_policies.contributors_ratio {
            policies.push(Box::new(ContributorsRatio::new(
                repository_retriever.clone(),
                policy.max_number_of_releases_to_check,
                policy.max_contributor_ratio,
            )));
        }
        if let Some(policy) = &config_policies.max_issue_lifespan {
            policies.push(Box::new(MaxIssueLifespan::new(
                self.contribution_retriever().await,
                policy.max_lifespan_in_seconds as f64,
                policy.last_issues,
            )));
        }
        if let Some(policy) = &config_policies.max_pull_request_lifespan {
            policies.push(Box::new(MaxPullRequestLifespan::new(
                self.contribution_retriever().await,
                policy.max_lifespan_in_seconds as f64,
                policy.last_pull_requests,
            )));
        }

        policies
    }

    async fn execution_configs(&self) -> Result<Vec<ExecutionConfig>> {
        let mut execution_configs = vec![];

        for dependency_config in &self.config.dependency_config {
            execution_configs.push(ExecutionConfig::new(
                self.config_policies_to_vector(&dependency_config.policies)
                    .await,
                Some(&dependency_config.name),
            )?);
        }

        let policies = self
            .config_policies_to_vector(&self.config.default_policies)
            .await;
        if !policies.is_empty() {
            execution_configs.push(ExecutionConfig::new(policies, None)?);
        }

        Ok(execution_configs)
    }

    fn info_retriever(&self, lock_file: &str) -> Arc<dyn InfoRetriever> {
        self.info_retriever
            .get(|| {
                let http_client = self.http_client();

                match Self::package_manager(lock_file) {
                    PackageManager::Npm | PackageManager::Yarn => {
                        Arc::new(NpmInfoRetriever::new(http_client))
                    }
                    PackageManager::Cargo => Arc::new(CargoInfoRetriever::new(http_client)),
                }
            })
            .clone()
    }

    fn http_client(&self) -> Arc<reqwest::Client> {
        self.http_client
            .get(|| {
                let reqwest_client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(600))
                    .build()
                    .expect("unable to create the reqwest client");

                Arc::new(reqwest_client)
            })
            .clone()
    }

    fn package_manager(lock_file: &str) -> PackageManager {
        PackageManager::from_filename(lock_file)
            .unwrap_or_else(|| panic!("unable to determine package manager for file: {lock_file}"))
    }

    fn repository_retriever(&self) -> Arc<dyn CommitRetriever> {
        self.repository_retriever
            .get(|| {
                let git_repository_retriever = RepositoryRetriever::new(self.commit_store());

                Arc::new(git_repository_retriever)
            })
            .clone()
    }

    async fn contribution_retriever(&self) -> Arc<dyn ContributionDataRetriever> {
        self.contribution_retriever
            .async_get(|| async {
                let git_contributor_retriever = repo_contribution::Retriever::new(
                    self.github_client(),
                    self.issue_store().await,
                );

                Arc::new(git_contributor_retriever) as Arc<dyn ContributionDataRetriever>
            })
            .await
            .clone()
    }

    fn github_authentication() -> github::Authentication {
        let github_username = std::env::var("GITHUB_USERNAME").ok();
        let github_password = std::env::var("GITHUB_PASSWORD").ok();

        match github_username {
            None => {
                info!(target: "dean::github_authentication", "using anonymous authentication");
                github::Authentication::None
            }
            Some(github_username) => {
                info!(
                    target: "dean::github_authentication",
                    "using basic authentication with username: {} and password: {}",
                    &github_username,
                    { if github_password.is_some() { "******" } else { "not set" } },
                );

                github::Authentication::Basic(github_username, github_password)
            }
        }
    }

    fn github_client(&self) -> Arc<github::Client> {
        self.github_client
            .get(|| {
                let github_client =
                    github::Client::new(reqwest::Client::new(), Self::github_authentication());

                Arc::new(github_client)
            })
            .clone()
    }

    pub fn result_reporter() -> Reporter<File> {
        let reader = std::fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open("result.csv")
            .expect("unable to open result.csv");

        let reader = File::from_std(reader);

        Reporter::new(Arc::new(Mutex::new(reader)))
    }

    pub async fn engine(&mut self) -> Result<PolicyExecutor> {
        Ok(PolicyExecutor::new(self.execution_configs().await?))
    }

    fn commit_store(&self) -> Arc<dyn CommitStore> {
        self.commit_store
            .get(|| {
                let connection =
                    rusqlite::Connection::open("dean.db3").expect("unable to open dean.db3");
                let commit_store = commit_store::Sqlite::new(std::sync::Mutex::new(connection));
                commit_store.init().expect("unable to init commit store");

                Arc::new(commit_store)
            })
            .clone()
    }

    async fn issue_store(&self) -> Arc<dyn IssueStore> {
        self.issue_store
            .async_get(|| async {
                #[cfg(not(feature = "experimental-surrealdb"))]
                {
                    let connection =
                        rusqlite::Connection::open("dean.db3").expect("unable to open dean.db3");
                    let issue_store = issue_store::Sqlite::new(std::sync::Mutex::new(connection));
                    issue_store.init().expect("unable to init issue store");

                    Arc::new(issue_store) as Arc<dyn IssueStore>
                }

                #[cfg(feature = "experimental-surrealdb")]
                {
                    let connection = surrealdb::engine::any::connect("file://issue_store.db")
                        .await
                        .expect("unable to connect");
                    connection
                        .use_ns("ns")
                        .use_db("db")
                        .await
                        .expect("unable to specify ns and db");
                    let issue_store = issue_store::SurrealDB::new(connection)
                        .await
                        .expect("unable to create surrealDB client");

                    Arc::new(issue_store) as Arc<dyn IssueStore>
                }
            })
            .await
            .clone()
    }
}

impl Factory {
    pub fn new(config: Rc<Config>) -> Self {
        Self {
            config,

            info_retriever: Lazy::new(),
            http_client: Lazy::new(),
            repository_retriever: Lazy::new(),
            contribution_retriever: AsyncLazy::new(),
            github_client: Lazy::new(),
            commit_store: Lazy::new(),
            issue_store: AsyncLazy::new(),
        }
    }
}
