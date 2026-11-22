//! Kostenermittlung als Kurztext-LV (Kosten-LV) nach DIN 276: Angaben
//! (Stufe, Kostenstand, Umsatzsteuer), Zusammenstellung nach Kostengruppen
//! in den Ebenen der Stufe und die Kurztext-Liste (Menge, Einheit,
//! Kostenkennwert, Betrag, KG). Layout in `templates/cost_lv.typ`.

use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::format::{breakable, money_in, number, percent, quantity};
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const TEMPLATE: &str = include_str!("../templates/cost_lv.typ");

/// One line of the Zusammenstellung nach Kostengruppen.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintCostLvGroup {
    /// 1–3 = Ebene der Kostengliederung (indent, bold for 1); 0 = a line
    /// outside the hierarchy (ohne Kostengruppe, other codes).
    pub level: u8,
    /// „400“, „420“, „421“; empty = ohne Kostengruppe.
    pub code: String,
    pub label: String,
    pub amount: Decimal,
}

/// One line of the Kurztext-Liste.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintCostLvItem {
    Title {
        /// 0 = Titel, 1 = Untertitel …
        level: usize,
        ordinal: String,
        text: String,
        /// Sum of the positions below (counted ones).
        total: Decimal,
    },
    Position {
        ordinal: String,
        text: String,
        /// Kostengruppe, own or from the title.
        cost_group: String,
        /// None for a Zuschlag or an Aufteilungszeile (no quantity of its own).
        quantity: Option<Decimal>,
        unit: String,
        /// Kostenkennwert (EP); None = without price.
        unit_price: Option<Decimal>,
        total: Decimal,
        /// Bedarfs-/Wahlposition, Rechengrundlage: shown in brackets, not in
        /// the sums.
        counts: bool,
        /// Line under the short text: „40 % von 118.200,00 €“ (Zuschlag,
        /// Aufteilung) or „Rechengrundlage, aufgeteilt“; empty without.
        note: String,
        /// Langtext as paragraphs (empty without).
        long_text: Vec<String>,
    },
    /// A Datenpunkt-Abschnitt: one line with the number of data points and
    /// its amount (the appendix „Datenpunkte“ has the details).
    Points {
        ordinal: String,
        text: String,
        /// „480“ (the split on 481–489 is in the appendix).
        cost_group: String,
        points: u64,
        total: Decimal,
    },
}

/// A Datenpunkt-Abschnitt of the Kosten-LV (GU-Fall): printed as an
/// appendix „Datenpunkte“ with the tables of the Datenpunktrechnung.
#[derive(Debug, Clone)]
pub struct PrintCostLvPoints {
    /// OZ and title, e.g. „480 Gebäude- und Anlagenautomation“.
    pub name: String,
    pub estimate: crate::PrintCostEstimate,
}

/// A Kosten-LV for the PDF.
#[derive(Debug, Clone)]
pub struct PrintCostLv {
    pub header: PrintHeader,
    /// Facts under the title block, e.g. („Stufe“, „Kostenberechnung …“).
    pub facts: Vec<(String, String)>,
    /// Vorbemerkungen and Hinweise: heading and paragraphs.
    pub notes: Vec<(String, Vec<String>)>,
    pub groups: Vec<PrintCostLvGroup>,
    pub items: Vec<PrintCostLvItem>,
    pub net_total: Decimal,
    pub vat_percent: Decimal,
    /// Kurztext-Liste after the Zusammenstellung.
    pub list: bool,
    pub landscape: bool,
    /// Datenpunkt-Abschnitte: one appendix each, after the Kurztext-Liste.
    pub points: Vec<PrintCostLvPoints>,
}

