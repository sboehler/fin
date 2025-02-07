use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{
    AccountID, Assertion, Booking, Close, CommodityID, Interval, Open, Partition, Price,
    SourceFileID, SourceLoc, Transaction,
};
use super::journal::{Day, Journal};
use super::registry::Registry;
use crate::syntax::file::File;
use crate::{
    model::entities::Period,
    syntax::{
        cst::{self, SyntaxTree},
        error::SyntaxError,
    },
};

pub struct Analyzer {
    registry: Registry,
    days: BTreeMap<NaiveDate, Day>,

    current_file: SourceFileID,
}

impl Analyzer {
    pub fn new(registry: Registry) -> Self {
        Analyzer {
            registry,
            days: Default::default(),
            current_file: SourceFileID(0),
        }
    }

    pub fn to_journal(self) -> Journal {
        Journal {
            registry: Rc::new(self.registry),
            days: self.days,
        }
    }

    fn day(&mut self, d: NaiveDate) -> &mut Day {
        self.days.entry(d).or_insert_with(|| Day::new(d))
    }

    pub fn analyze(
        &mut self,
        tree: &SyntaxTree,
        source: &File,
    ) -> std::result::Result<(), SyntaxError> {
        self.current_file = self.registry.add_source_file(source.clone());
        for d in &tree.directives {
            use cst::Directive::*;
            match d {
                Price(p) => self.price(p, source)?,
                Open(o) => self.open(o, source)?,
                Transaction(t) => self.transaction(t, source)?,
                Assertion(a) => self.assertion(a, source)?,
                Close(c) => self.close(c, source)?,
                Include(_) => (),
            }
        }
        Ok(())
    }

