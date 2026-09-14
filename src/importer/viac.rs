//! Importer for VIAC portfolio values.
//!
//! Open app.viac.ch, select "From start" in the overview dashboard, and save
//! the response of the `summary` XHR call from the browser's dev tools.
//!
//! The file holds a daily wealth time series for the account as a whole and
//! one for each individual portfolio. The importer turns one of those series
//! into price directives, valuing the commodity which stands for the
//! portfolio in the journal.

use std::{collections::BTreeMap, error::Error, io::Write, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use rust_decimal::{Decimal, RoundingStrategy, prelude::FromPrimitive};
use serde::Deserialize;

use crate::model::{entities::Price, printer::Printer, registry::Registry};

/// VIAC reports all values in Swiss francs.
const CURRENCY: &str = "CHF";

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The commodity representing the portfolio.
    #[arg(short, long)]
    commodity: String,

    /// Value this portfolio instead of the account total, e.g. `3.172.474.493.01`.
    #[arg(short, long)]
    portfolio: Option<String>,

    /// Ignore entries before this date.
    #[arg(short, long)]
    from: Option<NaiveDate>,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        import(
            &source,
            &self.commodity,
            self.portfolio.as_deref(),
            self.from,
            w,
        )
    }
}

/// Imports a VIAC summary and writes the resulting journal to `w`: one price
/// directive per day on which the selected series reports a value, sorted by
/// date.
pub fn import(
    source: &str,
    commodity: &str,
    portfolio: Option<&str>,
    from: Option<NaiveDate>,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let registry = Rc::new(Registry::new());
    let commodity = registry.commodity_id(commodity)?;
    let target = registry.commodity_id(CURRENCY)?;

    let summary: Summary = serde_json::from_str(source)?;
    let mut prices = summary
        .daily_wealth(portfolio)?
        .iter()
        .filter(|dv| from.is_none_or(|from| dv.date >= from))
        .map(|dv| {
            Ok(Price {
                loc: None,
                date: dv.date,
                commodity,
                price: dv.rounded()?,
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
#[serde(rename_all = "camelCase")]
struct Summary {
    daily_wealth: Vec<DailyValue>,
    /// Absent in exports which only cover the account total.
    p3a_summary: Option<P3aSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct P3aSummary {
    /// Keyed by portfolio number. A `BTreeMap` so error messages list the
    /// portfolios in a stable order.
    portfolio_wealth_summaries: BTreeMap<String, PortfolioSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortfolioSummary {
    daily_wealth: Vec<DailyValue>,
}

#[derive(Debug, Deserialize)]
struct DailyValue {
    date: NaiveDate,
    /// VIAC reports values with twenty-odd decimals; they are Swiss francs, so
    /// everything past the rappen is noise.
    value: f64,
}

impl DailyValue {
    /// The value in francs and rappen.
    fn rounded(&self) -> Result<Decimal, Box<dyn Error>> {
        Decimal::from_f64(self.value)
            .map(|v| v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
            .ok_or_else(|| format!("invalid value {} on {}", self.value, self.date).into())
    }
}

impl Summary {
    /// Returns the daily wealth series of `portfolio`, or the account total
    /// if no portfolio is selected.
    fn daily_wealth(&self, portfolio: Option<&str>) -> Result<&[DailyValue], Box<dyn Error>> {
        let Some(portfolio) = portfolio else {
            return Ok(&self.daily_wealth);
        };
        self.p3a_summary
            .as_ref()
            .and_then(|s| s.portfolio_wealth_summaries.get(portfolio))
            .map(|p| p.daily_wealth.as_slice())
            .ok_or_else(|| {
                format!(
                    "unknown portfolio {portfolio:?}, have [{}]",
                    self.portfolios().join(", ")
                )
                .into()
            })
    }

    fn portfolios(&self) -> Vec<&str> {
        self.p3a_summary
            .iter()
            .flat_map(|s| s.portfolio_wealth_summaries.keys())
            .map(String::as_str)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SOURCE: &str = r#"{
      "dailyWealth": [
        {"date": "2024-01-03", "value": 300.005},
        {"date": "2024-01-01", "value": 0},
        {"date": "2024-01-02", "value": 100.239876543}
      ],
      "totalValue": 300.005,
      "p3aSummary": {
        "portfolioWealthSummaries": {
          "1.01": {
            "portfolioNumber": "1.01",
            "dailyWealth": [{"date": "2024-01-02", "value": 100.239876543}]
          },
          "1.02": {
            "portfolioNumber": "1.02",
            "dailyWealth": [{"date": "2024-01-03", "value": 199.765}]
          }
        }
      }
    }"#;

    fn import_to_string(portfolio: Option<&str>, from: Option<NaiveDate>) -> String {
        let mut out = Vec::new();
        import(SOURCE, "VIAC", portfolio, from, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_import() {
        // Sorted by date, zero values dropped, rounded to two digits.
        assert_eq!(
            import_to_string(None, None),
            "2024-01-02 price VIAC 100.24 CHF\n2024-01-03 price VIAC 300.01 CHF\n"
        );
    }

    #[test]
    fn test_import_from() {
        let from = NaiveDate::from_ymd_opt(2024, 1, 3);
        assert_eq!(
            import_to_string(None, from),
            "2024-01-03 price VIAC 300.01 CHF\n"
        );
    }

    #[test]
    fn test_import_portfolio() {
        assert_eq!(
            import_to_string(Some("1.02"), None),
            "2024-01-03 price VIAC 199.77 CHF\n"
        );
    }

    #[test]
    fn test_import_unknown_portfolio() {
        let mut out = Vec::new();
        let err = import(SOURCE, "VIAC", Some("1.03"), None, &mut out).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unknown portfolio \"1.03\", have [1.01, 1.02]"
        );
    }

    #[test]
    fn test_import_without_p3a_summary() {
        let source = r#"{"dailyWealth": [{"date": "2024-01-02", "value": 1}]}"#;
        let mut out = Vec::new();
        import(source, "VIAC", None, None, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "2024-01-02 price VIAC 1 CHF\n"
        );
        // Without a p3aSummary there are no portfolios to select from.
        let err = import(source, "VIAC", Some("1.01"), None, &mut Vec::new()).unwrap_err();
        assert_eq!(err.to_string(), "unknown portfolio \"1.01\", have []");
    }
}
