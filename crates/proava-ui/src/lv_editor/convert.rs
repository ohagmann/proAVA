//! Zeilenart umwandeln (Anwender 18.11.): Position ↔ Hinweis (Text) ↔
//! Ausführungsbeschreibung – über Rechtsklick ▸ Bearbeiten ▸ „Umwandeln in“,
//! das Menü „Bearbeiten“, die Befehlssuche, das Feld „Zeilenart“ im
//! Bearbeitungsbereich und im Fenster „Eigenschaften“. Was die neue Art nicht
//! tragen kann, nennt eine Rückfrage vorher (Domäne: `Lv::conversion`); ein
//! Strg+Z nimmt alles zurück.

use eframe::egui::{self, RichText};
use proava_domain::{Conversion, Lv, LvRow, Project, RowId, RowKind};

use super::{Dialog, EditorOutput, LvEditor};
use crate::theme::Palette;
use crate::widgets;

/// A conversion waiting for the answer to its question.
pub(super) struct PendingConversion {
    id: RowId,
    to: RowKind,
    /// „Position 01.0010 Standardbeschreibung …“.
    name: String,
    conversion: Conversion,
    /// Order numbers of the positions that lose their reference.
    unlinked: Vec<String>,
    /// Versand und Angebote (like the warnings before „Neu nummerieren“).
    warnings: Vec<String>,
    /// The warnings name the sent X83: no second question afterwards.
    sent: bool,
}

