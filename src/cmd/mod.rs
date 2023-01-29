use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(version, author, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,

    #[clap(
        global = true,
        short = 'v',
        default_value = "error",
        help = "Valid values, from more verbose to less are: trace, debug, info, warn, error, off"
    )]
    pub log_level: String,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[clap(about = "Scans the dependencies of a given lock file.")]
    Scan {
        #[clap(
            long,
            short,
            default_value = "Cargo.lock",
            help = "Lock file where the dependencies are defined. Supported locks are: Cargo.lock, package-lock.json and yarn.lock"
        )]
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
