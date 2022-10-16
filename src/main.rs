#![deny(clippy::pedantic, clippy::style)]
#![warn(unused)]

mod cmd;
mod factory;
mod infra;
mod lazy;
mod pkg;

use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;

use log::{error, info, warn, LevelFilter};
use tokio::fs::File;

use crate::cmd::{parse_args, Commands, ConfigCommands};
use crate::factory::Factory;
use crate::pkg::config::Config;
use crate::pkg::policy::{Evaluation, Policy};
use crate::pkg::{Dependency, ResultReporter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    load_logger()?;
    let config = Rc::new(Config::load_from_default_file_path_or_default().await);
    let mut factory = Factory::new(config.clone());

    match &args.command {
        Commands::Scan { lock_file } => {
            scan_lock_file(&mut factory, lock_file).await?;
        }
        Commands::Config { command } => match command {
            ConfigCommands::Show => {
                println!("{}", config.dump_to_string()?);
            }
        },
    }

    Ok(())
}

async fn scan_lock_file(factory: &mut Factory, lock_file_name: &str) -> Result<(), Box<dyn Error>> {
    let lock_file = File::open(lock_file_name)
        .await
        .map_err(|err| format!("file {} could not be opened: {}", lock_file_name, err))?;
    let mut reporter = Factory::result_reporter();
    let dependency_reader = factory.dependency_reader(lock_file, lock_file_name).await;

    let engine = Arc::new(factory.engine()?);

    let async_results = dependency_reader.map(move |dep| {
        let engine = engine.clone();
        tokio::spawn(async move {
            let evaluations = engine.evaluate(&dep);
            if let Err(err) = evaluations {
                error!("error evaluating dependency {}: {}", dep.name, err);
                return None;
            }

            for evaluation in evaluations.as_ref().unwrap() {
                match evaluation {
                    Evaluation::Pass(policy, dep) => {
                        info!(
                        "dependency [name={}, version={}, latest version={}, repository={}, policy={}] is okay",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository, policy
                    );
                    }
                    Evaluation::Fail(policy, dep, reason, score) => {
                        warn!(
                        "dependency [name={}, version={}, latest version={}, repository={}, policy={}] is not okay: {} (score: {})",
                        dep.name, dep.version, dep.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dep.repository, policy, reason, score,
                    );
                    }
                }
            }

            Some(evaluations.unwrap())
        })
    });
    let mut results = Vec::new();
    for async_result in async_results {
        results.push(async_result.await);
    }

    let sequential_results = results.into_iter().flatten().flatten().flatten();
    reporter.report_results(sequential_results).await?;

    Ok(())
}

fn load_logger() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Off)
        .with_module_level("dean", LevelFilter::Debug)
        .with_colors(true)
        .env()
        .init()?;
    Ok(())
}
