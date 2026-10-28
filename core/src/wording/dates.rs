//! Dates the game writes in English order whatever the language
//! (docs/research/dates.md). The long date ("11 November 1444", in some 226
//! places) comes from the localisation key `LONG_EU3_DATE_STRING`, which a
//! language mod words as it likes, and is left alone. The rest are made in
//! code, in a fixed order: the top bar's date and the timeline's, through a
//! small format language (0x14c7890); "November 1444" (0x6e6100); the same,
//! moved on to the first of the next month and with a comma, for truces and
//! the like (0x6e5b40); "1444 November 11" (0x6e5dc0); and "2560 BC".
//!
//! A language that orders dates otherwise gives its templates in the
//! framework's localisation (`EU4HARMONY_DATE*`, with `$YEAR$`,
//! `$MONTH_NUMBER$` and `$DAY$`), and those dates are written from them in
//! the player language. Without them the game's own function runs.
//!
//! Each of the functions takes the date, a `std::string` slot it constructs
//! the text in, and, for the format language, the format.
use crate::lang;
use crate::loc;
use crate::points::Point;
use crate::unicode::{game_text, text_loops};
use eu4_harmony_loc::Language;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The days of each month: the game's calendar has no leap years.
const DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A date as the game keeps it: hours since 1 January 5000 BC (0xcfd70).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Date {
    year: i32,
    /// 1 to 12.
    month: u32,
    /// 1 to 31.
    day: u32,
}

impl Date {
    fn from_hours(hours: i32) -> Self {
        let days = hours.div_euclid(24);
        let mut day = days.rem_euclid(365) as u32;
        let mut month = 0;
        while day >= DAYS[month] {
            day -= DAYS[month];
            month += 1;
        }
        Self { year: days.div_euclid(365) - 5000, month: month as u32 + 1, day: day + 1 }
    }

    /// The first of the next month, unless this is a first already, as the
    /// game rounds a truce's end.
    fn next_month_start(self) -> Self {
        match (self.day, self.month) {
            (1, _) => self,
            (_, 12) => Self { year: self.year + 1, month: 1, day: 1 },
            _ => Self { month: self.month + 1, day: 1, ..self },
        }
    }
}

/// What a date is written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Day,
    Month,
    /// The year with its era.
    Year,
}

/// `date` as `shape` in `language`'s own template, if it has one.
fn text(language: Language, shape: Shape, date: Date) -> Option<String> {
    let (key, year) = match shape {
        Shape::Day => ("EU4HARMONY_DATE", date.year),
        Shape::Month => ("EU4HARMONY_DATE_MONTH", date.year),
        Shape::Year if date.year < 0 => ("EU4HARMONY_DATE_YEAR_BC", -date.year),
        Shape::Year => ("EU4HARMONY_DATE_YEAR", date.year),
    };
    let (year, month, day) = (year.to_string(), date.month.to_string(), date.day.to_string());
    loc::framework_own(language, key, &[("YEAR", &year), ("MONTH_NUMBER", &month), ("DAY", &day)])
}

type DateText = unsafe extern "C" fn(*const i32, *mut u8, usize) -> *mut u8;

static ORIGINAL_FORMAT: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_MONTH: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_NEXT_MONTH: AtomicUsize = AtomicUsize::new(0);
static ORIGINAL_DAY: AtomicUsize = AtomicUsize::new(0);

/// Constructs `text` in `out`, a `std::string` slot nothing has constructed
/// yet; the caller frees it as it would the game's own.
unsafe fn construct(out: *mut u8, text: &str) {
    std::ptr::write_bytes(out, 0, 0x20);
    *(out.add(0x18) as *mut usize) = 15;
    text_loops::append(out as usize, &game_text::from_str(text));
}

/// The date at `date` written in the player language's template for the
/// shape `choose` picks, or by the game's own function when there is none.
unsafe fn write(original: &AtomicUsize, date: *const i32, out: *mut u8, extra: usize, choose: impl FnOnce(Date) -> Option<(Shape, Date)>) -> *mut u8 {
    let written = crate::guarded("date text", || {
        let (shape, date) = choose(Date::from_hours(*date))?;
        let text = text(lang::player(), shape, date)?;
        construct(out, &text);
        Some(())
    });
    if written.is_some() {
        return out;
    }
    let original: DateText = std::mem::transmute(original.load(Ordering::Acquire));
    original(date, out, extra)
}

