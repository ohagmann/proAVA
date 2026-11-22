//! Text fields that check their value when they are left: the typed text
//! stays (red, with the message) until it is valid or the row changes.

use eframe::egui::{self, FontId, RichText, Stroke};

use crate::theme::Palette;

#[derive(Clone, Default)]
struct Buffer {
    text: String,
    error: Option<String>,
    active: bool,
}

/// Result of [`checked_field`] in this frame.
pub(super) struct Checked {
    pub response: egui::Response,
    /// The field was left with a text different from `current`.
    pub commit: Option<String>,
}

/// A single line field showing `current`; returns the typed text when the
/// field is left. Report a refused value with [`refuse`].
pub(super) fn checked_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    current: &str,
    width: f32,
    font: FontId,
) -> Checked {
    let mut buffer: Buffer = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    if !buffer.active {
        buffer.text = current.to_string();
    }
    let response = ui.add(
        egui::TextEdit::singleline(&mut buffer.text)
            .id(id)
            .desired_width(width)
            .font(font)
            .margin(egui::Margin::symmetric(6, 5)),
    );
    if response.changed() {
        buffer.active = true;
        buffer.error = None;
    }
    let mut commit = None;
    if response.lost_focus() {
        if buffer.text.trim() != current.trim() {
            commit = Some(buffer.text.clone());
        } else if buffer.error.is_none() {
            buffer.active = false;
        }
    }
    if buffer.error.is_some() {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            egui::CornerRadius::same(5),
            Stroke::new(1.5, palette.error),
            egui::StrokeKind::Outside,
        );
    }
    let error = buffer.error.clone();
    ui.data_mut(|data| data.insert_temp(id, buffer));
    if let Some(error) = error {
        // Wraps in the room next to the field instead of running out of the panel.
        ui.add(egui::Label::new(RichText::new(error).size(12.0).color(palette.error)).wrap());
    }
    Checked { response, commit }
}

/// The committed value was refused: keep the text and show `error`.
pub(super) fn refuse(ctx: &egui::Context, id: egui::Id, error: String) {
    ctx.data_mut(|data| {
        let buffer: &mut Buffer = data.get_temp_mut_or_default(id);
        buffer.active = true;
        buffer.error = Some(error);
    });
}

/// The committed value was taken: show the stored value again.
pub(super) fn accept(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| data.remove::<Buffer>(id));
}

/// Small grey caption above a field.
pub(super) fn caption(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
}

/// A plain text field that applies every change at once.
pub(super) fn text_field(ui: &mut egui::Ui, value: &mut String, id: egui::Id, hint: &str) -> bool {
    ui.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::symmetric(6, 5)),
    )
    .changed()
}
