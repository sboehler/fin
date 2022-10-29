use std::fmt;
use std::fmt::Display;

#[derive(Debug, Copy, Clone, Hash, PartialEq)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct Account {
    pub account_type: AccountType,
    pub segments: Vec<String>,
}

impl Account {
    pub fn new(account_type: AccountType, segments: Vec<String>) -> Account {
        Account {
            account_type,
            segments,
        }
    }
}

impl Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.account_type)?;
        for segment in self.segments.iter() {
            write!(f, ":")?;
            write!(f, "{}", *segment)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt() {
        let tests = [
            (
                Account::new(AccountType::Assets, vec!["Bank".into(), "Checking".into()]),
                "Assets:Bank:Checking",
            ),
            (Account::new(AccountType::Expenses, vec![]), "Expenses"),
        ];
        for (test, expected) in tests.iter() {
            assert_eq!(format!("{}", test), **expected);
        }
    }
}
