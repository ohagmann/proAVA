use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub};

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

/// Money in whole cents. No floating point where it becomes an amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money {
    cents: i64,
}

impl Money {
    pub const ZERO: Money = Money { cents: 0 };

    pub fn from_cents(cents: i64) -> Self {
        Self { cents }
    }

    /// Rounds to whole cents, half away from zero. Beyond the range of the
    /// type (about ±92 billiarden Euro – only from damaged files or absurd
    /// input) the amount stays at the largest value of its sign instead of
    /// panicking or turning into 0.
    pub fn from_decimal(value: Decimal) -> Self {
        let cents = value
            .checked_mul(Decimal::ONE_HUNDRED)
            .map(|cents| cents.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero))
            .and_then(|cents| cents.to_i64());
        match cents {
            Some(cents) => Self { cents },
            None => Self::saturated(value.is_sign_negative()),
        }
    }

    /// The largest amount of a sign (overflow).
    fn saturated(negative: bool) -> Self {
        Self {
            cents: if negative { i64::MIN } else { i64::MAX },
        }
    }

    pub fn cents(self) -> i64 {
        self.cents
    }

    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.cents, 2)
    }

    pub fn euros(self) -> f64 {
        self.cents as f64 / 100.0
    }

    pub fn is_zero(self) -> bool {
        self.cents == 0
    }

    /// Total price: unit price × quantity, rounded to cents half away from zero.
    pub fn total(unit_price: Decimal, quantity: Decimal) -> Self {
        match unit_price.checked_mul(quantity) {
            Some(value) => Self::from_decimal(value),
            None => Self::saturated(unit_price.is_sign_negative() != quantity.is_sign_negative()),
        }
    }

    /// `percent` of this amount, rounded to cents.
    pub fn percent(self, percent: Decimal) -> Self {
        match self.to_decimal().checked_mul(percent) {
            Some(value) => Self::from_decimal(value / Decimal::ONE_HUNDRED),
            None => Self::saturated((self.cents < 0) != percent.is_sign_negative()),
        }
    }

    /// Amount without the euro sign, e.g. "1.234,56".
    pub fn plain(self) -> String {
        format_decimal_de(self.to_decimal(), 2)
    }
}

impl Add for Money {
    type Output = Money;

    /// Saturating like [`Money::from_decimal`]: never wraps round.
    fn add(self, rhs: Money) -> Money {
        Money {
            cents: self.cents.saturating_add(rhs.cents),
        }
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.cents = self.cents.saturating_add(rhs.cents);
    }
}

impl Sub for Money {
    type Output = Money;

    fn sub(self, rhs: Money) -> Money {
        Money {
            cents: self.cents.saturating_sub(rhs.cents),
        }
    }
}

impl Neg for Money {
    type Output = Money;

    fn neg(self) -> Money {
        Money {
            cents: self.cents.saturating_neg(),
        }
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, |sum, value| sum + value)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} €", self.plain())
    }
}

/// German number format with thousands dots and a fixed number of decimals:
/// `format_decimal_de(1234.5, 2)` → "1.234,50".
pub fn format_decimal_de(value: Decimal, places: u32) -> String {
    let rounded = value.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let text = format!("{:.*}", places as usize, rounded.abs());
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole.to_string(), Some(fraction.to_string())),
        None => (text, None),
    };

    let digits: Vec<char> = whole.chars().collect();
    let mut grouped = String::new();
    for (index, digit) in digits.iter().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(*digit);
    }

    let sign = if negative { "-" } else { "" };
    match fraction {
        Some(fraction) => format!("{sign}{grouped},{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Reads German or plain numbers: "1.234,5", "1234,5", "1234.5", "12".
pub fn parse_decimal_de(text: &str) -> Result<Decimal, String> {
    let cleaned: String = text
        .trim()
        .trim_end_matches('€')
        .trim()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return Ok(Decimal::ZERO);
    }

    let normalized = if cleaned.contains(',') {
        cleaned.replace('.', "").replace(',', ".")
    } else if cleaned.matches('.').count() > 1 {
        cleaned.replace('.', "")
    } else {
        cleaned
    };

    normalized
        .parse::<Decimal>()
        .map_err(|_| format!("„{}“ ist keine Zahl.", text.trim()))
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{format_decimal_de, parse_decimal_de, Money};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn money_is_grouped_and_comma_separated() {
        assert_eq!(Money::from_cents(123456).to_string(), "1.234,56 €");
        assert_eq!(Money::from_cents(5).to_string(), "0,05 €");
        assert_eq!(Money::from_cents(-100000).to_string(), "-1.000,00 €");
    }

    #[test]
    fn total_rounds_half_away_from_zero() {
        assert_eq!(
            Money::total(dec("12.50"), dec("2.5")),
            Money::from_cents(3125)
        );
        assert_eq!(Money::total(dec("0.125"), dec("1")), Money::from_cents(13));
        assert_eq!(
            Money::total(dec("613.555"), dec("1")),
            Money::from_cents(61356)
        );
    }

    /// Gefunden mit dem Eigenschaftstest der Kostenermittlung: ein Kennwert
    /// 79.228.162.514.264.337.593.543.950.335 (aus einer beschädigten
    /// Projektdatei) mal Datenpunkte ließ die Dezimal-Multiplikation
    /// überlaufen (Panic), Beträge über ±92 Billiarden Euro wurden still 0 €.
    #[test]
    fn huge_amounts_saturate_instead_of_panicking_or_vanishing() {
        let max = Money::from_cents(i64::MAX);
        let min = Money::from_cents(i64::MIN);
        assert_eq!(Money::total(Decimal::MAX, dec("4000000000")), max);
        assert_eq!(Money::total(Decimal::MAX, dec("-2")), min);
        assert_eq!(Money::from_decimal(Decimal::MAX), max);
        assert_eq!(Money::from_decimal(dec("100000000000000000000")), max);
        assert_eq!(Money::from_decimal(dec("-100000000000000000000")), min);
        assert_eq!(Money::from_cents(500).percent(Decimal::MAX), max);
        assert_eq!(Money::from_cents(-500).percent(Decimal::MAX), min);
        assert_eq!(max + Money::from_cents(1), max);
        assert_eq!(min - Money::from_cents(1), min);
        assert_eq!(-min, max);
        let mut sum = max;
        sum += max;
        assert_eq!(sum, max);
        assert_eq!(
            [max, max, Money::from_cents(-3)].into_iter().sum::<Money>(),
            max - Money::from_cents(3)
        );
        // Normal amounts as before.
        assert_eq!(
            Money::total(dec("12.50"), dec("2.5")),
            Money::from_cents(3125)
        );
        assert_eq!(
            Money::from_cents(100_001).percent(dec("40")),
            Money::from_cents(40_000)
        );
    }

    #[test]
    fn german_numbers() {
        assert_eq!(format_decimal_de(dec("1234.5"), 2), "1.234,50");
        assert_eq!(format_decimal_de(dec("1234567.125"), 3), "1.234.567,125");
        assert_eq!(format_decimal_de(dec("-0.004"), 2), "0,00");
        assert_eq!(parse_decimal_de("1.234,5").unwrap(), dec("1234.5"));
        assert_eq!(parse_decimal_de("1234.5").unwrap(), dec("1234.5"));
        assert_eq!(parse_decimal_de("1.234.567").unwrap(), dec("1234567"));
        assert_eq!(parse_decimal_de("12 €").unwrap(), dec("12"));
        assert!(parse_decimal_de("abc").is_err());
    }
}
