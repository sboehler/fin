use crate::model::{Account, Accounts};
use std::sync::{Arc, RwLock};

pub struct Context {
    pub accounts: RwLock<Accounts>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            accounts: RwLock::new(Accounts::default()),
        }
    }

    pub fn account(&self, s: &str) -> Result<Arc<Account>, String> {
        if let Some(a) = self.accounts.read().unwrap().get(s) {
            return Ok(a);
        }
        self.accounts.write().unwrap().create(s)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test_context {
    use super::*;

    #[test]
    fn test_account() {
        let ctx = Context::new();
        assert!(ctx.account("Assets").is_ok());
        assert!(ctx.account("Assets:Foo").is_ok());
        assert!(ctx.account("Foobar").is_err());
        let a = ctx.account("Assets").unwrap();
        assert!(!ctx.accounts.read().unwrap().children(&a).is_empty())
    }
}
