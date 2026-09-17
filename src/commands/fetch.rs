use std::{
    collections::BTreeMap,
    error::Error,
    fmt::Display,
    fs::File,
    io::{BufWriter, Write},
    ops::Bound,
    path::{Path, PathBuf},
};

use crate::{
    model::{
        build_journal,
        entities::{CommodityID, Price},
        journal::Journal,
        printer::Printer,
    },
    quotes::yahoo::{Client, Quote},
    syntax::parse_file,
};
use chrono::{Days, NaiveDate};
use clap::Args;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rust_decimal::{Decimal, prelude::FromPrimitive};
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
        let entries: Vec<ConfigEntry> = serde_yaml::from_reader(config)?;
        let now = chrono::offset::Utc::now();
        let quotes = fetch_quotes(&entries, Client::default(), now);
        let directory = self
            .config
            .parent()
            .ok_or(format!("no parent for {:?}", self.config))?;
        // A symbol which cannot be fetched - delisted, misspelled, or a
        // request the API refused - must not cost the quotes of all the
        // others. Every symbol which was fetched is written, and the ones
        // which failed are reported together at the end.
        let mut failures = Vec::new();
        for (entry, quotes) in entries.iter().zip(quotes) {
            let written =
                quotes.and_then(|quotes| write_quotes(directory, entry, quotes).map_err(err));
            match written {
                // A split is not an error, but it rewrites prices which were
                // not fetched, so it is reported rather than done quietly.
                Ok(Some((split, rescaled))) => {
                    println!("{}: {}", entry.symbol, split.describe(entry, rescaled))
                }
                Ok(None) => (),
                Err(e) => failures.push(format!("{}: {e}", entry.symbol)),
            }
        }
        if !failures.is_empty() {
            return Err(format!(
                "{} of {} symbols failed:\n{}",
                failures.len(),
                entries.len(),
                failures.join("\n")
            )
            .into());
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

/// Fetches the quotes of the last year for every entry, in the order of
/// `entries`, reporting per entry whether it could be fetched.
fn fetch_quotes(
    entries: &[ConfigEntry],
    client: Client,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<Result<Vec<Quote>, String>> {
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
            client.fetch(&config.symbol, one_year_ago, now).map_err(err)
        })
        .collect()
}

/// Failures are collected rather than returned, and `Box<dyn Error>` is
/// neither `Send` nor worth keeping around, so they are kept as messages.
fn err(e: Box<dyn Error>) -> String {
    e.to_string()
}

/// Writes the fetched quotes to the entry's file. If the prices already in it
/// say there was a stock split, the ones the fetch does not restate itself are
/// brought onto the new scale, so that the file is left on one scale
/// throughout. The comparison happens before the file is written, which is the
/// last moment the old prices are known.
fn write_quotes(
    parent: &Path,
    entry: &ConfigEntry,
    quotes: Vec<Quote>,
) -> Result<Option<(Split, usize)>, Box<dyn Error>> {
    let path = parent.join(&entry.file);
    let mut journal = read_file(&path)?;
    let commodity = journal.registry().commodity_id(&entry.commodity)?;
    let target = journal.registry().commodity_id(&entry.target_commodity)?;

    let existing = prices(&journal, commodity, target);
    let fetched = quotes
        .iter()
        .map(|q| {
            let price = price(q.close).ok_or_else(|| format!("invalid price {}", q.close))?;
            Ok((q.date, price))
        })
        .collect::<Result<BTreeMap<_, _>, Box<dyn Error>>>()?;

    let split = detect_split(&existing, &fetched)
        .map(|split| {
            let rescaled = rescale(&mut journal, commodity, target, &split, &existing, &fetched)?;
            Ok::<_, Box<dyn Error>>((split, rescaled))
        })
        .transpose()?;

    add_quotes(&mut journal, commodity, target, fetched);
    write_file(&path, &journal)?;
    Ok(split)
}

/// Divides the prices which predate `split` by its factor, leaving out the
/// ones the fetch restates itself, and returns how many were changed.
///
/// Only the prices which are known to predate the split are touched: the
/// fetch reaches back a year, while the file goes back as far as the
/// commodity has been held.
fn rescale(
    journal: &mut Journal,
    commodity: CommodityID,
    target: CommodityID,
    split: &Split,
    existing: &BTreeMap<NaiveDate, Decimal>,
    fetched: &BTreeMap<NaiveDate, Decimal>,
) -> Result<usize, Box<dyn Error>> {
    let before = match split.effective {
        Effective::On(date) => (Bound::Unbounded, Bound::Excluded(date)),
        // The split is only known to be later than that date, so the price of
        // the date itself is still on the old scale.
        Effective::After(date) => (Bound::Unbounded, Bound::Included(date)),
    };
    let dates = existing
        .range(before)
        .map(|(date, _)| *date)
        .filter(|date| !fetched.contains_key(date))
        .collect::<Vec<_>>();
    let mut rescaled = 0;
    for date in dates {
        for price in &mut journal.day(date).prices {
            if price.commodity != commodity || price.target != target {
                continue;
            }
            price.price = price
                .price
                .checked_div(split.factor)
                .and_then(|price| price.round_sf(10))
                .ok_or_else(|| format!("cannot divide the price on {date} by {}", split.factor))?;
            rescaled += 1;
        }
    }
    Ok(rescaled)
}

