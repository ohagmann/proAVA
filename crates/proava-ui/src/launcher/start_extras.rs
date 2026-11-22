//! Startfenster: Vorschau der gewählten Zeile, Rechtsklick-Menü der
//! Projektliste und die Vorlage im Dialog „Neues Projekt“.

use std::path::Path;

use eframe::egui::{self, RichText};
use proava_application::{ProjectPreview, RecentProject, TemplateEntry, TemplateSource};

use crate::icons::Icon;
use crate::theme::Palette;
use crate::{format, widgets};

/// Width of the preview column right of the list.
pub(super) const PREVIEW_WIDTH: f32 = 300.0;

/// What a row's context menu or the preview asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowCommand {
    Open,
    OpenReadOnly,
    ShowFolder,
    Copy,
    Forget,
    Locate,
}

/// Rechtsklick auf ein Projekt der Liste.
pub(super) fn row_menu(response: &egui::Response, missing: bool) -> Option<RowCommand> {
    let mut command = None;
    response.context_menu(|ui| {
        ui.set_min_width(230.0);
        let mut entry = |ui: &mut egui::Ui, enabled: bool, label: &str, wanted: RowCommand| {
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                command = Some(wanted);
                ui.close();
            }
        };
        entry(ui, !missing, "Öffnen", RowCommand::Open);
        entry(
            ui,
            !missing,
            "Schreibgeschützt öffnen",
            RowCommand::OpenReadOnly,
        );
        ui.separator();
        entry(ui, true, "Im Dateimanager zeigen", RowCommand::ShowFolder);
        entry(ui, !missing, "Kopie anlegen …", RowCommand::Copy);
        if missing {
            entry(ui, true, "Pfad suchen …", RowCommand::Locate);
        }
        ui.separator();
        entry(ui, true, crate::strings::FORGET_PROJECT, RowCommand::Forget);
    });
    command
}

/// The preview column: name, Bauherr, LVs with net total, last change,
/// place. `preview` is `None` while nothing is chosen.
pub(super) fn preview_panel(
    ui: &mut egui::Ui,
    palette: &Palette,
    item: Option<&RecentProject>,
    preview: Option<&Result<ProjectPreview, String>>,
    missing: bool,
) -> Option<RowCommand> {
    let mut command = None;
    let Some(item) = item else {
        ui.add_space(26.0);
        ui.label(
            RichText::new(
                "Ein Projekt in der Liste wählen, um hier Bauherr, LVs und Speicherort zu sehen.",
            )
            .size(12.0)
            .color(palette.text_dim),
        );
        return None;
    };
    ui.label(RichText::new("Vorschau").size(12.0).color(palette.text_dim));
    ui.add_space(4.0);
    ui.label(
        RichText::new(&item.name)
            .size(15.0)
            .strong()
            .color(palette.text),
    );
    ui.add_space(8.0);

    if missing {
        ui.label(
            RichText::new("Die Projektdatei fehlt.")
                .size(12.5)
                .color(palette.warn),
        );
        ui.add_space(4.0);
        place(ui, palette, &item.path);
        ui.add_space(10.0);
        if widgets::icon_label_button(ui, palette, Icon::Search, "Pfad suchen …")
            .on_hover_text("Die Datei an ihrem neuen Ort wählen; der Eintrag zeigt dann dorthin.")
            .clicked()
        {
            command = Some(RowCommand::Locate);
        }
        return command;
    }

    match preview {
        None => {}
        Some(Err(error)) => {
            ui.label(RichText::new(error).size(12.0).color(palette.error));
        }
        Some(Ok(preview)) => {
            if !preview.label.trim().is_empty() {
                fact(ui, palette, "Bezeichnung", &preview.label);
            }
            fact(
                ui,
                palette,
                "Bauherr",
                if preview.owner.trim().is_empty() {
                    "–"
                } else {
                    &preview.owner
                },
            );
            ui.label(
                RichText::new(if preview.lvs.len() == 1 {
                    "Leistungsverzeichnis".to_string()
                } else {
                    format!("{} Leistungsverzeichnisse", preview.lvs.len())
                })
                .size(11.5)
                .color(palette.text_dim),
            );
            for (title, net) in &preview.lvs {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format::truncate_middle(title, 30))
                            .size(12.5)
                            .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(net.to_string())
                                .size(12.5)
                                .color(palette.text),
                        );
                    });
                });
            }
            ui.label(
                RichText::new("Netto nach Planerpreisen")
                    .size(10.5)
                    .color(palette.text_dim),
            );
            ui.add_space(6.0);
            if preview.bids > 0 {
                fact(ui, palette, "Angebote", &preview.bids.to_string());
            }
            if let Some(modified) = &preview.modified {
                fact(
                    ui,
                    palette,
                    "Letzte Änderung",
                    &super::german_date_time(modified),
                );
            }
            place(ui, palette, &item.path);
        }
    }
    ui.add_space(10.0);
    if widgets::button(ui, palette, "Öffnen", true, 120.0).clicked() {
        command = Some(RowCommand::Open);
    }
    command
}

