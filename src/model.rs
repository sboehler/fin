use chrono::prelude::NaiveDate;
use rust_decimal::prelude::{Decimal, Zero};
use std::fmt;
use std::fmt::Display;
use std::io::{Error, ErrorKind, Result};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub account_type: AccountType,
    pub segments: Vec<String>,
}

impl Account {
    pub fn new(account_type: AccountType, segments: Vec<String>) -> Account {
        Account {
            account_type,
            segments,
        }
    }
}

impl Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.account_type)?;
        for segment in self.segments.iter() {
            write!(f, ":")?;
            write!(f, "{}", *segment)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub date: NaiveDate,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub date: NaiveDate,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub date: NaiveDate,
    pub description: String,
    pub tags: Vec<Tag>,
    pub postings: Vec<Posting>,
}

impl Transaction {
    pub fn new(
        d: NaiveDate,
        desc: String,
        tags: Vec<Tag>,
        mut postings: Vec<Posting>,
        account: Option<Account>,
    ) -> Result<Transaction> {
        let mut amounts: Vec<(Commodity, Decimal)> = Vec::with_capacity(postings.len());
        for p in postings.iter() {
            match amounts.iter_mut().find(|c| c.0 == p.commodity) {
                None => amounts.push((p.commodity.clone(), p.amount)),
                Some(e) => e.1 += p.amount,
            };
        }
        for amt in amounts.iter() {
            if amt.1.is_zero() {
                continue;
            }
            match account {
                None => return Err(Error::new(ErrorKind::InvalidData, format!("error"))),
                Some(ref a) => postings.push(Posting {
                    account: a.clone(),
                    commodity: amt.0.clone(),
                    amount: -amt.1,
                    lot: None,
                    tag: None,
                }),
            }
        }
        Ok(Transaction {
            date: d,
            description: desc,
            tags: tags,
            postings: postings,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub tag: String,
}

impl Tag {
    pub fn new(tag: String) -> Tag {
        Tag { tag }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub account: Account,
    pub commodity: Commodity,
    pub amount: Decimal,
    pub lot: Option<Lot>,
    pub tag: Option<Tag>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lot;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Commodity {
    pub name: String,
}

impl Commodity {
    pub fn new(name: String) -> Commodity {
        Commodity { name }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Directive {
    Open(Open),
    Close(Close),
    Trx(Transaction),
    Price,
    Assertion,
}
