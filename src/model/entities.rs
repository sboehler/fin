use std::{cmp, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::error::ModelError;

type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
}

impl AccountType {
    pub fn is_al(&self) -> bool {
        *self == Self::Assets || *self == Self::Liabilities
    }

    pub fn is_ie(&self) -> bool {
        *self == Self::Income || *self == Self::Expenses
    }
}

impl TryFrom<&str> for AccountType {
    type Error = ModelError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "Assets" => Ok(AccountType::Assets),
            "Liabilities" => Ok(AccountType::Liabilities),
            "Equity" => Ok(AccountType::Equity),
            "Income" => Ok(AccountType::Income),
            "Expenses" => Ok(AccountType::Expenses),
            _ => Err(ModelError::InvalidAccountType),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Account {
    pub account_type: AccountType,
    pub name: String,
}

impl Account {
    pub fn new(s: &str) -> Result<Account> {
        match s.split(':').collect::<Vec<_>>().as_slice() {
            &[at, ref segments @ ..] => {
                for segment in segments {
                    if segment.is_empty() {
                        return Err(ModelError::InvalidAccountName);
                    }
                    if segment.chars().any(|c| !c.is_alphanumeric()) {
                        return Err(ModelError::InvalidAccountName);
                    }
                }
                Ok(Account {
                    account_type: AccountType::try_from(at)?,
                    name: s.to_string(),
                })
            }
            _ => Err(ModelError::InvalidAccountName),
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct Commodity {
    pub name: String,
}

impl Commodity {
    pub fn new(name: &str) -> Result<Commodity> {
        if name.is_empty() || !name.chars().all(char::is_alphanumeric) {
            return Err(ModelError::InvalidCommodityName);
        }
        Ok(Commodity {
            name: name.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Price {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub commodity: Rc<Commodity>,
    pub price: Decimal,
    pub target: Rc<Commodity>,
}

#[derive(Debug, Clone)]
pub struct Open {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
}

#[derive(Debug, Clone)]
pub struct Booking {
    pub account: Rc<Account>,
    pub other: Rc<Account>,
    pub commodity: Rc<Commodity>,
    pub quantity: Decimal,
    pub value: Decimal,
}

impl Booking {
    pub fn create(
        credit: Rc<Account>,
        debit: Rc<Account>,
        quantity: Decimal,
        commodity: Rc<Commodity>,
        value: Decimal,
    ) -> Vec<Booking> {
        vec![
            Booking {
                account: credit.clone(),
                other: debit.clone(),
                commodity: commodity.clone(),
                quantity: -quantity,
                value: -value,
            },
            Booking {
                account: debit.clone(),
                other: credit.clone(),
                commodity: commodity.clone(),
                quantity,
                value,
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub description: String,
    pub postings: Vec<Booking>,
    pub targets: Option<Vec<Rc<Commodity>>>,
}

#[derive(Debug, Clone)]
pub struct Assertion {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
    pub balance: Decimal,
    pub commodity: Rc<Commodity>,
}

#[derive(Debug, Clone)]
pub struct Close {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
}

use chrono::{Datelike, Days, Months};

use crate::syntax::cst::Rng;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum Interval {
    Once,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Period(pub NaiveDate, pub NaiveDate);

impl Period {
    pub fn dates(self, interval: Interval, n: Option<usize>) -> Partition {
        if interval == Interval::Once {
            return Partition {
                period: self,
                interval,
                periods: vec![self],
            };
        }
        let mut periods = Vec::new();
        let mut d = self.1;
        let mut counter = 0;
        while d >= self.0 {
            match n {
                Some(n) if counter == n => break,
                Some(_) => counter += 1,
                None => (),
            }
            let start = cmp::max(start_of(d, interval).unwrap(), self.0);
            periods.push(Period(start, d));
            d = start_of(d, interval)
                .and_then(|d| d.checked_sub_days(Days::new(1)))
                .unwrap();
        }
        periods.reverse();
        Partition {
            period: self,
            interval,
            periods,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]

pub struct Partition {
    pub period: Period,
    pub interval: Interval,
    pub periods: Vec<Period>,
}

/// StartOf returns the first date in the given period which
/// contains the receiver.
pub fn start_of(d: NaiveDate, p: Interval) -> Option<NaiveDate> {
    use Interval::*;
    match p {
        Once | Daily => Some(d),
        Weekly => d.checked_sub_days(Days::new(d.weekday().number_from_monday() as u64 - 1)),
        Monthly => d.checked_sub_days(Days::new((d.day() - 1) as u64)),
        Quarterly => NaiveDate::from_ymd_opt(d.year(), ((d.month() - 1) / 3 * 3) + 1, 1),
        Yearly => NaiveDate::from_ymd_opt(d.year(), 1, 1),
    }
}

/// StartOf returns the first date in the given period which
/// contains the receiver.
pub fn end_of(d: NaiveDate, p: Interval) -> Option<NaiveDate> {
    use Interval::*;
    match p {
        Once | Daily => Some(d),
        Weekly => d.checked_add_days(Days::new(7 - d.weekday().number_from_monday() as u64)),
        Monthly => start_of(d, Monthly)
            .and_then(|d| d.checked_add_months(Months::new(1)))
            .and_then(|d| d.checked_sub_days(Days::new(1))),
        Quarterly => start_of(d, Quarterly)
            .and_then(|d| d.checked_add_months(Months::new(3)))
            .and_then(|d| d.checked_sub_days(Days::new(1))),
        Yearly => NaiveDate::from_ymd_opt(d.year(), 12, 31),
    }
}

#[cfg(test)]
mod test_period {

    use super::Interval::*;
    use super::*;
    use pretty_assertions::assert_eq;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn dt(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, d)
    }

    #[test]
    fn test_dates() {
        assert_eq!(
            Period(date(2022, 1, 1), date(2022, 3, 20)).dates(Monthly, None),
            Partition {
                period: Period(date(2022, 1, 1), date(2022, 3, 20)),
                interval: Monthly,
                periods: vec![
                    Period(date(2022, 1, 1), date(2022, 1, 31)),
                    Period(date(2022, 2, 1), date(2022, 2, 28)),
                    Period(date(2022, 3, 1), date(2022, 3, 20)),
                ],
            }
        );
        assert_eq!(
            Period(date(2022, 1, 1), date(2022, 12, 20)).dates(Monthly, Some(4)),
            Partition {
                period: Period(date(2022, 1, 1), date(2022, 12, 20)),
                interval: Monthly,
                periods: vec![
                    Period(date(2022, 9, 1), date(2022, 9, 30)),
                    Period(date(2022, 10, 1), date(2022, 10, 31)),
                    Period(date(2022, 11, 1), date(2022, 11, 30)),
                    Period(date(2022, 12, 1), date(2022, 12, 20))
                ]
            }
        )
    }

    #[test]
    fn test_start_of() {
        let d = date(2022, 6, 22);
        assert_eq!(start_of(d, Once), dt(2022, 6, 22));
        assert_eq!(start_of(d, Daily), dt(2022, 6, 22));
        assert_eq!(start_of(d, Weekly), dt(2022, 6, 20));
        assert_eq!(start_of(d, Monthly), dt(2022, 6, 1));
        assert_eq!(start_of(d, Quarterly), dt(2022, 4, 1));
        assert_eq!(start_of(d, Yearly), dt(2022, 1, 1))
    }

    #[test]
    fn test_end_of() {
        let d = date(2022, 6, 22);
        assert_eq!(end_of(d, Once), dt(2022, 6, 22));
        assert_eq!(end_of(d, Daily), dt(2022, 6, 22));
        assert_eq!(end_of(d, Weekly), dt(2022, 6, 26));
        assert_eq!(end_of(d, Monthly), dt(2022, 6, 30));
        assert_eq!(end_of(d, Quarterly), dt(2022, 6, 30));
        assert_eq!(end_of(d, Yearly), dt(2022, 12, 31))
    }
}
