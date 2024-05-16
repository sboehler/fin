use std::{collections::HashMap, rc::Rc};

use super::{
    entities::{Account, Commodity},
    error::ModelError,
};

pub struct Registry {
    commodities: HashMap<String, Rc<Commodity>>,
    accounts: HashMap<String, Rc<Account>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            accounts: HashMap::new(),
            commodities: HashMap::new(),
        }
    }

    pub fn account(&mut self, s: &str) -> Result<Rc<Account>, ModelError> {
        if let Some(a) = self.accounts.get(s) {
            return Ok(a.clone());
        }
        let a = Rc::new(Account::new(s)?);
        self.accounts.insert(s.to_string(), a.clone());
        Ok(a)
    }

    pub fn commodity(&mut self, s: &str) -> Result<Rc<Commodity>, ModelError> {
        if let Some(a) = self.commodities.get(s) {
            return Ok(a.clone());
        }
        let commodity = Rc::new(Commodity::new(s)?);
        self.commodities.insert(s.to_string(), commodity.clone());
        Ok(commodity)
    }
}
