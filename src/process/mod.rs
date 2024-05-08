use chrono::NaiveDate;

use crate::model::{journal::Journal, prices::Prices};

pub fn foo() {
    let mut j = Journal::new();
    let mut prices = Prices::default();
    let target = j.registry.borrow_mut().commodity("USD").unwrap();

    for day in &mut j {
        day.date = NaiveDate::from_ymd_opt(2002, 2, 1).unwrap();
        day.prices.iter().for_each(|p| prices.insert(p));
        day.normalized_prices = prices.normalize(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foo() {
        foo();
    }
}
