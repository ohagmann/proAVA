//! The bar for Suchen und Ersetzen above the LV table (Strg+R). It only
//! collects input; the editor runs the actions with [`crate::search_replace`].

use std::collections::HashSet;

use eframe::egui::{self, RichText};
use proava_domain::RowId;

use crate::search_replace::{Scope, SearchOptions};
use crate::theme::Palette;

/// What the user asked for in this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarAction {
    Next,
    Replace,
    ReplaceAll,
    Close,
}

#[derive(Debug, Clone, Default)]
pub struct FindReplace {
    pub find: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub scope: Scope,
    /// Rows chosen when „nur Auswahl“ was switched on; None = whole LV.
    pub only: Option<HashSet<RowId>>,
    focus_find: bool,
}

impl FindReplace {
    pub fn new(find: String) -> Self {
        Self {
            find,
            focus_find: true,
            ..Self::default()
        }
    }

    pub fn focus(&mut self) {
        self.focus_find = true;
    }

    pub fn options(&self) -> SearchOptions {
        SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            scope: self.scope,
            only: self.only.clone(),
        }
    }

    /// Draws the bar. `selection` is offered for „nur Auswahl“, `hit_rows`
    /// is shown as the number of rows with hits.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        selection: &HashSet<RowId>,
        hit_rows: usize,
    ) -> Option<BarAction> {
        let mut action = None;
        let (enter, escape) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        ui.horizontal(|ui| {
            crate::widgets::search_mark(ui, palette);
            let find = ui.add(
                egui::TextEdit::singleline(&mut self.find)
                    .id(egui::Id::unique("lv_replace_find"))
                    .hint_text("Suchen …")
                    .desired_width(220.0),
            );
            if std::mem::take(&mut self.focus_find) {
                find.request_focus();
            }
            if find.lost_focus() && enter {
                action = Some(BarAction::Next);
                find.request_focus();
            }
            let replace = ui.add(
                egui::TextEdit::singleline(&mut self.replacement)
                    .id(egui::Id::unique("lv_replace_with"))
                    .hint_text("Ersetzen durch …")
                    .desired_width(220.0),
            );
            if replace.lost_focus() && enter {
                action = Some(BarAction::Replace);
                replace.request_focus();
            }
            if ui
                .button("Weitersuchen")
                .on_hover_text("F3 oder Enter im Suchfeld")
                .clicked()
            {
                action = Some(BarAction::Next);
            }
            if ui
                .button("Ersetzen")
                .on_hover_text("Ersetzt in der gewählten Zeile und springt weiter (Enter im Ersetzen-Feld)")
                .clicked()
            {
                action = Some(BarAction::Replace);
            }
            if ui
                .button("Alle ersetzen")
                .on_hover_text("Ein Schritt – Strg+Z nimmt alles zurück. Fertige Positionen bleiben unverändert.")
                .clicked()
            {
                action = Some(BarAction::ReplaceAll);
            }
            if ui.button("Schließen").on_hover_text("Esc").clicked() {
                action = Some(BarAction::Close);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.checkbox(&mut self.case_sensitive, "Groß/klein");
            ui.checkbox(&mut self.whole_word, "Ganzes Wort");
            ui.separator();
            ui.label(RichText::new("in").size(12.5).color(palette.text_dim));
            ui.checkbox(&mut self.scope.short, "Kurztext");
            ui.checkbox(&mut self.scope.long, "Langtext");
            ui.checkbox(&mut self.scope.notes, "Hinweise");
            ui.checkbox(&mut self.scope.performances, "Ausführungsbeschr.");
            ui.checkbox(&mut self.scope.unit, "Einheit");
            ui.separator();
            let mut only = self.only.is_some();
            let label = match &self.only {
                Some(rows) => format!(
                    "nur Auswahl ({})",
                    crate::format::count(rows.len(), "Zeile", "Zeilen")
                ),
                None => "nur Auswahl".to_string(),
            };
            if ui
                .checkbox(&mut only, label)
                .on_hover_text(
                    "Merkt sich die beim Einschalten gewählten Zeilen (Titel samt Inhalt).",
                )
                .changed()
            {
                self.only = only.then(|| selection.clone());
            }
            ui.separator();
            let (text, color) = if self.find.is_empty() {
                (String::new(), palette.text_dim)
            } else if self.scope.is_empty() {
                ("kein Bereich gewählt".to_string(), palette.error)
            } else if hit_rows == 0 {
                ("keine Treffer".to_string(), palette.error)
            } else {
                (
                    format!(
                        "{} mit Treffern",
                        crate::format::count(hit_rows, "Zeile", "Zeilen")
                    ),
                    palette.hit,
                )
            };
            ui.label(RichText::new(text).size(12.5).color(color));
        });
        if escape && action.is_none() {
            action = Some(BarAction::Close);
        }
        action
    }
}
