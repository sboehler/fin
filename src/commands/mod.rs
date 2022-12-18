use clap::Subcommand;

pub mod print;

#[derive(Subcommand)]
pub enum Commands {
    Print(print::Command),
}
