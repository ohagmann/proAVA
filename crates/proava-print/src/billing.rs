//! Abrechnung: Aufmaßprüfung (the planner's check of one invoice per
//! position) and Rechnungsfreigabe (release calculation with signatures).
//! Model, JSON for the templates and rendering; kept apart from the LV
//! printouts like the Niederschrift.

use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::format::{breakable, money, money_in, quantity};
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const BILLING_CHECK_TEMPLATE: &str = include_str!("../templates/billing_check.typ");
const RELEASE_TEMPLATE: &str = include_str!("../templates/release.typ");

/// One line of a calculation or sums block.
#[derive(Debug, Clone, Default)]
pub struct PrintAmountLine {
    pub label: String,
    /// Percent or other short value between label and amount („10,00 %“).
    pub extra: String,
    /// Amount in euro; None = line of text only.
    pub amount: Option<Decimal>,
    /// Result line (bold, with a rule above).
    pub strong: bool,
}

impl PrintAmountLine {
    pub fn new(label: impl Into<String>, amount: Decimal) -> Self {
        Self {
            label: label.into(),
            amount: Some(amount),
            ..Self::default()
        }
    }

    pub fn strong(label: impl Into<String>, amount: Decimal) -> Self {
        Self {
            strong: true,
            ..Self::new(label, amount)
        }
    }

    pub fn with_extra(mut self, extra: impl Into<String>) -> Self {
        self.extra = extra.into();
        self
    }
}

/// How a position row is marked (findings of the check).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintMark {
    #[default]
    None,
    Warning,
    Error,
}

/// One position of the Aufmaßprüfung.
#[derive(Debug, Clone, Default)]
pub struct PrintBillingPosition {
    pub ordinal: String,
    pub text: String,
    pub unit: String,
    /// Ordered quantity (contract and approved change orders); None = not
    /// in the contract.
    pub ordered: Option<Decimal>,
    /// Checked quantity of the earlier invoices (cumulative).
    pub previous: Decimal,
    /// Quantity on the invoice, in the invoice's sense.
    pub claimed: Option<Decimal>,
    /// Checked quantity, in the invoice's sense.
    pub checked: Decimal,
    pub unit_price: Option<Decimal>,
    /// Amounts net before discount, in the invoice's sense.
    pub claimed_total: Option<Decimal>,
    pub checked_total: Decimal,
    /// Share of the ordered quantity reached after this invoice.
    pub reached_percent: Option<Decimal>,
    /// Where the checked quantity comes from („Aufmaß“, „Fortschritt 25 %“ …).
    pub basis: String,
    /// Vermerk of the planner and the findings, one per line.
    pub notes: Vec<String>,
    pub mark: PrintMark,
    /// Position of a change order (Nachtrag).
    pub change_order: bool,
}

/// One row of the Aufmaßprüfung: a title with its sums or a position.
#[derive(Debug, Clone)]
pub enum PrintBillingRow {
    Title {
        level: usize,
        ordinal: String,
        text: String,
        claimed: Decimal,
        checked: Decimal,
    },
    Position(Box<PrintBillingPosition>),
}

/// Aufmaßprüfung of one invoice.
#[derive(Debug, Clone, Default)]
pub struct PrintBillingCheck {
    pub header: PrintHeader,
    /// „3. Abschlagsrechnung AR-3 vom 30.06.2027“.
    pub invoice: String,
    /// Facts about the invoice: (label, text).
    pub facts: Vec<(String, String)>,
    /// „kumuliert“ or „Zuwachs“ – sense of the quantities and amounts.
    pub sense: String,
    pub rows: Vec<PrintBillingRow>,
    /// Sums under the table (claimed, checked, discount, VAT …).
    pub totals: Vec<PrintAmountLine>,
    /// Findings for the whole invoice.
    pub findings: Vec<String>,
    /// Prüfvermerk and „geprüft am … von …“.
    pub remarks: Vec<String>,
    pub header_text: String,
    pub footer_text: String,
}

/// An earlier release on the Rechnungsfreigabe.
#[derive(Debug, Clone, Default)]
pub struct PrintEarlierRelease {
    /// „1. Abschlagsrechnung AR-1“.
    pub label: String,
    pub date: String,
    pub performance: Decimal,
    pub release_net: Decimal,
    pub release_gross: Decimal,
}

