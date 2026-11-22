//! Kostenverfolgung in `projekt.xml` (schema 2, additive: files without the
//! elements read as before, older proAVA ignore them). Everything else of
//! the Kostenverfolgung is computed from the project.
//!
//! ```xml
//! <cost-tracking budget-kind="obergrenze" budget-date="18.11.2026" budget-note="aus …" threshold="10">
//!   <budget code="481" amount="120000.00"/>
//!   <note code="481" text="Mehrkosten durch …; Maßnahme: …"/>
//!   <source key="datenpunkte:3" counts="0"/>
//!   <snapshot date="18.11.2026" note="Kostenberechnung abgestimmt">
//!     <cell lv="LV 2 Lüftung" code="431" net="…;…" gross="…;…"/>   (9 values in cents, empty = none)
//!   </snapshot>
//! </cost-tracking>
//! ```
//!
//! Per LV, inside `<lv>`: `<offer-choice bid="2" reason="wirtschaftlichstes Angebot"/>`.

use proava_domain::{
    BidId, BudgetKind, CostSnapshot, CostTracking, Money, OfferChoice, Project, SnapshotCell,
};

use crate::project_xml::optional_decimal;
use crate::xml_tree::Element;

const STAGES: usize = proava_domain::TrackStage::ALL.len();

