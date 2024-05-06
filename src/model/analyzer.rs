use std::{cell::RefCell, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::{
    model::model::Period,
    syntax::{
        error::SyntaxError,
        file::ParsedFile,
        syntax::{self},
    },
};

use super::{
    journal::Journal,
    model::{self, Account, Assertion, Booking, Close, Interval, Open, Price, Transaction},
    registry::Registry,
};

pub struct Analyzer {
    registry: Rc<RefCell<Registry>>,
    journal: RefCell<Journal>,
}

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
                addon,
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
                        description,
                        bookings,
                        ..
                    } => self.analyze_transaction(f, addon, date, description, bookings)?,
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
        addon: &Option<syntax::Addon>,
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
        let mut ts = match addon {
            Some(syntax::Addon::Performance { commodities, .. }) => {
                t.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.analyze_commodity(f, c))
                        .collect::<Result<Vec<_>>>()?,
                );
                vec![t]
            }
            Some(syntax::Addon::Accrual {
                start,
                end,
                account,
                interval,
                ..
            }) => self.expand(
                t,
                self.analyze_date(&f, start)?,
                self.analyze_date(&f, end)?,
                self.analyze_interval(&f, interval)?,
                self.analyze_account(&f, account)?,
            ),
            None => vec![t],
        };

        Ok(self
            .journal
            .borrow_mut()
            .day(date)
            .transactions
            .append(&mut ts))
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

    fn analyze_interval(&self, f: &ParsedFile, d: &syntax::Rng) -> Result<Interval> {
        match d.slice(&f.text) {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Once),
            o => Err(SyntaxError::new(
                &f.text,
                d.start,
                None,
                syntax::Token::Decimal,
                syntax::Token::Custom(o.into()),
            )),
        }
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

    fn expand(
        &self,
        t: Transaction,
        start: NaiveDate,
        end: NaiveDate,
        interval: Interval,
        account: Rc<Account>,
    ) -> Vec<Transaction> {
        let mut res: Vec<Transaction> = Vec::new();

        for b in t.postings {
            if b.account.account_type.is_al() {
                res.push(Transaction {
                    date: t.date,
                    description: t.description.clone(),
                    postings: Booking::create(
                        account.clone(),
                        b.account.clone(),
                        b.quantity,
                        b.commodity.clone(),
                        Decimal::ZERO,
                    ),
                    targets: t.targets.clone(),
                })
            }

            if b.account.account_type.is_ie() {
                let p = Period(start, end).dates(interval, None);
                let amount = b.quantity / Decimal::from(p.periods.len());
                let rem = b.quantity % Decimal::from(p.periods.len());
                for (i, dt) in p.periods.iter().enumerate() {
                    let a = match i {
                        0 => amount + rem,
                        _ => amount,
                    };
                    res.push(Transaction {
                        date: dt.1,
                        description: format!(
                            "{} (accrual {}/{})",
                            t.description,
                            i + 1,
                            p.periods.len()
                        ),
                        postings: Booking::create(
                            account.clone(),
                            b.account.clone(),
                            a,
                            b.commodity.clone(),
                            Decimal::ZERO,
                        ),
                        targets: t.targets.clone(),
                    });
                }
            }
        }
        res
    }
}
