//! Checking and updating the STLB-Bau positions of an LV, the way AVA
//! programs do it with STLB-Bau: the STLB-Bau positions go out as a GAEB
//! file ([`positions_for_check`]), STLB-Bau checks and updates them, and
//! the file it writes comes back ([`plan_update`]). Each position of the
//! file is matched to the LV by its OZ, else by its Leistung (LB + Gruppe,
//! when that is unique); the differences are those of the LV-Vergleich.
//! [`apply_updates`] takes over texts, unit, Textergänzungen and the key –
//! OZ, quantity, prices and position kind stay.

use std::collections::HashSet;

use crate::edit_status::EditStatus;
use crate::lv::{Lv, LvRow, RowId};
use crate::lv_diff::{ordinal_key, row_details, Detail};
use crate::position::Position;

/// The LV reduced to its positions with an STLB-Bau key and the titles
/// above them; OZ and everything else unchanged (for the GAEB file that goes
/// to STLB-Bau).
pub fn positions_for_check(lv: &Lv) -> Lv {
    let mut reduced = lv.clone();
    let keep = |row: &LvRow| {
        row.as_position()
            .is_some_and(|position| position.stlb_reference().is_some())
    };
    // Positions without a key, notes and Ausführungsbeschreibungen go.
    for index in (0..reduced.rows().len()).rev() {
        let row = &reduced.rows()[index];
        if row.as_group().is_none() && !keep(row) {
            reduced.remove_row(index);
        }
    }
    // Then titles without any position below them (deepest first).
    loop {
        let rows = reduced.rows();
        let empty = (0..rows.len()).rev().find(|&index| {
            let Some(group) = rows[index].as_group() else {
                return false;
            };
            !rows[index + 1..]
                .iter()
                .take_while(|row| {
                    row.as_group()
                        .is_none_or(|below| below.level() > group.level())
                })
                .any(LvRow::is_position)
        });
        match empty {
            Some(index) => {
                reduced.remove_row(index);
            }
            None => break,
        }
    }
    reduced
}

/// How a position of the file found its position in the LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedBy {
    Ordinal,
    /// Same Leistungsbereich and Gruppe, the only such position left.
    Group,
}

impl MatchedBy {
    pub fn label(self) -> &'static str {
        match self {
            MatchedBy::Ordinal => "über die OZ",
            MatchedBy::Group => "über die Leistung",
        }
    }
}

/// One LV position with its updated text from STLB-Bau.
#[derive(Debug, Clone, PartialEq)]
pub struct StlbUpdate {
    pub row: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub matched_by: MatchedBy,
    /// „fertig“: shown, but not taken over.
    pub locked: bool,
    /// Catalogue line before and after, e.g. „STLB-Bau 10/2025 070“ →
    /// „STLB-Bau 4/2026 070“ (equal when the version did not change).
    pub catalog: (String, String),
    /// Differences as in the LV-Vergleich (Kurztext, Langtext, Einheit,
    /// Textergänzungen).
    pub details: Vec<Detail>,
    /// The position as it comes from the file.
    pub source: Position,
}

/// Result of matching a file from STLB-Bau with the LV.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UpdatePlan {
    /// Positions whose text or key changes, in LV order.
    pub updates: Vec<StlbUpdate>,
    /// Matched positions without any difference.
    pub unchanged: usize,
    /// Positions of the file without a partner in the LV („OZ Kurztext“).
    pub unmatched: Vec<String>,
}

