//! Anlagenliste der Kostenermittlung: je Bauteil eine Kopfzeile, die
//! Anlagen mit ihren Datenpunkten je DP-Art und eine feste Summenzeile, am
//! Ende die Gesamtsumme. Bearbeiten wie in einer Tabellenkalkulation:
//! Klick in eine Zelle, Tab/Umschalt+Tab zur Nachbarzelle, Enter/Umschalt+Enter
//! eine Zeile tiefer/höher, Strg+Enter neue Zeile darunter, Esc bricht ab.

use eframe::egui::{self, Align2, FontId, Key, Modifiers, RichText, Sense, Stroke};
use proava_domain::{CostEstimate, CostSection, Money, PointRow};

use super::{CostsOutput, CostsView};
use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets;

const ROW_H: f32 = 24.0;
const HEAD_H: f32 = 26.0;

/// Column of a plant line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Col {
    Location,
    Trade,
    Plant,
    Kind(usize),
    Remark,
}

/// An editable cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellRef {
    /// Name of a Bauteil.
    Section(usize),
    Row {
        section: usize,
        row: usize,
        col: Col,
    },
}

/// The cell being edited and its text.
#[derive(Debug, Clone)]
pub struct Editing {
    pub cell: CellRef,
    pub text: String,
    /// Focus it and select its text in the next frame.
    fresh: bool,
}

/// Scroll position of the table, for keeping the edited cell in view.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollState {
    offset: f32,
    height: f32,
    /// Line to bring into view in the next frame.
    target: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Section(usize),
    Row(usize, usize),
    Sum(usize),
    Total,
}

fn lines(estimate: &CostEstimate) -> Vec<Line> {
    let mut lines = Vec::new();
    for (section, part) in estimate.sections.iter().enumerate() {
        lines.push(Line::Section(section));
        lines.extend((0..part.rows.len()).map(|row| Line::Row(section, row)));
        lines.push(Line::Sum(section));
    }
    lines.push(Line::Total);
    lines
}

fn columns(estimate: &CostEstimate) -> Vec<Col> {
    let mut columns = vec![Col::Location, Col::Trade, Col::Plant];
    columns.extend((0..estimate.kinds.len()).map(Col::Kind));
    columns.push(Col::Remark);
    columns
}

/// Every editable cell in reading order (Tab order).
fn cells(estimate: &CostEstimate) -> Vec<CellRef> {
    let columns = columns(estimate);
    let mut cells = Vec::new();
    for (section, part) in estimate.sections.iter().enumerate() {
        cells.push(CellRef::Section(section));
        for row in 0..part.rows.len() {
            cells.extend(columns.iter().map(|col| CellRef::Row {
                section,
                row,
                col: *col,
            }));
        }
    }
    cells
}

fn line_of(estimate: &CostEstimate, cell: CellRef) -> Option<usize> {
    lines(estimate).iter().position(|line| match (line, cell) {
        (Line::Section(a), CellRef::Section(b)) => *a == b,
        (Line::Row(s, r), CellRef::Row { section, row, .. }) => *s == section && *r == row,
        _ => false,
    })
}

/// Text a cell starts with.
fn value_of(estimate: &CostEstimate, cell: CellRef) -> String {
    match cell {
        CellRef::Section(section) => estimate
            .sections
            .get(section)
            .map(|part| part.name.clone())
            .unwrap_or_default(),
        CellRef::Row { section, row, col } => {
            let Some(line) = estimate
                .sections
                .get(section)
                .and_then(|part| part.rows.get(row))
            else {
                return String::new();
            };
            match col {
                Col::Location => line.location.clone(),
                Col::Trade => line.trade.clone(),
                Col::Plant => line.plant.clone(),
                Col::Remark => line.remark.clone(),
                Col::Kind(kind) => match line.count(kind) {
                    0 => String::new(),
                    count => count.to_string(),
                },
            }
        }
    }
}

