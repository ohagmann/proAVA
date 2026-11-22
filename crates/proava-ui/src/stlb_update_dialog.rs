//! „Aktualisierte STLB-Bau-Positionen einlesen“: shows which STLB-Bau
//! positions of the LV get a new text from the file STLB-Bau wrote (matched
//! by OZ or Leistung, see `proava_domain::stlb_update`) with the differences
//! of the LV-Vergleich; the chosen ones are taken over.

use eframe::egui::{self, RichText};
use proava_domain::lv_diff::Aspect;
use proava_domain::stlb_update::{StlbUpdate, UpdatePlan};

use crate::theme::Palette;
use crate::widgets;

pub enum StlbUpdateResult {
    Open,
    Cancelled,
    /// Indices into the plan's updates.
    Apply(Vec<usize>),
}

pub struct StlbUpdateDialog {
    file: String,
    plan: UpdatePlan,
    chosen: Vec<bool>,
    /// Update whose differences are shown.
    shown: Option<usize>,
    warnings: Vec<String>,
}

impl StlbUpdateDialog {
    pub fn new(file: String, plan: UpdatePlan, warnings: Vec<String>) -> Self {
        let chosen = plan.updates.iter().map(|update| !update.locked).collect();
        Self {
            file,
            shown: (!plan.updates.is_empty()).then_some(0),
            plan,
            chosen,
            warnings,
        }
    }

    pub fn plan(&self) -> &UpdatePlan {
        &self.plan
    }

    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> StlbUpdateResult {
        let mut result = StlbUpdateResult::Open;
        let (_, close) = widgets::modal(ui, palette, 860.0, |ui| {
            widgets::title(
                ui,
                palette,
                "STLB-Bau-Positionen aktualisieren",
                "Texte aus der Datei von STLB-Bau. Zuordnung über die OZ, sonst über die Leistung (LB und Gruppe). Übernommen werden Kurz- und Langtext, Einheit, Textergänzungen und die Kennung; OZ, Menge und Preise bleiben, der Status wird offen.",
            );
            ui.label(
                RichText::new(format!(
                    "Datei: {} – {} geändert, {} unverändert, {} ohne Partner im LV",
                    self.file,
                    self.plan.updates.len(),
                    self.plan.unchanged,
                    self.plan.unmatched.len()
                ))
                .size(12.5)
                .color(palette.text),
            );
            for warning in &self.warnings {
                ui.label(RichText::new(warning).size(12.0).color(palette.warn));
            }
            if !self.plan.unmatched.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Ohne Partner im LV: {}",
                        self.plan.unmatched.join("; ")
                    ))
                    .size(12.0)
                    .color(palette.warn),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.small_button("Alle").clicked() {
                    for (chosen, update) in self.chosen.iter_mut().zip(&self.plan.updates) {
                        *chosen = !update.locked;
                    }
                }
                if ui.small_button("Keine").clicked() {
                    self.chosen.iter_mut().for_each(|chosen| *chosen = false);
                }
            });
            self.list(ui, palette);
            ui.add_space(6.0);
            if let Some(update) = self.shown.and_then(|index| self.plan.updates.get(index)) {
                differences(ui, palette, update);
            }
            ui.add_space(8.0);
            let count = self.chosen.iter().filter(|chosen| **chosen).count();
            let (cancelled, confirmed) =
                widgets::dialog_buttons(ui, palette, &format!("Übernehmen ({count})"));
            if cancelled {
                result = StlbUpdateResult::Cancelled;
            } else if confirmed {
                let chosen: Vec<usize> = self
                    .chosen
                    .iter()
                    .enumerate()
                    .filter(|(_, chosen)| **chosen)
                    .map(|(index, _)| index)
                    .collect();
                if !chosen.is_empty() {
                    result = StlbUpdateResult::Apply(chosen);
                }
            }
        });
        if close {
            StlbUpdateResult::Cancelled
        } else {
            result
        }
    }

    fn list(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("stlb_update_list")
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("stlb_updates")
                            .num_columns(5)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                let head = |text: &str| {
                                    RichText::new(text)
                                        .size(12.0)
                                        .strong()
                                        .color(palette.text_dim)
                                };
                                ui.label("");
                                ui.label(head("OZ"));
                                ui.label(head("Kurztext"));
                                ui.label(head("Änderung"));
                                ui.label(head("Zuordnung"));
                                ui.end_row();
                                for (index, update) in self.plan.updates.iter().enumerate() {
                                    let chosen = &mut self.chosen[index];
                                    ui.add_enabled_ui(!update.locked, |ui| ui.checkbox(chosen, ""))
                                        .inner
                                        .on_disabled_hover_text("Position ist fertig (gesperrt).");
                                    let selected = self.shown == Some(index);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            RichText::new(&update.ordinal).monospace(),
                                        )
                                        .on_hover_text("Unterschiede zeigen")
                                        .clicked()
                                    {
                                        self.shown = Some(index);
                                    }
                                    ui.label(RichText::new(&update.short_text).color(palette.text));
                                    ui.label(
                                        RichText::new(change_summary(update))
                                            .size(12.0)
                                            .color(palette.text_dim),
                                    );
                                    let how = if update.locked {
                                        format!("{}, fertig", update.matched_by.label())
                                    } else {
                                        update.matched_by.label().to_string()
                                    };
                                    ui.label(RichText::new(how).size(12.0).color(palette.text_dim));
                                    ui.end_row();
                                }
                            });
                    });
            });
    }
}

/// „Langtext, Textergänzungen, Version 10/2025 → 4/2026“
pub fn change_summary(update: &StlbUpdate) -> String {
    let mut parts: Vec<String> = update
        .details
        .iter()
        .map(|detail| detail.aspect.label().to_string())
        .collect();
    if update.catalog.0 != update.catalog.1 {
        parts.push(format!(
            "Kennung {} → {}",
            update.catalog.0, update.catalog.1
        ));
    } else if parts.is_empty() {
        parts.push("Kennung".to_string());
    }
    parts.join(", ")
}

fn differences(ui: &mut egui::Ui, palette: &Palette, update: &StlbUpdate) {
    ui.label(
        RichText::new(format!(
            "Unterschiede {} {}",
            update.ordinal, update.short_text
        ))
        .size(12.5)
        .strong()
        .color(palette.text),
    );
    egui::ScrollArea::vertical()
        .id_salt("stlb_update_details")
        .max_height(180.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for detail in &update.details {
                let (old, new) = if detail.aspect == Aspect::LongText {
                    ("entfällt", "neu")
                } else {
                    ("bisher", "neu")
                };
                ui.label(
                    RichText::new(detail.aspect.label())
                        .size(12.0)
                        .strong()
                        .color(palette.text_dim),
                );
                if !detail.old.trim().is_empty() {
                    ui.label(
                        RichText::new(format!("{old}: {}", detail.old))
                            .size(12.0)
                            .color(palette.danger),
                    );
                }
                if !detail.new.trim().is_empty() {
                    ui.label(
                        RichText::new(format!("{new}: {}", detail.new))
                            .size(12.0)
                            .color(palette.accent),
                    );
                }
            }
            if update.catalog.0 != update.catalog.1 {
                ui.label(
                    RichText::new(format!(
                        "Kennung: {} → {}",
                        update.catalog.0, update.catalog.1
                    ))
                    .size(12.0)
                    .color(palette.text),
                );
            }
        });
}
