use std::rc::Rc;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::{
    model::Period,
    syntax::{
        error::SyntaxError,
        file::ParsedFile,
        {self},
    },
};

use super::{
    journal::Journal, Account, Assertion, Booking, Close, Commodity, Interval, Open, Price,
    Transaction,
};

pub struct Analyzer<'a> {
    journal: &'a mut Journal,
    file: &'a ParsedFile,
}

type Result<T> = std::result::Result<T, SyntaxError>;

impl<'a> Analyzer<'a> {
    pub fn analyze_files(files: Vec<ParsedFile>) -> Result<Journal> {
        let mut journal = Journal::new();
        for file in &files {
            Analyzer {
                journal: &mut journal,
                file,
            }
            .analyze()?
        }
        Ok(journal)
    }

    fn analyze(&mut self) -> Result<()> {
        for d in &self.file.syntax_tree.directives {
            match d {
                syntax::Directive::Price {
                    date,
                    commodity,
                    price,
                    target,
                    ..
                } => self.analyze_price(date, commodity, price, target)?,
                syntax::Directive::Open { date, account, .. } => {
                    self.analyze_open(date, account)?
                }
                syntax::Directive::Transaction {
                    date,
                    addon,
                    description,
                    bookings,
                    ..
                } => self.analyze_transaction(addon, date, description, bookings)?,
                syntax::Directive::Assertion {
                    date, assertions, ..
                } => self.analyze_assertion(date, assertions)?,
                syntax::Directive::Close { date, account, .. } => {
                    self.analyze_close(date, account)?
                }
                syntax::Directive::Include { .. } => (),
            }
        }
        Ok(())
    }

    fn analyze_price(
        &mut self,
        date: &syntax::Date,
        commodity: &syntax::Commodity,
        price: &syntax::Decimal,
        target: &syntax::Commodity,
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

    fn analyze_open(&mut self, date: &syntax::Date, account: &syntax::Account) -> Result<()> {
        let date = self.analyze_date(date)?;
        let account = self.analyze_account(account)?;
        self.journal.day(date).openings.push(Open { date, account });
        Ok(())
    }

    fn analyze_transaction(
        &mut self,
        addon: &Option<syntax::Addon>,
        date: &syntax::Date,
        description: &syntax::QuotedString,
        bookings: &[syntax::Booking],
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
            description: description.content.slice(&self.file.text).to_string(),
            postings: bookings,
            targets: None,
        };
        let mut ts = match addon {
            Some(syntax::Addon::Performance { commodities, .. }) => {
                t.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.analyze_commodity(c))
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

    fn analyze_assertion(
        &mut self,

        date: &syntax::Date,
        assertions: &[syntax::Assertion],
    ) -> Result<()> {
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

    fn analyze_close(&mut self, date: &syntax::Date, account: &syntax::Account) -> Result<()> {
        let date = self.analyze_date(date)?;
        let account = self.analyze_account(account)?;
        self.journal
            .day(date)
            .closings
            .push(Close { date, account });
        Ok(())
    }

    fn analyze_date(&mut self, d: &syntax::Date) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(d.0.slice(&self.file.text), "%Y-%m-%d").map_err(|e| {
            SyntaxError::new(
                &self.file.text,
                d.0.start,
                Some(e.to_string()),
                syntax::Token::Date,
                syntax::Token::Custom(d.0.slice(&self.file.text).to_string()),
            )
        })
    }

    fn analyze_decimal(&self, d: &syntax::Decimal) -> Result<rust_decimal::Decimal> {
        rust_decimal::Decimal::from_str_exact(d.0.slice(&self.file.text)).map_err(|e| {
            SyntaxError::new(
                &self.file.text,
                d.0.start,
                Some(e.to_string()),
                syntax::Token::Decimal,
                syntax::Token::Custom(d.0.slice(&self.file.text).to_string()),
            )
        })
    }

    fn analyze_interval(&mut self, d: &syntax::Rng) -> Result<Interval> {
        match d.slice(&self.file.text) {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Once),
            o => Err(SyntaxError::new(
                &self.file.text,
                d.start,
                None,
                syntax::Token::Decimal,
                syntax::Token::Custom(o.into()),
            )),
        }
    }

    fn analyze_commodity(&mut self, c: &syntax::Commodity) -> Result<Rc<Commodity>> {
        self.journal
            .registry
            .borrow_mut()
            .commodity(c.0.slice(&self.file.text))
            .map_err(|e| {
                SyntaxError::new(
                    &self.file.text,
                    c.0.start,
                    Some(e.to_string()),
                    syntax::Token::Custom("identifier".into()),
                    syntax::Token::Custom(c.0.slice(&self.file.text).to_string()),
                )
            })
    }

    fn analyze_account(&mut self, c: &syntax::Account) -> Result<Rc<Account>> {
        self.journal
            .registry
            .borrow_mut()
            .account(c.range.slice(&self.file.text))
            .map_err(|e| {
                SyntaxError::new(
                    &self.file.text,
                    c.range.start,
                    Some(e.to_string()),
                    syntax::Token::Custom("account".into()),
                    syntax::Token::Custom(c.range.slice(&self.file.text).to_string()),
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
