use core::fmt;
use std::fmt::Display;

use chrono::{Datelike, Days, Months, NaiveDate};

#[derive(Clone, Copy, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum Interval {
    Once,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use super::Interval::*;
        match self {
            Once => write!(f, "once"),
            Daily => write!(f, "daily"),
            Weekly => write!(f, "weekly"),
            Monthly => write!(f, "monthly"),
            Quarterly => write!(f, "quarterly"),
            Yearly => write!(f, "yearly"),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl Period {
    pub fn dates(&self, interval: Interval, n: Option<usize>) -> Vec<NaiveDate> {
        if interval == Interval::Once {
            return vec![self.end];
        }
        let mut res = Vec::new();
        let mut d = self.end;
        let mut counter = 0;
        while d >= self.start {
            match n {
                Some(n) if counter == n => break,
                Some(_) => counter += 1,
                None => (),
            }
            res.push(d);
            d = start_of(d, interval)
                .and_then(|d| d.checked_sub_days(Days::new(1)))
                .unwrap();
        }
        res.reverse();
        res
    }
}

/// StartOf returns the first date in the given period which
/// contains the receiver.
pub fn start_of(d: NaiveDate, p: Interval) -> Option<NaiveDate> {
    use super::Interval::*;
    match p {
        Once | Daily => Some(d),
        Weekly => d.checked_sub_days(Days::new(d.weekday().number_from_monday() as u64 - 1)),
        Monthly => d.checked_sub_days(Days::new((d.day() - 1) as u64)),
        Quarterly => NaiveDate::from_ymd_opt(d.year(), ((d.month() - 1) / 3 * 3) + 1, 1),
        Yearly => NaiveDate::from_ymd_opt(d.year(), 1, 1),
    }
}

/// StartOf returns the first date in the given period which
/// contains the receiver.
pub fn end_of(d: NaiveDate, p: Interval) -> Option<NaiveDate> {
    use super::Interval::*;
    match p {
        Once | Daily => Some(d),
        Weekly => d.checked_add_days(Days::new(7 - d.weekday().number_from_monday() as u64)),
        Monthly => start_of(d, Monthly)
            .and_then(|d| d.checked_add_months(Months::new(1)))
            .and_then(|d| d.checked_sub_days(Days::new(1))),
        Quarterly => start_of(d, Quarterly)
            .and_then(|d| d.checked_add_months(Months::new(3)))
            .and_then(|d| d.checked_sub_days(Days::new(1))),
        Yearly => NaiveDate::from_ymd_opt(d.year(), 12, 31),
    }
}

#[cfg(test)]
mod test_period {

    use super::Interval::*;
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn dt(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, d)
    }

    #[test]
    fn test_dates() {
        assert_eq!(
            Period {
                start: date(2022, 1, 1),
                end: date(2022, 3, 20),
            }
            .dates(Monthly, None),
            vec![date(2022, 1, 31), date(2022, 2, 28), date(2022, 3, 20)]
        );
        assert_eq!(
            Period {
                start: date(2022, 1, 1),
                end: date(2022, 12, 20),
            }
            .dates(Monthly, Some(4)),
            vec![
                date(2022, 9, 30),
                date(2022, 10, 31),
                date(2022, 11, 30),
                date(2022, 12, 20)
            ]
        )
    }

    #[test]
    fn test_start_of() {
        let d = date(2022, 6, 22);
        assert_eq!(start_of(d, Once), dt(2022, 6, 22));
        assert_eq!(start_of(d, Daily), dt(2022, 6, 22));
        assert_eq!(start_of(d, Weekly), dt(2022, 6, 20));
        assert_eq!(start_of(d, Monthly), dt(2022, 6, 1));
        assert_eq!(start_of(d, Quarterly), dt(2022, 4, 1));
        assert_eq!(start_of(d, Yearly), dt(2022, 1, 1))
    }

    #[test]
    fn test_end_of() {
        let d = date(2022, 6, 22);
        assert_eq!(end_of(d, Once), dt(2022, 6, 22));
        assert_eq!(end_of(d, Daily), dt(2022, 6, 22));
        assert_eq!(end_of(d, Weekly), dt(2022, 6, 26));
        assert_eq!(end_of(d, Monthly), dt(2022, 6, 30));
        assert_eq!(end_of(d, Quarterly), dt(2022, 6, 30));
        assert_eq!(end_of(d, Yearly), dt(2022, 12, 31))
    }
}

// // Today returns today's
// func Today() time.Time {
// 	now := time.Now().Local()
// 	return Date(now.Year(), now.Month(), now.Day())
// }

// type Period struct {
// 	Start, End time.Time
// }

// func (p Period) Clip(p2 Period) Period {
// 	if p2.Start.After(p.Start) {
// 		p.Start = p2.Start
// 	}
// 	if p2.End.Before(p.End) {
// 		p.End = p2.End
// 	}
// 	return p
// }

// func (period Period) Dates(p Interval, n int) []time.Time {
// 	if p == Once {
// 		return []time.Time{period.End}
// 	}
// 	var res []time.Time
// 	for t := period.Start; !t.After(period.End); t = EndOf(t, p).AddDate(0, 0, 1) {
// 		ed := EndOf(t, p)
// 		if ed.After(period.End) {
// 			ed = period.End
// 		}
// 		res = append(res, ed)
// 	}
// 	if n > 0 && len(res) > n {
// 		res = res[len(res)-n:]
// 	}
// 	return res
// }

// func (p Period) Contains(t time.Time) bool {
// 	return !t.Before(p.Start) && !t.After(p.End)
// }

// func Align(ds []time.Time) mapper.Mapper[time.Time] {
// 	return func(d time.Time) time.Time {
// 		index := sort.Search(len(ds), func(i int) bool {
// 			// find first i where ds[i] >= t
// 			return !ds[i].Before(d)
// 		})
// 		if index < len(ds) {
// 			return ds[index]
// 		}
// 		return time.Time{}
// 	}
// }
