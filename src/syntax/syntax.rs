use super::scanner::Range;

pub struct Commodity<'a> {
    pub range: Range<'a>,
}

pub struct Date<'a> {
    pub range: Range<'a>,
}
