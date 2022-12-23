use clap::Subcommand;

mod parse;
mod print;

#[derive(Subcommand)]
pub enum Commands {
    Print(print::Command),
    Parse(parse::Command),
}
