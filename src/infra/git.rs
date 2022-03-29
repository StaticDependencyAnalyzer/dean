use crate::pkg::policy::{Commit, CommitRetriever, Tag};
use anyhow::Context;
use git2::Oid;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;

pub struct RepositoryRetriever {}

impl CommitRetriever for RepositoryRetriever {
    fn commits_for_each_tag(
        &self,
        repository_url: &str,
    ) -> Result<HashMap<String, Vec<Commit>>, Box<dyn Error>> {
        Repository::new(repository_url)
            .map_err(|e| format!("unable to create repository: {}", e))?
            .commits_for_each_tag()
    }

    fn all_tags(&self, repository_url: &str) -> Result<Vec<Tag>, Box<dyn Error>> {
        Repository::new(repository_url)
            .map_err(|e| format!("unable to create repository: {}", e))?
            .all_tags()
    }
}

pub struct Repository {
    repository: git2::Repository,
    #[allow(unused)]
    temp_dir: tempfile::TempDir,
}

impl Repository {
    pub fn new(url: &str) -> Result<Self, Box<dyn Error>> {
        let temp_dir = tempfile::tempdir()?;
        let repository = git2::Repository::clone(url, temp_dir.path())?;

        Ok(Repository {
            repository,
            temp_dir,
        })
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
                        .map(|commit_id| self.commit_from_id(Cow::from(commit_id)))
                        .filter(std::result::Result::is_ok)
                        .map(std::result::Result::unwrap)
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
                if let Some(commit) = obj.as_commit() {
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

    fn all_commits(&self) -> Result<Vec<Commit>, Box<dyn Error>> {
        let mut revwalk = self
            .repository
            .revwalk()
            .with_context(|| "unable to create a revwalk for repository")?;
        revwalk
            .push_head()
            .with_context(|| "unable to push head to revwalk".to_string())?;

        let commits = revwalk
            .into_iter()
            .map(|oid| {
                self.repository
                    .find_commit(oid.unwrap())
                    .expect("unable to obtain commit from Oid")
            })
            .map(|commit| Commit {
                id: commit.id().to_string(),
                author_email: commit.author().email().unwrap().into(),
                author_name: commit.author().name().unwrap().into(),
                creation_timestamp: commit.time().seconds(),
            })
            .collect::<Vec<_>>();

        Ok(commits)
    }

    fn commit_ids_for_each_tag(&self) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
        let mut result = HashMap::new();

        let tags: Vec<_> = self.all_tags()?.into_iter().rev().collect();
        let mut commit_buffer = Vec::new();
        for i in 0..tags.len() - 1 {
            let first_tag = tags.get(i).unwrap();
            let second_tag = tags.get(i + 1).unwrap();
            let first_oid = Oid::from_str(&first_tag.commit_id)?;
            let second_oid = Oid::from_str(&second_tag.commit_id)?;

            let mut revwalk = self.repository.revwalk()?;
            revwalk.push(first_oid)?;
            for oid in revwalk
                .filter(std::result::Result::is_ok)
                .map(std::result::Result::unwrap)
            {
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
            .with_context(|| "unable to retrieve author email".to_string())?
            .to_string();
        let author_name = commit
            .author()
            .name()
            .with_context(|| "unable to retrieve author name".to_string())?
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
    use expects::matcher::{consist_of, contain_element, equal};
    use expects::Subject;

    #[test]
    fn it_retrieves_the_commits_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2").unwrap();

        let commits = repository.all_commits().unwrap();

        assert!(commits.len() >= 14483);
        commits.should(contain_element(Commit {
            id: "2a0d0bd19b5d13e2ab7f3780e094404828cbb9a7".into(),
            author_name: "Edward Thomson".into(),
            author_email: "ethomson@edwardthomson.com".into(),
            creation_timestamp: 1_646_268_794,
        }));
    }

    #[test]
    fn it_retrieves_the_tags_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2").unwrap();

        let tags = repository.all_tags().unwrap();

        assert!(tags.len() >= 76);
        tags.should(contain_element(Tag {
            name: "v1.4.2".to_string(),
            commit_id: "182d0d1ee933de46bf0b5a6ec269bafa77aba9a2".to_string(),
            commit_timestamp: 1_645_905_004,
        }));
    }

    #[test]
    fn it_retrieves_commit_ids_for_each_tag_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2").unwrap();

        let commit_ids_for_tag = repository.commit_ids_for_each_tag().unwrap();

        commit_ids_for_tag
            .get("v1.4.2")
            .unwrap()
            .should(consist_of(&[
                "43bfa124c844288a9e2e361e1122cc1cc51f1e8f".to_string(),
                "5d9f2aff9423a0395fd909312e2cfd7085552fd8".to_string(),
                "377ec9bfe7d84aad1ac23206144b7cdb7f867df2".to_string(),
                "f2c5d1b105d07c3643d1af388715321bdcbd83db".to_string(),
                "970c3c71cefd764857a57b6d9f04e147ec3114b6".to_string(),
                "182d0d1ee933de46bf0b5a6ec269bafa77aba9a2".to_string(),
            ]));
        commit_ids_for_tag
            .get("v1.4.0")
            .unwrap()
            .len()
            .should(equal(302_usize));
    }

    #[test]
    fn it_retrieves_commit_for_each_tag_of_a_repository() {
        let repository = Repository::new("https://github.com/libgit2/libgit2").unwrap();

        let commits_for_each_tag = repository.commits_for_each_tag().unwrap();

        commits_for_each_tag
            .get("v1.4.2")
            .unwrap()
            .should(contain_element(Commit {
                id: "43bfa124c844288a9e2e361e1122cc1cc51f1e8f".to_string(),
                author_name: "Carlos Martín Nieto".to_string(),
                author_email: "carlosmn@github.com".to_string(),
                creation_timestamp: 1_645_898_340,
            }));
        commits_for_each_tag
            .get("v1.4.2")
            .unwrap()
            .len()
            .should(equal(6_usize));
    }
}
