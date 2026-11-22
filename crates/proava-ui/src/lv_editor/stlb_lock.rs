//! Schreibschutz für Positionen mit STLB-Bau-Kennung (Anwender 18.11.): der
//! Langtext (Text, Formatierung, Bilder, neue Angaben) und die Einheit kommen
//! aus STLB-Bau und sind gesperrt; Kurztext, Menge, Preise, KG, LB, Status
//! und die Werte der Textergänzungen bleiben frei. Ändern geht über STLB-Bau
//! (Prüfdatei, Aktualisieren) oder „In freie Position umwandeln“ – dann ist
//! die Kennung weg.

use eframe::egui::{self, RichText, Sense, Stroke};
use proava_domain::stlb::{key_line, STLB_CONVERT_QUESTION, STLB_LOCKED};
use proava_domain::{LvRow, Position, Project, RowId, TextComplement};

use super::{Dialog, EditorOutput, LvEditor};
use crate::theme::Palette;
use crate::widgets;

/// Short line for the frame above a locked Langtext.
pub const LOCKED_TITLE: &str = "Langtext aus STLB-Bau – schreibgeschützt (Kurztext frei)";
/// Button that removes the key.
pub const TO_FREE: &str = "In freie Position umwandeln …";

/// Whether row `index` is a position with a write-protected STLB-Bau text.
pub(crate) fn text_locked(project: &Project, index: usize) -> bool {
    project
        .lv()
        .rows()
        .get(index)
        .and_then(LvRow::as_position)
        .is_some_and(Position::is_text_locked)
}

/// Frame above the Langtext of a locked position: key with version, the
/// protection and „In freie Position umwandeln …“; true when clicked.
pub(super) fn banner(ui: &mut egui::Ui, palette: &Palette, position: &Position) -> bool {
    let mut convert = false;
    egui::Frame::new()
        .fill(palette.accent_soft)
        .stroke(Stroke::new(1.0, palette.line))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            // As wide as the fields below it.
            ui.set_min_width(ui.available_width());
            if let Some(line) = key_line(position) {
                ui.label(RichText::new(line).size(12.5).color(palette.text));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(LOCKED_TITLE)
                        .size(12.0)
                        .color(palette.text_dim),
                )
                .on_hover_text(STLB_LOCKED);
                if ui
                    .small_button(TO_FREE)
                    .on_hover_text(STLB_CONVERT_QUESTION)
                    .clicked()
                {
                    convert = true;
                }
            });
            if position.stlb_text_changed() {
                ui.label(
                    RichText::new(proava_domain::stlb::STLB_CHANGED)
                        .size(12.0)
                        .color(palette.warn),
                );
            }
        });
    ui.add_space(6.0);
    convert
}

/// Shows `add` disabled (it looks locked and takes no input); a click on it
/// is answered with the explanation. Returns what `add` returned and the
/// toast for a click.
pub(super) fn locked_view<R>(
    ui: &mut egui::Ui,
    id: egui::Id,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, Option<String>) {
    let inner = ui.add_enabled_ui(false, add);
    // Not a stop for Tab (Anwender 18.11. 23:34: Tab from the Menge must reach
    // the EP): the area takes the focus only once it was clicked.
    let area = id.with("stlb_locked");
    let clicked_before = ui
        .ctx()
        .data(|data| data.get_temp::<egui::Id>(focus_mark()))
        == Some(area)
        && ui.ctx().memory(|memory| memory.has_focus(area));
    let sense = if clicked_before {
        Sense::click()
    } else {
        Sense::CLICK
    };
    let click = ui
        .interact(inner.response.rect, area, sense)
        .on_hover_text(STLB_LOCKED);
    if click.clicked() {
        // The locked area takes the focus: typing goes nowhere – not into
        // the field that had it before, not into a table cell (the table
        // asks [`locked_area_focused`]).
        click.request_focus();
        ui.ctx()
            .data_mut(|data| data.insert_temp(focus_mark(), click.id));
    }
    let typed = click.has_focus()
        && ui.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Text(_) | egui::Event::Paste(_)))
        });
    let toast = (click.clicked() || typed).then(|| STLB_LOCKED.to_string());
    (inner.inner, toast)
}