/// Rechnungsfreigabe (Zahlungsfreigabe) of one invoice.
#[derive(Debug, Clone, Default)]
pub struct PrintRelease {
    pub header: PrintHeader,
    /// „3. Abschlagsrechnung AR-3 vom 30.06.2027“.
    pub invoice: String,
    /// Facts about the invoice: (label, text).
    pub facts: Vec<(String, String)>,
    /// Address blocks: (caption, lines), e.g. („Auftragnehmer“, […]).
    pub addresses: Vec<(String, Vec<String>)>,
    /// Contract against performance: (label, ordered, performed).
    pub overview: Vec<(String, Option<Decimal>, Option<Decimal>)>,
    /// Stand of the performance in percent of the ordered sum.
    pub performed_percent: Decimal,
    /// The release calculation step by step.
    pub calculation: Vec<PrintAmountLine>,
    pub earlier: Vec<PrintEarlierRelease>,
    /// Lines after the calculation (Skonto, Bauabzugssteuer …).
    pub notes: Vec<String>,
    /// Prüfvermerk and „geprüft am … von …“.
    pub remarks: Vec<String>,
    /// Captions under the signature lines, e.g. „Planer“, „Bauherr“.
    pub signatures: Vec<String>,
    pub header_text: String,
    pub footer_text: String,
}