fn fact(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &str) {
    ui.label(RichText::new(label).size(11.5).color(palette.text_dim));
    ui.label(RichText::new(value).size(13.0).color(palette.text));
    ui.add_space(6.0);
}

fn place(ui: &mut egui::Ui, palette: &Palette, path: &Path) {
    ui.label(
        RichText::new("Speicherort")
            .size(11.5)
            .color(palette.text_dim),
    );
    let folder = path.parent().unwrap_or(path).to_string_lossy().to_string();
    ui.add(egui::Label::new(RichText::new(folder).size(12.0).color(palette.text)).wrap());
}

/// „Vorlage“ in the dialog „Neues Projekt“: the choice, a line about what
/// it brings and, when it holds positions, whether they come along.
pub(super) fn template_choice(
    ui: &mut egui::Ui,
    palette: &Palette,
    templates: &[TemplateEntry],
    chosen: &mut usize,
    mask: &mut String,
    info: Option<&Result<(String, bool), String>>,
    keep_positions: &mut bool,
) {
    let label = ui.label(RichText::new("Vorlage").size(12.0).color(palette.text_dim));
    ui.add_space(4.0);
    let shown = templates
        .get(*chosen)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    ui.scope(|ui| {
        // As high as the text fields above.
        ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
        egui::ComboBox::from_id_salt("new_project_template")
            .selected_text(shown)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (index, entry) in templates.iter().enumerate() {
                    ui.selectable_value(chosen, index, &entry.name);
                }
            })
            .response
            .labelled_by(label.id);
    });
    ui.add_space(6.0);
    if let Some(entry) = templates.get(*chosen) {
        mask_row(ui, palette, &entry.source, mask);
    }
    ui.add_space(4.0);
    match info {
        Some(Ok((text, has_positions))) => {
            ui.label(RichText::new(text).size(11.5).color(palette.text_dim));
            if *has_positions {
                ui.add_space(4.0);
                ui.checkbox(keep_positions, "Positionen der Vorlage übernehmen");
            }
        }
        Some(Err(error)) => {
            ui.label(RichText::new(error).size(11.5).color(palette.error));
        }
        None => {}
    }
    ui.add_space(10.0);
}

/// „OZ-Maske der Vorlage: 11.222.PPPPI“ – a field for „leer“ and the GA
/// template, plain text for an own template (it brings its outline along).
fn mask_row(ui: &mut egui::Ui, palette: &Palette, source: &TemplateSource, mask: &mut String) {
    let caption = if *source == TemplateSource::Empty {
        "OZ-Maske:"
    } else {
        "OZ-Maske der Vorlage:"
    };
    crate::widgets::row(ui, |ui| {
        let label = ui.label(RichText::new(caption).size(12.0).color(palette.text_dim));
        if source.mask_changeable() {
            ui.add(
                egui::TextEdit::singleline(mask)
                    .desired_width(150.0)
                    .margin(egui::Margin::symmetric(8, 5)),
            )
            .labelled_by(label.id)
            .on_hover_text(
                "1/2/3 = Titelebenen, L = Buchstaben, P = Position, I = Index; \
                 gilt nur für das neue Projekt",
            );
        } else {
            ui.label(RichText::new(mask.as_str()).size(12.0).color(palette.text));
        }
    });
}
