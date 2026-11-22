//! Nachträge: „Prüfung Nachtragsangebot“ (one change order with its items
//! offered and checked, notes, result, recommendation and signatures) and
//! „Nachtragsübersicht“ (all change orders with sums by status, contract sum
//! old/new and forecast). Amounts come formatted from the caller.

use serde_json::{json, Value};

use crate::format::breakable;
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const CHECK_TEMPLATE: &str = include_str!("../templates/change_order.typ");
const OVERVIEW_TEMPLATE: &str = include_str!("../templates/change_order_overview.typ");

/// One position of the Prüfung Nachtragsangebot.
#[derive(Debug, Clone, Default)]
pub struct ChangeOrderCheckLine {
    pub ordinal: String,
    pub short_text: String,
    /// „neu“, „Mehrmenge“, „Mindermenge“.
    pub kind: String,
    pub unit: String,
    pub offered_quantity: String,
    pub offered_price: String,
    pub offered_total: String,
    pub checked_quantity: String,
    pub checked_price: String,
    pub checked_total: String,
    /// Checked EP against offered EP, e.g. „−12,8 %“.
    pub deviation: String,
    /// Status when it differs from the change order („abgelehnt“), else empty.
    pub status: String,
    pub note: String,
    /// Rejected or withdrawn: shown struck out, not in the checked sum.
    pub dropped: bool,
}

/// Everything printed in the Prüfung Nachtragsangebot.
#[derive(Debug, Clone, Default)]
pub struct PrintChangeOrderCheck {
    pub header: PrintHeader,
    /// „N01 Wetterstation auf dem Dach“.
    pub title: String,
    /// („Auftragnehmer“, …), („Auftrag“, …), („Anlass“, …) …
    pub facts: Vec<(String, String)>,
    pub reason: String,
    pub lines: Vec<ChangeOrderCheckLine>,
    /// Sum lines (label, amount, bold).
    pub sums: Vec<(String, String, bool)>,
    pub check_note: String,
    pub recommendation: String,
    /// „Fachtechnisch und rechnerisch geprüft am … von …“.
    pub checked: String,
    /// Status history lines („12.10.2026 angeboten – per Mail“).
    pub history: Vec<String>,
    /// Hints (missing prices, positions twice …).
    pub notes: Vec<String>,
    /// Captions under the signature lines.
    pub signatures: Vec<String>,
}

/// One change order of the Nachtragsübersicht.
#[derive(Debug, Clone, Default)]
pub struct ChangeOrderOverviewLine {
    pub label: String,
    pub title: String,
    pub basis: String,
    pub reason: String,
    pub status: String,
    pub date: String,
    pub offered: String,
    pub checked: String,
    pub approved: String,
    pub credit: String,
    pub forecast: String,
}

/// Everything printed in the Nachtragsübersicht.
#[derive(Debug, Clone, Default)]
pub struct PrintChangeOrderOverview {
    pub header: PrintHeader,
    pub facts: Vec<(String, String)>,
    pub lines: Vec<ChangeOrderOverviewLine>,
    /// Sum row: offered, checked, approved, credit.
    pub total: [String; 4],
    /// (status, count, offered, checked, approved).
    pub by_status: Vec<[String; 5]>,
    /// Contract sum old/new, forecast: (label, amount, bold).
    pub contract: Vec<(String, String, bool)>,
    pub notes: Vec<String>,
}

