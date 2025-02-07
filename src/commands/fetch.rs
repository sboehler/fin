use std::{
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use crate::{
    model::{
        build_journal,
        entities::Price,
        journal::{self, Journal},
        printing::Printer,
    },
    quotes::yahoo::{self, Config},
    syntax::parse_file,
};
use chrono::Days;
use clap::Args;
use rayon::prelude::*;
use rust_decimal::{prelude::FromPrimitive, Decimal};
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
        let quotes = parsed_configs
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
        for (config, quotes) in parsed_configs.iter().zip(quotes) {
            let path = &self.config.parent().unwrap().join(&config.file);
            let mut journal = Self::read_file(&self.config.parent().unwrap().join(&config.file))?;
            Self::add_quotes(&mut journal, &parsed_configs[0], quotes)?;
            Self::write_file(path, &journal)?;
        }
        Ok(())
    }

    pub fn read_file(path: &Path) -> Result<Journal, Box<dyn Error>> {
        let (tree, file) = parse_file(path)?;
        let journal = build_journal(&[(tree, file)])?;
        Ok(journal)
    }

    pub fn add_quotes(
        journal: &mut Journal,
        config: &Config,
        quotes: Vec<yahoo::Quote>,
    ) -> Result<(), Box<dyn Error>> {
        let commodity = journal.registry.commodity_id(&config.commodity)?;
        let target = journal.registry.commodity_id(&config.target_commodity)?;
        quotes
            .into_iter()
            .map(|q| Price {
                loc: None,
                date: q.date,
                commodity,
                price: Decimal::from_f64(q.close).unwrap().round_sf(10).unwrap(),
                target,
            })
            .for_each(|price| {
                let day = journal
                    .days
                    .entry(price.date)
                    .or_insert_with(|| journal::Day::new(price.date));
                day.prices = vec![price];
            });
        Ok(())
    }

    pub fn write_file(path: &PathBuf, journal: &Journal) -> Result<(), Box<dyn Error>> {
        let file = File::create(path)?;
        let mut buf_writer = BufWriter::new(file);
        let mut printer = Printer::new(&mut buf_writer, journal.registry.clone());
        journal
            .days
            .iter()
            .flat_map(|d| d.1.prices.iter())
            .try_for_each(|p| printer.price(p))?;
        buf_writer.flush()?;
        Ok(())
    }
}
