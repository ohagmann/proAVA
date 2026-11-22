//! Projekt ▸ Als Vorlage speichern …: the general part of the open project
//! (LVs with mask, outline, hints, performance descriptions, positions on
//! request, planner, contract terms, letter templates) as a template in the
//! template folder (Einstellungen ▸ Neue Projekte).

use eframe::egui::{self, RichText};

use super::{Dialog, WorkspaceScreen};
use crate::widgets;

/// Form state of „Als Vorlage speichern …“.
pub(super) struct TemplateForm {
    name: String,
    keep_positions: bool,
    /// Replace a template of the same name (asked after the first try).
    replace: bool,
    exists: bool,
    error: Option<String>,
    focus: bool,
}

impl WorkspaceScreen {
    pub(super) fn open_save_template(&mut self) {
        self.dialog = Some(Dialog::SaveTemplate(Box::new(TemplateForm {
            name: self.project.name().as_str().to_string(),
            keep_positions: false,
            replace: false,
            exists: false,
            error: None,
            focus: true,
        })));
    }

    /// One frame of the dialog; false when it closes.
    pub(super) fn save_template_step(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        form: &mut TemplateForm,
    ) -> bool {
        let palette = self.palette;
        let positions = self
            .project
            .lvs()
            .map(|lv| lv.position_count())
            .sum::<usize>();
        let folder = self.launcher.template_folder();
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let ((cancelled, confirmed), close) = widgets::modal(ui, &palette, 560.0, |ui| {
            widgets::title(
                ui,
                &palette,
                "Als Vorlage speichern",
                "Neue Projekte können damit beginnen: auf der Startseite „Neues Projekt …“, Auswahl „Vorlage“.",
            );
            widgets::focus_field(
                ui,
                &palette,
                "Name der Vorlage",
                &mut form.name,
                &mut form.focus,
            );
            ui.checkbox(
                &mut form.keep_positions,
                format!("Positionen mitnehmen ({positions} im Projekt)"),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Mit kommen: LVs mit OZ-Maske und Schrittweite, Titel mit Kostengruppe, \
                     Vorbemerkungen und Hinweise, Ausführungsbeschreibungen, Planer, Vergabeart, \
                     Vertragsbedingungen mit Abzügen und die Briefvorlagen.\n\
                     Nicht mit kommen: Bauherr, Auftraggeber, Baustelle, weitere Adressen, \
                     Bezeichnung, Vergabenummer, Termine, Angebote, Auftrag und Abrechnung.",
                )
                .size(12.0)
                .color(palette.text_dim),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Ordner: {}", folder.display()))
                    .size(11.5)
                    .color(palette.text_dim),
            );
            ui.add_space(8.0);
            if form.exists {
                ui.checkbox(
                    &mut form.replace,
                    "Vorhandene Vorlage gleichen Namens ersetzen",
                );
                ui.add_space(4.0);
            }
            widgets::error_line(ui, &palette, form.error.as_deref());
            widgets::dialog_buttons(ui, &palette, "Speichern")
        });
        if close || cancelled {
            return false;
        }
        if !(confirmed || enter) {
            return true;
        }
        match self.launcher.save_as_template(
            &self.project,
            &form.name,
            form.keep_positions,
            form.replace,
        ) {
            Ok(file) => {
                self.say_with_file(
                    ctx,
                    format!(
                        "Vorlage gespeichert: {}",
                        crate::file_dialog::file_name(&file)
                    ),
                    Some(file),
                );
                false
            }
            Err(error) => {
                form.exists = self
                    .launcher
                    .template_file(&form.name)
                    .is_ok_and(|file| self.launcher.is_file(&file));
                form.error = Some(error);
                form.focus = true;
                true
            }
        }
    }
}
