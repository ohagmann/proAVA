//! Editing in the table: the active cell of the selected row, the edit box
//! over the cell, keys (Tab, Enter, Esc, Strg+Enter, ←/→, F2, typing) and
//! the unit suggestions. The logic lives in [`crate::cell_edit`].

use eframe::egui::{self, FontId, Key, Modifiers, Rect, RichText, Stroke};
use proava_domain::{Lv, LvRow, Project, RowId};

use super::{EditorOutput, LvEditor, UNITS};
use crate::cell_edit::{self, Cell, Shown};
use crate::columns::Columns;
use crate::lv_ops::Command;
use crate::theme::Palette;
use crate::undo::EditKey;

pub(super) struct CellState {
    /// Cell of the selected row the keys work on.
    pub active: Cell,
    pub edit: Option<CellEdit>,
    /// Columns of the last drawn row (for keys and clicks).
    pub columns: Option<Columns>,
    /// Rows marked from outside (LV-Vergleich).
    pub highlight: std::collections::HashSet<RowId>,
    /// KI-Markierung: rows an assistant works on (bridge).
    pub ai_marks: std::collections::HashSet<RowId>,
    /// Row (and below it) where dragged rows of the Übernahme-Fenster would land.
    pub drop_hover: Option<(RowId, bool)>,
    /// Pointer of a drag from another proAVA window (local bridge), in
    /// points of this window; None = no such drag.
    pub remote_pointer: Option<egui::Pos2>,
    /// Where that drag would land (row, below it), as of the last frame.
    pub remote_hover: Option<(RowId, bool)>,
    /// The rows were dropped there (taken by the shell).
    pub dropped: Option<(RowId, bool)>,
    /// Pass of the last key handling: the Enter that takes over a title's
    /// KG/LB must not answer the question it opens.
    pub pass: u64,
}

impl Default for CellState {
    fn default() -> Self {
        Self {
            active: Cell::Short,
            edit: None,
            columns: None,
            highlight: std::collections::HashSet::new(),
            ai_marks: std::collections::HashSet::new(),
            drop_hover: None,
            remote_pointer: None,
            remote_hover: None,
            dropped: None,
            pass: 0,
        }
    }
}

pub(super) struct CellEdit {
    pub row: RowId,
    pub cell: Cell,
    pub text: String,
    /// Text before editing (Esc puts the Kurztext back).
    pub original: String,
    pub error: Option<String>,
    pub focus: bool,
    /// Entered without typing: the content is selected, typing replaces it.
    pub select_all: bool,
    /// KG/LB: grey hint in the empty box („481 (vom Titel)“).
    pub hint: Option<String>,
    /// KG/LB: the list under the box – (text taken over, line shown).
    pub choices: Vec<(String, String)>,
    /// KG/LB: line of the filtered list chosen with ↑/↓ (Enter takes it).
    pub pick: Option<usize>,
    /// The chosen line moved: scroll the list to it.
    pub scroll_pick: bool,
}

impl CellEdit {
    /// KG/LB: the code of the line chosen with ↑/↓, if any.
    fn picked_code(&self) -> Option<String> {
        let at = self.pick?;
        cell_edit::matching_choices(&self.choices, &self.text)
            .get(at)
            .map(|(code, _)| code.clone())
    }

    /// ↑/↓ in the list of a KG/LB cell.
    fn move_pick(&mut self, down: bool) {
        let count = cell_edit::matching_choices(&self.choices, &self.text).len();
        if count == 0 {
            self.pick = None;
            return;
        }
        self.pick = Some(match (self.pick, down) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(at), true) => (at + 1).min(count - 1),
            (Some(at), false) => at.saturating_sub(1),
        });
        self.scroll_pick = true;
    }
}

/// Cells with a list of choices (Kostengruppe, Leistungsbereich).
fn has_choices(cell: Cell) -> bool {
    matches!(cell, Cell::CostGroup | Cell::WorkCategory)
}

/// What happened to the edit box in this frame (handled after the table).
#[derive(Default)]
pub(super) struct CellEvents {
    pub short_changed: bool,
    pub left: bool,
    pub unit_picked: Option<String>,
    /// A line of the KG/LB list was clicked: its code.
    pub choice_picked: Option<String>,
    /// A command of the row menu, chosen by a right click into the edit box.
    pub menu: Option<super::context_menu::MenuChoice>,
}

