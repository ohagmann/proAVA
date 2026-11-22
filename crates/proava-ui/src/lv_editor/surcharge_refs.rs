//! Zuschlagsposition in the window „Eigenschaften“ (Reiter Allgemein): the
//! kind of Zuschlag, Bezugssumme and Betrag, and for a Zuschlag „auf genannte
//! Positionen“ the list of positions with their Teilmenge. Positions are
//! added by search (OZ or Kurztext), as a range „von … bis …“ or from the rows
//! marked in the table.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    normalize_ordinal, Decimal, Lv, LvRow, Project, RowId, Surcharge, SurchargeKind,
    SurchargeReference,
};

use super::{EditorOutput, LvEditor};
use crate::theme::Palette;
use crate::undo::EditKey;

/// Typed text of the Bezug fields (kept while the window shows the row).
#[derive(Debug, Default)]
pub(super) struct SurchargeInput {
    /// Search for positions to add (OZ or part of the Kurztext).
    filter: String,
    /// Range „von OZ … bis OZ …“.
    from: String,
    to: String,
    /// Teilmenge being typed, per listed position.
    parts: HashMap<RowId, String>,
    error: Option<String>,
}

/// Ordinary positions (no Zuschlag, not `own`) that may be listed.
fn candidate(lv: &Lv, own: RowId, id: RowId) -> bool {
    lv.position_by_id(id)
        .is_some_and(|position| !position.is_surcharge() && id != own)
}

/// Positions whose OZ or Kurztext contains `filter` (at most `limit`), not
/// listed yet.
pub(super) fn search(
    lv: &Lv,
    own: RowId,
    surcharge: &Surcharge,
    filter: &str,
    limit: usize,
) -> Vec<RowId> {
    let needle = filter.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    lv.positions()
        .filter(|position| candidate(lv, own, position.id()))
        .filter(|position| {
            !surcharge
                .references
                .iter()
                .any(|r| r.position == position.id())
        })
        .filter(|position| {
            normalize_ordinal(position.ordinal()).contains(&normalize_ordinal(&needle))
                || position.short_text().to_lowercase().contains(&needle)
        })
        .take(limit)
        .map(|position| position.id())
        .collect()
}

/// Ordinary positions from the row with OZ `from` to the row with OZ `to`
/// (either order). Err names an OZ the LV does not have.
pub(super) fn range(lv: &Lv, own: RowId, from: &str, to: &str) -> Result<Vec<RowId>, String> {
    let index = |oz: &str| {
        let wanted = normalize_ordinal(oz.trim());
        lv.rows()
            .iter()
            .position(|row| row.has_ordinal() && normalize_ordinal(row.ordinal()) == wanted)
            .ok_or_else(|| format!("Die OZ „{}“ gibt es im LV nicht.", oz.trim()))
    };
    let (a, b) = (index(from)?, index(to)?);
    let (start, end) = (a.min(b), a.max(b));
    // A title as end takes its whole block.
    let end = lv.block_range(end).end;
    Ok(lv.rows()[start..end]
        .iter()
        .filter_map(LvRow::as_position)
        .map(|position| position.id())
        .filter(|id| candidate(lv, own, *id))
        .collect())
}

/// `surcharge` with `ids` added at the end (each once).
pub(super) fn with_added(surcharge: &Surcharge, ids: &[RowId]) -> Surcharge {
    let mut surcharge = surcharge.clone();
    for id in ids {
        if !surcharge
            .references
            .iter()
            .any(|reference| reference.position == *id)
        {
            surcharge.references.push(SurchargeReference {
                position: *id,
                quantity: None,
            });
        }
    }
    surcharge
}

