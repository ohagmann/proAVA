//! Gliederung und OZ-Maske as a list of levels (Assistent): Bezeichnung,
//! Stellen, Ziffern oder Buchstaben je Stufe, dann Position, Index und
//! Schrittweite, with a live example and the GAEB limits. On a filled LV it
//! says what happens to the numbers and lets the user choose.

use eframe::egui::{self, RichText};
use proava_domain::mask_spec::{
    self, MAX_LEVELS, MAX_LEVEL_WIDTH, MAX_OZ_LENGTH, MAX_POSITION_WIDTH,
};
use proava_domain::{LevelSpec, Lv, LvRow, MaskSpec};

use crate::icons::Icon;
use crate::struct_ops::{self, MaskImpact, MaskNumbering};
use crate::theme::Palette;
use crate::widgets;

pub struct OzMaskDialog {
    spec: MaskSpec,
    step: u32,
    /// The user's choice; until then the suggestion of [`MaskImpact::suggested`].
    numbering: Option<MaskNumbering>,
    /// X83 already sent, bids read – shown when numbers change.
    warnings: Vec<String>,
    /// Level numbers of the example, taken from the LV (01.481.10 …).
    sample: Vec<u32>,
    error: Option<String>,
}

pub enum MaskDialogResult {
    Open,
    Cancelled,
    /// Applied to the LV; the message for the status line.
    Applied(String),
}

impl OzMaskDialog {
    pub fn of(lv: &Lv, bids: usize) -> Self {
        let spec = MaskSpec::of(lv.oz_mask(), lv.level_labels());
        let sample = sample_numbers(lv);
        Self {
            spec,
            step: lv.position_step(),
            numbering: None,
            warnings: struct_ops::renumber_warnings(lv, bids),
            sample,
            error: None,
        }
    }

