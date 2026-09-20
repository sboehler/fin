use std::io::Write;

use clap::Parser;
use fin::commands;

#[derive(Parser)]
#[command(name = "fin")]
#[command(author = "Silvio Böhler")]
#[command(version = env!("CARGO_PKG_VERSION"))]
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
        commands::Commands::Fetch(p) => p.run(),
        commands::Commands::Infer(p) => p.run(),
        commands::Commands::Review(p) => p.run(),
        commands::Commands::Chart(p) => {
            let mut stdout = std::io::stdout().lock();
            p.run(&mut stdout).and_then(|_| Ok(stdout.flush()?))
        }
        commands::Commands::Import(importer) => {
            let mut stdout = std::io::stdout().lock();
            importer.run(&mut stdout).and_then(|_| Ok(stdout.flush()?))
        }
    };
    if let Err(e) = r {
        println!("{e}");
        std::process::exit(1)
    };
}
