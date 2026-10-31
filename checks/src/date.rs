//! The game's calendar.
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A game date, as the game keeps it: hours since 1 January 5000 BC in a
/// calendar of 365-day years (docs/research/dates.md). In JSON
/// `"year.month.day"`, such as `"1444.11.11"`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(pub i32);

/// The days of each month, January's first: the game's calendar has no leap
/// years. The framework's one copy of it.
const MONTH_DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const EPOCH_YEAR: i32 = -5000;

impl Date {
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        let length = *MONTH_DAYS.get(month.checked_sub(1)? as usize)?;
        if day == 0 || day as i32 > length {
            return None;
        }
        let before: i32 = MONTH_DAYS[..month as usize - 1].iter().sum();
        let days = year.checked_sub(EPOCH_YEAR)?.checked_mul(365)?.checked_add(before + day as i32 - 1)?;
        days.checked_mul(24).filter(|_| days >= 0).map(Self)
    }

    /// Year, month (from 1) and day (from 1).
    pub fn ymd(self) -> (i32, u32, u32) {
        let days = self.0.div_euclid(24);
        let mut day = days.rem_euclid(365);
        let year = days.div_euclid(365) + EPOCH_YEAR;
        let mut month = 0;
        while day >= MONTH_DAYS[month] {
            day -= MONTH_DAYS[month];
            month += 1;
        }
        (year, month as u32 + 1, day as u32 + 1)
    }

    /// The date `months` on, as the game adds months (0x14c8030): the same
    /// day of the month, or the month's last when that month is shorter, at
    /// the same hour; None past the calendar's end.
    pub fn months_later(self, months: u32) -> Option<Self> {
        let (year, month, day) = self.ymd();
        let month = month - 1 + months;
        let (year, month) = (year.checked_add((month / 12) as i32)?, month % 12);
        let day = day.min(MONTH_DAYS[month as usize] as u32);
        Some(Self(Self::from_ymd(year, month + 1, day)?.0.checked_add(self.0.rem_euclid(24))?))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, '.');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Self::from_ymd(year, month, day)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (year, month, day) = self.ymd();
        write!(f, "{year}.{month}.{day}")
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for Date {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Date {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| de::Error::custom(format!("{text:?} is not a date (year.month.day, such as 1444.11.11)")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dates_as_the_game_keeps_them() {
        // docs/research/dates.md: 1444.11.11 is 0x035D7710.
        let start = Date(0x035D_7710);
        assert_eq!(start.ymd(), (1444, 11, 11));
        assert_eq!(start.to_string(), "1444.11.11");
        assert_eq!(Date::parse("1444.11.11"), Some(start));
        assert_eq!(Date::parse("1445.01.01"), Date::from_ymd(1445, 1, 1));
        assert_eq!(Date::from_ymd(1445, 1, 1).unwrap().0 - start.0, 51 * 24);
        // Every day of a year comes back as itself, and the hour is ignored.
        let first = Date::from_ymd(1450, 1, 1).unwrap();
        for day in 0..365 {
            let date = Date(first.0 + day * 24 + 13);
            let (y, m, d) = date.ymd();
            assert_eq!(Date::from_ymd(y, m, d), Some(Date(first.0 + day * 24)));
        }
        assert_eq!(Date(first.0 + 364 * 24).to_string(), "1450.12.31");
        for bad in ["1444.2.29", "1444.13.1", "1444.0.1", "1444.1.0", "1444.11", "x.1.1", "-5001.1.1"] {
            assert_eq!(Date::parse(bad), None, "{bad}");
        }
        assert_eq!(serde_json::to_value(start).unwrap(), json!("1444.11.11"));
    }

    #[test]
    fn months_on_keep_the_day_or_take_the_month_s_last() {
        let date = |year, month, day| Date::from_ymd(year, month, day).unwrap();
        let later = |from: Date, months| from.months_later(months).unwrap();
        assert_eq!(later(date(1444, 11, 11), 2), date(1445, 1, 11));
        assert_eq!(later(date(1444, 12, 31), 2), date(1445, 2, 28));
        assert_eq!(later(date(1445, 1, 31), 2), date(1445, 3, 31));
        assert_eq!(later(date(1445, 8, 31), 2), date(1445, 10, 31));
        assert_eq!(later(date(1445, 10, 31), 2), date(1445, 12, 31));
        assert_eq!(later(date(1445, 12, 30), 2), date(1446, 2, 28));
        assert_eq!(later(Date(date(1445, 3, 1).0 + 5), 2), Date(date(1445, 5, 1).0 + 5));
        assert_eq!(later(date(1445, 1, 31), 1), date(1445, 2, 28));
        assert_eq!(later(date(1445, 11, 30), 15), date(1447, 2, 28));
        assert_eq!(later(date(1445, 5, 9), 0), date(1445, 5, 9));
        assert_eq!(Date(i32::MAX).months_later(1), None);
    }
}
