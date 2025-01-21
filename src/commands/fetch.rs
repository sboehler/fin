use std::{error::Error, fs::File, path::PathBuf};

use chrono::Days;
use clap::Args;
use yahoo::Client;

use crate::quotes::yahoo::{self, Config};

#[derive(Args)]
pub struct Command {
    config: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let config = File::open(&self.config)?;
        let parsed_configs: Vec<Config> = serde_yaml::from_reader(config)?;
        let client = Client::new();
        let parsed_config = &parsed_configs[10];
        let res = client.fetch(
            &parsed_config.symbol,
            chrono::offset::Utc::now(),
            chrono::offset::Utc::now()
                .checked_sub_days(Days::new(365))
                .unwrap(),
        );
        println!("{:?}", res);
        Ok(())
    }
}