pub fn render_change_order_check_pdf(
    check: &PrintChangeOrderCheck,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = check_world(check)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub fn render_change_order_overview_pdf(
    overview: &PrintChangeOrderOverview,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = overview_world(overview)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub(crate) fn check_world(
    check: &PrintChangeOrderCheck,
) -> Result<crate::world::PrintWorld, PrintError> {
    let mut files = Files::default();
    let data = check_data(check, &mut files);
    crate::build_world(CHECK_TEMPLATE, files, &data)
}

pub(crate) fn overview_world(
    overview: &PrintChangeOrderOverview,
) -> Result<crate::world::PrintWorld, PrintError> {
    let mut files = Files::default();
    let data = overview_data(overview, &mut files);
    crate::build_world(OVERVIEW_TEMPLATE, files, &data)
}

fn facts(facts: &[(String, String)]) -> Vec<Value> {
    facts
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(label, text)| json!({"label": label, "text": breakable(text)}))
        .collect()
}

fn sums(lines: &[(String, String, bool)]) -> Vec<Value> {
    lines
        .iter()
        .map(|(label, amount, bold)| json!({"label": label, "amount": amount, "bold": bold}))
        .collect()
}

fn check_data(check: &PrintChangeOrderCheck, files: &mut Files) -> Value {
    let lines: Vec<Value> = check
        .lines
        .iter()
        .map(|line| {
            json!({
                // PDF-Lesezeichen under „Positionen“.
                "bm": view::bookmark(2, &[&line.ordinal, &line.short_text]),
                "oz": line.ordinal,
                "text": breakable(&line.short_text),
                "kind": line.kind,
                "unit": line.unit,
                "oq": line.offered_quantity,
                "op": line.offered_price,
                "ot": line.offered_total,
                "cq": line.checked_quantity,
                "cp": line.checked_price,
                "ct": line.checked_total,
                "dev": line.deviation,
                "status": line.status,
                "note": breakable(&line.note),
                "dropped": line.dropped,
            })
        })
        .collect();
    json!({
        "header": view::header_with(&check.header, "Prüfung Nachtragsangebot", files, &[
            ("nachtrag", check.title.clone()),
        ]),
        "title": breakable(&check.title),
        "bm": view::bookmark(1, &[&check.title]),
        "facts": facts(&check.facts),
        "reason": breakable(&check.reason),
        "lines": lines,
        "sums": sums(&check.sums),
        "check_note": breakable(&check.check_note),
        "recommendation": breakable(&check.recommendation),
        "checked": check.checked,
        "history": check.history,
        "notes": check.notes,
        "signatures": check.signatures,
    })
}

fn overview_data(overview: &PrintChangeOrderOverview, files: &mut Files) -> Value {
    let lines: Vec<Value> = overview
        .lines
        .iter()
        .map(|line| {
            json!({
                // PDF-Lesezeichen under „Nachträge“.
                "bm": view::bookmark(2, &[&line.label, &line.title]),
                "label": line.label,
                "title": breakable(&line.title),
                "basis": breakable(&line.basis),
                "reason": breakable(&line.reason),
                "status": line.status,
                "date": line.date,
                "offered": line.offered,
                "checked": line.checked,
                "approved": line.approved,
                "credit": line.credit,
                "forecast": line.forecast,
            })
        })
        .collect();
    json!({
        "header": view::header_with(&overview.header, "Nachtragsübersicht", files, &[]),
        "facts": facts(&overview.facts),
        "lines": lines,
        "total": overview.total,
        "by_status": overview.by_status,
        "contract": sums(&overview.contract),
        "notes": overview.notes,
    })
}

#[cfg(test)]
mod tests {
    use typst::layout::{Frame, FrameItem};

    use super::*;

    fn collect(frame: &Frame, text: &mut String) {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Group(group) => collect(&group.frame, text),
                FrameItem::Text(t) => text.push_str(&t.text),
                _ => {}
            }
        }
    }

    /// All text of the laid-out pages without spaces (search with words joined).
    fn text_of(world: crate::world::PrintWorld) -> String {
        let document = world.layout().unwrap();
        let mut text = String::new();
        for page in document.pages() {
            collect(&page.frame, &mut text);
        }
        text.split_whitespace().collect::<String>()
    }

    #[test]
    fn check_and_overview_render() {
        let check = PrintChangeOrderCheck {
            title: "N01 Wetterstation".into(),
            facts: vec![("Auftragnehmer".into(), "Firma A".into())],
            reason: "Bauherrenwunsch".into(),
            lines: vec![
                ChangeOrderCheckLine {
                    ordinal: "04.01.0010".into(),
                    short_text: "Wetterstation".into(),
                    kind: "neu".into(),
                    unit: "St".into(),
                    offered_quantity: "1".into(),
                    offered_price: "1.947,00".into(),
                    offered_total: "1.947,00".into(),
                    checked_quantity: "1".into(),
                    checked_price: "1.697,00".into(),
                    checked_total: "1.697,00".into(),
                    deviation: "−12,8".into(),
                    note: "DDC-Automationsstation geprüft".into(),
                    ..ChangeOrderCheckLine::default()
                },
                ChangeOrderCheckLine {
                    ordinal: "04.01.0020".into(),
                    short_text: "Zuschlag".into(),
                    status: "abgelehnt".into(),
                    dropped: true,
                    ..ChangeOrderCheckLine::default()
                },
            ],
            sums: vec![(
                "Geprüfte Nachtragssumme netto".into(),
                "1.697,00 €".into(),
                true,
            )],
            check_note: "In Ordnung".into(),
            recommendation: "Beauftragen".into(),
            checked: "Fachtechnisch und rechnerisch geprüft am 20.10.2026 von O. Planer".into(),
            history: vec!["12.10.2026 angeboten".into()],
            notes: vec![],
            signatures: vec!["Planer".into(), "Auftraggeber".into()],
            ..PrintChangeOrderCheck::default()
        };
        let (pdf, pages) = render_change_order_check_pdf(&check).unwrap();
        assert!(pdf.starts_with(b"%PDF") && pages >= 1);
        // PDF-Lesezeichen, print unchanged by them.
        let outline =
            crate::tests::bookmarks_without_change(|| check_world(&check).unwrap(), "Nachtrag");
        let entries: Vec<String> = outline
            .iter()
            .map(|(level, text)| format!("{level} {text}"))
            .collect();
        assert_eq!(
            entries,
            [
                "1 N01 Wetterstation",
                "1 Positionen",
                "2 04.01.0010 Wetterstation",
                "2 04.01.0020 Zuschlag",
                "1 Ergebnis der Prüfung",
                "1 Unterschriften",
            ]
        );
        let text = text_of(check_world(&check).unwrap());
        // Break chances in long words print no character (U+200B).
        for wanted in [
            "PrüfungNachtragsangebot",
            "N01Wetterstation",
            "1.697,00",
            "abgelehnt",
            "Beauftragen",
            "DDC-Automationsstationgeprüft",
        ] {
            assert!(text.contains(wanted), "{wanted} fehlt");
        }
        let overview = PrintChangeOrderOverview {
            lines: vec![ChangeOrderOverviewLine {
                label: "N01".into(),
                title: "Wetterstation".into(),
                status: "beauftragt".into(),
                approved: "2.047,00 €".into(),
                ..ChangeOrderOverviewLine::default()
            }],
            total: [
                "2.297,00 €".into(),
                "2.047,00 €".into(),
                "2.047,00 €".into(),
                "0,00 €".into(),
            ],
            by_status: vec![[
                "beauftragt".into(),
                "1".into(),
                "2.297,00 €".into(),
                "2.047,00 €".into(),
                "2.047,00 €".into(),
            ]],
            contract: vec![("Auftragssumme neu".into(), "115.947,22 €".into(), true)],
            ..PrintChangeOrderOverview::default()
        };
        let (pdf, _) = render_change_order_overview_pdf(&overview).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let outline = crate::tests::bookmarks_without_change(
            || overview_world(&overview).unwrap(),
            "Nachtragsübersicht",
        );
        let entries: Vec<String> = outline
            .iter()
            .map(|(level, text)| format!("{level} {text}"))
            .collect();
        assert_eq!(
            entries,
            [
                "1 Nachträge",
                "2 N01 Wetterstation",
                "1 Summen je Status",
                "1 Auftragssumme"
            ]
        );
        let text = text_of(overview_world(&overview).unwrap());
        for wanted in [
            "Nachtragsübersicht",
            "Wetterstation",
            "115.947,22",
            "SummenjeStatus",
        ] {
            assert!(text.contains(wanted), "{wanted} fehlt");
        }
    }
}