/// The prices of one commodity already in the journal, by date.
fn prices(
    journal: &Journal,
    commodity: CommodityID,
    target: CommodityID,
) -> BTreeMap<NaiveDate, Decimal> {
    journal
        .values()
        .flat_map(|day| &day.prices)
        .filter(|price| price.commodity == commodity && price.target == target)
        .map(|price| (price.date, price.price))
        .collect()
}

/// The price as it is written to the journal. Yahoo reports more digits than
/// a price is worth, and the comparison against the file has to be against
/// the value which would be written to it.
fn price(close: f64) -> Option<Decimal> {
    Decimal::from_f64(close)?.round_sf(10)
}

fn read_file(path: &Path) -> Result<Journal, Box<dyn Error>> {
    let (tree, file) = parse_file(path)?;
    let journal = build_journal(&[(tree, file)])?;
    Ok(journal)
}

fn add_quotes(
    journal: &mut Journal,
    commodity: CommodityID,
    target: CommodityID,
    fetched: BTreeMap<NaiveDate, Decimal>,
) {
    for (date, price) in fetched {
        journal.day(date).prices = vec![Price {
            loc: None,
            date,
            commodity,
            price,
            target,
        }];
    }
}

fn write_file(path: &PathBuf, journal: &Journal) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let mut buf_writer = BufWriter::new(file);
    let mut printer = Printer::new(&mut buf_writer, journal.registry().clone());
    for day in journal.values() {
        for price in &day.prices {
            printer.price(price)?;
        }
    }
    buf_writer.flush()?;
    Ok(())
}

/// Yahoo reports the prices from before a stock split divided by the split
/// factor, so a file fetched before one and the quotes fetched after it
/// disagree by that factor - up to the day the split took effect, from which
/// on they agree again.
#[derive(Debug, PartialEq, Eq)]
struct Split {
    /// What the prices in the file must be divided by to be on the scale of
    /// the fetched ones. Greater than one for a split, smaller for a reverse
    /// split.
    factor: Decimal,
    effective: Effective,
}

/// The day a split took effect, as far as the dates the file and the fetch
/// have in common show it.
#[derive(Debug, PartialEq, Eq)]
enum Effective {
    /// The first date the fetch did not restate.
    On(NaiveDate),
    /// Every date in common was restated, which only places the split after
    /// the last of them.
    After(NaiveDate),
}

impl Display for Effective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effective::On(date) => write!(f, "on {date}"),
            Effective::After(date) => write!(f, "after {date}"),
        }
    }
}

impl Split {
    /// Reports the split and what was done about it.
    fn describe(&self, entry: &ConfigEntry, rescaled: usize) -> String {
        let rescaled = match rescaled {
            0 => "no earlier prices needed restating".to_string(),
            n => format!(
                "{n} earlier price{} in {} divided by {}",
                if n == 1 { "" } else { "s" },
                entry.file.display(),
                self.factor,
            ),
        };
        format!(
            "stock split {} by a factor of {}: {rescaled}",
            self.effective, self.factor
        )
    }
}

/// Fewer restated prices than this are taken to be a correction of the data
/// rather than a split, which restates the entire history before it.
const MIN_RESTATED: usize = 3;

/// Detects a stock split from the prices the file and the fetch have in
/// common: a run of prices which all changed by the same factor, followed by
/// prices which did not change. Anything else - no change at all, a single
/// corrected price, changes by differing factors, or a change after a date
/// which did not change - is not a split and is not reported as one.
fn detect_split(
    existing: &BTreeMap<NaiveDate, Decimal>,
    fetched: &BTreeMap<NaiveDate, Decimal>,
) -> Option<Split> {
    let ratios = existing
        .iter()
        .filter_map(|(date, old)| {
            let new = fetched.get(date)?;
            Some((*date, old.checked_div(*new)?))
        })
        .collect::<Vec<_>>();
    let restated = ratios
        .iter()
        .take_while(|(_, ratio)| !close_to(*ratio, Decimal::ONE))
        .count();
    if restated < MIN_RESTATED {
        return None;
    }
    let (restated, unchanged) = ratios.split_at(restated);
    let factor = restated[0].1;
    // A price of zero in the file is not a split, and cannot be divided by.
    if factor.is_zero()
        || !restated.iter().all(|(_, ratio)| close_to(*ratio, factor))
        || !unchanged
            .iter()
            .all(|(_, ratio)| close_to(*ratio, Decimal::ONE))
    {
        return None;
    }
    let (last_restated, _) = restated[restated.len() - 1];
    Some(Split {
        // The ratios carry the rounding of both sides; the factor of a split
        // is a round number, and reporting it as one saves reading it.
        factor: factor.round_sf(4)?.normalize(),
        effective: match unchanged.first() {
            Some((date, _)) => Effective::On(*date),
            None => Effective::After(last_restated),
        },
    })
}

