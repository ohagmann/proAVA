//! Position table of an invoice: like the LV (titles and positions), with
//! ordered quantity, quantity so far, the contractor's quantity, the checked
//! quantity, progress, source, price, amount and note. The selected row is
//! edited in place; findings colour the rows.

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke};
use proava_domain::{
    format_decimal_de, parse_decimal_de, CheckBasis, Decimal, Finding, InvoiceId, InvoiceItem,
    LvRow, Mode, Money, PositionCheck, Progress, Project, RowId, Severity,
};

use super::{qty, BillingView, Field};
use crate::theme::Palette;
use crate::view::ViewResponse;
use crate::widgets;

const ROW_H: f32 = 26.0;
const HEAD_H: f32 = 24.0;

/// One table row.
enum Row {
    Group {
        level: usize,
        ordinal: String,
        text: String,
        amount: Money,
    },
    Position(Box<PositionRow>),
}

struct PositionRow {
    id: RowId,
    ordinal: String,
    short_text: String,
    unit: String,
    ordered: Option<Decimal>,
    unit_price: Option<Decimal>,
    previous: Decimal,
    claimed: Option<Decimal>,
    checked: Decimal,
    progress: Option<Decimal>,
    basis: Option<CheckBasis>,
    chosen_basis: Option<CheckBasis>,
    amount: Money,
    note: String,
    findings: Vec<Finding>,
    takeoff_lines: usize,
    hourly: bool,
    evidence: bool,
    change_order: bool,
}

impl PositionRow {
    fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(Finding::severity).max()
    }
}

#[derive(Clone, Copy)]
enum Col {
    Ordinal,
    Text,
    Unit,
    Ordered,
    Previous,
    Claimed,
    Checked,
    Progress,
    Basis,
    Price,
    Amount,
    Note,
}

const COLUMNS: [(Col, f32); 12] = [
    (Col::Ordinal, 96.0),
    (Col::Text, 0.0),
    (Col::Unit, 44.0),
    (Col::Ordered, 76.0),
    (Col::Previous, 76.0),
    (Col::Claimed, 92.0),
    (Col::Checked, 92.0),
    (Col::Progress, 84.0),
    (Col::Basis, 104.0),
    (Col::Price, 86.0),
    (Col::Amount, 100.0),
    (Col::Note, 0.0),
];