fn focus_mark() -> egui::Id {
    egui::Id::unique("proava_stlb_locked_focus")
}

/// Whether a write-protected Langtext has the keyboard focus (clicked into
/// it): typed text then belongs to nobody.
pub(crate) fn locked_area_focused(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<egui::Id>(focus_mark()))
        .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
}

/// `add` as it is, or – when `locked` – disabled with the explanation on a
/// click (see [`locked_view`]).
pub(super) fn locked_or_free<R>(
    ui: &mut egui::Ui,
    id: egui::Id,
    locked: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, Option<String>) {
    if locked {
        locked_view(ui, id, add)
    } else {
        (add(ui), None)
    }
}

/// The values of the Textergänzungen of a locked text, editable one by one
/// (Vortext and Nachtext as labels). Some(new list) when a value changed.
pub(super) fn complement_values(
    ui: &mut egui::Ui,
    palette: &Palette,
    position: &Position,
    salt: &str,
) -> Option<Vec<TextComplement>> {
    if position.complements().is_empty() {
        return None;
    }
    let mut list = position.complements().to_vec();
    let mut changed = false;
    ui.label(
        RichText::new("Werte der Textergänzungen (frei)")
            .size(12.0)
            .color(palette.text_dim),
    );
    for (number, complement) in list.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            let caption = complement.caption.trim();
            let kind = complement.kind_name();
            ui.label(
                RichText::new(if caption.is_empty() { kind } else { caption })
                    .size(12.5)
                    .color(palette.text),
            )
            .on_hover_text(format!("{kind} {}", complement.marker.trim()));
            let response = ui.add(
                egui::TextEdit::singleline(&mut complement.body)
                    .id(egui::Id::unique((
                        salt,
                        "stlb_value",
                        position.id(),
                        number,
                    )))
                    .desired_width(220.0)
                    .hint_text("…………"),
            );
            changed |= response.changed();
            let tail = complement.tail.trim();
            if !tail.is_empty() {
                ui.label(RichText::new(tail).size(12.5).color(palette.text_dim));
            }
        });
    }
    ui.add_space(4.0);
    changed.then_some(list)
}

/// Positions whose Langtext differs from STLB-Bau („STLB-Bau*“).
pub fn changed_stlb_positions(lv: &proava_domain::Lv) -> Vec<RowId> {
    lv.positions()
        .filter(|position| position.is_text_locked() && position.stlb_text_changed())
        .map(Position::id)
        .collect()
}

impl LvEditor {
    /// Sofortaktion of the LV check: every „STLB-Bau*“ becomes a free
    /// position (one undo step).
    pub fn changed_stlb_to_free(&mut self, project: &mut Project, today: &str) -> EditorOutput {
        let ids = changed_stlb_positions(project.lv());
        self.convert_free_now(project, &ids, today)
    }

    /// Langtext of an STLB-Bau position in the edit panel: the editor shown
    /// disabled (a click explains it), below it the values of the
    /// Textergänzungen (free).
    pub(super) fn locked_long_text(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        text: proava_domain::RichText,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = position.id();
        // The values first (free, like in the Eigenschaften window), then
        // the text as delivered.
        if let Some(edited) = complement_values(ui, palette, &position, "panel") {
            self.store_complement_values(project, index, &position, &edited, output);
            return;
        }
        let mut shown = text;
        let mut list = position.complements().to_vec();
        let images = project.images();
        let editor = &mut self.long_text;
        let (result, toast) = locked_view(ui, egui::Id::unique(("panel_locked", id)), |ui| {
            editor.show_with_complements(ui, palette, &mut shown, &mut list, images)
        });
        if result.changed {
            // Nothing typed arrives; the editor loads the text again.
            self.long_text.reset();
        }
        if let Some(toast) = toast {
            output.toast = Some(toast);
        }
    }

