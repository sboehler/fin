use std::error::Error;

use clap::Subcommand;

pub mod postfinance;

#[derive(Subcommand)]
pub enum Commands {
    #[command(name = "ch.postfinance", about = "Import Postfinance CSV file.")]
    Postfinance(postfinance::Command),
}

impl Commands {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        match self {
            Commands::Postfinance(command) => command.run(),
        }
    }
}
