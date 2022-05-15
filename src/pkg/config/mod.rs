use serde::{Deserialize, Serialize};

mod contributors_ratio;
mod min_number_of_releases_required;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct Config {
    pub policies: Policies,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct Policies {
    pub contributors_ratio: contributors_ratio::Config,
    pub min_number_of_releases_required: min_number_of_releases_required::Config,
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
    use expects::matcher::equal;
    use expects::Subject;

    use super::*;

    #[test]
    fn it_loads_the_default_config_from_an_empty_file() {
        let config: Config = Config::load_from_reader(&mut "".as_bytes()).unwrap_or_default();
        config.should(equal(Config {
            policies: Policies {
                contributors_ratio: contributors_ratio::Config {
                    max_number_of_releases_to_check: 3_usize,
                    max_contributor_ratio: 0.5,
                    skip: vec![],
                    enabled: true,
                },
                min_number_of_releases_required: min_number_of_releases_required::Config {
                    min_number_of_releases: 3_usize,
                    days: 365_u64,
                    skip: vec![],
                    enabled: true,
                },
            },
        }));
    }

    #[test]
    fn it_loads_the_config_from_reader() {
        let config: Config = Config::load_from_reader(&mut config_example()).unwrap();
        config.should(equal(Config {
            policies: Policies {
                contributors_ratio: contributors_ratio::Config {
                    max_number_of_releases_to_check: 3_usize,
                    max_contributor_ratio: 0.8,
                    skip: vec!["react-*".to_string()],
                    enabled: true,
                },
                min_number_of_releases_required: min_number_of_releases_required::Config {
                    min_number_of_releases: 3_usize,
                    days: 180_u64,
                    skip: vec!["react-*".to_string()],
                    enabled: false,
                },
            },
        }));
    }

    #[test]
    fn it_dumps_the_config_to_string() {
        let config: Config = Config::default();
        let config_string = config.dump_to_string().unwrap();
        config_string.should(equal(
            "\
---
policies:
  contributors_ratio:
    max_number_of_releases_to_check: 3
    max_contributor_ratio: 0.5
    skip: []
    enabled: true
  min_number_of_releases_required:
    min_number_of_releases: 3
    days: 365
    skip: []
    enabled: true
",
        ));
    }

    fn config_example() -> &'static [u8] {
        "\
policies:
  contributors_ratio:
    max_number_of_releases_to_check: 3
    max_contributor_ratio: 0.8
    skip:
    - 'react-*'
  min_number_of_releases_required:
    min_number_of_releases: 3
    days: 180
    skip:
    - 'react-*'
    enabled: false
"
        .as_bytes()
    }
}
