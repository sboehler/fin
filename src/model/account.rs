use std::convert::TryFrom;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
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

impl TryFrom<&str> for AccountType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let res = match value {
            "Assets" => AccountType::Assets,
            "Liabilities" => AccountType::Liabilities,
            "Equity" => AccountType::Equity,
            "Income" => AccountType::Income,
            "Expenses" => AccountType::Expenses,
            _ => return Err(format!("invalid account type: {}", value)),
        };
        Ok(res)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Account {
    pub account_type: AccountType,
    pub segments: Vec<String>,
}

impl Account {
    pub fn new(account_type: AccountType, segments: &[&str]) -> Account {
        Account {
            account_type,
            segments: segments.iter().map(|s| s.to_string()).collect(),
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

impl TryFrom<&str> for Account {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.split(":").collect::<Vec<_>>().as_slice() {
            &[at, ref segments @ ..] => {
                for segment in segments {
                    if segment.len() == 0 || segment.chars().any(|c| !c.is_alphanumeric()) {
                        return Err(format!("invalid segment: {}", segment));
                    }
                }
                Ok(Account::new(AccountType::try_from(at)?, segments))
            }
            _ => Err(format!("invalid account name: {}", value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt() {
        let tests = [
            (
                Account::new(AccountType::Assets, &vec!["Bank", "Checking"]),
                "Assets:Bank:Checking",
            ),
            (Account::new(AccountType::Expenses, &vec![]), "Expenses"),
        ];
        for (test, expected) in tests.iter() {
            assert_eq!(format!("{}", test), **expected);
        }
    }

    #[test]
    fn test_try_from() {
        assert_eq!(
            Account::new(AccountType::Assets, &["Bank"]),
            Account::try_from("Assets:Bank").unwrap()
        );
        assert_eq!(
            Account::new(AccountType::Assets, &["Bank", "Foo"]),
            Account::try_from("Assets:Bank:Foo").unwrap()
        );
    }
}
