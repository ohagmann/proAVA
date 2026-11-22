//! Draws a [`Protocol`] (import protocol, preview, „Angebote eingelesen“,
//! hints after an export): two columns „Bezeichnung | Wert“, one fact per
//! line. The label column is narrow and grey, values are left-aligned,
//! numbers right-aligned to one edge, paths stay on one line – shortened in
//! the middle with „…“ and the full path as tooltip only when even the
//! widest dialog is too narrow. Details are a list under their line.
//!
//! A dialog takes its width from [`dialog_width`]: as wide as the longest
//! value needs (paths in one line), at most about 90 % of the window.

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Vec2};
use proava_application::{Protocol, ProtocolEntry, Tone, ValueKind};

use crate::theme::Palette;

const LABEL_SIZE: f32 = 12.5;
const VALUE_SIZE: f32 = 13.0;
const DETAIL_SIZE: f32 = 12.0;
/// Space between the label and the value column.
const GAP: f32 = 18.0;
/// Labels wider than this wrap.
const LABEL_MAX: f32 = 230.0;
/// Text values wider than this wrap instead of widening the dialog.
const TEXT_MAX: f32 = 520.0;
/// Inner margin of [`crate::widgets::modal`] on each side.
const MODAL_MARGIN: f32 = 18.0;
/// Share of the window a protocol dialog may take.
const MAX_SHARE: f32 = 0.9;

/// Widths of one set of entries.
#[derive(Debug, Clone, Copy, Default)]
struct Widths {
    label: f32,
    /// Widest number (they share one right edge).
    number: f32,
    /// Widest value the dialog should make room for (paths uncut, text up
    /// to [`TEXT_MAX`]).
    value: f32,
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE)
        .size()
        .x
}

fn widths<'a>(ui: &egui::Ui, entries: impl IntoIterator<Item = &'a ProtocolEntry>) -> Widths {
    let mut widths = Widths::default();
    for entry in entries {
        widths.label = widths
            .label
            .max(text_width(ui, &entry.label, LABEL_SIZE).min(LABEL_MAX));
        let value = text_width(ui, &entry.value, VALUE_SIZE);
        match entry.kind {
            ValueKind::Number => widths.number = widths.number.max(value),
            ValueKind::Path => widths.value = widths.value.max(value),
            ValueKind::Text => widths.value = widths.value.max(value.min(TEXT_MAX)),
        }
        for detail in &entry.details {
            let width = text_width(ui, &format!("– {detail}"), DETAIL_SIZE).min(TEXT_MAX);
            widths.value = widths.value.max(width);
        }
    }
    widths.value = widths.value.max(widths.number);
    widths
}

/// Width of a dialog that shows `entries` (and perhaps more): room for the
/// longest value – a path in one line –, at least `min`, at most about 90 %
/// of the window.
pub fn dialog_width<'a>(
    ui: &egui::Ui,
    entries: impl IntoIterator<Item = &'a ProtocolEntry>,
    min: f32,
) -> f32 {
    let widths = widths(ui, entries);
    let wanted = widths.label + GAP + widths.value + 4.0;
    let max = ui.ctx().content_rect().width() * MAX_SHARE - 2.0 * MODAL_MARGIN;
    let min = min.min(max);
    wanted.clamp(min, max.max(min))
}

/// Draws the entries in the available width.
pub fn show(ui: &mut egui::Ui, palette: &Palette, entries: &[ProtocolEntry]) {
    let widths = widths(ui, entries.iter());
    let value_width = (ui.available_width() - widths.label - GAP).max(80.0);
    for entry in entries {
        row(ui, palette, entry, widths, value_width);
    }
}

/// Title, hint line and entries of a protocol (the dialog adds its buttons).
pub fn show_protocol(ui: &mut egui::Ui, palette: &Palette, protocol: &Protocol, max_height: f32) {
    crate::widgets::title(ui, palette, &protocol.title, &protocol.hint);
    egui::ScrollArea::vertical()
        .id_salt(("protocol", protocol.title.as_str()))
        .max_height(max_height)
        .show(ui, |ui| show(ui, palette, &protocol.entries));
}