/// Whether two ratios agree to within a tenth of a percent. Prices are stored
/// rounded and Yahoo restates them with a rounding of its own, so the ratios
/// of a split are not exactly equal; no split factor is anywhere near this
/// close to one.
fn close_to(ratio: Decimal, other: Decimal) -> bool {
    (ratio - other).abs() * Decimal::ONE_THOUSAND <= other.abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }

    /// A price series starting on the `start`th of January 2026.
    fn series(start: u32, prices: &[&str]) -> BTreeMap<NaiveDate, Decimal> {
        prices
            .iter()
            .enumerate()
            .map(|(i, price)| (date(start + i as u32), price.parse().unwrap()))
            .collect()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn entry() -> ConfigEntry {
        ConfigEntry {
            commodity: "GOOG".into(),
            target_commodity: "USD".into(),
            file: PathBuf::from("prices/GOOG.knut"),
            symbol: "GOOG".into(),
        }
    }

    /// A journal holding the given prices of GOOG in USD, starting on the
    /// first of January 2026.
    fn journal(prices: &[&str]) -> (Journal, CommodityID, CommodityID) {
        let mut journal = Journal::default();
        let commodity = journal.registry().commodity_id("GOOG").unwrap();
        let target = journal.registry().commodity_id("USD").unwrap();
        for (date, price) in series(1, prices) {
            journal.day(date).prices.push(Price {
                loc: None,
                date,
                commodity,
                price,
                target,
            });
        }
        (journal, commodity, target)
    }

    /// The prices before the split are restated, the ones after it are not.
    #[test]
    fn test_detect_split() {
        let existing = series(1, &["400", "404", "408", "103", "104", "105"]);
        let fetched = series(1, &["100", "101", "102", "103", "104", "105"]);
        assert_eq!(
            detect_split(&existing, &fetched),
            Some(Split {
                factor: dec("4"),
                effective: Effective::On(date(4)),
            })
        );
    }

    /// A split which happened after the last price in the file restates every
    /// date the two have in common, which places it after that date.
    #[test]
    fn test_detect_split_after_the_last_common_date() {
        let existing = series(1, &["400", "404", "408", "412"]);
        let fetched = series(1, &["100", "101", "102", "103", "104", "105"]);
        assert_eq!(
            detect_split(&existing, &fetched),
            Some(Split {
                factor: dec("4"),
                effective: Effective::After(date(4)),
            })
        );
    }

    /// A reverse split restates the prices upwards.
    #[test]
    fn test_detect_reverse_split() {
        let existing = series(1, &["10", "10.1", "10.2", "103"]);
        let fetched = series(1, &["100", "101", "102", "103"]);
        let split = detect_split(&existing, &fetched).unwrap();
        assert_eq!(split.factor, dec("0.1"));
    }

    /// The prices before the fetched interval take no part in the detection;
    /// they are what the split is detected for.
    #[test]
    fn test_detect_split_over_a_partial_overlap() {
        let existing = series(1, &["400", "404", "408", "412", "416", "105"]);
        let fetched = series(3, &["102", "103", "104", "105"]);
        assert_eq!(
            detect_split(&existing, &fetched),
            Some(Split {
                factor: dec("4"),
                effective: Effective::On(date(6)),
            })
        );
    }

    /// Both sides are rounded, so the ratios of a split are not exactly
    /// equal and the factor is not exactly round.
    #[test]
    fn test_detect_split_tolerates_rounding() {
        let existing = series(1, &["400.0002", "403.9997", "408.0001", "103"]);
        let fetched = series(1, &["100", "101", "102", "103"]);
        let split = detect_split(&existing, &fetched).unwrap();
        assert_eq!(split.factor, dec("4"));
    }

    #[test]
    fn test_detect_no_split() {
        let prices = series(1, &["100", "101", "102", "103"]);
        assert_eq!(detect_split(&prices, &prices), None);
        // Nothing to compare against.
        assert_eq!(detect_split(&BTreeMap::new(), &prices), None);
        assert_eq!(detect_split(&prices, &BTreeMap::new()), None);
        // No dates in common.
        assert_eq!(
            detect_split(&series(1, &["400", "404"]), &series(9, &["100"])),
            None
        );
    }

    /// A handful of restated prices is a split; one or two are a correction.
    #[test]
    fn test_detect_split_needs_a_run_of_restated_prices() {
        let fetched = series(1, &["100", "101", "102", "103"]);
        assert_eq!(
            detect_split(&series(1, &["400", "101", "102", "103"]), &fetched),
            None
        );
        assert_eq!(
            detect_split(&series(1, &["400", "404", "102", "103"]), &fetched),
            None
        );
        assert!(detect_split(&series(1, &["400", "404", "408", "103"]), &fetched).is_some());
    }

    /// Prices which changed by differing factors are not a split.
    #[test]
    fn test_detect_split_needs_one_factor() {
        let fetched = series(1, &["100", "101", "102", "103"]);
        let existing = series(1, &["400", "404", "306", "103"]);
        assert_eq!(detect_split(&existing, &fetched), None);
    }

    /// A split restates a prefix of the history: a price which changed after
    /// one which did not is something else.
    #[test]
    fn test_detect_split_needs_the_change_to_stop() {
        let fetched = series(1, &["100", "101", "102", "103", "104"]);
        let existing = series(1, &["400", "404", "408", "103", "416"]);
        assert_eq!(detect_split(&existing, &fetched), None);
    }

    /// The prices before the split which the fetch does not restate itself
    /// are divided by the factor; the ones it covers are left to it.
    #[test]
    fn test_rescale() {
        let (mut journal, commodity, target) = journal(&["400", "404", "408", "412", "416", "105"]);
        let existing = prices(&journal, commodity, target);
        let fetched = series(3, &["102", "103", "104", "105"]);
        let split = Split {
            factor: dec("4"),
            effective: Effective::On(date(6)),
        };
        let rescaled = rescale(&mut journal, commodity, target, &split, &existing, &fetched);
        assert_eq!(rescaled.unwrap(), 2);
        assert_eq!(
            prices(&journal, commodity, target),
            series(1, &["100", "101", "408", "412", "416", "105"])
        );
    }

    /// A split which is only known to be after a date leaves that date's own
    /// price on the old scale, so it is restated too.
    #[test]
    fn test_rescale_after() {
        let (mut journal, commodity, target) = journal(&["400", "404", "408", "412"]);
        let existing = prices(&journal, commodity, target);
        let split = Split {
            factor: dec("4"),
            effective: Effective::After(date(4)),
        };
        let fetched = BTreeMap::new();
        let rescaled = rescale(&mut journal, commodity, target, &split, &existing, &fetched);
        assert_eq!(rescaled.unwrap(), 4);
        assert_eq!(
            prices(&journal, commodity, target),
            series(1, &["100", "101", "102", "103"])
        );
    }

    /// The split is one commodity's; the file may hold others.
    #[test]
    fn test_rescale_leaves_other_commodities_alone() {
        let (mut journal, commodity, target) = journal(&["400", "404", "408"]);
        let other = journal.registry().commodity_id("MSFT").unwrap();
        journal.day(date(1)).prices.push(Price {
            loc: None,
            date: date(1),
            commodity: other,
            price: dec("500"),
            target,
        });
        let existing = prices(&journal, commodity, target);
        let split = Split {
            factor: dec("4"),
            effective: Effective::After(date(3)),
        };
        let rescaled = rescale(
            &mut journal,
            commodity,
            target,
            &split,
            &existing,
            &BTreeMap::new(),
        );
        assert_eq!(rescaled.unwrap(), 3);
        assert_eq!(prices(&journal, other, target), series(1, &["500"]));
    }

    /// A price which does not divide evenly keeps the precision the fetched
    /// ones are written with.
    #[test]
    fn test_rescale_rounds_like_a_fetched_price() {
        let (mut journal, commodity, target) = journal(&["1", "1", "1"]);
        let existing = prices(&journal, commodity, target);
        let split = Split {
            factor: dec("3"),
            effective: Effective::After(date(3)),
        };
        rescale(
            &mut journal,
            commodity,
            target,
            &split,
            &existing,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            prices(&journal, commodity, target),
            series(1, &["0.3333333333", "0.3333333333", "0.3333333333"])
        );
    }

    #[test]
    fn test_describe() {
        let split = Split {
            factor: dec("20"),
            effective: Effective::After(date(4)),
        };
        assert_eq!(
            split.describe(&entry(), 743),
            "stock split after 2026-01-04 by a factor of 20: \
             743 earlier prices in prices/GOOG.knut divided by 20"
        );
        let split = Split {
            effective: Effective::On(date(4)),
            ..split
        };
        assert_eq!(
            split.describe(&entry(), 0),
            "stock split on 2026-01-04 by a factor of 20: no earlier prices needed restating"
        );
    }
}
