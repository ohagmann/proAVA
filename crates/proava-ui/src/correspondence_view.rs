//! Schriftverkehr im Projekt: the filed letters (PDFs inside the project
//! file, folder `schriftverkehr/`) – list with filter, open, save as,
//! Bemerkung, remove and own PDFs.

use eframe::egui::{self, RichText};
use proava_domain::{
    size_label, CorrespondenceEntry, CorrespondenceId, CorrespondenceKind, Project,
};

use crate::icons::Icon;
use crate::list_keys::{ListFocus, ListKey};
use crate::theme::Palette;
use crate::widgets;

/// Tooltip of the buttons without a selection.
const CHOOSE_FIRST: &str = "Erst ein Schreiben wählen";

/// What the list wants this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum CorrespondenceResult {
    Open,
    Closed,
    /// Show the PDF in the system viewer (from a temporary copy).
    OpenPdf(CorrespondenceId),
    /// „Speichern unter …“: choose a file for the PDF.
    SaveAs(CorrespondenceId),
    /// „PDF hinzufügen …“: choose PDFs to file as eigene Schreiben.
    AddPdf,
    /// The archive was changed in the list (Bemerkung, Entfernen); `before`
    /// is the state for Rückgängig.
    Changed {
        before: Vec<CorrespondenceEntry>,
        label: &'static str,
        join: bool,
    },
}

/// Columns: Datum, Art, Empfänger, Betreff, LV, Größe (widths in points).
const COLUMNS: [(&str, f32); 6] = [
    ("Datum", 86.0),
    ("Art", 150.0),
    ("Empfänger", 190.0),
    ("Betreff", 300.0),
    ("LV", 150.0),
    ("Größe", 70.0),
];
const ROW_HEIGHT: f32 = 24.0;
/// Fixed heights, so the (centred) dialog does not move when a row is
/// selected or the filter changes – a double click stays on its row.
const LIST_HEIGHT: f32 = 330.0;
const SELECTION_HEIGHT: f32 = 34.0;

#[derive(Default)]
pub struct CorrespondenceDialog {
    kind: Option<CorrespondenceKind>,
    recipient: Option<String>,
    selected: Option<CorrespondenceId>,
    /// „Wirklich entfernen?“ is open for this entry.
    confirm_remove: Option<CorrespondenceId>,
    /// The dialog owns the keyboard: ↑/↓, Entf, Enter (open).
    keys: ListFocus,
}

/// Name of a row for screen readers and UI tests: „Absage an Müller GmbH vom 18.11.2026“.
pub fn row_label(entry: &CorrespondenceEntry) -> String {
    let who = if entry.recipient_name.trim().is_empty() {
        "(ohne Empfänger)"
    } else {
        entry.recipient_name.trim()
    };
    let date = if entry.date.trim().is_empty() {
        "(ohne Datum)".to_string()
    } else {
        format!("vom {}", entry.date.trim())
    };
    format!("{} an {who} {date}", entry.kind.label())
}

