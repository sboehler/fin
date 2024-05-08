use clap::Subcommand;

mod check;
mod format;
mod parse;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Format(format::Command),
    Check(check::Command),
}
