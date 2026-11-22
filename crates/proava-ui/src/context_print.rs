//! Druckdialog für Dokumente aus einem Fenster: Änderungsliste (LV-Vergleich),
//! Prüfung Nachtragsangebot, Nachtragsübersicht und Schreiben
//! (Schriftverkehr). Gleicher Aufbau wie „Drucken …“ (Strg+P): links das
//! Dokument mit Kurzbeschreibung, rechts oben der Stil, darunter die Optionen
//! der Art; Enter speichert das PDF. Die Optionen merkt sich proAVA je Art.

use eframe::egui::{self, RichText};
use proava_application::{PrintKind, PrintLook, PrintSettings, PrintStyle};

use crate::theme::Palette;
use crate::widgets;

pub struct ContextPrintDialog {
    kind: PrintKind,
    /// What exactly is printed („N01 Zusätzliche Raumbediengeräte“,
    /// „gegenüber LV 480 (Stand 12.11.)“, „Zuschlagsschreiben an 2 Bieter“).
    detail: String,
    settings: PrintSettings,
    default_style: PrintStyle,
    default_border: bool,
    default_grid: bool,
    /// Kopf-/Fußzeile of the project (true) or from the settings.
    own_frame: bool,
    /// Schreiben: one PDF per bidder into a folder (None: no choice, e.g. to the Bauherrn).
    pub separate: Option<bool>,
    /// Schreiben: „Im Projekt ablegen“.
    pub file_in_project: Option<bool>,
}

pub enum ContextPrintResult {
    Open,
    Cancelled,
    /// Close and open Projektstammdaten ▸ Kopf-/Fußzeile.
    EditFrame,
    /// Choose the file (or folder) and write.
    Print,
}

impl ContextPrintDialog {
    /// `settings` = the remembered options of `kind`; `look` = the defaults
    /// from Extras ▸ Einstellungen ▸ Drucken.
    pub fn new(
        kind: PrintKind,
        detail: String,
        settings: PrintSettings,
        look: &PrintLook,
        own_frame: bool,
    ) -> Self {
        Self {
            kind,
            detail,
            settings,
            default_style: look.style,
            default_border: look.border,
            default_grid: look.table_grid,
            own_frame,
            separate: None,
            file_in_project: None,
        }
    }

    /// Schreiben: the output choices of the Schriftverkehr.
    pub fn with_letter_options(mut self, separate: Option<bool>, file_in_project: bool) -> Self {
        self.separate = separate;
        self.file_in_project = Some(file_in_project);
        self
    }

    pub fn kind(&self) -> PrintKind {
        self.kind
    }

    pub fn settings(&self) -> &PrintSettings {
        &self.settings
    }

    /// The look for the renderer: Stil and Blattrahmen of this print.
    pub fn look(&self, base: &PrintLook) -> PrintLook {
        PrintLook {
            style: self.settings.style.unwrap_or(base.style),
            border: self.kind.offers_border() && self.settings.border.unwrap_or(base.border),
            table_grid: self.kind.offers_grid() && self.settings.grid.unwrap_or(base.table_grid),
            ..base.clone()
        }
    }