impl CorrespondenceDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Preselects a bidder (from the Bieterverzeichnis).
    pub fn with_recipient(mut self, name: Option<String>) -> Self {
        self.recipient = name.filter(|name| !name.trim().is_empty());
        self
    }

    pub fn selected(&self) -> Option<CorrespondenceId> {
        self.selected
    }

    /// Selects an entry (e.g. the PDF just added).
    pub fn select(&mut self, id: Option<CorrespondenceId>) {
        self.selected = id;
    }

    /// Entries after the filter, newest first.
    fn shown<'a>(&self, project: &'a Project) -> Vec<&'a CorrespondenceEntry> {
        let mut list: Vec<&CorrespondenceEntry> = project
            .correspondence()
            .iter()
            .filter(|entry| self.kind.is_none_or(|kind| entry.kind == kind))
            .filter(|entry| {
                self.recipient
                    .as_deref()
                    .is_none_or(|name| entry.recipient_name.trim() == name)
            })
            .collect();
        list.sort_by(|a, b| b.sort_date().cmp(&a.sort_date()).then(b.id.cmp(&a.id)));
        list
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> CorrespondenceResult {
        let mut result = CorrespondenceResult::Open;
        if self
            .selected
            .is_some_and(|id| project.correspondence_entry(id).is_none())
        {
            self.selected = None;
        }
        let (_, close) = widgets::modal(ui, palette, 1000.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Schriftverkehr im Projekt",
                "Abgelegte Schreiben als PDF in der Projektdatei – je Empfänger ein Eintrag, auch beim Sammel-PDF. \
                 Doppelklick öffnet das Schreiben.",
            );
            self.filter_row(ui, palette, project);
            ui.add_space(6.0);
            self.table(ui, palette, project, &mut result);
            self.keyboard(ui, project, &mut result);
            ui.add_space(6.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), SELECTION_HEIGHT),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_height(SELECTION_HEIGHT);
                    self.selection_row(ui, palette, project, &mut result);
                },
            );
            if let Some(hint) = project.correspondence_hint() {
                ui.add_space(4.0);
                ui.label(RichText::new(hint).size(12.0).color(palette.warn));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::icon_label_button(ui, palette, Icon::Plus, "PDF hinzufügen …")
                    .on_hover_text("Eigenes Schreiben (PDF) im Projekt ablegen, z. B. ein Protokoll oder eine Mail als PDF")
                    .clicked()
                {
                    result = CorrespondenceResult::AddPdf;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("Schließen").min_size(egui::vec2(110.0, 32.0)))
                        .clicked()
                    {
                        result = CorrespondenceResult::Closed;
                    }
                });
            });
        });
        if close && result == CorrespondenceResult::Open && self.confirm_remove.is_none() {
            result = CorrespondenceResult::Closed;
        } else if close {
            // Esc closes the question first.
            self.confirm_remove = None;
        }
        result
    }

    fn filter_row(&mut self, ui: &mut egui::Ui, palette: &Palette, project: &Project) {
        crate::widgets::row(ui, |ui| {
            let caption = ui.label(RichText::new("Art").size(12.5).color(palette.text_dim));
            egui::ComboBox::from_id_salt("correspondence_kind")
                .selected_text(self.kind.map_or("Alle Arten", CorrespondenceKind::label))
                .width(190.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.kind, None, "Alle Arten");
                    for kind in CorrespondenceKind::ALL {
                        ui.selectable_value(&mut self.kind, Some(kind), kind.label());
                    }
                })
                .response
                .labelled_by(caption.id);
            ui.add_space(12.0);
            let caption = ui.label(
                RichText::new("Empfänger")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            egui::ComboBox::from_id_salt("correspondence_recipient")
                .selected_text(
                    self.recipient
                        .clone()
                        .unwrap_or_else(|| "Alle Empfänger".into()),
                )
                .width(240.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.recipient, None, "Alle Empfänger");
                    for name in project.correspondence_recipients() {
                        let label = name.clone();
                        ui.selectable_value(&mut self.recipient, Some(name), label);
                    }
                })
                .response
                .labelled_by(caption.id);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let all = project.correspondence();
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        crate::format::count(all.len(), "Schreiben", "Schreiben"),
                        size_label(project.correspondence_size())
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
            });
        });
    }

    /// ↑/↓ choose, Entf asks before removing, Enter opens the PDF.
    fn keyboard(&mut self, ui: &egui::Ui, project: &Project, result: &mut CorrespondenceResult) {
        if self.confirm_remove.is_some() || *result != CorrespondenceResult::Open {
            return;
        }
        // The dialog is modal: its list always has the keyboard.
        self.keys.take();
        let Some(key) = self.keys.key(ui) else {
            return;
        };
        let ids: Vec<CorrespondenceId> = self.shown(project).iter().map(|entry| entry.id).collect();
        match key {
            ListKey::Up | ListKey::Down => {
                if let Some(next) =
                    crate::list_keys::step(&ids, self.selected, key == ListKey::Down)
                {
                    self.selected = Some(next);
                }
            }
            ListKey::Delete => self.confirm_remove = self.selected,
            ListKey::Enter => {
                if let Some(entry) = self
                    .selected
                    .and_then(|id| project.correspondence_entry(id))
                {
                    if entry.has_pdf() {
                        *result = CorrespondenceResult::OpenPdf(entry.id);
                    }
                }
            }
        }
    }

    fn table(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        result: &mut CorrespondenceResult,
    ) {
        let scroll = self.keys.take_scroll();
        let width: f32 = COLUMNS.iter().map(|(_, width)| width).sum();
        // Header
        let (header, _) =
            ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::hover());
        let painter = ui.painter_at(header);
        painter.rect_filled(header, 0.0, palette.panel_alt);
        let mut x = header.left() + 6.0;
        for (title, column) in COLUMNS {
            painter.text(
                egui::pos2(x, header.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                crate::fonts::bold(12.0),
                palette.text_dim,
            );
            x += column;
        }
        let shown = self.shown(project);
        egui::ScrollArea::vertical()
            .id_salt("correspondence_rows")
            .max_height(LIST_HEIGHT)
            .min_scrolled_height(LIST_HEIGHT)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_height(LIST_HEIGHT);
                ui.set_min_width(width);
                if shown.is_empty() {
                    ui.add_space(12.0);
                    let text = if project.correspondence().is_empty() {
                        "Noch nichts abgelegt. Schreiben aus dem Schriftverkehr (Aufforderung, Zuschlag, Absage) und \
                         das Anschreiben „Freigabe an den Bauherrn“ kommen mit „Im Projekt ablegen“ hierher."
                    } else {
                        "Kein Schreiben passt zum Filter."
                    };
                    ui.label(RichText::new(text).size(12.5).color(palette.text_dim));
                    return;
                }
                for (index, entry) in shown.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
                    let selected = self.selected == Some(entry.id);
                    widgets::name_row(&response, &row_label(entry), selected);
                    let fill = if selected {
                        palette.selected
                    } else if response.hovered() {
                        palette.row_hover
                    } else if index % 2 == 1 {
                        palette.row_alt
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 0.0, fill);
                    if selected {
                        widgets::selection_bar(&painter, palette, rect);
                        if scroll {
                            ui.scroll_to_rect(rect, None);
                        }
                    }
                    let size = if entry.has_pdf() {
                        size_label(entry.size)
                    } else {
                        "PDF fehlt".to_string()
                    };
                    let cells = [
                        entry.date.clone(),
                        entry.kind.label().to_string(),
                        entry.recipient_name.clone(),
                        entry.subject.clone(),
                        entry.lv_label(),
                        size,
                    ];
                    let mut x = rect.left() + 6.0;
                    for ((_, column), text) in COLUMNS.iter().zip(cells) {
                        let font = egui::FontId::proportional(12.5);
                        let mut job = egui::text::LayoutJob::single_section(
                            crate::format::one_line(&text).into_owned(),
                            egui::TextFormat::simple(font, palette.text),
                        );
                        job.wrap = egui::text::TextWrapping::truncate_at_width(column - 10.0);
                        let galley = painter.layout_job(job);
                        painter.galley(
                            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
                            galley,
                            palette.text,
                        );
                        x += column;
                    }
                    let mut hover = format!("{}\nDatei im Projekt: {}", entry.subject, entry.file);
                    if !entry.remark.trim().is_empty() {
                        hover.push_str(&format!("\nBemerkung: {}", entry.remark.trim()));
                    }
                    let response = response.on_hover_text(hover);
                    if response.clicked() || response.secondary_clicked() {
                        self.selected = Some(entry.id);
                        self.confirm_remove = None;
                    }
                    response.context_menu(|ui| {
                        if ui.add_enabled(entry.has_pdf(), egui::Button::new("Öffnen")).clicked() {
                            *result = CorrespondenceResult::OpenPdf(entry.id);
                            ui.close();
                        }
                        if ui
                            .add_enabled(entry.has_pdf(), egui::Button::new("Speichern unter …"))
                            .clicked()
                        {
                            *result = CorrespondenceResult::SaveAs(entry.id);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Entfernen …").clicked() {
                            self.selected = Some(entry.id);
                            self.confirm_remove = Some(entry.id);
                            ui.close();
                        }
                    });
                    if response.double_clicked() && entry.has_pdf() {
                        self.selected = Some(entry.id);
                        *result = CorrespondenceResult::OpenPdf(entry.id);
                    }
                }
            });
    }

    /// Bemerkung and the buttons for the selected entry; the question before
    /// removing.
    fn selection_row(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        result: &mut CorrespondenceResult,
    ) {
        let Some(entry) = self
            .selected
            .and_then(|id| project.correspondence_entry(id))
            .cloned()
        else {
            // The buttons stay in their place, greyed out (Anwender 18.11.).
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        "Ein Schreiben wählen, um es zu öffnen, zu speichern oder zu entfernen.",
                    )
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (icon, label) in [
                        (Icon::Trash, "Entfernen …"),
                        (Icon::Save, "Speichern unter …"),
                        (Icon::Document, "Öffnen"),
                    ] {
                        widgets::list_button(ui, palette, Some(icon), label, false, CHOOSE_FIRST);
                    }
                });
            });
            return;
        };
        if self.confirm_remove == Some(entry.id) {
            ui.horizontal(|ui| {
                // Wraps within the dialog; the buttons keep their place on the right.
                let width = (ui.available_width() - 240.0).max(200.0);
                ui.allocate_ui(egui::vec2(width, SELECTION_HEIGHT), |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{} mit dem PDF aus dem Projekt entfernen? Strg+Z holt es zurück.",
                                row_label(&entry)
                            ))
                            .size(12.5)
                            .color(palette.text),
                        )
                        .wrap(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Abbrechen").clicked() {
                        self.confirm_remove = None;
                    }
                    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if widgets::danger_button(ui, palette, "Entfernen", 100.0).clicked() || enter {
                        let before = project.correspondence_snapshot();
                        project.remove_correspondence(entry.id);
                        self.confirm_remove = None;
                        self.selected = None;
                        *result = CorrespondenceResult::Changed {
                            before,
                            label: "Schreiben entfernen",
                            join: false,
                        };
                    }
                });
            });
            return;
        }
        crate::widgets::row(ui, |ui| {
            ui.label(
                RichText::new("Bemerkung")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            let mut remark = entry.remark.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut remark)
                    .id_salt("correspondence_remark")
                    .desired_width(430.0)
                    .hint_text("z. B. per Mail am …, Einschreiben"),
            );
            if response.changed() {
                let before = project.correspondence_snapshot();
                project.set_correspondence_remark(entry.id, &remark);
                *result = CorrespondenceResult::Changed {
                    before,
                    label: "Bemerkung im Schriftverkehr",
                    join: true,
                };
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_label_button(ui, palette, Icon::Trash, "Entfernen …")
                    .on_hover_text(
                        "Schreiben mit seinem PDF aus dem Projekt entfernen (Entf) – mit Rückfrage",
                    )
                    .clicked()
                {
                    self.confirm_remove = Some(entry.id);
                }
                ui.add_enabled_ui(entry.has_pdf(), |ui| {
                    if widgets::icon_label_button(ui, palette, Icon::Save, "Speichern unter …")
                        .on_hover_text("Das PDF als Datei speichern, z. B. zum Versenden")
                        .clicked()
                    {
                        *result = CorrespondenceResult::SaveAs(entry.id);
                    }
                    if widgets::icon_label_button(ui, palette, Icon::Document, "Öffnen")
                        .on_hover_text(
                            "Das PDF in der PDF-Anzeige des Systems öffnen (Kopie im Temp-Ordner)",
                        )
                        .clicked()
                    {
                        *result = CorrespondenceResult::OpenPdf(entry.id);
                    }
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{LetterKind, Lv, OzMask, ProjectName};

    #[test]
    fn rows_are_named_and_filtered() {
        let mut project = Project::new(
            ProjectName::parse("P").unwrap(),
            "2026",
            Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        for (kind, who, date) in [
            (
                CorrespondenceKind::Letter(LetterKind::Rejection),
                "Müller GmbH",
                "18.11.2026",
            ),
            (
                CorrespondenceKind::Letter(LetterKind::Award),
                "Schmidt KG",
                "19.11.2026",
            ),
            (CorrespondenceKind::Own, "", ""),
        ] {
            let mut entry = CorrespondenceEntry::new(kind, date, "Betreff", vec![1]);
            entry.recipient_name = who.into();
            project.add_correspondence(entry);
        }
        let mut dialog = CorrespondenceDialog::new();
        let shown: Vec<String> = dialog
            .shown(&project)
            .iter()
            .map(|entry| row_label(entry))
            .collect();
        assert_eq!(
            shown,
            [
                "Zuschlag an Schmidt KG vom 19.11.2026",
                "Absage an Müller GmbH vom 18.11.2026",
                "Eigenes Schreiben an (ohne Empfänger) (ohne Datum)"
            ]
        );
        dialog.kind = Some(CorrespondenceKind::Letter(LetterKind::Rejection));
        assert_eq!(dialog.shown(&project).len(), 1);
        dialog.kind = None;
        let dialog = dialog.with_recipient(Some("Schmidt KG".into()));
        assert_eq!(dialog.shown(&project)[0].recipient_name, "Schmidt KG");
    }
}
