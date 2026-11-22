//! „Rechtschreibung prüfen …“ for all LVs of the table (those of the
//! tender stand one below the other; a Kostenermittlung alone): unknown words with count and places;
//! replace everywhere, add to the dictionary, ignore, jump to a place.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText};
use proava_application::spelling::{lv_texts, SpellField};
use proava_domain::{Project, RowId};
use proava_spell::Speller;

use crate::theme::Palette;

struct Entry {
    word: String,
    count: usize,
    /// LV (index), row and label, each once.
    places: Vec<(usize, RowId, String, SpellField)>,
}

/// What the LV editor does next.
#[derive(Debug, Clone, PartialEq)]
pub enum SpellOutcome {
    Open,
    Close,
    /// Replace `word` by `with` in all LVs.
    Replace {
        word: String,
        with: String,
    },
    /// Select this row of the LV at this index.
    Jump(usize, RowId),
}

pub struct SpellDialog {
    entries: Vec<Entry>,
    selected: usize,
    replacement: String,
    /// Suggestions of the selected word.
    suggestions: Option<(String, Vec<String>)>,
    pub message: Option<String>,
}

impl SpellDialog {
    pub fn new(project: &Project, speller: &Speller) -> Self {
        let mut dialog = Self {
            entries: Vec::new(),
            selected: 0,
            replacement: String::new(),
            suggestions: None,
            message: None,
        };
        dialog.refresh(project, speller);
        dialog
    }