/// The format language (0x14c7890): the three formats the game uses.
extern "C" fn formatted(date: *const i32, out: *mut u8, format: usize) -> *mut u8 {
    unsafe {
        write(&ORIGINAL_FORMAT, date, out, format, |date| {
            let shape = match game_text::std_string(format as *const u8) {
                b"d w mw w y" => Shape::Day,
                b"y, w mw" => Shape::Month,
                b"ywe" => Shape::Year,
                _ => return None,
            };
            Some((shape, date))
        })
    }
}

extern "C" fn month_year(date: *const i32, out: *mut u8, extra: usize) -> *mut u8 {
    unsafe { write(&ORIGINAL_MONTH, date, out, extra, |date| Some((Shape::Month, date))) }
}

extern "C" fn next_month_year(date: *const i32, out: *mut u8, extra: usize) -> *mut u8 {
    unsafe { write(&ORIGINAL_NEXT_MONTH, date, out, extra, |date| Some((Shape::Month, date.next_month_start()))) }
}

extern "C" fn year_month_day(date: *const i32, out: *mut u8, extra: usize) -> *mut u8 {
    unsafe { write(&ORIGINAL_DAY, date, out, extra, |date| Some((Shape::Day, date))) }
}

pub fn points() -> Vec<Point> {
    let f = |f: extern "C" fn(*const i32, *mut u8, usize) -> *mut u8| f as usize;
    vec![
        // The calendar the dates are read in: hours since 5000 BC.
        Point::keep(0xcfd70usize, "44 8b 01 41 b9 01 00 00 00 41 81 e8 c0 55 9c 02", "date year"),
        Point::replace(0x14c7890usize, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 48", "date in a format", f(formatted), &ORIGINAL_FORMAT),
        Point::replace(0x6e6100usize, "48 89 5c 24 08 57 48 81 ec c0 00 00 00 48 8b fa", "date as month and year", f(month_year), &ORIGINAL_MONTH),
        Point::replace(0x6e5b40usize, "48 89 5c 24 18 48 89 74 24 20 48 89 54 24 10 55", "date as the next month and year", f(next_month_year), &ORIGINAL_NEXT_MONTH),
        Point::replace(0x6e5dc0usize, "48 89 5c 24 08 48 89 74 24 18 48 89 54 24 10 55", "date as year, month and day", f(year_month_day), &ORIGINAL_DAY),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_as_the_game_keeps_them() {
        // 1444.11.11, as the game stores the start date.
        assert_eq!(Date::from_hours(0x035d7710), Date { year: 1444, month: 11, day: 11 });
        let start = |year: i32| (year + 5000) * 365 * 24;
        assert_eq!(Date::from_hours(start(1450)), Date { year: 1450, month: 1, day: 1 });
        assert_eq!(Date::from_hours(start(1451) - 24), Date { year: 1450, month: 12, day: 31 });
        assert_eq!(Date::from_hours(start(-2560) + 24 * 59 + 5), Date { year: -2560, month: 3, day: 1 });
    }

    #[test]
    fn a_truce_ends_on_the_next_first_of_the_month() {
        let date = |year, month, day| Date { year, month, day };
        assert_eq!(date(1446, 11, 15).next_month_start(), date(1446, 12, 1));
        assert_eq!(date(1446, 12, 15).next_month_start(), date(1447, 1, 1));
        assert_eq!(date(1446, 12, 1).next_month_start(), date(1446, 12, 1));
    }

    #[test]
    fn chinese_writes_dates_year_first_and_others_keep_the_game() {
        let date = Date { year: 1444, month: 11, day: 11 };
        assert_eq!(text(Language::SimpChinese, Shape::Day, date).as_deref(), Some("1444年11月11日"));
        assert_eq!(text(Language::SimpChinese, Shape::Month, date).as_deref(), Some("1444年11月"));
        assert_eq!(text(Language::SimpChinese, Shape::Year, Date { year: -2560, ..date }).as_deref(), Some("公元前2560年"));
        for language in [Language::English, Language::French, Language::German, Language::Spanish] {
            assert_eq!(text(language, Shape::Day, date), None);
        }
    }
}
