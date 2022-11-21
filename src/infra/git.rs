use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use git2::Oid;

use crate::infra::cache::Cache;
use crate::pkg::policy::{Commit, CommitRetriever, Tag};

#[derive(Clone)]
struct RepositoryResult {
    commits_for_each_tag: HashMap<String, Vec<Commit>>,
    all_tags: Vec<Tag>,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CommitStore: Send + Sync {
    async fn get_commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Option<HashMap<String, Vec<Commit>>>;
    async fn save_commits_for_each_tag(
        &self,
        repository_url: &str,
        commits_for_each_tag: &HashMap<String, Vec<Commit>>,
    ) -> Result<(), Box<dyn Error>>;

    async fn get_all_tags(&self, repository_url: &str) -> Option<Vec<Tag>>;
    async fn save_all_tags(
        &self,
        repository_url: &str,
        all_tags: &[Tag],
    ) -> Result<(), Box<dyn Error>>;
}

pub struct RepositoryRetriever {
    cache: Cache<String, RepositoryResult>,
    commit_store: Arc<dyn CommitStore>,
}

#[async_trait]
impl CommitRetriever for RepositoryRetriever {
    async fn commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Result<HashMap<String, Vec<Commit>>, Box<dyn Error>> {
        self.cache
            .get_or_try_init(repository_url.to_string(), || async move {
                self.repository_result_from_url(repository_url).await
            })
            .await
            .map(|handle| handle.commits_for_each_tag)
            .map_err(std::convert::Into::into)
    }

    async fn all_tags(&self, repository_url: &str) -> Result<Vec<Tag>, Box<dyn Error>> {
        self.cache
            .get_or_try_init(repository_url.to_string(), || async move {
                self.repository_result_from_url(repository_url).await
            })
            .await
            .map(|handle| handle.all_tags)
            .map_err(std::convert::Into::into)
    }
}

impl RepositoryRetriever {
    pub fn new<T: Into<Arc<dyn CommitStore>>>(commit_store: T) -> Self {
        let cache = Cache::new();
        Self {
            cache,
            commit_store: commit_store.into(),
        }
    }

    async fn repository_result_from_url(
        &self,
        repository_url: &str,
    ) -> Result<RepositoryResult, anyhow::Error> {
        let commits_for_each_tag = self
            .commit_store
            .get_commits_for_each_tag(repository_url)
            .await;
        let all_tags = self.commit_store.get_all_tags(repository_url).await;

        if let Some(commits) = &commits_for_each_tag {
            if let Some(tags) = &all_tags {
                return Ok(RepositoryResult {
                    commits_for_each_tag: commits.clone(),
                    all_tags: tags.clone(),
                });
            }
        }

        let repository = Repository::new(repository_url)
            .await
            .map_err(|e| anyhow!("unable to create repository: {}", e))?;
        let commits_for_each_tag_in_repository = repository
            .commits_for_each_tag()
            .map_err(|e| anyhow!("error retrieving commits for each tag: {}", e))?;
        let all_tags_in_repository = repository
            .all_tags()
            .map_err(|e| anyhow!("error retrieving tags: {}", e))?;

        if commits_for_each_tag.is_none() {
            let commits_for_each_tag = commits_for_each_tag_in_repository.clone();
            self.commit_store
                .save_commits_for_each_tag(repository_url, &commits_for_each_tag)
                .await
                .map_err(|e| anyhow!("unable to save commits for each tag: {}", e))?;
        }

        if all_tags.is_none() {
            let all_tags = all_tags_in_repository.clone();
            self.commit_store
                .save_all_tags(repository_url, &all_tags)
                .await
                .map_err(|e| anyhow!("unable to save all tags: {}", e))?;
        }

        Ok(RepositoryResult {
            commits_for_each_tag: commits_for_each_tag_in_repository,
            all_tags: all_tags_in_repository,
        })
    }
}

pub struct Repository {
    repository: git2::Repository,
    #[allow(unused)]
    temp_dir: tempfile::TempDir,
}

impl Repository {
    pub async fn new(url: &str) -> Result<Self, Box<dyn Error>> {
        let url = url.to_string();
        let result: Result<Repository, anyhow::Error> = tokio::task::spawn_blocking(move || {
            let temp_dir = tempfile::tempdir().context("unable to create temp dir")?;
            let repository = git2::build::RepoBuilder::new()
                .bare(true)
                .clone(&url, temp_dir.path())
                .context("unable to clone repository")?;

            Ok(Repository {
                repository,
                temp_dir,
            })
        })
        .await?;

        result.map_err(std::convert::Into::into)
    }

