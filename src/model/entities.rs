use std::{
    cmp,
    collections::HashMap,
    fmt::Display,
    iter::Sum,
    ops::{AddAssign, Deref, DerefMut},
    rc::Rc,
};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::error::ModelError;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountType::Assets => write!(f, "Assets"),
            AccountType::Liabilities => write!(f, "Liabilities"),
            AccountType::Equity => write!(f, "Equity"),
            AccountType::Income => write!(f, "Income"),
            AccountType::Expenses => write!(f, "Expenses"),
        }
    }
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
            _ => Err(ModelError::InvalidAccountType(value.into())),
        }
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct AccountID {
    pub account_type: AccountType,
    pub id: usize,
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct CommodityID {
    pub id: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Price {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub commodity: CommodityID,
    pub price: Decimal,
    pub target: CommodityID,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Open {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: AccountID,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Value {
    target: CommodityID,
    value: Decimal,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Booking {
    pub account: AccountID,
    pub other: AccountID,
    pub commodity: CommodityID,
    pub quantity: Decimal,
    pub value: Option<Decimal>,
}

impl Booking {
    pub fn create(
        credit: AccountID,
        debit: AccountID,
        quantity: Decimal,
        commodity: CommodityID,
        value: Option<Decimal>,
    ) -> Vec<Booking> {
        vec![
            Booking {
                account: credit,
                other: debit,
                commodity,
                quantity: -quantity,
                value: value.map(|v| -v),
            },
            Booking {
                account: debit,
                other: credit,
                commodity,
                quantity,
                value,
            },
        ]
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Transaction {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub description: Rc<String>,
    pub bookings: Vec<Booking>,
    pub targets: Option<Vec<CommodityID>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Assertion {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: AccountID,
    pub balance: Decimal,
    pub commodity: CommodityID,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Close {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: AccountID,
}

use chrono::{Datelike, Days, Months};

use crate::syntax::cst::Rng;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum Interval {
    Single,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Interval {
    /// StartOf returns the first date in the given period which
    /// contains the receiver.
    pub fn start_of(self: Interval, d: NaiveDate) -> Option<NaiveDate> {
        use Interval::*;
        match self {
            Single | Daily => Some(d),
            Weekly => d.checked_sub_days(Days::new(d.weekday().number_from_monday() as u64 - 1)),
            Monthly => d.checked_sub_days(Days::new((d.day() - 1) as u64)),
            Quarterly => NaiveDate::from_ymd_opt(d.year(), ((d.month() - 1) / 3 * 3) + 1, 1),
            Yearly => NaiveDate::from_ymd_opt(d.year(), 1, 1),
        }
    }

    /// StartOf returns the first date in the given period which
    /// contains the receiver.
    pub fn end_of(self, d: NaiveDate) -> Option<NaiveDate> {
        use Interval::*;
        match self {
            Single | Daily => Some(d),
            Weekly => d.checked_add_days(Days::new(7 - d.weekday().number_from_monday() as u64)),
            Monthly => self
                .start_of(d)
                .and_then(|d| d.checked_add_months(Months::new(1)))
                .and_then(|d| d.checked_sub_days(Days::new(1))),
            Quarterly => self
                .start_of(d)
                .and_then(|d| d.checked_add_months(Months::new(3)))
                .and_then(|d| d.checked_sub_days(Days::new(1))),
            Yearly => NaiveDate::from_ymd_opt(d.year(), 12, 31),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Period(pub NaiveDate, pub NaiveDate);

impl Period {
    pub fn dates(&self) -> impl Iterator<Item = NaiveDate> + '_ {
        self.0.iter_days().take_while(|d| d <= &self.1)
    }

    pub fn contains(&self, d: NaiveDate) -> bool {
        self.0 <= d && d <= self.1
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Partition {
    pub periods: Vec<Period>,
}

impl Partition {
    pub fn new(periods: Vec<Period>) -> Self {
        Partition { periods }
    }

    pub fn cover(&self) -> Option<Period> {
        match (self.periods.first(), self.periods.last()) {
            (Some(first), Some(last)) => Some(Period(first.0, last.1)),
            _ => None,
        }
    }

    pub fn from_interval(period: Period, interval: Interval) -> Partition {
        if interval == Interval::Single {
            return Partition {
                periods: vec![period],
            };
        }
        let mut periods = Vec::new();
        let mut d = period.0;
        while d <= period.1 {
            let end = cmp::min(interval.end_of(d).unwrap(), period.1);
            periods.push(Period(d, end));
            d = end.checked_add_days(Days::new(1)).unwrap();
        }
        Partition { periods }
    }

    pub fn start_dates(&self) -> Vec<NaiveDate> {
        self.periods.iter().map(|p| p.0).collect()
    }

    pub fn end_dates(&self) -> Vec<NaiveDate> {
        self.periods.iter().map(|p| p.1).collect()
    }

    pub fn last_n(&self, n: usize) -> Partition {
        Partition {
            periods: self.periods.iter().rev().take(n).rev().copied().collect(),
        }
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
            Partition::from_interval(Period(date(2022, 1, 1), date(2022, 3, 20)), Monthly),
            Partition {
                periods: vec![
                    Period(date(2022, 1, 1), date(2022, 1, 31)),
                    Period(date(2022, 2, 1), date(2022, 2, 28)),
                    Period(date(2022, 3, 1), date(2022, 3, 20)),
                ],
            }
        );
        assert_eq!(
            Partition::from_interval(Period(date(2022, 1, 1), date(2022, 12, 20)), Monthly)
                .last_n(4),
            Partition {
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
        assert_eq!(Single.start_of(d), dt(2022, 6, 22));
        assert_eq!(Daily.start_of(d), dt(2022, 6, 22));
        assert_eq!(Weekly.start_of(d), dt(2022, 6, 20));
        assert_eq!(Monthly.start_of(d), dt(2022, 6, 1));
        assert_eq!(Quarterly.start_of(d), dt(2022, 4, 1));
        assert_eq!(Yearly.start_of(d), dt(2022, 1, 1))
    }

    #[test]
    fn test_end_of() {
        let d = date(2022, 6, 22);
        assert_eq!(Single.end_of(d), dt(2022, 6, 22));
        assert_eq!(Daily.end_of(d), dt(2022, 6, 22));
        assert_eq!(Weekly.end_of(d), dt(2022, 6, 26));
        assert_eq!(Monthly.end_of(d), dt(2022, 6, 30));
        assert_eq!(Quarterly.end_of(d), dt(2022, 6, 30));
        assert_eq!(Yearly.end_of(d), dt(2022, 12, 31))
    }
}

#[derive(Debug)]
pub struct Positions<K, V> {
    positions: HashMap<K, V>,
}

impl<K, V> Default for Positions<K, V> {
    fn default() -> Self {
        Self {
            positions: Default::default(),
        }
    }
}

impl<'a, K, V> Positions<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: AddAssign<&'a V> + 'a + Default,
{
    pub fn add(&mut self, key: &K, value: &'a V) {
        *self.entry(key.clone()).or_default() += value;
    }

    pub fn map_keys<F>(&'a self, f: F) -> Self
    where
        F: Fn(K) -> Option<K>,
        K: Copy + std::hash::Hash + Eq,
    {
        self.positions
            .iter()
            .filter_map(|(k, v)| f(*k).map(|k| (k, v)))
            .collect()
    }
}

impl<'a, 'b, K, V> FromIterator<(K, &'a V)> for Positions<K, V>
where
    K: Eq + std::hash::Hash + Copy,
    V: AddAssign<&'b V> + Default + 'b,
    'a: 'b,
{
    fn from_iter<T: IntoIterator<Item = (K, &'a V)>>(iter: T) -> Self {
        let mut res: Positions<K, V> = Default::default();
        iter.into_iter().for_each(|(k, v)| res.add(&k, v));
        res
    }
}

impl<'a, 'b, K, V> Extend<(K, &'a V)> for Positions<K, V>
where
    K: Eq + std::hash::Hash + Copy,
    V: AddAssign<&'b V> + Default + 'b,
    'a: 'b,
{
    fn extend<T: IntoIterator<Item = (K, &'a V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.add(&k, v)
        }
    }
}

impl<'a, 'b, K, V> AddAssign<&'a Positions<K, V>> for Positions<K, V>
where
    K: Eq + std::hash::Hash + Copy,
    V: AddAssign<&'b V> + Default + 'b,
    'a: 'b,
{
    fn add_assign(&mut self, rhs: &'a Positions<K, V>) {
        for (k, v) in &rhs.positions {
            self.add(k, v)
        }
    }
}

impl<'a, 'b, K, V> Sum<&'a Positions<K, V>> for Positions<K, V>
where
    K: Eq + std::hash::Hash + Copy,
    V: Default + AddAssign<&'b V> + Copy,
    'a: 'b,
{
    fn sum<I: Iterator<Item = &'a Positions<K, V>>>(iter: I) -> Self {
        let mut res = Default::default();
        iter.for_each(|v| res += v);
        res
    }
}

impl<K, V> Deref for Positions<K, V> {
    type Target = HashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.positions
    }
}

impl<K, V> DerefMut for Positions<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.positions
    }
}