impl LvEditor {
    /// Zuschlag of the position at `index` (only for a Zuschlagsposition).
    pub(super) fn surcharge_references(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        input: &mut SurchargeInput,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let Some(surcharge) = position.surcharge().cloned() else {
            return;
        };
        let own = position.id();
        let lv = project.lv();
        let currency = lv.currency().to_string();
        let amount = |money| proava_domain::price_check::amount(money, &currency);
        let value = lv.surcharge_values().get(&own).copied().unwrap_or_default();
        let mut changed: Option<Surcharge> = None;

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let word = if surcharge.split {
                "Aufteilung"
            } else {
                "Zuschlag"
            };
            ui.label(RichText::new(word).size(12.0).color(palette.text_dim));
            let mut kind = surcharge.kind;
            egui::ComboBox::from_id_salt(("window-surcharge-kind", own))
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for option in [
                        SurchargeKind::AllInList,
                        SurchargeKind::Marked,
                        SurchargeKind::Listed,
                    ] {
                        ui.selectable_value(&mut kind, option, option.label());
                    }
                });
            if kind != surcharge.kind {
                changed = Some(Surcharge {
                    // An Aufteilungszeile stays one with another Bezug.
                    split: surcharge.split,
                    kind,
                    references: surcharge.references.clone(),
                });
            }
            ui.label(
                RichText::new(format!(
                    "Bezugssumme {} · {word} {}",
                    amount(value.base),
                    amount(value.amount)
                ))
                .size(12.5)
                .color(palette.text),
            );
        });
        let hint = match surcharge.kind {
            SurchargeKind::AllInList => {
                "Bezug: alle Positionen der Liste davor (bis zum Titel bzw. zum vorigen Zuschlag)."
            }
            SurchargeKind::Marked => {
                "Bezug: die Positionen davor mit „zu bezuschlagen“ (Reiter Weiteres)."
            }
            SurchargeKind::Listed => {
                "Bezug: die Positionen unten, je mit ganzer Menge oder Teilmenge."
            }
        };
        ui.label(RichText::new(hint).size(12.0).color(palette.text_dim));

        if surcharge.kind == SurchargeKind::Listed {
            ui.add_space(4.0);
            let mut remove = None;
            let mut quantities: Vec<(usize, Option<Decimal>)> = Vec::new();
            egui::Grid::new(("window-surcharge-refs", own))
                .num_columns(4)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for (number, reference) in surcharge.references.iter().enumerate() {
                        let Some(listed) = project.lv().position_by_id(reference.position) else {
                            continue;
                        };
                        ui.label(RichText::new(listed.ordinal()).monospace());
                        ui.label(crate::format::one_line(listed.short_text()));
                        let text = input.parts.entry(reference.position).or_insert_with(|| {
                            reference
                                .quantity
                                .map(crate::numbers::quantity_text)
                                .unwrap_or_default()
                        });
                        let response = ui.add(
                            egui::TextEdit::singleline(text)
                                .hint_text(format!("ganz ({})", listed.unit()))
                                .desired_width(110.0),
                        );
                        if response.lost_focus() {
                            let trimmed = text.trim();
                            let parsed = if trimmed.is_empty() {
                                Ok(None)
                            } else {
                                proava_domain::parse_decimal_de(trimmed).map(Some)
                            };
                            match parsed {
                                Ok(quantity) if quantity != reference.quantity => {
                                    quantities.push((number, quantity));
                                }
                                Ok(_) => {}
                                Err(_) => {
                                    input.error =
                                        Some(format!("Teilmenge „{trimmed}“ ist keine Zahl."))
                                }
                            }
                        }
                        if ui.small_button("Entfernen").clicked() {
                            remove = Some(number);
                        }
                        ui.end_row();
                    }
                });
            if surcharge.references.is_empty() {
                ui.label(
                    RichText::new(
                        "Noch keine Position – unten wählen (GAEB verlangt mindestens eine).",
                    )
                    .size(12.0)
                    .color(palette.warn),
                );
            }
            if !quantities.is_empty() || remove.is_some() {
                let mut next = surcharge.clone();
                for (number, quantity) in quantities {
                    next.references[number].quantity = quantity;
                }
                if let Some(number) = remove {
                    let gone = next.references.remove(number);
                    input.parts.remove(&gone.position);
                }
                changed = Some(next);
            }

            // Add: search, range, marked rows.
            ui.add_space(4.0);
            let lv = project.lv();
            let mut add: Vec<RowId> = Vec::new();
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Position suchen")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut input.filter)
                        .hint_text("OZ oder Kurztext")
                        .desired_width(180.0),
                );
            });
            let found = search(lv, own, &surcharge, &input.filter, 8);
            if !found.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for id in found {
                        let Some(listed) = lv.position_by_id(id) else {
                            continue;
                        };
                        let label = format!(
                            "{} {}",
                            listed.ordinal(),
                            crate::format::one_line(listed.short_text())
                                .chars()
                                .take(30)
                                .collect::<String>()
                        );
                        if ui
                            .button(label)
                            .on_hover_text("Zum Bezug hinzufügen")
                            .clicked()
                        {
                            add.push(id);
                        }
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Bereich von").size(12.0).color(palette.text_dim));
                ui.add(egui::TextEdit::singleline(&mut input.from).hint_text("OZ").desired_width(110.0));
                ui.label(RichText::new("bis").size(12.0).color(palette.text_dim));
                ui.add(egui::TextEdit::singleline(&mut input.to).hint_text("OZ").desired_width(110.0));
                if ui
                    .button("Bereich übernehmen")
                    .on_hover_text("Alle Positionen dazwischen (ohne Zuschläge); ein Titel als Ende mit seinem Block")
                    .clicked()
                {
                    match range(lv, own, &input.from, &input.to) {
                        Ok(ids) => {
                            input.error = None;
                            add.extend(ids);
                        }
                        Err(error) => input.error = Some(error),
                    }
                }
                let marked: Vec<RowId> = lv
                    .positions()
                    .map(|position| position.id())
                    .filter(|id| self.marked.contains(id) && candidate(lv, own, *id))
                    .collect();
                if ui
                    .add_enabled(
                        !marked.is_empty(),
                        egui::Button::new(format!("Markierte Zeilen übernehmen ({})", marked.len())),
                    )
                    .on_hover_text(
                        "Positionen, die in der Tabelle markiert sind (Strg+Klick); „Festhalten“ hält dieses Fenster dabei fest",
                    )
                    .clicked()
                {
                    add.extend(marked);
                }
            });
            if !add.is_empty() {
                changed = Some(with_added(changed.as_ref().unwrap_or(&surcharge), &add));
            }
            if let Some(error) = &input.error {
                ui.label(RichText::new(error).size(12.0).color(palette.warn));
            }
        }

        if let Some(next) = changed {
            self.undo.record(
                project.lv(),
                Some(EditKey(format!("surcharge-refs:{}", own.0))),
            );
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_surcharge(Some(next));
            }
            output.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Surcharge, SurchargeKind};

    use super::{range, search, with_added};

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Rohbau"));
        lv.push_group(LvGroup::new(1, "01.01", "Erde"));
        lv.push_position(Position::new(
            "01.01.0010",
            "Aushub",
            Decimal::TEN,
            "m3",
            None,
        ));
        lv.push_position(Position::new(
            "01.01.0020",
            "Verfüllen",
            Decimal::TEN,
            "m3",
            None,
        ));
        lv.push_group(LvGroup::new(1, "01.02", "Mauer"));
        lv.push_position(Position::new(
            "01.02.0010",
            "Mauerwerk",
            Decimal::TEN,
            "m2",
            None,
        ));
        let mut zuschlag = Position::new("01.02.0020", "Zuschlag", Decimal::ZERO, "", None);
        zuschlag.set_surcharge(Some(Surcharge {
            split: false,
            kind: SurchargeKind::Listed,
            references: Vec::new(),
        }));
        lv.push_position(zuschlag);
        lv
    }

    #[test]
    fn positions_are_found_ranged_and_added_once() {
        let lv = lv();
        let own = lv.position_by_ordinal("01.02.0020").unwrap().id();
        let surcharge = lv.position_by_id(own).unwrap().surcharge().unwrap().clone();
        let id = |oz: &str| lv.position_by_ordinal(oz).unwrap().id();
        // search by text and by OZ, never the Zuschlag itself
        assert_eq!(
            search(&lv, own, &surcharge, "füll", 8),
            vec![id("01.01.0020")]
        );
        assert_eq!(
            search(&lv, own, &surcharge, "1.02", 8),
            vec![id("01.02.0010")]
        );
        assert!(search(&lv, own, &surcharge, "Zuschlag", 8).is_empty());
        // range in any order; a title as end takes its block
        assert_eq!(
            range(&lv, own, "01.01.0020", "01.01.0010").unwrap(),
            vec![id("01.01.0010"), id("01.01.0020")]
        );
        assert_eq!(
            range(&lv, own, "01.01.0010", "01.02").unwrap(),
            vec![id("01.01.0010"), id("01.01.0020"), id("01.02.0010")]
        );
        assert!(range(&lv, own, "09", "01.01.0010")
            .unwrap_err()
            .contains("„09“"));
        // added once, in order
        let added = with_added(
            &surcharge,
            &[id("01.01.0010"), id("01.01.0010"), id("01.02.0010")],
        );
        assert_eq!(added.references.len(), 2);
        let again = with_added(&added, &[id("01.01.0010")]);
        assert_eq!(again.references.len(), 2);
        // listed ones are no longer offered by the search
        assert!(search(&lv, own, &added, "Aushub", 8).is_empty());
    }
}
