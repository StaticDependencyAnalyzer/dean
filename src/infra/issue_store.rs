use std::error::Error;
use std::sync::{Arc, Mutex};

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
        let conn = self.connection.lock().unwrap();
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

impl IssueStore for Sqlite {
    fn get_issues(&self, provider: &str, organization: &str, repo: &str) -> Option<Vec<Value>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT issue_body FROM issuestore_issue WHERE provider = ? AND organization = ? AND repo = ?",
        ).unwrap();

        let rows = stmt
            .query_map(&[provider, organization, repo], |row| {
                let value_str: String = row.get(0)?;
                let value: Value = serde_json::from_str(&value_str)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
                Ok(value)
            })
            .unwrap()
            .flatten();

        let issues: Vec<_> = rows.collect();
        if issues.is_empty() {
            None
        } else {
            Some(issues)
        }
    }

    fn save_issues(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        issues: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let mut conn = self.connection.lock().unwrap();

        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO issuestore_issue (provider, organization, repo, issue_body) VALUES (?, ?, ?, ?)",
            ).unwrap();
            for issue in issues {
                stmt.execute(&[
                    provider,
                    organization,
                    repo,
                    serde_json::to_string(issue).unwrap().as_str(),
                ])?;
            }
        }

        tx.commit().unwrap();
        Ok(())
    }

    fn get_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
    ) -> Option<Vec<Value>> {
        let conn = self.connection.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT pullrequest_body FROM issuestore_pullrequest WHERE provider = ? AND organization = ? AND repo = ?",
        ).unwrap();

        let rows = stmt
            .query_map(&[provider, organization, repo], |row| {
                let value_str: String = row.get(0)?;
                let value: Value = serde_json::from_str(&value_str)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
                Ok(value)
            })
            .unwrap()
            .flatten();

        let pull_requests_vec: Vec<_> = rows.collect();
        if pull_requests_vec.is_empty() {
            None
        } else {
            Some(pull_requests_vec)
        }
    }

    fn save_pull_requests(
        &self,
        provider: &str,
        organization: &str,
        repo: &str,
        pull_requests: &[Value],
    ) -> Result<(), Box<dyn Error>> {
        let mut conn = self.connection.lock().unwrap();

        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO issuestore_pullrequest (provider, organization, repo, pullrequest_body) VALUES (?, ?, ?, ?)",
            ).unwrap();
            for pr in pull_requests {
                stmt.execute(&[
                    provider,
                    organization,
                    repo,
                    serde_json::to_string(pr).unwrap().as_str(),
                ])?;
            }
        }

        tx.commit().unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn it_stores_and_retrieves_the_issues() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        issue_store
            .save_issues("github", "rust-lang", "rust", &issues_in_repo())
            .unwrap();
        let issues = issue_store
            .get_issues("github", "rust-lang", "rust")
            .unwrap();

        assert_eq!(issues, issues_in_repo());
    }

    #[test]
    fn it_stores_and_retrieves_the_pull_requests() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        issue_store
            .save_pull_requests("github", "rust-lang", "rust", &pull_requests_in_repo())
            .unwrap();
        let pull_requests = issue_store
            .get_pull_requests("github", "rust-lang", "rust")
            .unwrap();

        assert_eq!(pull_requests, pull_requests_in_repo());
    }

    #[test]
    fn if_there_are_no_issues_it_returns_none() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        let issues = issue_store.get_issues("github", "unknown", "unknown");

        assert_eq!(issues, None);
    }

    #[test]
    fn if_there_are_no_pull_requests_it_returns_none() {
        let connection = Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
        let issue_store = Sqlite::new(connection);
        issue_store.init().unwrap();

        let pull_requests = issue_store.get_pull_requests("github", "unknown", "unknown");

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