fn row(
    ui: &mut egui::Ui,
    palette: &Palette,
    entry: &ProtocolEntry,
    widths: Widths,
    value_width: f32,
) {
    let color = match entry.tone {
        Tone::Normal => palette.text,
        Tone::Warning => palette.warn,
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(
            Vec2::new(widths.label + GAP, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(widths.label + GAP);
                ui.add_space(1.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&entry.label)
                            .size(LABEL_SIZE)
                            .color(palette.text_dim),
                    )
                    .wrap(),
                );
            },
        );
        ui.allocate_ui_with_layout(
            Vec2::new(value_width, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(value_width);
                let text = |value: &str| RichText::new(value).size(VALUE_SIZE).color(color);
                match entry.kind {
                    ValueKind::Number => {
                        // Right edge shared by all numbers, same height as a text line.
                        let galley = ui.painter().layout_no_wrap(
                            entry.value.clone(),
                            FontId::proportional(VALUE_SIZE),
                            color,
                        );
                        let size = Vec2::new(widths.number.max(galley.size().x), galley.size().y);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
                        let value = entry.value.clone();
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Label, true, &value)
                        });
                        ui.painter().galley(
                            egui::pos2(rect.right() - galley.size().x, rect.top()),
                            galley,
                            color,
                        );
                    }
                    ValueKind::Path => {
                        let (shown, cut) = fit_middle(ui, &entry.value, value_width - 2.0);
                        let response = ui.add(egui::Label::new(text(&shown)).extend());
                        if cut {
                            response.on_hover_text(&entry.value);
                        }
                    }
                    ValueKind::Text => {
                        ui.add(egui::Label::new(text(&entry.value)).wrap());
                    }
                }
                let detail_color = match entry.tone {
                    Tone::Normal => palette.text_dim,
                    Tone::Warning => palette.warn,
                };
                for detail in &entry.details {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("– {detail}"))
                                .size(DETAIL_SIZE)
                                .color(detail_color),
                        )
                        .wrap(),
                    );
                }
            },
        );
    });
    ui.add_space(4.0);
}

/// `text` as it fits `width` on one line: unchanged, or start and end with
/// „…“ in between (measured, not counted). Returns whether it was cut.
pub fn fit_middle(ui: &egui::Ui, text: &str, width: f32) -> (String, bool) {
    if text_width(ui, text, VALUE_SIZE) <= width {
        return (text.to_string(), false);
    }
    let characters: Vec<char> = text.chars().collect();
    let shortened = |keep: usize| {
        let head = keep.div_ceil(2);
        let tail = keep / 2;
        let start: String = characters[..head].iter().collect();
        let end: String = characters[characters.len() - tail..].iter().collect();
        format!("{start}…{end}")
    };
    let (mut low, mut high) = (0, characters.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if text_width(ui, &shortened(middle), VALUE_SIZE) <= width {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    (shortened(low), true)
}

/// „Fertig“ and „Kopieren“ under a protocol (`text`: one „Bezeichnung:
/// Wert“ per line, see [`Protocol::text`]); returns whether „Fertig“ was
/// clicked.
pub fn buttons(ui: &mut egui::Ui, palette: &Palette, text: &str, copied: &mut bool) -> bool {
    let mut done = false;
    crate::widgets::button_row(ui, |ui| {
        if ui
            .add(
                egui::Button::new(RichText::new(crate::strings::DONE).color(palette.on_accent))
                    .fill(palette.accent)
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            done = true;
        }
        ui.add_space(8.0);
        let label = if *copied { "Kopiert" } else { "Kopieren" };
        if ui
            .add(egui::Button::new(label).min_size(egui::vec2(120.0, 32.0)))
            .on_hover_text("Als Text in die Zwischenablage (je Zeile „Bezeichnung: Wert“)")
            .clicked()
        {
            ui.ctx().copy_text(text.to_string());
            *copied = true;
        }
    });
    done
}

/// A dialog that only shows `protocol`: width from its values, „Fertig“
/// and „Kopieren“. Returns whether it stays open (Fertig, Esc, Enter close).
pub fn dialog(
    ui: &mut egui::Ui,
    palette: &Palette,
    protocol: &Protocol,
    copied: &mut bool,
) -> bool {
    let width = dialog_width(ui, protocol.entries.iter(), 620.0);
    let max_height = (ui.ctx().content_rect().height() * 0.6).max(200.0);
    let (done, close) = crate::widgets::modal(ui, palette, width, |ui| {
        show_protocol(ui, palette, protocol, max_height);
        ui.add_space(12.0);
        buttons(ui, palette, &protocol.text(), copied)
    });
    let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
    !(done || close || enter)
}
