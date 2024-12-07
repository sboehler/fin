use std::{
    cmp,
    collections::HashMap,
    fmt::Display,
    iter::Sum,
    ops::{Add, AddAssign, Index, IndexMut, Neg, Sub, SubAssign},
    rc::Rc,
};

use chrono::NaiveDate;
use rust_decimal::{prelude::Zero, Decimal};

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
            _ => Err(ModelError::InvalidAccountType(value.into())),
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
                        return Err(ModelError::InvalidAccountName(s.into()));
                    }
                    if segment.chars().any(|c| !c.is_alphanumeric()) {
                        return Err(ModelError::InvalidAccountName(s.into()));
                    }
                }
                Ok(Account {
                    account_type: AccountType::try_from(at)?,
                    name: s.to_string(),
                })
            }
            _ => Err(ModelError::InvalidAccountName(s.into())),
        }
    }
}

impl Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct Commodity {
    pub name: String,
}

impl Commodity {
    pub fn new(name: &str) -> Result<Commodity> {
        if name.is_empty() || !name.chars().all(char::is_alphanumeric) {
            return Err(ModelError::InvalidCommodityName(name.into()));
        }
        Ok(Commodity {
            name: name.to_string(),
        })
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Price {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub commodity: Rc<Commodity>,
    pub price: Decimal,
    pub target: Rc<Commodity>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Open {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Booking {
    pub account: Rc<Account>,
    pub other: Rc<Account>,
    pub commodity: Rc<Commodity>,
    pub amount: Amount,
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
                amount: Amount::new(-quantity, -value),
            },
            Booking {
                account: debit.clone(),
                other: credit.clone(),
                commodity: commodity.clone(),
                amount: Amount::new(quantity, value),
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
    pub targets: Option<Vec<Rc<Commodity>>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Assertion {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
    pub balance: Decimal,
    pub commodity: Rc<Commodity>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Close {
    pub rng: Option<Rng>,
    pub date: NaiveDate,
    pub account: Rc<Account>,
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
        let mut d = period.1;
        while d >= period.0 {
            let start = cmp::max(interval.start_of(d).unwrap(), period.0);
            periods.push(Period(start, d));
            d = interval
                .start_of(d)
                .and_then(|d| d.checked_sub_days(Days::new(1)))
                .unwrap();
        }
        periods.reverse();
        Partition { periods }
    }

    pub fn start_dates(&self) -> Vec<NaiveDate> {
        self.periods.iter().map(|p| p.0).collect()
    }

    pub fn end_dates(&self) -> Vec<NaiveDate> {
        self.periods.iter().map(|p| p.0).collect()
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

#[derive(Default, Clone, Copy, Debug, Eq, PartialEq)]
pub struct Amount {
    pub quantity: Decimal,
    pub value: Decimal,
}

impl Amount {
    pub const ZERO: Self = Self {
        quantity: Decimal::ZERO,
        value: Decimal::ZERO,
    };

    pub fn new(quantity: Decimal, value: Decimal) -> Self {
        Amount { quantity, value }
    }
}

impl Zero for Amount {
    fn zero() -> Self {
        Self::ZERO
    }

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

impl Add for Amount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            quantity: self.quantity + rhs.quantity,
            value: self.value + rhs.value,
        }
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        self.quantity += rhs.quantity;
        self.value += rhs.value;
    }
}

impl AddAssign<&Amount> for Amount {
    fn add_assign(&mut self, rhs: &Self) {
        self.quantity += rhs.quantity;
        self.value += rhs.value;
    }
}
impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        self.quantity -= rhs.quantity;
        self.value -= rhs.value;
    }
}
impl SubAssign<&Amount> for Amount {
    fn sub_assign(&mut self, rhs: &Self) {
        self.quantity -= rhs.quantity;
        self.value -= rhs.value;
    }
}

impl Sub for Amount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            quantity: self.quantity - rhs.quantity,
            value: self.value - rhs.value,
        }
    }
}

impl Neg for Amount {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            quantity: -self.quantity,
            value: -self.value,
        }
    }
}

impl Sum for Amount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|acc, e| acc + e).unwrap_or_default()
    }
}

#[derive(Clone, Default)]
pub struct Vector<T> {
    elements: Vec<T>,
}

impl<T> Vector<T>
where
    T: Default + Clone,
{
    pub fn new(size: usize) -> Self {
        Self {
            elements: vec![Default::default(); size],
        }
    }

    pub fn elements(&self) -> impl Iterator<Item = &T> {
        self.elements.iter()
    }

    pub fn into_elements(self) -> impl Iterator<Item = T> {
        self.elements.into_iter()
    }
}

impl<T> AddAssign<&Vector<T>> for Vector<T>
where
    T: AddAssign<T> + Default + Copy,
{
    fn add_assign(&mut self, rhs: &Self) {
        self.elements
            .resize_with(rhs.elements.len(), Default::default);
        self.elements
            .iter_mut()
            .zip(rhs.elements.iter())
            .for_each(|(a, b)| *a += *b)
    }
}

impl<T> SubAssign<&Vector<T>> for Vector<T>
where
    T: SubAssign<T> + Default + Copy,
{
    fn sub_assign(&mut self, rhs: &Self) {
        if self.elements.len() < rhs.elements.len() {
            self.elements
                .resize_with(rhs.elements.len(), Default::default);
        }
        self.elements
            .iter_mut()
            .zip(rhs.elements.iter())
            .for_each(|(a, b)| *a -= *b)
    }
}

impl<'a, T> Sum<&'a Vector<T>> for Vector<T>
where
    T: Default + AddAssign<T> + Copy,
{
    fn sum<I: Iterator<Item = &'a Vector<T>>>(iter: I) -> Self {
        let mut res = Default::default();
        iter.for_each(|v| res += v);
        res
    }
}

impl<T> Index<usize> for Vector<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.elements[index]
    }
}

impl<T> IndexMut<usize> for Vector<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.elements[index]
    }
}

#[derive(Clone)]
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
    V: AddAssign<&'a V> + Clone + 'a,
{
    pub fn add(&mut self, header: &K, value: &'a V) {
        if let Some(target) = self.positions.get_mut(header) {
            *target += value
        } else {
            self.positions.insert(header.clone(), value.clone());
        }
    }

    pub fn add_all<I: Iterator<Item = (&'a K, &'a V)>>(&mut self, iter: I)
    where
        K: 'a,
        V: 'a,
    {
        iter.for_each(|(k, v)| self.add(k, v));
    }

    pub fn get_or_create<F>(&'a mut self, header: &'a K, default: F) -> &'a mut V
    where
        F: Fn() -> V,
    {
        self.positions.entry(header.clone()).or_insert_with(default)
    }

    pub fn get(&self, key: &K) -> V
    where
        V: Default,
    {
        self.positions.get(key).cloned().unwrap_or_default()
    }

    pub fn positions(&self) -> impl Iterator<Item = (&K, &V)> {
        self.positions.iter()
    }

    pub fn clear(&mut self) {
        self.positions.clear();
    }
}