/// Takes the typed text over; `Ok(true)` when something changed.
fn commit(estimate: &mut CostEstimate, editing: &Editing) -> Result<bool, String> {
    let text = editing.text.trim();
    match editing.cell {
        CellRef::Section(section) => {
            let Some(part) = estimate.sections.get_mut(section) else {
                return Ok(false);
            };
            if part.name == text {
                return Ok(false);
            }
            part.name = text.to_string();
            Ok(true)
        }
        CellRef::Row { section, row, col } => {
            let count = match col {
                Col::Kind(_) => Some(count_of(text)?),
                _ => None,
            };
            let Some(line) = estimate
                .sections
                .get_mut(section)
                .and_then(|part| part.rows.get_mut(row))
            else {
                return Ok(false);
            };
            let target = match col {
                Col::Location => &mut line.location,
                Col::Trade => &mut line.trade,
                Col::Plant => &mut line.plant,
                Col::Remark => &mut line.remark,
                Col::Kind(kind) => {
                    let count = count.unwrap_or(0);
                    if line.count(kind) == count {
                        return Ok(false);
                    }
                    line.set_count(kind, count);
                    return Ok(true);
                }
            };
            if target.as_str() == text {
                return Ok(false);
            }
            *target = text.to_string();
            Ok(true)
        }
    }
}

/// Anzahl Datenpunkte: whole number ≥ 0, empty = 0, „1.234“ = 1234.
pub(crate) fn count_of(text: &str) -> Result<u32, String> {
    if text.trim().is_empty() {
        return Ok(0);
    }
    // Same limit as the MCP server and the CSV import (`input_limits`).
    let value = proava_domain::NumberKind::PointCount
        .parse(text)
        .map_err(|error| format!("Anzahl Datenpunkte: {error}"))?;
    value
        .to_string()
        .parse::<u32>()
        .map_err(|_| "Anzahl Datenpunkte: bitte eine ganze Zahl.".to_string())
}

/// Where a key moves the edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Next,
    Previous,
    Down,
    Up,
    NewRow,
    Cancel,
}

fn neighbour(estimate: &CostEstimate, cell: CellRef, step: Move) -> Option<CellRef> {
    match step {
        Move::Next | Move::Previous => {
            let all = cells(estimate);
            let index = all.iter().position(|other| *other == cell)?;
            if step == Move::Next {
                all.get(index + 1).copied()
            } else {
                index
                    .checked_sub(1)
                    .and_then(|index| all.get(index).copied())
            }
        }
        Move::Down | Move::Up => {
            let col = match cell {
                CellRef::Row { col, .. } => col,
                CellRef::Section(_) => Col::Plant,
            };
            // Plant lines in order; a Bauteil without lines is skipped.
            let rows: Vec<(usize, usize)> = estimate
                .sections
                .iter()
                .enumerate()
                .flat_map(|(section, part)| (0..part.rows.len()).map(move |row| (section, row)))
                .collect();
            let index = match cell {
                CellRef::Row { section, row, .. } => {
                    rows.iter().position(|place| *place == (section, row))?
                }
                CellRef::Section(section) => {
                    let first = rows.iter().position(|(other, _)| *other >= section)?;
                    return (step == Move::Down).then_some(CellRef::Row {
                        section: rows[first].0,
                        row: rows[first].1,
                        col,
                    });
                }
            };
            let target = if step == Move::Down {
                rows.get(index + 1)
            } else {
                index.checked_sub(1).and_then(|index| rows.get(index))
            };
            target.map(|(section, row)| CellRef::Row {
                section: *section,
                row: *row,
                col,
            })
        }
        Move::NewRow | Move::Cancel => None,
    }
}

/// Widths of the columns for the available width.
struct Widths {
    location: f32,
    trade: f32,
    plant: f32,
    kind: f32,
    points: f32,
    remark: f32,
}

impl Widths {
    /// Fits the columns into `available` (the result column docks beside
    /// the table): Anlage and Bemerkung share the rest; in narrow windows the
    /// fixed columns get narrower first.
    fn of(available: f32, kinds: usize) -> Self {
        let narrow = available < 110.0 + 64.0 + 50.0 * kinds as f32 + 58.0 + 260.0;
        let (location, trade, kind, points) = if narrow {
            (84.0, 50.0, 42.0, 48.0)
        } else {
            (110.0, 64.0, 50.0, 58.0)
        };
        let rest = (available - location - trade - kind * kinds as f32 - points).max(200.0);
        let plant = (rest * 0.5).max(100.0);
        Self {
            location,
            trade,
            plant,
            kind,
            points,
            remark: (rest - plant).max(100.0),
        }
    }

    fn total(&self, kinds: usize) -> f32 {
        self.location
            + self.trade
            + self.plant
            + self.kind * kinds as f32
            + self.points
            + self.remark
    }

