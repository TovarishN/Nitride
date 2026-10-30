//! The Gregorian calendar for Nitride's Calendar language, written against
//! its WIT alone. Dates are day numbers since 0001-01-01.
wit_bindgen::generate!({ path: "wit", world: "implementation" });

use exports::nitride::calendar::gregorian::{Date, Dates, Duration, GregorianFailure, Guest, Span};

/// 9999-12-31.
const LAST: i64 = 3_652_058;

/// Days since 0001-01-01 of a proleptic Gregorian date.
fn day_number(year: i64, month: i64, day: i64) -> i64 {
    // Howard Hinnant's days_from_civil, shifted from 1970-01-01 to 0001-01-01.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + 719_162
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn year_of(date: i64) -> i64 {
    let mut year = (date / 366 + 1).max(1);
    while day_number(year + 1, 1, 1) <= date {
        year += 1;
    }
    year
}

fn shift(date: Date, days: Duration) -> Result<Date, GregorianFailure> {
    if days.fract() != 0.0 {
        return Err(GregorianFailure::PartialDay);
    }
    let day = date + days as i64;
    if (0..=LAST).contains(&day) {
        Ok(day)
    } else {
        Err(GregorianFailure::OutOfRange)
    }
}

struct Calendar;

impl Guest for Calendar {
    fn parse_iso(text: String) -> Result<Date, GregorianFailure> {
        let bytes = text.as_bytes();
        let shaped = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
        if !shaped {
            return Err(GregorianFailure::InvalidDate);
        }
        let number = |r: std::ops::Range<usize>| text[r].parse::<i64>().unwrap();
        let (year, month, day) = (number(0..4), number(5..7), number(8..10));
        let lengths = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        if year < 1 || !(1..=12).contains(&month) || day < 1 || day > lengths[month as usize - 1] {
            return Err(GregorianFailure::InvalidDate);
        }
        Ok(day_number(year, month, day))
    }

    fn later(date: Date, duration: Duration) -> Result<Date, GregorianFailure> {
        shift(date, duration)
    }

    fn earlier(date: Date, duration: Duration) -> Result<Date, GregorianFailure> {
        shift(date, -duration)
    }

    fn between(date1: Date, date2: Date) -> Result<Duration, GregorianFailure> {
        Ok((date1 - date2) as f64)
    }

    fn day_of_week(date: Date) -> Result<String, GregorianFailure> {
        // 0001-01-01 was a Monday.
        let names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
        Ok(names[date.rem_euclid(7) as usize].into())
    }

    fn is_leap_year(date: Date) -> Result<bool, GregorianFailure> {
        Ok(leap(year_of(date)))
    }

    fn mondays(span: Span) -> Result<Dates, GregorianFailure> {
        if span.start < 0 || span.end > LAST {
            return Err(GregorianFailure::OutOfRange);
        }
        let first = span.start + (7 - span.start.rem_euclid(7)) % 7;
        Ok((first..=span.end).step_by(7).collect())
    }
}

export!(Calendar);
