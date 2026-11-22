//! Eingabegrenzen – eine Stelle für die Oberfläche und den MCP-Server.
//!
//! Zahlen: Die Oberfläche nimmt nur an, was hier passt, der MCP-Server lehnt
//! alles andere mit derselben Meldung ab. Beträge bleiben so weit unter dem
//! Bereich von [`Money`](crate::Money) und der Dezimal-Multiplikation (Menge ×
//! EP, Datenpunkte × Kennwert), dass keine Summe überläuft.
//!
//! Texte haben keine harte Grenze (die Oberfläche schneidet nichts ab);
//! Kurztext, Einheit und Leistungsbereich haben eine Grenze von GAEB, die
//! „LV prüfen“, die Exportprüfung und der MCP-Server als Hinweis nennen.

use rust_decimal::Decimal;

use crate::money::parse_decimal_de;

/// Largest absolute value (exclusive) of a quantity, price, amount,
/// percentage or count typed in.
pub const MAX_INPUT: i64 = 100_000_000;

/// GAEB: Kurztext (Outline-Text) in GAEB 90 at most 70 characters.
pub const SHORT_TEXT_LIMIT: usize = 70;
/// GAEB carries at most four characters for the unit (QU); longer ones are cut.
pub const UNIT_LIMIT: usize = 4;
pub use crate::work_category::WORK_CATEGORY_LIMIT;

/// What a number is; each kind has its decimals and sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    /// Menge (LV, Nachtrag, Angebotsmenge): three decimals, also negative.
    Quantity,
    /// Einheitspreis (Planer, Bieter, Nachtrag): three decimals, also negative.
    UnitPrice,
    /// Betrag in Euro (Nachlass): two decimals, not negative.
    Amount,
    /// Kostenermittlung: Kennwert (DP-Preis), Spanne, Preis je DP-Art –
    /// two decimals, not negative.
    CostPrice,
    /// Kostenermittlung: Anteil einer Kostengruppe in % – two decimals, not negative.
    CostShare,
    /// Anzahl Datenpunkte: a whole number ≥ 0.
    PointCount,
    /// Nachlass in %: above 0 and below 100.
    DiscountPercent,
}

impl NumberKind {
    pub fn decimals(self) -> u32 {
        match self {
            NumberKind::Quantity | NumberKind::UnitPrice => 3,
            NumberKind::Amount | NumberKind::CostPrice | NumberKind::CostShare => 2,
            NumberKind::PointCount => 0,
            NumberKind::DiscountPercent => 4,
        }
    }

    pub fn allows_negative(self) -> bool {
        matches!(self, NumberKind::Quantity | NumberKind::UnitPrice)
    }

    /// The value as the program accepts it (normalized), else the reason.
    pub fn check(self, value: Decimal) -> Result<Decimal, String> {
        if self == NumberKind::DiscountPercent
            && (value <= Decimal::ZERO || value >= Decimal::ONE_HUNDRED)
        {
            return Err("Der Nachlass muss zwischen 0 und 100 % liegen.".to_string());
        }
        let value = check_number(value, self.decimals())?;
        if value.is_sign_negative() && !value.is_zero() && !self.allows_negative() {
            return Err("keine negativen Werte.".to_string());
        }
        Ok(value)
    }

    /// German text („1.234,5“) read and checked.
    pub fn parse(self, text: &str) -> Result<Decimal, String> {
        self.check(parse_input(text)?)
    }
}

/// At most `decimals` decimals and an absolute value below [`MAX_INPUT`].
pub fn check_number(value: Decimal, decimals: u32) -> Result<Decimal, String> {
    let value = value.normalize();
    if value.scale() > decimals {
        return Err(match decimals {
            0 => "bitte eine ganze Zahl.".to_string(),
            _ => format!("höchstens {decimals} Nachkommastellen."),
        });
    }
    if value.abs() >= Decimal::from(MAX_INPUT) {
        return Err(format!(
            "die Zahl ist zu groß (höchstens {}).",
            largest(decimals)
        ));
    }
    Ok(value)
}

/// A German number („1.234,5“, „1.150“ = 1150, „2.5“ = 2,5) with at most
/// `decimals` decimals and an absolute value below [`MAX_INPUT`].
pub fn parse_number(text: &str, decimals: u32) -> Result<Decimal, String> {
    check_number(parse_input(text)?, decimals)
}

/// Any number of decimals, an absolute value below [`MAX_INPUT`] (sums such as
/// a Mengenermittlung).
pub fn check_size(value: Decimal) -> Result<Decimal, String> {
    check_number(value, 28)
}

/// A German number as the input fields read it, without limits.
pub fn parse_text(text: &str) -> Result<Decimal, String> {
    parse_input(text)
}

/// Number of data points from a whole number ≥ 0 (MCP, CSV).
pub fn point_count(value: u64) -> Result<u32, String> {
    NumberKind::PointCount
        .check(Decimal::from(value))
        .map(|_| value as u32)
}