/// Matches the positions read from STLB-Bau (`sources`, e.g. from
/// `ExchangeFiles::read_catalog_positions`) with the STLB-Bau positions of
/// the LV.
pub fn plan_update(lv: &Lv, sources: &[Position]) -> UpdatePlan {
    let candidates: Vec<(usize, &Position)> = lv
        .rows()
        .iter()
        .enumerate()
        .filter_map(|(index, row)| row.as_position().map(|position| (index, position)))
        .filter(|(_, position)| position.stlb_reference().is_some())
        .collect();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut pairs: Vec<(usize, usize, MatchedBy)> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    // 1. Same OZ (pieces without leading zeros).
    for (source_index, source) in sources.iter().enumerate() {
        let key = ordinal_key(source.ordinal());
        let found = (!source.ordinal().trim().is_empty())
            .then(|| {
                candidates
                    .iter()
                    .find(|(index, position)| {
                        !taken.contains(index) && ordinal_key(position.ordinal()) == key
                    })
                    .map(|(index, _)| *index)
            })
            .flatten();
        match found {
            Some(index) => {
                taken.insert(index);
                pairs.push((index, source_index, MatchedBy::Ordinal));
            }
            None => open.push(source_index),
        }
    }
    // 2. Same Leistung, when exactly one such LV position is left.
    let mut unmatched = Vec::new();
    for source_index in open {
        let source = &sources[source_index];
        let same: Vec<usize> = source
            .stlb_reference()
            .map(|wanted| {
                candidates
                    .iter()
                    .filter(|(index, position)| {
                        !taken.contains(index)
                            && position.stlb_reference().is_some_and(|have| {
                                have.group == wanted.group
                                    && have.work_category.trim() == wanted.work_category.trim()
                            })
                    })
                    .map(|(index, _)| *index)
                    .collect()
            })
            .unwrap_or_default();
        if let [index] = same[..] {
            taken.insert(index);
            pairs.push((index, source_index, MatchedBy::Group));
        } else {
            unmatched.push(
                format!("{} {}", source.ordinal(), source.short_text())
                    .trim()
                    .to_string(),
            );
        }
    }
    pairs.sort_by_key(|(index, _, _)| *index);
    let mut plan = UpdatePlan {
        unmatched,
        ..UpdatePlan::default()
    };
    for (index, source_index, matched_by) in pairs {
        let Some(current) = lv.rows()[index].as_position() else {
            continue;
        };
        let source = &sources[source_index];
        let mut updated = current.clone();
        update_from_catalog(&mut updated, source);
        let details = row_details(
            &LvRow::Position(current.clone()),
            &LvRow::Position(updated.clone()),
        );
        let catalog = (
            current.catalog_line().unwrap_or_default(),
            updated.catalog_line().unwrap_or_default(),
        );
        let same_key = current.stlb_reference().map(key_without_fingerprint)
            == updated.stlb_reference().map(key_without_fingerprint);
        if details.is_empty() && same_key {
            plan.unchanged += 1;
            continue;
        }
        plan.updates.push(StlbUpdate {
            row: current.id(),
            ordinal: current.ordinal().to_string(),
            short_text: current.short_text().to_string(),
            matched_by,
            locked: current.status() == EditStatus::Done,
            catalog,
            details,
            source: source.clone(),
        });
    }
    plan
}

fn key_without_fingerprint(stlb: &crate::StlbReference) -> crate::StlbReference {
    crate::StlbReference {
        text_fingerprint: None,
        legacy_fingerprint: None,
        ..stlb.clone()
    }
}

/// Takes the updated text of STLB-Bau: Kurz- and Langtext, unit,
/// Textergänzungen, Unterbeschreibungen and the key (with a new fingerprint);
/// OZ, quantity, prices, position kind and cost group stay. The status goes
/// back to „offen“: the planner checks the new text.
pub fn update_from_catalog(target: &mut Position, source: &Position) {
    target.set_short_text(source.short_text());
    target.set_long_text(source.long_text().clone());
    if !source.unit().trim().is_empty() {
        target.set_unit(source.unit());
    }
    target.set_complements(source.complements().to_vec());
    target.set_sub_descriptions(source.sub_descriptions().to_vec());
    target.copy_catalog_keys_from(source);
    target.set_stlb_placeholder(false);
    target.set_status(EditStatus::Open);
}

