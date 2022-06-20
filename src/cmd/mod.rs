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
        about = "Scans the dependencies of a given lock file. Supported lock files are: `Cargo.lock`, `package-lock.json` and `yarn.lock`"
    )]
    Scan {
        #[clap(long, short, default_value = "Cargo.lock")]
        lock_file: String,
    },

    #[clap(about = "Manages the configuration of the tool.")]
    #[clap(arg_required_else_help(true))]
    Config {
        #[clap(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    #[clap(about = "Displays the current configuration")]
    Show,
}

pub fn parse_args() -> Args {
    Args::parse()
}