impl BillingView {
    fn rows(&self, project: &Project, id: InvoiceId) -> Vec<Row> {
        let Some(cache) = &self.cache else {
            return Vec::new();
        };
        let invoice = project.billing().invoice(id);
        let check = self.check(id);
        let release_cumulative =
            invoice.is_some_and(|invoice| invoice.release_mode() == Mode::Cumulative);
        let quantity_cumulative =
            invoice.is_some_and(|invoice| invoice.quantity_mode() == Mode::Cumulative);
        let line_of = |row: RowId| -> Option<&PositionCheck> {
            check.and_then(|check| {
                check
                    .positions
                    .iter()
                    .chain(check.unchanged.iter())
                    .find(|line| line.position == row)
            })
        };
        let change_order_rows: std::collections::HashSet<RowId> = project
            .change_orders()
            .iter()
            .flat_map(|order| order.items.iter().map(|item| item.position))
            .collect();
        let mut rows = Vec::new();
        // Titles above, not yet shown: (level, output index or pending data).
        let mut stack: Vec<(usize, Option<usize>, String, String)> = Vec::new();
        for row in project.lv().rows() {
            match row {
                LvRow::Group(group) => {
                    stack.retain(|(level, ..)| *level < group.level());
                    stack.push((
                        group.level(),
                        None,
                        group.ordinal().to_string(),
                        group.short_text().to_string(),
                    ));
                }
                LvRow::Position(position) => {
                    let row_id = position.id();
                    let item = invoice.and_then(|invoice| invoice.items.get(&row_id));
                    let takeoff_lines = project
                        .billing()
                        .takeoff_of(row_id)
                        .iter()
                        .filter(|line| line.invoice == Some(id))
                        .count();
                    let billable = cache.billable.get(&row_id);
                    let line = line_of(row_id);
                    let included = self.show_all
                        || billable.is_some()
                        || item.is_some()
                        || takeoff_lines > 0
                        || line.is_some()
                        || change_order_rows.contains(&row_id);
                    if !included {
                        continue;
                    }
                    for entry in stack.iter_mut() {
                        if entry.1.is_none() {
                            entry.1 = Some(rows.len());
                            rows.push(Row::Group {
                                level: entry.0,
                                ordinal: entry.2.clone(),
                                text: entry.3.clone(),
                                amount: Money::ZERO,
                            });
                        }
                    }
                    let amount = line
                        .map(|line| {
                            if release_cumulative {
                                line.cumulative_total
                            } else {
                                line.increment_total
                            }
                        })
                        .unwrap_or_default();
                    for (_, index, ..) in &stack {
                        if let Some(Row::Group { amount: sum, .. }) =
                            index.and_then(|index| rows.get_mut(index))
                        {
                            *sum += amount;
                        }
                    }
                    let previous = line.map(|line| line.previous).unwrap_or_default();
                    rows.push(Row::Position(Box::new(PositionRow {
                        id: row_id,
                        ordinal: position.ordinal().to_string(),
                        short_text: position.short_text().to_string(),
                        unit: position.unit().to_string(),
                        ordered: billable.map(|item| item.quantity),
                        unit_price: billable.map(|item| item.unit_price),
                        previous,
                        claimed: item.and_then(|item| item.claimed_quantity),
                        checked: line
                            .map(|line| line.checked)
                            .unwrap_or(if quantity_cumulative {
                                previous
                            } else {
                                Decimal::ZERO
                            }),
                        progress: item.and_then(|item| item.progress.as_ref().map(|p| p.factor)),
                        basis: line.and_then(|line| line.basis),
                        chosen_basis: item.and_then(|item| item.basis),
                        amount,
                        note: item.map(|item| item.note.clone()).unwrap_or_default(),
                        findings: line.map(|line| line.findings.clone()).unwrap_or_default(),
                        takeoff_lines,
                        hourly: billable.is_some_and(|item| item.hourly),
                        evidence: item.is_some_and(|item| item.evidence),
                        change_order: billable.is_some_and(|item| {
                            matches!(item.source, proava_domain::BillableSource::ChangeOrder(_))
                        }),
                    })));
                }
                _ => {}
            }
        }
        rows
    }

