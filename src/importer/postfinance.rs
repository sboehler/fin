use std::{
    error::Error,
    fs::File,
    path::{Path, PathBuf},
    rc::Rc,
};

use chrono::NaiveDate;
use clap::Args;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::model::{entities::AccountID, registry::Registry};

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    #[arg(short, long)]
    account: String,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let registry = Rc::new(Registry::new());
        let importer = Importer::new(registry.clone(), registry.account_id(&self.account)?);
        importer.import(&self.source)?;

        Ok(())
    }
}

struct Importer {
    _registry: Rc<Registry>,
    _account: AccountID,
}

impl Importer {
    fn new(registry: Rc<Registry>, account: AccountID) -> Self {
        Self {
            _registry: registry,
            _account: account,
        }
    }

    fn import(&self, source: &Path) -> Result<(), Box<dyn Error>> {
        self.load(source)?;
        Ok(())
    }

    fn load(&self, source: &Path) -> Result<Vec<Transaction>, Box<dyn Error>> {
        let file = File::open(source)?;
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .delimiter(b';')
            .from_reader(&file);
        let headers = reader
            .records()
            .skip_while(|res| {
                println!("{:?}", res);
                res.as_ref()
                    .map(|r| r[0].to_string() != "Datum")
                    .unwrap_or_default()
            })
            .next()
            .unwrap()?;

        println!("{:?}", headers);
        reader.set_headers(headers);
        for result in reader.deserialize::<Line>() {
            println!("{:?}", result);
        }
        Ok(vec![])
    }
}

#[derive(Debug, Deserialize)]
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
