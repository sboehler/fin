use std::{error::Error, iter::Peekable, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use csv::{StringRecord, StringRecordsIntoIter};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::model::{
    self,
    entities::{AccountID, Booking, CommodityID},
    registry::Registry,
};

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    #[arg(short, long)]
    account: String,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let registry = Rc::new(Registry::new());
        let source = std::fs::read_to_string(&self.source)?;
        let mut importer = Parser::new(
            registry.clone(),
            registry.account_id(&self.account)?,
            &source,
        );
        importer.load()?;
        Ok(())
    }
}

struct Parser<'a> {
    registry: Rc<Registry>,
    account: AccountID,

    iter: Peekable<StringRecordsIntoIter<&'a [u8]>>,
    current: Option<StringRecord>,
}

impl<'a> Parser<'a> {
    fn new(registry: Rc<Registry>, account: AccountID, source: &'a str) -> Self {
        Self {
            registry,
            account,
            current: None,
            iter: csv::ReaderBuilder::new()
                .flexible(true)
                .delimiter(b';')
                .from_reader(source.as_bytes())
                .into_records()
                .peekable(),
        }
    }

    fn advance(&mut self) -> Result<(), Box<dyn Error>> {
        self.current = self.iter.next().transpose()?;
        Ok(())
    }

    fn load(&mut self) -> Result<Vec<model::entities::Transaction>, Box<dyn Error>> {
        let currency = self.read_preamble()?;
        let headers = self.read_headers()?;
        let transactions = self.read_transactions(&headers, currency)?;
        Ok(transactions)
    }

    fn read_preamble(&mut self) -> Result<CommodityID, Box<dyn Error>> {
        while let Some(ref rec) = self.current {
            if rec.len() != 2 {
                return Err("no currency found in preamble".into());
            }
            if &rec[0] != "Währung:" {
                self.advance()?;
                continue;
            }
            let name = rec[1].replace(&['"', '='], "");
            let currency = self.registry.commodity_id(&name)?;
            return Ok(currency);
        }
        Err("unexpected end of file while looking for currency".into())
    }

    fn read_headers(&mut self) -> Result<StringRecord, Box<dyn Error>> {
        let Some(rec) = self.current.clone() else {
            return Err("no headers found".into());
        };
        if rec.len() != 8 || &rec[0] != "Datum" {
            return Err(format!("invalid headers: {:?}", rec).into());
        }
        self.advance()?;
        Ok(rec)
    }

    fn read_transactions(
        &mut self,
        headers: &StringRecord,
        currency: CommodityID,
    ) -> Result<Vec<model::entities::Transaction>, Box<dyn Error>> {
        let mut transactions = Vec::new();
        while let Some(ref rec) = self.current {
            if rec.len() != 8 {
                return Err(format!("invalid transaction: {:?}", rec).into());
            }
            transactions.push(self.read_transaction(currency, headers, rec)?);
            self.advance()?;
        }
        Ok(transactions)
    }

    fn read_transaction(
        &self,
        currency: CommodityID,
        headers: &csv::StringRecord,
        record: &csv::StringRecord,
    ) -> Result<model::entities::Transaction, Box<dyn Error>> {
        let line: Line = record.deserialize(Some(headers))?;
        let quantity = line.credit.or(line.debit).ok_or("No quantity")?;
        let trx = model::entities::Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.description),
            bookings: Booking::create(self.account, self.account, quantity, currency, None),
            targets: None,
        };
        println!("{:?}", trx);
        Ok(trx)
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Line {
    #[serde(
        deserialize_with = "date_format::deserialize_naive_date",
        rename = "Datum"
    )]
    date: NaiveDate,

    #[serde(rename = "Avisierungstext")]
    description: String,

    #[serde(rename = "Gutschrift in CHF")]
    debit: Option<Decimal>,

    #[serde(rename = "Lastschrift in CHF")]
    credit: Option<Decimal>,

    #[serde(rename = "Label")]
    label: Option<String>,

    #[serde(rename = "Kategorie")]
    category: String,

    #[serde(deserialize_with = "date_format::option_naivedate", rename = "Valuta")]
    valuta: Option<NaiveDate>,

    #[serde(rename = "Saldo in CHF")]
    balance: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Transaction {
    buchungsdatum: NaiveDate,
    avisierungstext: String,
    gutschrift_in_chf: Decimal,
    belastung_in_chf: Decimal,
    label: String,
    kategorie: String,
    valuta: NaiveDate,
    saldo_in_chf: Decimal,
}

mod date_format {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer};

    const FORMAT: &'static str = "%d.%m.%Y";

    pub fn deserialize_naive_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let dt = NaiveDate::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)?;
        Ok(dt)
    }

    pub fn option_naivedate<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wrapper(#[serde(deserialize_with = "deserialize_naive_date")] NaiveDate);

        let v = Option::deserialize(deserializer)?;
        Ok(v.map(|Wrapper(a)| a))
    }
}
