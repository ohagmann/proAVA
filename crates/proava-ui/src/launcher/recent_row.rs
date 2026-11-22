//! One entry of the list „Zuletzt verwendet“ on the start screen.
//!
//! Name and path get the room that is left beside the right-hand part
//! („zuletzt geöffnet …“, or „Datei fehlt“ with „Pfad suchen …“ and
//! „Entfernen“); they are shortened with „…“ and show the full text as
//! tooltip. Is the row too narrow for both, the right-hand part moves onto
//! lines of its own below the path. Nothing overlaps, nothing leaves the row.

use eframe::egui::{self, Align, FontId, Layout, Rect, RichText, Sense, Stroke, UiBuilder, Vec2};
use proava_application::RecentProject;

use crate::icons::Icon;
use crate::theme::Palette;
use crate::{format, strings, widgets};

/// Height of name and path (the row without extra lines).
pub(super) const ROW_HEIGHT: f32 = 56.0;
/// Name and path start right of the document symbol.
const TEXT_LEFT: f32 = 44.0;
/// Room on the right (also for the scroll bar that lies over the list).
const PAD_RIGHT: f32 = 16.0;
/// Space between text and the right-hand part, and between its items.
const GAP: f32 = 10.0;
/// Narrowest room for name and path before the right-hand part moves below.
const MIN_TEXT_WIDTH: f32 = 220.0;
/// Height of a line of the right-hand part when it stands below the text.
const EXTRA_LINE: f32 = 34.0;
/// Height of a button.
const BUTTON_HEIGHT: f32 = 28.0;
const NAME_SIZE: f32 = 14.0;
const PATH_SIZE: f32 = 11.5;
const NOTE_SIZE: f32 = 12.5;

/// Where the parts of a row go, relative to its top left corner.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RowLayout {
    pub height: f32,
    /// Width for name and path.
    pub text_width: f32,
    /// Top left corner and width of each right-hand item, in the order given.
    pub items: Vec<(Vec2, f32)>,
}

/// Leftmost start of a right-hand item in a very narrow row.
const NARROW_LEFT: f32 = 12.0;

/// Lays out a row of `width` whose right-hand part consists of items of these
/// widths (all `BUTTON_HEIGHT` high): beside the text if that keeps
/// `MIN_TEXT_WIDTH` for it, else in lines below the path (left-aligned under
/// the text, wrapping when a line is full; an item that does not fit there
/// starts further left and is shortened to the row).
pub(super) fn row_layout(width: f32, items: &[f32]) -> RowLayout {
    let block = items.iter().sum::<f32>() + GAP * items.len().saturating_sub(1) as f32;
    let beside = width - TEXT_LEFT - PAD_RIGHT - GAP - block;
    if beside >= MIN_TEXT_WIDTH || items.is_empty() {
        let mut x = width - PAD_RIGHT - block;
        let y = (ROW_HEIGHT - BUTTON_HEIGHT) / 2.0;
        let positions = items
            .iter()
            .map(|item| {
                let at = (Vec2::new(x, y), *item);
                x += item + GAP;
                at
            })
            .collect();
        return RowLayout {
            height: ROW_HEIGHT,
            text_width: beside.max(0.0),
            items: positions,
        };
    }
    let right = width - PAD_RIGHT;
    let top = ROW_HEIGHT - 6.0;
    let mut line = 0usize;
    let mut x = TEXT_LEFT;
    let mut positions = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        if index > 0 && x + item > right {
            line += 1;
            x = TEXT_LEFT;
        }
        // Does not fit even at the start of a line: further left, at most the row.
        let (start, item_width) = if x + item > right {
            let start = (right - item).max(NARROW_LEFT).min(x);
            (start, item.min(right - start).max(0.0))
        } else {
            (x, *item)
        };
        let y = top + line as f32 * EXTRA_LINE + (EXTRA_LINE - BUTTON_HEIGHT) / 2.0;
        positions.push((Vec2::new(start, y), item_width));
        x = start + item_width + GAP;
    }
    RowLayout {
        height: top + (line + 1) as f32 * EXTRA_LINE + 6.0,
        text_width: (width - TEXT_LEFT - PAD_RIGHT).max(0.0),
        items: positions,
    }
}

