//! German numbers for tables and input fields.

use proava_domain::{format_decimal_de, Decimal, NumberKind};

/// Quantity without trailing zeros: 12, 12,5, 1.234,125.
pub fn quantity_text(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    if text.contains(',') {
        text.trim_end_matches('0').trim_end_matches(',').to_string()
    } else {
        text
    }
}

/// Price for an input field: no thousands dots, two decimals, empty when unpriced.
pub fn price_input(price: Option<Decimal>) -> String {
    price
        .map(|value| format_decimal_de(value, 2).replace('.', ""))
        .unwrap_or_default()
}

/// Quantity for an input field (no thousands dots).
pub fn quantity_input(value: Decimal) -> String {
    quantity_text(value).replace('.', "")
}

/// Quantity in the Menge field of the edit panel and the window
/// „Eigenschaften“ while it is not being edited: with thousands dots like
/// the GP (Anwender 18.11.); the input reads them back, typing stays free. A
/// Rechenansatz stays as typed.
pub fn quantity_entry_shown(position: &proava_domain::Position) -> String {
    position
        .quantity_formula()
        .map(str::to_string)
        .unwrap_or_else(|| quantity_text(position.quantity()))
}

pub fn price_text(price: Option<Decimal>) -> String {
    price
        .map(|value| format_decimal_de(value, 2))
        .unwrap_or_default()
}

/// Largest quantity and unit price the input accepts (shared with the MCP
/// server: `proava_domain::input_limits`).
pub use proava_domain::input_limits::MAX_INPUT;

/// A German number („1.234,5“, „1.150“ = 1150, „2.5“ = 2,5) with at most
/// `decimals` decimals and an absolute value below [`MAX_INPUT`].
pub fn parse_number(text: &str, decimals: u32) -> Result<Decimal, String> {
    proava_domain::input_limits::parse_number(text, decimals)
}

/// Empty means zero. Quantities carry up to three decimals (GAEB).
pub fn parse_quantity(text: &str) -> Result<Decimal, String> {
    NumberKind::Quantity.parse(text)
}

/// A quantity or a Rechenansatz („3*12,5+4“, „=2*(1,5+0,75)“): the value and,
/// for a calculation, the text to keep at the position.
pub fn parse_quantity_entry(text: &str) -> Result<(Decimal, Option<String>), String> {
    if proava_domain::quantity_formula::is_formula(text) {
        let value = proava_domain::quantity_formula::evaluate(text)?;
        if value.abs() >= Decimal::from(MAX_INPUT) {
            return Err("das Ergebnis ist zu groß (höchstens 99.999.999,999).".to_string());
        }
        Ok((value, Some(text.trim().to_string())))
    } else {
        parse_quantity(text).map(|value| (value, None))
    }
}

/// What a quantity field shows: the Rechenansatz, else the number.
pub fn quantity_entry_text(position: &proava_domain::Position) -> String {
    position
        .quantity_formula()
        .map(str::to_string)
        .unwrap_or_else(|| quantity_input(position.quantity()))
}

/// Empty means "not priced yet".
pub fn parse_price(text: &str) -> Result<Option<Decimal>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    NumberKind::UnitPrice.parse(text).map(Some)
}

/// Shortens a text to roughly `width` pixels at font `size` (proportional font).
/// Line breaks become spaces (one line, like a table cell).
pub fn fit_text(text: &str, width: f32, size: f32) -> (String, bool) {
    let text = &*crate::format::one_line(text);
    let approximate_char = size * 0.55;
    let max_chars = (width / approximate_char).max(3.0) as usize;
    let characters: Vec<char> = text.chars().collect();
    if characters.len() <= max_chars {
        return (text.to_string(), false);
    }
    let keep = max_chars.saturating_sub(1);
    let cut: String = characters[..keep].iter().collect();
    (format!("{}…", cut.trim_end()), true)
}

#[cfg(test)]
mod tests {
    use proava_domain::Decimal;

    use super::*;

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn numbers_read_and_write_german() {
        assert_eq!(quantity_text(dec("1234.5")), "1.234,5");
        assert_eq!(quantity_text(dec("12")), "12");
        assert_eq!(quantity_text(dec("1000")), "1.000");
        assert_eq!(quantity_input(dec("1234.125")), "1234,125");
        assert_eq!(price_input(Some(dec("1234.5"))), "1234,50");
        assert_eq!(price_text(Some(dec("1234.5"))), "1.234,50");
        assert_eq!(parse_price("").unwrap(), None);
        assert_eq!(parse_price("1.234,50").unwrap(), Some(dec("1234.5")));
        assert!(parse_quantity("zwölf").is_err());
    }

    #[test]
    fn decimals_and_size_are_limited() {
        assert_eq!(parse_price("12,125").unwrap(), Some(dec("12.125")));
        assert_eq!(parse_price("12,5000").unwrap(), Some(dec("12.5")));
        assert!(parse_price("1,23456").is_err());
        assert!(parse_quantity("0,0001").is_err());
        assert!(parse_quantity("99999999,999").is_ok());
        assert!(parse_quantity("100000000").is_err());
        assert!(parse_price("999999999999999999999").is_err());
        assert!(parse_price("-5").is_ok());
    }

    #[test]
    fn a_single_dot_with_three_digits_is_a_thousands_dot() {
        assert_eq!(parse_price("1.150").unwrap(), Some(dec("1150")));
        assert_eq!(parse_quantity("2.500").unwrap(), dec("2500"));
        assert_eq!(parse_price("-1.150").unwrap(), Some(dec("-1150")));
        assert_eq!(parse_price("2.5").unwrap(), Some(dec("2.5")));
        assert_eq!(parse_price("2.50").unwrap(), Some(dec("2.5")));
        assert_eq!(parse_quantity("0.125").unwrap(), dec("0.125"));
        assert_eq!(parse_price("1.150,5").unwrap(), Some(dec("1150.5")));
        assert_eq!(parse_price("1150.5").unwrap(), Some(dec("1150.5")));
    }

    #[test]
    fn long_texts_are_cut() {
        let (text, cut) = fit_text("Raumtemperaturfühler mit Sollwertsteller", 80.0, 13.0);
        assert!(cut);
        assert!(text.ends_with('…'));
        assert_eq!(fit_text("kurz", 200.0, 13.0), ("kurz".to_string(), false));
    }

    #[test]
    fn quantity_entries_take_a_calculation() {
        assert_eq!(parse_quantity_entry("12,5").unwrap(), (dec("12.5"), None));
        let (value, formula) = parse_quantity_entry(" 3*12,5+4 ").unwrap();
        assert_eq!(value, dec("41.5"));
        assert_eq!(formula.as_deref(), Some("3*12,5+4"));
        assert!(parse_quantity_entry("=1/0")
            .unwrap_err()
            .contains("Division durch 0"));
        assert!(parse_quantity_entry("=99999999*10")
            .unwrap_err()
            .contains("zu groß"));
        let mut position = proava_domain::Position::new("", "", dec("2"), "m", None);
        assert_eq!(quantity_entry_text(&position), "2");
        position.set_quantity(dec("4.5"));
        position.set_quantity_formula(Some("=2*(1,5+0,75)".into()));
        assert_eq!(quantity_entry_text(&position), "=2*(1,5+0,75)");
        position.set_quantity(dec("3"));
        assert_eq!(
            position.quantity_formula(),
            None,
            "a plain number drops the formula"
        );
    }
}
