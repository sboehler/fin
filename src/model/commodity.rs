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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt() {
        let tests = [
            (Commodity::new("USD".into()), "USD"),
            (Commodity::new("100T".into()), "100T"),
        ];
        for (test, expected) in tests.iter() {
            assert_eq!(format!("{}", test), **expected);
        }
    }
}
