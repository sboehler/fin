use std::{cell::RefCell, error::Error, fmt::Display, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::syntax::{
    error::SyntaxError,
    file::ParsedFile,
    syntax::{self},
};

use super::{
    error::ModelError,
    journal::{Day, Journal},
    model::{self, Assertion, Booking, Close, Open, Price, Transaction},
    registry::Registry,
};

pub struct Analyzer {
    registry: Rc<RefCell<Registry>>,
    journal: RefCell<Journal>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum AnalyzerError {
    InvalidDate,
    InvalidDecimal,
    ModelError(ModelError),
}

impl Display for AnalyzerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Error for AnalyzerError {}

type Result<T> = std::result::Result<T, SyntaxError>;

impl Analyzer {
    pub fn analyze(mut self, files: Vec<ParsedFile>) -> Result<Journal> {
        self.registry = Rc::new(RefCell::new(Registry::new()));
        self.journal = RefCell::new(Journal::new(self.registry.clone()));

        for f in &files {
            self.analyze_file(f)?
        }
        Ok(self.journal.replace(Journal::new(self.registry.clone())))
    }

    fn analyze_file(&self, f: &ParsedFile) -> Result<()> {
        for d in &f.syntax_tree.directives {
            if let syntax::Directive::Dated {
                date,
                addons,
                command,
                ..
            } = d
            {
                match command {
                    syntax::Command::Price {
                        commodity,
                        price,
                        target,
                        ..
                    } => self.analyze_price(&f, date, commodity, price, target)?,
                    syntax::Command::Open { account, .. } => {
                        self.analyze_open(&f, date, account)?
                    }
                    syntax::Command::Transaction {
                        range,
                        description,
                        bookings,
                    } => todo!(),
                    syntax::Command::Assertion { assertions, .. } => {
                        self.analyze_assertion(&f, date, assertions)?
                    }
                    syntax::Command::Close { account, .. } => {
                        self.analyze_close(&f, date, account)?
                    }
                }
            }
        }
        Ok(())
    }

    fn analyze_price(
        &self,
        f: &ParsedFile,
        date: &syntax::Date,
        commodity: &syntax::Commodity,
        price: &syntax::Decimal,
        target: &syntax::Commodity,
    ) -> Result<()> {
        let date = self.analyze_date(f, date)?;
        Ok(self.journal.borrow_mut().day(date).prices.push(Price {
            date,
            commodity: self.analyze_commodity(f, commodity)?,
            price: self.analyze_decimal(f, price)?,
            target: self.analyze_commodity(f, target)?,
        }))
    }

    fn analyze_open(
        &self,
        f: &ParsedFile,
        date: &syntax::Date,
        account: &syntax::Account,
    ) -> Result<()> {
        let date = self.analyze_date(f, date)?;
        Ok(self.journal.borrow_mut().day(date).openings.push(Open {
            date,
            account: self.analyze_account(f, account)?,
        }))
    }

    fn analyze_transaction(
        &self,
        f: &ParsedFile,
        addons: &Vec<syntax::Addon>,
        date: &syntax::Date,
        description: &syntax::QuotedString,
        bookings: &Vec<syntax::Booking>,
    ) -> Result<()> {
        let date = self.analyze_date(f, date)?;
        let bookings = bookings
            .iter()
            .map(|a| {
                Ok(Booking::create(
                    self.analyze_account(f, &a.credit)?,
                    self.analyze_account(f, &a.debit)?,
                    self.analyze_decimal(f, &a.quantity)?,
                    self.analyze_commodity(f, &a.commodity)?,
                    Decimal::ZERO,
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let mut t = Transaction {
            date,
            description: description.content.slice(&f.text).to_string(),
            postings: bookings,
            targets: None,
        };

        // Ok(self
        //     .journal
        //     .borrow_mut()
        //     .day(date)
        //     .transactions
        //     .append(vec![]));
        Ok(())
    }

    fn analyze_assertion(
        &self,
        f: &ParsedFile,
        date: &syntax::Date,
        assertions: &Vec<syntax::Assertion>,
    ) -> Result<()> {
        let date = self.analyze_date(f, date)?;
        let mut res = assertions
            .iter()
            .map(|a| {
                Ok(Assertion {
                    date,
                    account: self.analyze_account(f, &a.account)?,
                    balance: self.analyze_decimal(f, &a.balance)?,
                    commodity: self.analyze_commodity(f, &a.commodity)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(self
            .journal
            .borrow_mut()
            .day(date)
            .assertions
            .append(&mut res))
    }

    fn analyze_close(
        &self,
        f: &ParsedFile,
        date: &syntax::Date,
        account: &syntax::Account,
    ) -> Result<()> {
        let date = self.analyze_date(f, date)?;
        Ok(self.journal.borrow_mut().day(date).closings.push(Close {
            date,
            account: self.analyze_account(f, account)?,
        }))
    }

    fn analyze_date(&self, f: &ParsedFile, d: &syntax::Date) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(d.0.slice(&f.text), "%Y-%m-%d").map_err(|e| {
            SyntaxError::new(
                &f.text,
                d.0.start,
                Some(e.to_string()),
                syntax::Token::Date,
                syntax::Token::Custom(d.0.slice(&f.text).to_string()),
            )
        })
    }

    fn analyze_decimal(
        &self,
        f: &ParsedFile,
        d: &syntax::Decimal,
    ) -> Result<rust_decimal::Decimal> {
        rust_decimal::Decimal::from_str_exact(d.0.slice(&f.text)).map_err(|e| {
            SyntaxError::new(
                &f.text,
                d.0.start,
                Some(e.to_string()),
                syntax::Token::Decimal,
                syntax::Token::Custom(d.0.slice(&f.text).to_string()),
            )
        })
    }

    fn analyze_commodity(
        &self,
        f: &ParsedFile,
        c: &syntax::Commodity,
    ) -> Result<Rc<model::Commodity>> {
        self.registry
            .borrow_mut()
            .commodity(c.0.slice(&f.text))
            .map_err(|e| {
                SyntaxError::new(
                    &f.text,
                    c.0.start,
                    Some(e.to_string()),
                    syntax::Token::Custom("identifier".into()),
                    syntax::Token::Custom(c.0.slice(&f.text).to_string()),
                )
            })
    }

    fn analyze_account(&self, f: &ParsedFile, c: &syntax::Account) -> Result<Rc<model::Account>> {
        self.registry
            .borrow_mut()
            .account(c.range.slice(&f.text))
            .map_err(|e| {
                SyntaxError::new(
                    &f.text,
                    c.range.start,
                    Some(e.to_string()),
                    syntax::Token::Custom("account".into()),
                    syntax::Token::Custom(c.range.slice(&f.text).to_string()),
                )
            })
    }
}
