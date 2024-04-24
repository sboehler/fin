use chrono::NaiveDate;

use super::scanner::Range;

#[derive(PartialEq, Eq, Debug)]
pub struct Commodity<'a> {
    pub range: Range<'a>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Account<'a> {
    pub range: Range<'a>,
    pub segments: Vec<Range<'a>>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Date<'a> {
    pub range: Range<'a>,
    pub date: NaiveDate,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Decimal<'a> {
    pub range: Range<'a>,
    pub decimal: rust_decimal::Decimal,
}
