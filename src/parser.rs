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
    let b = read_string(s, 10)
        .map_err(|_| Error::new(ErrorKind::UnexpectedEof, format!("Expected date, got EOF")))?;
    match NaiveDate::parse_from_str(b.as_str(), "%Y-%m-%d") {
        Ok(d) => Ok(d),
        Err(_) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid date '{}'", b),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::read_while;
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
        let tests = [
            ("0202-02-02", chrono::NaiveDate::from_ymd(202, 2, 2), ""),
            ("2020-09-15 ", chrono::NaiveDate::from_ymd(2020, 9, 15), " "),
        ];
        for (test, expected, remainder) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_date(&mut s)?, *expected);
            assert_eq!(read_while(&mut s, |_| true)?, *remainder)
        }
        Ok(())
    }

    #[test]
    fn test_playground() {
        assert_eq!("00000202".parse::<i32>().unwrap(), 202)
    }
}
