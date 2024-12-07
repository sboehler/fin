use std::{cell::RefCell, collections::HashMap, rc::Rc};

use super::{
    entities::{Account, AccountID, Commodity},
    error::ModelError,
};

pub struct Registry {
    commodities: RefCell<HashMap<String, Rc<Commodity>>>,
    accounts_by_name: RefCell<HashMap<String, AccountID>>,

    accounts: RefCell<Vec<Account>>,
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
            commodities: RefCell::new(HashMap::new()),
            accounts: Default::default(),
        }
    }

    pub fn account_id(&self, s: &str) -> Result<AccountID, ModelError> {
        if let Some(a) = self.accounts_by_name.borrow().get(s) {
            return Ok(*a);
        }
        let a = Account::new(s)?;
        let account_type = a.account_type;
        self.accounts.borrow_mut().push(a);
        let id = AccountID {
            id: self.accounts.borrow().len() - 1,
            account_type: account_type,
        };
        self.accounts_by_name.borrow_mut().insert(s.to_string(), id);
        Ok(id)
    }

    pub fn account_name(&self, id: AccountID) -> String {
        self.accounts.borrow()[id.id].name.clone()
    }

    pub fn shorten(&self, account: AccountID, levels: usize) -> Option<AccountID> {
        let segments = self
            .account_name(account)
            .split(":")
            .take(levels)
            .collect::<Vec<_>>()
            .join(":");
        self.account_id(&segments).ok()
    }

    pub fn commodity(&self, s: &str) -> Result<Rc<Commodity>, ModelError> {
        if let Some(a) = self.commodities.borrow().get(s) {
            return Ok(a.clone());
        }
        let commodity = Rc::new(Commodity::new(s)?);
        self.commodities
            .borrow_mut()
            .insert(s.to_string(), commodity.clone());
        Ok(commodity)
    }
}
