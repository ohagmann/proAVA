/// `2026-11-14T09:12:33` → `14.11.2026`
pub fn iso_to_german_date(iso: &str) -> String {
    let date = iso.get(0..10).unwrap_or(iso);
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3 {
        format!("{}.{}.{}", parts[2], parts[1], parts[0])
    } else {
        date.to_string()
    }
}

/// A date typed by hand → ISO `JJJJ-MM-TT`. Accepts `14.11.2026`, `4.1.2026`,
/// `14.11.26` (→ 2026) and `2026-11-14`; checks that the day exists.
pub fn parse_german_date(text: &str) -> Result<String, String> {
    const HINT: &str = "Datum bitte als TT.MM.JJJJ eingeben (z. B. 14.11.2026).";
    let text = text.trim();
    let numbers = |parts: Vec<&str>| -> Option<Vec<u32>> {
        parts
            .iter()
            .map(|part| {
                (!part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
                    .then(|| part.parse::<u32>().ok())
                    .flatten()
            })
            .collect()
    };
    let (year, month, day) = if text.contains('-') {
        let parts: Vec<&str> = text.split('-').collect();
        match (parts.len(), parts.first().map(|year| year.len())) {
            (3, Some(4)) => match numbers(parts).as_deref() {
                Some([year, month, day]) => (*year, *month, *day),
                _ => return Err(HINT.to_string()),
            },
            _ => return Err(HINT.to_string()),
        }
    } else {
        let parts: Vec<&str> = text.split('.').collect();
        if parts.len() != 3 || !matches!(parts[2].len(), 2 | 4) {
            return Err(HINT.to_string());
        }
        let short_year = parts[2].len() == 2;
        match numbers(parts).as_deref() {
            Some([day, month, year]) => {
                (if short_year { 2000 + year } else { *year }, *month, *day)
            }
            _ => return Err(HINT.to_string()),
        }
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(format!("Den Monat {month} gibt es nicht.")),
    };
    if day == 0 || day > days || !(1900..=2200).contains(&year) {
        return Err(format!("Den {day}.{month}.{year} gibt es nicht."));
    }
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

/// „1 Zeile“, „3 Zeilen“ – number with the right word.
pub fn count(number: usize, one: &str, many: &str) -> String {
    proava_domain::wording::count(number, one, many)
}

/// A text for one line of a table: line breaks (GAEB-Kurztexte come with
/// several lines) and tabs become one space each, the lines are trimmed.
/// The stored text keeps its lines; only the display is flat.
pub fn one_line(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains(['\n', '\r', '\t']) {
        return std::borrow::Cow::Borrowed(text);
    }
    let joined = text
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    std::borrow::Cow::Owned(joined.replace('\t', " "))
}

/// Keeps the start and the end of a long path readable.
pub fn truncate_middle(text: &str, max_chars: usize) -> String {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() <= max_chars {
        return text.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let head = keep * 2 / 3;
    let tail = keep - head;
    let start: String = characters[..head].iter().collect();
    let end: String = characters[characters.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

#[cfg(test)]
mod tests {
    use super::{iso_to_german_date, one_line, parse_german_date, truncate_middle};

    #[test]
    fn table_text_stays_on_one_line() {
        assert_eq!(one_line("Ventil DN 20"), "Ventil DN 20");
        assert!(matches!(one_line("Ventil"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(
            one_line(
                "2-Wege Umschaltventil (Auf / Zu)\r\ninkl. Stellantrieb - V17, V18 \n- Motorklappe"
            ),
            "2-Wege Umschaltventil (Auf / Zu) inkl. Stellantrieb - V17, V18 - Motorklappe"
        );
        assert_eq!(one_line("a\n\n\tb"), "a b");
    }

    #[test]
    fn typed_dates_become_iso() {
        assert_eq!(parse_german_date("14.11.2026").unwrap(), "2026-11-14");
        assert_eq!(parse_german_date(" 4.1.2026 ").unwrap(), "2026-01-04");
        assert_eq!(parse_german_date("14.11.26").unwrap(), "2026-11-14");
        assert_eq!(parse_german_date("2026-02-04").unwrap(), "2026-02-04");
        assert_eq!(parse_german_date("29.02.2028").unwrap(), "2028-02-29");
        assert!(parse_german_date("29.02.2026").is_err());
        assert!(parse_german_date("31.04.2026").is_err());
        assert!(parse_german_date("14.13.2026").is_err());
        assert!(parse_german_date("gestern").is_err());
        assert!(parse_german_date("14.11.").is_err());
        assert!(parse_german_date("26-11-14").is_err());
    }

    #[test]
    fn date_keeps_day_month_year() {
        assert_eq!(iso_to_german_date("2026-11-14T09:12:33"), "14.11.2026");
    }

    #[test]
    fn long_paths_are_cut_in_the_middle() {
        let result = truncate_middle("/home/nutzer/projekte/rathaus-nord/daten.proava", 24);
        assert!(result.contains('…'));
        assert!(result.len() < 30);
    }
}
