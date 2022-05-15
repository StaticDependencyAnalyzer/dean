use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(version, author, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[clap(
        about = "Scans the dependencies of a given lock file. Supported lock files are: `Cargo.lock` and `package-lock.json`"
    )]
    Scan {
        #[clap(long, short, default_value = "Cargo.lock")]
        lock_file: String,
    },
}

pub fn parse_args() -> Args {
    Args::parse()
}
