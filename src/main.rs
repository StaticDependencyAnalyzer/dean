#![forbid(unsafe_code)]
#![deny(clippy::pedantic, clippy::cargo)]

mod cmd;
mod infra;
mod pkg;

use std::error::Error;
use std::fs::File;

use cmd::parse_args;
use infra::npm::DependencyInfoRetriever;
use pkg::npm::DependencyReader;

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args();

    let retriever = Box::new(DependencyInfoRetriever::default());
    let reader = DependencyReader::new(retriever);

    let file = File::open(&args.lock_file)
        .map_err(|err| format!("file {} could not be opened: {}", &args.lock_file, err))?;

    reader.retrieve_from_reader(file).map(|x| {
        for dep in &x {
            println!(
                "{}: {} ({} latest: {})",
                dep.name,
                dep.version,
                if dep.version == dep.latest_version {
                    "✅"
                } else {
                    "️⚠️"
                },
                dep.latest_version,
            );
        }
    })?;

    Ok(())
}
