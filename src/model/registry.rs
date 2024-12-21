use std::{cell::RefCell, collections::HashMap, fmt::Display, iter};

use super::{
    entities::{AccountID, AccountType, CommodityID},
    error::ModelError,
};

pub struct Registry {
    commodities_by_name: RefCell<HashMap<String, CommodityID>>,
    accounts_by_name: RefCell<HashMap<String, AccountID>>,

    accounts: RefCell<Vec<Account>>,
    commodities: RefCell<Vec<Commodity>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            accounts_by_name: RefCell::new(HashMap::new()),
            commodities_by_name: RefCell::new(HashMap::new()),
            accounts: Default::default(),
            commodities: Default::default(),
        }
    }

    pub fn account_id(&self, s: &str) -> Result<AccountID, ModelError> {
        if let Some(a) = self.accounts_by_name.borrow().get(s) {
            return Ok(*a);
        }
        let account = Account::new(s)?;
        let id = AccountID {
            id: self.accounts.borrow().len(),
            account_type: account.account_type,
        };
        self.accounts.borrow_mut().push(account);
        self.accounts_by_name.borrow_mut().insert(s.to_string(), id);
        Ok(id)
    }

    pub fn account_name(&self, id: AccountID) -> String {
        self.accounts.borrow()[id.id].name.clone()
    }

    pub fn shorten(&self, account: AccountID, levels: usize) -> Option<AccountID> {
        let name = self
            .account_name(account)
            .split(":")
            .take(levels)
            .collect::<Vec<_>>()
            .join(":");
        self.account_id(&name).ok()
    }

    pub fn commodity_id(&self, s: &str) -> Result<CommodityID, ModelError> {
        if let Some(a) = self.commodities_by_name.borrow().get(s) {
            return Ok(*a);
        }
        let commodity = Commodity::new(s)?;
        let id = CommodityID {
            id: self.commodities.borrow().len(),
        };
        self.commodities.borrow_mut().push(commodity);
        self.commodities_by_name
            .borrow_mut()
            .insert(s.to_string(), id);
        Ok(id)
    }

    pub fn commodity_name(&self, id: CommodityID) -> String {
        self.commodities.borrow()[id.id].name.clone()
    }

    pub fn valuation_account_for(&self, account: AccountID) -> AccountID {
        let account_name = self.account_name(account);
        let name = iter::once("Income")
            .chain(account_name.split(":").skip(1))
            .collect::<Vec<_>>()
            .join(":");
        self.account_id(&name).unwrap()
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
struct Commodity {
    name: String,
}

impl Commodity {
    pub fn new(name: &str) -> Result<Commodity, ModelError> {
        if name.is_empty() || !name.chars().all(char::is_alphanumeric) {
            return Err(ModelError::InvalidCommodityName(name.into()));
        }
        Ok(Commodity {
            name: name.to_string(),
        })
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
struct Account {
    account_type: AccountType,
    name: String,
}

impl Account {
    pub fn new(s: &str) -> Result<Account, ModelError> {
        match s.split(':').collect::<Vec<_>>().as_slice() {
            &[at, ref segments @ ..] => {
                for segment in segments {
                    if segment.is_empty() {
                        return Err(ModelError::InvalidAccountName(s.into()));
                    }
                    if segment.chars().any(|c| !c.is_alphanumeric()) {
                        return Err(ModelError::InvalidAccountName(s.into()));
                    }
                }
                Ok(Account {
                    account_type: AccountType::try_from(at)?,
                    name: s.to_string(),
                })
            }
            _ => Err(ModelError::InvalidAccountName(s.into())),
        }
    }
}

impl Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
