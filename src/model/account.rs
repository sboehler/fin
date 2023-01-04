use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum AccountType {
    Assets,
    Liabilities,
    Equity,
    Income,
    Expenses,
}
impl AccountType {
    pub fn is_al(&self) -> bool {
        *self == Self::Assets || *self == Self::Liabilities
    }

    pub fn is_ie(&self) -> bool {
        *self == Self::Income || *self == Self::Expenses
    }
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
        match value.split(':').collect::<Vec<_>>().as_slice() {
            &[at, ref segments @ ..] => {
                for segment in segments {
                    if segment.is_empty() || segment.chars().any(|c| !c.is_alphanumeric()) {
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

pub struct Accounts {
    accounts: HashMap<String, Arc<Account>>,
    parents: HashMap<Arc<Account>, Arc<Account>>,
    children: HashMap<Arc<Account>, HashSet<Arc<Account>>>,
}

impl Accounts {
    pub fn new() -> Accounts {
        use super::AccountType::*;
        Accounts {
            accounts: vec![Assets, Liabilities, Equity, Income, Expenses]
                .into_iter()
                .map(|t| (t.to_string(), Arc::new(Account::new(t, &[]))))
                .collect(),
            parents: HashMap::new(),
            children: HashMap::new(),
        }
    }

    pub fn get(&self, index: &str) -> Option<Arc<Account>> {
        self.accounts.get(index).cloned()
    }

    pub fn create(&mut self, s: &str) -> Result<Arc<Account>, String> {
        if let Some(a) = self.accounts.get(s) {
            return Ok(a.clone());
        }
        let account = Arc::new(Account::try_from(s)?);
        self.accounts.insert(s.to_string(), account.clone());
        if let Some(parent) = s.rfind(':').map(|i| self.create(&s[..i])).transpose()? {
            self.parents.insert(account.clone(), parent.clone());
            self.children
                .entry(parent)
                .or_default()
                .insert(account.clone());
        }
        Ok(account)
    }

    pub fn children(&self, a: &Arc<Account>) -> Vec<&Arc<Account>> {
        self.children
            .get(a)
            .map(|hs| hs.iter().collect::<Vec<_>>())
            .unwrap_or_default()
    }

    pub fn parent(&self, a: &Account) -> Option<&Arc<Account>> {
        self.parents.get(a)
    }
}

impl Default for Accounts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test_accounts {
    use super::*;

    #[test]
    fn test_get() {
        let a = Accounts::new();
        assert_eq!(
            a.get("Assets").unwrap(),
            Arc::new(Account::new(AccountType::Assets, &[]))
        )
    }

    #[test]
    fn test_create() {
        let mut accounts = Accounts::new();
        let afb = accounts.create("Assets:Foo:Bar").unwrap();
        let af = accounts.get("Assets:Foo").unwrap();
        let a = accounts.get("Assets").unwrap();
        assert!(accounts.children(&a).contains(&&af));
        assert!(accounts.children(&af).contains(&&afb));
        assert_eq!(accounts.parent(&a), None);
        assert_eq!(accounts.parent(&af), Some(&a));
        assert_eq!(accounts.parent(&afb), Some(&af));
    }
}
