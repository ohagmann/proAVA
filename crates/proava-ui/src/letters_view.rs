//! Schriftverkehr: letter templates (Aufforderung, Zuschlag, Absage) with
//! placeholders, recipients from the Bieterverzeichnis, preview and PDF.

use std::collections::HashSet;

use eframe::egui::{self, RichText};
use proava_domain::{
    default_recipients, letter_values, release_letter_values, InviteeId, InvoiceId, LetterKind,
    LetterTemplate, Project, PLACEHOLDERS,
};

use crate::form_kit;
use crate::rich_editor::RichEditor;
use crate::theme::Palette;
use crate::widgets;

/// What the dialog wants this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum LettersResult {
    Open,
    Closed,
    /// Print the letters: one PDF with all (separate = false) or one per bidder into a folder;
    /// `file_in_project` = „Im Projekt ablegen“ (one entry per bidder).
    Export {
        kind: LetterKind,
        recipients: Vec<InviteeId>,
        separate: bool,
        file_in_project: bool,
    },
    /// „Bild einfügen“ in the text: choose image files, then
    /// [`LettersDialog::insert_image`].
    PickImage,
    /// Print the letter to the client about this invoice (Freigabe).
    ExportRelease {
        invoice: InvoiceId,
        file_in_project: bool,
    },
    /// Show the filed letters (Schriftverkehr im Projekt).
    ShowFiled,
}

pub struct LettersDialog {
    kind: LetterKind,
    editor: RichEditor,
    recipients: HashSet<InviteeId>,
    preview: Option<InviteeId>,
    separate: bool,
    copied: Option<String>,
    error: Option<String>,
    today: String,
    show_placeholders: bool,
    pick_image: bool,
    /// Invoice of the letter to the client (Freigabe an den Bauherrn).
    invoice: Option<InvoiceId>,
    /// „Im Projekt ablegen“: a copy of each letter goes into the project file.
    file_in_project: bool,
}

/// File name part for a bidder (letters, digits, `-`; rest `_`).
pub fn file_part(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('_').to_string();
    if cleaned.is_empty() {
        "Bieter".to_string()
    } else {
        cleaned.chars().take(40).collect()
    }
}

impl LettersDialog {
    /// The output choices made in the print dialog (kept for the next time).
    pub fn set_output_options(&mut self, separate: Option<bool>, file_in_project: bool) {
        if let Some(separate) = separate {
            self.separate = separate;
        }
        self.file_in_project = file_in_project;
    }

    /// `today` as TT.MM.JJJJ (date of the letters in the preview).
    pub fn new(project: &Project, kind: LetterKind, today: &str) -> Self {
        let mut dialog = Self {
            kind,
            editor: RichEditor::new("letters_body"),
            recipients: HashSet::new(),
            preview: None,
            separate: false,
            copied: None,
            error: None,
            today: today.to_string(),
            show_placeholders: false,
            pick_image: false,
            invoice: project
                .billing()
                .invoices()
                .last()
                .map(|invoice| invoice.id),
            file_in_project: true,
        };
        dialog.choose_kind(project, kind);
        dialog
    }

    /// Preselects the invoice of the letter to the client (Reiter Abrechnung).
    pub fn set_invoice(&mut self, invoice: Option<InvoiceId>) {
        if invoice.is_some() {
            self.invoice = invoice;
        }
    }

    fn choose_kind(&mut self, project: &Project, kind: LetterKind) {
        self.kind = kind;
        self.recipients = default_recipients(project, kind).into_iter().collect();
        self.preview = project
            .invitees()
            .iter()
            .map(|invitee| invitee.id)
            .find(|id| self.recipients.contains(id))
            .or_else(|| project.invitees().first().map(|invitee| invitee.id));
        self.editor.reset();
        self.error = None;
    }

    /// Inserts an image (file name in the project's images) at the caret of
    /// the template text shown.
    pub fn insert_image(&mut self, project: &mut Project, file: String) {
        let mut template = project.letter_template(self.kind);
        template.body = self.editor.insert_image(file);
        project.set_letter_template(template);
    }

