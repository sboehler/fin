use std::rc::Rc;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::{
    model::Period,
    syntax::{
        cst::{self, SyntaxTree},
        error::SyntaxError,
    },
};

use super::{
    journal::Journal, Account, Assertion, Booking, Close, Commodity, Interval, Open, Price,
    Transaction,
};

pub struct Analyzer<'a> {
    journal: &'a mut Journal,
    file: &'a SyntaxTree,
}

type Result<T> = std::result::Result<T, SyntaxError>;

impl<'a> Analyzer<'a> {
    pub fn analyze_files(files: &Vec<SyntaxTree>) -> Result<Journal> {
        let mut journal = Journal::new();
        for file in files {
            Analyzer {
                journal: &mut journal,
                file,
            }
            .analyze()?
        }
        Ok(journal)
    }

    fn analyze(&mut self) -> Result<()> {
        for d in &self.file.directives {
            match d {
                cst::Directive::Price {
                    date,
                    commodity,
                    price,
                    target,
                    ..
                } => self.analyze_price(date, commodity, price, target)?,
                cst::Directive::Open { date, account, .. } => self.analyze_open(date, account)?,
                cst::Directive::Transaction {
                    date,
                    addon,
                    description,
                    bookings,
                    ..
                } => self.analyze_transaction(addon, date, description, bookings)?,
                cst::Directive::Assertion {
                    date, assertions, ..
                } => self.analyze_assertion(date, assertions)?,
                cst::Directive::Close { date, account, .. } => self.analyze_close(date, account)?,
                cst::Directive::Include { .. } => (),
            }
        }
        Ok(())
    }

    fn analyze_price(
        &mut self,
        date: &cst::Date,
        commodity: &cst::Commodity,
        price: &cst::Decimal,
        target: &cst::Commodity,
    ) -> Result<()> {
        let date = self.analyze_date(date)?;
        let commodity = self.analyze_commodity(commodity)?;
        let price = self.analyze_decimal(price)?;
        let target = self.analyze_commodity(target)?;
        self.journal.day(date).prices.push(Price {
            date,
            commodity,
            price,
            target,
        });
        Ok(())
    }

    fn analyze_open(&mut self, date: &cst::Date, account: &cst::Account) -> Result<()> {
        let date = self.analyze_date(date)?;
        let account = self.analyze_account(account)?;
        self.journal.day(date).openings.push(Open { date, account });
        Ok(())
    }

    fn analyze_transaction(
        &mut self,
        addon: &Option<cst::Addon>,
        date: &cst::Date,
        description: &cst::QuotedString,
        bookings: &[cst::Booking],
    ) -> Result<()> {
        let date = self.analyze_date(date)?;
        let bookings = bookings
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
        let mut t = Transaction {
            date,
            description: description.content.text().to_string(),
            postings: bookings,
            targets: None,
        };
        let mut ts = match addon {
            Some(cst::Addon::Performance { commodities, .. }) => {
                t.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.analyze_commodity(c))
                        .collect::<Result<Vec<_>>>()?,
                );
                vec![t]
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
                self.expand(t, start, end, interval, account)
            }
            None => vec![t],
        };

        self.journal.day(date).transactions.append(&mut ts);
        Ok(())
    }

    fn analyze_assertion(&mut self, date: &cst::Date, assertions: &[cst::Assertion]) -> Result<()> {
        let date = self.analyze_date(date)?;
        let mut res = assertions
            .iter()
            .map(|a| {
                Ok(Assertion {
                    date,
                    account: self.analyze_account(&a.account)?,
                    balance: self.analyze_decimal(&a.balance)?,
                    commodity: self.analyze_commodity(&a.commodity)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.journal.day(date).assertions.append(&mut res);
        Ok(())
    }

    fn analyze_close(&mut self, date: &cst::Date, account: &cst::Account) -> Result<()> {
        let date = self.analyze_date(date)?;
        let account = self.analyze_account(account)?;
        self.journal
            .day(date)
            .closings
            .push(Close { date, account });
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
        self.journal
            .registry
            .borrow_mut()
            .commodity(commodity.0.text())
            .map_err(|_e| SyntaxError {
                rng: commodity.0.clone(),
                want: cst::Token::Commodity,
                source: None,
            })
    }

    fn analyze_account(&mut self, account: &cst::Account) -> Result<Rc<Account>> {
        self.journal
            .registry
            .borrow_mut()
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
