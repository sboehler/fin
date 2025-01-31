use std::{error::Error, fs::File, path::PathBuf};

use crate::quotes::yahoo::{self, Config};
use chrono::Days;
use clap::Args;
use rayon::prelude::*;
use yahoo::Client;

#[derive(Args)]
pub struct Command {
    config: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build_global()
            .unwrap();

        let config = File::open(&self.config)?;
        let parsed_configs: Vec<Config> = serde_yaml::from_reader(config)?;
        let client = Client::default();
        let res = parsed_configs[10..]
            .par_iter()
            .map(|config| {
                let t0 = chrono::offset::Utc::now();
                let t1 = chrono::offset::Utc::now()
                    .checked_sub_days(Days::new(365))
                    .unwrap();
                client
                    .fetch(&config.symbol, t0, t1)
                    .map_err(|e| format!("error fetching {}: {}", config.symbol, e))
            })
            .collect::<Result<Vec<_>, _>>()?;

        println!("{:?}", res);
        Ok(())
    }
}
