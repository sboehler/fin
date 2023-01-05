use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct Commodity {
    name: String,
}

impl Commodity {
    pub fn new(name: &str) -> Commodity {
        Commodity { name: name.into() }
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