    /// Shows the dialog; `apply` gets the LV only when the user confirms.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        lv: &Lv,
        apply: impl FnOnce(&MaskSpec, u32, MaskNumbering) -> Result<String, String>,
    ) -> MaskDialogResult {
        let mut problems = self.spec.problems();
        let mask = self.spec.to_mask().ok();
        if let Some(problem) = mask.as_ref().and_then(|mask| mask.step_problem(self.step)) {
            problems.push(problem);
        }
        let impact = mask.as_ref().map(|mask| struct_ops::mask_impact(lv, mask));
        let changes_numbers = impact
            .as_ref()
            .is_some_and(|impact| !impact.same_mask || self.step != lv.position_step());
        if let Some(impact) = &impact {
            if self.numbering == Some(MaskNumbering::KeepTitles) && impact.keep_titles.is_err() {
                self.numbering = None;
            }
        }
        let numbering = match (&impact, self.numbering) {
            (_, Some(chosen)) => chosen,
            (Some(impact), None) => impact.suggested(),
            (None, None) => MaskNumbering::All,
        };
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 640.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Gliederung und Ordnungszahl",
                "Stufen vor der Position (Los, Titel, Untertitel …) mit Bezeichnung und Stellen. \
                 GAEB: höchstens 5 Stufen, zusammen höchstens 14 Stellen.",
            );
            self.levels(ui, palette);
            ui.add_space(10.0);
            self.position_part(ui, palette);
            ui.add_space(12.0);
            // Vorschau
            let example = self.spec.example(&self.sample, self.step);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Beispiel").size(12.0).color(palette.text_dim));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(example)
                        .monospace()
                        .size(18.0)
                        .strong()
                        .color(palette.text),
                );
            });
            ui.label(
                RichText::new(format!(
                    "Maske {}  ·  {} von {MAX_OZ_LENGTH} Stellen",
                    self.spec.pattern(),
                    self.spec.total_length()
                ))
                .size(12.0)
                .color(if self.spec.total_length() > MAX_OZ_LENGTH {
                    palette.error
                } else {
                    palette.text_dim
                }),
            );
            // Feste Höhe: der Dialog springt nicht, wenn Hinweise kommen und gehen.
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("oz_mask_notes")
                .min_scrolled_height(210.0)
                .max_height(210.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_height(210.0);
                    for problem in &problems {
                        ui.label(RichText::new(problem).size(12.5).color(palette.error));
                    }
                    if let Some(impact) = impact.as_ref().filter(|_| changes_numbers) {
                        for warning in &self.warnings {
                            ui.label(RichText::new(warning).size(12.5).color(palette.warn));
                        }
                        ui.add_space(4.0);
                        self.impact(ui, palette, impact, numbering);
                    } else if problems.is_empty() {
                        ui.label(
                            RichText::new("Die Ordnungszahlen bleiben, wie sie sind.")
                                .size(12.5)
                                .color(palette.text_dim),
                        );
                    }
                });
            ui.add_space(10.0);
            widgets::error_line(ui, palette, self.error.as_deref());
            // Mit Fehlern bleibt „Übernehmen“ ohne Wirkung; die Liste oben sagt, warum.
            widgets::dialog_buttons(ui, palette, "Übernehmen")
        });
        if close || cancelled {
            return MaskDialogResult::Cancelled;
        }
        if confirmed && problems.is_empty() {
            let numbering = if changes_numbers {
                numbering
            } else {
                MaskNumbering::Unchanged
            };
            return match apply(&self.spec, self.step, numbering) {
                Ok(message) => MaskDialogResult::Applied(message),
                Err(error) => {
                    self.error = Some(error);
                    MaskDialogResult::Open
                }
            };
        }
        MaskDialogResult::Open
    }

    fn levels(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let mut lot = self.spec.has_lot();
        if ui
            .checkbox(&mut lot, "Los als erste Stufe")
            .on_hover_text("GAEB: ein Los ist immer die äußerste Stufe.")
            .changed()
        {
            if lot {
                self.spec
                    .levels
                    .insert(0, LevelSpec::digits(mask_spec::LOT_LABEL, 1));
                self.sample.insert(0, 1);
            } else if self.spec.has_lot() {
                self.spec.levels.remove(0);
                if !self.sample.is_empty() {
                    self.sample.remove(0);
                }
            }
        }
        ui.add_space(6.0);
        // Tab/Umschalt+Tab in a „Bezeichnung“ go to the next/previous one
        // (not over the buttons of the row in between).
        let label_id = |index: usize| egui::Id::unique(("level_label", index));
        let editable: Vec<usize> = (0..self.spec.levels.len())
            .filter(|&index| {
                !(index == 0 && mask_spec::is_lot_label(&self.spec.levels[index].label))
            })
            .collect();
        if let Some(at) = editable
            .iter()
            .position(|&index| ui.memory(|memory| memory.has_focus(label_id(index))))
        {
            let target = ui.input_mut(|input| {
                if input.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                    at.checked_sub(1)
                } else if editable.get(at + 1).is_some()
                    && input.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
                {
                    Some(at + 1)
                } else {
                    None
                }
            });
            if let Some(target) = target.and_then(|target| editable.get(target)) {
                ui.memory_mut(|memory| {
                    // egui would move on from the field in this frame as well.
                    memory.move_focus(egui::FocusDirection::None);
                    memory.request_focus(label_id(*target));
                });
            }
        }
        let mut remove = None;
        let mut swap = None;
        let count = self.spec.levels.len();
        egui::Grid::new("oz_mask_levels")
            .num_columns(5)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                for header in ["Stufe", "Bezeichnung", "Stellen", "Art", ""] {
                    ui.label(RichText::new(header).size(12.0).color(palette.text_dim));
                }
                ui.end_row();
                for (index, level) in self.spec.levels.iter_mut().enumerate() {
                    let is_lot = index == 0 && mask_spec::is_lot_label(&level.label);
                    ui.label(RichText::new(format!("{}", index + 1)).monospace());
                    ui.add_enabled_ui(!is_lot, |ui| {
                        ui.add_sized(
                            [200.0, 24.0],
                            egui::TextEdit::singleline(&mut level.label)
                                .id(egui::Id::unique(("level_label", index))),
                        );
                    });
                    ui.horizontal(|ui| {
                        for width in 1..=MAX_LEVEL_WIDTH {
                            if ui
                                .selectable_label(level.width == width, width.to_string())
                                .clicked()
                            {
                                level.width = width;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!level.letters, "Ziffern").clicked() {
                            level.letters = false;
                        }
                        if ui
                            .selectable_label(level.letters, "Buchstaben")
                            .on_hover_text("A, B, C … (wird selten gebraucht)")
                            .clicked()
                        {
                            level.letters = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled_ui(index > 0 && !is_lot, |ui| {
                                widgets::tool_button(
                                    ui,
                                    palette,
                                    Icon::Up,
                                    false,
                                    "Stufe nach oben",
                                )
                            })
                            .inner
                            .clicked()
                        {
                            swap = Some(index - 1);
                        }
                        if ui
                            .add_enabled_ui(index + 1 < count, |ui| {
                                widgets::tool_button(
                                    ui,
                                    palette,
                                    Icon::Down,
                                    false,
                                    "Stufe nach unten",
                                )
                            })
                            .inner
                            .clicked()
                        {
                            swap = Some(index);
                        }
                        if ui
                            .add(egui::Button::new("Entfernen").small())
                            .on_hover_text("Stufe entfernen")
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(index) = swap {
            if index + 1 < self.spec.levels.len() {
                self.spec.levels.swap(index, index + 1);
                if index + 1 < self.sample.len() {
                    self.sample.swap(index, index + 1);
                }
            }
        }
        if let Some(index) = remove {
            self.spec.levels.remove(index);
            if index < self.sample.len() {
                self.sample.remove(index);
            }
        }
        ui.add_space(4.0);
        let room = self.spec.levels.len() < MAX_LEVELS;
        if ui
            .add_enabled(room, egui::Button::new("+ Stufe"))
            .on_hover_text(if room {
                "Eine weitere Stufe vor der Position"
            } else {
                "GAEB erlaubt höchstens 5 Stufen."
            })
            .clicked()
        {
            let level = self.spec.levels.len();
            self.spec
                .levels
                .push(LevelSpec::digits(mask_spec::default_level_label(level), 2));
        }
    }

    fn position_part(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        egui::Grid::new("oz_mask_position")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Positionsstellen")
                        .size(12.5)
                        .color(palette.text_dim),
                );
                ui.horizontal(|ui| {
                    for width in 1..=MAX_POSITION_WIDTH {
                        if ui
                            .selectable_label(self.spec.position_width == width, width.to_string())
                            .clicked()
                        {
                            self.spec.position_width = width;
                        }
                    }
                });
                ui.end_row();
                ui.label(RichText::new("Index").size(12.5).color(palette.text_dim));
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.spec.index_width == 0, "keiner")
                        .clicked()
                    {
                        self.spec.index_width = 0;
                    }
                    if ui
                        .selectable_label(self.spec.index_width == 1, "1 Stelle")
                        .clicked()
                    {
                        self.spec.index_width = 1;
                    }
                });
                ui.end_row();
                ui.label(
                    RichText::new("Schrittweite")
                        .size(12.5)
                        .color(palette.text_dim),
                );
                ui.horizontal(|ui| {
                    for value in [1u32, 10, 100, 1000] {
                        if ui
                            .selectable_label(self.step == value, value.to_string())
                            .clicked()
                        {
                            self.step = value;
                        }
                    }
                });
                ui.end_row();
            });
    }

    fn impact(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        impact: &MaskImpact,
        current: MaskNumbering,
    ) {
        if let Some(blocked) = &impact.blocked {
            ui.label(RichText::new(blocked).size(12.5).color(palette.error));
            return;
        }
        if impact.numbered_rows == 0 {
            return;
        }
        egui::Frame::new()
            .fill(palette.warn.gamma_multiply(0.12))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Das LV hat {} Zeilen mit Ordnungszahl{}.",
                        impact.numbered_rows,
                        if impact.own_numbers { " und eigene Nummern" } else { "" }
                    ))
                    .size(12.5)
                    .color(palette.text),
                );
                if impact.lifted_titles > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} Titel stehen auf Stufen, die es dann nicht mehr gibt – sie rücken auf die \
                             unterste Stufe; gelöscht wird nichts.",
                            impact.lifted_titles
                        ))
                        .size(12.5)
                        .color(palette.warn),
                    );
                }
                ui.add_space(4.0);
                let mut choice = current;
                ui.radio_value(&mut choice, MaskNumbering::All, "Alles neu nummerieren");
                ui.add_enabled_ui(impact.keep_titles.is_ok(), |ui| {
                    let response = ui.radio_value(
                        &mut choice,
                        MaskNumbering::KeepTitles,
                        "Titelnummern behalten (481 …), nur Positionen neu nummerieren",
                    );
                    if let Err(reason) = &impact.keep_titles {
                        response.on_disabled_hover_text(reason.as_str());
                    }
                });
                ui.radio_value(
                    &mut choice,
                    MaskNumbering::Unchanged,
                    "Nummern unverändert lassen (LV prüfen zeigt, was nicht passt)",
                );
                if choice != current {
                    self.numbering = Some(choice);
                }
            });
    }
}