impl LvEditor {
    fn shown(&self) -> Shown {
        self.cells
            .columns
            .as_ref()
            .map(Shown::of)
            .unwrap_or(Shown::ALL)
    }

    /// Marks rows in the table (LV-Vergleich); empty = no marks.
    pub fn set_highlight(&mut self, rows: std::collections::HashSet<RowId>) {
        self.cells.highlight = rows;
    }

    /// KI-Markierung: a frame of its own colour around these rows; empty = none.
    pub fn set_ai_marks(&mut self, rows: std::collections::HashSet<RowId>) {
        self.cells.ai_marks = rows;
    }

    /// Pointer of a drag from another window (points of this window), or None.
    pub fn set_remote_pointer(&mut self, pointer: Option<egui::Pos2>) {
        if pointer.is_none() {
            self.cells.remote_hover = None;
        }
        self.cells.remote_pointer = pointer;
    }

    /// Where a drag from another window would land: the row and whether
    /// below it (as of the last frame).
    pub fn remote_drop_place(&self) -> Option<(RowId, bool)> {
        self.cells.remote_hover
    }

    /// The row under `pointer` (points of this window) and whether the
    /// pointer is in its lower half – from the rows drawn in the last frame.
    pub fn row_at(&self, lv: &Lv, pointer: egui::Pos2) -> Option<(RowId, bool)> {
        self.row_rects
            .iter()
            .find(|(_, rect)| rect.contains(pointer))
            .and_then(|(index, rect)| {
                lv.rows()
                    .get(*index)
                    .map(|row| (row.id(), pointer.y > rect.center().y))
            })
    }

    /// Rows with the KI-Markierung (for tests).
    pub fn ai_marks(&self) -> &std::collections::HashSet<RowId> {
        &self.cells.ai_marks
    }

    /// Whether a table cell is open for editing (for the workspace and tests).
    pub(crate) fn cell_editing(&self) -> bool {
        self.is_cell_editing()
    }

    /// The cell being edited (for the workspace and tests).
    pub(crate) fn editing_cell(&self) -> Option<Cell> {
        self.cells.edit.as_ref().map(|edit| edit.cell)
    }

    /// The row of the cell being edited.
    pub(crate) fn editing_row(&self) -> Option<proava_domain::RowId> {
        self.cells.edit.as_ref().map(|edit| edit.row)
    }

    pub(super) fn is_cell_editing(&self) -> bool {
        self.cells.edit.is_some()
    }

    /// The cell the table frames in `row` when it is selected (the active
    /// cell, or the one taking its place when the row has none such).
    pub(super) fn framed_cell(&self, row: &LvRow) -> Option<Cell> {
        let columns = self.cells.columns?;
        cell_edit::fit(row, Shown::of(&columns), self.cells.active)
    }

