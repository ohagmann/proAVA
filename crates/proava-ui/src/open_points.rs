//! Offene Punkte: the open work in an LV as a list – positions without
//! quantity, without unit price (when the LV is being priced), without
//! short text or unit, open STLB placeholders and positions „in
//! Bearbeitung“, hints without text. No UI here; the editor shows it in a
//! window.

use std::collections::HashMap;

use proava_domain::{EditStatus, Lv, LvRow, Project, QuantityChange, RowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OpenKind {
    MissingQuantity,
    MissingPrice,
    MissingShortText,
    MissingUnit,
    StlbOpen,
    InProgress,
    /// GP of an imported file did not fit Menge × EP (not yet confirmed).
    PriceCheck,
    /// Hint without heading and text (an empty line: Abstandshalter).
    BlankNote,
    /// LV quantity changed after the bids came in (Anwender 18.11.).
    QuantityAfterBids,
}

impl OpenKind {
    pub const ALL: [OpenKind; 9] = [
        OpenKind::MissingQuantity,
        OpenKind::MissingPrice,
        OpenKind::MissingShortText,
        OpenKind::MissingUnit,
        OpenKind::StlbOpen,
        OpenKind::InProgress,
        OpenKind::PriceCheck,
        OpenKind::BlankNote,
        OpenKind::QuantityAfterBids,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OpenKind::MissingQuantity => "ohne Menge",
            OpenKind::MissingPrice => "ohne EP",
            OpenKind::MissingShortText => "ohne Kurztext",
            OpenKind::MissingUnit => "ohne Einheit",
            OpenKind::StlbOpen => "STLB-Platzhalter offen",
            OpenKind::InProgress => "in Bearbeitung",
            OpenKind::PriceCheck => "GP prüfen (Abweichung aus dem Import)",
            OpenKind::BlankNote => "Hinweis ohne Text",
            OpenKind::QuantityAfterBids => "Massenmehrung/-minderung nach Angebotseingang",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPoint {
    pub row: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub kind: OpenKind,
}

/// Whether missing unit prices count: as soon as one position has a
/// planner's price (Kostenvoranschlag), the others should have one too.
pub fn prices_expected(lv: &Lv) -> bool {
    lv.positions()
        .any(|position| position.unit_price().is_some())
}

/// Quantities changed after the bids came in, over all bids of the active
/// LV (first bid that shows the position).
pub fn quantity_changes(project: &Project) -> HashMap<RowId, QuantityChange> {
    let mut changes = HashMap::new();
    for bid in project.bids().iter().filter(|bid| !bid.is_excluded()) {
        for change in bid.quantity_changes(project.lv()) {
            changes.entry(change.position).or_insert(change);
        }
    }
    changes
}

/// All open points in document order; a position can appear once per kind.
/// `with_prices` adds „ohne EP“.
pub fn collect(lv: &Lv, with_prices: bool) -> Vec<OpenPoint> {
    collect_with(lv, with_prices, &HashMap::new())
}

/// [`collect`] plus „Massenmehrung/-minderung nach Angebotseingang“ for the
/// positions in `changed` ([`quantity_changes`]).
pub fn collect_with(
    lv: &Lv,
    with_prices: bool,
    changed: &HashMap<RowId, QuantityChange>,
) -> Vec<OpenPoint> {
    let mut points = Vec::new();
    // Last OZ above, to say where a hint without text stands.
    let mut place = String::new();
    for row in lv.rows() {
        if row.has_ordinal() && !row.ordinal().trim().is_empty() {
            place = row.ordinal().to_string();
        }
        if row.is_blank_note() {
            points.push(OpenPoint {
                row: row.id(),
                ordinal: String::new(),
                short_text: if place.is_empty() {
                    "Hinweis am Anfang".to_string()
                } else {
                    format!("Hinweis nach {place}")
                },
                kind: OpenKind::BlankNote,
            });
            continue;
        }
        let LvRow::Position(position) = row else {
            continue;
        };
        let stlb = position.is_stlb_placeholder();
        let mut push = |kind| {
            points.push(OpenPoint {
                row: row.id(),
                ordinal: position.ordinal().to_string(),
                short_text: position.short_text().to_string(),
                kind,
            })
        };
        if stlb {
            push(OpenKind::StlbOpen);
        }
        if position.quantity_missing() {
            push(OpenKind::MissingQuantity);
        }
        if with_prices && !stlb && !position.is_not_applicable() && position.unit_price().is_none()
        {
            push(OpenKind::MissingPrice);
        }
        if !stlb && position.short_text().trim().is_empty() {
            push(OpenKind::MissingShortText);
        }
        if !stlb && position.unit().trim().is_empty() && !position.is_surcharge() {
            push(OpenKind::MissingUnit);
        }
        if position.status() == EditStatus::InProgress {
            push(OpenKind::InProgress);
        }
        if position.price_check().is_some() {
            push(OpenKind::PriceCheck);
        }
        if changed.contains_key(&position.id()) {
            push(OpenKind::QuantityAfterBids);
        }
    }
    points
}

/// Number of points per kind, in the order of [`OpenKind::ALL`].
pub fn counts(points: &[OpenPoint]) -> Vec<(OpenKind, usize)> {
    OpenKind::ALL
        .iter()
        .map(|&kind| {
            (
                kind,
                points.iter().filter(|point| point.kind == kind).count(),
            )
        })
        .collect()
}

/// The list as text for the clipboard (tab between the columns).
pub fn as_text(lv_title: &str, points: &[OpenPoint]) -> String {
    let count = match points.len() {
        1 => "1 Eintrag".to_string(),
        count => format!("{count} Einträge"),
    };
    let mut lines = vec![format!("Offene Punkte – {lv_title} ({count})")];
    lines.push("OZ\tKurztext\tPunkt".to_string());
    lines.extend(points.iter().map(|point| {
        format!(
            "{}\t{}\t{}",
            point.ordinal,
            point.short_text.trim(),
            point.kind.label()
        )
    }));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, EditStatus, Lv, LvGroup, OzMask, Position};

    use super::*;

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "A"));
        lv.push_group(LvGroup::new(1, "", "A1"));
        lv.push_position(Position::new(
            "",
            "Station",
            Decimal::ONE,
            "St",
            Some(Decimal::TEN),
        ));
        lv.push_position(Position::new("", "", Decimal::ZERO, "", None));
        lv.push_position(Position::stlb("offen"));
        let mut busy = Position::new("", "Modul", Decimal::TWO, "St", None);
        busy.set_status(EditStatus::InProgress);
        lv.push_position(busy);
        lv.renumber();
        lv
    }

    #[test]
    fn open_points_by_kind() {
        let lv = lv();
        assert!(prices_expected(&lv));
        let points = collect(&lv, true);
        let kinds = |ordinal: &str| -> Vec<OpenKind> {
            points
                .iter()
                .filter(|point| point.ordinal == ordinal)
                .map(|point| point.kind)
                .collect()
        };
        assert!(kinds("01.01.0010").is_empty());
        assert_eq!(
            kinds("01.01.0020"),
            vec![
                OpenKind::MissingQuantity,
                OpenKind::MissingPrice,
                OpenKind::MissingShortText,
                OpenKind::MissingUnit
            ]
        );
        assert_eq!(kinds("01.01.0030"), vec![OpenKind::StlbOpen]);
        assert_eq!(
            kinds("01.01.0040"),
            vec![OpenKind::MissingPrice, OpenKind::InProgress]
        );
        assert!(!collect(&lv, false)
            .iter()
            .any(|point| point.kind == OpenKind::MissingPrice));
        let counts = counts(&points);
        assert_eq!(counts[1], (OpenKind::MissingPrice, 2));
        let text = as_text("LV", &points);
        assert!(
            text.starts_with("Offene Punkte – LV (7 Einträge)"),
            "{text}"
        );
        assert!(text.contains("01.01.0040\tModul\tin Bearbeitung"));
    }

    #[test]
    fn hints_without_text_are_open_points_spacers_not() {
        let mut lv = lv();
        let blank = lv.push_note(proava_domain::Note::new(""));
        lv.push_spacer();
        let points = collect(&lv, false);
        let point = points
            .iter()
            .find(|point| point.kind == OpenKind::BlankNote)
            .expect("Hinweis ohne Text");
        assert_eq!(point.row, blank);
        assert_eq!(point.short_text, "Hinweis nach 01.01.0040");
        assert_eq!(
            points
                .iter()
                .filter(|point| point.kind == OpenKind::BlankNote)
                .count(),
            1
        );
        let index = lv.index_of(blank).unwrap();
        let mut converted = lv.clone();
        converted.blank_notes_to_spacers(&[blank]);
        assert!(converted.rows()[index].is_spacer());
        assert!(!collect(&converted, false)
            .iter()
            .any(|point| point.kind == OpenKind::BlankNote));
    }
}
