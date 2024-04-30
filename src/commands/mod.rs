use clap::Subcommand;

mod format;
mod parse;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Format(format::Command),
}