    /// Left edge and width of `col` (Σ DP sits between the kinds and the remark).
    fn span(&self, col: Col, kinds: usize) -> (f32, f32) {
        let after_plant = self.location + self.trade + self.plant;
        match col {
            Col::Location => (0.0, self.location),
            Col::Trade => (self.location, self.trade),
            Col::Plant => (self.location + self.trade, self.plant),
            Col::Kind(kind) => (after_plant + self.kind * kind as f32, self.kind),
            Col::Remark => (
                after_plant + self.kind * kinds as f32 + self.points,
                self.remark,
            ),
        }
    }

    fn points_span(&self, kinds: usize) -> (f32, f32) {
        (
            self.location + self.trade + self.plant + self.kind * kinds as f32,
            self.points,
        )
    }
}

/// What the user asked for in a context menu or with a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAction {
    InsertAbove(usize, usize),
    InsertBelow(usize, usize),
    Delete(usize, usize),
    Up(usize, usize),
    Down(usize, usize),
    MoveTo(usize, usize, usize),
    AddRow(usize),
    SectionUp(usize),
    SectionDown(usize),
    DeleteSection(usize),
    AddSection,
}

pub(super) fn show(
    view: &mut CostsView,
    ui: &mut egui::Ui,
    palette: &Palette,
    estimate: &mut CostEstimate,
    output: &mut CostsOutput,
) {
    // Never paint into the result column beside the table.
    let area = ui.available_rect_before_wrap();
    let mut clip = ui.clip_rect();
    clip.max.x = clip.max.x.min(area.max.x);
    ui.set_clip_rect(clip);

    let kinds = estimate.kinds.len();
    // One text field id per cell: with a shared id egui's own Tab handling
    // would pass the focus on to the next widget after the move.
    let edit_id = |cell: CellRef| egui::Id::unique("cost_cell_edit").with(cell);

    // Keys while a cell is edited (before the text field sees them). Esc
    // takes egui the focus before the frame: look at the key itself.
    let focused = view
        .editing
        .as_ref()
        .is_some_and(|editing| ui.memory(|memory| memory.has_focus(edit_id(editing.cell))));
    let mut step: Option<Move> = None;
    if view.editing.is_some() && ui.input(|input| input.key_pressed(Key::Escape)) {
        step = Some(Move::Cancel);
    } else if focused {
        step = ui.input_mut(|input| {
            if input.consume_key(Modifiers::SHIFT, Key::Tab) {
                Some(Move::Previous)
            } else if input.consume_key(Modifiers::NONE, Key::Tab) {
                Some(Move::Next)
            } else if input.consume_key(Modifiers::COMMAND, Key::Enter) {
                Some(Move::NewRow)
            } else if input.consume_key(Modifiers::SHIFT, Key::Enter) {
                Some(Move::Up)
            } else if input.consume_key(Modifiers::NONE, Key::Enter) {
                Some(Move::Down)
            } else if input.consume_key(Modifiers::NONE, Key::Escape) {
                Some(Move::Cancel)
            } else {
                None
            }
        });
    }
    if let Some(step) = step {
        key_move(view, estimate, step, output);
    }

    // Buttons above the table.
    let mut action: Option<RowAction> = None;
    crate::widgets::row(ui, |ui| {
        ui.label(
            RichText::new("Anlagenliste")
                .size(13.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(10.0);
        if widgets::icon_label_button(ui, palette, Icon::Plus, "Zeile")
            .on_hover_text("Neue Zeile am Ende des Bauteils, in dem du gerade bist (Strg+Enter beim Bearbeiten: darunter)")
            .clicked()
        {
            let section = match view.editing.as_ref().map(|editing| editing.cell) {
                Some(CellRef::Section(section)) | Some(CellRef::Row { section, .. }) => section,
                None => estimate.sections.len().saturating_sub(1),
            };
            action = Some(RowAction::AddRow(section));
        }
        if widgets::icon_label_button(ui, palette, Icon::Plus, "Bauteil")
            .on_hover_text("Neues Bauteil (Abschnitt) am Ende")
            .clicked()
        {
            action = Some(RowAction::AddSection);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("Klick oder Tippen bearbeitet · Tab/Enter: nächste Zelle · Rechtsklick: Zeile einfügen, löschen, verschieben")
                .size(11.0)
                .color(palette.text_dim),
        );
    });
    ui.add_space(4.0);

    let widths = Widths::of(ui.available_width() - 14.0, kinds);
    let total_width = widths.total(kinds);
    let result = estimate.result();
    let all_lines = lines(estimate);

    // Column heads.
    let (head, _) = ui.allocate_exact_size(egui::vec2(total_width, HEAD_H), Sense::hover());
    let painter = ui.painter_at(head);
    painter.rect_filled(head, 0.0, palette.panel_alt);
    painter.hline(
        head.x_range(),
        head.bottom(),
        Stroke::new(1.0, palette.line),
    );
    let head_text = |col_left: f32, width: f32, text: &str, right: bool| {
        let pos = if right {
            egui::pos2(head.left() + col_left + width - 6.0, head.center().y)
        } else {
            egui::pos2(head.left() + col_left + 6.0, head.center().y)
        };
        let align = if right {
            Align2::RIGHT_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        widgets::bold_text(
            &painter,
            pos,
            align,
            text,
            FontId::proportional(12.0),
            palette.text,
        );
    };
    for (col, text) in [
        (Col::Location, "Ort"),
        (Col::Trade, "Gewerk"),
        (Col::Plant, "Anlage"),
        (Col::Remark, "Bemerkung"),
    ] {
        let (left, width) = widths.span(col, kinds);
        head_text(left, width, text, false);
    }
    for (index, kind) in estimate.kinds.iter().enumerate() {
        let (left, width) = widths.span(Col::Kind(index), kinds);
        head_text(left, width, &kind.key, true);
        let rect = egui::Rect::from_min_size(
            egui::pos2(head.left() + left, head.top()),
            egui::vec2(width, HEAD_H),
        );
        ui.interact(
            rect,
            ui.scope_id().with(("cost_kind_head", index)),
            Sense::hover(),
        )
        .on_hover_text(format!(
            "{}{}",
            kind.caption(),
            if kind.is_virtual() {
                " – virtuell"
            } else {
                ""
            }
        ));
    }
    let (left, width) = widths.points_span(kinds);
    head_text(left, width, "DP", true);

    // Lines.
    let mut clicked: Option<CellRef> = None;
    let mut edit_seen = false;
    let mut lost_focus = false;
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("cost_table_scroll")
        .auto_shrink([false, false]);
    if let Some(target) = view.scroll.target.take() {
        let top = target as f32 * ROW_H;
        let state = view.scroll;
        if top < state.offset {
            scroll = scroll.vertical_scroll_offset(top);
        } else if top + ROW_H > state.offset + state.height && state.height > 0.0 {
            scroll = scroll.vertical_scroll_offset(top + ROW_H - state.height);
        }
    }
    let section_names: Vec<String> = estimate
        .sections
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if part.name.trim().is_empty() {
                format!("Bauteil {}", index + 1)
            } else {
                part.name.clone()
            }
        })
        .collect();
    let scrolled = scroll.show_rows(ui, ROW_H, all_lines.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for index in range {
            let line = all_lines[index];
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(total_width, ROW_H), Sense::click());
            let painter = ui.painter_at(rect);
            let cell_rect = |col: Col| {
                let (left, width) = widths.span(col, kinds);
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + left, rect.top()),
                    egui::vec2(width, ROW_H),
                )
            };
            let points_rect = {
                let (left, width) = widths.points_span(kinds);
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + left, rect.top()),
                    egui::vec2(width, ROW_H),
                )
            };
            let text_at =
                |cell: egui::Rect, text: &str, right: bool, strong: bool, color: egui::Color32| {
                    let pos = if right {
                        egui::pos2(cell.right() - 6.0, cell.center().y)
                    } else {
                        egui::pos2(cell.left() + 6.0, cell.center().y)
                    };
                    let align = if right {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    };
                    let font = FontId::proportional(12.5);
                    let clipped =
                        painter.with_clip_rect(cell.shrink2(egui::vec2(2.0, 0.0)).intersect(rect));
                    // One line, „…“ when the column is too narrow; true = cut.
                    let measure = if strong {
                        crate::fonts::bold(font.size)
                    } else {
                        font.clone()
                    };
                    let (shown, cut) =
                        widgets::fit_line(&clipped, text, &measure, cell.width() - 12.0);
                    if strong {
                        widgets::bold_text(&clipped, pos, align, &shown, font, color);
                    } else {
                        clipped.text(pos, align, shown, font, color);
                    }
                    cut
                };
            match line {
                Line::Section(section) => {
                    painter.rect_filled(rect, 0.0, palette.accent_soft);
                    let name_rect = egui::Rect::from_min_max(
                        cell_rect(Col::Location).min,
                        cell_rect(Col::Plant).max,
                    );
                    let editing_here = view
                        .editing
                        .as_ref()
                        .is_some_and(|editing| editing.cell == CellRef::Section(section));
                    if editing_here {
                        let (seen, lost) = edit_field(
                            view,
                            ui,
                            name_rect,
                            false,
                            edit_id(CellRef::Section(section)),
                        );
                        edit_seen |= seen;
                        lost_focus |= lost;
                    } else {
                        text_at(
                            name_rect,
                            &format!("Bauteil: {}", section_names[section]),
                            false,
                            true,
                            palette.text,
                        );
                    }
                    let rows = estimate.sections[section].rows.len();
                    let after = egui::Rect::from_min_max(
                        cell_rect(Col::Kind(0)).min,
                        cell_rect(Col::Remark).max,
                    );
                    text_at(
                        after,
                        &crate::format::count(rows, "Zeile", "Zeilen"),
                        true,
                        false,
                        palette.text_dim,
                    );
                    if response.clicked() && !editing_here {
                        clicked = Some(CellRef::Section(section));
                    }
                    let count = estimate.sections.len();
                    response.context_menu(|ui| {
                        if ui.button("Bauteil umbenennen").clicked() {
                            clicked = Some(CellRef::Section(section));
                            ui.close();
                        }
                        if ui.button("Zeile hinzufügen").clicked() {
                            action = Some(RowAction::AddRow(section));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(section > 0, egui::Button::new("Bauteil nach oben"))
                            .clicked()
                        {
                            action = Some(RowAction::SectionUp(section));
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                section + 1 < count,
                                egui::Button::new("Bauteil nach unten"),
                            )
                            .clicked()
                        {
                            action = Some(RowAction::SectionDown(section));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                count > 1,
                                egui::Button::new("Bauteil löschen (mit seinen Zeilen)"),
                            )
                            .clicked()
                        {
                            action = Some(RowAction::DeleteSection(section));
                            ui.close();
                        }
                    });
                }
                Line::Row(section, row) => {
                    if row % 2 == 1 {
                        painter.rect_filled(rect, 0.0, palette.row_alt);
                    }
                    if response.hovered() {
                        painter.rect_filled(rect, 0.0, palette.row_hover);
                    }
                    painter.hline(
                        rect.x_range(),
                        rect.bottom() - 0.5,
                        Stroke::new(0.5, palette.line),
                    );
                    let line_data: &PointRow = &estimate.sections[section].rows[row];
                    let mut hover: Vec<String> = Vec::new();
                    for col in columns(estimate) {
                        let cell = CellRef::Row { section, row, col };
                        let crect = cell_rect(col);
                        let numeric = matches!(col, Col::Kind(_));
                        if view
                            .editing
                            .as_ref()
                            .is_some_and(|editing| editing.cell == cell)
                        {
                            let (seen, lost) = edit_field(view, ui, crect, numeric, edit_id(cell));
                            edit_seen |= seen;
                            lost_focus |= lost;
                            continue;
                        }
                        let text = match col {
                            Col::Location => line_data.location.clone(),
                            Col::Trade => line_data.trade.clone(),
                            Col::Plant => line_data.plant.clone(),
                            Col::Remark => line_data.remark.clone(),
                            Col::Kind(kind) => match line_data.count(kind) {
                                0 => String::new(),
                                count => count.to_string(),
                            },
                        };
                        let color = if col == Col::Remark {
                            palette.text_dim
                        } else {
                            palette.text
                        };
                        if text_at(crect, &text, numeric, false, color) {
                            hover.push(text);
                        }
                    }
                    if !hover.is_empty() {
                        let _ = response.clone().on_hover_text(hover.join("\n"));
                    }
                    let points = line_data.points();
                    if points > 0 {
                        text_at(points_rect, &points.to_string(), true, true, palette.text);
                    }
                    if response.clicked() {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let x = pos.x - rect.left();
                            clicked = columns(estimate)
                                .into_iter()
                                .find(|col| {
                                    let (left, width) = widths.span(*col, kinds);
                                    x >= left && x < left + width
                                })
                                .map(|col| CellRef::Row { section, row, col });
                        }
                    }
                    let rows = estimate.sections[section].rows.len();
                    response.context_menu(|ui| {
                        if ui.button("Zeile darüber einfügen").clicked() {
                            action = Some(RowAction::InsertAbove(section, row));
                            ui.close();
                        }
                        if ui.button("Zeile darunter einfügen").clicked() {
                            action = Some(RowAction::InsertBelow(section, row));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(row > 0, egui::Button::new("Zeile nach oben"))
                            .clicked()
                        {
                            action = Some(RowAction::Up(section, row));
                            ui.close();
                        }
                        if ui
                            .add_enabled(row + 1 < rows, egui::Button::new("Zeile nach unten"))
                            .clicked()
                        {
                            action = Some(RowAction::Down(section, row));
                            ui.close();
                        }
                        if section_names.len() > 1 {
                            crate::menus::menu(ui, "In anderes Bauteil", |ui| {
                                for (target, name) in section_names.iter().enumerate() {
                                    if target != section && ui.button(name).clicked() {
                                        action = Some(RowAction::MoveTo(section, row, target));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        ui.separator();
                        if ui.button("Zeile löschen").clicked() {
                            action = Some(RowAction::Delete(section, row));
                            ui.close();
                        }
                    });
                }
                Line::Sum(section) => {
                    let sums = &result.sections[section];
                    painter.rect_filled(rect, 0.0, palette.panel_alt);
                    painter.hline(rect.x_range(), rect.top(), Stroke::new(1.0, palette.line));
                    text_at(
                        cell_rect(Col::Plant),
                        &format!("Summe {}", section_names[section]),
                        false,
                        true,
                        palette.text,
                    );
                    for (kind, sum) in sums.kinds.iter().enumerate() {
                        if sum.count > 0 {
                            text_at(
                                cell_rect(Col::Kind(kind)),
                                &sum.count.to_string(),
                                true,
                                true,
                                palette.text,
                            );
                        }
                    }
                    text_at(
                        points_rect,
                        &sums.points.to_string(),
                        true,
                        true,
                        palette.text,
                    );
                    text_at(
                        cell_rect(Col::Remark),
                        &money(sums.total),
                        true,
                        true,
                        palette.text,
                    );
                }
                Line::Total => {
                    painter.rect_filled(rect, 0.0, palette.accent_soft);
                    painter.hline(
                        rect.x_range(),
                        rect.top(),
                        Stroke::new(1.5, palette.text_dim),
                    );
                    text_at(cell_rect(Col::Plant), "Gesamt", false, true, palette.text);
                    for (kind, sum) in result.kinds.iter().enumerate() {
                        text_at(
                            cell_rect(Col::Kind(kind)),
                            &sum.count.to_string(),
                            true,
                            true,
                            palette.text,
                        );
                    }
                    text_at(
                        points_rect,
                        &result.points.to_string(),
                        true,
                        true,
                        palette.text,
                    );
                    text_at(
                        cell_rect(Col::Remark),
                        &money(result.total),
                        true,
                        true,
                        palette.text,
                    );
                }
            }
        }
    });
    view.scroll.offset = scrolled.state.clamped_offset().y;
    view.scroll.height = scrolled.inner_rect.height();

    // A click starts editing there (the open cell is taken over first).
    if let Some(cell) = clicked {
        if finish(view, estimate, output) {
            start(view, estimate, cell);
        }
    } else if lost_focus
        || (view.editing.as_ref().is_some_and(|editing| !editing.fresh) && !edit_seen)
    {
        finish(view, estimate, output);
    }
    if let Some(action) = action {
        finish(view, estimate, output);
        row_action(view, estimate, action, output);
    }
}

fn money(value: Money) -> String {
    value.to_string()
}

/// The text field of the edited cell; (drawn, lost focus).
fn edit_field(
    view: &mut CostsView,
    ui: &mut egui::Ui,
    rect: egui::Rect,
    numeric: bool,
    id: egui::Id,
) -> (bool, bool) {
    let Some(editing) = view.editing.as_mut() else {
        return (false, false);
    };
    let response = ui.put(
        rect.shrink2(egui::vec2(1.0, 1.0)),
        egui::TextEdit::singleline(&mut editing.text)
            .id(id)
            .margin(egui::vec2(5.0, 2.0))
            .align(if numeric {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::LEFT_CENTER
            }),
    );
    if editing.fresh {
        response.request_focus();
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let end = editing.text.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(end),
            )));
        state.store(ui.ctx(), id);
        editing.fresh = false;
        return (true, false);
    }
    (true, response.lost_focus())
}

