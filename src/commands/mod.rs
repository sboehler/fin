use clap::Subcommand;

use crate::importer;

mod balance;
mod crossvalidate;
mod fetch;
mod format;
mod infer;
mod parse;
mod review;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Format(format::Command),
    Balance(balance::Command),
    Fetch(fetch::Command),
    Infer(infer::Command),
    Review(review::Command),

    #[command(subcommand)]
    Import(importer::Commands),
}
