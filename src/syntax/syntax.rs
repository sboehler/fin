use super::scanner::Range;

pub struct Commodity<'a> {
    pub range: Range<'a>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Date<'a> {
    pub range: Range<'a>,
}