fn start(view: &mut CostsView, estimate: &CostEstimate, cell: CellRef) {
    view.editing = Some(Editing {
        cell,
        text: value_of(estimate, cell),
        fresh: true,
    });
    view.scroll.target = line_of(estimate, cell);
}

/// Takes the edited cell over and closes it; false (and stays open) when
/// the text is no valid number.
fn finish(view: &mut CostsView, estimate: &mut CostEstimate, output: &mut CostsOutput) -> bool {
    let Some(editing) = view.editing.clone() else {
        return true;
    };
    match commit(estimate, &editing) {
        Ok(changed) => {
            if changed {
                output.change("Anlagenliste ändern", false);
            }
            view.editing = None;
            true
        }
        Err(error) => {
            output.response.toast = Some(error);
            if let Some(editing) = view.editing.as_mut() {
                editing.fresh = true;
            }
            false
        }
    }
}

fn key_move(
    view: &mut CostsView,
    estimate: &mut CostEstimate,
    step: Move,
    output: &mut CostsOutput,
) {
    let Some(cell) = view.editing.as_ref().map(|editing| editing.cell) else {
        return;
    };
    if step == Move::Cancel {
        view.editing = None;
        return;
    }
    if !finish(view, estimate, output) {
        return;
    }
    if step == Move::NewRow {
        let (section, row) = match cell {
            CellRef::Row { section, row, .. } => (section, row + 1),
            CellRef::Section(section) => (section, 0),
        };
        insert_row(estimate, section, row);
        output.change("Zeile einfügen", false);
        let col = match cell {
            CellRef::Row { col, .. } => col,
            CellRef::Section(_) => Col::Location,
        };
        start(view, estimate, CellRef::Row { section, row, col });
        return;
    }
    if let Some(next) = neighbour(estimate, cell, step) {
        start(view, estimate, next);
    }
}

