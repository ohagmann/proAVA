//! Table of the Kostenverfolgung: a fixed header over a body that scrolls
//! both ways, rows indented by level with a triangle to fold groups and
//! LVs, cells with the value and (by choice) the deviation below it.
//! Exceeding the budget or the threshold to the previous stage shows as a
//! background, never as a symbol.

use std::collections::HashSet;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, UiBuilder, Vec2,
};
use proava_domain::{Mark, TrackDeviation, TrackReport, TrackRow, TrackRowKind, TrackStage};

use super::{amount, deviation_text, row_key};
use crate::comparison_view::Signal;
use crate::theme::Palette;

const HEAD_H: f32 = 40.0;
const TITLE_W: f32 = 270.0;
const VALUE_W: f32 = 120.0;
const NOTE_W: f32 = 240.0;
const INDENT: f32 = 16.0;

/// One column of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Col {
    Stage(TrackStage),
    Note,
}

impl Col {
    fn width(self) -> f32 {
        match self {
            Col::Note => NOTE_W,
            _ => VALUE_W,
        }
    }
}

/// Columns shown: Budget and Prognose always, the other stages when they
/// hold a value somewhere.
fn columns(report: &TrackReport) -> Vec<Col> {
    let mut columns: Vec<Col> = report.shown_stages().into_iter().map(Col::Stage).collect();
    columns.push(Col::Note);
    columns
}

/// Rows below a folded group or LV are hidden.
fn visible<'a>(report: &'a TrackReport, collapsed: &HashSet<String>) -> Vec<&'a TrackRow> {
    let mut out = Vec::new();
    let mut hide_below: Option<u8> = None;
    for row in &report.rows {
        if let Some(depth) = hide_below {
            if row.depth > depth {
                continue;
            }
            hide_below = None;
        }
        if foldable(report, row) && collapsed.contains(&row_key(row)) {
            hide_below = Some(row.depth);
        }
        out.push(row);
    }
    out.push(&report.total);
    out
}

/// Group and LV rows with members below them can fold.
fn foldable(report: &TrackReport, row: &TrackRow) -> bool {
    matches!(
        row.kind,
        TrackRowKind::Group | TrackRowKind::Lv | TrackRowKind::NoLv
    ) && report
        .rows
        .iter()
        .skip_while(|other| !std::ptr::eq(*other, row))
        .nth(1)
        .is_some_and(|next| next.depth > row.depth)
}

