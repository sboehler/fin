use super::scanner::Range1;

#[derive(PartialEq, Eq, Debug)]
pub struct Commodity<'a>(pub Range1<'a>);

#[derive(PartialEq, Eq, Debug)]
pub struct Account<'a> {
    pub range: Range1<'a>,
    pub segments: Vec<Range1<'a>>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Date<'a>(pub Range1<'a>);

#[derive(Eq, PartialEq, Debug)]
pub struct Decimal<'a>(pub Range1<'a>);

#[derive(Eq, PartialEq, Debug)]
pub struct QuotedString<'a> {
    pub range: Range1<'a>,
    pub content: Range1<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct SourceFile<'a> {
    pub range: Range1<'a>,
    pub directives: Vec<Directive<'a>>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Directive<'a> {
    Include {
        range: Range1<'a>,
        path: QuotedString<'a>,
    },
    Dated {
        range: Range1<'a>,
        addons: Vec<Addon<'a>>,
        date: Date<'a>,
        command: Command<'a>,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command<'a> {
    Price {
        range: Range1<'a>,
        commodity: Commodity<'a>,
        price: Decimal<'a>,
        target: Commodity<'a>,
    },
    Open {
        range: Range1<'a>,
        account: Account<'a>,
    },
    Transaction {
        range: Range1<'a>,
        description: QuotedString<'a>,
        bookings: Vec<Booking<'a>>,
    },
    Assertion {
        range: Range1<'a>,
        assertions: Vec<Assertion<'a>>,
    },
    Close {
        range: Range1<'a>,
        account: Account<'a>,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub struct Assertion<'a> {
    pub range: Range1<'a>,
    pub account: Account<'a>,
    pub amount: Decimal<'a>,
    pub commodity: Commodity<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Booking<'a> {
    pub range: Range1<'a>,
    pub credit: Account<'a>,
    pub debit: Account<'a>,
    pub quantity: Decimal<'a>,
    pub commodity: Commodity<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Addon<'a> {
    Performance {
        range: Range1<'a>,
        commodities: Vec<Commodity<'a>>,
    },
    Accrual {
        range: Range1<'a>,
        interval: Range1<'a>,
        start: Date<'a>,
        end: Date<'a>,
        account: Account<'a>,
    },
}