/// „99.999.999,999“ for three decimals, „99.999.999“ for none.
fn largest(decimals: u32) -> String {
    match decimals {
        0 => "99.999.999".to_string(),
        1..=4 => format!("99.999.999,{}", "9".repeat(decimals as usize)),
        _ => "99.999.999,999…".to_string(),
    }
}

fn parse_input(text: &str) -> Result<Decimal, String> {
    parse_decimal_de(&german_thousands(text))
}

/// „1.150“ ohne Komma ist im Deutschen eintausendeinhundertfünfzig, nicht 1,15:
/// genau ein Punkt mit genau drei Ziffern dahinter gilt als Tausenderpunkt.
/// „2.5“ und „0.125“ (englische Schreibweise) bleiben Dezimalzahlen.
fn german_thousands(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.contains(',') || trimmed.matches('.').count() != 1 {
        return text.to_string();
    }
    let (before, after) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let digits = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let before_digits = before.trim_start_matches('-');
    if after.len() == 3
        && digits(after)
        && digits(before_digits)
        && before_digits.len() <= 3
        && !before_digits.starts_with('0')
    {
        format!("{before}{after}")
    } else {
        text.to_string()
    }
}

/// Hints about texts longer than GAEB carries (Kurztext, Einheit), empty when all fit.
pub fn text_hints(short_text: &str, unit: &str) -> Vec<String> {
    let mut hints = Vec::new();
    let length = short_text.trim().chars().count();
    if length > SHORT_TEXT_LIMIT {
        hints.push(format!(
            "Kurztext hat {length} Zeichen – GAEB überträgt höchstens {SHORT_TEXT_LIMIT} (GAEB 90 kürzt)."
        ));
    }
    let length = unit.trim().chars().count();
    if length > UNIT_LIMIT {
        hints.push(format!(
            "Einheit „{}“ hat {length} Zeichen – GAEB überträgt höchstens {UNIT_LIMIT}.",
            unit.trim()
        ));
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn every_kind_has_its_decimals_size_and_sign() {
        assert_eq!(
            NumberKind::Quantity.check(dec("99999999.999")).unwrap(),
            dec("99999999.999")
        );
        assert!(NumberKind::Quantity
            .check(dec("100000000"))
            .unwrap_err()
            .contains("zu groß (höchstens 99.999.999,999)"));
        assert!(NumberKind::Quantity
            .check(dec("0.0001"))
            .unwrap_err()
            .contains("3 Nachkommastellen"));
        assert_eq!(NumberKind::UnitPrice.check(dec("-5")).unwrap(), dec("-5"));
        assert!(NumberKind::Amount
            .check(dec("-5"))
            .unwrap_err()
            .contains("negativ"));
        assert!(NumberKind::CostPrice
            .check(dec("1.005"))
            .unwrap_err()
            .contains("2 Nachkommastellen"));
        assert!(NumberKind::CostShare
            .check(dec("100000000"))
            .unwrap_err()
            .contains("99.999.999,99"));
        assert!(NumberKind::PointCount
            .check(dec("1.5"))
            .unwrap_err()
            .contains("ganze Zahl"));
        assert!(NumberKind::PointCount
            .check(dec("100000000"))
            .unwrap_err()
            .contains("(höchstens 99.999.999)"));
        assert_eq!(point_count(99_999_999).unwrap(), 99_999_999);
        assert!(point_count(u64::from(u32::MAX)).is_err());
        assert!(NumberKind::DiscountPercent
            .check(dec("100"))
            .unwrap_err()
            .contains("zwischen 0 und 100 %"));
        assert!(NumberKind::DiscountPercent.check(Decimal::ZERO).is_err());
        assert_eq!(
            NumberKind::DiscountPercent.check(dec("3.5")).unwrap(),
            dec("3.5")
        );
        // Trailing zeros do not count as decimals.
        assert_eq!(
            NumberKind::CostPrice.check(dec("12.5000")).unwrap(),
            dec("12.5")
        );
    }

    #[test]
    fn german_text_with_thousands_dots() {
        assert_eq!(NumberKind::UnitPrice.parse("1.150").unwrap(), dec("1150"));
        assert_eq!(
            NumberKind::UnitPrice.parse("1.234,50 €").unwrap(),
            dec("1234.5")
        );
        assert_eq!(parse_number("2.5", 3).unwrap(), dec("2.5"));
        assert!(parse_number("zwölf", 3).is_err());
        assert!(NumberKind::Quantity.parse("999999999999999999999").is_err());
    }

    #[test]
    fn long_texts_give_hints() {
        assert!(text_hints("Kurz", "St").is_empty());
        let hints = text_hints(&"x".repeat(71), "Stück");
        assert_eq!(hints.len(), 2);
        assert!(hints[0].contains("71 Zeichen"));
        assert!(hints[1].contains("„Stück“"));
    }
}
