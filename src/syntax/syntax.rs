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
pub struct QuotedString<'a> {
    pub range: Range<'a>,
    pub content: Range<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct SourceFile<'a> {
    pub range: Range<'a>,
    pub directives: Vec<Directive<'a>>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Directive<'a> {
    Include {
        range: Range<'a>,
        path: QuotedString<'a>,
    },
    Command {
        range: Range<'a>,
        date: Date<'a>,
        command: Command<'a>,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command<'a> {
    Price(Price<'a>),
    Open(Open<'a>),
    Transaction(Transaction<'a>),
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
pub struct Booking<'a> {
    pub range: Range<'a>,
    pub credit: Account<'a>,
    pub debit: Account<'a>,
    pub quantity: Decimal<'a>,
    pub commodity: Commodity<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Transaction<'a> {
    pub range: Range<'a>,
    pub description: QuotedString<'a>,
    pub bookings: Vec<Booking<'a>>,
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

#[derive(Eq, PartialEq, Debug)]
pub enum Addon<'a> {
    Performance {
        range: Range<'a>,
        performance: Performance<'a>,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub struct Performance<'a> {
    pub range: Range<'a>,
    pub commodities: Vec<Commodity<'a>>,
}
