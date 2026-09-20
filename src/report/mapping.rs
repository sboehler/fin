//! Account projection shared by the reports.
//!
//! Two mechanisms, applied in order:
//!
//! - virtual accounts (`-r`): an account matching one of a virtual account's
//!   patterns is replaced by that virtual account.
//! - mappings (`-m LEVEL,REGEX`): an account whose (possibly already
//!   substituted) name matches `REGEX` is shortened to `LEVEL` segments.
//!
//! Mapping an account to `None` drops it from the report.

use std::str::FromStr;

use regex::Regex;

use crate::model::{entities::AccountID, error::ModelError, registry::Registry};

#[derive(Clone)]
pub struct Mapping {
    regex: Regex,
    level: usize,
}

impl FromStr for Mapping {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(2, ',').collect();
        if parts.len() > 2 {
            return Err(format!("invalid mapping: {s}"));
        }
        let level = parts[0]
            .parse()
            .map_err(|e| format!("invalid mapping: {e}"))?;
        let regex = Regex::new(parts[1]).map_err(|e| format!("invalid mapping: {e}"))?;
        Ok(Mapping { regex, level })
    }
}

#[derive(Default, Clone)]
pub struct AccountMapper {
    mapping: Vec<Mapping>,
    account_map: Vec<(Regex, AccountID)>,
}

impl AccountMapper {
    /// Resolves `vaccounts` (account names) against the registry and expands
    /// them into the pattern list they were declared with.
    pub fn new(
        registry: &Registry,
        mapping: Vec<Mapping>,
        vaccounts: &[String],
    ) -> Result<Self, ModelError> {
        let account_map = vaccounts
            .iter()
            .map(|name| registry.account_id(name))
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .flat_map(|id| {
                registry
                    .get_patterns(id)
                    .into_iter()
                    .map(|regex| (regex, *id))
            })
            .collect();
        Ok(Self {
            mapping,
            account_map,
        })
    }

    pub fn map(&self, registry: &Registry, mut account: AccountID) -> Option<AccountID> {
        let name = registry.account_name(account);
        for (regex, id) in &self.account_map {
            if regex.is_match(&name) {
                account = *id;
                break;
            }
        }
        let name = registry.account_name(account);
        for mapping in &self.mapping {
            if mapping.regex.is_match(&name) {
                return registry.shorten(account, mapping.level);
            }
        }
        Some(account)
    }
}
