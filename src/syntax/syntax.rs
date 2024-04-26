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
    Dated {
        range: Range<'a>,
        date: Date<'a>,
        command: Command<'a>,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command<'a> {
    Price {
        range: Range<'a>,
        commodity: Commodity<'a>,
        price: Decimal<'a>,
        target: Commodity<'a>,
    },
    Open {
        range: Range<'a>,
        account: Account<'a>,
    },
    Transaction {
        range: Range<'a>,
        description: QuotedString<'a>,
        bookings: Vec<Booking<'a>>,
    },
    Assertion {
        range: Range<'a>,
        account: Account<'a>,
        amount: Decimal<'a>,
        commodity: Commodity<'a>,
    },
    Close {
        range: Range<'a>,
        account: Account<'a>,
    },
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
pub enum Addon<'a> {
    Performance {
        range: Range<'a>,
        commodities: Vec<Commodity<'a>>,
    },
}
