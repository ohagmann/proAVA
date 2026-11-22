//! Dates as the user types them: TT.MM.JJJJ (Stammdaten, Fristen).

use std::fmt;

/// A calendar date written the German way, e.g. „15.12.2026“.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GermanDate {
    // Field order = comparison order.
    year: u16,
    month: u8,
    day: u8,
}

impl GermanDate {
    pub fn new(day: u8, month: u8, year: u16) -> Option<Self> {
        if !(1..=12).contains(&month) || !(1900..=2999).contains(&year) {
            return None;
        }
        if day == 0 || day > days_in_month(month, year) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Reads „TT.MM.JJJJ“; single digits („1.2.2027“) are fine, two-digit
    /// years are not (unclear century).
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let parts: Vec<&str> = text.split('.').collect();
        let wrong = || format!("„{text}“ ist kein Datum im Format TT.MM.JJJJ.");
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
            return Err(wrong());
        }
        if parts[2].len() != 4 {
            return Err(wrong());
        }
        let day: u8 = parts[0].parse().map_err(|_| wrong())?;
        let month: u8 = parts[1].parse().map_err(|_| wrong())?;
        let year: u16 = parts[2].parse().map_err(|_| wrong())?;
        Self::new(day, month, year).ok_or_else(|| format!("Den {text} gibt es nicht."))
    }

    /// Reads ISO „JJJJ-MM-TT“ (GAEB, MCP).
    pub fn parse_iso(text: &str) -> Option<Self> {
        let mut parts = text.trim().get(..10)?.split('-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        Self::new(day, month, year)
    }

    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn day(&self) -> u8 {
        self.day
    }

    pub fn month(&self) -> u8 {
        self.month
    }

    pub fn year(&self) -> u16 {
        self.year
    }

    /// Completes a short date as typed in a date field: „1.12.26“, „1.12.“,
    /// „1.12“ (year of `today`), „011226“, „01122026“, or a full
    /// „01.12.2026“. Two-digit years mean 20JJ. `None` when it is no date.
    pub fn complete(text: &str, today: GermanDate) -> Option<GermanDate> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let number = |part: &str| -> Option<u32> {
            (!part.is_empty() && part.len() <= 4 && part.chars().all(|c| c.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        };
        let year_of = |part: &str| -> Option<u16> {
            match part.len() {
                0 => Some(today.year),
                2 => number(part).map(|year| 2000 + year as u16),
                4 => number(part).map(|year| year as u16),
                _ => None,
            }
        };
        let (day, month, year) = if text.contains('.') {
            let parts: Vec<&str> = text.split('.').collect();
            match parts.as_slice() {
                [day, month] | [day, month, ""] => (number(day)?, number(month)?, today.year),
                [day, month, year] => (number(day)?, number(month)?, year_of(year)?),
                _ => return None,
            }
        } else if text.chars().all(|c| c.is_ascii_digit()) && matches!(text.len(), 6 | 8) {
            (
                number(&text[0..2])?,
                number(&text[2..4])?,
                year_of(&text[4..])?,
            )
        } else {
            return None;
        };
        GermanDate::new(u8::try_from(day).ok()?, u8::try_from(month).ok()?, year)
    }

    /// Days since 1.1.1970 (may be negative), for weekday and arithmetic.
    fn days_since_epoch(&self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month = i64::from(self.month);
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    fn from_days_since_epoch(days: i64) -> Option<GermanDate> {
        let days = days + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let mp = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        GermanDate::new(day as u8, month as u8, u16::try_from(year).ok()?)
    }

    /// The date `days` later (earlier when negative); `None` outside 1900–2999.
    pub fn add_days(&self, days: i64) -> Option<GermanDate> {
        Self::from_days_since_epoch(self.days_since_epoch() + days)
    }

    /// Weekday, 0 = Monday … 6 = Sunday.
    pub fn weekday(&self) -> u8 {
        // 1.1.1970 was a Thursday (3).
        ((self.days_since_epoch() + 3).rem_euclid(7)) as u8
    }

    /// ISO 8601 calendar week (KW), 1–53.
    pub fn iso_week(&self) -> u8 {
        // The week belongs to the year of its Thursday.
        let thursday = self.days_since_epoch() - i64::from(self.weekday()) + 3;
        let Some(thursday) = Self::from_days_since_epoch(thursday) else {
            return 1;
        };
        let first = GermanDate {
            year: thursday.year,
            month: 1,
            day: 1,
        };
        ((thursday.days_since_epoch() - first.days_since_epoch()) / 7 + 1) as u8
    }

    /// Number of days in the month of this date.
    pub fn days_in_month(&self) -> u8 {
        days_in_month(self.month, self.year)
    }
}

impl fmt::Display for GermanDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}.{:02}.{:04}", self.day, self.month, self.year)
    }
}

