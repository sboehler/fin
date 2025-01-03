use clap::{command, Parser};
use fin::commands;

#[derive(Parser)]
#[command(name = "fin")]
#[command(author = "Silvio Böhler")]
#[command(version = "0.0.1")]
#[command(about = "Command line accounting tool.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: commands::Commands,
}

fn main() {
    let cli = Cli::parse();
    let r = match &cli.command {
        commands::Commands::Parse(p) => p.run(),
        commands::Commands::Format(p) => p.run(),
        commands::Commands::Balance(p) => p.run(),
    };
    if let Err(e) = r {
        println!("{}", e);
        std::process::exit(1)
    };
}
