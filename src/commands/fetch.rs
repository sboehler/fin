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
    quotes::yahoo::{Client, Quote},
    syntax::parse_file,
};
use chrono::Days;
use clap::Args;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rust_decimal::{prelude::FromPrimitive, Decimal};
use serde::Deserialize;

#[derive(Args)]
pub struct Command {
    config: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        // set the rayon thread pool to 5 threads
        rayon::ThreadPoolBuilder::new()
            .num_threads(5)
            .build_global()
            .unwrap();
        let config = File::open(&self.config)?;
        let entries = serde_yaml::from_reader(config)?;
        let now = chrono::offset::Utc::now();
        let quotes = fetch_quotes(&entries, Client::default(), now)?;
        let directory = self
            .config
            .parent()
            .ok_or(format!("no parent for {:?}", self.config))?;
        for (entry, quotes) in entries.iter().zip(quotes) {
            write_quotes(directory, entry, quotes)?;
        }
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct ConfigEntry {
    pub commodity: String,
    pub target_commodity: String,
    pub file: PathBuf,
    pub symbol: String,
}

fn fetch_quotes(
    entries: &Vec<ConfigEntry>,
    client: Client,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Vec<Quote>>, String> {
    let bar = ProgressBar::new(u64::from_usize(entries.len()).unwrap()).with_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .expect("invalid template"),
    );
    entries
        .par_iter()
        .progress_with(bar.clone())
        .map(|config| {
            let one_year_ago = now.checked_sub_days(Days::new(365)).unwrap();
            bar.set_message(format!("fetching {}", config.symbol));
            client
                .fetch(&config.symbol, now, one_year_ago)
                .map_err(|e| format!("error fetching {}: {}", config.symbol, e))
        })
        .collect()
}

fn write_quotes(
    parent: &Path,
    entry: &ConfigEntry,
    quotes: Vec<Quote>,
) -> Result<(), Box<dyn Error>> {
    let path = parent.join(&entry.file);
    let mut journal = read_file(&path)?;
    add_quotes(&mut journal, entry, quotes)?;
    write_file(&path, &journal)?;
    Ok(())
}

fn read_file(path: &Path) -> Result<Journal, Box<dyn Error>> {
    let (tree, file) = parse_file(path)?;
    let journal = build_journal(&[(tree, file)])?;
    Ok(journal)
}

fn add_quotes(
    journal: &mut Journal,
    config: &ConfigEntry,
    quotes: Vec<Quote>,
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

fn write_file(path: &PathBuf, journal: &Journal) -> Result<(), Box<dyn Error>> {
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