fn days_in_month(month: u8, year: u16) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// An optional date field: empty is fine, anything else must be a date.
pub fn optional_date(text: &str) -> Result<Option<GermanDate>, String> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        GermanDate::parse(text).map(Some)
    }
}

/// „HH:MM“ (Uhrzeit des Abgabetermins); empty is fine.
pub fn optional_time(text: &str) -> Result<Option<(u8, u8)>, String> {
    let text = text.trim().trim_end_matches("Uhr").trim();
    if text.is_empty() {
        return Ok(None);
    }
    let wrong = || format!("„{text}“ ist keine Uhrzeit im Format HH:MM.");
    let (hours, minutes) = text.split_once(':').ok_or_else(wrong)?;
    let hours: u8 = hours.parse().map_err(|_| wrong())?;
    let minutes: u8 = minutes.parse().map_err(|_| wrong())?;
    if hours > 23 || minutes > 59 || text.split_once(':').is_some_and(|(_, m)| m.len() != 2) {
        return Err(wrong());
    }
    Ok(Some((hours, minutes)))
}

#[cfg(test)]
mod tests {
    use super::{optional_date, optional_time, GermanDate};

    #[test]
    fn parses_and_checks_dates() {
        let date = GermanDate::parse("1.2.2027").unwrap();
        assert_eq!(date.to_string(), "01.02.2027");
        assert_eq!(date.iso(), "2027-02-01");
        assert_eq!(GermanDate::parse_iso("2027-02-01T10:00"), Some(date));
        assert!(GermanDate::parse("29.02.2028").is_ok());
        assert!(GermanDate::parse("29.02.2027").is_err());
        assert!(GermanDate::parse("31.04.2027").is_err());
        assert!(GermanDate::parse("15.12.26").is_err());
        assert!(GermanDate::parse("morgen").is_err());
        assert!(
            GermanDate::parse("01.12.2026").unwrap() < GermanDate::parse("15.12.2026").unwrap()
        );
        assert!(
            GermanDate::parse("31.12.2026").unwrap() < GermanDate::parse("01.01.2027").unwrap()
        );
        assert_eq!(optional_date(" ").unwrap(), None);
    }

    #[test]
    fn short_dates_are_completed() {
        let today = GermanDate::new(17, 11, 2026).unwrap();
        let complete = |text: &str| GermanDate::complete(text, today).map(|date| date.to_string());
        assert_eq!(complete("1.12.26").as_deref(), Some("01.12.2026"));
        assert_eq!(complete("1.12.").as_deref(), Some("01.12.2026"));
        assert_eq!(complete("1.12").as_deref(), Some("01.12.2026"));
        assert_eq!(complete("01.12.2027").as_deref(), Some("01.12.2027"));
        assert_eq!(complete(" 3.2.27 ").as_deref(), Some("03.02.2027"));
        assert_eq!(complete("011226").as_deref(), Some("01.12.2026"));
        assert_eq!(complete("01122026").as_deref(), Some("01.12.2026"));
        assert_eq!(complete("31.2."), None, "gibt es nicht");
        assert_eq!(complete("1.12.202"), None, "dreistelliges Jahr");
        assert_eq!(complete("morgen"), None);
        assert_eq!(complete(""), None);
        assert_eq!(complete("1.2.3.4"), None);
    }

    #[test]
    fn weekdays_weeks_and_days() {
        let date = |d, m, y| GermanDate::new(d, m, y).unwrap();
        assert_eq!(date(17, 11, 2026).weekday(), 1, "Dienstag");
        assert_eq!(date(1, 1, 1970).weekday(), 3, "Donnerstag");
        assert_eq!(date(29, 2, 2028).weekday(), 1);
        assert_eq!(date(17, 11, 2026).iso_week(), 47);
        assert_eq!(
            date(1, 1, 2027).iso_week(),
            53,
            "1.1.2027 gehört zur KW 53 von 2026"
        );
        assert_eq!(date(4, 1, 2027).iso_week(), 1);
        assert_eq!(date(29, 12, 2025).iso_week(), 1, "gehört zur KW 1 von 2026");
        assert_eq!(date(31, 12, 2026).add_days(1), Some(date(1, 1, 2027)));
        assert_eq!(date(1, 3, 2028).add_days(-1), Some(date(29, 2, 2028)));
        assert_eq!(date(17, 11, 2026).add_days(-7), Some(date(10, 11, 2026)));
        assert_eq!(date(15, 2, 2027).days_in_month(), 28);
    }

    #[test]
    fn parses_times() {
        assert_eq!(optional_time("10:00 Uhr").unwrap(), Some((10, 0)));
        assert_eq!(optional_time("").unwrap(), None);
        assert!(optional_time("25:00").is_err());
        assert!(optional_time("10:5").is_err());
        assert!(optional_time("zehn").is_err());
    }
}
