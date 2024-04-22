use super::scanner::Range;

pub struct Commodity<'a> {
    pub range: Range<'a>,
}

pub struct Account<'a> {
    pub range: Range<'a>,
    pub segments: Vec<Range<'a>>,
}

#[derive(Eq, PartialEq, Debug)]
pub struct Date<'a> {
    pub range: Range<'a>,
}
