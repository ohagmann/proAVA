//! GP-Prüfung beim Import: a file (CSV) gives a GP that does not match
//! Menge × EP. proAVA takes the computed GP and marks the position until the
//! planner confirms it or changes Menge or EP. Also the currencies an LV can
//! be priced in.

use crate::money::Money;

/// The GP of the file did not match Menge × EP when it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceCheck {
    /// GP as written in the file.
    pub stated: Money,
    /// Menge × EP when it was read (this one counts).
    pub computed: Money,
}

impl PriceCheck {
    /// Short text in the status column of the LV table.
    pub const LABEL: &'static str = "GP prüfen";

    /// GP laut Datei minus errechneter GP.
    pub fn difference(&self) -> Money {
        self.stated - self.computed
    }

    /// „GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €“
    pub fn text(&self, currency: &str) -> String {
        format!(
            "GP laut Datei {}, errechnet {} – Abweichung {}",
            amount(self.stated, currency),
            amount(self.computed, currency),
            amount(self.difference(), currency)
        )
    }
}

/// Currencies offered for an LV: code (GAEB `Cur`) and name (`CurLbl`).
pub const CURRENCIES: [(&str, &str); 3] = [
    ("EUR", "Euro"),
    ("USD", "US-Dollar"),
    ("CHF", "Schweizer Franken"),
];

/// Name of a currency code („Euro“); empty for unknown codes.
pub fn currency_name(code: &str) -> &'static str {
    let code = code.trim();
    CURRENCIES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(code))
        .map(|(_, name)| *name)
        .unwrap_or("")
}

/// Sign or code written behind amounts: „€“ for Euro, else the code („USD“,
/// „CHF“); an empty code counts as Euro.
pub fn currency_symbol(code: &str) -> String {
    let code = code.trim();
    if code.is_empty() || code.eq_ignore_ascii_case("EUR") {
        "€".to_string()
    } else {
        code.to_uppercase()
    }
}

/// „1.234,56 €“, „1.234,56 USD“.
pub fn amount(money: Money, currency: &str) -> String {
    format!("{} {}", money.plain(), currency_symbol(currency))
}

/// Positions of `lv` still marked by the GP-Prüfung and the sum of their
/// differences (GP laut Datei minus errechnet).
pub fn open_checks(lv: &crate::Lv) -> (usize, Money) {
    lv.positions()
        .filter_map(|position| position.price_check())
        .fold((0, Money::ZERO), |(count, sum), check| {
            (count + 1, sum + check.difference())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_names_both_totals_and_the_difference() {
        let check = PriceCheck {
            stated: Money::from_cents(1200),
            computed: Money::from_cents(1000),
        };
        assert_eq!(check.difference(), Money::from_cents(200));
        assert_eq!(
            check.text("EUR"),
            "GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €"
        );
        assert_eq!(
            PriceCheck {
                stated: Money::from_cents(90_000),
                computed: Money::from_cents(123_456)
            }
            .text("CHF"),
            "GP laut Datei 900,00 CHF, errechnet 1.234,56 CHF – Abweichung -334,56 CHF"
        );
        assert_eq!(currency_name("usd"), "US-Dollar");
        assert_eq!(currency_symbol(""), "€");
    }
}