fn insert_row(estimate: &mut CostEstimate, section: usize, row: usize) {
    let count = estimate.kinds.len();
    if let Some(part) = estimate.sections.get_mut(section) {
        let row = row.min(part.rows.len());
        part.rows.insert(
            row,
            PointRow {
                counts: vec![0; count],
                ..PointRow::default()
            },
        );
    }
}

fn row_action(
    view: &mut CostsView,
    estimate: &mut CostEstimate,
    action: RowAction,
    output: &mut CostsOutput,
) {
    view.editing = None;
    match action {
        RowAction::InsertAbove(section, row) => {
            insert_row(estimate, section, row);
            output.change("Zeile einfügen", false);
            start(
                view,
                estimate,
                CellRef::Row {
                    section,
                    row,
                    col: Col::Location,
                },
            );
        }
        RowAction::InsertBelow(section, row) => {
            insert_row(estimate, section, row + 1);
            output.change("Zeile einfügen", false);
            start(
                view,
                estimate,
                CellRef::Row {
                    section,
                    row: row + 1,
                    col: Col::Location,
                },
            );
        }
        RowAction::AddRow(section) => {
            let row = estimate
                .sections
                .get(section)
                .map(|part| part.rows.len())
                .unwrap_or(0);
            if estimate.sections.is_empty() {
                estimate.sections.push(CostSection::new("Gebäude"));
            }
            let section = section.min(estimate.sections.len() - 1);
            insert_row(estimate, section, row);
            output.change("Zeile einfügen", false);
            start(
                view,
                estimate,
                CellRef::Row {
                    section,
                    row,
                    col: Col::Location,
                },
            );
        }
        RowAction::Delete(section, row) => {
            if let Some(part) = estimate.sections.get_mut(section) {
                if row < part.rows.len() {
                    part.rows.remove(row);
                    output.change("Zeile löschen", false);
                    output.response.toast = Some("Zeile gelöscht (Strg+Z holt sie zurück)".into());
                }
            }
        }
        RowAction::Up(section, row) | RowAction::Down(section, row) => {
            let up = matches!(action, RowAction::Up(..));
            if let Some(part) = estimate.sections.get_mut(section) {
                let other = if up {
                    row.checked_sub(1)
                } else {
                    Some(row + 1).filter(|other| *other < part.rows.len())
                };
                if let Some(other) = other {
                    part.rows.swap(row, other);
                    output.change("Zeile verschieben", false);
                }
            }
        }
        RowAction::MoveTo(section, row, target) => {
            if section != target
                && target < estimate.sections.len()
                && row < estimate.sections[section].rows.len()
            {
                let line = estimate.sections[section].rows.remove(row);
                estimate.sections[target].rows.push(line);
                output.change("Zeile verschieben", false);
            }
        }
        RowAction::SectionUp(section) | RowAction::SectionDown(section) => {
            let up = matches!(action, RowAction::SectionUp(_));
            let other = if up {
                section.checked_sub(1)
            } else {
                Some(section + 1).filter(|other| *other < estimate.sections.len())
            };
            if let Some(other) = other {
                estimate.sections.swap(section, other);
                output.change("Bauteil verschieben", false);
            }
        }
        RowAction::DeleteSection(section) => {
            if estimate.sections.len() > 1 && section < estimate.sections.len() {
                let part = estimate.sections.remove(section);
                output.change("Bauteil löschen", false);
                output.response.toast = Some(format!(
                    "Bauteil „{}“ gelöscht (Strg+Z holt es zurück)",
                    part.name.trim()
                ));
                view.open_section = Some(0);
            }
        }
        RowAction::AddSection => {
            let name = format!("Bauteil {}", estimate.sections.len() + 1);
            let mut part = CostSection::new(&name);
            part.rows.push(PointRow {
                counts: vec![0; estimate.kinds.len()],
                ..PointRow::default()
            });
            estimate.sections.push(part);
            output.change("Bauteil anlegen", false);
            let section = estimate.sections.len() - 1;
            view.open_section = Some(section);
            start(view, estimate, CellRef::Section(section));
        }
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{cost_demo::demo_cost_estimate, CostStage};

    use super::*;

    #[test]
    fn tab_and_enter_walk_the_cells() {
        let estimate = demo_cost_estimate(CostStage::Calculation);
        let first = CellRef::Row {
            section: 0,
            row: 0,
            col: Col::Location,
        };
        assert_eq!(
            neighbour(&estimate, CellRef::Section(0), Move::Next),
            Some(first)
        );
        assert_eq!(
            neighbour(&estimate, first, Move::Previous),
            Some(CellRef::Section(0))
        );
        let last_col = CellRef::Row {
            section: 0,
            row: 0,
            col: Col::Remark,
        };
        assert_eq!(
            neighbour(&estimate, last_col, Move::Next),
            Some(CellRef::Row {
                section: 0,
                row: 1,
                col: Col::Location
            })
        );
        // Enter on the last line of a Bauteil goes on in the next one.
        let rows = estimate.sections[0].rows.len();
        let last = CellRef::Row {
            section: 0,
            row: rows - 1,
            col: Col::Kind(2),
        };
        assert_eq!(
            neighbour(&estimate, last, Move::Down),
            Some(CellRef::Row {
                section: 1,
                row: 0,
                col: Col::Kind(2)
            })
        );
        assert_eq!(
            neighbour(
                &estimate,
                CellRef::Row {
                    section: 1,
                    row: 0,
                    col: Col::Kind(2)
                },
                Move::Up
            ),
            Some(last)
        );
        assert_eq!(
            neighbour(&estimate, CellRef::Section(1), Move::Down),
            Some(CellRef::Row {
                section: 1,
                row: 0,
                col: Col::Plant
            })
        );
    }

    #[test]
    fn commit_checks_counts_and_texts() {
        let mut estimate = demo_cost_estimate(CostStage::Calculation);
        let cell = CellRef::Row {
            section: 0,
            row: 0,
            col: Col::Kind(1),
        };
        let editing = |text: &str| Editing {
            cell,
            text: text.to_string(),
            fresh: false,
        };
        assert_eq!(commit(&mut estimate, &editing("1.234")), Ok(true));
        assert_eq!(estimate.sections[0].rows[0].count(1), 1234);
        assert_eq!(commit(&mut estimate, &editing("1234")), Ok(false));
        assert!(commit(&mut estimate, &editing("-3")).is_err());
        assert!(commit(&mut estimate, &editing("2,5")).is_err());
        assert_eq!(commit(&mut estimate, &editing("")), Ok(true));
        assert_eq!(estimate.sections[0].rows[0].count(1), 0);
        let name = Editing {
            cell: CellRef::Section(1),
            text: " Anbau ".into(),
            fresh: false,
        };
        assert_eq!(commit(&mut estimate, &name), Ok(true));
        assert_eq!(estimate.sections[1].name, "Anbau");
        assert_eq!(
            value_of(
                &estimate,
                CellRef::Row {
                    section: 0,
                    row: 0,
                    col: Col::Plant
                }
            ),
            "RLT-01 Klassen"
        );
    }
}
