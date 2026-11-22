//! GAEB codecs for proAVA: GAEB DA XML 3.x and GAEB 90 (DATAN).
//!
//! Pure library without UI or project knowledge. Both formats read into and
//! write from the exchange model in [`model`].

pub mod model;
mod text_reflow;

pub use model::*;
pub mod datan;
pub mod rules;
pub mod schemas;
pub mod xml;

/// An Ausführungsbeschreibung (PerfNo) in messages as proAVA shows it: „1“ → „A0001“ (same rule as `proava_domain::performance_mark`); the
/// file keeps its value.
pub(crate) fn perf_mark(number: &str) -> String {
    let number = number.trim();
    match number.parse::<u32>() {
        Ok(value) if value <= 9999 && number.bytes().all(|byte| byte.is_ascii_digit()) => {
            format!("A{value:04}")
        }
        _ => format!("A{number}"),
    }
}