/// „Position 01.0010 Kurztext“, „Hinweis Überschrift“, „Ausführungsbeschreibung A0001 Kabel“.
fn row_name(row: &LvRow) -> String {
    let (kind, number, title) = match row {
        LvRow::Position(position) => (
            "Position",
            position.ordinal().to_string(),
            position.short_text().to_string(),
        ),
        LvRow::Note(note) => ("Hinweis", String::new(), note.title()),
        LvRow::Performance(performance) => (
            "Ausführungsbeschreibung",
            performance.mark(),
            performance.title(),
        ),
        LvRow::Group(group) => (
            "Titel",
            group.ordinal().to_string(),
            group.short_text().to_string(),
        ),
        LvRow::Spacer(_) => ("Abstandshalter", String::new(), String::new()),
    };
    let title: String = title.chars().take(60).collect();
    [kind, number.trim(), title.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Warnings like those before „Neu nummerieren“: the X83 went out, bids
/// carry prices for the position.
pub(super) fn conversion_warnings(
    project: &Project,
    index: usize,
    conversion: &Conversion,
) -> (Vec<String>, bool) {
    let lv: &Lv = project.lv();
    let mut warnings = Vec::new();
    let positions = conversion.from == RowKind::Position || conversion.to == RowKind::Position;
    let sent = lv.request_exported().is_some();
    if let Some(iso) = lv.request_exported() {
        warnings.push(format!(
            "Die Angebotsaufforderung (X83) wurde am {} {} exportiert – nach dem Umwandeln muss das LV \
             neu versendet werden{}.",
            crate::format::iso_to_german_date(iso),
            iso.get(11..16).unwrap_or_default(),
            if positions {
                "; die Rückläufer der Bieter passen an dieser Stelle nicht mehr"
            } else {
                ""
            }
        ));
    }
    if conversion.from == RowKind::Position {
        let id = lv.rows()[index].id();
        let bids = project
            .bids()
            .iter()
            .filter(|bid| bid.price(id).is_some())
            .count();
        if bids > 0 {
            warnings.push(format!(
                "{} Preise für diese Position – sie zählen nicht mehr, solange die Zeile keine \
                 Position ist (Strg+Z stellt sie wieder her).",
                match bids {
                    1 => "1 Angebot enthält".to_string(),
                    count => format!("{count} Angebote enthalten"),
                }
            ));
        }
    }
    (warnings, sent)
}

impl LvEditor {
    /// Converts row `index` into `to` – at once when nothing is lost,
    /// otherwise after a question that names what goes.
    pub(super) fn request_conversion(
        &mut self,
        project: &mut Project,
        index: usize,
        to: RowKind,
    ) -> EditorOutput {
        let conversion = match project.lv().conversion(index, to) {
            Ok(conversion) => conversion,
            Err(error) => return EditorOutput::toast(error),
        };
        let (warnings, sent) = conversion_warnings(project, index, &conversion);
        let id = project.lv().rows()[index].id();
        if !conversion.loses_something() && warnings.is_empty() {
            return self.convert_now(project, id, to, false);
        }
        let lv = project.lv();
        let unlinked = conversion
            .unlinked
            .iter()
            .filter_map(|id| lv.position_by_id(*id))
            .map(|position| position.ordinal().to_string())
            .collect();
        self.dialog = Some(Dialog::ConvertRow(Box::new(PendingConversion {
            id,
            to,
            name: row_name(&lv.rows()[index]),
            conversion,
            unlinked,
            warnings,
            sent,
        })));
        EditorOutput::default()
    }

    /// Converts the row with `id` (one undo step); `sent_warned`: the
    /// question already named the sent X83.
    fn convert_now(
        &mut self,
        project: &mut Project,
        id: RowId,
        to: RowKind,
        sent_warned: bool,
    ) -> EditorOutput {
        let Some(index) = project.lv().index_of(id) else {
            return EditorOutput::toast("Die Zeile gibt es nicht (mehr).");
        };
        let before = project.lv().clone();
        match project.lv_mut().convert_row(index, to) {
            Ok(conversion) => {
                self.undo.record(&before, None);
                self.reload();
                self.select(project.lv(), id);
                if sent_warned {
                    self.sent_warned = true;
                }
                let lv = project.lv();
                let number = match lv.row_by_id(id) {
                    Some(LvRow::Position(position)) => format!(" {}", position.ordinal()),
                    Some(LvRow::Performance(performance)) => format!(" {}", performance.mark()),
                    _ => String::new(),
                };
                let mut toast = format!("In {}{number} umgewandelt", to.name());
                if !conversion.unlinked.is_empty() {
                    toast.push_str(&format!(
                        ", Bezug bei {} gelöst",
                        crate::format::count(conversion.unlinked.len(), "Position", "Positionen")
                    ));
                }
                toast.push_str(" – Strg+Z macht es rückgängig.");
                EditorOutput {
                    dirty: true,
                    toast: Some(toast),
                    ..EditorOutput::default()
                }
            }
            Err(error) => EditorOutput::toast(error),
        }
    }

    /// The question before a conversion; true while it is shown.
    pub(super) fn conversion_dialog(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) -> bool {
        if !matches!(self.dialog, Some(Dialog::ConvertRow(_))) {
            return false;
        }
        let Some(Dialog::ConvertRow(pending)) = self.dialog.take() else {
            return false;
        };
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 560.0, |ui| {
            widgets::title(
                ui,
                palette,
                &format!("In {} umwandeln?", pending.to.name()),
                &format!("{} – Strg+Z macht es rückgängig.", pending.name),
            );
            ui.label(
                RichText::new(
                    "Kurztext, Langtext mit Bildern, Bieterangaben, STLB-Bau-Kennung und \
                     Textquelle bleiben.",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
            if !pending.conversion.dropped.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Fällt weg (ein{} {} hat das nicht):",
                        if pending.to == RowKind::Note { "" } else { "e" },
                        pending.to.name()
                    ))
                    .size(12.5)
                    .color(palette.text),
                );
                for part in &pending.conversion.dropped {
                    ui.label(
                        RichText::new(format!("• {part}"))
                            .size(12.5)
                            .color(palette.text),
                    );
                }
            }
            if !pending.unlinked.is_empty() {
                ui.add_space(6.0);
                let what = if pending.conversion.from == RowKind::Performance {
                    "verweisen auf diese Ausführungsbeschreibung"
                } else {
                    "wiederholen den Text dieser Position"
                };
                ui.label(
                    RichText::new(format!(
                        "{} {what}; der Bezug wird gelöst: {}",
                        crate::format::count(pending.unlinked.len(), "Position", "Positionen"),
                        pending.unlinked.join(", ")
                    ))
                    .size(12.5)
                    .color(palette.warn),
                );
            }
            for warning in &pending.warnings {
                ui.add_space(4.0);
                ui.label(RichText::new(warning).size(12.5).color(palette.warn));
            }
            ui.add_space(12.0);
            widgets::dialog_buttons(ui, palette, "Umwandeln")
        });
        if close || cancelled {
            return true;
        }
        if confirmed || enter {
            output.merge(self.convert_now(project, pending.id, pending.to, pending.sent));
            // The question named Versand and bids: none from the shell.
            self.change_asked = true;
            return true;
        }
        self.dialog = Some(Dialog::ConvertRow(pending));
        true
    }
}

/// Field „Zeilenart“ (edit panel, window „Eigenschaften“): the kind of the
/// row; another choice asks for the conversion. `enabled` = false for a
/// finished position (it must be unlocked first).
pub(super) fn kind_choice(
    ui: &mut egui::Ui,
    palette: &Palette,
    current: RowKind,
    id: RowId,
    enabled: bool,
) -> Option<RowKind> {
    let mut chosen = None;
    crate::widgets::row(ui, |ui| {
        ui.label(
            RichText::new("Zeilenart")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(4.0);
        ui.add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(("row_kind", id))
                .selected_text(current.label())
                .show_ui(ui, |ui| {
                    for kind in RowKind::ALL {
                        if ui.selectable_label(kind == current, kind.label()).clicked()
                            && kind != current
                        {
                            chosen = Some(kind);
                        }
                    }
                })
                .response
                .on_hover_text(
                    "Umwandeln: Position, Hinweis (Text ohne OZ und Preis) oder \
                     Ausführungsbeschreibung – Text, Bieterangaben und STLB-Bau-Kennung bleiben",
                )
                .on_disabled_hover_text(format!(
                    "Die Position ist fertig und gesperrt – {}.",
                    proava_domain::LOCKED_HINT
                ));
        });
    });
    chosen
}
