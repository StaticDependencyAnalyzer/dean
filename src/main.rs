#![deny(clippy::pedantic, clippy::style)]
#![deny(unused)]

mod cmd;
mod factory;
mod infra;
mod lazy;
mod pkg;

pub type Result<T, E = anyhow::Error> = core::result::Result<T, E>;

use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;
use futures::future::join_all;
use log::{error, info, warn, LevelFilter};
use tokio::fs::File;
use tokio_stream::StreamExt;

use crate::cmd::{parse_args, Commands, ConfigCommands};
use crate::factory::Factory;
use crate::pkg::config::Config;
use crate::pkg::policy::{Evaluation, Policy};
use crate::pkg::{Dependency, ResultReporter};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = parse_args();
    load_logger(&args.log_level)?;

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

async fn scan_lock_file(factory: &mut Factory, lock_file_name: &str) -> Result<()> {
    let lock_file = File::open(lock_file_name)
        .await
        .with_context(|| format!("failed to open lock file: {lock_file_name}"))?;
    let mut reporter = Factory::result_reporter();
    let mut dependency_reader = factory.dependency_reader(lock_file, lock_file_name).await;

    let engine = Arc::new(factory.engine()?);

    let mut async_results = Vec::new();

    while let Some(dep) = dependency_reader.next().await {
        let engine = engine.clone();
        let task = tokio::spawn(async move {
            let evaluations = engine.evaluate(&dep).await;
            if let Err(err) = evaluations {
                error!("error evaluating dependency {}: {}", dep.name, err);
                return None;
            }

            for evaluation in evaluations.as_ref().unwrap() {
                match evaluation {
                    Evaluation::Pass {
                        policy_name,
                        dependency,
                    } => {
                        info!(
                        "dependency [name={}, version={}, latest version={}, repository={}, policy={}] is okay",
                        dependency.name, dependency.version, dependency.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dependency.repository, policy_name
                    );
                    }
                    Evaluation::Fail {
                        policy_name,
                        dependency,
                        reason,
                        fail_score,
                    } => {
                        warn!(
                        "dependency [name={}, version={}, latest version={}, repository={}, policy={}] is not okay: {} (score: {})",
                        dependency.name, dependency.version, dependency.latest_version.as_ref().unwrap_or(&"unknown".to_string()), dependency.repository, policy_name, reason, fail_score,
                    );
                    }
                }
            }

            Some(evaluations.unwrap())
        });
        async_results.push(task);
    }

    let async_results = join_all(async_results).await;
    let sequential_results = async_results.into_iter().flatten().flatten().flatten();
    reporter.report_results(sequential_results).await?;

    Ok(())
}

fn load_logger(level: &str) -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Error)
        .with_module_level("dean", LevelFilter::from_str(level)?)
        .with_colors(true)
        .env()
        .init()?;
    Ok(())
}
