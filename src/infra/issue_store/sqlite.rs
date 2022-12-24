use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

use crate::infra::cached_issue_client::IssueStore;

pub struct Sqlite {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

impl Sqlite {
    pub fn new<C>(connection: C) -> Self
    where
        C: Into<Arc<Mutex<rusqlite::Connection>>>,
    {
        Self {
            connection: connection.into(),
        }
    }

    pub fn init(&self) -> Result<(), Box<dyn Error>> {
        let conn = self.connection.lock().map_err(|_| "unable to lock connection")?;
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS issuestore_issue (
    provider TEXT NOT NULL,
    organization TEXT NOT NULL,
    repo TEXT NOT NULL,
    issue_body TEXT NOT NULL,
    PRIMARY KEY (provider, organization, repo, issue_body)
);

CREATE TABLE IF NOT EXISTS issuestore_pullrequest (
    provider TEXT NOT NULL,
    organization TEXT NOT NULL,
    repo TEXT NOT NULL,
    pullrequest_body TEXT NOT NULL,
    PRIMARY KEY (provider, organization, repo, pullrequest_body)
);
"#,
        )?;

        Ok(())
    }
}

#[async_trait]
impl IssueStore for Sqlite {
    async fn get_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>> {
        let connection_clone = self.connection.clone();
        let provider = provider.to_string();
        let organization = organization.to_string();
        let repo = repo.to_string();

        let issues = tokio::task::spawn_blocking(move || {
            let conn = connection_clone.lock().ok()?;

            let mut stmt = conn.prepare(
                "SELECT issue_body FROM issuestore_issue WHERE provider = ? AND organization = ? AND repo = ?",
            ).ok()?;

            let issues: Vec<Value> = stmt
                .query_map([provider, organization, repo], |row| {
                    let value_str: String = row.get(0)?;
                    let value: Value = serde_json::from_str(&value_str)
                        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
                    Ok(value)
                }).ok()?
                .flatten()
                .collect();

            Some(issues)
        }).await.ok()??;

        if issues.is_empty() {
            None
        } else {
            Some(issues)
        }
    }

    async fn save_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        issues: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let conn = self.connection.clone();
        let provider = provider.to_string();
        let organization = organization.to_string();
        let repo = repo.to_string();
        let issues = issues.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT OR IGNORE INTO issuestore_issue (provider, organization, repo, issue_body) VALUES (?, ?, ?, ?)",
                ).unwrap();
                for issue in issues {
                    stmt.execute([
                        &provider,
                        &organization,
                        &repo,
                        serde_json::to_string(&issue).unwrap().as_str(),
                    ])?;
                }
            }

            tx.commit()
        }).await??;

        Ok(())
    }

    async fn get_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>> {
        let conn_cloned = self.connection.clone();
        let provider = provider.to_string();
        let organization = organization.to_string();
        let repo = repo.to_string();

        let prs = tokio::task::spawn_blocking(move || {
            let conn = conn_cloned.lock().ok()?;
            let mut stmt = conn.prepare(
                "SELECT pullrequest_body FROM issuestore_pullrequest WHERE provider = ? AND organization = ? AND repo = ?",
            ).ok()?;

            let vec: Vec<Value> = stmt
                .query_map([provider, organization, repo], |row| {
                    let value_str: String = row.get(0)?;
                    let value: Value = serde_json::from_str(&value_str)
                        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
                    Ok(value)
                }).ok()?
                .flatten()
                .collect();

            Some(vec)
        }).await.ok()??;

        if prs.is_empty() {
            None
        } else {
            Some(prs)
        }
    }

    async fn save_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        pull_requests: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let conn_cloned = self.connection.clone();
        let provider = provider.to_string();
        let organization = organization.to_string();
        let repo = repo.to_string();
        let pull_requests = pull_requests.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn_cloned.lock().unwrap();

            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT OR IGNORE INTO issuestore_pullrequest (provider, organization, repo, pullrequest_body) VALUES (?, ?, ?, ?)",
                ).unwrap();
                for pr in pull_requests {
                    stmt.execute([
                        &provider,
                        &organization,
                        &repo,
                        serde_json::to_string(&pr).unwrap().as_str(),
                    ])?;
                }
            }

            tx.commit()
        }).await??;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[tokio::test]
    async fn it_stores_and_retrieves_the_issues() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        issue_store
            .save_issues("github", "rust-lang", "rust", &issues_in_repo())
            .await
            .unwrap();
        let issues = issue_store
            .get_issues("github", "rust-lang", "rust")
            .await
            .unwrap();

        assert_eq!(issues, issues_in_repo());
    }

    #[tokio::test]
    async fn it_stores_and_retrieves_the_pull_requests() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        issue_store
            .save_pull_requests("github", "rust-lang", "rust", &pull_requests_in_repo())
            .await
            .unwrap();
        let pull_requests = issue_store
            .get_pull_requests("github", "rust-lang", "rust")
            .await
            .unwrap();

        assert_eq!(pull_requests, pull_requests_in_repo());
    }

    #[tokio::test]
    async fn if_there_are_no_issues_it_returns_none() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        let issues = issue_store.get_issues("github", "unknown", "unknown").await;

        assert_eq!(issues, None);
    }

    #[tokio::test]
    async fn if_there_are_no_pull_requests_it_returns_none() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        let pull_requests = issue_store
            .get_pull_requests("github", "unknown", "unknown")
            .await;

        assert_eq!(pull_requests, None);
    }

    fn issues_in_repo() -> Vec<Value> {
        vec![
            Value::String("issue1".to_string()),
            Value::String("issue2".to_string()),
        ]
    }

    fn pull_requests_in_repo() -> Vec<Value> {
        vec![
            Value::String("pull_request1".to_string()),
            Value::String("pull_request2".to_string()),
        ]
    }
}