/// The PDF with its number of pages.
pub fn render_cost_lv_pdf_counted(model: &PrintCostLv) -> Result<(Vec<u8>, usize), PrintError> {
    let mut files = Files::default();
    let data = cost_lv_json(model, &mut files);
    let world = crate::build_world(TEMPLATE, files, &data)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub fn render_cost_lv_pdf(model: &PrintCostLv) -> Result<Vec<u8>, PrintError> {
    render_cost_lv_pdf_counted(model).map(|(bytes, _)| bytes)
}

fn cost_lv_json(model: &PrintCostLv, files: &mut Files) -> Value {
    let currency = model.header.currency_sign().to_string();
    let amount = |value: Decimal| money_in(value, &currency);
    let share = |value: Decimal| -> String {
        if model.net_total.is_zero() {
            String::new()
        } else {
            format!(
                "{} %",
                number(value * Decimal::ONE_HUNDRED / model.net_total, 1)
            )
        }
    };
    let vat = (model.net_total * model.vat_percent / Decimal::ONE_HUNDRED).round_dp(2);
    json!({
        "header": view::header(&model.header, "Kostenermittlung", files),
        "landscape": model.landscape,
        "facts": model.facts.iter().map(|(label, value)| json!([label, value])).collect::<Vec<_>>(),
        "notes": model.notes.iter().map(|(heading, paragraphs)| json!({
            "heading": heading,
            "paragraphs": paragraphs,
        })).collect::<Vec<_>>(),
        "groups": model.groups.iter().map(|group| json!({
            "level": group.level,
            "code": if group.code.is_empty() { "–".to_string() } else { group.code.clone() },
            "label": group.label,
            "amount": amount(group.amount),
            "share": share(group.amount),
        })).collect::<Vec<_>>(),
        "net": amount(model.net_total),
        "vat_label": format!("Umsatzsteuer {}", percent(model.vat_percent)),
        "vat": amount(vat),
        "gross": amount(model.net_total + vat),
        "list": model.list,
        "points": model.points.iter().map(|points| json!({
            "name": points.name,
            "data": crate::cost_estimate::cost_estimate_json(&points.estimate, files),
        })).collect::<Vec<_>>(),
        "items": model.items.iter().map(|item| match item {
            PrintCostLvItem::Title { level, ordinal, text, total } => json!({
                "t": "title",
                "level": level,
                "oz": breakable(ordinal),
                "text": text,
                "total": amount(*total),
            }),
            PrintCostLvItem::Points { ordinal, text, cost_group, points, total } => json!({
                "t": "points",
                "oz": breakable(ordinal),
                "text": text,
                "kg": cost_group,
                "qty": number(Decimal::from(*points), 0),
                "gp": number(*total, 2),
            }),
            PrintCostLvItem::Position { ordinal, text, cost_group, quantity: count, unit, unit_price, total, counts, note, long_text } => {
                let bracket = |text: String| if *counts { text } else { format!("({text})") };
                json!({
                    "t": "position",
                    "oz": breakable(ordinal),
                    "text": text,
                    "kg": cost_group,
                    "qty": count.map(quantity).unwrap_or_default(),
                    "note": note,
                    "unit": unit,
                    "ep": unit_price.map(|price| bracket(number(price, 2))).unwrap_or_default(),
                    "gp": bracket(number(*total, 2)),
                    "long": long_text,
                })
            }
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> PrintCostLv {
        PrintCostLv {
            header: PrintHeader {
                project_name: "Rathaus 3".into(),
                lv_name: "Kostenberechnung Heizung".into(),
                date: "18.11.2026".into(),
                document_kind: Some("Kostenberechnung nach DIN 276".into()),
                ..PrintHeader::default()
            },
            facts: vec![("Stufe".into(), "Kostenberechnung, 3. Ebene".into())],
            notes: vec![(
                "Angaben zur Kostenberechnung".into(),
                vec!["Alle Beträge netto ohne Umsatzsteuer.".into()],
            )],
            groups: vec![
                PrintCostLvGroup {
                    level: 1,
                    code: "400".into(),
                    label: "Bauwerk – Technische Anlagen".into(),
                    amount: Decimal::from(68_000),
                },
                PrintCostLvGroup {
                    level: 2,
                    code: "420".into(),
                    label: "Wärmeversorgungsanlagen".into(),
                    amount: Decimal::from(66_000),
                },
                PrintCostLvGroup {
                    level: 3,
                    code: "421".into(),
                    label: "Wärmeerzeugungsanlagen".into(),
                    amount: Decimal::from(60_000),
                },
                PrintCostLvGroup {
                    level: 0,
                    code: String::new(),
                    label: "ohne Kostengruppe".into(),
                    amount: Decimal::from(2000),
                },
            ],
            items: vec![
                PrintCostLvItem::Title {
                    level: 0,
                    ordinal: "420".into(),
                    text: "Wärmeversorgungsanlagen".into(),
                    total: Decimal::from(66_000),
                },
                PrintCostLvItem::Position {
                    ordinal: "420.421.0010".into(),
                    text: "Wärmepumpe Sole/Wasser".into(),
                    cost_group: "421".into(),
                    quantity: Some(Decimal::from(40)),
                    unit: "kW".into(),
                    unit_price: Some(Decimal::from(1500)),
                    total: Decimal::from(60_000),
                    counts: true,
                    note: String::new(),
                    long_text: vec!["Kennwert aus eigenem Projekt 2025".into()],
                },
            ],
            net_total: Decimal::from(68_000),
            vat_percent: Decimal::from(19),
            list: true,
            landscape: false,
            points: Vec::new(),
        }
    }

    #[test]
    fn prints_summary_and_list() {
        let model = model();
        let mut files = Files::default();
        let data = cost_lv_json(&model, &mut files);
        assert_eq!(data["groups"][2]["share"], "88,2 %");
        assert_eq!(data["gross"], "80.920,00 €");
        assert_eq!(data["items"][1]["gp"], "60.000,00");
        let (bytes, pages) = render_cost_lv_pdf_counted(&model).expect("PDF");
        assert!(bytes.starts_with(b"%PDF"));
        assert_eq!(pages, 1);
        let mut landscape = model.clone();
        landscape.landscape = true;
        landscape.list = false;
        assert!(render_cost_lv_pdf(&landscape).is_ok());
    }
}