/// Aufmaßprüfung (A4 quer) with the number of pages.
pub fn render_billing_check_pdf_counted(
    report: &PrintBillingCheck,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = billing_check_world(report)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

/// Rechnungsfreigabe (A4 hoch) with the number of pages.
pub fn render_release_pdf_counted(report: &PrintRelease) -> Result<(Vec<u8>, usize), PrintError> {
    let world = release_world(report)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub(crate) fn billing_check_world(
    report: &PrintBillingCheck,
) -> Result<crate::world::PrintWorld, PrintError> {
    let mut files = Files::default();
    let data = billing_check_data(report, &mut files);
    crate::build_world(BILLING_CHECK_TEMPLATE, files, &data)
}

pub(crate) fn release_world(report: &PrintRelease) -> Result<crate::world::PrintWorld, PrintError> {
    let mut files = Files::default();
    let data = release_data(report, &mut files);
    crate::build_world(RELEASE_TEMPLATE, files, &data)
}

fn amount_lines(lines: &[PrintAmountLine], currency: &str) -> Vec<Value> {
    lines
        .iter()
        .map(|line| {
            json!({
                "label": breakable(&line.label),
                "extra": line.extra,
                "amount": line.amount.map(|amount| signed(amount, currency)),
                "strong": line.strong,
            })
        })
        .collect()
}

fn facts(facts: &[(String, String)]) -> Vec<Value> {
    facts
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(label, text)| json!({ "label": label, "text": breakable(text) }))
        .collect()
}

fn frame_header(
    header: &PrintHeader,
    kind: &str,
    files: &mut Files,
    header_text: &str,
    footer_text: &str,
) -> Value {
    view::header_with(
        header,
        kind,
        files,
        &[
            ("kopftext", header_text.trim().to_string()),
            ("fusstext", footer_text.trim().to_string()),
        ],
    )
}

fn billing_check_data(report: &PrintBillingCheck, files: &mut Files) -> Value {
    let optional_quantity = |value: Option<Decimal>| value.map(quantity);
    // PDF-Lesezeichen: titles by their level (0 = outermost), positions
    // one below the title above them.
    let mut below_title = 1;
    let rows: Vec<Value> = report
        .rows
        .iter()
        .map(|row| match row {
            PrintBillingRow::Title {
                level,
                ordinal,
                text,
                claimed,
                checked,
            } => {
                below_title = level + 2;
                json!({
                "t": "title",
                "bm": view::bookmark(level + 1, &[ordinal, text]),
                "level": level,
                "oz": breakable(ordinal),
                "text": breakable(text),
                "claimed": money(*claimed),
                "checked": money(*checked),
                "difference": difference(*checked - *claimed),
                })
            }
            PrintBillingRow::Position(position) => {
                let difference_value = position
                    .claimed_total
                    .map(|claimed| position.checked_total - claimed);
                json!({
                    "t": "position",
                    "bm": view::bookmark(below_title, &[&position.ordinal, &position.text]),
                    "oz": breakable(&position.ordinal),
                    "text": breakable(&position.text),
                    "unit": position.unit,
                    "ordered": optional_quantity(position.ordered),
                    "previous": if position.previous.is_zero() { None } else { Some(quantity(position.previous)) },
                    "claimed": optional_quantity(position.claimed),
                    "checked": quantity(position.checked),
                    "price": position.unit_price.map(money),
                    "claimed_total": position.claimed_total.map(money),
                    "checked_total": money(position.checked_total),
                    "difference": difference_value.map(difference),
                    "reached": position.reached_percent.map(|p| crate::format::number(p, 0) + " %"),
                    "basis": position.basis,
                    "notes": position.notes.iter().map(|note| breakable(note)).collect::<Vec<_>>(),
                    "mark": match position.mark {
                        PrintMark::None => "",
                        PrintMark::Warning => "warn",
                        PrintMark::Error => "error",
                    },
                    "change_order": position.change_order,
                })
            }
        })
        .collect();
    json!({
        "header": frame_header(&report.header, "Aufmaßprüfung", files, &report.header_text, &report.footer_text),
        "invoice": breakable(&report.invoice),
        "bm": view::bookmark(1, &[&report.invoice]),
        "facts": facts(&report.facts),
        "sense": report.sense,
        "rows": rows,
        "totals": amount_lines(&report.totals, report.header.currency_sign()),
        "findings": report.findings.iter().map(|f| breakable(f)).collect::<Vec<_>>(),
        "remarks": report.remarks.iter().map(|r| breakable(r)).collect::<Vec<_>>(),
    })
}

/// Amount in the Währung of the LV with a typographic minus („−6.000,00 €“).
fn signed(value: Decimal, currency: &str) -> String {
    if value.is_sign_negative() && !value.is_zero() {
        format!("−{}", money_in(-value, currency))
    } else {
        money_in(value, currency)
    }
}

/// Difference as „+12,50“ / „−3,00“, empty for zero.
fn difference(value: Decimal) -> String {
    if value.is_zero() {
        String::new()
    } else if value.is_sign_negative() {
        format!("−{}", money(-value))
    } else {
        format!("+{}", money(value))
    }
}

fn release_data(report: &PrintRelease, files: &mut Files) -> Value {
    json!({
        "header": frame_header(&report.header, "Rechnungsfreigabe", files, &report.header_text, &report.footer_text),
        "invoice": breakable(&report.invoice),
        "bm": view::bookmark(1, &[&report.invoice]),
        "facts": facts(&report.facts),
        "addresses": report.addresses.iter()
            .filter(|(_, lines)| lines.iter().any(|line| !line.trim().is_empty()))
            .map(|(caption, lines)| json!({
                "caption": caption,
                "lines": lines.iter().filter(|line| !line.trim().is_empty()).map(|line| breakable(line)).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "overview": report.overview.iter().map(|(label, ordered, performed)| json!({
            "label": label,
            "ordered": ordered.map(|value| money_in(value, report.header.currency_sign())),
            "performed": performed.map(|value| money_in(value, report.header.currency_sign())),
        })).collect::<Vec<_>>(),
        "performed_percent": format!("{} %", crate::format::number(report.performed_percent, 1)),
        "calculation": amount_lines(&report.calculation, report.header.currency_sign()),
        "earlier": report.earlier.iter().map(|earlier| json!({
            "label": breakable(&earlier.label),
            "date": earlier.date,
            "performance": money(earlier.performance),
            "release_net": money(earlier.release_net),
            "release_gross": money(earlier.release_gross),
        })).collect::<Vec<_>>(),
        "earlier_total": money(report.earlier.iter().map(|e| e.release_net).sum()),
        "earlier_total_gross": money(report.earlier.iter().map(|e| e.release_gross).sum()),
        "notes": report.notes.iter().map(|note| breakable(note)).collect::<Vec<_>>(),
        "remarks": report.remarks.iter().map(|r| breakable(r)).collect::<Vec<_>>(),
        "signatures": report.signatures,
    })
}
