pub mod error;
pub mod file;
pub mod format;
pub mod parser;
pub mod scanner;

use error::SyntaxError;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct Rng {
    pub start: usize,
    pub end: usize,
}

impl Rng {
    pub fn new(start: usize, str: &str) -> Rng {
        Rng {
            start,
            end: start + str.len(),
        }
    }

    pub fn slice<'a>(&self, s: &'a str) -> &'a str {
        &s[self.start..self.end]
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }
}

pub type Result<T> = std::result::Result<T, SyntaxError>;

#[derive(Debug, Eq, PartialEq)]
pub enum Token {
    EOF,
    BlankLine,
    Char(char),
    Digit,
    Comment,
    Directive,
    AlphaNum,
    Either(Vec<Token>),
    Decimal,
    Interval,
    Any,
    Date,
    WhiteSpace,
    Custom(String),
    Error(Box<SyntaxError>),
}

impl Token {
    pub fn from_char(ch: Option<char>) -> Self {
        match ch {
            None => Self::EOF,
            Some(c) if c.is_whitespace() => Self::WhiteSpace,
            Some(c) => Self::Char(c),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::EOF => write!(f, "EOF"),
            Self::Error(_) => write!(f, "error"),
            Self::Char(ch) => write!(f, "'{}'", ch.escape_debug()),
            Self::Digit => write!(f, "a digit (0-9)"),
            Self::Decimal => write!(f, "a decimal number"),
            Self::Directive => write!(f, "a directive"),
            Self::BlankLine => write!(f, "a blank line"),
            Self::Comment => write!(f, "a comment"),
            Self::Interval => write!(
                f,
                "a time interval (daily, monthly, quarterly, yearly, once)"
            ),
            Self::Date => write!(f, "a date"),
            Self::AlphaNum => {
                write!(f, "a character (a-z, A-Z) or a digit (0-9)")
            }
            Self::Any => write!(f, "any character"),
            Self::WhiteSpace => write!(f, "whitespace"),
            Self::Custom(s) => write!(f, "{}", s),
            Self::Either(chars) => {
                for (i, ch) in chars.iter().enumerate() {
                    write!(f, "{}", ch)?;
                    if i < chars.len().saturating_sub(1) {
                        write!(f, ", ")?;
                    }
                }
                writeln!(f)?;
                Ok(())
            }
        }
    }
}

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
pub struct SyntaxTree {
    pub range: Rng,
    pub directives: Vec<Directive>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum Directive {
    Include {
        range: Rng,
        path: QuotedString,
    },
    Price {
        range: Rng,
        date: Date,
        commodity: Commodity,
        price: Decimal,
        target: Commodity,
    },
    Open {
        range: Rng,
        date: Date,
        account: Account,
    },
    Transaction {
        range: Rng,
        addon: Option<Addon>,
        date: Date,
        description: QuotedString,
        bookings: Vec<Booking>,
    },
    Assertion {
        range: Rng,
        date: Date,
        assertions: Vec<Assertion>,
    },
    Close {
        range: Rng,
        date: Date,
        account: Account,
    },
}

impl Directive {
    pub fn range(&self) -> Rng {
        match self {
            Directive::Include { range, .. } => *range,
            Directive::Price { range, .. } => *range,
            Directive::Open { range, .. } => *range,
            Directive::Transaction { range, .. } => *range,
            Directive::Assertion { range, .. } => *range,
            Directive::Close { range, .. } => *range,
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
pub struct Assertion {
    pub range: Rng,
    pub account: Account,
    pub balance: Decimal,
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
