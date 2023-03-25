use std::{
    collections::BTreeMap,
    error::Error,
    fs, io,
    path::PathBuf,
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

use chrono::NaiveDate;

use crate::{
    context::Context,
    model::{Assertion, Close, Command, Open, Price, Transaction, Value},
    parser::{Directive, Parser},
    scanner::ParserError,
};

#[derive(Debug)]
pub enum JournalError {
    ParserError(ParserError),
    IOError(io::Error),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ParserError(e) => e.fmt(f),
            Self::IOError(e) => e.fmt(f),
        }
    }
}

impl Error for JournalError {}

pub type Result<T> = std::result::Result<T, JournalError>;

pub fn read_from_file(p: PathBuf) -> (mpsc::Receiver<Result<Command>>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let context = Arc::new(Context::new());
    (rx, thread::spawn(move || parse_spawn(context, p, tx)))
}

fn parse_spawn(context: Arc<Context>, file: PathBuf, tx: mpsc::Sender<Result<Command>>) {
    match fs::read_to_string(&file) {
        Ok(text) => {
            let parser = Parser::new_from_file(context.clone(), &text, Some(file.clone()));
            let mut handles = Vec::new();
            for directive in parser {
                match directive {
                    Ok(Directive::Command(cmd)) => tx.send(Ok(cmd)).unwrap(),
                    Ok(Directive::Include(path)) => {
                        let tx = tx.clone();
                        let path = file.parent().unwrap().join(path);
                        let context = context.clone();
                        handles.push(thread::spawn(move || parse_spawn(context, path, tx)));
                    }
                    Err(err) => tx.send(Err(JournalError::ParserError(err))).unwrap(),
                }
            }
            for j in handles {
                j.join().unwrap()
            }
        }
        Err(e) => tx.send(Err(JournalError::IOError(e))).unwrap(),
    };
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Day {
    pub date: NaiveDate,
    pub prices: Vec<Price>,
    pub assertions: Vec<Assertion>,
    pub values: Vec<Value>,
    pub openings: Vec<Open>,
    pub transactions: Vec<Transaction>,
    pub closings: Vec<Close>,
}

impl Day {
    fn new(d: NaiveDate) -> Self {
        Day {
            date: d,
            prices: Vec::new(),
            assertions: Vec::new(),
            values: Vec::new(),
            openings: Vec::new(),
            transactions: Vec::new(),
            closings: Vec::new(),
        }
    }

    fn add(&mut self, cmd: Command) {
        use Command::*;
        match cmd {
            Open(o) => self.openings.push(o),
            Price(p) => self.prices.push(p),
            Trx(t) => self.transactions.push(t),
            Value(v) => self.values.push(v),
            Assertion(a) => self.assertions.push(a),
            Close(c) => self.closings.push(c),
        }
    }
}

pub struct Journal {
    pub context: Arc<Context>,
    pub days: BTreeMap<NaiveDate, Day>,
}

impl Journal {
    pub fn new(context: Arc<Context>) -> Self {
        Journal {
            context,
            days: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, cmd: Command) {
        self.days
            .entry(cmd.date())
            .or_insert_with(|| Day::new(cmd.date()))
            .add(cmd)
    }

    pub fn min_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .find(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }

    pub fn max_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .rfind(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }
}
#[cfg(test)]
mod journal_tests {
    use super::*;

    #[test]
    fn test_min_max() {
        let ctx = Arc::new(Context::new());
        let mut j = Journal::new(ctx.clone());
        assert_eq!(j.min_date(), None);
        assert_eq!(j.max_date(), None);
        for day in 1..20 {
            j.add(Command::Trx(Transaction::new(
                NaiveDate::from_ymd_opt(2022, 4, day).unwrap(),
                "A transaction".into(),
                Vec::new(),
                Vec::new(),
                None,
            )));
            assert_eq!(j.max_date(), NaiveDate::from_ymd_opt(2022, 4, day));
        }
        assert_eq!(j.min_date(), NaiveDate::from_ymd_opt(2022, 4, 1));
    }
}
