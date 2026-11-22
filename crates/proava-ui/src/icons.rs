//! The operating symbols of proAVA, drawn as plain one-colour line
//! drawings in the manner of common office programs (no image files, no
//! third-party artwork).
//!
//! Anwender, 17.11.2026 22:19: the usual standard symbols stay („die kennt
//! jeder und sie sind nicht verspielt … die helfen ja bei der Bedienung“):
//! alignment, lists, table, image, arrows, magnifier, gear, sun/moon, folder
//! and document. Every symbol sits on a button with a tooltip that names it
//! (see `widgets::tool_button`, `widgets::icon_label_button`,
//! `widgets::square_icon_button`). Status and data marks (fertig/in Bearb.,
//! Hinweis, hoch/tief) stay text. The test `tests/ui_symbols.rs` keeps the
//! list below and watches for emojis, coloured pictograms and drawn status
//! pictures.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Arrow to the left: back to the project list.
    Back,
    /// Arrow to the right: go to (e.g. the performance description).
    Forward,
    /// Arrow up: move up, parent folder.
    Up,
    /// Arrow down: move down.
    Down,
    /// Plus: new, add.
    Plus,
    /// Cross: clear, close.
    Close,
    /// Waste bin: remove.
    Trash,
    /// Magnifier: search, command palette.
    Search,
    /// Gear: Einstellungen.
    Gear,
    /// Sun: switch to the light look.
    Sun,
    /// Moon: switch to the dark look.
    Moon,
    /// Sheet with a folded corner: project, file.
    Document,
    /// Folder: open, folder entries.
    Folder,
    /// Folder with a plus: new folder (file dialog).
    NewFolder,
    /// Arrow into a tray: import (Aus GAEB).
    Import,
    /// Floppy disk: save, create a backup.
    Save,
    /// Circular arrow around a clock: restore a backup.
    Restore,
    /// Paragraph alignment.
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignJustify,
    /// Bulleted list.
    Bullets,
    /// Numbered list.
    Numbering,
    /// Table (grid).
    Table,
    /// Picture (frame with mountains and sun).
    Image,
    /// Calendar sheet with two rings and a grid: date field.
    Calendar,
    /// Input box with a text caret (form field, as in Word): Bieterangabe.
    Field,
    /// Text lines with an input box below: Bieterangabe unter dem Langtext.
    FieldBelow,
    /// Two chain links: link (Verknüpfung, e.g. to an Ausführungsbeschreibung).
    Link,
}

impl Icon {
    /// Every symbol, for tests and the overview.
    pub const ALL: [Icon; 29] = [
        Icon::Back,
        Icon::Forward,
        Icon::Up,
        Icon::Down,
        Icon::Plus,
        Icon::Close,
        Icon::Trash,
        Icon::Search,
        Icon::Gear,
        Icon::Sun,
        Icon::Moon,
        Icon::Document,
        Icon::Folder,
        Icon::NewFolder,
        Icon::Import,
        Icon::Save,
        Icon::Restore,
        Icon::AlignLeft,
        Icon::AlignCenter,
        Icon::AlignRight,
        Icon::AlignJustify,
        Icon::Bullets,
        Icon::Numbering,
        Icon::Table,
        Icon::Image,
        Icon::Calendar,
        Icon::Field,
        Icon::FieldBelow,
        Icon::Link,
    ];
}

/// Space a symbol takes inside a row of text.
pub const INLINE_SIZE: f32 = 16.0;

