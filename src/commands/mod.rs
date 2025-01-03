use clap::Subcommand;

mod balance;
mod format;
mod parse;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Format(format::Command),
    Balance(balance::Command),
}
