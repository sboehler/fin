use chrono::NaiveDate;

use crate::model::journal::Journal;

pub fn foo() {
    let mut j = Journal::new();

    for day in &mut j {
        day.date = NaiveDate::from_ymd_opt(2002, 2, 1).unwrap();
        println!("Hello {}", day.date)
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
