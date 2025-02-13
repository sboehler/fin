use std::{error::Error, path::PathBuf};

use clap::Args;

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    #[arg(short, long)]
    account: String,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        println!("Not implemented!");
        Ok(())
    }
}
