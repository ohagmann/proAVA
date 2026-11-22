//! Kostenschätzung / Kostenberechnung nach DIN 276 (KG 480): model, JSON for
//! `templates/cost_estimate.typ` and rendering. A4 quer by default:
//! optional Deckblatt, Zusammenfassung with the Bauteile and the cost
//! groups (optional bar chart), one page per Bauteil with the data points,
//! the split onto KG 481–489 and the plant list, Kostenkontrolle and the
//! Erläuterungen (Kennwerte, Annahmen, Rundung).

use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::format::{breakable, money_in, number, percent};
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const TEMPLATE: &str = include_str!("../templates/cost_estimate.typ");

/// One DP-Art (column of the plant list).
#[derive(Debug, Clone, Default)]
pub struct PrintPointKind {
    /// Short name, e.g. „AI“.
    pub key: String,
    /// „Analoge Eingabe“.
    pub label: String,
    pub virtual_point: bool,
}

/// Count, price and amount of one DP-Art.
#[derive(Debug, Clone, Default)]
pub struct PrintKindSum {
    pub count: u64,
    pub unit_price: Decimal,
    pub amount: Decimal,
}

/// One cost group of the split.
#[derive(Debug, Clone, Default)]
pub struct PrintShare {
    pub code: String,
    pub label: String,
    pub percent: Decimal,
    pub amount: Decimal,
}

/// One line of the plant list.
#[derive(Debug, Clone, Default)]
pub struct PrintPointRow {
    pub location: String,
    pub trade: String,
    pub plant: String,
    pub remark: String,
    pub counts: Vec<u32>,
}

/// One Bauteil with its sums.
#[derive(Debug, Clone, Default)]
pub struct PrintCostSection {
    pub name: String,
    pub rows: Vec<PrintPointRow>,
    pub kinds: Vec<PrintKindSum>,
    pub points: u64,
    pub total: Decimal,
    pub low: Option<Decimal>,
    pub high: Option<Decimal>,
    pub shares: Vec<PrintShare>,
    pub percent_sum: Decimal,
    pub check: Decimal,
    pub own_shares: bool,
}

/// One line of the Kostenkontrolle.
#[derive(Debug, Clone, Default)]
pub struct PrintControlLine {
    pub code: String,
    pub label: String,
    pub estimate: Decimal,
    pub planner: Decimal,
    pub awarded: Decimal,
}

/// Kostenkontrolle: Kostenermittlung ↔ LV ↔ Auftrag je KG.
#[derive(Debug, Clone, Default)]
pub struct PrintCostControl {
    pub lines: Vec<PrintControlLine>,
    pub estimate: Decimal,
    pub planner: Decimal,
    pub awarded: Decimal,
    pub has_award: bool,
}

/// What the printout shows besides the sums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintCostOptions {
    pub cover: bool,
    pub landscape: bool,
    /// Bar chart of the cost groups (Berechnung) or Bauteile (Schätzung).
    pub chart: bool,
    /// Plant list (Anlagenliste) under each Bauteil.
    pub plants: bool,
}

impl Default for PrintCostOptions {
    fn default() -> Self {
        Self {
            cover: false,
            landscape: true,
            chart: true,
            plants: true,
        }
    }
}

/// A Kostenschätzung or Kostenberechnung for print.
#[derive(Debug, Clone, Default)]
pub struct PrintCostEstimate {
    pub header: PrintHeader,
    /// Kostenberechnung (3. Ebene, split onto KG 481–489); false = Schätzung.
    pub calculation: bool,
    /// „Kostenberechnung“
    pub stage: String,
    /// „DIN 276, 3. Ebene“
    pub level: String,
    /// e.g. „Kostenberechnung LPH 3“.
    pub title: String,
    pub remark: String,
    pub kinds: Vec<PrintPointKind>,
    pub sections: Vec<PrintCostSection>,
    pub kind_sums: Vec<PrintKindSum>,
    pub points: u64,
    pub total: Decimal,
    pub low: Option<Decimal>,
    pub high: Option<Decimal>,
    /// Schätzung with one DP-Preis: that price; None = price per DP-Art.
    pub rate: Option<Decimal>,
    pub rate_low: Option<Decimal>,
    pub rate_high: Option<Decimal>,
    /// Per cost group over all Bauteile.
    pub groups: Vec<PrintShare>,
    pub check: Decimal,
    /// Split of the estimate (Annahmen); Bauteile with their own split are named.
    pub shares: Vec<PrintShare>,
    pub control: Option<PrintCostControl>,
    pub warnings: Vec<String>,
    pub options: PrintCostOptions,
}

