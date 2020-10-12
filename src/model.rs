use chrono::prelude::NaiveDate;
use rust_decimal::prelude::{Decimal, Zero};
use std::fmt;
use std::fmt::Display;
use std::result::Result;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
    TBD,
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

impl Display for Open {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} open {}", self.date.format("%Y-%m-%d"), self.account)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub date: NaiveDate,
    pub account: Account,
}

impl Display for Close {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} close {}", self.date.format("%Y-%m-%d"), self.account)
    }
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
    ) -> Result<Transaction, String> {
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
            match &account {
                None => return Err(format!("Transaction is not balanced")),
                Some(a) => postings.push(Posting {
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

impl Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} \"{}\"",
            self.date.format("%Y-%m-%d"),
            self.description
        )?;
        for t in &self.tags {
            write!(f, " {}", t)?
        }
        for posting in &self.postings {
            writeln!(f)?;
            write!(f, "{}", posting)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    tag: String,
}

impl Tag {
    pub fn new(tag: String) -> Tag {
        Tag { tag }
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.tag)
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

impl Display for Posting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.account, self.amount, self.commodity)?;
        if let Some(l) = &self.lot {
            write!(f, " {}", l)?
        }
        if let Some(t) = &self.tag {
            write!(f, " {}", t)?
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lot {
    price: Decimal,
    commodity: Commodity,
    date: NaiveDate,
    label: Option<String>,
}

impl Lot {
    pub fn new(
        price: Decimal,
        commodity: Commodity,
        date: NaiveDate,
        label: Option<String>,
    ) -> Self {
        Self {
            price,
            commodity,
            date,
            label,
        }
    }
}

impl Display for Lot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ {} {}, {}", self.price, self.commodity, self.date)?;
        if let Some(l) = &self.label {
            write!(f, ", {}", l)?
        }
        write!(f, "}}")
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Commodity {
    name: String,
}

impl Commodity {
    pub fn new(name: String) -> Commodity {
        Commodity { name }
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    pub date: NaiveDate,
    pub price: Decimal,
    pub source: Commodity,
    pub target: Commodity,
}

impl Price {
    pub fn new(date: NaiveDate, price: Decimal, target: Commodity, source: Commodity) -> Price {
        Price {
            date,
            price,
            source,
            target,
        }
    }
}

impl Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} price {} {} {}",
            self.date.format("%Y-%m-%d"),
            self.source,
            self.price,
            self.target
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assertion {
    pub date: NaiveDate,
    pub account: Account,
    pub balance: Decimal,
    pub commodity: Commodity,
}

impl Assertion {
    pub fn new(
        date: NaiveDate,
        account: Account,
        balance: Decimal,
        commodity: Commodity,
    ) -> Assertion {
        Assertion {
            date,
            account,
            balance,
            commodity,
        }
    }
}

impl Display for Assertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.date.format("%Y-%m-%d"),
            self.account,
            self.balance,
            self.commodity
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Open(Open),
    Close(Close),
    Trx(Transaction),
    Price(Price),
    Assertion(Assertion),
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Open(o) => write!(f, "{}", o),
            Command::Close(c) => write!(f, "{}", c),
            Command::Trx(t) => write!(f, "{}", t),
            Command::Price(p) => write!(f, "{}", p),
            Command::Assertion(a) => write!(f, "{}", a),
        }
    }
}
