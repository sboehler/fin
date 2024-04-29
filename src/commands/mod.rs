use clap::Subcommand;

mod parse;
mod syntax;

#[derive(Subcommand)]
pub enum Commands {
    Parse(parse::Command),
    Syntax(syntax::Command),
}