/// `text` shortened in the middle („/tmp/…/Projekt.proava“) so that it fits
/// into `max_width` in `font`; the second value tells whether it was shortened.
fn fit_middle(ui: &egui::Ui, text: &str, font: &FontId, max_width: f32) -> (String, bool) {
    let width = |candidate: &str| {
        ui.painter()
            .layout_no_wrap(candidate.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    if width(text) <= max_width {
        return (text.to_string(), false);
    }
    // Longest shortening that still fits (binary search over the kept characters).
    let total = text.chars().count();
    let (mut low, mut high) = (1usize, total);
    while low < high {
        let middle = (low + high).div_ceil(2);
        if width(&format::truncate_middle(text, middle)) <= max_width {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    (format::truncate_middle(text, low), true)
}

fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_string(),
            FontId::proportional(size),
            egui::Color32::WHITE,
        )
        .size()
        .x
}

/// Width of a button with this text (text, inner padding and some air).
fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    (text_width(ui, text, NOTE_SIZE) + 2.0 * ui.spacing().button_padding.x + 16.0).ceil()
}

/// A child area at `rect` that does not move the list's cursor.
fn place<R>(ui: &mut egui::Ui, rect: Rect, clip: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.set_clip_rect(rect.intersect(clip));
    add(&mut child)
}

/// What the user did in a row besides selecting/opening it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RowClicks {
    pub remove: bool,
    pub locate: bool,
}

