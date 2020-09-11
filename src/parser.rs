use crate::model::AccountType;
use crate::scanner::{read_identifier, read_string, Scanner};
use chrono::NaiveDate;
use std::io::{Error, ErrorKind, Read, Result};

pub fn parse_account_type<R: Read>(s: &mut Scanner<R>) -> Result<AccountType> {
    let s = read_identifier(s)?;
    match s.as_str() {
        "Assets" => Ok(AccountType::Assets),
        "Liabilities" => Ok(AccountType::Liabilities),
        "Equity" => Ok(AccountType::Equity),
        "Income" => Ok(AccountType::Income),
        "Expenses" => Ok(AccountType::Expenses),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected account type, got '{}'", s),
        )),
    }
}

pub fn parse_date<R: Read>(s: &mut Scanner<R>) -> Result<NaiveDate> {
    let r = read_string(s, 10)?;
    NaiveDate::parse_from_str(r.as_str(), "%Y-%m-%d")
        .map_err(|_| Error::new(ErrorKind::InvalidData, format!("Invalid date '{}'", r)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Result;

    #[test]
    fn test_parse_account_type() -> Result<()> {
        let mut s = Scanner::new("Assets".as_bytes());
        s.advance()?;
        assert_eq!(parse_account_type(&mut s)?, AccountType::Assets);
        Ok(())
    }

    #[test]
    fn test_parse_date() -> Result<()> {
        let mut s = Scanner::new("2020-02-03".as_bytes());
        s.advance()?;
        assert_eq!(parse_date(&mut s)?, NaiveDate::from_ymd(2020, 2, 3));
        Ok(())
    }
}
