//! Client for Yahoo Finance's chart API, which serves the daily bars of a
//! symbol over a time interval.

use std::{error::Error, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use reqwest::{StatusCode, Url, header::HeaderMap};

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
                .timeout(Self::TIMEOUT)
                .build()
                .unwrap(),
        }
    }
}

impl Client {
    const YAHOO_URL: &str = "https://query2.finance.yahoo.com/v8/finance/chart";
    const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Safari/605.1.15";
    /// Applies to the entire request. Without it a stalled connection blocks
    /// the fetch forever.
    const TIMEOUT: Duration = Duration::from_secs(30);

    /// Fetches the daily quotes of `sym` between `start` and `end`, in the
    /// time zone of the exchange it trades on. Days on which it did not trade
    /// are reported by the API with all fields null and are left out.
    pub fn fetch(
        &self,
        sym: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Quote>, Box<dyn Error>> {
        let url = Self::create_url(sym, start, end)?;
        let response = self.client.get(url).send()?;
        let result = Self::parse(response.status(), &response.text()?)?;
        Self::quotes(result)
    }

    /// Turns a chart into one quote per complete daily bar.
    fn quotes(result: api::Result) -> Result<Vec<Quote>, Box<dyn Error>> {
        let tz: Tz = result.meta.exchange_timezone_name.parse()?;
        let ohlc = result
            .indicators
            .quote
            .first()
            .ok_or("the response holds no quotes")?;
        let adjclose = &result
            .indicators
            .adjclose
            .first()
            .ok_or("the response holds no adjusted closing prices")?
            .adjclose;

        let mut quotes = Vec::with_capacity(result.timestamp.len());
        for (i, timestamp) in result.timestamp.iter().enumerate() {
            let date = DateTime::from_timestamp(*timestamp, 0)
                .ok_or_else(|| format!("invalid timestamp {timestamp}"))?
                .with_timezone(&tz)
                .date_naive();
            // A bar is only reported if it is complete. The arrays are
            // expected to be as long as `timestamp`, but a short one drops
            // the remaining bars rather than being an error.
            let (Some(open), Some(high), Some(low), Some(close), Some(adj_close), Some(volume)) = (
                at(&ohlc.open, i),
                at(&ohlc.high, i),
                at(&ohlc.low, i),
                at(&ohlc.close, i),
                at(adjclose, i),
                at(&ohlc.volume, i),
            ) else {
                continue;
            };
            if close <= 0.0 {
                continue;
            }
            quotes.push(Quote {
                date,
                open,
                high,
                low,
                close,
                adj_close,
                volume,
            });
        }
        Ok(quotes)
    }

    /// Reads the chart out of a response. Yahoo reports an unknown symbol as
    /// a 404 whose body names the reason, but a rate limited or blocked
    /// request with a body which is not JSON at all, so the status is only
    /// reported when the body cannot be read.
    fn parse(status: StatusCode, body: &str) -> Result<api::Result, Box<dyn Error>> {
        let body: api::Body = serde_json::from_str(body).map_err(|e| {
            let body = body.trim();
            let body = body.get(..200).unwrap_or(body);
            format!("{status}: {e}: {body:?}")
        })?;
        if let Some(error) = body.chart.error {
            return Err(format!("{}: {}", error.code, error.description).into());
        }
        body.chart
            .result
            .into_iter()
            .flatten()
            .next()
            .ok_or_else(|| format!("{status}: the response holds no data").into())
    }

    fn create_url(
        sym: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Url, Box<dyn Error>> {
        let period1 = start.timestamp().to_string();
        let period2 = end.timestamp().to_string();
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

/// The `i`th value of one of the parallel arrays of a chart, if it holds one.
fn at<T: Copy>(values: &[Option<T>], i: usize) -> Option<T> {
    values.get(i).copied().flatten()
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
        /// Null when the request failed, in which case `error` says why.
        #[serde(default)]
        pub result: Option<Vec<Result>>,
        #[serde(default)]
        pub error: Option<Error>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Error {
        pub code: String,
        pub description: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct Result {
        pub meta: Meta,
        pub timestamp: Vec<i64>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A chart as the API returns it, shortened to four bars: two complete,
    /// one the exchange was closed on, and one still without a volume.
    const BODY: &str = r#"{"chart":{"result":[{
        "meta":{"currency":"CHF","symbol":"USDCHF=X","exchangeTimezoneName":"Europe/Zurich"},
        "timestamp":[1758009600,1758096000,1758182400,1758268800],
        "indicators":{
            "quote":[{
                "open":[0.7844,null,0.7871,0.7883],
                "high":[0.7869,null,0.7902,0.7921],
                "low":[0.7841,null,0.7865,0.7879],
                "close":[0.78591,null,0.78834,0.79205],
                "volume":[0,null,null,0]
            }],
            "adjclose":[{"adjclose":[0.78591,null,0.78834,0.79205]}]
        }
    }],"error":null}}"#;

    fn parse(body: &str) -> Result<Vec<Quote>, Box<dyn Error>> {
        Client::quotes(Client::parse(StatusCode::OK, body)?)
    }

    #[test]
    fn test_create_url() -> Result<(), Box<dyn Error>> {
        let start = DateTime::parse_from_rfc3339("2023-10-01T12:09:14Z")?;
        let end = DateTime::parse_from_rfc3339("2024-10-01T12:09:14Z")?;
        assert_eq!(
            Client::create_url("GOOG", start.into(), end.into())?.as_str(),
            "https://query2.finance.yahoo.com/v8/finance/chart/GOOG?events=history&interval=1d&period1=1696162154&period2=1727784554"
        );
        Ok(())
    }

    #[test]
    fn test_parse() {
        let quotes = parse(BODY).unwrap();
        // The closed day and the bar without a volume are left out.
        // Timestamps are days in the time zone of the exchange, not in UTC.
        let date = |day| NaiveDate::from_ymd_opt(2025, 9, day).unwrap();
        assert_eq!(
            quotes.iter().map(|q| q.date).collect::<Vec<_>>(),
            vec![date(16), date(19)]
        );
        let quote = &quotes[0];
        assert_eq!(
            (quote.open, quote.high, quote.low, quote.close),
            (0.7844, 0.7869, 0.7841, 0.78591)
        );
        assert_eq!((quote.adj_close, quote.volume), (0.78591, 0));
    }

    /// Yahoo reports an unknown symbol as a 404 with a JSON body naming the
    /// reason, which is more use than the status.
    #[test]
    fn test_parse_reports_api_error() {
        let body = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        let err = Client::parse(StatusCode::NOT_FOUND, body)
            .unwrap_err()
            .to_string();
        assert_eq!(err, "Not Found: No data found, symbol may be delisted");
    }

    /// A rate limited request has no JSON body at all, so the error names the
    /// status and quotes what did come back.
    #[test]
    fn test_parse_reports_non_json_body() {
        let err = Client::parse(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests\n")
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("429 Too Many Requests: "), "{err}");
        assert!(err.ends_with(r#": "Too Many Requests""#), "{err}");
    }

    #[test]
    fn test_parse_reports_missing_data() {
        let err = |body| Client::parse(StatusCode::OK, body).unwrap_err().to_string();
        assert_eq!(
            err(r#"{"chart":{"result":[],"error":null}}"#),
            "200 OK: the response holds no data"
        );
        assert_eq!(
            err(r#"{"chart":{"result":null,"error":null}}"#),
            "200 OK: the response holds no data"
        );
        let no_quotes = BODY.replace(r#""quote":[{"#, r#""quote":[], "unused":[{"#);
        assert_eq!(
            parse(&no_quotes).unwrap_err().to_string(),
            "the response holds no quotes"
        );
    }

    /// Short arrays drop the bars they do not cover instead of panicking on
    /// an index out of bounds.
    #[test]
    fn test_parse_tolerates_short_arrays() {
        let truncated = BODY.replace("\"open\":[0.7844,null,0.7871,0.7883]", "\"open\":[0.7844]");
        assert_eq!(parse(&truncated).unwrap().len(), 1);
        let empty = BODY.replace("\"close\":[0.78591,null,0.78834,0.79205]", "\"close\":[]");
        assert_eq!(parse(&empty).unwrap().len(), 0);
    }

    /// A closing price of zero is not a price.
    #[test]
    fn test_parse_drops_zero_close() {
        let zero = BODY.replace("\"close\":[0.78591,", "\"close\":[0.0,");
        assert_eq!(parse(&zero).unwrap().len(), 1);
    }
}