/// Level numbers of the first heading on the deepest level, for the example
/// (01.481.10 → [1, 481, 10]); ones when the LV has none.
fn sample_numbers(lv: &Lv) -> Vec<u32> {
    let deepest = lv.level_count().saturating_sub(1);
    lv.rows()
        .iter()
        .filter_map(LvRow::as_group)
        .find(|group| group.level() == deepest)
        .and_then(|group| lv.oz_mask().read_group(group.ordinal()))
        .unwrap_or_else(|| vec![1; lv.level_count()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{LvGroup, OzMask};

    #[test]
    fn example_numbers_come_from_the_lv() {
        let mut lv = Lv::new("x", OzMask::parse("11.222.33.PPPP").unwrap());
        assert_eq!(sample_numbers(&lv), vec![1, 1, 1]);
        lv.push_group(LvGroup::new(0, "01", "a"));
        lv.push_group(LvGroup::new(1, "01.481", "b"));
        lv.push_group(LvGroup::new(2, "01.481.10", "c"));
        assert_eq!(sample_numbers(&lv), vec![1, 481, 10]);
        let dialog = OzMaskDialog::of(&lv, 0);
        assert_eq!(dialog.spec.example(&dialog.sample, 10), "01.481.10.0010");
        assert_eq!(dialog.numbering, None);
        let mut wider = dialog.spec.clone();
        wider.levels[2].width = 3;
        let impact = struct_ops::mask_impact(&lv, &wider.to_mask().unwrap());
        assert_eq!(impact.suggested(), MaskNumbering::KeepTitles);
    }
}
