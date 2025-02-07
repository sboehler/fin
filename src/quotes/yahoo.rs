use std::{error::Error, path::PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use reqwest::{header::HeaderMap, Url};
use serde::Deserialize;

pub struct Client {
    client: reqwest::blocking::Client,
}

impl Default for Client {
    fn default() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", Self::USER_AGENT.parse().unwrap());
        Self {
            client: reqwest::blocking::ClientBuilder::new()
                .default_headers(headers)
                .build()
                .unwrap(),
        }
    }
}

impl Client {
    const YAHOO_URL: &str = "https://query2.finance.yahoo.com/v8/finance/chart";
    const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Safari/605.1.15";

    // Fetch fetches a set of quotes
    pub fn fetch(
        &self,
        sym: &str,
        t0: DateTime<Utc>,
        t1: DateTime<Utc>,
    ) -> Result<Vec<Quote>, Box<dyn Error>> {
        let url = Self::create_url(sym, t0, t1)?;
        let body: api::Body = self.client.get(url).send()?.json().unwrap();
        let result = body.chart.result.first().unwrap();
        let tz: Tz = result.meta.exchange_timezone_name.parse()?;
        let dates = result.timestamp.iter().map(|ts| {
            DateTime::from_timestamp(*ts as i64, 0)
                .unwrap()
                .with_timezone(&tz)
                .date_naive()
        });
        let q = &result.indicators.quote.first().unwrap();
        let ac = &result.indicators.adjclose.first().unwrap();
        Ok(dates
            .enumerate()
            .filter_map(|(i, date)| {
                Some(Quote {
                    date,
                    open: q.open[i]?,
                    high: q.high[i]?,
                    low: q.low[i]?,
                    close: q.close[i]?,
                    adj_close: ac.adjclose[i]?,
                    volume: q.volume[i]?,
                })
            })
            .filter(|q| q.close > 0.0)
            .collect())
    }

    fn create_url(sym: &str, t0: DateTime<Utc>, t1: DateTime<Utc>) -> Result<Url, Box<dyn Error>> {
        let period1 = t1.timestamp().to_string();
        let period2 = t0.timestamp().to_string();
        let params = vec![
            ("events", "history"),
            ("interval", "1d"),
            ("period1", &period1),
            ("period2", &period2),
        ];

        let mut url = Url::parse_with_params(Self::YAHOO_URL, &params)?;
        url.path_segments_mut().unwrap().push(sym);
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_create_url() -> Result<(), Box<dyn Error>> {
        let t0 = DateTime::parse_from_rfc3339("2024-10-01T12:09:14Z")?;
        let t1 = DateTime::parse_from_rfc3339("2023-10-01T12:09:14Z")?;
        assert_eq!(
            Client::create_url("GOOG", t0.into(), t1.into())
                .unwrap()
                .as_str(),
            "https://query2.finance.yahoo.com/v8/finance/chart/GOOG?events=history&interval=1d&period1=1696162154&period2=1727784554"
        );
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub commodity: String,
    pub target_commodity: String,
    pub file: PathBuf,
    pub symbol: String,
}

#[derive(Debug)]
pub struct Quote {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub adj_close: f64,
    pub volume: usize,
}

pub mod api {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct Body {
        pub chart: Chart,
    }
    #[derive(Deserialize, Debug)]
    pub struct Chart {
        pub result: Vec<Result>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Result {
        pub meta: Meta,
        pub timestamp: Vec<usize>,
        pub indicators: Indicators,
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    pub struct Meta {
        pub exchange_timezone_name: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct Indicators {
        pub quote: Vec<Quote>,
        pub adjclose: Vec<Adjclose>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Quote {
        pub volume: Vec<Option<usize>>,
        pub high: Vec<Option<f64>>,
        pub close: Vec<Option<f64>>,
        pub low: Vec<Option<f64>>,
        pub open: Vec<Option<f64>>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Adjclose {
        pub adjclose: Vec<Option<f64>>,
    }
}
