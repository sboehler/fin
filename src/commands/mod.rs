use clap::Subcommand;

use crate::importer;

mod balance;
mod fetch;
mod format;
mod infer;
mod parse;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Format(format::Command),
    Balance(balance::Command),
    Fetch(fetch::Command),
    Infer(infer::Command),

    #[command(subcommand)]
    Import(importer::Commands),
}
