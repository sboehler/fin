use clap::{command, Parser};
use fin::commands;
use std::error::Error;

#[derive(Parser)]
#[command(name = "fin")]
#[command(author = "Silvio Böhler")]
#[command(version = "0.0.1")]
#[command(about = "Command line accounting tool.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: commands::Commands,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match &cli.command {
        commands::Commands::Parse(p) => p.run(),
        commands::Commands::Format(p) => p.run(),
    }
}
