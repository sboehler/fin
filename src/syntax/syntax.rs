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
}

#[derive(Eq, PartialEq, Debug)]
pub struct Decimal<'a> {
    pub range: Range<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Directive<'a> {
    pub range: Range<'a>,
    pub date: Date<'a>,
    pub command: Command<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command<'a> {
    Price(Price<'a>),
    Open(Open<'a>),
    Assertion(Assertion<'a>),
    Close(Close<'a>),
}

#[derive(Eq, PartialEq, Debug)]
pub struct Price<'a> {
    pub range: Range<'a>,
    pub commodity: Commodity<'a>,
    pub price: Decimal<'a>,
    pub target: Commodity<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Open<'a> {
    pub range: Range<'a>,
    pub account: Account<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Assertion<'a> {
    pub range: Range<'a>,
    pub account: Account<'a>,
    pub amount: Decimal<'a>,
    pub commodity: Commodity<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Close<'a> {
    pub range: Range<'a>,
    pub account: Account<'a>,
}
