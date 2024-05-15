use std::{fmt::Display, rc::Rc};

use super::file::File;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Rng {
    pub file: Rc<File>,
    pub start: usize,
    pub end: usize,
}

impl Rng {
    pub fn new(file: Rc<File>, start: usize, end: usize) -> Rng {
        Rng { file, start, end }
    }

    pub fn text(&self) -> &str {
        &self.file.text[self.start..self.end]
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.end == self.start
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Character {
    EOF,
    Char(char),
    NotChar(char),
    Digit,
    Alphabetic,
    AlphaNum,
    Any,
    HorizontalSpace,
    NewLine,
    OneOf(Vec<Character>),
}

impl Character {
    pub fn from_char(ch: Option<char>) -> Self {
        match ch {
            None => Self::EOF,
            Some('\n') => Self::NewLine,
            Some(c) if c.is_whitespace() => Self::HorizontalSpace,
            Some(c) => Self::Char(c),
        }
    }

    pub fn is(&self, o: Option<char>) -> bool {
        match o {
            None => matches!(self, Character::EOF | Character::NewLine),
            Some(c) => match self {
                Character::EOF => false,
                Character::Char(a) => c == *a,
                Character::NotChar(a) => c != *a,
                Character::Digit => c.is_ascii_digit(),
                Character::Alphabetic => c.is_alphabetic(),
                Character::AlphaNum => c.is_alphanumeric(),
                Character::Any => true,
                Character::HorizontalSpace => c.is_ascii_whitespace() && c != '\n',
                Character::NewLine => c == '\n',
                Character::OneOf(cs) => cs.iter().any(|c| c.is(o)),
            },
        }
    }
}

impl Display for Character {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Character::EOF => write!(f, "EOF"),
            Character::Char(ch) => write!(f, "'{}'", ch),
            Character::NotChar(ch) => write!(f, "not '{}'", ch),
            Character::Digit => write!(f, "digit"),
            Character::Alphabetic => write!(f, "alphabetic character"),
            Character::AlphaNum => write!(f, "alphanumeric character"),
            Character::Any => write!(f, "any character"),
            Character::HorizontalSpace => write!(f, "horizontal space"),
            Character::NewLine => write!(f, "newline"),
            Character::OneOf(chs) => {
                write!(
                    f,
                    "one of: {}",
                    chs.iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Sequence {
    One(Character),
    OneOf(Vec<Sequence>),
    NumberOf(usize, Character),
    String(String),
}

impl Display for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sequence::One(ch) => write!(f, "{}", ch),
            Sequence::OneOf(seq) => {
                write!(
                    f,
                    "{}",
                    seq.iter()
                        .map(Sequence::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Sequence::NumberOf(n, ch) => write!(f, "{} {}", n, ch),
            Sequence::String(ch) => write!(f, "{}", ch),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Token {
    EOF,
    Addon,
    Accrual,
    Account,
    Close,
    Assertion,
    SubAssertion,
    Performance,
    BlankLine,
    Booking,
    Character(Character),
    Sequence(Sequence),
    Transaction,
    Price,
    Open,
    QuotedString,
    Digit,
    AccountType,
    Commodity,
    File,
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
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Token::EOF => write!(f, "EOF"),
            Token::Character(ch) => write!(f, "{}", ch),
            Token::Digit => write!(f, "a digit (0-9)"),
            Token::Decimal => write!(f, "a decimal number"),
            Token::Directive => write!(f, "a directive"),
            Token::BlankLine => write!(f, "a blank line"),
            Token::Comment => write!(f, "a comment"),
            Token::Interval => write!(
                f,
                "a time interval (daily, monthly, quarterly, yearly, once)"
            ),
            Token::Date => write!(f, "a date"),
            Token::AlphaNum => {
                write!(f, "a character (a-z, A-Z) or a digit (0-9)")
            }
            Token::Any => write!(f, "any character"),
            Token::WhiteSpace => write!(f, "whitespace"),
            Token::Custom(s) => write!(f, "{}", s),
            Token::Either(chars) => {
                for (i, ch) in chars.iter().enumerate() {
                    write!(f, "{}", ch)?;
                    if i < chars.len().saturating_sub(1) {
                        write!(f, ", ")?;
                    }
                }
                writeln!(f)?;
                Ok(())
            }
            Token::Addon => write!(f, "addon"),
            Token::Accrual => write!(f, "accrual"),
            Token::Close => write!(f, "close directive"),
            Token::Assertion => write!(f, "balance directive"),
            Token::SubAssertion => write!(f, "subassertion"),
            Token::Performance => write!(f, "performance addon"),
            Token::Booking => write!(f, "booking"),
            Token::Transaction => write!(f, "transaction"),
            Token::Price => write!(f, "price directive"),
            Token::Open => write!(f, "open directive"),
            Token::QuotedString => write!(f, "quoted string"),
            Token::AccountType => write!(f, "account type"),
            Token::Commodity => write!(f, "commodity"),
            Token::File => write!(f, "source file"),
            Token::Account => write!(f, "account"),
            Token::Sequence(seq) => write!(f, "{}", seq),
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
            Directive::Include { range, .. } => range.clone(),
            Directive::Price { range, .. } => range.clone(),
            Directive::Open { range, .. } => range.clone(),
            Directive::Transaction { range, .. } => range.clone(),
            Directive::Assertion { range, .. } => range.clone(),
            Directive::Close { range, .. } => range.clone(),
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
