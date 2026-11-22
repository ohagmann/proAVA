//! Library data of a position in the edit panel: in a library (opened with
//! „Bibliothek bearbeiten …“) Stichwörter, Stand and the date of the
//! Erfahrungspreis; in an ordinary LV the origin of a position taken from a
//! library.

use eframe::egui::{self, FontId, RichText};
use proava_domain::library::EntryInfo;
use proava_domain::Project;

use super::fields::{self, caption};
use super::{EditorOutput, LvEditor};
use crate::theme::Palette;

impl LvEditor {
    /// Shows the library fields (library mode) or the origin line.
    pub(super) fn library_fields(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        if !self.library_mode {
            if let Some(origin) = position.origin() {
                let as_of = if origin.as_of.trim().is_empty() {
                    String::new()
                } else {
                    format!(", Stand {}", origin.as_of.trim())
                };
                ui.label(
                    RichText::new(format!(
                        "Aus der Bibliothek „{}“{as_of} – abgleichen mit „Mit Bibliothek abgleichen …“ (Menü LV, Bibliothek)",
                        origin.library
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(6.0);
            }
            return;
        }
        let info = position.library_info().clone();
        let id = position.id();
        let width = ui.available_width();
        egui::CollapsingHeader::new(RichText::new("Bibliothek").size(13.0))
            .id_salt("library_fields")
            .default_open(true)
            .show(ui, |ui| {
                caption(ui, palette, "Stichwörter (mit Komma getrennt)");
                let field = egui::Id::unique(("library_keywords", id));
                let checked = fields::checked_field(
                    ui,
                    palette,
                    field,
                    &info.keywords_text(),
                    width,
                    FontId::proportional(13.0),
                );
                if let Some(text) = checked.commit {
                    let keywords = EntryInfo::parse_keywords(&text);
                    self.edit_position(project, index, "keywords", |p| {
                        let mut changed = p.library_info().clone();
                        changed.keywords = keywords;
                        p.set_library_info(changed);
                    });
                    fields::accept(ui.ctx(), field);
                    output.dirty = true;
                }
                ui.add_space(6.0);
                crate::widgets::row(ui, |ui| {
                    ui.vertical(|ui| {
                        caption(ui, palette, "Stand");
                        let field = egui::Id::unique(("library_as_of", id));
                        let checked = fields::checked_field(
                            ui,
                            palette,
                            field,
                            &info.as_of,
                            110.0,
                            FontId::proportional(13.0),
                        );
                        if let Some(text) = checked.commit {
                            match german_date(&text) {
                                Ok(date) => {
                                    self.edit_position(project, index, "as_of", |p| {
                                        let mut changed = p.library_info().clone();
                                        changed.as_of = date;
                                        p.set_library_info(changed);
                                    });
                                    fields::accept(ui.ctx(), field);
                                    output.dirty = true;
                                }
                                Err(error) => fields::refuse(ui.ctx(), field, error),
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        caption(ui, palette, "Preis vom (Erfahrungspreis)");
                        let field = egui::Id::unique(("library_price_date", id));
                        let checked = fields::checked_field(
                            ui,
                            palette,
                            field,
                            &info.price_date,
                            130.0,
                            FontId::proportional(13.0),
                        );
                        if let Some(text) = checked.commit {
                            let date = text.trim().to_string();
                            self.edit_position(project, index, "price_date", |p| {
                                let mut changed = p.library_info().clone();
                                changed.price_date = date;
                                p.set_library_info(changed);
                            });
                            fields::accept(ui.ctx(), field);
                            output.dirty = true;
                        }
                    });
                });
                if ui
                    .button("Stand auf heute")
                    .on_hover_text("Nach einer Überarbeitung des Eintrags")
                    .clicked()
                {
                    let today = crate::format::iso_to_german_date(&self.today_iso);
                    self.edit_position(project, index, "as_of", |p| {
                        let mut changed = p.library_info().clone();
                        changed.as_of = today;
                        p.set_library_info(changed);
                    });
                    output.dirty = true;
                }
                ui.add_space(4.0);
            });
        ui.add_space(4.0);
    }
}

/// „1.11.2026“ → „01.11.2026“; empty stays empty.
fn german_date(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    crate::format::parse_german_date(text).map(|iso| crate::format::iso_to_german_date(&iso))
}