    pub(super) fn table(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        response: &mut ViewResponse,
    ) {
        let rows = self.rows(project, id);
        let positions: Vec<RowId> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Position(position) => Some(position.id),
                _ => None,
            })
            .collect();
        if self.row.is_none_or(|row| !positions.contains(&row)) {
            self.row = positions.first().copied();
        }
        self.keyboard(ui, project, id, &positions, response);
        self.load_edits(project, id);

        let invoice = project.billing().invoice(id);
        let quantity_mode = invoice.map(|i| i.quantity_mode()).unwrap_or_default();
        let release_mode = invoice.map(|i| i.release_mode()).unwrap_or_default();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Positionen").strong().color(palette.text));
            ui.add_space(8.0);
            ui.checkbox(&mut self.show_all, "alle LV-Positionen zeigen");
            ui.add_space(8.0);
            if ui
                .add_enabled(self.row.is_some(), egui::Button::new("Aufmaß der Position …"))
                .on_hover_text("Strg+M")
                .clicked()
            {
                self.takeoff = self.row;
            }
            ui.label(
                RichText::new("Pfeiltasten wählen, Enter/F2 bearbeitet „geprüft“, Tab wechselt das Feld, Doppelklick öffnet das Aufmaß.")
                    .size(11.5)
                    .color(palette.text_dim),
            );
        });
        ui.add_space(2.0);

        let total_width = ui.available_width().max(1100.0);
        let fixed: f32 = COLUMNS.iter().map(|(_, width)| width).sum();
        let flexible = (total_width - fixed).max(340.0);
        let width_of = |col: Col, width: f32| match col {
            Col::Text => flexible * 0.58,
            Col::Note => flexible * 0.42,
            _ => width,
        };
        let percent = self.percent;
        let header = |col: Col| -> String {
            match col {
                Col::Ordinal => "OZ".into(),
                Col::Text => "Kurztext".into(),
                Col::Unit => "Einh.".into(),
                Col::Ordered => "Auftrag".into(),
                Col::Previous => "bisher".into(),
                Col::Claimed => format!("Rechn.{}", sense(quantity_mode)),
                Col::Checked => format!("geprüft{}", sense(quantity_mode)),
                Col::Progress => {
                    if percent {
                        "Fortschr. %".into()
                    } else {
                        "Faktor".into()
                    }
                }
                Col::Basis => "Quelle".into(),
                Col::Price => "EP".into(),
                Col::Amount => match release_mode {
                    Mode::Cumulative => "Betrag kum.".into(),
                    Mode::Increment => "Betrag Zuwachs".into(),
                },
                Col::Note => "Vermerk".into(),
            }
        };

        egui::ScrollArea::horizontal()
            .id_salt("billing_table_h")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(total_width);
                let (head, _) =
                    ui.allocate_exact_size(egui::vec2(total_width, HEAD_H), Sense::hover());
                let mut x = head.left();
                for (col, width) in COLUMNS {
                    let width = width_of(col, width);
                    let cell = egui::Rect::from_min_size(
                        egui::pos2(x, head.top()),
                        egui::vec2(width, HEAD_H),
                    );
                    let text = header(col);
                    let right = is_number(col);
                    let response = ui.interact(
                        cell,
                        egui::Id::unique(("billing_head", x as i32)),
                        Sense::click(),
                    );
                    let painter = ui.painter_at(cell);
                    let pos = if right {
                        cell.right_center() - egui::vec2(6.0, 0.0)
                    } else {
                        cell.left_center() + egui::vec2(4.0, 0.0)
                    };
                    painter.text(
                        pos,
                        if right {
                            Align2::RIGHT_CENTER
                        } else {
                            Align2::LEFT_CENTER
                        },
                        text,
                        FontId::proportional(12.0),
                        palette.text_dim,
                    );
                    if matches!(col, Col::Progress)
                        && response
                            .on_hover_text("Klick: Fortschritt als Prozent oder Faktor")
                            .clicked()
                    {
                        self.percent = !self.percent;
                        self.edits.key = None;
                    }
                    x += width;
                }
                ui.painter().line_segment(
                    [
                        egui::pos2(head.left(), head.bottom()),
                        egui::pos2(head.right(), head.bottom()),
                    ],
                    Stroke::new(1.0, palette.line),
                );

                let scroll_to = self.scroll_to_row.then_some(self.row).flatten();
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(("billing_table_v", id.0))
                    .auto_shrink([false, false]);
                if let Some(target) = scroll_to {
                    if let Some(index) = rows
                        .iter()
                        .position(|row| matches!(row, Row::Position(p) if p.id == target))
                    {
                        let visible = ui.available_height();
                        scroll = scroll.vertical_scroll_offset(
                            (index as f32 * ROW_H - visible / 3.0).max(0.0),
                        );
                    }
                    self.scroll_to_row = false;
                }
                scroll.show_rows(ui, ROW_H, rows.len(), |ui, range| {
                    for row in &rows[range] {
                        match row {
                            Row::Group {
                                level,
                                ordinal,
                                text,
                                amount,
                            } => {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(total_width, ROW_H),
                                    Sense::hover(),
                                );
                                let painter = ui.painter_at(rect);
                                // Yellow like the title rows of the LV (Einstellung).
                                let fill = palette.title_fill(*level, false);
                                painter.rect_filled(rect, 0.0, fill.unwrap_or(palette.panel_alt));
                                let title_palette = palette.on_title_row(*level);
                                let palette = &title_palette;
                                let indent = *level as f32 * 10.0;
                                widgets::cell_text(
                                    &painter,
                                    rect.left_center() + egui::vec2(4.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    ordinal,
                                    FontId::monospace(12.5),
                                    palette.group_text,
                                    COLUMNS[0].1 - 8.0,
                                );
                                // The title runs over the quantity columns up to the amount.
                                let text_left = rect.left() + COLUMNS[0].1 + indent;
                                let amount_left = rect.left()
                                    + COLUMNS
                                        .iter()
                                        .take(10)
                                        .map(|(col, width)| width_of(*col, *width))
                                        .sum::<f32>();
                                widgets::cell_text(
                                    &painter,
                                    egui::pos2(text_left, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    text,
                                    crate::fonts::bold(13.0),
                                    palette.group_text,
                                    amount_left - text_left - 8.0,
                                );
                                if !amount.is_zero() {
                                    let right = rect.left()
                                        + COLUMNS
                                            .iter()
                                            .take(11)
                                            .map(|(col, width)| width_of(*col, *width))
                                            .sum::<f32>()
                                        - 6.0;
                                    painter.text(
                                        egui::pos2(right, rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        amount.plain(),
                                        crate::fonts::bold(12.5),
                                        palette.text,
                                    );
                                }
                            }
                            Row::Position(position) => {
                                self.position_row(
                                    ui,
                                    palette,
                                    project,
                                    id,
                                    position,
                                    total_width,
                                    &width_of,
                                    response,
                                );
                            }
                        }
                    }
                });
            });
        self.commit_if_left(project, response);
    }

    #[allow(clippy::too_many_arguments)]
    fn position_row(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        position: &PositionRow,
        total_width: f32,
        width_of: &dyn Fn(Col, f32) -> f32,
        response: &mut ViewResponse,
    ) {
        let (rect, row_response) =
            ui.allocate_exact_size(egui::vec2(total_width, ROW_H), Sense::click());
        let selected = self.row == Some(position.id);
        let worst = position.worst();
        let fill = if selected {
            palette.selected
        } else {
            match worst {
                Some(Severity::Error) => palette.missing_fill,
                Some(Severity::Warning) => palette.gap_fill,
                _ if row_response.hovered() => palette.row_hover,
                _ => palette.panel,
            }
        };
        ui.painter().rect_filled(rect, 0.0, fill);
        if let Some(severity) = worst {
            let color = match severity {
                Severity::Error => palette.error,
                Severity::Warning => palette.warn,
                Severity::Info => palette.text_dim,
            };
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
                0.0,
                color,
            );
        }
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), rect.bottom() - 0.5),
                egui::pos2(rect.right(), rect.bottom() - 0.5),
            ],
            Stroke::new(1.0, palette.background),
        );
        let name = format!(
            "{} {}",
            position.ordinal,
            crate::format::one_line(&position.short_text)
        );
        row_response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Row, true, selected, name.trim())
        });
        if row_response.clicked() && !selected {
            self.commit_edits(project, response);
            self.row = Some(position.id);
        }
        if row_response.double_clicked() {
            self.row = Some(position.id);
            self.takeoff = Some(position.id);
        }
        // Tooltip: the cells shortened with „…“, then the findings.
        let mut hover: Vec<String> = Vec::new();
        let mut x = rect.left();
        for (col, width) in COLUMNS {
            let width = width_of(col, width);
            let cell =
                egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, ROW_H));
            x += width;
            if selected && self.cell_editor(ui, palette, project, id, position, col, cell, response)
            {
                continue;
            }
            let (text, color) = self.cell_text(palette, position, col);
            if text.is_empty() {
                continue;
            }
            let painter = ui.painter_at(cell.shrink2(egui::vec2(4.0, 0.0)));
            let (pos, align) = if is_number(col) {
                (
                    cell.right_center() - egui::vec2(6.0, 0.0),
                    Align2::RIGHT_CENTER,
                )
            } else {
                (
                    cell.left_center() + egui::vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                )
            };
            let font = if matches!(col, Col::Ordinal) || is_number(col) {
                FontId::monospace(12.5)
            } else {
                FontId::proportional(13.0)
            };
            // One line, „…“ when the column is too narrow.
            let (_, cut) = widgets::cell_text(
                &painter,
                pos,
                align,
                &text,
                font,
                color,
                cell.width() - 12.0,
            );
            if cut {
                hover.push(text);
            }
        }
        hover.extend(position.findings.iter().map(Finding::message));
        if !hover.is_empty() {
            let _ = row_response.on_hover_text(hover.join("\n"));
        }
    }

    fn cell_text(
        &self,
        palette: &Palette,
        position: &PositionRow,
        col: Col,
    ) -> (String, egui::Color32) {
        let dim = palette.text_dim;
        let normal = palette.text;
        match col {
            Col::Ordinal => (position.ordinal.clone(), dim),
            Col::Text => {
                let mark = if position.change_order {
                    "Nachtrag · "
                } else {
                    ""
                };
                let text = if position.ordered.is_none() {
                    format!("{} (nicht im Auftrag)", position.short_text)
                } else {
                    format!("{mark}{}", position.short_text)
                };
                (text, normal)
            }
            Col::Unit => (position.unit.clone(), dim),
            Col::Ordered => (position.ordered.map(qty).unwrap_or_else(|| "–".into()), dim),
            Col::Previous => (
                if position.previous.is_zero() {
                    String::new()
                } else {
                    qty(position.previous)
                },
                dim,
            ),
            Col::Claimed => (position.claimed.map(qty).unwrap_or_default(), normal),
            Col::Checked => (
                if position.basis.is_none() && position.checked.is_zero() {
                    String::new()
                } else {
                    qty(position.checked)
                },
                if position.basis == Some(CheckBasis::Claimed) {
                    dim
                } else {
                    normal
                },
            ),
            Col::Progress => (
                position
                    .progress
                    .map(|factor| progress_text(factor, self.percent))
                    .unwrap_or_default(),
                normal,
            ),
            Col::Basis => {
                let mut text = position
                    .basis
                    .map(|basis| short_basis(basis).to_string())
                    .unwrap_or_default();
                if position.takeoff_lines > 0 && position.basis != Some(CheckBasis::Takeoff) {
                    text = format!("{text} (Aufmaß {})", position.takeoff_lines);
                }
                (text, dim)
            }
            Col::Price => (
                position
                    .unit_price
                    .map(|p| format_decimal_de(p, 2))
                    .unwrap_or_default(),
                dim,
            ),
            Col::Amount => (
                if position.amount.is_zero() {
                    String::new()
                } else {
                    position.amount.plain()
                },
                normal,
            ),
            Col::Note => {
                let mut text = position.note.clone();
                if position.hourly {
                    let proof = if position.evidence {
                        "Nachweis liegt vor"
                    } else {
                        "ohne Nachweis"
                    };
                    text = if text.is_empty() {
                        proof.to_string()
                    } else {
                        format!("{proof} · {text}")
                    };
                }
                (text, normal)
            }
        }
    }

    /// Editor in a cell of the selected row; false = draw the text.
    #[allow(clippy::too_many_arguments)]
    fn cell_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        position: &PositionRow,
        col: Col,
        cell: egui::Rect,
        response: &mut ViewResponse,
    ) -> bool {
        let field = match col {
            Col::Claimed => Field::Claimed,
            Col::Checked => Field::Checked,
            Col::Progress if position.ordered.is_some() => Field::Progress,
            Col::Note => Field::Note,
            Col::Basis => {
                let mut chosen = position.chosen_basis;
                let shown = chosen.map(short_basis).unwrap_or("auto");
                let inner = cell.shrink2(egui::vec2(2.0, 2.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    egui::ComboBox::from_id_salt(("billing_basis", id.0, position.id.0))
                        .selected_text(shown)
                        .width(inner.width() - 4.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut chosen,
                                None,
                                "automatisch (Fortschritt, geprüft, Aufmaß, Rechnung)",
                            );
                            for basis in [
                                CheckBasis::Progress,
                                CheckBasis::Quantity,
                                CheckBasis::Takeoff,
                                CheckBasis::Claimed,
                            ] {
                                ui.selectable_value(&mut chosen, Some(basis), basis.label());
                            }
                        });
                });
                if chosen != position.chosen_basis {
                    self.commit_edits(project, response);
                    if let Some(invoice) = project.billing_mut().invoice_mut(id) {
                        invoice.item_mut(position.id).basis = chosen;
                        response.dirty = true;
                    }
                }
                return true;
            }
            _ => return false,
        };
        let inner = cell.shrink2(egui::vec2(2.0, 2.0));
        let mut evidence_toggle = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.horizontal(|ui| {
                if field == Field::Note && position.hourly {
                    let mut evidence = position.evidence;
                    if ui
                        .checkbox(&mut evidence, "Nachweis")
                        .on_hover_text("Stundennachweis liegt vor")
                        .changed()
                    {
                        evidence_toggle = true;
                    }
                }
                let text = match field {
                    Field::Claimed => &mut self.edits.claimed,
                    Field::Checked => &mut self.edits.checked,
                    Field::Progress => &mut self.edits.progress,
                    Field::Note => &mut self.edits.note,
                };
                let numeric = field != Field::Note;
                let hint = match field {
                    Field::Checked if position.basis.is_some() => qty(position.checked),
                    _ => String::new(),
                };
                let mut edit = egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .id(egui::Id::unique(("billing_cell", field as u8)))
                    .desired_width(ui.available_width())
                    .margin(egui::Margin::symmetric(4, 2));
                if numeric {
                    edit = edit
                        .align(egui::Align2::RIGHT_CENTER)
                        .font(egui::TextStyle::Monospace);
                }
                let field_response = ui.add(edit);
                if field_response.changed() {
                    self.edits.changed = true;
                }
                if field_response.has_focus() {
                    self.focused = Some(field);
                }
                if self.focus == Some(field) {
                    field_response.request_focus();
                    self.focus = None;
                }
                if field_response.lost_focus() {
                    self.commit_pending = true;
                }
            });
        });
        if evidence_toggle {
            self.commit_edits(project, response);
            if let Some(invoice) = project.billing_mut().invoice_mut(id) {
                let item = invoice.item_mut(position.id);
                item.evidence = !item.evidence;
                response.dirty = true;
            }
        }
        let _ = palette;
        true
    }

    /// Fills the cell texts of the selected row from the project.
    fn load_edits(&mut self, project: &Project, id: InvoiceId) {
        let Some(row) = self.row else {
            self.edits.key = None;
            return;
        };
        let key = Some((id, row));
        let revision = project.revision();
        if self.edits.key == key && (self.edits.changed || self.edits_revision == revision) {
            return;
        }
        let item = project
            .billing()
            .invoice(id)
            .and_then(|invoice| invoice.items.get(&row))
            .cloned()
            .unwrap_or_default();
        self.edits.key = key;
        self.edits.changed = false;
        self.edits.claimed = item.claimed_quantity.map(qty).unwrap_or_default();
        self.edits.checked = item.checked_quantity.map(qty).unwrap_or_default();
        self.edits.progress = item
            .progress
            .as_ref()
            .map(|progress| progress_text(progress.factor, self.percent))
            .unwrap_or_default();
        self.edits.note = item.note.clone();
        self.edits_revision = revision;
    }

    fn commit_if_left(&mut self, project: &mut Project, response: &mut ViewResponse) {
        if std::mem::take(&mut self.commit_pending) {
            self.commit_edits(project, response);
        }
    }

    /// Writes the typed cell texts of the selected row into the invoice.
    pub(super) fn commit_edits(&mut self, project: &mut Project, response: &mut ViewResponse) {
        let Some((id, row)) = self.edits.key else {
            return;
        };
        if !std::mem::take(&mut self.edits.changed) {
            return;
        }
        let Some(invoice) = project.billing_mut().invoice_mut(id) else {
            return;
        };
        let before = invoice.items.get(&row).cloned().unwrap_or_default();
        let mut item = before.clone();
        let optional = |text: &str| -> Result<Option<Decimal>, String> {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                parse_decimal_de(text).map(Some)
            }
        };
        let mut problems = Vec::new();
        match optional(&self.edits.claimed) {
            Ok(value) => item.claimed_quantity = value,
            Err(error) => problems.push(format!("Rechnung: {error}")),
        }
        match optional(&self.edits.checked) {
            Ok(value) => item.checked_quantity = value,
            Err(error) => problems.push(format!("geprüft: {error}")),
        }
        let note = self.edits.note.trim().to_string();
        match optional(&self.edits.progress) {
            Ok(None) => item.progress = None,
            Ok(Some(value)) => {
                let factor = if self.percent {
                    value / Decimal::ONE_HUNDRED
                } else {
                    value
                };
                let probe = Progress {
                    factor,
                    note: "x".into(),
                };
                match probe.problem() {
                    Some(_) if self.percent && factor.normalize().scale() > Progress::DECIMALS => {
                        problems.push(
                            "Fortschritt: in Prozent höchstens zwei Nachkommastellen (37,50 %)."
                                .into(),
                        )
                    }
                    Some(problem) => problems.push(problem),
                    None => {
                        item.progress = Some(Progress {
                            factor,
                            note: note.clone(),
                        });
                        if note.is_empty() {
                            response.toast = Some(
                                "Leistungsfortschritt gesetzt – bitte im Feld Vermerk angeben, worauf er beruht.".into(),
                            );
                        }
                    }
                }
            }
            Err(error) => problems.push(format!("Fortschritt: {error}")),
        }
        item.note = note.clone();
        if let Some(progress) = &mut item.progress {
            progress.note = note;
        }
        if !problems.is_empty() {
            response.toast = Some(problems.join(" "));
        }
        if item != before {
            if item == InvoiceItem::default() {
                invoice.items.remove(&row);
            } else {
                invoice.items.insert(row, item);
            }
            response.dirty = true;
        }
        self.edits_revision = 0;
    }

    fn keyboard(
        &mut self,
        ui: &egui::Ui,
        project: &mut Project,
        id: InvoiceId,
        positions: &[RowId],
        response: &mut ViewResponse,
    ) {
        if self.has_dialog() || positions.is_empty() {
            return;
        }
        let focused = self.focused.take();
        let text_focus = ui.ctx().text_edit_focused();
        if text_focus && focused.is_none() {
            // Another text field (header) has the keyboard.
            return;
        }
        let (up, down, page_up, page_down, home, end, edit, takeoff) =
            ui.ctx().input_mut(|input| {
                let none = egui::Modifiers::NONE;
                (
                    input.consume_key(none, egui::Key::ArrowUp),
                    input.consume_key(none, egui::Key::ArrowDown),
                    input.consume_key(none, egui::Key::PageUp),
                    input.consume_key(none, egui::Key::PageDown),
                    !text_focus && input.consume_key(none, egui::Key::Home),
                    !text_focus && input.consume_key(none, egui::Key::End),
                    !text_focus
                        && (input.consume_key(none, egui::Key::Enter)
                            || input.consume_key(none, egui::Key::F2)),
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::M),
                )
            });
        let current = self
            .row
            .and_then(|row| positions.iter().position(|id| *id == row))
            .unwrap_or(0);
        let last = positions.len() - 1;
        let target = if up {
            current.saturating_sub(1)
        } else if down {
            (current + 1).min(last)
        } else if page_up {
            current.saturating_sub(15)
        } else if page_down {
            (current + 15).min(last)
        } else if home {
            0
        } else if end {
            last
        } else {
            current
        };
        if target != current {
            self.commit_edits(project, response);
            self.row = Some(positions[target]);
            self.scroll_to_row = true;
            // Keep editing the same column in the next row.
            self.focus = focused;
        }
        if edit {
            self.focus = Some(Field::Checked);
        }
        if takeoff {
            self.commit_edits(project, response);
            self.takeoff = self.row;
        }
        let _ = id;
    }
}

fn is_number(col: Col) -> bool {
    matches!(
        col,
        Col::Ordered
            | Col::Previous
            | Col::Claimed
            | Col::Checked
            | Col::Progress
            | Col::Price
            | Col::Amount
    )
}

/// Short name of a source for the table.
fn short_basis(basis: CheckBasis) -> &'static str {
    match basis {
        CheckBasis::Claimed => "Rechnung",
        CheckBasis::Quantity => "geprüft",
        CheckBasis::Takeoff => "Aufmaß",
        CheckBasis::Progress => "Fortschritt",
    }
}

fn sense(mode: Mode) -> &'static str {
    match mode {
        Mode::Cumulative => " kum.",
        Mode::Increment => " Zuw.",
    }
}

/// Progress as percent with two decimals or as factor with four.
pub(crate) fn progress_text(factor: Decimal, percent: bool) -> String {
    if percent {
        format_decimal_de(factor * Decimal::ONE_HUNDRED, 2)
    } else {
        format_decimal_de(factor, 4)
    }
}
