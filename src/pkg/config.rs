use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Eq, PartialEq, Clone, Debug))]
pub struct Policy {
    pub name: String,
    pub params: HashMap<String, Value>,
    pub skip: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(Eq, PartialEq, Clone, Debug))]
pub struct Config {
    pub policies: Vec<Policy>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use expects::matcher::equal;
    use expects::Subject;
    use serde_yaml::Value;

    #[test]
    fn it_loads_the_config_from_reader() {
        let config: Config = serde_yaml::from_reader(config_example()).unwrap();

        config.should(equal(Config {
            policies: vec![
                Policy {
                    name: "ContributorsRatio".to_string(),
                    params: HashMap::from([
                        (
                            "max_number_of_releases_to_check".to_string(),
                            Value::from(3_usize),
                        ),
                        ("max_contributor_ratio".to_string(), Value::from(0.8_f64)),
                    ]),
                    skip: vec!["react-*".to_string()],
                },
                Policy {
                    name: "MinNumberOfReleasesRequired".to_string(),
                    params: HashMap::from([
                        ("min_number_of_releases".to_string(), Value::from(3_usize)),
                        ("days".to_string(), Value::from(180_usize)),
                    ]),
                    skip: vec!["react-*".to_string()],
                },
            ],
        }));
    }

    fn config_example() -> &'static [u8] {
        "\
policies:
- name: ContributorsRatio
  params:
    max_number_of_releases_to_check: 3
    max_contributor_ratio: 0.8
  skip:
  - react-*
- name: MinNumberOfReleasesRequired
  params:
    min_number_of_releases: 3
    days: 180
  skip:
  - react-*
"
        .as_bytes()
    }
}
