use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, author, about, long_about = None)]
pub struct Args {
    #[clap(short, long, default_value = "package-lock.json")]
    pub lock_file: String,
}

pub fn parse_args() -> Args {
    Args::parse()
}
