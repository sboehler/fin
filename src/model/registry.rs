use std::{cell::RefCell, collections::HashMap, rc::Rc};

use super::{
    entities::{Account, Commodity},
    error::ModelError,
};

pub struct Registry {
    commodities: RefCell<HashMap<String, Rc<Commodity>>>,
    accounts: RefCell<HashMap<String, Rc<Account>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            accounts: RefCell::new(HashMap::new()),
            commodities: RefCell::new(HashMap::new()),
        }
    }

    pub fn account(&self, s: &str) -> Result<Rc<Account>, ModelError> {
        if let Some(a) = self.accounts.borrow().get(s) {
            return Ok(a.clone());
        }
        let a = Rc::new(Account::new(s)?);
        self.accounts.borrow_mut().insert(s.to_string(), a.clone());
        Ok(a)
    }

    pub fn shorten(&self, account: &Rc<Account>, levels: usize) -> Option<Rc<Account>> {
        let segments = account
            .name
            .split(":")
            .take(levels)
            .collect::<Vec<_>>()
            .join(":");
        self.account(&segments).ok()
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