    pub fn landscape(&self) -> bool {
        self.kind.offers_landscape() && self.settings.landscape
    }

    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> ContextPrintResult {
        // Enter prints – unless it ends typing in a text field.
        let typing = ui.ctx().egui_wants_keyboard_input();
        let enter = !typing && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let mut edit_frame = false;
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 620.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Drucken",
                "Die Optionen merkt sich proAVA für diese Dokumentart. Enter speichert das PDF.",
            );
            ui.columns(2, |columns| {
                let ui = &mut columns[0];
                heading(ui, palette, "Dokumentart");
                // The one kind of this window, chosen.
                let _ = ui.radio(true, self.kind.label());
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.kind.description()).size(11.5).color(palette.text_dim),
                        )
                        .wrap(),
                    );
                });
                if !self.detail.is_empty() {
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(&self.detail).size(12.5).color(palette.text)).wrap());
                }

                let ui = &mut columns[1];
                heading(ui, palette, "Stil");
                ui.horizontal(|ui| {
                    let current = self.settings.style.unwrap_or(self.default_style);
                    for style in PrintStyle::ALL {
                        let button = egui::Button::selectable(current == style, RichText::new(style.label()).size(13.5))
                            .frame_when_inactive(true)
                            .min_size(egui::vec2(110.0, 30.0));
                        let hint = match style {
                            PrintStyle::Classic => "Schwarze Schrift, Titelzeilen in der Akzentfarbe hinterlegt",
                            PrintStyle::ProAva => "Überschriften und Spaltenköpfe in der Akzentfarbe",
                        };
                        if ui.add(button).on_hover_text(hint).clicked() {
                            self.settings.style = Some(style);
                        }
                    }
                });
                ui.label(
                    RichText::new("Vorgabe und Akzentfarbe: Extras › Einstellungen › Drucken")
                        .size(11.0)
                        .color(palette.text_dim),
                );
                ui.add_space(8.0);
                if self.kind.offers_landscape() {
                    heading(ui, palette, "Seite");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.settings.landscape, false, "A4 hoch");
                        ui.radio_value(&mut self.settings.landscape, true, "A4 quer");
                    });
                    ui.add_space(6.0);
                }
                if self.separate.is_some() || self.file_in_project.is_some() {
                    heading(ui, palette, "Ausgabe");
                    if let Some(separate) = self.separate.as_mut() {
                        ui.checkbox(separate, "Ein PDF je Bieter (in einen Ordner)");
                    }
                    if let Some(file) = self.file_in_project.as_mut() {
                        ui.checkbox(file, "Im Projekt ablegen").on_hover_text(
                            "Eine Kopie jedes Schreibens als PDF in der Projektdatei ablegen – je Empfänger ein \
                             Eintrag, auch beim Sammel-PDF (Angebote › Schriftverkehr im Projekt)",
                        );
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let which = if self.kind == PrintKind::Letters { "Fußzeile" } else { "Kopf-/Fußzeile" };
                let source = if self.own_frame { "eigene des Projekts" } else { "aus den Einstellungen" };
                ui.label(RichText::new(format!("{which}: {source}")).size(11.5).color(palette.text_dim));
                if ui
                    .button("Kopf-/Fußzeile …")
                    .on_hover_text("Öffnet die Projektstammdaten, Reiter Kopf-/Fußzeile")
                    .clicked()
                {
                    edit_frame = true;
                }
                if self.kind.offers_border() {
                    ui.add_space(12.0);
                    let mut border = self.settings.border.unwrap_or(self.default_border);
                    if ui
                        .checkbox(&mut border, "Blattrahmen")
                        .on_hover_text(
                            "Linie um Kopfzeile, Inhalt und Fußzeile jeder Seite; je Dokumentart gemerkt. \
                             Vorgabe und Strichstärke unter Extras › Einstellungen › Drucken.",
                        )
                        .changed()
                    {
                        self.settings.border = Some(border);
                    }
                }
                if self.kind.offers_grid() {
                    ui.add_space(12.0);
                    let mut grid = self.settings.grid.unwrap_or(self.default_grid);
                    if ui
                        .checkbox(&mut grid, "Gitter")
                        .on_hover_text(
                            "Tabellen voll umrandet: Außenrahmen, alle senkrechten und waagrechten Linien; im LV ein Rahmen \
                         je Position und die Preiszeile in Feldern. Je Dokumentart gemerkt, Vorgabe unter Extras › \
                         Einstellungen › Drucken (der Preisspiegel ist ab Werk umrandet).",
                        )
                        .changed()
                    {
                        self.settings.grid = Some(grid);
                    }
                }
            });
            ui.add_space(10.0);
            widgets::dialog_buttons(ui, palette, "PDF speichern …")
        });
        if cancelled || close {
            return ContextPrintResult::Cancelled;
        }
        if edit_frame {
            return ContextPrintResult::EditFrame;
        }
        if confirmed || enter {
            return ContextPrintResult::Print;
        }
        ContextPrintResult::Open
    }
}

/// Section heading like in the print dialog.
fn heading(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(13.0).strong().color(palette.text));
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_follows_the_choice_and_letters_have_no_border() {
        let base = PrintLook {
            border: true,
            ..PrintLook::default()
        };
        let settings = PrintSettings {
            style: Some(PrintStyle::ProAva),
            ..PrintSettings::default()
        };
        let dialog = ContextPrintDialog::new(
            PrintKind::ChangeOrderCheck,
            String::new(),
            settings.clone(),
            &base,
            false,
        );
        assert_eq!(dialog.look(&base).style, PrintStyle::ProAva);
        assert!(dialog.look(&base).border, "Vorgabe Blattrahmen");
        let letters =
            ContextPrintDialog::new(PrintKind::Letters, String::new(), settings, &base, false);
        assert!(!letters.look(&base).border, "Briefe nie mit Rahmen");
        assert!(!letters.landscape());
    }
}