/// Draws one symbol into `rect` in the single colour `color`. `backdrop` is
/// the colour behind it (cut-out of the moon).
pub fn draw(painter: &Painter, rect: Rect, icon: Icon, color: Color32, backdrop: Color32) {
    let size = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(size * 0.5);
    let at = |x: f32, y: f32| origin + Vec2::new(x * size, y * size);
    let stroke = Stroke::new((size * 0.085).max(1.0), color);
    let thin = Stroke::new((size * 0.065).max(1.0), color);
    let line =
        |a: (f32, f32), b: (f32, f32)| painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    let path = |points: &[(f32, f32)], stroke: Stroke| {
        painter.add(Shape::line(
            points.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        ));
    };
    let closed = |points: &[(f32, f32)]| {
        painter.add(Shape::closed_line(
            points.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        ));
    };
    let frame =
        |x0: f32, y0: f32, x1: f32, y1: f32| closed(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    // Four text lines; `spans` gives (start, end) per line.
    let text_lines = |spans: [(f32, f32); 4]| {
        for (index, (x0, x1)) in spans.into_iter().enumerate() {
            let y = 0.20 + index as f32 * 0.20;
            line((x0, y), (x1, y));
        }
    };

    match icon {
        Icon::Back => {
            line((0.86, 0.50), (0.16, 0.50));
            path(&[(0.42, 0.24), (0.16, 0.50), (0.42, 0.76)], stroke);
        }
        Icon::Forward => {
            line((0.14, 0.50), (0.84, 0.50));
            path(&[(0.58, 0.24), (0.84, 0.50), (0.58, 0.76)], stroke);
        }
        Icon::Up => {
            line((0.50, 0.86), (0.50, 0.16));
            path(&[(0.24, 0.42), (0.50, 0.16), (0.76, 0.42)], stroke);
        }
        Icon::Down => {
            line((0.50, 0.14), (0.50, 0.84));
            path(&[(0.24, 0.58), (0.50, 0.84), (0.76, 0.58)], stroke);
        }
        Icon::Plus => {
            line((0.50, 0.16), (0.50, 0.84));
            line((0.16, 0.50), (0.84, 0.50));
        }
        Icon::Close => {
            line((0.24, 0.24), (0.76, 0.76));
            line((0.76, 0.24), (0.24, 0.76));
        }
        Icon::Trash => {
            line((0.18, 0.26), (0.82, 0.26));
            path(
                &[(0.40, 0.26), (0.42, 0.14), (0.58, 0.14), (0.60, 0.26)],
                stroke,
            );
            path(
                &[(0.26, 0.26), (0.32, 0.88), (0.68, 0.88), (0.74, 0.26)],
                stroke,
            );
            line((0.44, 0.40), (0.44, 0.76));
            line((0.56, 0.40), (0.56, 0.76));
        }
        Icon::Search => {
            painter.circle_stroke(at(0.42, 0.42), size * 0.24, stroke);
            line((0.60, 0.60), (0.86, 0.86));
        }
        Icon::Calendar => calendar(painter, rect, size, stroke),
        Icon::Gear => {
            painter.circle_stroke(rect.center(), size * 0.22, stroke);
            painter.circle_stroke(rect.center(), size * 0.06, stroke);
            let tooth = Stroke::new((size * 0.16).max(2.0), color);
            for step in 0..6 {
                let angle = std::f32::consts::TAU * step as f32 / 6.0 + 0.26;
                let inner = rect.center() + Vec2::new(angle.cos(), angle.sin()) * size * 0.24;
                let outer = rect.center() + Vec2::new(angle.cos(), angle.sin()) * size * 0.39;
                painter.line_segment([inner, outer], tooth);
            }
        }
        Icon::Sun => {
            painter.circle_stroke(rect.center(), size * 0.20, stroke);
            for step in 0..8 {
                let angle = std::f32::consts::TAU * step as f32 / 8.0;
                let inner = rect.center() + Vec2::new(angle.cos(), angle.sin()) * size * 0.30;
                let outer = rect.center() + Vec2::new(angle.cos(), angle.sin()) * size * 0.44;
                painter.line_segment([inner, outer], stroke);
            }
        }
        Icon::Moon => {
            painter.circle_filled(rect.center() - Vec2::splat(size * 0.02), size * 0.30, color);
            painter.circle_filled(
                rect.center() + Vec2::new(size * 0.14, -size * 0.06),
                size * 0.26,
                backdrop,
            );
        }
        Icon::Document => {
            closed(&[
                (0.20, 0.10),
                (0.62, 0.10),
                (0.80, 0.28),
                (0.80, 0.90),
                (0.20, 0.90),
            ]);
            path(&[(0.62, 0.10), (0.62, 0.28), (0.80, 0.28)], thin);
            line((0.32, 0.46), (0.68, 0.46));
            line((0.32, 0.60), (0.68, 0.60));
            line((0.32, 0.74), (0.56, 0.74));
        }
        Icon::Folder => {
            closed(&[
                (0.12, 0.24),
                (0.40, 0.24),
                (0.48, 0.32),
                (0.88, 0.32),
                (0.88, 0.82),
                (0.12, 0.82),
            ]);
            line((0.12, 0.42), (0.88, 0.42));
        }
        Icon::NewFolder => {
            closed(&[
                (0.12, 0.24),
                (0.40, 0.24),
                (0.48, 0.32),
                (0.88, 0.32),
                (0.88, 0.82),
                (0.12, 0.82),
            ]);
            line((0.50, 0.48), (0.50, 0.74));
            line((0.37, 0.61), (0.63, 0.61));
        }
        Icon::Import => {
            path(
                &[(0.16, 0.60), (0.16, 0.86), (0.84, 0.86), (0.84, 0.60)],
                stroke,
            );
            line((0.50, 0.12), (0.50, 0.66));
            path(&[(0.30, 0.46), (0.50, 0.66), (0.70, 0.46)], stroke);
        }
        Icon::Save => {
            closed(&[
                (0.14, 0.14),
                (0.74, 0.14),
                (0.86, 0.26),
                (0.86, 0.86),
                (0.14, 0.86),
            ]);
            frame(0.30, 0.14, 0.64, 0.40);
            frame(0.28, 0.58, 0.72, 0.86);
        }
        Icon::Restore => {
            // Three quarters of a circle running anticlockwise, the head at
            // the upper left, clock hands inside.
            let center = (0.52, 0.52);
            let radius = 0.34;
            let points: Vec<(f32, f32)> = (0..=20)
                .map(|step| {
                    let angle = (-150.0 + 285.0 * step as f32 / 20.0).to_radians();
                    (
                        center.0 + radius * angle.cos(),
                        center.1 + radius * angle.sin(),
                    )
                })
                .collect();
            path(&points, stroke);
            let tip = points[0];
            path(
                &[
                    (tip.0 - 0.02, tip.1 - 0.22),
                    tip,
                    (tip.0 + 0.20, tip.1 - 0.04),
                ],
                stroke,
            );
            path(&[(0.52, 0.32), (0.52, 0.54), (0.66, 0.62)], thin);
        }
        Icon::AlignLeft => text_lines([(0.10, 0.90), (0.10, 0.62), (0.10, 0.90), (0.10, 0.62)]),
        Icon::AlignCenter => text_lines([(0.10, 0.90), (0.26, 0.74), (0.10, 0.90), (0.26, 0.74)]),
        Icon::AlignRight => text_lines([(0.10, 0.90), (0.38, 0.90), (0.10, 0.90), (0.38, 0.90)]),
        Icon::AlignJustify => text_lines([(0.10, 0.90), (0.10, 0.90), (0.10, 0.90), (0.10, 0.90)]),
        Icon::Bullets => {
            for y in [0.22, 0.50, 0.78] {
                painter.circle_filled(at(0.17, y), size * 0.065, color);
                line((0.36, y), (0.90, y));
            }
        }
        Icon::Numbering => {
            // Small digits 1, 2, 3 drawn as strokes, then the lines.
            let digit = Stroke::new((size * 0.06).max(1.0), color);
            path(&[(0.11, 0.17), (0.17, 0.12), (0.17, 0.32)], digit);
            path(
                &[
                    (0.10, 0.41),
                    (0.22, 0.41),
                    (0.22, 0.50),
                    (0.10, 0.60),
                    (0.22, 0.60),
                ],
                digit,
            );
            path(
                &[(0.10, 0.69), (0.22, 0.69), (0.22, 0.88), (0.10, 0.88)],
                digit,
            );
            path(&[(0.13, 0.785), (0.22, 0.785)], digit);
            for y in [0.22, 0.50, 0.78] {
                line((0.36, y), (0.90, y));
            }
        }
        Icon::Table => {
            frame(0.10, 0.16, 0.90, 0.84);
            line((0.10, 0.385), (0.90, 0.385));
            line((0.10, 0.615), (0.90, 0.615));
            line((0.37, 0.16), (0.37, 0.84));
            line((0.63, 0.16), (0.63, 0.84));
        }
        Icon::Field => {
            // Box with a short text and the caret, dotted line for the entry.
            frame(0.08, 0.26, 0.92, 0.74);
            line((0.20, 0.38), (0.20, 0.62));
            for step in 0..4 {
                let x = 0.32 + step as f32 * 0.14;
                line((x, 0.60), (x + 0.06, 0.60));
            }
        }
        Icon::Link => {
            // Two links on the diagonal joined by a short bar, as in office programs.
            let along = (
                std::f32::consts::FRAC_1_SQRT_2,
                -std::f32::consts::FRAC_1_SQRT_2,
            );
            let across = (
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            );
            let (half, radius) = (0.10_f32, 0.13_f32);
            for (cx, cy) in [(0.30_f32, 0.70_f32), (0.70, 0.30)] {
                let mut points = Vec::with_capacity(26);
                for (sign, from) in [
                    (1.0_f32, -std::f32::consts::FRAC_PI_2),
                    (-1.0, std::f32::consts::FRAC_PI_2),
                ] {
                    let (ex, ey) = (cx + sign * along.0 * half, cy + sign * along.1 * half);
                    for step in 0..=12 {
                        let t = from + std::f32::consts::PI * step as f32 / 12.0;
                        points.push((
                            ex + (along.0 * t.cos() + across.0 * t.sin()) * radius,
                            ey + (along.1 * t.cos() + across.1 * t.sin()) * radius,
                        ));
                    }
                }
                closed(&points);
            }
            line((0.38, 0.62), (0.62, 0.38));
        }
        Icon::FieldBelow => {
            line((0.08, 0.16), (0.92, 0.16));
            line((0.08, 0.34), (0.70, 0.34));
            frame(0.08, 0.52, 0.92, 0.88);
            line((0.20, 0.60), (0.20, 0.80));
            for step in 0..4 {
                let x = 0.32 + step as f32 * 0.14;
                line((x, 0.78), (x + 0.06, 0.78));
            }
        }
        Icon::Image => {
            frame(0.10, 0.18, 0.90, 0.82);
            path(
                &[
                    (0.16, 0.76),
                    (0.40, 0.48),
                    (0.58, 0.68),
                    (0.68, 0.58),
                    (0.84, 0.76),
                ],
                stroke,
            );
            painter.circle_stroke(at(0.68, 0.36), size * 0.07, thin);
        }
    }
}

/// The disclosure triangle of a collapsible outline row (Gliederung,
/// Preisspiegel): pointing right while collapsed, down while open.
pub fn disclosure(painter: &Painter, center: Pos2, collapsed: bool, color: Color32) {
    let v = Vec2::new;
    let points = if collapsed {
        vec![
            center + v(-2.0, -3.5),
            center + v(2.5, 0.0),
            center + v(-2.0, 3.5),
        ]
    } else {
        vec![
            center + v(-3.5, -2.0),
            center + v(3.5, -2.0),
            center + v(0.0, 2.5),
        ]
    };
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Scroll arrow of a long menu (`menus::scroll`): small filled triangle
/// pointing up or down, like the arrows of Windows and macOS menus.
pub fn scroll_arrow(painter: &Painter, center: Pos2, up: bool, color: Color32) {
    let v = Vec2::new;
    // Clockwise on screen.
    let points = if up {
        vec![
            center + v(0.0, -2.5),
            center + v(5.0, 2.5),
            center + v(-5.0, 2.5),
        ]
    } else {
        vec![
            center + v(-5.0, -2.5),
            center + v(5.0, -2.5),
            center + v(0.0, 2.5),
        ]
    };
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Plain outline calendar sheet like in Office date fields: frame, header
/// line, two rings on top and a hinted grid of days.
fn calendar(painter: &Painter, rect: Rect, size: f32, stroke: Stroke) {
    let sheet = Rect::from_center_size(
        rect.center() + Vec2::new(0.0, size * 0.04),
        Vec2::new(size * 0.76, size * 0.66),
    );
    painter.rect_stroke(
        sheet,
        eframe::egui::CornerRadius::same(1),
        stroke,
        eframe::egui::StrokeKind::Middle,
    );
    let header = sheet.top() + sheet.height() * 0.28;
    painter.line_segment(
        [
            Pos2::new(sheet.left(), header),
            Pos2::new(sheet.right(), header),
        ],
        stroke,
    );
    for x in [0.3, 0.7] {
        let ring_x = sheet.left() + sheet.width() * x;
        painter.line_segment(
            [
                Pos2::new(ring_x, sheet.top() - size * 0.09),
                Pos2::new(ring_x, sheet.top() + size * 0.07),
            ],
            stroke,
        );
    }
    let dot = Vec2::splat((size * 0.09).max(1.5));
    for row in 0..2 {
        for column in 0..3 {
            let center = Pos2::new(
                sheet.left() + sheet.width() * (0.25 + 0.25 * column as f32),
                header + (sheet.bottom() - header) * (0.33 + 0.36 * row as f32),
            );
            painter.rect_filled(
                Rect::from_center_size(center, dot),
                eframe::egui::CornerRadius::ZERO,
                stroke.color,
            );
        }
    }
}
