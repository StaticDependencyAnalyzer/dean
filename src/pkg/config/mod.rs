use serde::{Deserialize, Serialize};
use std::error::Error;

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
    pub fn load_from_reader(reader: &mut dyn std::io::Read) -> Result<Self, Box<dyn Error>> {
        let result = serde_yaml::from_reader(reader)?;
        Ok(result)
    }

    pub fn dump_to_string(&self) -> Result<String, Box<dyn Error>> {
        let result = serde_yaml::to_string(&self)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expects::matcher::equal;
    use expects::Subject;

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
                    days: 180_usize,
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
                    days: 180_usize,
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
    days: 180
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