    /// Draws the dialog; template changes go straight into the project
    /// (`dirty` is set then).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        dirty: &mut bool,
    ) -> LettersResult {
        let mut result = LettersResult::Open;
        let mut template = project.letter_template(self.kind);
        let before = template.clone();
        let mut kind = self.kind;
        let (_, close) = widgets::modal(ui, palette, 980.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Schriftverkehr",
                "Vorlage mit Platzhaltern in {geschweiften Klammern}; je angefragtem Bieter ein Schreiben. Die Vorlage gilt für das ganze Projekt.",
            );
            let tabs: Vec<(LetterKind, &str)> = LetterKind::ALL
                .iter()
                .map(|kind| (*kind, kind.label()))
                .collect();
            form_kit::tab_bar(ui, palette, &tabs, &mut kind);
            ui.columns(2, |columns| {
                self.template_column(&mut columns[0], palette, project, &mut template);
                if self.kind.to_client() {
                    self.release_column(&mut columns[1], palette, project, &template);
                } else {
                    self.recipient_column(&mut columns[1], palette, project, &template);
                }
            });
            ui.add_space(8.0);
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).size(12.5).color(palette.error));
            }
            let unknown = template.unknown_placeholders();
            if !unknown.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Unbekannte Platzhalter: {}",
                        unknown
                            .iter()
                            .map(|name| format!("{{{name}}}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .size(12.5)
                    .color(palette.warn),
                );
            }
            let to_client = self.kind.to_client();
            let release_ready = to_client
                && self.invoice.is_some()
                && project.contract().is_some()
                && !project.info().client().name1.trim().is_empty();
            // „Ein PDF je Bieter“ and „Im Projekt ablegen“ are in the print
            // dialog that „PDF erzeugen …“ opens (Stil, Ausgabe).
            ui.horizontal(|ui| {
                let filed = project.correspondence().len();
                if ui
                    .small_button(format!("Abgelegt ({filed}) …"))
                    .on_hover_text("Die im Projekt abgelegten Schreiben")
                    .clicked()
                {
                    result = LettersResult::ShowFiled;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if to_client {
                        "PDF erzeugen …".to_string()
                    } else {
                        format!("PDF erzeugen ({}) …", self.recipients.len())
                    };
                    let ready = if to_client {
                        release_ready
                    } else {
                        !self.recipients.is_empty()
                    };
                    if ui
                        .add_enabled(
                            ready && unknown.is_empty(),
                            egui::Button::new(RichText::new(label).color(palette.on_accent))
                                .fill(palette.accent)
                                .min_size(egui::vec2(160.0, 32.0)),
                        )
                        .clicked()
                    {
                        if let (true, Some(invoice)) = (to_client, self.invoice) {
                            result = LettersResult::ExportRelease {
                                invoice,
                                file_in_project: self.file_in_project,
                            };
                        } else {
                            let recipients: Vec<InviteeId> = project
                                .invitees()
                                .iter()
                                .map(|invitee| invitee.id)
                                .filter(|id| self.recipients.contains(id))
                                .collect();
                            result = LettersResult::Export {
                                kind: self.kind,
                                recipients,
                                separate: self.separate,
                                file_in_project: self.file_in_project,
                            };
                        }
                    }
                    if ui
                        .add(egui::Button::new("Schließen").min_size(egui::vec2(110.0, 32.0)))
                        .clicked()
                    {
                        result = LettersResult::Closed;
                    }
                });
            });
        });
        if template != before {
            project.set_letter_template(template);
            *dirty = true;
        }
        if kind != self.kind {
            self.choose_kind(project, kind);
        }
        if std::mem::take(&mut self.pick_image) && result == LettersResult::Open {
            result = LettersResult::PickImage;
        }
        if close && result == LettersResult::Open {
            result = LettersResult::Closed;
        }
        result
    }

    fn template_column(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        template: &mut LetterTemplate,
    ) {
        form_kit::field(ui, palette, "Betreff", &mut template.subject, None);
        ui.label(RichText::new("Text").size(12.0).color(palette.text_dim));
        egui::ScrollArea::vertical()
            .id_salt("letter_body_scroll")
            .max_height(330.0)
            .show(ui, |ui| {
                let output = self
                    .editor
                    .show(ui, palette, &mut template.body, project.images());
                self.pick_image |= output.pick_image;
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let toggle = if self.show_placeholders {
                "Platzhalter ausblenden"
            } else {
                "Platzhalter einblenden …"
            };
            if ui
                .button(toggle)
                .on_hover_text("Liste der Platzhalter; ein Klick kopiert ihn in die Zwischenablage")
                .clicked()
            {
                self.show_placeholders = !self.show_placeholders;
            }
            if ui
                .button("Standardtext wiederherstellen")
                .on_hover_text("Betreff und Text der mitgelieferten Vorlage")
                .clicked()
            {
                *template = LetterTemplate::standard(template.kind);
                self.editor.reset();
            }
        });
        if let Some(copied) = &self.copied {
            ui.label(
                RichText::new(format!(
                    "{copied} kopiert – mit Strg+V in den Text einfügen"
                ))
                .size(11.5)
                .color(palette.accent),
            );
        }
        if self.show_placeholders {
            egui::ScrollArea::vertical()
                .id_salt("letter_placeholder_list")
                .max_height(150.0)
                .show(ui, |ui| {
                    egui::Grid::new("letter_placeholder_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (key, description) in PLACEHOLDERS {
                                let marker = format!("{{{key}}}");
                                if ui
                                    .button(&marker)
                                    .on_hover_text("In die Zwischenablage kopieren")
                                    .clicked()
                                {
                                    ui.ctx().copy_text(marker.clone());
                                    self.copied = Some(marker);
                                }
                                ui.label(
                                    RichText::new(description)
                                        .size(11.5)
                                        .color(palette.text_dim),
                                );
                                ui.end_row();
                            }
                        });
                });
        }
    }

    /// Right column for the letter to the client: recipient, invoice, preview.
    fn release_column(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        template: &LetterTemplate,
    ) {
        form_kit::section(ui, palette, "Empfänger (Auftraggeber, sonst Bauherr)");
        let client = project.info().client();
        if client.name1.trim().is_empty() {
            form_kit::hint(
                ui,
                palette,
                "Noch kein Bauherr bzw. Auftraggeber – in den Projektstammdaten eintragen.",
            );
        } else {
            ui.label(RichText::new(client.one_line()).color(palette.text));
        }
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Rechnung (Reiter Abrechnung)");
        let invoices = project.billing().invoices();
        if project.contract().is_none() || invoices.is_empty() {
            form_kit::hint(
                ui,
                palette,
                "Noch kein Auftrag oder keine Rechnung – erst im Reiter Abrechnung erfassen.",
            );
            return;
        }
        let label = |id: Option<InvoiceId>| {
            invoices
                .iter()
                .find(|invoice| Some(invoice.id) == id)
                .map(|invoice| {
                    let number = if invoice.number.trim().is_empty() {
                        "(ohne Nr.)"
                    } else {
                        invoice.number.trim()
                    };
                    format!("{number} – {} vom {}", invoice.kind.label(), invoice.date)
                })
                .unwrap_or_else(|| "Rechnung wählen".into())
        };
        egui::ComboBox::from_id_salt("letter_invoice")
            .selected_text(label(self.invoice))
            .width(300.0)
            .show_ui(ui, |ui| {
                for invoice in invoices {
                    ui.selectable_value(
                        &mut self.invoice,
                        Some(invoice.id),
                        label(Some(invoice.id)),
                    );
                }
            });
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Vorschau");
        match release_letter_values(project, self.invoice, &self.today) {
            Ok(values) => {
                let (subject, body) = template.filled(&values);
                preview(ui, palette, subject, body.to_plain());
            }
            Err(error) => form_kit::hint(ui, palette, &error),
        }
    }

    fn recipient_column(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        template: &LetterTemplate,
    ) {
        form_kit::section(ui, palette, "Empfänger (Bieterverzeichnis dieses LV)");
        if project.invitees().is_empty() {
            form_kit::hint(
                ui,
                palette,
                "Noch keine angefragten Bieter – erst im Reiter Angebote ins Bieterverzeichnis aufnehmen.",
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("letter_recipients")
            .max_height(150.0)
            .show(ui, |ui| {
                for invitee in project.invitees() {
                    let mut on = self.recipients.contains(&invitee.id);
                    let name = if invitee.name().is_empty() {
                        "(ohne Namen)"
                    } else {
                        invitee.name()
                    };
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                self.recipients.insert(invitee.id);
                            } else {
                                self.recipients.remove(&invitee.id);
                            }
                        }
                        ui.label(
                            RichText::new(invitee.status.label())
                                .size(11.0)
                                .color(palette.text_dim),
                        );
                    });
                }
            });
        ui.horizontal(|ui| {
            if ui.small_button("Alle").clicked() {
                self.recipients = project
                    .invitees()
                    .iter()
                    .map(|invitee| invitee.id)
                    .collect();
            }
            if ui.small_button("Keiner").clicked() {
                self.recipients.clear();
            }
            if ui
                .small_button("Vorschlag")
                .on_hover_text("Je Art: alle / beauftragter Bieter / übrige Bieter")
                .clicked()
            {
                self.recipients = default_recipients(project, self.kind).into_iter().collect();
            }
        });
        let uses_reason = template.subject.contains("{absagegrund}")
            || template.body.to_plain().contains("{absagegrund}");
        if uses_reason {
            let missing: Vec<&str> = project
                .invitees()
                .iter()
                .filter(|invitee| {
                    self.recipients.contains(&invitee.id)
                        && invitee.rejection_reason.trim().is_empty()
                })
                .map(|invitee| invitee.name())
                .collect();
            if !missing.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Ohne Absagegrund (im Bieterverzeichnis eintragen, sonst „–“): {}",
                        missing.join(", ")
                    ))
                    .size(11.5)
                    .color(palette.warn),
                );
            }
        }
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Vorschau");
        let shown = self
            .preview
            .and_then(|id| project.invitees().iter().find(|invitee| invitee.id == id));
        egui::ComboBox::from_id_salt("letter_preview")
            .selected_text(
                shown
                    .map(|invitee| invitee.name().to_string())
                    .unwrap_or_else(|| "–".into()),
            )
            .width(260.0)
            .show_ui(ui, |ui| {
                for invitee in project.invitees() {
                    ui.selectable_value(&mut self.preview, Some(invitee.id), invitee.name());
                }
            });
        if let Some(invitee) = shown {
            let values = letter_values(project, invitee, &self.today);
            let (subject, body) = template.filled(&values);
            preview(ui, palette, subject, body.to_plain());
        }
    }
}

