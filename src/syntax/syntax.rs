use super::scanner::Rng;

#[derive(PartialEq, Eq, Debug)]
pub struct Commodity(pub Rng);

#[derive(PartialEq, Eq, Debug)]
pub struct Account {
    pub range: Rng,
    pub segments: Vec<Rng>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Date(pub Rng);

#[derive(Eq, PartialEq, Debug)]
pub struct Decimal(pub Rng);

#[derive(Eq, PartialEq, Debug)]
pub struct QuotedString {
    pub range: Rng,
    pub content: Rng,
}

#[derive(Eq, PartialEq, Debug)]
pub struct SourceFile {
    pub range: Rng,
    pub directives: Vec<Directive>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Directive {
    Include {
        range: Rng,
        path: QuotedString,
    },
    Dated {
        range: Rng,
        addons: Vec<Addon>,
        date: Date,
        command: Command,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command {
    Price {
        range: Rng,
        commodity: Commodity,
        price: Decimal,
        target: Commodity,
    },
    Open {
        range: Rng,
        account: Account,
    },
    Transaction {
        range: Rng,
        description: QuotedString,
        bookings: Vec<Booking>,
    },
    Assertion {
        range: Rng,
        assertions: Vec<Assertion>,
    },
    Close {
        range: Rng,
        account: Account,
    },
}

#[derive(Eq, PartialEq, Debug)]
pub struct Assertion {
    pub range: Rng,
    pub account: Account,
    pub amount: Decimal,
    pub commodity: Commodity,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Booking {
    pub range: Rng,
    pub credit: Account,
    pub debit: Account,
    pub quantity: Decimal,
    pub commodity: Commodity,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Addon {
    Performance {
        range: Rng,
        commodities: Vec<Commodity>,
    },
    Accrual {
        range: Rng,
        interval: Rng,
        start: Date,
        end: Date,
        account: Account,
    },
}