/// Shows the table; returns the key of a row clicked.
pub(super) fn show(
    ui: &mut egui::Ui,
    palette: &Palette,
    report: &TrackReport,
    mode: TrackDeviation,
    collapsed: &mut HashSet<String>,
    selected: Option<&str>,
) -> Option<String> {
    let signal = Signal::of(ui);
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    ui.painter()
        .rect_filled(full, CornerRadius::same(8), palette.panel);
    let columns = columns(report);
    let width = TITLE_W + columns.iter().map(|col| col.width()).sum::<f32>();
    let row_h = if mode == TrackDeviation::Off {
        24.0
    } else {
        38.0
    };
    let rows = visible(report, collapsed);
    let head = Rect::from_min_size(full.min, Vec2::new(full.width(), HEAD_H));
    let body = Rect::from_min_max(egui::pos2(full.left(), head.bottom()), full.max);

    let mut clicked = None;
    let mut toggled = None;
    let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body).id_salt("tracking_body"));
    let output = egui::ScrollArea::both()
        .id_salt("tracking_table")
        .auto_shrink([false, false])
        .show_viewport(&mut body_ui, |ui, viewport| {
            let size = Vec2::new(width.max(viewport.width()), rows.len() as f32 * row_h);
            let (content, _) = ui.allocate_exact_size(size, Sense::hover());
            let first = (viewport.top() / row_h).floor().max(0.0) as usize;
            let last = ((viewport.bottom() / row_h).ceil() as usize + 1).min(rows.len());
            let painter = ui.painter().clone();
            for (index, row) in rows.iter().enumerate().take(last).skip(first) {
                let top = content.top() + index as f32 * row_h;
                let rect =
                    Rect::from_min_size(egui::pos2(content.left(), top), Vec2::new(size.x, row_h));
                let key = row_key(row);
                let response = ui.interact(
                    rect,
                    ui.scope_id().with(("tracking_row", &key)),
                    Sense::click(),
                );
                let is_selected = selected == Some(key.as_str())
                    || (selected.is_none() && row.kind == TrackRowKind::Total);
                crate::widgets::name_row(&response, &row.title(), is_selected);
                let background = if is_selected {
                    palette.selected
                } else if row.kind == TrackRowKind::Total {
                    palette.panel_alt
                } else if matches!(
                    row.kind,
                    TrackRowKind::Group | TrackRowKind::Lv | TrackRowKind::NoLv
                ) {
                    palette.row_alt
                } else if response.hovered() {
                    palette.row_hover
                } else {
                    Color32::TRANSPARENT
                };
                painter.rect_filled(rect, CornerRadius::ZERO, background);
                painter.line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(0.5, palette.line),
                );
                // Title with indent and fold triangle.
                let indent = row.depth as f32 * INDENT;
                let strong = !matches!(row.kind, TrackRowKind::Leaf | TrackRowKind::Without);
                let font = FontId::proportional(12.5);
                let mut text_left = rect.left() + 8.0 + indent;
                if foldable(report, row) {
                    let folded = collapsed.contains(&key);
                    let center = egui::pos2(text_left + 6.0, rect.top() + 12.0);
                    crate::icons::disclosure(&painter, center, folded, palette.text_dim);
                    let triangle = Rect::from_center_size(center, Vec2::splat(18.0));
                    if response.clicked()
                        && response
                            .interact_pointer_pos()
                            .is_some_and(|pos| triangle.expand(2.0).contains(pos))
                    {
                        toggled = Some(key.clone());
                    }
                    text_left += 16.0;
                }
                let title_rect = Rect::from_min_max(
                    egui::pos2(text_left, rect.top()),
                    egui::pos2(rect.left() + TITLE_W - 6.0, rect.top() + 24.0),
                );
                crate::widgets::cell_text(
                    &painter.with_clip_rect(title_rect),
                    title_rect.left_center(),
                    Align2::LEFT_CENTER,
                    &row.title(),
                    if strong {
                        crate::fonts::bold(12.5)
                    } else {
                        font
                    },
                    palette.text,
                    title_rect.width(),
                );
                // Values.
                let mut x = rect.left() + TITLE_W;
                for col in &columns {
                    let cell = Rect::from_min_size(
                        egui::pos2(x, rect.top()),
                        Vec2::new(col.width(), row_h),
                    );
                    x += col.width();
                    painter.line_segment(
                        [cell.left_top(), cell.left_bottom()],
                        Stroke::new(0.5, palette.line),
                    );
                    match col {
                        Col::Stage(stage) => {
                            // A stage gone since the recorded state shows
                            // „–“ with its change.
                            let deviation = mode.of(row, *stage);
                            let value = match row.value(*stage) {
                                Some(value) => amount(value),
                                None if deviation.is_some() => "–".to_string(),
                                None => continue,
                            };
                            let mark = report.mark(row, *stage);
                            let fill = match mark {
                                Mark::OverBudget => Some(signal.bad_bg),
                                Mark::AbovePrevious => Some(signal.warn_bg),
                                Mark::None => None,
                            };
                            if let Some(fill) = fill {
                                painter.rect_filled(cell.shrink(1.0), CornerRadius::same(3), fill);
                            }
                            let line = Rect::from_min_size(cell.min, Vec2::new(cell.width(), 24.0));
                            painter.text(
                                line.right_center() - Vec2::new(8.0, 0.0),
                                Align2::RIGHT_CENTER,
                                value,
                                if strong {
                                    crate::fonts::bold(12.5)
                                } else {
                                    FontId::proportional(12.5)
                                },
                                palette.text,
                            );
                            if let Some(deviation) = deviation {
                                painter.text(
                                    egui::pos2(cell.right() - 8.0, cell.top() + 30.0),
                                    Align2::RIGHT_CENTER,
                                    short_deviation(&deviation),
                                    FontId::proportional(11.0),
                                    palette.text,
                                );
                            }
                        }
                        Col::Note => {
                            if !row.note.is_empty() {
                                let note_rect = cell.shrink2(Vec2::new(8.0, 0.0));
                                crate::widgets::cell_text(
                                    &painter.with_clip_rect(note_rect),
                                    egui::pos2(note_rect.left(), cell.top() + 12.0),
                                    Align2::LEFT_CENTER,
                                    &row.note,
                                    FontId::proportional(12.0),
                                    palette.text,
                                    note_rect.width(),
                                );
                            }
                        }
                    }
                }
                if response.clicked() && toggled.as_deref() != Some(key.as_str()) {
                    clicked = Some(key.clone());
                }
                if row.kind != TrackRowKind::Total {
                    let _ = response.on_hover_text(hover(row));
                }
            }
        });
    let inner = output.inner_rect;
    let origin_x = inner.left() - output.state.clamped_offset().x;

    // Fixed header, moved with the horizontal scroll.
    let painter = ui.painter().with_clip_rect(head);
    painter.rect_filled(
        head,
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        palette.panel_alt,
    );
    let header = |rect: Rect, label: &str, right: bool| {
        let position = if right {
            egui::pos2(rect.right() - 8.0, rect.center().y)
        } else {
            egui::pos2(rect.left() + 8.0, rect.center().y)
        };
        painter.text(
            position,
            if right {
                Align2::RIGHT_CENTER
            } else {
                Align2::LEFT_CENTER
            },
            label,
            crate::fonts::bold(12.0),
            palette.text,
        );
    };
    let title_rect =
        Rect::from_min_size(egui::pos2(origin_x, head.top()), Vec2::new(TITLE_W, HEAD_H));
    header(
        title_rect,
        match report.options.view {
            proava_domain::TrackView::ByGroup => "Kostengruppe",
            proava_domain::TrackView::ByLv => "Vergabeeinheit / Kostengruppe",
        },
        false,
    );
    let mut x = origin_x + TITLE_W;
    for col in &columns {
        let rect = Rect::from_min_size(egui::pos2(x, head.top()), Vec2::new(col.width(), HEAD_H));
        x += col.width();
        let (label, hint) = match col {
            Col::Stage(stage) => (stage.label().to_string(), stage.explanation().to_string()),
            Col::Note => (
                "Erläuterung / Maßnahme".to_string(),
                "Je Kostengruppe (DIN 276, 4.4.3 und 4.5); bearbeiten rechts nach Klick auf die Zeile".to_string(),
            ),
        };
        header(rect, &label, *col != Col::Note);
        if rect.intersects(head) {
            let clip = rect.intersect(head);
            let _ = ui
                .interact(
                    clip,
                    ui.scope_id().with(("tracking_head", label.as_str())),
                    Sense::hover(),
                )
                .on_hover_text(hint);
        }
    }
    ui.painter().rect_stroke(
        full,
        CornerRadius::same(8),
        Stroke::new(1.0, palette.line),
        StrokeKind::Inside,
    );

    if let Some(key) = toggled {
        if !collapsed.remove(&key) {
            collapsed.insert(key);
        }
    }
    clicked
}

/// „+1.234,56 / +12,3 %“ (without „€“, narrow cells).
fn short_deviation(deviation: &proava_domain::Deviation) -> String {
    deviation_text(deviation).replace(" €", "")
}

fn hover(row: &TrackRow) -> String {
    let mut text = row.title();
    if !row.note.is_empty() {
        text.push_str("\n\n");
        text.push_str(&row.note);
    }
    text
}