/// Subject and text of one filled letter.
fn preview(ui: &mut egui::Ui, palette: &Palette, subject: String, body: String) {
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(palette.input)
        .stroke(egui::Stroke::new(1.0, palette.line))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("letter_preview_text")
                .max_height(260.0)
                .show(ui, |ui| {
                    // Left-aligned like the letter (columns justify text).
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.label(
                            RichText::new(subject)
                                .font(crate::fonts::bold(13.0))
                                .color(palette.text),
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new(body).size(12.0).color(palette.text));
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::file_part;

    /// Typing into the template text inside the dialog (a modal) changes the
    /// project's template.
    #[test]
    fn template_text_takes_typing() {
        use super::LettersDialog;
        use crate::theme::Palette;
        use egui_kittest::kittest::Queryable;
        use proava_application::ThemeChoice;
        use proava_domain::{Address, InvitedBidder, LetterKind, Lv, OzMask, Project, ProjectName};

        struct State {
            dialog: LettersDialog,
            project: Project,
            dirty: bool,
        }
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "2026",
            Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        project.add_invitee(InvitedBidder::new(Address::named("Müller GmbH")));
        let dialog = LettersDialog::new(&project, LetterKind::Invitation, "17.11.2026");
        let palette = Palette::of(ThemeChoice::Light);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .with_step_dt(1.0 / 60.0)
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::fonts::install(ctx);
                crate::theme::apply(ctx, ThemeChoice::Light);
            })
            .build_ui_state(
                move |ui, state: &mut State| {
                    state
                        .dialog
                        .show(ui, &palette, &mut state.project, &mut state.dirty);
                },
                State {
                    dialog,
                    project,
                    dirty: false,
                },
            );
        harness.run();
        // First paragraph of the text: just below the last toolbar button.
        let toolbar = harness.get_by_label("Bild einfügen …").rect();
        let label = harness.get_by_label("Text").rect();
        let target = egui::pos2(label.left() + 40.0, toolbar.bottom() + 24.0);
        harness.event(egui::Event::PointerMoved(target));
        harness.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        harness.event(egui::Event::Text("QQ".into()));
        harness.run();
        let body = harness
            .state()
            .project
            .letter_template(LetterKind::Invitation)
            .body
            .to_plain();
        assert!(body.contains("QQ"), "{body}");
        assert!(harness.state().dirty);
    }

    #[test]
    fn file_names_of_bidders() {
        assert_eq!(file_part("Müller & Söhne GmbH"), "Müller___Söhne_GmbH");
        assert_eq!(file_part("  "), "Bieter");
    }
}