fn money_text(value: Money) -> String {
    value.to_decimal().to_string()
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

/// The element for the project, None when nothing is entered.
pub(crate) fn write(project: &Project) -> Option<Element> {
    let tracking = project.cost_tracking();
    if tracking.is_empty() {
        return None;
    }
    let mut element = Element::new("cost-tracking")
        .attr("budget-kind", tracking.budget_kind.key())
        .attr_opt("budget-date", non_empty(&tracking.budget_date))
        .attr_opt("budget-note", non_empty(&tracking.budget_note))
        .attr(
            "threshold",
            tracking.threshold_percent.normalize().to_string(),
        );
    for (code, amount) in &tracking.budget {
        element.push(
            Element::new("budget")
                .attr("code", code.as_str())
                .attr("amount", money_text(*amount)),
        );
    }
    for (code, text) in &tracking.notes {
        element.push(
            Element::new("note")
                .attr("code", code.as_str())
                .attr("text", text.as_str()),
        );
    }
    for (key, counts) in &tracking.source_choice {
        element.push(
            Element::new("source")
                .attr("key", key.as_str())
                .attr("counts", if *counts { "1" } else { "0" }),
        );
    }
    for snapshot in &tracking.snapshots {
        element.push(snapshot_element(snapshot));
    }
    Some(element)
}

fn values_text(values: &[Option<Money>; STAGES]) -> String {
    values
        .iter()
        .map(|value| {
            value
                .map(|value| value.cents().to_string())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn snapshot_element(snapshot: &CostSnapshot) -> Element {
    Element::new("snapshot")
        .attr("date", snapshot.date.as_str())
        .attr_opt("note", non_empty(&snapshot.note))
        .children(snapshot.cells.iter().map(|cell| {
            Element::new("cell")
                .attr_opt("lv", cell.lv.as_deref())
                .attr_opt("code", cell.code.as_deref())
                .attr("net", values_text(&cell.net))
                .attr("gross", values_text(&cell.gross))
        }))
}

/// The chosen bid of the LV at `index`, inside its `<lv>` element.
pub(crate) fn write_lv(project: &Project, index: usize, element: &mut Element) {
    if let Some(choice) = project.offer_choice_of(index) {
        element.push(
            Element::new("offer-choice")
                .attr("bid", choice.bid.0.to_string())
                .attr_opt("reason", non_empty(&choice.reason)),
        );
    }
}

fn values_of(text: &str, what: &str) -> Result<[Option<Money>; STAGES], String> {
    let mut values = [None; STAGES];
    for (slot, part) in values.iter_mut().zip(text.split(';')) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let cents: i64 = part
            .parse()
            .map_err(|_| format!("{what}: „{part}“ ist keine Zahl."))?;
        *slot = Some(Money::from_cents(cents));
    }
    Ok(values)
}

/// Reads the Kostenverfolgung and the chosen bids (after all bids are read).
pub(crate) fn read(project: &mut Project, root: &Element) -> Result<(), String> {
    for (index, node) in root.all("lv").enumerate() {
        if let Some(choice) = node.first("offer-choice") {
            let bid: Option<u64> = crate::project_xml::number(choice, "bid")?;
            if let Some(bid) = bid {
                // A bid that is gone (file edited by hand) is ignored.
                project.set_offer_choice_of(
                    index,
                    Some(OfferChoice {
                        bid: BidId(bid),
                        reason: choice.text("reason"),
                    }),
                );
            }
        }
    }
    let Some(node) = root.first("cost-tracking") else {
        return Ok(());
    };
    let mut tracking = CostTracking {
        // An unknown kind (a newer proAVA) reads as Obergrenze.
        budget_kind: BudgetKind::from_key(&node.text("budget-kind")).unwrap_or_default(),
        budget_date: node.text("budget-date"),
        budget_note: node.text("budget-note"),
        ..CostTracking::default()
    };
    if let Some(threshold) = optional_decimal(node, "threshold", "Schwelle der Kostenverfolgung")? {
        tracking.threshold_percent = threshold;
    }
    for budget in node.all("budget") {
        let amount = optional_decimal(budget, "amount", "Budget")?.unwrap_or_default();
        tracking.set_budget(&budget.text("code"), Money::from_decimal(amount));
    }
    for note in node.all("note") {
        tracking.set_note(&note.text("code"), &note.text("text"));
    }
    for source in node.all("source") {
        tracking
            .source_choice
            .insert(source.text("key"), source.text("counts") != "0");
    }
    for snapshot in node.all("snapshot") {
        let mut cells = Vec::new();
        for cell in snapshot.all("cell") {
            cells.push(SnapshotCell {
                lv: cell.get("lv").map(str::to_string),
                code: cell.get("code").map(str::to_string),
                net: values_of(&cell.text("net"), "Kostenstand netto")?,
                gross: values_of(&cell.text("gross"), "Kostenstand brutto")?,
            });
        }
        tracking.snapshots.push(CostSnapshot {
            date: snapshot.text("date"),
            note: snapshot.text("note"),
            cells,
        });
    }
    project.replace_cost_tracking(tracking);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{project_from_bytes, project_to_bytes};
    use proava_domain::{Address, Bid, Decimal, Lv, OzMask, Position, ProjectName, TrackStage};

    #[test]
    fn budget_notes_choices_states_and_the_chosen_bid_survive_the_file() {
        let mut lv = Lv::new("Heizung", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let row = lv.push_position(Position::new(
            "",
            "Wärmepumpe",
            Decimal::ONE,
            "St",
            Some(Decimal::from(1000)),
        ));
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18T12:00:00",
            lv,
        );
        let mut bid = Bid::new(Address::named("Wärme Süd"), "a.X84");
        bid.set_unit_price(row, Some(Decimal::from(900)));
        let bid = project.add_bid_to(0, bid);
        project.set_offer_choice_of(
            0,
            Some(OfferChoice {
                bid,
                reason: "nach Wertung\nzweite Zeile".into(),
            }),
        );
        {
            let tracking = project.cost_tracking_mut();
            tracking.set_budget("421", Money::from_cents(1_234_567));
            tracking.set_budget("480", Money::from_cents(100));
            tracking.budget_kind = BudgetKind::Target;
            tracking.budget_date = "18.11.2026".into();
            tracking.budget_note = "aus Kostenberechnung".into();
            tracking.threshold_percent = Decimal::new(75, 1);
            tracking.set_note("421", "Mehrkosten Wärmepumpe;\nMaßnahme: Variante prüfen");
            tracking.source_choice.insert("datenpunkte:2".into(), false);
            let mut cell = SnapshotCell {
                lv: Some("LV 1 Heizung".into()),
                code: Some("421".into()),
                ..SnapshotCell::default()
            };
            cell.net[TrackStage::Forecast.index()] = Some(Money::from_cents(-5));
            cell.gross[TrackStage::Budget.index()] = Some(Money::from_cents(7));
            tracking.snapshots.push(CostSnapshot {
                date: "17.11.2026".into(),
                note: "LPH 3".into(),
                cells: vec![
                    cell,
                    SnapshotCell {
                        lv: None,
                        code: None,
                        ..SnapshotCell::default()
                    },
                ],
            });
        }
        let bytes = project_to_bytes(&project).unwrap();
        let back = project_from_bytes(&bytes).unwrap();
        assert_eq!(back.cost_tracking(), project.cost_tracking());
        assert_eq!(back.offer_choice_of(0), project.offer_choice_of(0));
        // Nothing entered: no element.
        let empty = Project::new(
            ProjectName::parse("Leer").unwrap(),
            "2026-11-18T12:00:00",
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        let text = crate::project_xml::write_project_xml(&empty).unwrap();
        assert!(!text.contains("cost-tracking"));
        let text = crate::project_xml::write_project_xml(&project).unwrap();
        assert!(text.contains("<offer-choice bid=\"1\""), "{text}");
    }
}