    fn commits_for_each_tag(&self) -> Result<HashMap<String, Vec<Commit>>, Box<dyn Error>> {
        let commits_ids = self.commit_ids_for_each_tag()?;
        let map = commits_ids
            .into_iter()
            .map(|(key, value)| {
                (
                    key,
                    value
                        .into_iter()
                        .flat_map(|commit_id| self.commit_from_id(Cow::from(commit_id)))
                        .collect(),
                )
            })
            .collect::<HashMap<_, _>>();
        Ok(map)
    }

    #[allow(clippy::cast_sign_loss)]
    fn all_tags(&self) -> Result<Vec<Tag>, Box<dyn Error>> {
        let mut tags = vec![];
        self.repository.tag_foreach(|oid, name| {
            if let Ok(obj) = self.repository.find_object(oid, None) {
                if let Ok(commit) = obj.peel_to_commit() {
                    tags.push(Tag {
                        name: String::from_utf8_lossy(name).replace("refs/tags/", ""),
                        commit_id: commit.id().to_string(),
                        commit_timestamp: commit.time().seconds() as u64,
                    });
                }
            }
            true
        })?;
        tags.sort_by(|a, b| a.commit_timestamp.cmp(&b.commit_timestamp));
        Ok(tags)
    }

    fn commit_ids_for_each_tag(&self) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
        let mut result = HashMap::new();

        let tags: Vec<_> = self.all_tags()?.into_iter().rev().collect();
        if tags.is_empty() {
            return Ok(result);
        }

        let mut commit_buffer = Vec::new();
        for i in 0..tags.len() - 1 {
            let first_tag = tags.get(i).context("unable to retrieve first tag")?;
            let second_tag = tags.get(i + 1).context("unable to retrieve second tag")?;
            let first_oid = Oid::from_str(&first_tag.commit_id)?;
            let second_oid = Oid::from_str(&second_tag.commit_id)?;

            let mut revwalk = self.repository.revwalk()?;
            revwalk.push(first_oid)?;
            for oid in revwalk.flatten() {
                if oid == second_oid {
                    break;
                }
                commit_buffer.push(oid.to_string());
            }
            result.insert(first_tag.name.clone(), commit_buffer.clone());
            commit_buffer.clear();
        }

        Ok(result)
    }

