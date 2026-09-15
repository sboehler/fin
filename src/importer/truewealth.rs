//! Importer for True Wealth portfolio values.
//!
//! Open app.truewealth.ch, select the portfolio's performance chart over its
//! full history, and save the response of the `evolution` XHR call from the
//! browser's dev tools.
//!
//! The file holds a daily performance time series for one portfolio. The
//! importer turns the end-of-day value of that series into price directives,
//! valuing the commodity which stands for the portfolio in the journal.

use std::{error::Error, io::Write, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use rust_decimal::{Decimal, RoundingStrategy, prelude::FromPrimitive};
use serde::Deserialize;

use crate::model::{entities::Price, printer::Printer, registry::Registry};

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The commodity representing the portfolio.
    #[arg(short, long)]
    commodity: String,

    /// Ignore entries before this date.
    #[arg(short, long)]
    from: Option<NaiveDate>,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        import(&source, &self.commodity, self.from, w)
    }
}

/// Imports a True Wealth evolution export and writes the resulting journal to
/// `w`: one price directive per day on which the series reports a value,
/// sorted by date.
pub fn import(
    source: &str,
    commodity: &str,
    from: Option<NaiveDate>,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let registry = Rc::new(Registry::new());
    let commodity = registry.commodity_id(commodity)?;

    let evolution: Evolution = serde_json::from_str(source)?;
    // Unlike VIAC, the export names the currency it reports in.
    let target = registry.commodity_id(&evolution.currency)?;

    let mut prices = evolution
        .performance
        .iter()
        .filter(|entry| from.is_none_or(|from| entry.date >= from))
        .map(|entry| {
            Ok(Price {
                loc: None,
                date: entry.date,
                commodity,
                price: entry.rounded()?,
                target,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    // A zero balance carries no price information: it is reported before the
    // first contribution and after the portfolio is emptied.
    prices.retain(|p| !p.price.is_zero());
    prices.sort_by_key(|p| p.date);

    let mut printer = Printer::new(w, registry);
    for price in &prices {
        printer.price(price)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Evolution {
    /// The currency all values in `performance` are denominated in.
    currency: String,
    performance: Vec<Entry>,
}

/// One day of the series. The export carries inflows, fees and returns
/// alongside, which are not needed to value the portfolio.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    date: NaiveDate,
    /// The portfolio value at the end of the day.
    v_end: f64,
}

impl Entry {
    /// The value in francs and rappen. True Wealth reports up to eight
    /// decimals; everything past the rappen is noise.
    fn rounded(&self) -> Result<Decimal, Box<dyn Error>> {
        Decimal::from_f64(self.v_end)
            .map(|v| v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
            .ok_or_else(|| format!("invalid value {} on {}", self.v_end, self.date).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SOURCE: &str = r#"{
      "currency": "CHF",
      "t0": "2024-01-01",
      "t1": "2024-01-03",
      "performance": [
        {"date": "2024-01-03", "vStart": 100.239876543, "vEnd": 300.005, "inflows": 200, "fees": -0.5, "netReturn": 1.1},
        {"date": "2024-01-01", "vStart": 0, "vEnd": 0, "inflows": 0, "fees": 0, "netReturn": 1},
        {"date": "2024-01-02", "vStart": 0, "vEnd": 100.239876543, "inflows": 100, "fees": 0, "netReturn": 1}
      ]
    }"#;

    fn import_to_string(from: Option<NaiveDate>) -> String {
        let mut out = Vec::new();
        import(SOURCE, "TRUEWEALTH", from, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_import() {
        // Sorted by date, zero values dropped, rounded to two digits.
        assert_eq!(
            import_to_string(None),
            "2024-01-02 price TRUEWEALTH 100.24 CHF\n2024-01-03 price TRUEWEALTH 300.01 CHF\n"
        );
    }

    #[test]
    fn test_import_from() {
        let from = NaiveDate::from_ymd_opt(2024, 1, 3);
        assert_eq!(
            import_to_string(from),
            "2024-01-03 price TRUEWEALTH 300.01 CHF\n"
        );
    }

    #[test]
    fn test_import_uses_reported_currency() {
        let source = r#"{"currency": "USD", "performance": [{"date": "2024-01-02", "vEnd": 1}]}"#;
        let mut out = Vec::new();
        import(source, "TRUEWEALTH", None, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "2024-01-02 price TRUEWEALTH 1 USD\n"
        );
    }

    #[test]
    fn test_import_rejects_missing_currency() {
        let source = r#"{"performance": [{"date": "2024-01-02", "vEnd": 1}]}"#;
        let err = import(source, "TRUEWEALTH", None, &mut Vec::new()).unwrap_err();
        assert!(
            err.to_string().contains("currency"),
            "unexpected error: {err}"
        );
    }
}
