//! „Datei ersetzen?“ – asked once when proAVA is about to write over
//! existing files (Anwender, 17.11.2026: „einmal drücken und dann ein Popup:
//! Datei existiert, überschreiben ja/nein“). One file: name, folder, size and
//! last change; several files (one PDF per bidder): the list and „Alle
//! ersetzen“. The focus starts on „Abbrechen“, Enter presses the focused
//! button, Esc cancels; E replaces, A cancels.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use proava_application::{FileDetails, Launcher};

use crate::file_dialog::file_name;
use crate::theme::Palette;
use crate::{format, widgets};

/// Shown in the list; more files are counted.
const LIST_LIMIT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceAnswer {
    Open,
    Replace,
    Cancel,
}

pub struct ReplaceQuestion {
    files: Vec<(PathBuf, Option<FileDetails>)>,
    focus_cancel: bool,
}

impl ReplaceQuestion {
    /// The question for these existing files (at least one).
    pub fn new(launcher: &Launcher, files: Vec<PathBuf>) -> Self {
        let files = files
            .into_iter()
            .map(|file| {
                let details = launcher.file_details(&file);
                (file, details)
            })
            .collect();
        Self {
            files,
            focus_cancel: true,
        }
    }

    pub fn files(&self) -> Vec<&Path> {
        self.files.iter().map(|(file, _)| file.as_path()).collect()
    }

    /// Title line of the question.
    pub fn question(&self) -> String {
        match self.files.as_slice() {
            [(file, _)] => format!("Die Datei „{}“ gibt es schon. Ersetzen?", file_name(file)),
            files => format!("{} Dateien gibt es schon. Alle ersetzen?", files.len()),
        }
    }

    /// Draws the question as its own modal on top of whatever is open.
    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> ReplaceAnswer {
        let id = egui::Id::unique("proava_replace_question");
        let several = self.files.len() > 1;
        let mut answer = ReplaceAnswer::Open;
        let (_, close) = widgets::modal_with_id(ui, palette, id, 560.0, |ui| {
            widgets::title(
                ui,
                palette,
                if several {
                    "Dateien ersetzen"
                } else {
                    "Datei ersetzen"
                },
                "",
            );
            ui.label(
                RichText::new(self.question())
                    .size(14.0)
                    .color(palette.text),
            );
            ui.add_space(8.0);
            if several {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (file, details) in self.files.iter().take(LIST_LIMIT) {
                            ui.label(
                                RichText::new(format!(
                                    "{}   {}",
                                    file_name(file),
                                    details_line(details.as_ref())
                                ))
                                .size(12.5)
                                .color(palette.text),
                            );
                        }
                        if self.files.len() > LIST_LIMIT {
                            ui.label(
                                RichText::new(format!(
                                    "… und {} weitere",
                                    self.files.len() - LIST_LIMIT
                                ))
                                .size(12.0)
                                .color(palette.text_dim),
                            );
                        }
                    });
                if let Some(folder) = self.files.first().and_then(|(file, _)| file.parent()) {
                    ui.add_space(4.0);
                    detail_row(ui, palette, "Ordner", &folder.to_string_lossy());
                }
            } else if let Some((file, details)) = self.files.first() {
                if let Some(folder) = file.parent() {
                    detail_row(ui, palette, "Ordner", &folder.to_string_lossy());
                }
                if let Some(details) = details {
                    detail_row(ui, palette, "Größe", &size_text(details.size));
                    if let Some(modified) = &details.modified_iso {
                        detail_row(ui, palette, "Geändert", &changed_text(modified));
                    }
                }
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("Die vorhandene Datei wird dabei überschrieben.")
                    .size(12.0)
                    .color(palette.text_dim),
            );
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let replace_label = if several { "Alle ersetzen" } else { "Ersetzen" };
                if widgets::danger_button(ui, palette, replace_label, 120.0).clicked() {
                    answer = ReplaceAnswer::Replace;
                }
                ui.add_space(8.0);
                let cancel = ui.add(
                    egui::Button::new(RichText::new("Abbrechen").color(palette.text))
                        .min_size(egui::vec2(120.0, 30.0)),
                );
                if std::mem::take(&mut self.focus_cancel) {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    answer = ReplaceAnswer::Cancel;
                }
            });
            // E = Ersetzen, A = Abbrechen (no text field in this window).
            let (e, a) = ui.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::E),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::A),
                )
            });
            if e {
                answer = ReplaceAnswer::Replace;
            } else if a {
                answer = ReplaceAnswer::Cancel;
            }
        });
        if close && answer == ReplaceAnswer::Open {
            answer = ReplaceAnswer::Cancel;
        }
        answer
    }
}

fn detail_row(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, 18.0],
            egui::Label::new(RichText::new(label).size(12.0).color(palette.text_dim)),
        );
        ui.label(
            RichText::new(format::truncate_middle(value, 70))
                .size(12.5)
                .color(palette.text),
        );
    });
}

fn details_line(details: Option<&FileDetails>) -> String {
    let Some(details) = details else {
        return String::new();
    };
    match &details.modified_iso {
        Some(modified) => format!("{} · {}", size_text(details.size), changed_text(modified)),
        None => size_text(details.size),
    }
}

/// `2026-11-17T22:05:00` → „17.11.2026, 22:05 Uhr“.
fn changed_text(iso: &str) -> String {
    let date = format::iso_to_german_date(iso);
    match iso.get(11..16) {
        Some(time) => format!("{date}, {time} Uhr"),
        None => date,
    }
}

/// Bytes in German: „812 Byte“, „12,3 KB“, „4,1 MB“.
pub fn size_text(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} Byte")
    } else if value < KB * KB {
        format!("{:.1} KB", value / KB).replace('.', ",")
    } else if value < KB * KB * KB {
        format!("{:.1} MB", value / KB / KB).replace('.', ",")
    } else {
        format!("{:.1} GB", value / KB / KB / KB).replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::{changed_text, size_text};

    #[test]
    fn sizes_and_times_read_german() {
        assert_eq!(size_text(812), "812 Byte");
        assert_eq!(size_text(12_595), "12,3 KB");
        assert_eq!(size_text(4_300_000), "4,1 MB");
        assert_eq!(changed_text("2026-11-17T22:05:00"), "17.11.2026, 22:05 Uhr");
    }
}
