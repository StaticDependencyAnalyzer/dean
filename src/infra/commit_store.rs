use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::anyhow;
use async_trait::async_trait;

use crate::infra::git::CommitStore;
use crate::pkg::policy::{Commit, Tag};

pub struct Sqlite {
    db: Arc<Mutex<rusqlite::Connection>>,
}

#[async_trait]
impl CommitStore for Sqlite {
    async fn get_commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Option<HashMap<String, Vec<Commit>>> {
        let connection = self.db.clone();
        let repository_url = repository_url.to_string();

        tokio::task::spawn_blocking(move || {
            let lock = connection.lock().ok()?;

            let mut select_tag_stmt = lock.prepare(
                "SELECT tag FROM commitstore_commits_for_each_tag WHERE repository = ? GROUP BY tag",
            ).ok()?;
            let mut select_commit_info_stmt = lock.prepare(
                "SELECT commit_id, commit_author_email, commit_author_name, commit_creation_timestamp FROM commitstore_commits_for_each_tag WHERE repository = ? AND tag = ?",
            ).ok()?;

            let tags = select_tag_stmt
                .query_map([&repository_url], |row| {
                    let tag: String = row.get(0)?;
                    Ok(tag)
                })
                .ok()?;

            let tags_and_commits: Option<HashMap<String, Vec<Commit>>> = tags
                .flatten()
                .map(|tag| {
                    let commits = select_commit_info_stmt
                        .query_map([&repository_url, tag.as_str()], |row| {
                            Ok(Commit {
                                id: row.get(0)?,
                                author_email: row.get(1)?,
                                author_name: row.get(2)?,
                                creation_timestamp: row.get(3)?,
                            })
                        })
                        .ok()?
                        .flatten();

                    let commits: Vec<Commit> = commits.collect();
                    if commits.is_empty() {
                        None
                    } else {
                        Some((tag, commits))
                    }
                })
                .collect();

            let tags_and_commits = tags_and_commits?;
            if tags_and_commits.is_empty() {
                None
            } else {
                Some(tags_and_commits)
            }
        }).await.ok()?
    }

    async fn save_commits_for_each_tag(
        &self,
        repository_url: &str,
        commits_for_each_tag: &HashMap<String, Vec<Commit>>,
    ) -> Result<(), Box<dyn Error>> {
        let connection = self.db.clone();
        let repository_url = repository_url.to_string();
        let commits_for_each_tag = commits_for_each_tag.clone();

        let result: Result<(), anyhow::Error> = tokio::task::spawn_blocking(move || {
            let mut lock = connection.lock().map_err(|e| anyhow!("unable to lock the database: {}", e))?;

            let tx = lock.transaction()?;

            {
                let mut stmt = tx.prepare(
                    "INSERT OR IGNORE INTO commitstore_commits_for_each_tag (repository, tag, commit_id, commit_author_email, commit_author_name, commit_creation_timestamp) VALUES (?, ?, ?, ?, ?, ?)",
                )?;

                for (tag_name, commits) in commits_for_each_tag {
                    for commit in commits {
                        stmt.execute([
                            &repository_url,
                            &tag_name,
                            &commit.id,
                            &commit.author_email,
                            &commit.author_name,
                            &commit.creation_timestamp.to_string(),
                        ])?;
                    }
                }
            }

            tx.commit()?;

            Ok(())
        }).await.map_err(|e| anyhow!("unable to save commits for each tag: {}", e))?;

        result.map_err(std::convert::Into::into)
    }

