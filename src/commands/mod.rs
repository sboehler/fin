use clap::Subcommand;

mod parse;
mod print;
mod syntax;

#[derive(Subcommand)]
pub enum Commands {
    Print(print::Command),
    Parse(parse::Command),
    Syntax(syntax::Command),
}
