#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod factory;
mod infra;
mod pkg;

use std::error::Error;
use std::fs::File;
use std::rc::Rc;

use log::{info, warn, LevelFilter};
use rayon::prelude::*;

use crate::cmd::{parse_args, Commands, ConfigCommands};
use crate::factory::Factory;
use crate::pkg::config::Config;
use crate::pkg::policy::{Evaluation, Policy};
use crate::pkg::{Dependency, ResultReporter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    load_logger()?;
    let config = Rc::new(Config::load_from_default_file_path_or_default());
    let factory = Factory::new(config.clone());

    match &args.command {
        Commands::Scan { lock_file } => {
            scan_lock_file(&factory, lock_file)?;
        }
        Commands::Config { command } => match command {
            ConfigCommands::Show => {
                println!("{}", config.dump_to_string()?);
            }
        },
    }

    Ok(())
}

fn scan_lock_file(factory: &Factory, lock_file_name: &str) -> Result<(), Box<dyn Error>> {
    let lock_file = File::open(lock_file_name)
        .map_err(|err| format!("file {} could not be opened: {}", lock_file_name, err))?;
    let mut reporter = Factory::result_reporter();
    let dependency_reader = Factory::dependency_reader(lock_file, lock_file_name);

    let policies = factory.policies();

    let results = dependency_reader.par_bridge().map(|dep| {
        let evaluation = check_if_dependency_is_okay(&policies, dep);
        match &evaluation {
            Evaluation::Pass(dep) => {
                info!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is okay",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository
                    );
            }
            Evaluation::Fail(dep, reason) => {
                warn!(
                        "dependency [name={}, version={}, latest version={}, repository={}] is not okay: {}",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository, reason
                    );
            }
        }
        evaluation
    }).collect::<Vec<_>>();

    reporter.report_results(results)?;

    Ok(())
}

fn check_if_dependency_is_okay(
    policies: &[Box<dyn Policy + Send + Sync>],
    dep: Dependency,
) -> Evaluation {
    for policy in policies.iter() {
        match policy.evaluate(&dep) {
            Ok(result) => match result {
                Evaluation::Pass(_) => continue,
                Evaluation::Fail(_, reason) => {
                    return Evaluation::Fail(dep, reason);
                }
            },
            Err(error) => {
                return Evaluation::Fail(dep, error.to_string());
            }
        }
    }
    Evaluation::Pass(dep)
}

fn load_logger() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .with_colors(true)
        .env()
        .init()?;
    Ok(())
}