    fn commit_from_id(&self, commit_id: Cow<str>) -> Result<Commit, Box<dyn Error>> {
        let oid = Oid::from_str(commit_id.as_ref())?;
        let commit = self
            .repository
            .find_object(oid, None)?
            .into_commit()
            .map_err(|_| "unable to convert into commit".to_string())?;
        let author_email = commit
            .author()
            .email()
            .context("unable to retrieve author email")?
            .to_string();
        let author_name = commit
            .author()
            .name()
            .context("unable to retrieve author name")?
            .to_string();

        Ok(Commit {
            id: commit_id.into_owned(),
            author_email,
            author_name,
            creation_timestamp: commit.time().seconds(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_retrieves_the_tags_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2")
            .await
            .expect("unable to create repository");

        let tags = repository.all_tags().expect("unable to retrieve all tags");

        assert!(tags.len() >= 76);
        assert!(tags.contains(&Tag {
            name: "v1.4.2".to_string(),
            commit_id: "182d0d1ee933de46bf0b5a6ec269bafa77aba9a2".to_string(),
            commit_timestamp: 1_645_905_004,
        }));
    }

    #[tokio::test]
    async fn it_retrieves_commit_ids_for_each_tag_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2")
            .await
            .unwrap();

        let commit_ids_for_each_tag = repository.commit_ids_for_each_tag().unwrap();

        assert_eq!(
            commit_ids_for_each_tag.get("v1.4.2").unwrap(),
            &[
                "182d0d1ee933de46bf0b5a6ec269bafa77aba9a2".to_string(),
                "970c3c71cefd764857a57b6d9f04e147ec3114b6".to_string(),
                "f2c5d1b105d07c3643d1af388715321bdcbd83db".to_string(),
                "377ec9bfe7d84aad1ac23206144b7cdb7f867df2".to_string(),
                "5d9f2aff9423a0395fd909312e2cfd7085552fd8".to_string(),
                "43bfa124c844288a9e2e361e1122cc1cc51f1e8f".to_string(),
            ]
        );
        assert_eq!(
            commit_ids_for_each_tag.get("v1.4.0").unwrap().len(),
            302_usize
        );
    }

    #[tokio::test]
    async fn it_retrieves_commit_for_each_tag_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2")
            .await
            .unwrap();

        let commits_for_each_tag = repository.commits_for_each_tag().unwrap();

        assert!(commits_for_each_tag
            .get("v1.4.2")
            .unwrap()
            .contains(&Commit {
                id: "43bfa124c844288a9e2e361e1122cc1cc51f1e8f".to_string(),
                author_name: "Carlos Martín Nieto".to_string(),
                author_email: "carlosmn@github.com".to_string(),
                creation_timestamp: 1_645_898_340,
            }));

        assert_eq!(commits_for_each_tag.get("v1.4.2").unwrap().len(), 6_usize);
    }

    #[tokio::test]
    async fn it_retrieves_the_contents_of_the_repositories_and_stores_them_in_a_cache() {
        let commit_store: Box<dyn CommitStore> = mock_commit_store();

        let repository_retriever = RepositoryRetriever::new(commit_store);
        let repository_url = "https://github.com/libgit2/libgit2";

        repository_retriever
            .commits_for_each_tag(repository_url)
            .await
            .unwrap();
        let after_retrieving_instant = std::time::Instant::now();

        let commits_for_each_tag = repository_retriever
            .commits_for_each_tag(repository_url)
            .await
            .unwrap();
        let after_second_retrieval_instant = std::time::Instant::now();

        assert!(
            after_second_retrieval_instant
                .duration_since(after_retrieving_instant)
                .as_secs()
                < 1
        );

        assert!(commits_for_each_tag
            .get("v1.4.2")
            .unwrap()
            .contains(&Commit {
                id: "43bfa124c844288a9e2e361e1122cc1cc51f1e8f".to_string(),
                author_name: "Carlos Martín Nieto".to_string(),
                author_email: "carlosmn@github.com".to_string(),
                creation_timestamp: 1_645_898_340,
            }));
        let after_retrieving_tags_instant = std::time::Instant::now();
        assert!(
            after_retrieving_tags_instant
                .duration_since(after_retrieving_instant)
                .as_secs()
                < 1
        );

        assert!(
            repository_retriever
                .all_tags(repository_url)
                .await
                .unwrap()
                .len()
                >= 76
        );
    }

    #[tokio::test]
    async fn it_retrieves_the_tags_for_yocto_queue() {
        let commit_store: Box<dyn CommitStore> = mock_commit_store();
        let repository_retriever = RepositoryRetriever::new(commit_store);
        let tags = repository_retriever
            .all_tags("https://github.com/sindresorhus/yocto-queue")
            .await
            .unwrap();

        assert!(tags.len() >= 2_usize);
    }

    fn mock_commit_store() -> Box<MockCommitStore> {
        let mut commit_store = Box::new(MockCommitStore::new());
        commit_store
            .expect_get_commits_for_each_tag()
            .return_const(None);
        commit_store.expect_get_all_tags().return_const(None);
        commit_store
            .expect_save_commits_for_each_tag()
            .once()
            .return_once(|_, _| Ok(()));
        commit_store
            .expect_save_all_tags()
            .once()
            .return_once(|_, _| Ok(()));

        commit_store
    }
}