    /// Stores new values of the Textergänzungen of a locked position (the
    /// rest of the list stays as delivered).
    pub(super) fn store_complement_values(
        &mut self,
        project: &mut Project,
        index: usize,
        position: &Position,
        edited: &[TextComplement],
        output: &mut EditorOutput,
    ) {
        match position.locked_complement_values(edited) {
            Ok(list) if list.as_slice() != position.complements() => {
                self.undo.record(
                    project.lv(),
                    Some(crate::undo::EditKey(format!("values:{}", position.id().0))),
                );
                if let Some(LvRow::Position(target)) = project.lv_mut().row_mut(index) {
                    target.set_complements(list);
                }
                self.long_text.reset();
                output.dirty = true;
            }
            Ok(_) => {}
            Err(error) => output.toast = Some(error),
        }
    }

    /// „In freie Position umwandeln …“ for `ids` (titles: their blocks):
    /// asks first.
    pub(super) fn request_free(&mut self, project: &Project, ids: &[RowId]) -> EditorOutput {
        let targets = project.lv().text_locked_positions(ids);
        if targets.is_empty() {
            return EditorOutput::toast("Keine Position mit STLB-Bau-Kennung gewählt.");
        }
        self.dialog = Some(Dialog::ConvertFree(targets));
        EditorOutput::default()
    }

    /// Removes the key of `ids` (one undo step); `today` TT.MM.JJJJ for the
    /// note in the Textquelle.
    pub(super) fn convert_free_now(
        &mut self,
        project: &mut Project,
        ids: &[RowId],
        today: &str,
    ) -> EditorOutput {
        let before = project.lv().clone();
        let count = project.lv_mut().convert_to_free(ids, today);
        if count == 0 {
            return EditorOutput::toast("Keine Position mit STLB-Bau-Kennung gewählt.");
        }
        self.undo.record(&before, None);
        self.reload();
        EditorOutput {
            dirty: true,
            toast: Some(format!(
                "{} in freie Position umgewandelt – der Langtext ist frei, Strg+Z macht es rückgängig.",
                crate::format::count(count, "Position", "Positionen")
            )),
            ..EditorOutput::default()
        }
    }

    /// The question before „In freie Position umwandeln“; true while shown.
    pub(super) fn free_dialog(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) -> bool {
        if !matches!(self.dialog, Some(Dialog::ConvertFree(_))) {
            return false;
        }
        let Some(Dialog::ConvertFree(ids)) = self.dialog.take() else {
            return false;
        };
        let lv = project.lv();
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| lv.position_by_id(*id))
            .take(5)
            .map(|position| format!("{} {}", position.ordinal(), position.short_text()))
            .collect();
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 520.0, |ui| {
            widgets::title(
                ui,
                palette,
                "In freie Position umwandeln?",
                &format!(
                    "{} – Strg+Z macht es rückgängig.",
                    crate::format::count(ids.len(), "Position", "Positionen")
                ),
            );
            ui.label(
                RichText::new(STLB_CONVERT_QUESTION)
                    .size(12.5)
                    .color(palette.text),
            );
            ui.label(
                RichText::new(
                    "Der Text bleibt, wie er ist; die Textquelle vermerkt die Umwandlung.",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
            for name in &names {
                ui.label(
                    RichText::new(format!("• {name}"))
                        .size(12.5)
                        .color(palette.text),
                );
            }
            if ids.len() > names.len() {
                ui.label(format!("… und {} weitere", ids.len() - names.len()));
            }
            ui.add_space(12.0);
            widgets::dialog_buttons(ui, palette, "Umwandeln")
        });
        if close || cancelled {
            return true;
        }
        if confirmed || enter {
            let today = crate::date_picker::today(ui.ctx()).to_string();
            output.merge(self.convert_free_now(project, &ids, &today));
            return true;
        }
        self.dialog = Some(Dialog::ConvertFree(ids));
        true
    }
}
