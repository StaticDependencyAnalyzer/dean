mod cmd;
mod infra;
mod pkg;

use cmd::parse_args;

fn main() {
    let args = parse_args();
    println!("{:?}", args);
}
