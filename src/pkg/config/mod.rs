use serde::{Deserialize, Serialize};

pub mod contributors_ratio;
pub mod max_issue_lifespan;
pub mod max_pull_request_lifespan;
pub mod min_number_of_releases_required;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct Config {
    #[serde(default)]
    pub default_policies: Policies,
    #[serde(default)]
    pub dependency_config: Vec<DependencyConfiguration>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_policies: Policies {
                min_number_of_releases_required: Some(
                    min_number_of_releases_required::Config::default(),
                ),
                contributors_ratio: Some(contributors_ratio::Config::default()),
                max_issue_lifespan: Some(max_issue_lifespan::Config::default()),
                max_pull_request_lifespan: Some(max_pull_request_lifespan::Config::default()),
            },
            dependency_config: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct DependencyConfiguration {
    pub name: String,
    pub policies: Policies,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct Policies {
    pub contributors_ratio: Option<contributors_ratio::Config>,
    pub min_number_of_releases_required: Option<min_number_of_releases_required::Config>,
    pub max_issue_lifespan: Option<max_issue_lifespan::Config>,
    pub max_pull_request_lifespan: Option<max_pull_request_lifespan::Config>,
}

impl Config {
    pub fn load_from_reader(
        reader: &mut dyn std::io::Read,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let result = serde_yaml::from_reader(reader)?;
        Ok(result)
    }

    pub fn dump_to_string(&self) -> Result<String, Box<dyn std::error::Error>> {
        let result = serde_yaml::to_string(&self)?;
        Ok(result)
    }

    fn default_config_file() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let home = dirs_next::home_dir().ok_or_else(|| { "Could not find home directory. Please set the environment variable HOME to your home directory.".to_string() })?;
        Ok(home.join(".config/dean.yaml"))
    }

    pub fn load_from_default_file_path_or_default() -> Self {
        match Self::default_config_file() {
            Ok(config_file) => match std::fs::File::open(&config_file) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_loads_the_default_config_from_an_empty_file() {
        let config: Config = Config::load_from_reader(&mut "".as_bytes()).unwrap_or_default();
        assert_eq!(
            config,
            Config {
                default_policies: Policies {
                    contributors_ratio: Some(contributors_ratio::Config {
                        max_number_of_releases_to_check: 3_usize,
                        max_contributor_ratio: 0.5,
                    }),
                    min_number_of_releases_required: Some(
                        min_number_of_releases_required::Config {
                            min_number_of_releases: 3_usize,
                            days: 365_u64,
                        }
                    ),
                    max_issue_lifespan: Some(max_issue_lifespan::Config {
                        max_lifespan_in_seconds: 2_592_000_usize,
                        last_issues: 300,
                    }),
                    max_pull_request_lifespan: Some(max_pull_request_lifespan::Config {
                        max_lifespan_in_seconds: 2_592_000_usize,
                        last_pull_requests: 300,
                    }),
                },
                dependency_config: vec![],
            }
        );
    }

    #[test]
    fn it_loads_the_config_from_reader() {
        let config: Config = Config::load_from_reader(&mut config_example()).unwrap();
        assert_eq!(
            config,
            Config {
                default_policies: Policies {
                    contributors_ratio: Some(contributors_ratio::Config {
                        max_number_of_releases_to_check: 3_usize,
                        max_contributor_ratio: 0.8,
                    }),
                    min_number_of_releases_required: Some(
                        min_number_of_releases_required::Config {
                            min_number_of_releases: 3_usize,
                            days: 180_u64,
                        }
                    ),
                    max_issue_lifespan: Some(max_issue_lifespan::Config {
                        max_lifespan_in_seconds: 2_592_000_usize,
                        last_issues: 300,
                    }),
                    max_pull_request_lifespan: Some(max_pull_request_lifespan::Config {
                        max_lifespan_in_seconds: 2_592_000_usize,
                        last_pull_requests: 300,
                    }),
                },
                dependency_config: vec![],
            }
        );
    }

    #[test]
    fn it_dumps_the_config_to_string() {
        let config: Config = Config::default();
        let config_string = config.dump_to_string().unwrap();
        assert_eq!(
            config_string,
            "\
---
default_policies:
  contributors_ratio:
    max_number_of_releases_to_check: 3
    max_contributor_ratio: 0.5
  min_number_of_releases_required:
    min_number_of_releases: 3
    days: 365
  max_issue_lifespan:
    max_lifespan_in_seconds: 2592000
    last_issues: 300
  max_pull_request_lifespan:
    max_lifespan_in_seconds: 2592000
    last_pull_requests: 300
dependency_config: []
"
        );
    }

    #[test]
    fn it_loads_the_config_with_a_missing_policy() {
        let config: Config =
            Config::load_from_reader(&mut config_example_with_missing_policy()).unwrap();
        assert_eq!(
            config,
            Config {
                default_policies: Policies {
                    contributors_ratio: Some(contributors_ratio::Config {
                        max_number_of_releases_to_check: 3_usize,
                        max_contributor_ratio: 0.5,
                    }),
                    min_number_of_releases_required: None,
                    max_issue_lifespan: None,
                    max_pull_request_lifespan: None,
                },
                dependency_config: vec![],
            }
        );
    }

    #[test]
    fn it_loads_the_config_for_a_specific_policy() {
        let config = Config::load_from_reader(&mut config_example_for_specific_policy()).unwrap();
        assert_eq!(
            config,
            Config {
                default_policies: Policies {
                    contributors_ratio: None,
                    min_number_of_releases_required: None,
                    max_issue_lifespan: None,
                    max_pull_request_lifespan: None,
                },
                dependency_config: vec![
                    DependencyConfiguration {
                        name: "foo".to_string(),
                        policies: Policies {
                            contributors_ratio: Some(contributors_ratio::Config {
                                max_number_of_releases_to_check: 3_usize,
                                max_contributor_ratio: 0.8,
                            }),
                            min_number_of_releases_required: Some(
                                min_number_of_releases_required::Config {
                                    min_number_of_releases: 3_usize,
                                    days: 180_u64,
                                },
                            ),
                            max_issue_lifespan: Some(max_issue_lifespan::Config {
                                max_lifespan_in_seconds: 2_592_000_usize,
                                last_issues: 300,
                            }),
                            max_pull_request_lifespan: Some(max_pull_request_lifespan::Config {
                                max_lifespan_in_seconds: 2_592_000_usize,
                                last_pull_requests: 300,
                            }),
                        },
                    },
                    DependencyConfiguration {
                        name: "bar".to_string(),
                        policies: Policies {
                            contributors_ratio: Some(contributors_ratio::Config {
                                max_number_of_releases_to_check: 5_usize,
                                max_contributor_ratio: 0.5,
                            }),
                            min_number_of_releases_required: None,
                            max_issue_lifespan: None,
                            max_pull_request_lifespan: None,
                        },
                    },
                ],
            }
        );
    }

    fn config_example_for_specific_policy() -> &'static [u8] {
        "\
dependency_config:
- name: foo
  policies:
    contributors_ratio:
      max_number_of_releases_to_check: 3
      max_contributor_ratio: 0.8
    min_number_of_releases_required:
      min_number_of_releases: 3
      days: 180
    max_issue_lifespan:
      max_lifespan_in_seconds: 2592000
      last_issues: 300
    max_pull_request_lifespan:
      max_lifespan_in_seconds: 2592000
      last_issues: 300
- name: bar
  policies:
    contributors_ratio:
      max_number_of_releases_to_check: 5
      max_contributor_ratio: 0.5
"
        .as_bytes()
    }

    fn config_example_with_missing_policy() -> &'static [u8] {
        "\
default_policies:
    contributors_ratio:
        max_number_of_releases_to_check: 3
        max_contributor_ratio: 0.5
"
        .as_bytes()
    }

    fn config_example() -> &'static [u8] {
        "\
default_policies:
  contributors_ratio:
    max_number_of_releases_to_check: 3
    max_contributor_ratio: 0.8
  min_number_of_releases_required:
    min_number_of_releases: 3
    days: 180
  max_issue_lifespan:
    max_lifespan_in_seconds: 2592000
    last_issues: 300
  max_pull_request_lifespan:
    max_lifespan_in_seconds: 2592000
    last_issues: 300
"
        .as_bytes()
    }
}