    /// Checks the LVs again (after a change), keeps the selected word if it is still there.
    pub fn refresh(&mut self, project: &Project, speller: &Speller) {
        let current = self
            .entries
            .get(self.selected)
            .map(|entry| entry.word.clone());
        let mut found: BTreeMap<String, Entry> = BTreeMap::new();
        // The LVs of the table (all of the tender, or a Kostenermittlung alone).
        let shown = project.lv_indices_shown_with(project.active_lv());
        let several = shown.len() > 1;
        for (index, lv) in shown
            .into_iter()
            .filter_map(|index| Some((index, project.lv_at(index)?)))
        {
            let lv_label = crate::lv_properties::lv_label(lv);
            for place in lv_texts(lv) {
                let label = if several {
                    format!("{lv_label} · {}", place.label)
                } else {
                    place.label.clone()
                };
                for word in speller.unknown_words(&place.text) {
                    let entry = found.entry(word.text.clone()).or_insert_with(|| Entry {
                        word: word.text.clone(),
                        count: 0,
                        places: Vec::new(),
                    });
                    entry.count += 1;
                    if !entry.places.iter().any(|(lv, row, _, field)| {
                        *lv == index && *row == place.row && *field == place.field
                    }) {
                        entry
                            .places
                            .push((index, place.row, label.clone(), place.field));
                    }
                }
            }
        }
        self.entries = found.into_values().collect();
        // Most frequent first, then alphabetically.
        self.entries
            .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
        let kept = current
            .as_ref()
            .and_then(|word| self.entries.iter().position(|entry| entry.word == *word));
        self.selected = kept.unwrap_or(0).min(self.entries.len().saturating_sub(1));
        if kept.is_none() {
            // Another word now: its own first suggestion, never the last
            // word's replacement.
            self.replacement.clear();
        }
        self.suggestions = None;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        speller: &Speller,
    ) -> SpellOutcome {
        let mut outcome = SpellOutcome::Open;
        let mut open = true;
        // Esc closes it right after opening, no click first (Anwender 18.11.).
        if open && crate::widgets::esc_closes_window(ctx) {
            open = false;
        }
        egui::Window::new("Rechtschreibung prüfen")
            .id(egui::Id::unique("proava_spell_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([620.0, 460.0])
            .show(ctx, |ui| {
                if self.entries.is_empty() {
                    ui.label(
                        RichText::new("Keine unbekannten Wörter im LV.")
                            .size(14.0)
                            .color(palette.text),
                    );
                    if let Some(message) = &self.message {
                        ui.label(RichText::new(message).color(palette.text_dim));
                    }
                    if ui.button("Schließen").clicked() {
                        outcome = SpellOutcome::Close;
                    }
                    return;
                }
                ui.label(
                    RichText::new(format!(
                        "{} unbekannte Wörter – Groß-/Kleinschreibung wird beachtet, ersetzt werden nur ganze Wörter.",
                        self.entries.len()
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(4.0);
                ui.columns(2, |columns| {
                    egui::ScrollArea::vertical()
                        .id_salt("spell_words")
                        .max_height(360.0)
                        .show(&mut columns[0], |ui| {
                            for (index, entry) in self.entries.iter().enumerate() {
                                let label = format!("{}   ({}×)", entry.word, entry.count);
                                if ui
                                    .selectable_label(index == self.selected, label)
                                    .clicked()
                                {
                                    self.selected = index;
                                    self.suggestions = None;
                                    self.replacement.clear();
                                }
                            }
                        });
                    let ui = &mut columns[1];
                    let Some(entry) = self.entries.get(self.selected) else {
                        return;
                    };
                    let word = entry.word.clone();
                    ui.label(RichText::new(&word).size(16.0).strong().color(palette.text));
                    ui.add_space(4.0);
                    let suggestions = match &self.suggestions {
                        Some((for_word, list)) if *for_word == word => list.clone(),
                        _ => {
                            let list = speller.suggest(&word, 8);
                            if self.replacement.is_empty() {
                                self.replacement = list.first().cloned().unwrap_or_default();
                            }
                            self.suggestions = Some((word.clone(), list.clone()));
                            list
                        }
                    };
                    ui.label(RichText::new("Vorschläge").size(12.0).color(palette.text_dim));
                    ui.horizontal_wrapped(|ui| {
                        if suggestions.is_empty() {
                            ui.label(RichText::new("keine").italics());
                        }
                        for suggestion in &suggestions {
                            if ui
                                .selectable_label(self.replacement == *suggestion, suggestion)
                                .clicked()
                            {
                                self.replacement = suggestion.clone();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    crate::widgets::row(ui, |ui| {
                        ui.label("Ersetzen durch");
                        ui.add(egui::TextEdit::singleline(&mut self.replacement).desired_width(180.0));
                    });
                    ui.horizontal_wrapped(|ui| {
                        let can_replace =
                            !self.replacement.trim().is_empty() && self.replacement != word;
                        if ui
                            .add_enabled(can_replace, egui::Button::new("Überall ersetzen"))
                            .clicked()
                        {
                            outcome = SpellOutcome::Replace {
                                word: word.clone(),
                                with: self.replacement.trim().to_string(),
                            };
                        }
                        if ui.button("Zum Wörterbuch hinzufügen").clicked() {
                            crate::spell::add_word(speller, &word);
                            self.remove_selected();
                        }
                        if ui.button("Ignorieren").clicked() {
                            speller.ignore(&word);
                            self.remove_selected();
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Fundstellen (Klick springt hin)").size(12.0).color(palette.text_dim));
                    egui::ScrollArea::vertical()
                        .id_salt("spell_places")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            if let Some(entry) = self.entries.get(self.selected) {
                                for (lv, row, label, field) in &entry.places {
                                    let what = match field {
                                        SpellField::Short => "Kurztext",
                                        SpellField::Long => "Langtext",
                                    };
                                    if ui.link(format!("{label} · {what}")).clicked() {
                                        outcome = SpellOutcome::Jump(*lv, *row);
                                    }
                                }
                            }
                        });
                });
                if let Some(message) = &self.message {
                    ui.add_space(4.0);
                    ui.label(RichText::new(message).size(12.0).color(palette.text_dim));
                }
            });
        if !open {
            outcome = SpellOutcome::Close;
        }
        outcome
    }

    fn remove_selected(&mut self) {
        if self.selected < self.entries.len() {
            self.entries.remove(self.selected);
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.suggestions = None;
        self.replacement.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_word_after_replacing_gets_its_own_replacement() {
        let mut project = crate::demo::demo_project();
        let aff = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwzESIANRTOLCDUGMPHBYFVKWZ\n";
        let speller = Speller::from_text(aff, "1\nHaus\n", &[]).unwrap();
        let mut dialog = SpellDialog::new(&project, &speller);
        assert!(dialog.entries.len() > 2);
        let first = dialog.entries[0].word.clone();
        dialog.replacement = "Haus".to_string();
        proava_application::spelling::replace_word(project.lv_mut(), &first, "Haus");
        dialog.refresh(&project, &speller);
        assert_ne!(dialog.entries[dialog.selected].word, first);
        assert!(
            dialog.replacement.is_empty(),
            "nicht „Haus“ für das nächste Wort"
        );
    }
}
