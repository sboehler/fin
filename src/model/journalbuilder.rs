use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;

use super::entities::{
    AccountID, Assertion, Booking, Close, CommodityID, Interval, Open, Partition, Price,
    SourceFileID, SourceLoc, Transaction,
};
use super::journal::{Day, Journal};
use super::registry::Registry;
use crate::syntax::sourcefile::SourceFile;
use crate::syntax::{
    cst::{self, SyntaxTree},
    error::SyntaxError,
};

pub struct JournalBuilder {
    registry: Registry,
    days: BTreeMap<NaiveDate, Day>,

    current_file: SourceFileID,
}

impl JournalBuilder {
    pub fn new(registry: Registry) -> Self {
        JournalBuilder {
            registry,
            days: Default::default(),
            current_file: SourceFileID(0),
        }
    }

    pub fn build(self) -> Journal {
        Journal::new(Rc::new(self.registry), self.days)
    }

    fn day(&mut self, d: NaiveDate) -> &mut Day {
        self.days.entry(d).or_insert_with(|| Day::new(d))
    }

    pub fn add(
        &mut self,
        tree: &SyntaxTree,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        self.current_file = self.registry.add_source_file(source.clone());
        for d in &tree.directives {
            use cst::Directive::*;
            match d {
                Price(p) => self.price(p, source)?,
                Open(o) => self.open(o, source)?,
                Transaction(t) => self.transaction(t, source)?,
                Assertion(a) => self.assertion(a, source)?,
                Close(c) => self.close(c, source)?,
                Include(_) => (),
                VirtualAccount(va) => self.virtual_account(va, source)?,
            }
        }
        Ok(())
    }

