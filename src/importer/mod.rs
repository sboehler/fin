use std::{error::Error, io::Write};

use clap::Subcommand;

pub mod postfinance;

#[derive(Subcommand)]
pub enum Commands {
    #[command(name = "ch.postfinance", about = "Import Postfinance CSV file.")]
    Postfinance(postfinance::Command),
}

impl Commands {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            Commands::Postfinance(command) => command.run(w),
        }
    }
}
