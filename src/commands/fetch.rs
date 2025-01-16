use std::{error::Error, fs::File, path::PathBuf};

use clap::Args;
use serde::Deserialize;

#[derive(Args)]
pub struct Command {
    config: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let config = File::open(&self.config)?;
        let result: Vec<Config> = serde_yaml::from_reader(config)?;
        println!("{:?}", result);
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    commodity: String,
    target_commodity: String,
    file: String,
    symbol: String,
}