    /// Keys of the table cells; runs before the other keys of the editor.
    pub(super) fn cell_keys(&mut self, ctx: &egui::Context, project: &mut Project) -> EditorOutput {
        let mut output = EditorOutput::default();
        self.cells.pass = ctx.cumulative_pass_nr();
        if self.cells.edit.is_some() {
            // Exact modifiers: egui's consume_key matches „logically“, so the
            // pattern Strg+Enter also took Strg+Umschalt+Enter (new title)
            // and made a position of it.
            let consume = |modifiers: Modifiers, key: Key| {
                ctx.input_mut(|input| {
                    let at = input.events.iter().position(|event| {
                        matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers: held, .. }
                            if *pressed == key && held.matches_exact(modifiers))
                    });
                    at.map(|at| input.events.remove(at)).is_some()
                })
            };
            let choosing = self
                .cells
                .edit
                .as_ref()
                .is_some_and(|edit| has_choices(edit.cell));
            if choosing && consume(Modifiers::NONE, Key::ArrowDown) {
                if let Some(edit) = self.cells.edit.as_mut() {
                    edit.move_pick(true);
                }
            } else if choosing && consume(Modifiers::NONE, Key::ArrowUp) {
                if let Some(edit) = self.cells.edit.as_mut() {
                    edit.move_pick(false);
                }
            } else if consume(Modifiers::COMMAND, Key::Enter) {
                if self.commit_cell(project, &mut output) {
                    self.cells.edit = None;
                    let ctx = ctx.clone();
                    output.merge(self.run(Command::NewPosition, project, &ctx));
                    self.focus = None;
                    if let Some(index) = self.selected_index(project.lv()) {
                        output.merge(self.begin_cell(project, index, Cell::Short, None));
                    }
                }
            } else if consume(Modifiers::SHIFT, Key::Tab) {
                self.tab(project, false, &mut output);
            } else if consume(Modifiers::NONE, Key::Tab) {
                self.tab(project, true, &mut output);
            } else if consume(Modifiers::NONE, Key::Enter) {
                let edit_row = self.cells.edit.as_ref().map(|edit| (edit.row, edit.cell));
                if self.commit_cell(project, &mut output) {
                    self.cells.edit = None;
                    if let Some((row, cell)) = edit_row {
                        let lv = project.lv();
                        let visible = self.visible_for_keys(project);
                        if let Some(below) = lv.index_of(row).and_then(|index| {
                            cell_edit::row_below(lv, &visible, index, cell, self.shown())
                        }) {
                            let id = lv.rows()[below].id();
                            self.selected = Some(id);
                            self.single(id);
                            self.scroll_to_selected = true;
                        }
                    }
                }
            } else if consume(Modifiers::NONE, Key::Escape) {
                self.cancel_cell(project, &mut output);
            }
            return output;
        }
        // A text field is about to take the focus (new row: Kurztext or
        // Langtext): the typed text belongs there, not into a table cell.
        // Also not while a write-protected STLB-Bau Langtext has the focus.
        if ctx.text_edit_focused()
            || self.focus.is_some()
            || super::stlb_lock::locked_area_focused(ctx)
        {
            return output;
        }
        let Some(index) = self.selected_index(project.lv()) else {
            return output;
        };
        let shown = self.shown();
        let row = &project.lv().rows()[index];
        // Hints and Ausführungsbeschreibungen: their text cell opens with the
        // mouse (Klick 1, Klick 2); the keys stay as they were – Enter goes
        // into the hint text (Abnahme qa3), typing does not overwrite the
        // heading, the active cell of the positions stays.
        if !cell_edit::keyboard_cells(row) {
            return output;
        }
        let Some(active) = cell_edit::fit(row, shown, self.cells.active) else {
            return output;
        };
        self.cells.active = active;
        // The modifiers of the key event itself, not those at the end of the
        // frame: Strg+Enter with Strg already released in the same frame is
        // no plain Enter (it would put the previous row into editing).
        let pressed = |key: Key| {
            ctx.input(|input| {
                input.events.iter().any(|event| {
                    matches!(event, egui::Event::Key { key: pressed_key, pressed: true, modifiers, .. }
                        if *pressed_key == key && modifiers.is_none())
                })
            })
        };
        for (key, forward) in [(Key::ArrowLeft, false), (Key::ArrowRight, true)] {
            if pressed(key) {
                if let Some(next) = cell_edit::step(row, shown, active, forward) {
                    ctx.input_mut(|input| input.consume_key(Modifiers::NONE, key));
                    self.cells.active = next;
                }
            }
        }
        if pressed(Key::Enter) || pressed(Key::F2) {
            ctx.input_mut(|input| {
                input.consume_key(Modifiers::NONE, Key::Enter);
                input.consume_key(Modifiers::NONE, Key::F2);
            });
            // A locked active cell (STLB unit): the next one that can be edited.
            let cell = cell_edit::fit_editable(project.lv(), index, shown, self.cells.active)
                .unwrap_or(self.cells.active);
            output.merge(self.begin_cell(project, index, cell, None));
            return output;
        }
        // Typing starts editing with the typed text (like a spreadsheet).
        let typed: String = ctx.input(|input| {
            if input.modifiers.command || input.modifiers.alt {
                return String::new();
            }
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        });
        if !typed.is_empty() && !typed.trim().is_empty() {
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Text(_)))
            });
            // Typing never overwrites the OZ by accident: from the OZ column
            // it goes into the Kurztext (the OZ: F2 or double click).
            let cell = if self.cells.active == Cell::Ordinal {
                Cell::Short
            } else {
                self.cells.active
            };
            output.merge(self.begin_cell(project, index, cell, Some(typed)));
        }
        output
    }

    fn visible_for_keys(&self, project: &Project) -> Vec<usize> {
        let visible = crate::lv_ops::visible_rows(project.lv(), &self.collapsed);
        crate::status_view::filter_rows(project.lv(), visible, self.filter)
    }

    /// Starts editing `cell` of row `index`; `typed` replaces the content.
    pub(super) fn begin_cell(
        &mut self,
        project: &mut Project,
        index: usize,
        cell: Cell,
        typed: Option<String>,
    ) -> EditorOutput {
        let lv = project.lv();
        let Some(row) = lv.rows().get(index) else {
            return EditorOutput::default();
        };
        let Some(cell) = cell_edit::fit(row, self.shown(), cell) else {
            return EditorOutput::default();
        };
        if let Err(message) = cell_edit::may_edit(lv, index, cell) {
            return EditorOutput::toast(message);
        }
        let original = cell_edit::text(lv, index, cell);
        let id = row.id();
        // KG/LB: what applies without an own value, and the list.
        let hint = match cell {
            Cell::CostGroup => lv.effective_cost_group(index),
            Cell::WorkCategory => lv.effective_work_category(index),
            _ => None,
        }
        .filter(|(_, inherited)| *inherited)
        .map(|(code, _)| format!("{code} (vom Titel)"));
        let choices = cell_edit::choices(lv, cell);
        self.cells.active = cell;
        self.selected = Some(id);
        self.single(id);
        self.scroll_to_selected = true;
        self.cells.edit = Some(CellEdit {
            row: id,
            cell,
            text: typed.clone().unwrap_or_else(|| original.clone()),
            original,
            error: None,
            focus: true,
            select_all: typed.is_none(),
            hint,
            choices,
            pick: None,
            scroll_pick: false,
        });
        let mut output = EditorOutput::default();
        if typed.is_some() && cell == Cell::Short {
            // live in the edit panel as well
            self.apply_short_live(project, &mut output);
        }
        output
    }

    /// Commits the edited cell; false when it was refused (the box stays,
    /// with the message in red).
    pub(super) fn commit_cell(&mut self, project: &mut Project, output: &mut EditorOutput) -> bool {
        let Some(edit) = self.cells.edit.as_mut() else {
            return true;
        };
        let Some(index) = project.lv().index_of(edit.row) else {
            self.cells.edit = None;
            return true;
        };
        if has_choices(edit.cell) {
            return self.commit_title_value(project, output);
        }
        let before = project.lv().clone();
        match cell_edit::commit(project.lv_mut(), index, edit.cell, &edit.text) {
            Ok(changed) => {
                if changed {
                    self.undo.record(&before, None);
                    output.dirty = true;
                }
                self.undo.seal();
                self.numbers.loaded = None;
                self.cells.edit = None;
                true
            }
            Err(error) => {
                edit.error = Some(error);
                edit.focus = true;
                false
            }
        }
    }

    /// KG/LB: the line chosen in the list or the typed text, checked like the
    /// edit panel; a title's new value over finished positions asks first
    /// (the box closes, the question decides).
    fn commit_title_value(&mut self, project: &mut Project, output: &mut EditorOutput) -> bool {
        let Some(edit) = self.cells.edit.as_mut() else {
            return true;
        };
        if let Some(code) = edit.picked_code() {
            edit.text = code;
        }
        let code = match cell_edit::parsed_code(edit.cell, &edit.text) {
            Ok(Some(code)) => code,
            Ok(None) => return true,
            Err(error) => {
                edit.error = Some(error);
                edit.focus = true;
                return false;
            }
        };
        let column = if edit.cell == Cell::CostGroup {
            crate::columns::Column::CostGroup
        } else {
            crate::columns::Column::WorkCategory
        };
        let unchanged = code.as_deref().unwrap_or_default() == edit.original;
        let id = edit.row;
        self.cells.edit = None;
        self.numbers.loaded = None;
        if !unchanged {
            let pass = self.cells.pass;
            output.merge(self.set_title_values(project, column, &[id], code, Some(pass)));
        }
        self.undo.seal();
        true
    }

    fn tab(&mut self, project: &mut Project, forward: bool, output: &mut EditorOutput) {
        let Some((row, cell)) = self.cells.edit.as_ref().map(|edit| (edit.row, edit.cell)) else {
            return;
        };
        if !self.commit_cell(project, output) {
            return;
        }
        let Some(index) = project.lv().index_of(row) else {
            return;
        };
        // Locked cells (the unit of an STLB-Bau position) are passed over.
        if let Some(next) =
            cell_edit::step_editable(project.lv(), index, self.shown(), cell, forward)
        {
            output.merge(self.begin_cell(project, index, next, None));
        }
    }

    fn cancel_cell(&mut self, project: &mut Project, output: &mut EditorOutput) {
        let Some(edit) = self.cells.edit.take() else {
            return;
        };
        if edit.cell == Cell::Short {
            if let Some(index) = project.lv().index_of(edit.row) {
                if cell_edit::short_value(&project.lv().rows()[index]) != edit.original {
                    self.undo.record(
                        project.lv(),
                        Some(EditKey(format!("cell_short:{}", edit.row.0))),
                    );
                    set_short(project, index, &edit.original);
                    output.dirty = true;
                }
            }
        }
        self.undo.seal();
    }

    /// The Kurztext is shown in the edit panel while it is typed.
    fn apply_short_live(&mut self, project: &mut Project, output: &mut EditorOutput) {
        let Some(edit) = self.cells.edit.as_ref() else {
            return;
        };
        let Some(index) = project.lv().index_of(edit.row) else {
            return;
        };
        if cell_edit::short_value(&project.lv().rows()[index]) == edit.text {
            return;
        }
        self.undo.record(
            project.lv(),
            Some(EditKey(format!("cell_short:{}", edit.row.0))),
        );
        set_short(project, index, &edit.text);
        output.dirty = true;
    }

    /// Draws the active cell frame or the edit box of a row (in the row loop).
    // The row, its place and the row menu (for a right click into the edit box).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn cell_overlay(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        row: &LvRow,
        rect: Rect,
        indent: f32,
        selected: bool,
        menu: &super::context_menu::MenuState,
        events: &mut CellEvents,
    ) {
        let columns = self.widths.layout(rect.left(), rect.right());
        self.cells.columns = Some(columns);
        if egui::DragAndDrop::has_payload_of_type::<crate::transfer_view::TransferDrag>(ui.ctx()) {
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|pos| rect.contains(*pos))
            {
                let after = pointer.y > rect.center().y;
                self.cells.drop_hover = Some((row.id(), after));
                let y = if after { rect.bottom() } else { rect.top() };
                ui.painter().line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    Stroke::new(3.0, palette.accent),
                );
                if ui.ctx().input(|input| input.pointer.any_released()) {
                    self.cells.dropped = Some((row.id(), after));
                }
            }
        }
        if let Some(pointer) = self.cells.remote_pointer.filter(|pos| rect.contains(*pos)) {
            // A drag from another window: the same mark as for rows of the
            // Übernahme-Fenster.
            let after = pointer.y > rect.center().y;
            self.cells.remote_hover = Some((row.id(), after));
            let y = if after { rect.bottom() } else { rect.top() };
            ui.painter().line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(3.0, palette.accent),
            );
        }
        if self.cells.highlight.contains(&row.id()) {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    egui::pos2(rect.right() - 5.0, rect.top() + 2.0),
                    egui::vec2(4.0, rect.height() - 4.0),
                ),
                egui::CornerRadius::same(2),
                palette.warn,
            );
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::ZERO,
                palette.warn.gamma_multiply(0.07),
            );
        }
        if self.cells.ai_marks.contains(&row.id()) {
            // Fill a touch, frame clearly: the rows stay readable, the
            // selection (blue) stays visible inside.
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::ZERO,
                palette.ai.gamma_multiply(0.08),
            );
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                egui::CornerRadius::same(2),
                Stroke::new(2.0, palette.ai),
                egui::StrokeKind::Inside,
            );
        }
        let group = row.is_group();
        let editing_here = self
            .cells
            .edit
            .as_ref()
            .is_some_and(|edit| edit.row == row.id());
        if !editing_here {
            if selected && !self.is_multi() {
                if let Some(cell) = cell_edit::fit(row, Shown::of(&columns), self.cells.active) {
                    let (left, right) = cell_edit::span(&columns, cell, indent, group);
                    let frame =
                        Rect::from_x_y_ranges(left..=right, rect.top() + 3.0..=rect.bottom() - 3.0);
                    ui.painter().rect_stroke(
                        frame,
                        egui::CornerRadius::same(3),
                        Stroke::new(1.0, palette.accent.gamma_multiply(0.6)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            return;
        }
        let Some(edit) = self.cells.edit.as_mut() else {
            return;
        };
        let (left, right) = cell_edit::span(&columns, edit.cell, indent, group);
        // KG/LB are narrow: the box may reach over the next column while it
        // is edited (room for „481 (vom Titel)“).
        let min_width = if has_choices(edit.cell) { 118.0 } else { 40.0 };
        let cell_rect = Rect::from_x_y_ranges(
            left..=right.max(left + min_width),
            rect.top() + 2.0..=rect.bottom() - 2.0,
        );
        let id = egui::Id::unique(("cell_edit", edit.row, edit.cell as u8));
        let numeric = matches!(edit.cell, Cell::Quantity | Cell::Price);
        let hint = edit.hint.clone();
        let mut text_edit = egui::TextEdit::singleline(&mut edit.text)
            .id(id)
            .margin(egui::Margin::symmetric(4, 3))
            .font(if numeric || edit.cell == Cell::Ordinal {
                FontId::monospace(13.0)
            } else {
                FontId::proportional(13.5)
            });
        if numeric {
            text_edit = text_edit.align(egui::Align2::RIGHT_CENTER);
        }
        if let Some(hint) = hint {
            text_edit = text_edit.hint_text(hint);
        }
        // A right click into the edit box shows the row menu (the box lies
        // over the row and would swallow it), spelling suggestions on top.
        let mut chosen = None;
        let row_menu = |ui: &mut egui::Ui| {
            if let Some(choice) = super::context_menu::show(ui, menu, Some(row)) {
                chosen = Some(choice);
            }
        };
        let response = if edit.cell == Cell::Short {
            // Kurztext: with spell checking (needs the layout of the field).
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cell_rect));
            let output = text_edit
                .desired_width(cell_rect.width() - 8.0)
                .show(&mut child);
            let mut response = output.response.response.clone();
            if crate::spell::text_edit_with_menu(&child, &output, &mut edit.text, Some(row_menu)) {
                response.mark_changed();
            }
            response
        } else {
            let response = ui.put(cell_rect, text_edit);
            response.context_menu(row_menu);
            response
        };
        if chosen.is_some() {
            events.menu = chosen;
        }
        if std::mem::take(&mut edit.focus) {
            response.request_focus();
        }
        if response.gained_focus() {
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            let end = egui::text::CCursor::new(edit.text.chars().count());
            let start = if std::mem::take(&mut edit.select_all) {
                egui::text::CCursor::new(0)
            } else {
                end
            };
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(start, end)));
            state.store(ui.ctx(), id);
        }
        if response.changed() {
            edit.error = None;
            // Typing filters the list anew.
            edit.pick = None;
            if edit.cell == Cell::Short {
                events.short_changed = true;
            }
        }
        if response.lost_focus() {
            events.left = true;
        }
        let stroke = if edit.error.is_some() {
            palette.error
        } else {
            palette.accent
        };
        ui.painter().rect_stroke(
            cell_rect.expand(1.0),
            egui::CornerRadius::same(3),
            Stroke::new(1.5, stroke),
            egui::StrokeKind::Outside,
        );
        if has_choices(edit.cell) {
            // KG/LB: the list of the edit panel as a dropdown under the cell;
            // typing filters, ↑/↓ chooses, Enter or a click takes it.
            let matching = cell_edit::matching_choices(&edit.choices, &edit.text);
            let scroll = std::mem::take(&mut edit.scroll_pick);
            let pick = edit.pick;
            let error = edit.error.clone();
            let groups = edit.cell == Cell::CostGroup;
            egui::Area::new(id.with("choices"))
                .order(egui::Order::Foreground)
                .fixed_pos(cell_rect.left_bottom() + egui::vec2(0.0, 3.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(340.0);
                        if let Some(error) = &error {
                            ui.label(RichText::new(error).size(12.0).color(palette.error));
                        }
                        if matching.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Kein Eintrag der Liste passt – Enter übernimmt die Eingabe.",
                                )
                                .size(12.0)
                                .color(palette.text_dim),
                            );
                            return;
                        }
                        egui::ScrollArea::vertical()
                            .id_salt(id.with("choices_scroll"))
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for (at, (code, line)) in matching.iter().enumerate() {
                                    let mut text = RichText::new(line.as_str()).size(12.5);
                                    if groups && code.ends_with('0') {
                                        // Group lines bold, in the text colour
                                        // (strong alone is white in the popup).
                                        text = text.strong().color(palette.text);
                                    }
                                    let item = ui.selectable_label(pick == Some(at), text);
                                    if scroll && pick == Some(at) {
                                        item.scroll_to_me(None);
                                    }
                                    if item.clicked() {
                                        events.choice_picked = Some(code.clone());
                                    }
                                }
                            });
                    });
                });
            return;
        }
        let mut tip = edit.error.clone();
        let mut over_limit = false;
        if tip.is_none() && edit.cell == Cell::Quantity {
            tip = Some("Zahl oder Rechenansatz, z. B. 3*12,5+4".to_string());
        }
        if tip.is_none() && edit.cell == Cell::Short {
            // GAEB limits: Kurztext 70, Bezeichnung of an AB 55 (as in the
            // edit panel) – a hint, the text is taken over anyway.
            tip = cell_edit::short_limit_hint(row, &edit.text);
            over_limit = tip.is_some();
        }
        if let Some(text) = tip {
            let color = if edit.error.is_some() {
                palette.error
            } else if over_limit {
                palette.warn
            } else {
                palette.text_dim
            };
            egui::Area::new(id.with("tip"))
                .order(egui::Order::Foreground)
                .fixed_pos(cell_rect.left_bottom() + egui::vec2(0.0, 3.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(RichText::new(text).size(12.0).color(color));
                    });
                });
        }
        if edit.cell == Cell::Unit {
            let needle = edit.text.trim().to_lowercase();
            let matching: Vec<&str> = UNITS
                .iter()
                .copied()
                .filter(|unit| needle.is_empty() || unit.to_lowercase().starts_with(&needle))
                .filter(|unit| unit.to_lowercase() != needle)
                .collect();
            if !matching.is_empty() {
                egui::Area::new(id.with("units"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(cell_rect.left_bottom() + egui::vec2(0.0, 3.0))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_max_width(260.0);
                            ui.horizontal_wrapped(|ui| {
                                for unit in matching {
                                    if ui.button(unit).clicked() {
                                        events.unit_picked = Some(unit.to_string());
                                    }
                                }
                            });
                        });
                    });
            }
        }
    }

    /// After the table: live Kurztext, picked unit, leaving the box.
    pub(super) fn cell_events(
        &mut self,
        project: &mut Project,
        events: CellEvents,
        output: &mut EditorOutput,
    ) {
        if events.short_changed {
            self.apply_short_live(project, output);
        }

        if let Some(text) = events.unit_picked.or(events.choice_picked) {
            if let Some(edit) = self.cells.edit.as_mut() {
                edit.text = text;
                edit.pick = None;
            }
            self.commit_cell(project, output);
            return;
        }
        if events.left
            && self
                .cells
                .edit
                .as_ref()
                .is_some_and(|edit| edit.error.is_none())
        {
            if !self.commit_cell(project, output) {
                if let Some(edit) = self.cells.edit.as_mut() {
                    edit.focus = false;
                }
                output.toast = self.cells.edit.as_ref().and_then(|edit| edit.error.clone());
            }
        }
    }

    /// The cell under the pointer for a click in row `index`.
    pub(super) fn cell_at_pointer(&self, row: &LvRow, x: f32, indent: f32) -> Option<Cell> {
        let columns = self.cells.columns?;
        cell_edit::at_x(&columns, row, Shown::of(&columns), x, indent)
    }
}

fn set_short(project: &mut Project, index: usize, text: &str) {
    if let Some(row) = project.lv_mut().row_mut(index) {
        cell_edit::set_short_value(row, text);
    }
}
