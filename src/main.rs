#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod factory;
mod infra;
mod pkg;

use std::fs::File;
use std::rc::Rc;

use anyhow::Context;
use log::{error, info, LevelFilter};
use rayon::prelude::*;

use crate::cmd::parse_args;
use crate::factory::Factory;
use crate::pkg::config::Config;
use crate::pkg::policy::{Evaluation, Policy};
use crate::pkg::recognizer::PackageManager;
use crate::pkg::Dependency;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Rc::new(parse_args());
    load_logger()?;
    let config = Config::load_from_default_file_path_or_default();
    let factory = Factory::new(config, args.clone());

    let lock_file = File::open(&args.lock_file)
        .map_err(|err| format!("file {} could not be opened: {}", &args.lock_file, err))?;
    let dependency_reader = factory.dependency_reader(lock_file);

    let _package_manager = PackageManager::from_filename(&args.lock_file).with_context(|| {
        format!(
            "unable to determine package manager for file: {}",
            &args.lock_file
        )
    })?;

    let policies = factory.policies();

    let dependencies = dependency_reader.dependencies()?;
    dependencies.into_par_iter().for_each(|dep| {
            let evaluation = check_if_dependency_is_okay(&policies, &dep);
            match evaluation {
                Evaluation::Pass => {
                    info!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is okay",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository
                    );
                }
                Evaluation::Fail(reason) => {
                    error!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is not okay: {}",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository, reason
                    );
                }
            }
        });

    Ok(())
}

fn check_if_dependency_is_okay(
    policies: &[Box<dyn Policy + Send + Sync>],
    dep: &Dependency,
) -> Evaluation {
    for policy in policies.iter() {
        match policy.evaluate(dep) {
            Ok(result) => match result {
                Evaluation::Pass => continue,
                Evaluation::Fail(reason) => {
                    return Evaluation::Fail(reason);
                }
            },
            Err(error) => {
                return Evaluation::Fail(error.to_string());
            }
        }
    }
    Evaluation::Pass
}

fn load_logger() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .with_colors(true)
        .env()
        .init()?;
    Ok(())
}