    async fn get_all_tags(&self, repository_url: &str) -> Option<Vec<Tag>> {
        let connection = self.db.clone();
        let repository_url = repository_url.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let lock = connection.lock().ok()?;

            let mut stmt = lock
                .prepare("SELECT name, commit_id, commit_timestamp FROM commitstore_tags WHERE repository = ?")
                .ok()?;

            let iter = stmt
                .query_map([&repository_url], |row| {
                    let name: String = row.get(0)?;
                    let commit_id: String = row.get(1)?;
                    let commit_timestamp: u64 = row.get(2)?;

                    Ok(Tag {
                        name,
                        commit_id,
                        commit_timestamp,
                    })
                })
                .ok()?;

            let tags: Vec<Tag> = iter.flatten().collect();
            if tags.is_empty() {
                None
            } else {
                Some(tags)
            }
        }).await;

        result.ok()?
    }

    async fn save_all_tags(
        &self,
        repository_url: &str,
        all_tags: &[Tag],
    ) -> Result<(), Box<dyn Error>> {
        let connection = self.db.clone();
        let repository_url = repository_url.to_string();
        let all_tags = all_tags.to_vec();

        let result: Result<(), anyhow::Error> = tokio::task::spawn_blocking(move || {
            let mut lock = connection.lock().map_err(|e| anyhow!("unable to lock the database: {}", e))?;
            let tx = lock.transaction()?;

            {
                let mut stmt = tx
                    .prepare("INSERT OR IGNORE INTO commitstore_tags (repository, name, commit_id, commit_timestamp) VALUES (?, ?, ?, ?)")?;

                for tag in all_tags {
                    stmt.execute([
                        repository_url.as_str(),
                        &tag.name,
                        &tag.commit_id,
                        tag.commit_timestamp.to_string().as_str(),
                    ])?;
                }
            }

            tx.commit()?;

            Ok(())
        }).await?;

        result.map_err(std::convert::Into::into)
    }
}

impl Sqlite {
    pub fn new<T: Into<Arc<Mutex<rusqlite::Connection>>>>(db: T) -> Self {
        Self { db: db.into() }
    }

    pub fn init(&self) -> Result<(), Box<dyn Error>> {
        self.db
            .lock()
            .map_err(|e| format!("unable to lock the database: {}", e))?
            .execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS commitstore_tags (
    repository TEXT NOT NULL,
    name TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    commit_timestamp INTEGER NOT NULL,
    PRIMARY KEY (repository, name)
);

CREATE TABLE IF NOT EXISTS commitstore_commits_for_each_tag (
    repository TEXT NOT NULL,
    tag TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    commit_author_name TEXT NOT NULL,
    commit_author_email TEXT NOT NULL,
    commit_creation_timestamp INTEGER NOT NULL,
    PRIMARY KEY (repository, tag, commit_id)
);
                    "#,
            )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_saves_and_retrieves_all_the_tags() {
        let commit_store = commit_store();

        commit_store
            .save_all_tags("repository", &tags_in_repo())
            .await
            .unwrap();
        let tags = commit_store.get_all_tags("repository").await.unwrap();

        assert_eq!(tags, tags_in_repo());
    }

    #[tokio::test]
    async fn it_saves_and_retrieves_all_commits_for_each_tag() {
        let commit_store = commit_store();

        commit_store
            .save_commits_for_each_tag("repository", &commits_for_each_tag_in_repo())
            .await
            .unwrap();
        let commits_for_each_tag = commit_store
            .get_commits_for_each_tag("repository")
            .await
            .unwrap();

        assert_eq!(commits_for_each_tag, commits_for_each_tag_in_repo());
    }

    #[tokio::test]
    async fn if_the_tags_are_not_present_it_returns_none() {
        let commit_store = commit_store();

        let tags = commit_store.get_all_tags("unknown_repository").await;

        assert_eq!(tags, None);
    }

    #[tokio::test]
    async fn if_the_commits_are_not_present_it_returns_none() {
        let commit_store = commit_store();

        let commits_for_each_tag = commit_store
            .get_commits_for_each_tag("unknown_repository")
            .await;

        assert_eq!(commits_for_each_tag, None);
    }

    fn commit_store() -> Sqlite {
        let in_memory_connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let commit_store = Sqlite::new(in_memory_connection);
        commit_store.init().unwrap();
        commit_store
    }

    fn commits_for_each_tag_in_repo() -> HashMap<String, Vec<Commit>> {
        let mut commits_for_each_tag = HashMap::new();
        commits_for_each_tag.insert(
            "1.0.0".to_string(),
            vec![
                Commit {
                    id: "commit1".to_string(),
                    author_name: "some_author".to_string(),
                    author_email: "some_email".to_string(),
                    creation_timestamp: 0,
                },
                Commit {
                    id: "commit2".to_string(),
                    author_name: "some_author".to_string(),
                    author_email: "some_email".to_string(),
                    creation_timestamp: 1,
                },
            ],
        );
        commits_for_each_tag
    }

    fn tags_in_repo() -> Vec<Tag> {
        vec![
            Tag {
                name: "v1.0.0".to_string(),
                commit_id: "commit_id".to_string(),
                commit_timestamp: 1,
            },
            Tag {
                name: "v1.0.1".to_string(),
                commit_id: "commit_id".to_string(),
                commit_timestamp: 2,
            },
        ]
    }
}
