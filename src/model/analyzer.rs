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
use crate::syntax::error::ParserError;
use crate::syntax::file::File;
use crate::{
    model::entities::Period,
    syntax::{
        cst::{self, SyntaxTree},
        error::SyntaxError,
    },
};

pub fn analyze_files(trees: &Vec<(SyntaxTree, File)>) -> std::result::Result<Journal, ParserError> {
    let mut analyzer = Analyzer::new();
    for (file, source_file) in trees {
        analyzer.analyze(file, source_file)?
    }
    Ok(Journal {
        registry: Rc::new(analyzer.registry),
        days: analyzer.days,
    })
}
struct Analyzer {
    registry: Registry,
    days: BTreeMap<NaiveDate, Day>,

    current_file: SourceFileID,
}

impl Analyzer {
    fn new() -> Self {
        Analyzer {
            registry: Registry::default(),
            days: Default::default(),
            current_file: SourceFileID(0),
        }
    }

    pub fn day(&mut self, d: NaiveDate) -> &mut Day {
        self.days.entry(d).or_insert_with(|| Day::new(d))
    }

    fn analyze(
        &mut self,
        tree: &SyntaxTree,
        source: &File,
    ) -> std::result::Result<(), ParserError> {
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

    fn price(&mut self, p: &cst::Price, source: &File) -> std::result::Result<(), ParserError> {
        let date = self.date(&p.date, source)?;
        let commodity = self.commodity(&p.commodity, source)?;
        let price = self.decimal(&p.price, source)?;
        let target = self.commodity(&p.target, source)?;
        let price = Price {
            loc: Some(SourceLoc::new(self.current_file, p.range.clone())),
            date,
            commodity,
            price,
            target,
        };
        self.day(date).prices.push(price);
        Ok(())
    }

    fn open(&mut self, o: &cst::Open, source: &File) -> std::result::Result<(), ParserError> {
        let date = self.date(&o.date, source)?;
        let account = self.account(&o.account, source)?;
        let value = Open {
            loc: Some(SourceLoc::new(self.current_file, o.range.clone())),
            date,
            account,
        };
        self.day(date).openings.push(value);
        Ok(())
    }

    fn transaction(
        &mut self,
        t: &cst::Transaction,
        source: &File,
    ) -> std::result::Result<(), ParserError> {
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
            .collect::<std::result::Result<Vec<_>, ParserError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let mut trx = Transaction {
            loc: Some(SourceLoc::new(self.current_file, t.range.clone())),
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
                        .collect::<std::result::Result<Vec<_>, ParserError>>()?,
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
    ) -> std::result::Result<(), ParserError> {
        let date = self.date(&a.date, source)?;
        let mut res = a
            .assertions
            .iter()
            .map(|a| {
                Ok(Assertion {
                    loc: Some(SourceLoc::new(self.current_file, a.range.clone())),

                    date,
                    account: self.account(&a.account, source)?,
                    balance: self.decimal(&a.balance, source)?,
                    commodity: self.commodity(&a.commodity, source)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, ParserError>>()?;
        self.day(date).assertions.append(&mut res);
        Ok(())
    }

    fn close(&mut self, c: &cst::Close, source: &File) -> std::result::Result<(), ParserError> {
        let date = self.date(&c.date, source)?;
        let account = self.account(&c.account, source)?;
        let value = Close {
            loc: Some(SourceLoc::new(self.current_file, c.range.clone())),
            date,
            account,
        };
        self.day(date).closings.push(value);
        Ok(())
    }

    fn date(
        &mut self,
        date: &cst::Date,
        source: &File,
    ) -> std::result::Result<NaiveDate, ParserError> {
        NaiveDate::parse_from_str(&source.text[date.0.clone()], "%Y-%m-%d").map_err(|_| {
            ParserError::SyntaxError(
                SyntaxError {
                    range: date.0.clone(),
                    want: cst::Token::Date,
                    source: None,
                },
                source.clone(),
            )
        })
    }

    fn decimal(
        &self,
        decimal: &cst::Decimal,
        source: &File,
    ) -> std::result::Result<rust_decimal::Decimal, ParserError> {
        rust_decimal::Decimal::from_str_exact(&source.text[decimal.0.clone()]).map_err(|_| {
            ParserError::SyntaxError(
                SyntaxError {
                    range: decimal.0.clone(),
                    want: cst::Token::Decimal,
                    source: None,
                },
                source.clone(),
            )
        })
    }

    fn interval(
        &mut self,
        d: &Range<usize>,
        source: &File,
    ) -> std::result::Result<Interval, ParserError> {
        match &source.text[d.clone()] {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Single),
            _ => Err(ParserError::SyntaxError(
                SyntaxError {
                    range: d.clone(),
                    want: cst::Token::Decimal,
                    source: None,
                },
                source.clone(),
            )),
        }
    }

    fn commodity(
        &mut self,
        commodity: &cst::Commodity,
        source: &File,
    ) -> std::result::Result<CommodityID, ParserError> {
        self.registry
            .commodity_id(&source.text[commodity.0.clone()])
            .map_err(|_| {
                ParserError::SyntaxError(
                    SyntaxError {
                        range: commodity.0.clone(),
                        want: cst::Token::Commodity,
                        source: None,
                    },
                    source.clone(),
                )
            })
    }

    fn account(
        &mut self,
        account: &cst::Account,
        source: &File,
    ) -> std::result::Result<AccountID, ParserError> {
        self.registry
            .account_id(&source.text[account.range.clone()])
            .map_err(|_| {
                ParserError::SyntaxError(
                    SyntaxError {
                        range: account.range.clone(),
                        want: cst::Token::Account,
                        source: None,
                    },
                    source.clone(),
                )
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

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}
