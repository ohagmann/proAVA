use rust_decimal::{Decimal, RoundingStrategy};

pub fn round_money(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// German number with fixed decimals and thousands separator: 1.234,56
pub fn number(value: Decimal, decimals: u32) -> String {
    let rounded = value.round_dp_with_strategy(decimals, RoundingStrategy::MidpointAwayFromZero);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let plain = format!("{:.*}", decimals as usize, rounded.abs());
    let (int_part, frac_part) = match plain.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (plain.as_str(), None),
    };
    let mut grouped = String::new();
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&grouped);
    if let Some(frac) = frac_part {
        out.push(',');
        out.push_str(frac);
    }
    out
}

pub fn money(value: Decimal) -> String {
    number(value, 2)
}

/// Amount with the sign of the LV's currency („1.234,56 CHF“).
pub fn money_in(value: Decimal, currency: &str) -> String {
    format!("{} {currency}", money(value))
}

/// Quantity with as many decimals as needed (at most three), like the LV
/// itself: „1“, „2.500“, „12,5“, „0,125“ – GAEB files often carry
/// „1.000“ (three decimals) for one piece.
pub fn quantity(value: Decimal) -> String {
    let rounded = value.round_dp_with_strategy(3, RoundingStrategy::MidpointAwayFromZero);
    number(rounded, rounded.normalize().scale())
}

pub fn percent(value: Decimal) -> String {
    let text = value.normalize().to_string().replace('.', ",");
    format!("{text} %")
}

const ZWSP: char = '\u{200B}';

/// Adds invisible break chances to long words (file names, URLs, OZ chains),
/// so that they wrap instead of running over neighbouring columns.
pub fn breakable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            push_word(&mut out, &word);
            word.clear();
            out.push(ch);
        } else {
            word.push(ch);
        }
    }
    push_word(&mut out, &word);
    out
}

fn push_word(out: &mut String, word: &str) {
    const LONG: usize = 20;
    const CHUNK: usize = 24;
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= LONG {
        out.push_str(word);
        return;
    }
    let mut run = 0;
    for (i, &ch) in chars.iter().enumerate() {
        out.push(ch);
        run += 1;
        let last = i + 1 == chars.len();
        if !last
            && (matches!(ch, '_' | '.' | '/' | '\\' | '-' | ',' | ';' | ':' | '+') || run >= CHUNK)
        {
            out.push(ZWSP);
            run = 0;
        }
    }
}

/// Longest text of a PDF-Lesezeichen; longer ones end with „…“.
const BOOKMARK_CHARS: usize = 100;

/// Text of a PDF-Lesezeichen: the parts („01.0010“, „Kurztext“) joined by a
/// space on one line, without break marks and control characters, at most
/// 100 characters. Empty parts are left out.
pub fn bookmark_text(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .map(|part| part.replace(ZWSP, ""))
        .collect::<Vec<_>>()
        .iter()
        .flat_map(|part| part.split(|c: char| c.is_whitespace() || c.is_control()))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.chars().count() <= BOOKMARK_CHARS {
        return joined;
    }
    // At the last word boundary, if there is one in the second half.
    let cut: String = joined.chars().take(BOOKMARK_CHARS - 1).collect();
    let cut = match cut.rfind(' ') {
        Some(space) if space > cut.len() / 2 => &cut[..space],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn formats_german_numbers() {
        assert_eq!(money(d("1234.5")), "1.234,50");
        assert_eq!(money(d("1234567.005")), "1.234.567,01");
        assert_eq!(money(d("0")), "0,00");
        assert_eq!(money(d("-12.344")), "-12,34");
        assert_eq!(money(d("-0.001")), "0,00");
        assert_eq!(quantity(d("12.5")), "12,5");
        assert_eq!(quantity(d("1000")), "1.000");
        assert_eq!(quantity(d("1.000")), "1");
        assert_eq!(quantity(d("2500.000")), "2.500");
        assert_eq!(quantity(d("0.1254")), "0,125");
        assert_eq!(quantity(d("0")), "0");
        assert_eq!(percent(d("19.00")), "19 %");
        assert_eq!(percent(d("2.5")), "2,5 %");
        assert_eq!(money_in(d("999.999"), "€"), "1.000,00 €");
        assert_eq!(money_in(d("-12.5"), "CHF"), "-12,50 CHF");
    }

    #[test]
    fn bookmark_texts_are_one_plain_line() {
        assert_eq!(
            bookmark_text(&["01.481", "Automationseinrichtungen"]),
            "01.481 Automationseinrichtungen"
        );
        assert_eq!(
            bookmark_text(&["", " Zusammen\nstellung\t"]),
            "Zusammen stellung"
        );
        assert_eq!(
            bookmark_text(&["01", &breakable("DDC-Automationsstation-Feldebene")]),
            "01 DDC-Automationsstation-Feldebene"
        );
        let long = bookmark_text(&["01.0010", &"Wort ".repeat(40)]);
        assert!(
            long.chars().count() <= 100 && long.chars().count() > 90,
            "{long}"
        );
        assert!(long.ends_with("Wort…"), "{long}");
        assert_eq!(bookmark_text(&["", "  "]), "");
    }

    #[test]
    fn long_words_get_break_chances() {
        assert_eq!(breakable("kurz_name.x84 bleibt"), "kurz_name.x84 bleibt");
        let long = breakable("Bieter aus 260101001_Angebot_GA_Markthalle_Musterstadt_V3.X84");
        assert!(long.starts_with("Bieter aus 260101001_\u{200B}Angebot_\u{200B}"));
        assert_eq!(
            long.replace(ZWSP, ""),
            "Bieter aus 260101001_Angebot_GA_Markthalle_Musterstadt_V3.X84"
        );
        let plain = breakable(&"x".repeat(60));
        assert_eq!(plain.matches(ZWSP).count(), 2);
    }
}