    fn price(
        &mut self,
        p: &cst::Price,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&p.date, source)?;
        let commodity = self.commodity(&p.commodity, source)?;
        let price = self.decimal(&p.price, source)?;
        let target = self.commodity(&p.target, source)?;
        let loc = Some(SourceLoc::new(self.current_file, p.range.clone()));
        self.day(date).prices.push(Price {
            loc,
            date,
            commodity,
            price,
            target,
        });
        Ok(())
    }

    fn open(&mut self, o: &cst::Open, source: &SourceFile) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&o.date, source)?;
        let account = self.account(&o.account, source)?;
        let loc = Some(SourceLoc::new(self.current_file, o.range.clone()));
        self.day(date).openings.push(Open { loc, date, account });
        Ok(())
    }

    fn virtual_account(
        &mut self,
        a: &cst::VirtualAccount,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        let id = self.account(&a.account, source)?;
        let mut regexes = Vec::new();
        for pattern in &a.patterns {
            let regex = Regex::new(&source.text[pattern.clone()]).map_err(|_| SyntaxError {
                range: pattern.clone(),
                want: cst::Token::Regex,
                source: None,
            })?;
            regexes.push(regex);
        }
        self.registry
            .virtual_account(id, regexes)
            .map_err(|_| SyntaxError {
                range: a.range.clone(),
                want: cst::Token::VirtualAccount,
                source: None,
            })?;
        Ok(())
    }

    fn transaction(
        &mut self,
        t: &cst::Transaction,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&t.date, source)?;
        let bookings = t
            .bookings
            .iter()
            .map(|a| {
                Ok(Booking::create(
                    self.account(a.credit, source)?,
                    self.account(a.debit, source)?,
                    self.decimal(a.quantity, source)?,
                    self.commodity(a.commodity, source)?,
                    None,
                ))
            })
            .collect::<std::result::Result<Vec<_>, SyntaxError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let loc = Some(SourceLoc::new(self.current_file, t.range.clone()));
        let mut trx = Transaction {
            loc,
            date,
            description: Rc::new(t.description.text(&source.text).into_owned()),
            bookings,
            targets: None,
        };
        let ts = match &t.addon {
            Some(cst::Addon::Performance { commodities, .. }) => {
                trx.targets = Some(
                    commodities
                        .iter()
                        .map(|c| self.commodity(c, source))
                        .collect::<std::result::Result<Vec<_>, SyntaxError>>()?,
                );
                vec![trx]
            }
            Some(cst::Addon::Accrual {
                start,
                end,
                account,
                interval,
                ..
            }) => {
                let start = self.date(start, source)?;
                let end = self.date(end, source)?;
                let interval = self.interval(interval, source)?;
                let account = self.account(account, source)?;
                self.expand(trx, start, end, interval, account)
            }
            None => vec![trx],
        };
        for t in ts {
            self.day(t.date).transactions.push(t);
        }
        Ok(())
    }

    fn assertion(
        &mut self,
        a: &cst::Assertion,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&a.date, source)?;
        let mut res = a
            .assertions
            .iter()
            .map(|a| {
                let loc = Some(SourceLoc::new(self.current_file, a.range.clone()));
                let account = self.account(&a.account, source)?;
                let balance = self.decimal(&a.balance, source)?;
                let commodity = self.commodity(&a.commodity, source)?;
                Ok(Assertion {
                    loc,
                    date,
                    account,
                    balance,
                    commodity,
                })
            })
            .collect::<std::result::Result<Vec<_>, SyntaxError>>()?;
        self.day(date).assertions.append(&mut res);
        Ok(())
    }

    fn close(
        &mut self,
        c: &cst::Close,
        source: &SourceFile,
    ) -> std::result::Result<(), SyntaxError> {
        let date = self.date(&c.date, source)?;
        let account = self.account(&c.account, source)?;
        let loc = Some(SourceLoc::new(self.current_file, c.range.clone()));
        self.day(date).closings.push(Close { loc, date, account });
        Ok(())
    }

    fn date(
        &mut self,
        date: &cst::Date,
        source: &SourceFile,
    ) -> std::result::Result<NaiveDate, SyntaxError> {
        NaiveDate::parse_from_str(&source.text[date.0.clone()], "%Y-%m-%d").map_err(|_| {
            SyntaxError {
                range: date.0.clone(),
                want: cst::Token::Date,
                source: None,
            }
        })
    }

    fn decimal(
        &self,
        decimal: &cst::Decimal,
        source: &SourceFile,
    ) -> std::result::Result<rust_decimal::Decimal, SyntaxError> {
        rust_decimal::Decimal::from_str_exact(&source.text[decimal.0.clone()]).map_err(|_| {
            SyntaxError {
                range: decimal.0.clone(),
                want: cst::Token::Decimal,
                source: None,
            }
        })
    }

    fn interval(
        &mut self,
        d: &Range<usize>,
        source: &SourceFile,
    ) -> std::result::Result<Interval, SyntaxError> {
        match &source.text[d.clone()] {
            "daily" => Ok(Interval::Daily),
            "weekly" => Ok(Interval::Weekly),
            "monthly" => Ok(Interval::Monthly),
            "quarterly" => Ok(Interval::Quarterly),
            "yearly" => Ok(Interval::Yearly),
            "once" => Ok(Interval::Single),
            _ => Err(SyntaxError {
                range: d.clone(),
                want: cst::Token::Decimal,
                source: None,
            }),
        }
    }

    fn commodity(
        &mut self,
        commodity: &cst::Commodity,
        source: &SourceFile,
    ) -> std::result::Result<CommodityID, SyntaxError> {
        self.registry
            .commodity_id(&source.text[commodity.0.clone()])
            .map_err(|_| SyntaxError {
                range: commodity.0.clone(),
                want: cst::Token::Commodity,
                source: None,
            })
    }

    fn account(
        &mut self,
        account: &cst::Account,
        source: &SourceFile,
    ) -> std::result::Result<AccountID, SyntaxError> {
        self.registry
            .account_id(&source.text[account.range.clone()])
            .map_err(|_| SyntaxError {
                range: account.range.clone(),
                want: cst::Token::Account,
                source: None,
            })
    }

    fn expand(
        &self,
        t: Transaction,
        start: NaiveDate,
        end: NaiveDate,
        interval: Interval,
        account: AccountID,
    ) -> Vec<Transaction> {
        let mut res: Vec<Transaction> = Vec::new();
        let p = Partition::from_interval(start, end, interval);
        for b in t.bookings {
            if b.account.account_type.is_al() {
                res.push(Transaction {
                    loc: t.loc,
                    date: t.date,
                    description: t.description.clone(),
                    bookings: Booking::create(account, b.account, b.quantity, b.commodity, None),
                    targets: t.targets.clone(),
                });
            }

            if b.account.account_type.is_ie() {
                let n = Decimal::from(p.periods.len());
                let quantity = (b.quantity / n).round_dp_with_strategy(
                    2,
                    rust_decimal::RoundingStrategy::MidpointAwayFromZero,
                );
                let rem = b.quantity - quantity * n;
                for (i, dt) in p.periods.iter().enumerate() {
                    let a = match i {
                        0 => quantity + rem,
                        _ => quantity,
                    };
                    res.push(Transaction {
                        loc: t.loc,
                        date: dt.1,
                        description: format!(
                            "{} (accrual {}/{})",
                            t.description,
                            i + 1,
                            p.periods.len()
                        )
                        .into(),
                        bookings: Booking::create(account, b.account, a, b.commodity, None),
                        targets: t.targets.clone(),
                    });
                }
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse_text;
    use pretty_assertions::assert_eq;

    /// The bookings `text` contributes to the journal, as
    /// `(credit, debit, quantity, commodity)`.
    fn bookings(text: &str) -> Vec<(String, String, String, String)> {
        let tree = parse_text(text).expect("parses");
        let source = SourceFile {
            path: None,
            text: text.to_string(),
        };
        let mut builder = JournalBuilder::new(Registry::new());
        builder.add(&tree, &source).expect("builds");
        let registry = &builder.registry;
        builder
            .days
            .values()
            .flat_map(|day| &day.transactions)
            // Bookings come in (credit, debit) pairs, so each pair is read
            // from its debit side.
            .flat_map(|t| t.bookings.iter().skip(1).step_by(2))
            .map(|b| {
                (
                    registry.account_name(b.other),
                    registry.account_name(b.account),
                    b.quantity.to_string(),
                    registry.commodity_name(b.commodity),
                )
            })
            .collect()
    }

    fn booking(
        credit: &str,
        debit: &str,
        quantity: &str,
        commodity: &str,
    ) -> (String, String, String, String) {
        (
            credit.to_string(),
            debit.to_string(),
            quantity.to_string(),
            commodity.to_string(),
        )
    }

    /// Both notations describe the same bookings.
    #[test]
    fn groups_and_lines_agree() {
        let groups = "2026-06-24\n  Buy 11 VT\n\
                      Assets:IBKR\n\
                      -> Expenses:Trading 1698.95 USD\n\
                      -> Expenses:Fees 1.00 USD\n\
                      Expenses:Trading\n\
                      -> Assets:IBKR 11 VT\n";
        let lines = "2026-06-24 \"Buy 11 VT\"\n\
                     Assets:IBKR Expenses:Trading 1698.95 USD\n\
                     Assets:IBKR Expenses:Fees 1.00 USD\n\
                     Expenses:Trading Assets:IBKR 11 VT\n";
        assert_eq!(
            vec![
                booking("Assets:IBKR", "Expenses:Trading", "1698.95", "USD"),
                booking("Assets:IBKR", "Expenses:Fees", "1.00", "USD"),
                booking("Expenses:Trading", "Assets:IBKR", "11", "VT"),
            ],
            bookings(groups)
        );
        assert_eq!(bookings(lines), bookings(groups));
    }

    /// A group with the amounts on the credit side books into its single
    /// debit account, and a negative amount reverses the direction.
    #[test]
    fn many_credits_to_one_debit() {
        let text = "2026-06-24\n  Rent\n\
                    Assets:Bank 1200 CHF\n\
                    Assets:Cash -100 CHF\n\
                    -> Expenses:Rent\n";
        assert_eq!(
            vec![
                booking("Assets:Bank", "Expenses:Rent", "1200", "CHF"),
                booking("Expenses:Rent", "Assets:Cash", "100", "CHF"),
            ],
            bookings(text)
        );
    }

    /// A reversed arrow books the other way round, which is the same as
    /// swapping the accounts of the group.
    #[test]
    fn reversed_arrows_book_the_other_way() {
        let text = "2026-06-24\n  Rebalance\n\
                    Assets:IBKR\n\
                    -> Expenses:Trading 5 CHF\n\
                    <- Income:Dividends 10 CHF\n";
        assert_eq!(
            vec![
                booking("Assets:IBKR", "Expenses:Trading", "5", "CHF"),
                booking("Income:Dividends", "Assets:IBKR", "10", "CHF"),
            ],
            bookings(text)
        );
    }

    /// The description of a grouped transaction reaches the journal with its
    /// line breaks, and its addon is read as usual.
    #[test]
    fn keeps_description_and_addon() {
        let text = "@performance(VT)\n\
                    2026-06-24\n\
                    \x20 Buy 11 VT\n\
                    \x20 at 154.45 USD\n\
                    Assets:IBKR\n\
                    -> Expenses:Trading 11 VT\n";
        let tree = parse_text(text).expect("parses");
        let source = SourceFile {
            path: None,
            text: text.to_string(),
        };
        let mut builder = JournalBuilder::new(Registry::new());
        builder.add(&tree, &source).expect("builds");
        let t = builder
            .days
            .values()
            .flat_map(|day| &day.transactions)
            .next()
            .expect("a transaction");
        assert_eq!("Buy 11 VT\nat 154.45 USD", *t.description);
        assert_eq!(1, t.targets.as_ref().expect("targets").len());
    }
}