/// One entry of the recent list.
pub(super) fn recent_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    item: &RecentProject,
    selected: bool,
    missing: bool,
) -> (egui::Response, RowClicks) {
    const LOCATE: &str = "Pfad suchen …";
    const REMOVE: &str = "Entfernen";
    const MISSING: &str = "Datei fehlt";
    let opened = format!(
        "zuletzt geöffnet {}",
        super::german_date_time(&item.opened_at)
    );
    let widths: Vec<f32> = if missing {
        vec![
            text_width(ui, MISSING, NOTE_SIZE).ceil() + 2.0,
            button_width(ui, LOCATE),
            button_width(ui, REMOVE),
        ]
    } else {
        vec![text_width(ui, &opened, 12.0).ceil() + 2.0]
    };
    let width = ui.available_width();
    let layout = row_layout(width, &widths);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, layout.height), Sense::click());
    widgets::name_button(ui, &response, &item.name);
    let clip = ui.clip_rect();

    let background = if selected {
        palette.selected
    } else if response.hovered() {
        palette.row_hover
    } else {
        palette.row_alt
    };
    let painter = ui.painter().clone();
    painter.rect_filled(rect, egui::CornerRadius::same(8), background);
    if selected {
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            Stroke::new(1.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    let (main, dim) = if missing {
        (palette.text_dim, palette.text_dim.gamma_multiply(0.8))
    } else {
        (palette.text, palette.text_dim)
    };
    crate::icons::draw(
        &painter,
        Rect::from_center_size(
            egui::pos2(rect.left() + 24.0, rect.top() + ROW_HEIGHT / 2.0),
            Vec2::splat(19.0),
        ),
        Icon::Document,
        if missing {
            palette.text_dim
        } else {
            palette.accent
        },
        background,
    );

    // Name and path: real labels (shortened, tooltip with the full text).
    let text_rect = |top: f32, height: f32| {
        Rect::from_min_size(
            egui::pos2(rect.left() + TEXT_LEFT, rect.top() + top),
            Vec2::new(layout.text_width, height),
        )
    };
    place(ui, text_rect(9.0, 20.0), clip, |ui| {
        ui.add(
            egui::Label::new(RichText::new(&item.name).size(NAME_SIZE).color(main))
                .selectable(false)
                .truncate(),
        )
    });
    let full_path = item.path.to_string_lossy().to_string();
    let path_font = FontId::proportional(PATH_SIZE);
    let (shown, _) = fit_middle(ui, &full_path, &path_font, layout.text_width - 2.0);
    place(ui, text_rect(30.0, 18.0), clip, |ui| {
        ui.add(egui::Label::new(RichText::new(shown).size(PATH_SIZE).color(dim)).selectable(false))
            .on_hover_text(&full_path)
    });

    let item_rect = |index: usize| {
        let (at, item_width) = layout.items[index];
        Rect::from_min_size(rect.min + at, Vec2::new(item_width, BUTTON_HEIGHT))
    };
    let mut clicks = RowClicks::default();
    if missing {
        place(ui, item_rect(0), clip, |ui| {
            ui.add(
                egui::Label::new(RichText::new(MISSING).size(NOTE_SIZE).color(palette.warn))
                    .selectable(false)
                    .truncate(),
            )
        });
        clicks.locate = place(ui, item_rect(1), clip, |ui| {
            ui.add(
                egui::Button::new(RichText::new(LOCATE).size(NOTE_SIZE))
                    .truncate()
                    .min_size(Vec2::new(layout.items[1].1, BUTTON_HEIGHT)),
            )
            .on_hover_text("Die Datei an ihrem neuen Ort wählen; der Eintrag zeigt dann dorthin.")
            .clicked()
        });
        clicks.remove = place(ui, item_rect(2), clip, |ui| {
            ui.add(
                egui::Button::new(RichText::new(REMOVE).size(NOTE_SIZE))
                    .truncate()
                    .min_size(Vec2::new(layout.items[2].1, BUTTON_HEIGHT)),
            )
            .on_hover_text(strings::FORGET_PROJECT)
            .clicked()
        });
    } else {
        place(ui, item_rect(0), clip, |ui| {
            ui.add(
                egui::Label::new(RichText::new(opened).size(12.0).color(palette.text_dim))
                    .selectable(false)
                    .truncate(),
            )
        });
    }
    (
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        clicks,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect((at, width): (Vec2, f32)) -> Rect {
        Rect::from_min_size(at.to_pos2(), Vec2::new(width, BUTTON_HEIGHT))
    }

    /// Abnahme qa3 (18.11.): bei schmalem Fenster lief „zuletzt geöffnet …“
    /// über Projektname und Pfad – der Pfad wird in der Mitte gekürzt, der
    /// Dateiname bleibt sichtbar, ein kurzer Pfad bleibt, wie er ist.
    #[test]
    fn path_is_shortened_in_the_middle_to_the_space_left() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let font = FontId::proportional(PATH_SIZE);
            let path = "/tmp/test/qa3/home/projekte/Schulzentrum Musterstraße QA3/Schulzentrum Musterstraße QA3.proava";
            let (fitted, shortened) = fit_middle(ui, path, &font, 240.0);
            assert!(shortened);
            assert!(text_width(ui, &fitted, PATH_SIZE) <= 240.5, "{fitted}");
            assert!(fitted.ends_with("QA3.proava"), "Dateiname bleibt sichtbar: {fitted}");
            assert_eq!(fit_middle(ui, "/tmp/a.proava", &font, 240.0), ("/tmp/a.proava".to_string(), false));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn wide_rows_keep_the_right_part_beside_the_text() {
        let items = [70.0, 110.0, 90.0];
        let layout = row_layout(1200.0, &items);
        assert_eq!(layout.height, ROW_HEIGHT);
        let text_end = TEXT_LEFT + layout.text_width;
        assert!(layout.items[0].0.x >= text_end, "{layout:?}");
        assert!(
            rect(layout.items[2]).right() <= 1200.0 - PAD_RIGHT + 0.01,
            "{layout:?}"
        );
        assert!(!rect(layout.items[0]).intersects(rect(layout.items[1])));
        assert!(!rect(layout.items[1]).intersects(rect(layout.items[2])));
    }

    #[test]
    fn narrow_rows_put_the_right_part_below_and_wrap() {
        for items in [vec![70.0, 110.0, 90.0], vec![180.0]] {
            for width in [120.0, 200.0, 300.0, 400.0, 560.0] {
                let layout = row_layout(width, &items);
                if layout.height == ROW_HEIGHT {
                    // Room enough beside the text (one short item, wider row).
                    assert!(layout.text_width >= MIN_TEXT_WIDTH, "{width}: {layout:?}");
                    continue;
                }
                for (index, item) in layout.items.iter().enumerate() {
                    let item = rect(*item);
                    assert!(item.top() >= ROW_HEIGHT - 6.0, "{width}: unter dem Pfad");
                    assert!(item.bottom() <= layout.height, "{width}: in der Zeile");
                    assert!(
                        item.left() >= 0.0 && item.right() <= width - PAD_RIGHT + 0.01,
                        "{width}: {layout:?}"
                    );
                    for other in layout.items.iter().skip(index + 1) {
                        assert!(!item.intersects(rect(*other)), "{width}: {layout:?}");
                    }
                }
            }
        }
    }
}
