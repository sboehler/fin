use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Tag {
    tag: String,
}

impl Tag {
    pub fn new(tag: String) -> Tag {
        Tag {
            tag,
        }
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.tag)
    }
}
