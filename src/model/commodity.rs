use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct Commodity {
    name: String,
}

impl Commodity {
    #[cfg(test)]
    pub fn new(name: &str) -> Arc<Commodity> {
        Arc::new(Commodity {
            name: name.into(),
        })
    }

    fn parse(value: &str) -> Result<Arc<Self>, String> {
        if value.is_empty() || value.chars().any(|c| !c.is_alphanumeric()) {
            return Err(format!("invalid commodity: {}", value));
        }
        Ok(Arc::new(Commodity {
            name: value.to_string(),
        }))
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt() {
        assert_eq!(Commodity::new("USD").to_string(), "USD");
        assert_eq!(Commodity::new("100T").to_string(), "100T");
    }
}

pub struct Commodities {
    commodities: HashMap<String, Arc<Commodity>>,
}

impl Commodities {
    pub fn new() -> Self {
        Commodities {
            commodities: HashMap::new(),
        }
    }

    pub fn get(&self, index: &str) -> Option<Arc<Commodity>> {
        self.commodities.get(index).cloned()
    }

    pub fn create(&mut self, s: &str) -> Result<Arc<Commodity>, String> {
        if let Some(a) = self.commodities.get(s) {
            return Ok(a.clone());
        }
        let commodity = Commodity::parse(s)?;
        self.commodities.insert(s.to_string(), commodity.clone());
        Ok(commodity)
    }
}

impl Default for Commodities {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test_commodities {
    use super::*;

    #[test]
    fn test_create() {
        let mut commodities = Commodities::new();
        assert!(commodities.create("CHF").is_ok());
    }
}