/// Applies the chosen updates (locked positions are left alone); returns
/// how many positions changed.
pub fn apply_updates<'a>(lv: &mut Lv, updates: impl IntoIterator<Item = &'a StlbUpdate>) -> usize {
    let mut count = 0;
    for update in updates {
        if update.locked {
            continue;
        }
        let Some(index) = lv.index_of(update.row) else {
            continue;
        };
        if let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
            update_from_catalog(position, &update.source);
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::lv_diff::Aspect;
    use crate::rich_text::RichText;
    use crate::{LvGroup, OzMask, StlbReference};

    fn stlb(position: &mut Position, version: &str, group: i64) {
        position.attach_stlb(StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some(version.into()),
            work_category: "070".into(),
            group,
            ..StlbReference::default()
        });
    }

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "GA"));
        lv.push_group(LvGroup::new(1, "", "Feldgeräte"));
        let mut sensor = Position::new(
            "",
            "Fühler",
            Decimal::from(12),
            "St",
            Some(Decimal::from(80)),
        );
        sensor.set_long_text(RichText::plain(
            "Kanalfühler PT1000, Messbereich 0 bis 50 Grad C.",
        ));
        stlb(&mut sensor, "2025-10", 9001);
        lv.push_position(sensor);
        lv.push_position(Position::new("", "Frei", Decimal::ONE, "psch", None));
        let mut device = Position::new("", "Raumbediengerät", Decimal::from(4), "St", None);
        device.set_long_text(RichText::plain("Raumbediengerät Unterputz."));
        stlb(&mut device, "2025-10", 9002);
        lv.push_position(device);
        lv.push_group(LvGroup::new(1, "", "Nur frei"));
        lv.push_position(Position::new("", "Montage", Decimal::ONE, "psch", None));
        lv.renumber();
        lv
    }

    #[test]
    fn check_file_holds_only_stlb_positions_with_their_titles_and_ordinals() {
        let lv = lv();
        let reduced = positions_for_check(&lv);
        let rows: Vec<String> = reduced
            .rows()
            .iter()
            .map(|row| format!("{} {}", row.ordinal(), row.short_text()))
            .collect();
        assert_eq!(
            rows,
            vec![
                "01 GA",
                "01.01 Feldgeräte",
                "01.01.0010 Fühler",
                "01.01.0030 Raumbediengerät"
            ]
        );
    }

    #[test]
    fn updated_texts_are_matched_compared_and_taken_over() {
        let mut lv = lv();
        // Datei aus STLB-Bau: Fühler über die OZ (neuer Text, neue Version),
        // Raumbediengerät mit anderer OZ über die Leistung, eine fremde Position.
        let mut sensor = Position::new("01.01.0010", "Fühler", Decimal::ONE, "St", None);
        sensor.set_long_text(RichText::plain(
            "Kanalfühler PT1000, Messbereich -30 bis 70 Grad C.",
        ));
        stlb(&mut sensor, "2026-04", 9001);
        let mut device = Position::new("5", "Raumbediengerät", Decimal::ONE, "St", None);
        device.set_long_text(RichText::plain("Raumbediengerät Unterputz."));
        stlb(&mut device, "2026-04", 9002);
        let mut unknown = Position::new("9", "Unbekannt", Decimal::ONE, "St", None);
        stlb(&mut unknown, "2026-04", 9999);
        let plan = plan_update(&lv, &[sensor, device, unknown]);
        assert_eq!(plan.unmatched, vec!["9 Unbekannt"]);
        assert_eq!(plan.unchanged, 0);
        assert_eq!(plan.updates.len(), 2, "Version neu = Kennung geändert");
        let first = &plan.updates[0];
        assert_eq!(first.ordinal, "01.01.0010");
        assert_eq!(first.matched_by, MatchedBy::Ordinal);
        assert_eq!(
            first.catalog,
            (
                "STLB-Bau 10/2025 070".to_string(),
                "STLB-Bau 4/2026 070".to_string()
            )
        );
        let long = first
            .details
            .iter()
            .find(|detail| detail.aspect == Aspect::LongText)
            .unwrap();
        assert!(
            long.old.contains('0') && long.new.contains("-30"),
            "{long:?}"
        );
        assert!(
            !first
                .details
                .iter()
                .any(|detail| detail.aspect == Aspect::Quantity),
            "Menge bleibt"
        );
        assert_eq!(plan.updates[1].matched_by, MatchedBy::Group);
        assert!(plan.updates[1].details.is_empty(), "nur die Version");

        let changed = apply_updates(&mut lv, &plan.updates);
        assert_eq!(changed, 2);
        let sensor = lv.positions().next().unwrap();
        assert_eq!(sensor.quantity(), Decimal::from(12));
        assert_eq!(sensor.unit_price(), Some(Decimal::from(80)));
        assert_eq!(sensor.ordinal(), "01.01.0010");
        assert!(sensor.long_text().to_plain().contains("-30 bis 70"));
        assert_eq!(
            sensor.catalog_line().as_deref(),
            Some("STLB-Bau 4/2026 070")
        );
        assert!(!sensor.stlb_text_changed());
        assert!(plan_update(&lv, &[]).updates.is_empty());
    }

    #[test]
    fn finished_positions_are_shown_but_not_changed() {
        let mut lv = lv();
        let id = lv.positions().next().unwrap().id();
        lv.set_status(&[id], EditStatus::Done);
        let mut sensor = Position::new("01.01.0010", "Fühler neu", Decimal::ONE, "St", None);
        stlb(&mut sensor, "2026-04", 9001);
        let plan = plan_update(&lv, &[sensor]);
        assert!(plan.updates[0].locked);
        let before = lv.clone();
        assert_eq!(apply_updates(&mut lv, &plan.updates), 0);
        assert_eq!(lv, before);
    }
}
