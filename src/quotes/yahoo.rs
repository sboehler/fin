use std::error::Error;

use chrono::{DateTime, NaiveDate, Utc};
use reqwest::{header::HeaderMap, Url};
use serde::Deserialize;

pub struct Client {
    client: reqwest::blocking::Client,
}

impl Client {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", Self::USER_AGENT.parse().unwrap());
        let builder = reqwest::blocking::ClientBuilder::new();
        let builder = builder.default_headers(headers);
        Self {
            client: builder.build().unwrap(),
        }
    }

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
        let json: api::Body = self.client.get(url).send()?.json()?;
        println!("{:?}", json);

        Ok(vec![])
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
        url.path_segments_mut().unwrap().push(&sym);
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
            Client::create_url("GOOG".into(), t0.into(), t1.into())
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
    pub file: String,
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
        pub volume: Vec<usize>,
        pub high: Vec<f64>,
        pub close: Vec<f64>,
        pub low: Vec<f64>,
        pub open: Vec<f64>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Adjclose {
        pub adjclose: Vec<f64>,
    }
}
