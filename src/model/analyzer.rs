use std::collections::BTreeMap;
use std::rc::Rc;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{
    Account, Assertion, Booking, Close, Commodity, Interval, Open, Price, Transaction,
};
use super::journal::{Day, Journal};
use super::registry::Registry;
use crate::{
    model::entities::Period,
    syntax::{
        cst::{self, SyntaxFile},
        error::SyntaxError,
    },
};

type Result<T> = std::result::Result<T, SyntaxError>;

pub fn analyze_files(files: &Vec<SyntaxFile>) -> Result<Journal> {
    let mut analyzer = Analyzer::new();
    for file in files {
        analyzer.analyze(file)?
    }
    Ok(Journal {
        registry: Rc::new(analyzer.registry),
        days: analyzer.days,
    })
}
struct Analyzer {
    registry: Registry,
    days: BTreeMap<NaiveDate, Day>,
}

impl Analyzer {
    fn new() -> Self {
        Analyzer {
            registry: Registry::default(),
            days: Default::default(),
        }
    }

    pub fn day(&mut self, d: NaiveDate) -> &mut Day {
        self.days.entry(d).or_insert_with(|| Day::new(d))
    }

    fn analyze(&mut self, file: &SyntaxFile) -> Result<()> {
        for d in &file.directives {
            match d {
                cst::Directive::Price(p) => self.analyze_price(p)?,
                cst::Directive::Open(o) => self.analyze_open(o)?,
                cst::Directive::Transaction(t) => self.analyze_transaction(t)?,
                cst::Directive::Assertion(a) => self.analyze_assertion(a)?,
                cst::Directive::Close(c) => self.analyze_close(c)?,
                cst::Directive::Include(_) => (),
            }
        }
        Ok(())
    }

    fn analyze_price(&mut self, p: &cst::Price) -> Result<()> {
        let date = self.analyze_date(&p.date)?;
        let commodity = self.analyze_commodity(&p.commodity)?;
        let price = self.analyze_decimal(&p.price)?;
        let target = self.analyze_commodity(&p.target)?;
        self.day(date).prices.push(Price {
            rng: Some(p.range.clone()),
            date,
            commodity,
            price,
            target,
        });
        Ok(())
    }

    fn analyze_open(&mut self, o: &cst::Open) -> Result<()> {
        let date = self.analyze_date(&o.date)?;
        let account = self.analyze_account(&o.account)?;
        self.day(date).openings.push(Open {
            rng: Some(o.range.clone()),
            date,
            account,
        });
        Ok(())
    }

    fn analyze_transaction(&mut self, t: &cst::Transaction) -> Result<()> {
        let date = self.analyze_date(&t.date)?;
        let bookings = t
            .bookings
            .iter()
            .map(|a| {
                Ok(Booking::create(
                    self.analyze_account(&a.credit)?,
                    self.analyze_account(&a.debit)?,
                    self.analyze_decimal(&a.quantity)?,
                    self.analyze_commodity(&a.commodity)?,
                    Decimal::ZERO,
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let mut trx = Transaction {
            rng: Some(t.range.clone()),
            date,
            description: t.description.content.text().to_string(),
            postings: bookings,
            targets: None,
        };
        let mut ts = match &t.addon {
            Some(cst::Addon::Performance { commodities, .. }) => {
                trx.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.analyze_commodity(c))
                        .collect::<Result<Vec<_>>>()?,
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
                let start = self.analyze_date(start)?;
                let end = self.analyze_date(end)?;
                let interval = self.analyze_interval(interval)?;
                let account = self.analyze_account(account)?;
                self.expand(trx, start, end, interval, account)
            }
            None => vec![trx],
        };

        self.day(date).transactions.append(&mut ts);
        Ok(())
    }

    fn analyze_assertion(&mut self, a: &cst::Assertion) -> Result<()> {
        let date = self.analyze_date(&a.date)?;
        let mut res = a
            .assertions
            .iter()
            .map(|a| {
                Ok(Assertion {
                    rng: Some(a.range.clone()),
                    date,
                    account: self.analyze_account(&a.account)?,
                    balance: self.analyze_decimal(&a.balance)?,
                    commodity: self.analyze_commodity(&a.commodity)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.day(date).assertions.append(&mut res);
        Ok(())
    }

    fn analyze_close(&mut self, c: &cst::Close) -> Result<()> {
        let date = self.analyze_date(&c.date)?;
        let account = self.analyze_account(&c.account)?;
        self.day(date).closings.push(Close {
            rng: Some(c.range.clone()),
            date,
            account,
        });
        Ok(())
    }

    fn analyze_date(&mut self, date: &cst::Date) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(date.0.text(), "%Y-%m-%d").map_err(|_| SyntaxError {
            rng: date.0.clone(),
            want: cst::Token::Date,
            source: None,
        })
    }

    fn analyze_decimal(&self, decimal: &cst::Decimal) -> Result<rust_decimal::Decimal> {
        rust_decimal::Decimal::from_str_exact(decimal.0.text()).map_err(|_| SyntaxError {
            rng: decimal.0.clone(),
            want: cst::Token::Decimal,
            source: None,
        })
    }

    fn analyze_interval(&mut self, d: &cst::Rng) -> Result<Interval> {
        match d.text() {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Once),
            _ => Err(SyntaxError {
                rng: d.clone(),
                want: cst::Token::Decimal,
                source: None,
            }),
        }
    }

    fn analyze_commodity(&mut self, commodity: &cst::Commodity) -> Result<Rc<Commodity>> {
        self.registry
            .commodity(commodity.0.text())
            .map_err(|_e| SyntaxError {
                rng: commodity.0.clone(),
                want: cst::Token::Commodity,
                source: None,
            })
    }

    fn analyze_account(&mut self, account: &cst::Account) -> Result<Rc<Account>> {
        self.registry
            .account(account.range.text())
            .map_err(|_e| SyntaxError {
                rng: account.range.clone(),
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
        account: Rc<Account>,
    ) -> Vec<Transaction> {
        let mut res: Vec<Transaction> = Vec::new();
        let p = Period(start, end).dates(interval, None);
        for b in t.postings {
            if b.account.account_type.is_al() {
                res.push(Transaction {
                    rng: t.rng.clone(),
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
                });
            }

            if b.account.account_type.is_ie() {
                let n = Decimal::from(p.periods.len());
                let amount = b.quantity / n;
                let rem = b.quantity - amount * n;
                for (i, dt) in p.periods.iter().enumerate() {
                    let a = match i {
                        0 => amount + rem,
                        _ => amount,
                    };
                    res.push(Transaction {
                        rng: t.rng.clone(),
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

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}