    fn price(&mut self, p: &cst::Price, source: &File) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&p.date, source)?;
        let commodity = self.commodity(&p.commodity, source)?;
        let price = self.decimal(&p.price, source)?;
        let target = self.commodity(&p.target, source)?;
        let loc = Some(SourceLoc::new(self.current_file, p.range.clone()));
        self.day(date).prices.push(Price {
            loc,
            date,
            commodity,
            price,
            target,
        });
        Ok(())
    }

    fn open(&mut self, o: &cst::Open, source: &File) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&o.date, source)?;
        let account = self.account(&o.account, source)?;
        let loc = Some(SourceLoc::new(self.current_file, o.range.clone()));
        self.day(date).openings.push(Open { loc, date, account });
        Ok(())
    }

    fn transaction(
        &mut self,
        t: &cst::Transaction,
        source: &File,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&t.date, source)?;
        let bookings = t
            .bookings
            .iter()
            .map(|a| {
                Ok(Booking::create(
                    self.account(&a.credit, source)?,
                    self.account(&a.debit, source)?,
                    self.decimal(&a.quantity, source)?,
                    self.commodity(&a.commodity, source)?,
                    None,
                ))
            })
            .collect::<std::result::Result<Vec<_>, SyntaxError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let loc = Some(SourceLoc::new(self.current_file, t.range.clone()));
        let mut trx = Transaction {
            loc,
            date,
            description: Rc::new(source.text[t.description.content.clone()].to_string()),
            bookings,
            targets: None,
        };
        let ts = match &t.addon {
            Some(cst::Addon::Performance { commodities, .. }) => {
                trx.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.commodity(c, source))
                        .collect::<std::result::Result<Vec<_>, SyntaxError>>()?,
                );
                vec![trx]
            }
            Some(cst::Addon::Accrual {
                start,
                end,
                account,
                interval,
                ..
            }) => {
                let start = self.date(start, source)?;
                let end = self.date(end, source)?;
                let interval = self.interval(interval, source)?;
                let account = self.account(account, source)?;
                self.expand(trx, start, end, interval, account)
            }
            None => vec![trx],
        };
        for t in ts {
            self.day(t.date).transactions.push(t);
        }
        Ok(())
    }

    fn assertion(
        &mut self,
        a: &cst::Assertion,
        source: &File,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&a.date, source)?;
        let mut res = a
            .assertions
            .iter()
            .map(|a| {
                let loc = Some(SourceLoc::new(self.current_file, a.range.clone()));
                let account = self.account(&a.account, source)?;
                let balance = self.decimal(&a.balance, source)?;
                let commodity = self.commodity(&a.commodity, source)?;
                Ok(Assertion {
                    loc,
                    date,
                    account,
                    balance,
                    commodity,
                })
            })
            .collect::<std::result::Result<Vec<_>, SyntaxError>>()?;
        self.day(date).assertions.append(&mut res);
        Ok(())
    }

    fn close(&mut self, c: &cst::Close, source: &File) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&c.date, source)?;
        let account = self.account(&c.account, source)?;
        let loc = Some(SourceLoc::new(self.current_file, c.range.clone()));
        self.day(date).closings.push(Close { loc, date, account });
        Ok(())
    }

    fn date(
        &mut self,
        date: &cst::Date,
        source: &File,
    ) -> std::result::Result<NaiveDate, SyntaxError> {
        NaiveDate::parse_from_str(&source.text[date.0.clone()], "%Y-%m-%d").map_err(|_| {
            SyntaxError {
                range: date.0.clone(),
                want: cst::Token::Date,
                source: None,
            }
        })
    }

    fn decimal(
        &self,
        decimal: &cst::Decimal,
        source: &File,
    ) -> std::result::Result<rust_decimal::Decimal, SyntaxError> {
        rust_decimal::Decimal::from_str_exact(&source.text[decimal.0.clone()]).map_err(|_| {
            SyntaxError {
                range: decimal.0.clone(),
                want: cst::Token::Decimal,
                source: None,
            }
        })
    }

    fn interval(
        &mut self,
        d: &Range<usize>,
        source: &File,
    ) -> std::result::Result<Interval, SyntaxError> {
        match &source.text[d.clone()] {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Single),
            _ => Err(SyntaxError {
                range: d.clone(),
                want: cst::Token::Decimal,
                source: None,
            }),
        }
    }

    fn commodity(
        &mut self,
        commodity: &cst::Commodity,
        source: &File,
    ) -> std::result::Result<CommodityID, SyntaxError> {
        self.registry
            .commodity_id(&source.text[commodity.0.clone()])
            .map_err(|_| SyntaxError {
                range: commodity.0.clone(),
                want: cst::Token::Commodity,
                source: None,
            })
    }

    fn account(
        &mut self,
        account: &cst::Account,
        source: &File,
    ) -> std::result::Result<AccountID, SyntaxError> {
        self.registry
            .account_id(&source.text[account.range.clone()])
            .map_err(|_| SyntaxError {
                range: account.range.clone(),
                want: cst::Token::Account,
                source: None,
            })
    }

    fn expand(
        &self,
        t: Transaction,
        start: NaiveDate,
        end: NaiveDate,
        interval: Interval,
        account: AccountID,
    ) -> Vec<Transaction> {
        let mut res: Vec<Transaction> = Vec::new();
        let p = Partition::from_interval(Period(start, end), interval);
        for b in t.bookings {
            if b.account.account_type.is_al() {
                res.push(Transaction {
                    loc: t.loc,
                    date: t.date,
                    description: t.description.clone(),
                    bookings: Booking::create(account, b.account, b.quantity, b.commodity, None),
                    targets: t.targets.clone(),
                });
            }

            if b.account.account_type.is_ie() {
                let n = Decimal::from(p.periods.len());
                let quantity = (b.quantity / n).round_dp_with_strategy(
                    2,
                    rust_decimal::RoundingStrategy::MidpointAwayFromZero,
                );
                let rem = b.quantity - quantity * n;
                for (i, dt) in p.periods.iter().enumerate() {
                    let a = match i {
                        0 => quantity + rem,
                        _ => quantity,
                    };
                    res.push(Transaction {
                        loc: t.loc,
                        date: dt.1,
                        description: format!(
                            "{} (accrual {}/{})",
                            t.description,
                            i + 1,
                            p.periods.len()
                        )
                        .into(),
                        bookings: Booking::create(account, b.account, a, b.commodity, None),
                        targets: t.targets.clone(),
                    });
                }
            }
        }
        res
    }
}
