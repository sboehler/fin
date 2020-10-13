use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Commodity {
    name: String,
}

impl Commodity {
    pub fn new(name: String) -> Commodity {
        Commodity { name }
    }
}

impl Display for Commodity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}
