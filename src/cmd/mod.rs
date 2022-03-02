use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, author, about, long_about = None)]
pub struct Args {}

pub fn parse_args() -> Args {
    Args::parse()
}