/// The PDF with its number of pages.
pub fn render_cost_estimate_pdf_counted(
    estimate: &PrintCostEstimate,
) -> Result<(Vec<u8>, usize), PrintError> {
    let mut files = Files::default();
    let data = cost_estimate_json(estimate, &mut files);
    let world = crate::build_world(TEMPLATE, files, &data)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub fn render_cost_estimate_pdf(estimate: &PrintCostEstimate) -> Result<Vec<u8>, PrintError> {
    render_cost_estimate_pdf_counted(estimate).map(|(bytes, _)| bytes)
}

fn count(value: u64) -> String {
    number(Decimal::from(value), 0)
}

fn share_percent(value: Decimal) -> String {
    let value = value.round_dp(2).normalize();
    percent(value)
}

pub(crate) fn cost_estimate_json(estimate: &PrintCostEstimate, files: &mut Files) -> Value {
    let currency = estimate.header.currency_sign().to_string();
    let amount = |value: Decimal| money_in(value, &currency);
    let price = |value: Decimal| money_in(value, &currency);
    // Hardware 1.1.n, virtual 1.2.n (like the planner's sheets).
    let mut hardware = 0;
    let mut virtual_points = 0;
    let numbers: Vec<String> = estimate
        .kinds
        .iter()
        .map(|kind| {
            if kind.virtual_point {
                virtual_points += 1;
                format!("1.2.{virtual_points}")
            } else {
                hardware += 1;
                format!("1.1.{hardware}")
            }
        })
        .collect();
    let kinds: Vec<Value> = estimate
        .kinds
        .iter()
        .zip(&numbers)
        .map(|(kind, number)| {
            json!({
                "key": kind.key,
                "label": kind.label,
                "number": number,
                "virtual": kind.virtual_point,
                "caption": if kind.label.trim().is_empty() { kind.key.clone() } else { format!("{} ({})", kind.label.trim(), kind.key.trim()) },
            })
        })
        .collect();
    let kind_sums = |sums: &[PrintKindSum]| -> Vec<Value> {
        sums.iter()
            .map(|sum| {
                json!({
                    "count": count(sum.count),
                    "used": sum.count > 0,
                    "price": price(sum.unit_price),
                    "amount": amount(sum.amount),
                })
            })
            .collect()
    };
    let shares = |shares: &[PrintShare]| -> Vec<Value> {
        shares
            .iter()
            .map(|share| {
                json!({
                    "code": share.code,
                    "label": breakable(&share.label),
                    "percent": share_percent(share.percent),
                    // 0…1 for the bar chart.
                    "ratio": (share.percent / Decimal::ONE_HUNDRED).round_dp(4).to_string().parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0),
                    "amount": amount(share.amount),
                })
            })
            .collect()
    };
    let total = estimate.total;
    let sections: Vec<Value> = estimate
        .sections
        .iter()
        .map(|section| {
            let rows: Vec<Value> = section
                .rows
                .iter()
                .filter(|row| {
                    !(row.location.trim().is_empty()
                        && row.trade.trim().is_empty()
                        && row.plant.trim().is_empty()
                        && row.remark.trim().is_empty()
                        && row.counts.iter().all(|count| *count == 0))
                })
                .map(|row| {
                    json!({
                        "location": breakable(&row.location),
                        "trade": breakable(&row.trade),
                        "plant": breakable(&row.plant),
                        "remark": breakable(&row.remark),
                        "counts": (0..estimate.kinds.len()).map(|index| {
                            let value = row.counts.get(index).copied().unwrap_or(0);
                            if value == 0 { String::new() } else { count(u64::from(value)) }
                        }).collect::<Vec<_>>(),
                        "points": count(row.counts.iter().map(|count| u64::from(*count)).sum()),
                    })
                })
                .collect();
            let ratio = if total.is_zero() {
                0.0
            } else {
                (section.total / total).round_dp(4).to_string().parse::<f64>().unwrap_or(0.0).clamp(0.0, 1.0)
            };
            json!({
                "name": breakable(if section.name.trim().is_empty() { "Bauteil" } else { section.name.trim() }),
                "rows": rows,
                "kinds": kind_sums(&section.kinds),
                "points": count(section.points),
                "total": amount(section.total),
                "low": section.low.map(amount),
                "high": section.high.map(amount),
                "ratio": ratio,
                "share_of_total": if total.is_zero() { String::new() } else { share_percent(section.total * Decimal::ONE_HUNDRED / total) },
                "shares": shares(&section.shares),
                "percent_sum": share_percent(section.percent_sum),
                "split_ok": section.percent_sum == Decimal::ONE_HUNDRED,
                "check": amount(section.check),
                "own_shares": section.own_shares,
                // Amount per cost group of the summary (same order as `groups`).
                "by_group": estimate.groups.iter().map(|group| {
                    section.shares.iter().find(|share| share.code == group.code).map(|share| amount(share.amount)).unwrap_or_default()
                }).collect::<Vec<_>>(),
            })
        })
        .collect();
    let control = estimate
        .control
        .as_ref()
        .filter(|control| !control.lines.is_empty())
        .map(|control| {
            let difference = |value: Decimal, base: Decimal| -> Value {
                json!({
                    "amount": amount(value - base),
                    "percent": if base.is_zero() { String::new() } else {
                        let p = ((value - base) * Decimal::ONE_HUNDRED / base).round_dp(1);
                        format!("{}{} %", if p > Decimal::ZERO { "+" } else { "" }, number(p, 1))
                    },
                    "sign": if value > base { "up" } else if value < base { "down" } else { "" },
                })
            };
            json!({
                "has_award": control.has_award,
                "lines": control.lines.iter().map(|line| json!({
                    "code": line.code,
                    "label": breakable(&line.label),
                    "estimate": amount(line.estimate),
                    "planner": amount(line.planner),
                    "planner_diff": difference(line.planner, line.estimate),
                    "awarded": amount(line.awarded),
                    "awarded_diff": difference(line.awarded, line.estimate),
                })).collect::<Vec<_>>(),
                "estimate": amount(control.estimate),
                "planner": amount(control.planner),
                "planner_diff": difference(control.planner, control.estimate),
                "awarded": amount(control.awarded),
                "awarded_diff": difference(control.awarded, control.estimate),
            })
        });
    let own_split: Vec<String> = estimate
        .sections
        .iter()
        .filter(|section| section.own_shares)
        .map(|section| section.name.trim().to_string())
        .collect();
    json!({
        "header": view::header(&estimate.header, &estimate.stage, files),
        "calculation": estimate.calculation,
        "stage": estimate.stage,
        "level": estimate.level,
        "title": breakable(&estimate.title),
        "remark": estimate.remark,
        "kinds": kinds,
        "sections": sections,
        "kind_sums": kind_sums(&estimate.kind_sums),
        "points": count(estimate.points),
        "total": amount(estimate.total),
        "low": estimate.low.map(amount),
        "high": estimate.high.map(amount),
        "rate": estimate.rate.map(price),
        "rate_low": estimate.rate_low.map(price),
        "rate_high": estimate.rate_high.map(price),
        "groups": shares(&estimate.groups),
        "check": amount(estimate.check),
        "check_ok": estimate.check.is_zero(),
        "shares": shares(&estimate.shares),
        "own_split": own_split,
        "control": control,
        "warnings": estimate.warnings,
        "cover": estimate.options.cover,
        "landscape": estimate.options.landscape,
        "chart": estimate.options.chart,
        "plants": estimate.options.plants,
        "cur": currency,
    })
}
