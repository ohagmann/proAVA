//! LV editor: outline table with keyboard control and the edit panel.

use std::collections::HashSet;

use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Vec2};
use proava_application::{Launcher, PanelSide};
use proava_domain::{EditStatus, Lv, LvRow, Money, Position, PositionKind, Project, RowId};

use crate::columns::{Column, ColumnWidths};
use crate::find_replace_bar::{BarAction, FindReplace};
use crate::images;
use crate::lv_check::{self, SHORT_TEXT_LIMIT};
use crate::lv_ops::{self, Command};
use crate::numbers;
use crate::open_points;
use crate::rich_editor::RichEditor;
use crate::search_replace;
use crate::status_view::{self, RowMarks, StatusFilter};
use crate::struct_ops;
use crate::theme::Palette;
use crate::undo::{EditKey, UndoStack};
use crate::{strings, widgets};

mod context_menu;
mod convert;
mod cost_group;
mod details;
mod fields;
mod insert_renumbering;
mod library_fields;
mod multi_lv;
mod pdf_clip;
mod performance_panel;
mod point_section;
mod position_window;
mod split_rows;
mod stlb_lock;
mod surcharge_refs;
mod table_cells;
mod transfer;

pub use context_menu::{LvAction, MenuChoice, StructCommand, FREE_ENTRY};
pub use multi_lv::{head_name, JumpThen, LvJump, ViewRow};
pub(crate) use position_window::new_row_keys;
pub use stlb_lock::changed_stlb_positions;

const ROW_HEIGHT: f32 = 30.0;
/// How long a dropped GAEB file waits for the pointer position (ms).
const DROP_SETTLE_MS: u64 = 250;
const HEADER_HEIGHT: f32 = 26.0;
/// Room after the longest OZ: the 6 px the row keeps free plus the bold
/// titles.
const OZ_GAP: f32 = 10.0;
/// Name of the table head for screen readers and UI tests.
pub const HEADER_NAME: &str = "Spaltenkopf der LV-Tabelle";
/// Switch (menu Ansicht, menu of the table head) for the share of the title sums.
pub const TITLE_SHARE_LABEL: &str = "Anteil der Titelsummen in %";
/// Key of that switch in the settings (next to the columns „KG“ and „LB“).
const TITLE_SHARE_KEY: &str = "anteil";
const DETAIL_WIDTH: f32 = 420.0;
const MIN_DETAIL_WIDTH: f32 = 260.0;
const MIN_TABLE_WIDTH: f32 = 320.0;
/// In a narrow window (e.g. half a 1920 screen) the table keeps this share
/// of the width, up to [`TABLE_WIDTH_KEPT`]; the Eigenschaftenfenster gets
/// narrower and returns to its width when the window grows again.
const TABLE_SHARE: f32 = 0.64;
const TABLE_WIDTH_KEPT: f32 = 720.0;
/// Below this width (e.g. a quarter screen at 150 %) the Eigenschaftenfenster
/// steps aside and the table gets the whole width; the setting stays and the
/// panel comes back when the window is wider.
const DETAIL_ROOM_NEEDED: f32 = MIN_DETAIL_WIDTH + 480.0;
const INDENT: f32 = 12.0;
const TEXT_SIZE: f32 = 13.5;
const NUMBER_SIZE: f32 = 13.0;
const UNDO_STEPS: usize = 100;

/// Band at the upper and lower edge of the table where a drag from another
/// window scrolls (points), and how far per frame.
const REMOTE_EDGE: f32 = 28.0;
const REMOTE_SCROLL: f32 = 9.0;

pub const UNITS: [&str; 16] = [
    "St", "Stck", "m", "lfm", "m²", "m³", "kg", "t", "l", "h", "d", "Wo", "Mt", "psch", "%", "DP",
];

/// What the editor needs from the frame around it.
#[derive(Debug, Default)]
pub struct EditorOutput {
    pub dirty: bool,
    pub toast: Option<String>,
    /// Open the file dialog for images; the result goes to [`LvEditor::insert_image_files`].
    pub pick_images: bool,
    pub open_palette: bool,
    /// Open the dialog „Aus STLB-Bau übernehmen“ (see `stlb_dialog`).
    pub open_stlb: bool,
    /// Open the dialog „Aus AUSSCHREIBEN.DE übernehmen“ (same dialog, other source).
    pub open_ausschreiben: bool,
    /// A GAEB file or zip was dropped on the table: open the take-over dialog
    /// with it; the row under the pointer is selected as the place to insert.
    pub dropped_catalog: Option<std::path::PathBuf>,
    /// A command of the shell (Gliederung, LV prüfen, Neues LV …).
    pub lv_action: Option<LvAction>,
    /// Export this PDF (file dialog of the shell).
    pub pdf: Option<proava_application::PdfDocument>,
    /// Strg+S in a window of the editor (Eigenschaften): save the project.
    pub save: bool,
    /// Rows were copied (Strg+C/X): the shell puts them into the system
    /// clipboard for other proAVA windows (see `proava_application::clipboard`).
    pub copied: Option<Copied>,
    /// Strg+V / „Einfügen“: the shell looks into the system clipboard and
    /// calls [`LvEditor::paste_transfer`] or [`LvEditor::paste_internal`].
    /// The text is what the key event brought along (empty from a menu).
    pub paste: Option<String>,
    /// The selection goes into another LV of the table (click, arrow key,
    /// search hit): the shell makes it active, then [`LvEditor::land`].
    pub jump: Option<LvJump>,
    /// A command from the context menu of an LV head row.
    pub lv_list: Option<crate::lv_list::LvListAction>,
    /// Rows dragged from the active LV into another LV of the table.
    pub cross_drop: Option<CrossDrop>,
    /// „Alle ersetzen“ over all LVs of the project (one undo step for all):
    /// find, replacement, options.
    pub replace_all: Option<(String, String, search_replace::SearchOptions)>,
    /// Rechtschreibung: replace this word by that one in all LVs (one undo step).
    pub spell_replace: Option<(String, String)>,
}

/// Rows of the active LV dropped on another LV of the same table.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossDrop {
    /// Index of the target LV.
    pub to_lv: usize,
    /// Insert before this row of the target LV (its length = at the end).
    pub before: usize,
    /// The dragged rows of the active LV (titles with their block).
    pub ids: Vec<RowId>,
    /// Strg held: copy; else move.
    pub copy: bool,
}

/// Rows copied in the table, ready for the system clipboard.
#[derive(Debug, Clone)]
pub struct Copied {
    /// The rows with what they need (Ausführungsbeschreibungen, images).
    pub transfer: proava_domain::transfer::Transfer,
    pub mask: proava_domain::OzMask,
    /// Tab separated text (Excel), as before.
    pub plain: String,
    /// The same as an HTML table.
    pub html: String,
}

impl EditorOutput {
    fn merge(&mut self, other: EditorOutput) {
        self.dirty |= other.dirty;
        if other.toast.is_some() {
            self.toast = other.toast;
        }
        self.pick_images |= other.pick_images;
        self.open_palette |= other.open_palette;
        self.open_stlb |= other.open_stlb;
        self.open_ausschreiben |= other.open_ausschreiben;
        if other.dropped_catalog.is_some() {
            self.dropped_catalog = other.dropped_catalog;
        }
        self.save |= other.save;
        if other.lv_action.is_some() {
            self.lv_action = other.lv_action;
        }
        if other.pdf.is_some() {
            self.pdf = other.pdf;
        }
        if other.copied.is_some() {
            self.copied = other.copied;
        }
        if other.paste.is_some() {
            self.paste = other.paste;
        }
        if other.jump.is_some() {
            self.jump = other.jump;
        }
        if other.lv_list.is_some() {
            self.lv_list = other.lv_list;
        }
        if other.cross_drop.is_some() {
            self.cross_drop = other.cross_drop;
        }
        if other.replace_all.is_some() {
            self.replace_all = other.replace_all;
        }
        if other.spell_replace.is_some() {
            self.spell_replace = other.spell_replace;
        }
    }

    fn toast(message: impl Into<String>) -> Self {
        Self {
            toast: Some(message.into()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Short,
    Long,
}

struct NumberFields {
    loaded: Option<(RowId, u64)>,
    quantity: String,
    price: String,
    quantity_error: Option<String>,
    price_error: Option<String>,
}

struct Search {
    query: String,
    focus: bool,
}

enum Dialog {
    ConfirmDelete {
        index: usize,
        count: usize,
    },
    ConfirmDeleteMany {
        count: usize,
    },
    /// Neu nummerieren with preview.
    Renumber(context_menu::RenumberForm),
    /// Delete an Ausführungsbeschreibung that `users` positions refer to.
    RemovePerformance {
        id: RowId,
        users: usize,
    },
    /// A title's cost group changes that of finished positions below it.
    ConfirmCostGroup(cost_group::PendingCostGroup),
    /// Zeilenart umwandeln: what the new kind drops (see `convert`).
    ConvertRow(Box<convert::PendingConversion>),
    /// „In freie Position umwandeln“ for these STLB-Bau positions (see `stlb_lock`).
    ConvertFree(Vec<RowId>),
    /// No free order number where rows go in: insert and renumber the title?
    InsertRenumbering(Box<insert_renumbering::PendingRenumber>),
}

pub struct LvEditor {
    /// „Neu nummerieren“ ran after warning about the sent X83 (see
    /// [`LvEditor::take_sent_warned`]).
    sent_warned: bool,
    /// The newest change was kept in a question of the editor that named its
    /// consequences (Umwandeln): the shell asks no second time (see
    /// [`LvEditor::take_change_asked`]).
    change_asked: bool,
    selected: Option<RowId>,
    widths: ColumnWidths,
    /// Further selected rows besides `selected` (multiple selection).
    marked: HashSet<RowId>,
    /// Start of an Umschalt selection.
    anchor: Option<RowId>,
    collapsed: HashSet<RowId>,
    undo: UndoStack<Lv>,
    clipboard: Vec<LvRow>,
    clipboard_text: String,
    revision: u64,
    scroll_to_selected: bool,
    /// Scroll to this row instead of the selected one (KI-Markierung without
    /// a change of the selection).
    scroll_row: Option<RowId>,
    focus: Option<Focus>,
    scroll_offset: f32,
    viewport_height: f32,
    numbers: NumberFields,
    unit_chips: bool,
    /// Einheit, die gerade für mehrere Positionen getippt wird.
    multi_unit: Option<String>,
    long_text: RichEditor,
    long_text_owner: Option<(RowId, u64)>,
    /// Long text in its own window (see `crate::long_text_window`).
    big_text: Option<crate::long_text_window::BigText>,
    /// Windows „Eigenschaften“ and „Langtext“ in front of the main window
    /// while proAVA has the focus; the level of this frame.
    in_front: crate::window_level::InFront,
    window_level: egui::WindowLevel,
    /// Window „Eigenschaften“ of a row (double click, Alt+Enter).
    position_window: Option<position_window::PositionWindow>,
    /// Window of a Datenpunkt-Abschnitt (plant list of a Kosten-LV title).
    point_window: Option<point_section::PointWindow>,
    /// „Hier bearbeiten“ in the edit panel: close that window.
    close_position_window: bool,
    /// Row whose fields the edit panel showed in this frame (the window
    /// waits a frame instead of showing the same fields twice).
    panel_row: Option<RowId>,
    search: Option<Search>,
    dialog: Option<Dialog>,
    drag: Option<RowId>,
    /// GAEB file dropped on the table, waiting for the pointer position.
    pending_drop: Option<(std::path::PathBuf, f64)>,
    /// PDF dropped on the editor: opens „Ausschnitt aus PDF einfügen“.
    dropped_pdf: Option<(String, std::path::PathBuf)>,
    /// Rows drawn in the last frame with their rectangle (place of a drop).
    row_rects: Vec<(usize, Rect)>,
    /// Rows of the other LVs drawn in the last frame: (LV, row, rectangle).
    other_rects: Vec<(usize, usize, Rect)>,
    /// The table over all LVs as drawn in the last frame.
    view_rows: Vec<ViewRow>,
    /// Folded titles of the not active LVs and folded LV head rows.
    folding: multi_lv::Folding,
    /// Sums and depths of the not active LVs.
    others: multi_lv::OtherLvs,
    /// Bring the head row of this LV to the top in the next frame; with
    /// `true` the selection is kept in view as well (another LV became the
    /// active one), with `false` only the head counts (Gehe zu LV).
    scroll_head: Option<(usize, bool)>,
    detail_width: f32,
    /// Width of the Eigenschaftenfenster in the last frame.
    detail_shown: f32,
    width_dragging: bool,
    /// Suchen und Ersetzen (Strg+R).
    replace: Option<FindReplace>,
    /// Rechtschreibung prüfen (whole LV).
    spelling: Option<crate::spell_dialog::SpellDialog>,
    /// „Ausschnitt aus PDF einfügen …“ for the long text of a row.
    clip_dialog: Option<(
        crate::pdf_clip_dialog::PdfClipDialog,
        RowId,
        pdf_clip::ClipPlace,
    )>,
    /// The edit panel or the big window asked for that dialog.
    wants_clip: Option<(RowId, pdf_clip::ClipPlace)>,
    /// „In Kurztext-Positionen umwandeln“ clicked for this Datenpunkt-Abschnitt.
    wants_point_to_kurztext: Option<RowId>,
    /// Folder of the last PDF chosen for a clip.
    clip_folder: Option<std::path::PathBuf>,
    /// Status filter of the table.
    filter: StatusFilter,
    /// Red/yellow marks of missing values (copy of the setting).
    show_gaps: bool,
    /// The menu asked to flip the setting; written in the next frame.
    toggle_gaps: bool,
    /// Window „Offene Punkte“.
    open_points: Option<OpenPointsWindow>,
    /// Text typed into the field „Kostengruppe“ for these rows.
    cost_group_input: Option<cost_group::CostGroupInput>,
    /// Window „Kosten nach Kostengruppen“.
    cost_groups_open: bool,
    /// A menu switched a table column („KG“, „LB“) on or off; stored in the
    /// settings in the next frame.
    column_toggle: Option<(Column, bool)>,
    /// Share of each title sum in the LV net total, in the EP column of the
    /// title rows (copy of the setting).
    show_share: bool,
    /// The menu asked to flip [`Self::show_share`]; written in the next frame.
    toggle_share: bool,
    /// Editing in the table cells.
    cells: table_cells::CellState,
    /// The open project is a Positionsbibliothek (Stichwörter, Stand, Preisdatum).
    library_mode: bool,
    /// Today (ISO), from the launcher's clock – for „Stand auf heute“.
    today_iso: String,
}

/// State of the window „Offene Punkte“.
struct OpenPointsWindow {
    /// Count missing unit prices (Kostenvoranschlag).
    with_prices: bool,
    /// Kinds shown; empty = all.
    kinds: HashSet<open_points::OpenKind>,
}

/// Selection and folded titles of one LV (kept per LV by the workspace).
#[derive(Debug, Clone, Default)]
pub struct EditorView {
    selected: Option<RowId>,
    collapsed: HashSet<RowId>,
    /// Scroll position of the table (MERGE(projekt): je LV gemerkt).
    scroll_offset: f32,
}

impl LvEditor {
    pub fn new(
        lv: &Lv,
        detail_width: Option<f32>,
        column_widths: &std::collections::BTreeMap<String, f32>,
    ) -> Self {
        Self {
            sent_warned: false,
            change_asked: false,
            selected: lv.rows().first().map(LvRow::id),
            marked: HashSet::new(),
            widths: ColumnWidths::from_map(column_widths),
            anchor: lv.rows().first().map(LvRow::id),
            collapsed: HashSet::new(),
            undo: UndoStack::new(UNDO_STEPS),
            clipboard: Vec::new(),
            clipboard_text: String::new(),
            revision: 0,
            scroll_to_selected: true,
            scroll_row: None,
            focus: None,
            scroll_offset: 0.0,
            viewport_height: 400.0,
            numbers: NumberFields {
                loaded: None,
                quantity: String::new(),
                price: String::new(),
                quantity_error: None,
                price_error: None,
            },
            unit_chips: false,
            multi_unit: None,
            long_text: {
                let mut editor = RichEditor::new("long_text_panel");
                editor.pdf_clips = true;
                editor
            },
            long_text_owner: None,
            big_text: None,
            in_front: crate::window_level::InFront::default(),
            window_level: egui::WindowLevel::Normal,
            position_window: None,
            point_window: None,
            close_position_window: false,
            panel_row: None,
            search: None,
            dialog: None,
            drag: None,
            pending_drop: None,
            dropped_pdf: None,
            row_rects: Vec::new(),
            other_rects: Vec::new(),
            view_rows: Vec::new(),
            folding: multi_lv::Folding::default(),
            others: multi_lv::OtherLvs::default(),
            scroll_head: None,
            detail_width: detail_width.unwrap_or(DETAIL_WIDTH).max(MIN_DETAIL_WIDTH),
            detail_shown: 0.0,
            width_dragging: false,
            replace: None,
            spelling: None,
            clip_dialog: None,
            wants_clip: None,
            wants_point_to_kurztext: None,
            clip_folder: None,
            filter: StatusFilter::All,
            show_gaps: true,
            toggle_gaps: false,
            open_points: None,
            cost_group_input: None,
            cost_groups_open: false,
            column_toggle: None,
            show_share: false,
            toggle_share: false,
            cells: table_cells::CellState::default(),
            library_mode: false,
            today_iso: String::new(),
        }
    }

    /// The open project is a library: the edit panel shows its fields, and a
    /// new position goes into the title of the Schreibmarke (no new titles).
    pub fn set_library_mode(&mut self, on: bool) {
        self.library_mode = on;
    }

    /// Column positions of the last drawn table row (for tests).
    pub fn table_layout(&self) -> Option<crate::columns::Columns> {
        self.cells.columns
    }

    /// Whether a table column is switched on (for the menus and tests).
    pub fn column_shown(&self, column: Column) -> bool {
        self.widths.shown(column)
    }

    /// Whether the title rows show their share of the LV total (menus, tests).
    pub fn title_share_shown(&self) -> bool {
        self.show_share
    }

    /// Switches the share of the title sums on or off (remembered in the
    /// settings in the next frame); the message for the status line.
    fn toggle_title_share(&mut self) -> String {
        self.toggle_share = !self.toggle_share;
        self.show_share = !self.show_share;
        if self.show_share {
            "Anteil der Titelsummen an der LV-Summe netto an.".to_string()
        } else {
            "Anteil der Titelsummen aus.".to_string()
        }
    }

    /// Switches „KG“ or „LB“ on or off (remembered in the settings); the
    /// message for the status line.
    fn toggle_column(&mut self, column: Column) -> String {
        let on = !self.widths.shown(column);
        self.widths.set_shown(column, on);
        self.column_toggle = Some((column, on));
        let name = column.menu_label();
        if on {
            format!("{name} an (grau, kursiv = vom Titel geerbt).")
        } else {
            format!("{name} aus.")
        }
    }

    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some() || self.clip_dialog.is_some()
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// Call before the shell changes the LV (OZ mask, project data …).
    pub fn record(&mut self, lv: &Lv) {
        self.undo.record(lv, None);
    }

    // MERGE(projekt): Zugriffe für mehrere LVs je Projekt (Arbeitsbereich, history.rs).
    /// Steps really added to the undo stack so far (see `UndoStack::push_count`).
    pub fn undo_pushes(&self) -> u64 {
        self.undo.push_count()
    }

    /// Undo and redo steps the editor took so far (see `UndoStack::moves`).
    pub fn undo_moves(&self) -> (u64, u64) {
        self.undo.moves()
    }

    /// Every recorded change so far (also typing joined into one step).
    pub fn undo_records(&self) -> u64 {
        self.undo.record_count()
    }

    /// The LV before the newest recorded change.
    pub fn last_undo_lv(&self) -> Option<&Lv> {
        self.undo.last()
    }

    /// Whether „Neu nummerieren“ ran after its own warning about the sent
    /// X83 (then the shell does not ask a second time); resets.
    pub fn take_sent_warned(&mut self) -> bool {
        std::mem::take(&mut self.sent_warned)
    }

    /// Whether the newest change was kept in a question of the editor (it
    /// named Versand and bids already); resets.
    pub fn take_change_asked(&mut self) -> bool {
        std::mem::take(&mut self.change_asked)
    }

    /// Exchanges the undo stack with the one of another LV (switching LVs).
    pub fn swap_undo(&mut self, other: &mut UndoStack<Lv>) {
        std::mem::swap(&mut self.undo, other);
    }

    /// A fresh, empty undo stack with the editor's limit.
    pub fn new_undo_stack() -> UndoStack<Lv> {
        UndoStack::new(UNDO_STEPS)
    }

    /// What is selected and folded, to come back to it after switching LVs.
    pub fn view_state(&self) -> EditorView {
        EditorView {
            selected: self.selected,
            collapsed: self.collapsed.clone(),
            scroll_offset: self.scroll_offset,
        }
    }

    /// Shows another LV: its saved view or the first row.
    pub fn show_lv(&mut self, lv: &Lv, view: Option<EditorView>) {
        let view = view.unwrap_or_default();
        self.collapsed = view.collapsed;
        // Gemerkte Scrollposition; die Auswahl wird nur hereingeholt, wenn sie außerhalb liegt.
        self.scroll_offset = view.scroll_offset;
        self.selected = view
            .selected
            .filter(|id| lv.index_of(*id).is_some())
            .or_else(|| lv.rows().first().map(LvRow::id));
        self.marked.clear();
        self.anchor = self.selected;
        self.focus = None;
        self.dialog = None;
        self.multi_unit = None;
        self.scroll_to_selected = true;
        self.reload();
    }

    /// The active LV changes from the LV with key `from` to `to` (a click or
    /// a step into another LV of the table, undo, LV commands). The folded
    /// titles stay with each LV; with several LVs in one table the scroll
    /// position is the table's and stays, the selection comes into view.
    pub fn switch_lv(
        &mut self,
        from: u64,
        to: u64,
        lv: &Lv,
        view: Option<EditorView>,
        several: bool,
    ) {
        let scroll = self.scroll_offset;
        let own = std::mem::take(&mut self.collapsed);
        self.folding.titles.insert(from, own);
        let mut view = view.unwrap_or_default();
        view.collapsed = self.folding.titles.remove(&to).unwrap_or(view.collapsed);
        self.show_lv(lv, Some(view));
        if several {
            self.scroll_offset = scroll;
        }
        self.windows_follow_other_lv(lv);
    }

    /// The windows „Eigenschaften“ and „Langtext“ after a change of the
    /// active LV: they show the selection of the new LV (or its first row
    /// that fits) at once and are no longer held – a row id of the LV before
    /// may name another row here. Without a fitting row they close.
    fn windows_follow_other_lv(&mut self, lv: &Lv) {
        let selected = self.selected.filter(|id| lv.index_of(*id).is_some());
        if let Some(window) = &mut self.position_window {
            match selected.or_else(|| lv.rows().first().map(LvRow::id)) {
                Some(row) => window.other_lv(row),
                None => self.close_position_window = true,
            }
        }
        let has_text = |row: &LvRow| {
            matches!(
                row,
                LvRow::Position(_) | LvRow::Note(_) | LvRow::Performance(_)
            )
        };
        let text_row = selected
            .filter(|id| {
                lv.index_of(*id)
                    .is_some_and(|index| has_text(&lv.rows()[index]))
            })
            .or_else(|| lv.rows().iter().find(|row| has_text(row)).map(LvRow::id));
        match (self.big_text.as_mut(), text_row) {
            (Some(big), Some(row)) => big.other_lv(row),
            (Some(_), None) => self.big_text = None,
            (None, _) => {}
        }
    }

    /// After a [`LvJump`] (the shell made its LV active): select the row
    /// and do what follows (menu, window), or show the head row.
    pub fn land(&mut self, project: &Project, ctx: &egui::Context, jump: LvJump) {
        let lv = project.lv();
        match (jump.then, jump.row.filter(|id| lv.index_of(*id).is_some())) {
            (JumpThen::Head, _) | (_, None) => {
                self.scroll_head = Some((jump.lv, false));
                self.scroll_to_selected = true;
            }
            (then, Some(id)) => {
                // The row was in view (click, arrow key): no jump of the table.
                self.scroll_head = None;
                self.select(lv, id);
                match then {
                    JumpThen::Menu => context_menu::open_keyboard_menu(ctx, id),
                    JumpThen::Window => self.open_row_window(project.lv(), id),
                    JumpThen::Drag => self.drag = Some(id),
                    JumpThen::Select | JumpThen::Head => {}
                }
            }
        }
    }

    /// The first row of the next LV (`down`) or the last row of the
    /// previous one in the table, seen from the selected row – for the arrow
    /// keys at the edge of an LV. None with one LV or at the end.
    fn neighbour_lv_row(
        &self,
        project: &Project,
        selected: Option<usize>,
        down: bool,
    ) -> Option<LvJump> {
        let active = project.active_lv();
        let here = self
            .view_rows
            .iter()
            .position(|row| Some(*row) == selected.map(|index| ViewRow::Row(active, index)))?;
        let pick = |row: &ViewRow| match *row {
            ViewRow::Row(lv, index) if lv != active => {
                let id = project.lv_at(lv)?.rows().get(index)?.id();
                Some(LvJump {
                    lv,
                    row: Some(id),
                    then: JumpThen::Select,
                })
            }
            _ => None,
        };
        if down {
            self.view_rows[here + 1..].iter().find_map(pick)
        } else {
            self.view_rows[..here].iter().rev().find_map(pick)
        }
    }

    /// The table over all LVs as drawn last (tests, the bridge).
    pub fn view_rows(&self) -> &[ViewRow] {
        &self.view_rows
    }

    /// Another LV became the active one without a click into it (import, new
    /// LV, undo): its head row comes to the top, the selection into view.
    pub fn show_head_of(&mut self, lv: usize) {
        self.scroll_head = Some((lv, true));
        self.scroll_to_selected = true;
    }

    /// Folds or opens the head row of the LV with this key.
    pub fn toggle_lv_fold(&mut self, key: u64) {
        if !self.folding.lvs.remove(&key) {
            self.folding.lvs.insert(key);
        }
    }

    /// The LV was changed from outside: reload the edit fields.
    pub fn reload(&mut self) {
        self.revision += 1;
        self.cells.edit = None;
        self.long_text.reset();
        if let Some(big) = &mut self.big_text {
            big.editor.reset();
        }
        if let Some(window) = &mut self.position_window {
            window.reload();
        }
        if let Some(window) = &mut self.point_window {
            window.reload();
        }
    }

    /// Index of the current row (for actions of the shell, e.g. STLB-Bau).
    pub fn selected_row(&self, lv: &Lv) -> Option<usize> {
        self.selected_index(lv)
    }

    /// Places (in the shown order) of the rows the table shows completely,
    /// as of the last frame – for tests: is a row really in view?
    pub fn fully_shown_rows(&self) -> std::ops::Range<usize> {
        let first = (self.scroll_offset / ROW_HEIGHT - 0.01).ceil().max(0.0) as usize;
        let end =
            ((self.scroll_offset + self.viewport_height) / ROW_HEIGHT + 0.01).floor() as usize;
        first..end.max(first)
    }

    fn selected_index(&self, lv: &Lv) -> Option<usize> {
        self.selected.and_then(|id| lv.index_of(id))
    }

    /// Selects a row, opens its titles and scrolls to it (e.g. from the check list).
    pub fn select(&mut self, lv: &Lv, id: RowId) {
        if let Some(index) = lv.index_of(id) {
            for ancestor in lv_ops::ancestors(lv, index) {
                self.collapsed.remove(&ancestor);
            }
            if !self.visible(lv).contains(&index) {
                self.filter = StatusFilter::All;
            }
            self.selected = Some(id);
            self.single(id);
            self.scroll_to_selected = true;
        }
    }

    /// Selects several rows (the first is the current one), opens their
    /// titles, clears a status filter that would hide them and scrolls to
    /// the first – the bridge shows rows this way. Unknown ids are left out.
    pub fn select_rows(&mut self, lv: &Lv, ids: &[RowId]) {
        let known: Vec<RowId> = ids
            .iter()
            .copied()
            .filter(|id| lv.index_of(*id).is_some())
            .collect();
        let Some(&first) = known.first() else {
            return;
        };
        for id in &known {
            if let Some(index) = lv.index_of(*id) {
                for ancestor in lv_ops::ancestors(lv, index) {
                    self.collapsed.remove(&ancestor);
                }
            }
        }
        let visible = self.visible(lv);
        if known
            .iter()
            .filter_map(|id| lv.index_of(*id))
            .any(|index| !visible.contains(&index))
        {
            self.filter = StatusFilter::All;
        }
        self.selected = Some(first);
        self.marked = known.iter().copied().collect();
        self.anchor = Some(first);
        self.scroll_row = None;
        self.scroll_to_selected = true;
    }

    /// Brings a row into view without changing the selection (its titles open).
    pub fn scroll_to_row(&mut self, lv: &Lv, id: RowId) {
        let Some(index) = lv.index_of(id) else {
            return;
        };
        for ancestor in lv_ops::ancestors(lv, index) {
            self.collapsed.remove(&ancestor);
        }
        if !self.visible(lv).contains(&index) {
            self.filter = StatusFilter::All;
        }
        self.scroll_row = Some(id);
        self.scroll_to_selected = true;
    }

    /// The rows a drag in the table takes along (the dragged row, or the
    /// whole selection when the dragged row is part of it); None without a drag.
    pub fn dragged_rows(&self, lv: &Lv) -> Option<Vec<RowId>> {
        let dragged = self.drag?;
        lv.index_of(dragged)?;
        if self.is_multi() && self.selection_ids().contains(&dragged) {
            let roots = lv_ops::selection_roots(lv, &self.selection_ids());
            return Some(roots.iter().map(|&index| lv.rows()[index].id()).collect());
        }
        Some(vec![dragged])
    }

    /// Esc while dragging rows into another window: the drag ends without
    /// doing anything here.
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    /// Pastes rows from another window (or project) after row `after`
    /// (None = before the first row) – the way of Strg+V, one undo step.
    pub fn paste_transfer_at(
        &mut self,
        project: &mut Project,
        transfer: &proava_domain::transfer::Transfer,
        after: Option<usize>,
    ) -> EditorOutput {
        let lv = project.lv();
        match after {
            Some(index) => {
                if let Some(row) = lv.rows().get(index) {
                    let id = row.id();
                    self.selected = Some(id);
                    self.single(id);
                }
                self.paste_transfer(project, transfer)
            }
            None => {
                let Some(first) = lv.rows().first().map(LvRow::id) else {
                    self.selected = None;
                    return self.paste_transfer(project, transfer);
                };
                // Behind the first row, then the new rows move up – still
                // one undo step: the undo point is the LV before the paste.
                self.selected = Some(first);
                self.single(first);
                let ids: HashSet<RowId> = lv.rows().iter().map(LvRow::id).collect();
                let output = self.paste_transfer(project, transfer);
                if !output.dirty {
                    // The question could only insert behind the first row.
                    return match self.withdraw_renumbering() {
                        Some(error) => EditorOutput::toast(error),
                        None => output,
                    };
                }
                let new: Vec<usize> = project
                    .lv()
                    .rows()
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| !ids.contains(&row.id()))
                    .map(|(index, _)| index)
                    .collect();
                if let (Some(&start), Some(&end)) = (new.first(), new.last()) {
                    let moved = struct_ops::move_rows_to(project.lv_mut(), start..end + 1, 0);
                    if moved.is_ok() {
                        let first_new = project.lv().rows()[0].id();
                        self.selected = Some(first_new);
                        self.single(first_new);
                        self.scroll_to_selected = true;
                    }
                }
                output
            }
        }
    }

    /// Inserts `transfer` into the active LV before its row `before` (its
    /// length: at the end), new order numbers after this LV's mask, and
    /// selects the new rows. No undo step – the caller records one (rows
    /// dragged in from another LV of the table change two LVs at once).
    pub fn place_transfer(
        &mut self,
        project: &mut Project,
        transfer: &proava_domain::transfer::Transfer,
        before: usize,
    ) -> Result<usize, String> {
        let mut transfer = transfer.clone();
        if project.lv().level_count() == 0 {
            transfer.rows.retain(|row| !row.is_group());
        }
        let old: HashSet<RowId> = project.lv().rows().iter().map(LvRow::id).collect();
        let length = project.lv().rows().len();
        let anchor = match before.checked_sub(1) {
            Some(after) => Some(after.min(length.saturating_sub(1))),
            None => (length > 0).then_some(0),
        }
        .filter(|_| length > 0);
        proava_domain::transfer::insert(project, anchor, &transfer)?;
        let new: Vec<usize> = project
            .lv()
            .rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| !old.contains(&row.id()))
            .map(|(index, _)| index)
            .collect();
        if before == 0 && length > 0 {
            // Behind the first row, then to the front (like pasting there).
            if let (Some(&start), Some(&end)) = (new.first(), new.last()) {
                let _ = struct_ops::move_rows_to(project.lv_mut(), start..end + 1, 0);
            }
        }
        let ids: Vec<RowId> = project
            .lv()
            .rows()
            .iter()
            .map(LvRow::id)
            .filter(|id| !old.contains(id))
            .collect();
        self.reload();
        self.select_rows(project.lv(), &ids);
        Ok(ids.len())
    }

    /// Opens the window „Eigenschaften“ of a row (as a double click does).
    pub fn show_position_window(&mut self, id: RowId) {
        self.open_position_window(id);
    }

    /// The selected rows in LV order, the current one first.
    pub fn selection_in_order(&self, lv: &Lv) -> Vec<RowId> {
        let ids = self.selection_ids();
        let mut ordered: Vec<RowId> = lv
            .rows()
            .iter()
            .map(LvRow::id)
            .filter(|id| ids.contains(id))
            .collect();
        if let Some(current) = self.selected.filter(|id| ordered.contains(id)) {
            ordered.retain(|id| *id != current);
            ordered.insert(0, current);
        }
        ordered
    }

    /// Rows the table shows: open titles and the status filter.
    fn visible(&self, lv: &Lv) -> Vec<usize> {
        status_view::filter_rows(lv, lv_ops::visible_rows(lv, &self.collapsed), self.filter)
    }

    /// Back to a single selection on `id`.
    fn single(&mut self, id: RowId) {
        self.marked.clear();
        self.anchor = Some(id);
    }

    /// All selected rows (the current one included).
    pub fn selection_ids(&self) -> HashSet<RowId> {
        let mut ids = self.marked.clone();
        ids.extend(self.selected);
        ids
    }

    fn is_multi(&self) -> bool {
        self.selection_ids().len() > 1
    }

    /// One line for the status bar: what is selected.
    pub fn selection_info(&self, lv: &Lv) -> String {
        if self.is_multi() {
            let roots = lv_ops::selection_roots(lv, &self.selection_ids());
            return format!(
                "{} Zeilen gewählt ({} mit Inhalt)",
                roots.len(),
                lv_ops::roots_len(lv, &roots)
            );
        }
        match self.selected_index(lv).map(|index| &lv.rows()[index]) {
            Some(LvRow::Group(group)) => {
                format!("Titel {} · Stufe {}", group.ordinal(), group.level() + 1)
            }
            Some(LvRow::Position(position)) => match position.status() {
                EditStatus::Open => format!("Position {}", position.ordinal()),
                status => format!("Position {} · {}", position.ordinal(), status.label()),
            },
            Some(LvRow::Note(note)) if note.is_blank() => "Hinweis ohne Text".to_string(),
            Some(LvRow::Note(_)) => "Hinweis".to_string(),
            Some(LvRow::Performance(performance)) => {
                format!("Ausführungsbeschreibung {}", performance.mark())
            }
            Some(LvRow::Spacer(_)) => "Abstandshalter (nur für die Bearbeitung)".to_string(),
            None => "Keine Auswahl".to_string(),
        }
    }

    /// Adds image files (name, bytes) to the project and to the selected long text.
    pub fn insert_images(
        &mut self,
        project: &mut Project,
        files: Vec<(String, Vec<u8>)>,
    ) -> EditorOutput {
        let Some(index) = self.selected_index(project.lv()) else {
            return EditorOutput::toast("Bitte zuerst eine Position oder einen Hinweis wählen.");
        };
        if project.lv().rows()[index].is_group() {
            return EditorOutput::toast(
                "Bilder gehören in den Langtext einer Position oder eines Hinweises.",
            );
        }
        if let Err(message) = project.lv().ensure_editable_rows(&[index]) {
            return EditorOutput::toast(message);
        }
        // Images belong to the text: not into an STLB-Bau text.
        if let Err(message) = project
            .lv()
            .ensure_text_editable(&[project.lv().rows()[index].id()])
        {
            return EditorOutput::toast(message);
        }
        let mut added = 0;
        let mut refused = Vec::new();
        for (name, bytes) in files {
            if !images::is_supported_image(&bytes) {
                refused.push(name);
                continue;
            }
            if added == 0 {
                self.undo.record(project.lv(), None);
            }
            let file = project.add_image(&name, bytes);
            let row = project.lv().rows()[index].id();
            let editor = match &mut self.big_text {
                Some(big) if big.row == row => &mut big.editor,
                _ => &mut self.long_text,
            };
            let text = editor.insert_image(file);
            let complements = editor.complements();
            set_long_text(project, index, text);
            set_complements(project, index, complements);
            added += 1;
        }
        let mut output = EditorOutput {
            dirty: added > 0,
            ..EditorOutput::default()
        };
        output.toast = Some(match (added, refused.is_empty()) {
            (0, _) => format!("Kein PNG/JPEG: {}", refused.join(", ")),
            (count, true) => format!(
                "{} eingefügt.",
                crate::format::count(count, "Bild", "Bilder")
            ),
            (count, false) => format!(
                "{} eingefügt; übergangen: {}",
                crate::format::count(count, "Bild", "Bilder"),
                refused.join(", ")
            ),
        });
        output
    }

    /// Reads image files from disk and inserts them.
    pub fn insert_image_files(
        &mut self,
        project: &mut Project,
        launcher: &Launcher,
        paths: &[std::path::PathBuf],
    ) -> EditorOutput {
        let mut files = Vec::new();
        for path in paths {
            match launcher.read_file(path) {
                Ok(bytes) => files.push((crate::file_dialog::file_name(path), bytes)),
                Err(error) => return EditorOutput::toast(error),
            }
        }
        self.insert_images(project, files)
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        keys_enabled: bool,
    ) -> EditorOutput {
        let ctx = ui.ctx().clone();
        let mut output = EditorOutput::default();
        crate::spell::configure(launcher);
        self.today_iso = launcher.now_iso();
        self.panel_row = None;
        if std::mem::take(&mut self.toggle_gaps) {
            launcher.set_mark_gaps(!launcher.mark_gaps());
        }
        self.show_gaps = launcher.mark_gaps();
        // Columns „KG“ and „LB“: the same in every LV and window, remembered.
        if let Some((column, on)) = self.column_toggle.take() {
            launcher.set_table_column(column.key(), on);
        }
        for column in Column::OPTIONAL {
            self.widths.set_shown(
                column,
                launcher.table_column(column.key(), column.shown_by_default()),
            );
        }
        // Share of the title sums: like the columns, remembered (default off).
        if std::mem::take(&mut self.toggle_share) {
            launcher.set_table_column(
                TITLE_SHARE_KEY,
                !launcher.table_column(TITLE_SHARE_KEY, false),
            );
        }
        self.show_share = launcher.table_column(TITLE_SHARE_KEY, false);
        // Width of the longest OZ (or „A0003“) of all LVs in the table: a
        // narrow table shrinks the OZ column to it before it drops Einheit or EP.
        let longest = multi_lv::shown(project)
            .into_iter()
            .filter_map(|index| project.lv_at(index))
            .flat_map(|lv| lv.rows())
            .map(|row| match row {
                LvRow::Performance(performance) => performance.mark().chars().count(),
                other => other.ordinal().chars().count(),
            })
            .max()
            .unwrap_or(0);
        let digit = ctx.fonts_mut(|fonts| fonts.glyph_width(&FontId::monospace(12.5), '0'));
        self.widths
            .set_oz_need((longest > 0).then(|| longest as f32 * digit + OZ_GAP));

        if self
            .selected
            .is_some_and(|id| project.lv().index_of(id).is_none())
        {
            self.selected = None;
        }
        if !self.marked.is_empty() {
            let lv = project.lv();
            self.marked.retain(|id| lv.index_of(*id).is_some());
        }

        // The window of a Datenpunkt-Abschnitt has a table of its own: the LV
        // table leaves the keys to it (Strg+Z goes through the workspace).
        if keys_enabled && !self.has_dialog() && self.point_window.is_none() {
            output.merge(self.keyboard(&ctx, project));
        }
        output.merge(self.dropped_files(&ctx, project));

        // Strg+Z/Y in a text field first undoes the field; when the field has
        // nothing left (no change this frame), the LV step is undone instead.
        let field_history = (keys_enabled && self.dialog.is_none() && ctx.text_edit_focused())
            .then(|| field_history_key(&ctx))
            .flatten();
        let records_before = self.undo.record_count();
        let pushes_before = self.undo.push_count();
        let lv_before = (field_history == Some(Command::Undo)).then(|| project.lv().clone());

        if launcher.detail_visible() && ui.available_width() >= DETAIL_ROOM_NEEDED {
            self.detail_panel(ui, palette, project, launcher, &mut output);
        } else {
            // No field would take it; typing in the table stays possible.
            self.focus = None;
        }
        self.table_panel(ui, palette, project, launcher, &mut output);
        self.dialogs(ui, palette, project, &mut output);
        if self.big_text.is_some() || self.position_window.is_some() {
            self.window_level = self.in_front.level(
                &ctx,
                launcher.windows_in_front(),
                &[
                    egui::ViewportId::ROOT,
                    position_window::PositionWindow::viewport_id(),
                    crate::long_text_window::BigText::id(),
                ],
            );
            if let Some(big) = &mut self.big_text {
                big.level = self.window_level;
            }
        }
        self.big_text_window(&ctx, palette, project, launcher, &mut output);
        if let Some(id) = self.wants_point_to_kurztext.take() {
            let index = project.lv().index_of(id);
            output.merge(self.point_section_to_kurztext(project, index));
        }
        if let Some((row, place)) = self.wants_clip.take() {
            self.open_clip_dialog(launcher, row, place);
        }
        if let Some((name, path)) = self.dropped_pdf.take() {
            let row = self
                .selected_index(project.lv())
                .filter(|index| !project.lv().rows()[*index].is_group())
                .map(|index| project.lv().rows()[index].id());
            match (row, launcher.read_file(&path)) {
                (Some(row), Ok(bytes)) => output.merge(self.open_clip_dialog_with(launcher, row, &name, bytes)),
                (None, _) => output.merge(EditorOutput::toast(
                    "Für einen PDF-Ausschnitt bitte zuerst eine Position oder einen Hinweis wählen.",
                )),
                (_, Err(error)) => output.merge(EditorOutput::toast(error)),
            }
        }
        self.clip_dialog_ui(ui, palette, project, launcher, &mut output);
        self.spelling_window(&ctx, palette, project, &mut output);
        self.position_window_ui(&ctx, palette, project, launcher, &mut output);
        self.open_points_window(&ctx, palette, project, &mut output);
        self.cost_groups_window(&ctx, palette, project, &mut output);
        self.point_window_ui(&ctx, palette, project, &mut output);

        if let Some(command) = field_history {
            if self.undo.record_count() == records_before {
                output.merge(self.run(command, project, &ctx));
            } else if let Some(before) = lv_before {
                // The field took back its own typing: that is the undo step.
                let pushed = self.undo.push_count() > pushes_before;
                self.undo.field_undone(pushed, before, project.lv());
            }
        }

        if output.open_palette {
            self.undo.seal();
        }
        output
    }

    fn keyboard(&mut self, ctx: &egui::Context, project: &mut Project) -> EditorOutput {
        let mut output = self.cell_keys(ctx, project);
        let text_focus = ctx.text_edit_focused() || self.is_cell_editing();
        let events = ctx.input(|input| input.events.clone());
        let mut commands = Vec::new();
        let mut structure = Vec::new();
        // Keys of new rows taken while typing: no text field may also act on
        // them (egui deletes a character on Strg+H).
        let mut taken: Vec<(egui::Key, egui::Modifiers)> = Vec::new();
        for event in events {
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if text_focus && context_menu::new_row_for_key(key, modifiers).is_some() {
                        taken.push((key, modifiers));
                    }
                    if context_menu::opens_menu(key, modifiers) && !text_focus {
                        if let Some(id) = self.selected {
                            self.scroll_to_selected = true;
                            context_menu::open_keyboard_menu(ctx, id);
                        }
                    } else if let Some(command) = StructCommand::for_key(key, modifiers) {
                        structure.push(command);
                    } else if let Some(command) = lv_ops::command_for_key(key, modifiers) {
                        let while_typing = matches!(
                            command,
                            Command::NewPosition
                                | Command::NewPositionBefore
                                | Command::NewTitle
                                | Command::NewNote
                                | Command::NewSpacer
                                | Command::FromStlb
                                | Command::FromAusschreiben
                                | Command::Find
                                | Command::Palette
                                | Command::FindNext
                                | Command::FindReplace
                                | Command::SpellCheck
                                | Command::ToggleDone
                                | Command::SetStatus(_)
                        );
                        if !text_focus || while_typing {
                            commands.push(command);
                        }
                    }
                }
                egui::Event::Copy if !text_focus => commands.push(Command::Copy),
                egui::Event::Cut if !text_focus => commands.push(Command::Cut),
                egui::Event::Paste(text) if !text_focus => {
                    output.paste = Some(text.clone());
                }
                egui::Event::PasteImage(image) => {
                    let selected_has_text = self
                        .selected_index(project.lv())
                        .is_some_and(|index| !project.lv().rows()[index].is_group());
                    if selected_has_text {
                        match images::encode_png(&image) {
                            Ok(bytes) => output.merge(self.insert_images(
                                project,
                                vec![("Zwischenablage.png".into(), bytes)],
                            )),
                            Err(error) => output.toast = Some(error),
                        }
                    }
                }
                _ => {}
            }
        }
        if !taken.is_empty() {
            ctx.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(event, egui::Event::Key { key, modifiers, .. }
                        if taken.contains(&(*key, *modifiers)))
                })
            });
        }
        if !text_focus && ctx.memory(|memory| memory.focused().is_none()) {
            // Tab and the arrow keys belong to the table, not to egui's focus walk.
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
        // A cell being edited is taken over first (as with Strg+Enter): the
        // new row then gets the focus, and the open cell does not catch the
        // next keys. If the value is refused, the command waits.
        if (!commands.is_empty() || !structure.is_empty()) && self.is_cell_editing() {
            if self.commit_cell(project, &mut output) {
                self.cells.edit = None;
            } else {
                commands.clear();
                structure.clear();
            }
        }
        for command in commands {
            output.merge(self.run(command, project, ctx));
        }
        for command in structure {
            output.merge(self.run_struct(command, project, ctx));
        }
        output
    }

    fn dropped_files(&mut self, ctx: &egui::Context, project: &mut Project) -> EditorOutput {
        let mut output = self.pending_drop_output(ctx, project);
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped.is_empty() {
            return output;
        }
        // GAEB files and zips: take-over dialog at the place of the drop.
        // winit tells the position only after the drop (X11, Windows), so the
        // row is found a moment later (see `pending_drop_output`).
        let (catalogs, others): (Vec<_>, Vec<_>) = dropped.into_iter().partition(|file| {
            crate::file_dialog::matches_extension(file.path(), &catalog_extensions())
        });
        if let Some(path) = catalogs.first().map(|file| file.path().to_path_buf()) {
            self.pending_drop = Some((path, ctx.input(|input| input.time)));
            ctx.request_repaint_after(std::time::Duration::from_millis(DROP_SETTLE_MS));
        }
        // A PDF: „Ausschnitt aus PDF einfügen“ with it (into the selected text).
        let (pdfs, others): (Vec<_>, Vec<_>) = others.into_iter().partition(|file| {
            crate::file_dialog::matches_extension(file.path(), &["pdf".to_string()])
        });
        if let Some(pdf) = pdfs.first() {
            self.dropped_pdf = Some((
                crate::file_dialog::file_name(pdf.path()),
                pdf.path().to_path_buf(),
            ));
        }
        if others.is_empty() {
            return output;
        }
        let mut files = Vec::new();
        for file in others {
            match file.bytes() {
                Ok(bytes) => files.push((crate::file_dialog::file_name(file.path()), bytes)),
                Err(error) => return EditorOutput::toast(error),
            }
        }
        output.merge(self.insert_images(project, files));
        output
    }

    /// A dropped GAEB file waits [`DROP_SETTLE_MS`] for the pointer position
    /// (it arrives after the drop); then the row under the pointer becomes the
    /// selection – the dialog inserts after it or fills it (placeholder) – and
    /// the take-over dialog opens with the file.
    fn pending_drop_output(&mut self, ctx: &egui::Context, project: &Project) -> EditorOutput {
        let Some((_, since)) = &self.pending_drop else {
            return EditorOutput::default();
        };
        let now = ctx.input(|input| input.time);
        if (now - since) * 1000.0 < DROP_SETTLE_MS as f64 {
            ctx.request_repaint_after(std::time::Duration::from_millis(DROP_SETTLE_MS));
            return EditorOutput::default();
        }
        let Some((path, _)) = self.pending_drop.take() else {
            return EditorOutput::default();
        };
        let pointer = ctx.input(|input| input.pointer.latest_pos());
        if let Some(index) = pointer.and_then(|pos| drop_row(&self.row_rects, pos)) {
            if let Some(row) = project.lv().rows().get(index) {
                self.selected = Some(row.id());
                self.single(row.id());
                self.scroll_to_selected = true;
            }
        }
        EditorOutput {
            dropped_catalog: Some(path),
            ..EditorOutput::default()
        }
    }

    /// Runs one command (keyboard, menu, context menu, command palette).
    pub fn run(
        &mut self,
        command: Command,
        project: &mut Project,
        ctx: &egui::Context,
    ) -> EditorOutput {
        let mut output = EditorOutput::default();
        let selected = self.selected_index(project.lv());
        let visible = self.visible(project.lv());
        let position = selected.and_then(|index| visible.iter().position(|&row| row == index));
        let page = (self.viewport_height / ROW_HEIGHT).floor().max(1.0) as usize;
        let last = visible.len().saturating_sub(1);

        if let Err(message) = self.check_lock(command, project.lv()) {
            return EditorOutput::toast(message);
        }

        if self.is_multi() {
            match command {
                Command::Delete
                | Command::Copy
                | Command::Cut
                | Command::MoveUp
                | Command::MoveDown => return self.run_multi(command, project),
                Command::ExtendUp
                | Command::ExtendDown
                | Command::Undo
                | Command::Redo
                | Command::Find
                | Command::FindNext
                | Command::Palette
                | Command::Renumber
                | Command::CollapseAll
                | Command::ExpandAll
                | Command::Paste
                | Command::SetStatus(_)
                | Command::ToggleDone
                | Command::ConfirmPriceCheck
                | Command::FindReplace
                | Command::SpellCheck
                | Command::Filter(_)
                | Command::ToggleGapMarks
                | Command::OpenPoints
                | Command::CostGroups
                | Command::ToggleCostGroupColumn
                | Command::ToggleWorkCategoryColumn
                | Command::ToggleTitleShare => {}
                _ => self.marked.clear(),
            }
        }

        let select_visible = |editor: &mut Self, target: usize| {
            if let Some(&row) = visible.get(target) {
                let id = project.lv().rows()[row].id();
                editor.selected = Some(id);
                editor.single(id);
                editor.scroll_to_selected = true;
            }
        };

        match command {
            Command::ExtendUp | Command::ExtendDown => {
                let target = match (position, command) {
                    (Some(at), Command::ExtendUp) => at.saturating_sub(1),
                    (Some(at), _) => (at + 1).min(last),
                    (None, _) => 0,
                };
                if let Some(&row) = visible.get(target) {
                    let anchor = self
                        .anchor
                        .and_then(|id| project.lv().index_of(id))
                        .or(selected)
                        .unwrap_or(row);
                    self.anchor = Some(project.lv().rows()[anchor].id());
                    self.marked = lv_ops::span_ids(project.lv(), &visible, anchor, row)
                        .into_iter()
                        .collect();
                    self.selected = Some(project.lv().rows()[row].id());
                    self.scroll_to_selected = true;
                }
            }
            // At the edge of the LV the arrow keys go on into the next LV of the table.
            Command::SelectUp | Command::PageUp if position == Some(0) => {
                output.jump = self.neighbour_lv_row(project, selected, false);
            }
            Command::SelectDown | Command::PageDown if position == Some(last) => {
                output.jump = self.neighbour_lv_row(project, selected, true);
            }
            Command::SelectUp => {
                select_visible(self, position.map(|at| at.saturating_sub(1)).unwrap_or(0))
            }
            Command::SelectDown => {
                select_visible(self, position.map(|at| (at + 1).min(last)).unwrap_or(0))
            }
            Command::PageUp => select_visible(
                self,
                position.map(|at| at.saturating_sub(page)).unwrap_or(0),
            ),
            Command::PageDown => {
                select_visible(self, position.map(|at| (at + page).min(last)).unwrap_or(0))
            }
            Command::First => select_visible(self, 0),
            Command::Last => select_visible(self, last),
            Command::Collapse => {
                if let Some(index) = selected {
                    let row = &project.lv().rows()[index];
                    if row.is_group() && !self.collapsed.contains(&row.id()) {
                        self.collapsed.insert(row.id());
                    } else if let Some(parent) = lv_ops::ancestors(project.lv(), index).last() {
                        self.collapsed.insert(*parent);
                        self.selected = Some(*parent);
                        self.scroll_to_selected = true;
                    }
                }
            }
            Command::Expand => {
                if let Some(id) = self.selected {
                    self.collapsed.remove(&id);
                }
            }
            Command::CollapseAll => {
                self.collapsed = project
                    .lv()
                    .rows()
                    .iter()
                    .filter(|row| row.is_group())
                    .map(LvRow::id)
                    .collect();
                // Every LV of the table (the outline of the whole project).
                for index in multi_lv::shown(project)
                    .into_iter()
                    .filter(|index| *index != project.active_lv())
                {
                    if let (Some(lv), Some(key)) = (project.lv_at(index), project.lv_key(index)) {
                        let titles = lv
                            .rows()
                            .iter()
                            .filter(|row| row.is_group())
                            .map(LvRow::id)
                            .collect();
                        self.folding.titles.insert(key, titles);
                    }
                }
                if let Some(index) = selected {
                    if let Some(top) = lv_ops::ancestors(project.lv(), index).first() {
                        self.selected = Some(*top);
                    }
                }
                self.scroll_to_selected = true;
            }
            Command::ExpandAll => {
                self.collapsed.clear();
                self.folding.titles.clear();
                self.folding.lvs.clear();
                self.scroll_to_selected = true;
            }
            Command::NewPosition | Command::NewPositionBefore => {
                let ahead = command == Command::NewPositionBefore;
                let before = project.lv().clone();
                let titles = self.new_position_titles();
                match lv_ops::insert_position_with(project.lv_mut(), selected, ahead, titles) {
                    Err(error) => output.merge(self.insert_failed(
                        project,
                        insert_renumbering::Retry::Command(command),
                        error,
                    )),
                    result => output.merge(self.inserted(project, before, result, Focus::Short)),
                }
            }
            Command::NewTitle => {
                let before = project.lv().clone();
                let (result, hint) = match struct_ops::insert_title(project.lv_mut(), selected) {
                    Ok((index, hint)) => (Ok(index), hint),
                    Err(error) => (Err(error), None),
                };
                output.merge(self.inserted(project, before, result, Focus::Short));
                if hint.is_some() {
                    output.toast = hint;
                }
            }
            Command::NewNote => {
                let before = project.lv().clone();
                let result = lv_ops::insert_note(project.lv_mut(), selected, false);
                output.merge(self.inserted(project, before, result, Focus::Long));
            }
            Command::NewSpacer => {
                let before = project.lv().clone();
                let result = lv_ops::insert_spacer(project.lv_mut(), selected, false);
                output.merge(self.inserted(project, before, result, Focus::Short));
                // Nothing to type into: the table keeps the keyboard.
                self.focus = None;
            }
            Command::NewPointSection => output.merge(self.new_point_section(project)),
            Command::NewSplit => output.merge(self.new_split(project, false)),
            Command::NewPointsTemplate => output.merge(self.new_split(project, true)),
            Command::PointSectionToKurztext => {
                output.merge(self.point_section_to_kurztext(project, selected))
            }
            Command::NoteToSpacer => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                output.merge(self.notes_to_spacers(project, &ids, false));
            }
            Command::AllBlankNotesToSpacers => {
                let ids = project.lv().blank_notes();
                output.merge(self.notes_to_spacers(project, &ids, true));
            }
            Command::FromStlb => output.open_stlb = true,
            Command::FromAusschreiben => output.open_ausschreiben = true,
            Command::Delete => {
                if let Some(index) = selected {
                    let count = lv_ops::block_len(project.lv(), index);
                    let row = &project.lv().rows()[index];
                    let users = project.lv().positions_using(row.id()).len();
                    if row.is_performance() && users > 0 {
                        self.dialog = Some(Dialog::RemovePerformance {
                            id: row.id(),
                            users,
                        });
                    } else if count > 1 {
                        self.dialog = Some(Dialog::ConfirmDelete { index, count });
                    } else {
                        output.merge(self.delete(project, index));
                    }
                }
            }
            Command::MoveUp | Command::MoveDown => {
                if let Some(index) = selected {
                    let before = project.lv().clone();
                    if let Some(moved) =
                        lv_ops::move_step(project.lv_mut(), index, command == Command::MoveUp)
                    {
                        self.undo.record(&before, None);
                        self.selected = Some(project.lv().rows()[moved].id());
                        self.scroll_to_selected = true;
                        output.dirty = true;
                    }
                }
            }
            Command::Deeper | Command::Higher => {
                if let Some(index) = selected {
                    if project.lv().rows()[index].is_group() {
                        let before = project.lv().clone();
                        if lv_ops::shift(project.lv_mut(), index, command == Command::Deeper) {
                            self.undo.record(&before, None);
                            output.dirty = true;
                            if !before.is_auto_numbered() {
                                let row = &project.lv().rows()[index];
                                output.toast = Some(format!(
                                    "Titel {} jetzt auf Stufe {} – die übrigen OZ bleiben (eigene Nummerierung, \
                                     bei Bedarf LV › Neu nummerieren …).",
                                    row.ordinal(),
                                    row.as_group().map_or(0, proava_domain::LvGroup::level) + 1
                                ));
                            }
                        } else {
                            output.toast = Some("Diese Stufe gibt die OZ-Maske nicht her.".into());
                        }
                    }
                }
            }
            Command::Duplicate => {
                if let Some(index) = selected {
                    let before = project.lv().clone();
                    match lv_ops::duplicate(project.lv_mut(), index) {
                        Ok(copy) => {
                            self.undo.record(&before, None);
                            self.selected = Some(project.lv().rows()[copy].id());
                            self.scroll_to_selected = true;
                            output.dirty = true;
                            output.toast = Some("Dupliziert.".into());
                        }
                        Err(error) => output.merge(self.insert_failed(
                            project,
                            insert_renumbering::Retry::Command(command),
                            error,
                        )),
                    }
                }
            }
            Command::Copy | Command::Cut => {
                if let Some(index) = selected {
                    self.clipboard = lv_ops::copy_block(project.lv(), index);
                    self.clipboard_text = lv_ops::rows_as_text(&self.clipboard);
                    output.copied = Some(self.copied(project, &[project.lv().rows()[index].id()]));
                    let count = self.clipboard.len();
                    if command == Command::Cut {
                        output.merge(self.delete(project, index));
                        output.toast = Some(format!(
                            "{} ausgeschnitten.",
                            crate::format::count(count, "Zeile", "Zeilen")
                        ));
                    } else {
                        output.toast = Some(format!(
                            "{} kopiert.",
                            crate::format::count(count, "Zeile", "Zeilen")
                        ));
                    }
                }
            }
            Command::Paste => {
                // The shell decides: rows of another proAVA window or our own.
                output.paste = Some(String::new());
            }
            Command::SelectAll => {
                let ids: Vec<RowId> = project.lv().rows().iter().map(LvRow::id).collect();
                if let Some(first) = ids.first().copied() {
                    if self.selected.is_none() {
                        self.selected = Some(first);
                    }
                    self.anchor = Some(first);
                    output.toast = Some(format!(
                        "{} markiert.",
                        crate::format::count(ids.len(), "Zeile", "Zeilen")
                    ));
                    self.marked = ids.into_iter().collect();
                }
            }
            Command::Undo => {
                if let Some(mut previous) = self.undo.undo(project.lv()) {
                    // The X83 export time is a fact, not an edit.
                    previous.keep_sent_state_of(project.lv());
                    let (message, row) = undo_message("Rückgängig", project.lv(), &previous);
                    *project.lv_mut() = previous;
                    self.reload();
                    self.select_after_undo(project, row);
                    output.dirty = true;
                    output.toast = Some(message);
                } else {
                    output.toast = Some("Nichts mehr rückgängig zu machen.".into());
                }
            }
            Command::Redo => {
                if let Some(mut next) = self.undo.redo(project.lv()) {
                    next.keep_sent_state_of(project.lv());
                    let (message, row) = undo_message("Wiederholt", project.lv(), &next);
                    *project.lv_mut() = next;
                    self.reload();
                    self.select_after_undo(project, row);
                    output.dirty = true;
                    output.toast = Some(message);
                } else {
                    output.toast = Some("Nichts zu wiederholen.".into());
                }
            }
            Command::Find => {
                self.replace = None;
                let query = self
                    .search
                    .take()
                    .map(|search| search.query)
                    .unwrap_or_default();
                self.search = Some(Search { query, focus: true });
            }
            Command::FindNext if self.replace.is_some() => {
                output.merge(self.replace_action(BarAction::Next, project));
            }
            Command::FindNext => match &self.search {
                Some(search) if !search.query.trim().is_empty() => {
                    let query = search.query.clone();
                    output.merge(self.find(project, &query, selected));
                }
                _ => {
                    self.search = Some(Search {
                        query: String::new(),
                        focus: true,
                    })
                }
            },
            Command::EditShort => {
                // An Abstandshalter has nothing to edit.
                if selected.is_some_and(|index| !project.lv().rows()[index].is_spacer()) {
                    let is_note =
                        selected.is_some_and(|index| project.lv().rows()[index].is_note());
                    self.focus = Some(if is_note { Focus::Long } else { Focus::Short });
                    // The Enter that moved the focus is used up: the text
                    // field takes the focus in this frame and would read it
                    // as a new paragraph (Abnahme qa3: Hinweis + Enter put an
                    // empty line in front of the hint text).
                    ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                    });
                }
            }
            Command::EditLong => {
                // Umschalt+F2: Langtext im eigenen, großen Fenster (F2 bearbeitet die Zelle).
                if selected.is_some_and(|index| {
                    let row = &project.lv().rows()[index];
                    !row.is_group() && !row.is_spacer()
                }) {
                    self.open_big_text(project);
                }
            }
            Command::Properties => match selected {
                Some(index) => self.open_position_window(project.lv().rows()[index].id()),
                None => output.toast = Some("Bitte zuerst eine Zeile wählen.".into()),
            },
            Command::Renumber => {
                self.dialog = Some(Dialog::Renumber(context_menu::RenumberForm::new(
                    project.lv(),
                    selected,
                )));
            }
            Command::Palette => output.open_palette = true,
            Command::SetStatus(status) => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                output.merge(self.set_status(project, &ids, status));
            }
            Command::ConfirmPriceCheck => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                output.merge(self.confirm_price_checks(project, &ids));
            }
            Command::ToggleDone => {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                let lv = project.lv();
                let mut positions = ids
                    .iter()
                    .filter_map(|id| lv.index_of(*id))
                    .flat_map(|index| lv.rows()[lv.block_range(index)].iter())
                    .filter_map(LvRow::as_position)
                    .peekable();
                let any = positions.peek().is_some();
                let all_done = any && positions.all(Position::is_locked);
                let status = if all_done {
                    EditStatus::Open
                } else {
                    EditStatus::Done
                };
                output.merge(self.set_status(project, &ids, status));
            }
            Command::SpellCheck => match crate::spell::speller(ctx) {
                Some(speller) => {
                    self.spelling = Some(crate::spell_dialog::SpellDialog::new(project, &speller));
                }
                None => output.toast = Some(crate::spell::status_text()),
            },
            Command::FindReplace => {
                match self.replace.as_mut() {
                    Some(bar) => bar.focus(),
                    None => {
                        let find = self
                            .search
                            .take()
                            .map(|search| search.query)
                            .unwrap_or_default();
                        self.replace = Some(FindReplace::new(find));
                    }
                }
                self.search = None;
            }
            Command::NextMissingQuantity => match project.lv().next_missing_quantity(selected) {
                Some(index) => {
                    let id = project.lv().rows()[index].id();
                    self.select(project.lv(), id);
                    let count = project.lv().missing_quantity_count();
                    output.toast = Some(format!(
                        "Position {} ohne Menge ({count} im LV).",
                        project.lv().rows()[index].ordinal()
                    ));
                }
                None => output.toast = Some("Alle Positionen haben eine Menge.".into()),
            },
            Command::Filter(filter) => {
                self.filter = filter;
                self.scroll_to_selected = true;
                let shown = self.visible(project.lv());
                output.toast = Some(match filter {
                    StatusFilter::All => "Filter aus – alle Zeilen.".to_string(),
                    _ if shown.is_empty() => {
                        format!("{} – keine passenden Zeilen.", filter.label())
                    }
                    _ => filter.label().to_string(),
                });
            }
            Command::CostGroups => self.cost_groups_open = !self.cost_groups_open,
            Command::ToggleCostGroupColumn => {
                output.toast = Some(self.toggle_column(Column::CostGroup));
            }
            Command::ToggleWorkCategoryColumn => {
                output.toast = Some(self.toggle_column(Column::WorkCategory));
            }
            Command::ToggleTitleShare => {
                output.toast = Some(self.toggle_title_share());
            }
            Command::OpenPoints => {
                self.open_points = match self.open_points.take() {
                    Some(_) => None,
                    None => Some(OpenPointsWindow {
                        with_prices: open_points::prices_expected(project.lv()),
                        kinds: HashSet::new(),
                    }),
                };
            }
            Command::ToggleGapMarks => {
                self.toggle_gaps = !self.toggle_gaps;
                self.show_gaps = !self.show_gaps;
                output.toast = Some(if self.show_gaps {
                    "Fehlende Angaben werden markiert (rot: Menge, orange: Einheit/Kurztext)."
                        .into()
                } else {
                    "Markierung fehlender Angaben aus (Zählung und F4 bleiben).".into()
                });
            }
        }
        output
    }

    /// Window „Offene Punkte“: open work as a clickable, filterable list.
    fn open_points_window(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let Some(state) = self.open_points.as_mut() else {
            return;
        };
        let changed = open_points::quantity_changes(project);
        let lv = project.lv();
        let all = open_points::collect_with(lv, state.with_prices, &changed);
        let counts = open_points::counts(&all);
        let shown: Vec<&open_points::OpenPoint> = all
            .iter()
            .filter(|point| state.kinds.is_empty() || state.kinds.contains(&point.kind))
            .collect();
        let selected = self.selected;
        let mut open = true;
        // Esc closes it right after opening, no click first (Anwender 18.11.).
        if open && crate::widgets::esc_closes_window(ctx) {
            open = false;
        }
        let mut jump: Option<RowId> = None;
        let mut copy = false;
        let mut to_spacers = false;
        let blank_notes = all
            .iter()
            .filter(|point| point.kind == open_points::OpenKind::BlankNote)
            .count();
        egui::Window::new("Offene Punkte")
            .open(&mut open)
            .default_size([560.0, 520.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.checkbox(
                    &mut state.with_prices,
                    "auch fehlende Einheitspreise (Kostenvoranschlag)",
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(state.kinds.is_empty(), format!("alle ({})", all.len()))
                        .clicked()
                    {
                        state.kinds.clear();
                    }
                    for (kind, count) in &counts {
                        if *count == 0 && !state.kinds.contains(kind) {
                            continue;
                        }
                        let active = state.kinds.contains(kind);
                        if ui
                            .selectable_label(active, format!("{} ({count})", kind.label()))
                            .on_hover_text("Klick: nur diese Art ein/aus")
                            .clicked()
                        {
                            if active {
                                state.kinds.remove(kind);
                            } else {
                                state.kinds.insert(*kind);
                            }
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(match shown.len() {
                            1 => "1 Eintrag".to_string(),
                            count => format!("{count} Einträge"),
                        })
                            .size(12.5)
                            .color(palette.text_dim),
                    );
                    if ui
                        .button("Liste kopieren")
                        .on_hover_text("Als Text in die Zwischenablage (OZ, Kurztext, Punkt)")
                        .clicked()
                    {
                        copy = true;
                    }
                    if blank_notes > 0
                        && ui
                            .button(format!("Hinweise ohne Text in Abstandshalter umwandeln ({blank_notes})"))
                            .on_hover_text("Leerzeile nur für die Bearbeitung, ohne Warnung – erscheint nicht im Druck (Strg+Z macht es rückgängig)")
                            .clicked()
                    {
                        to_spacers = true;
                    }
                });
                ui.separator();
                if shown.is_empty() {
                    ui.label(
                        RichText::new("Nichts offen.")
                            .size(13.0)
                            .color(palette.done),
                    );
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, 22.0, shown.len(), |ui, range| {
                        for point in &shown[range] {
                            let text = format!(
                                "{}   {}   – {}",
                                if point.kind == open_points::OpenKind::BlankNote {
                                    ""
                                } else if point.ordinal.is_empty() {
                                    "(ohne OZ)"
                                } else {
                                    &point.ordinal
                                },
                                if point.short_text.trim().is_empty() {
                                    "(ohne Kurztext)"
                                } else {
                                    point.short_text.trim()
                                },
                                point.kind.label()
                            );
                            let color = match point.kind {
                                open_points::OpenKind::MissingQuantity => palette.error,
                                open_points::OpenKind::InProgress => palette.in_progress,
                                open_points::OpenKind::PriceCheck
                                | open_points::OpenKind::BlankNote
                                | open_points::OpenKind::QuantityAfterBids => palette.warn,
                                _ => palette.text,
                            };
                            if ui
                                .selectable_label(
                                    selected == Some(point.row),
                                    RichText::new(text).size(13.0).color(color),
                                )
                                .clicked()
                            {
                                jump = Some(point.row);
                            }
                        }
                    });
            });
        if copy {
            let list: Vec<open_points::OpenPoint> =
                shown.iter().map(|point| (*point).clone()).collect();
            ctx.copy_text(open_points::as_text(lv.title(), &list));
            output.toast = Some(format!("{} offene Punkte kopiert.", list.len()));
        }
        if let Some(id) = jump {
            self.select(project.lv(), id);
        }
        if to_spacers {
            let ids = project.lv().blank_notes();
            output.merge(self.notes_to_spacers(project, &ids, true));
        }
        if !open {
            self.open_points = None;
        }
    }

    /// The lock of finished positions for commands that change rows:
    /// deleting, cutting, moving and shifting titles.
    fn check_lock(&self, command: Command, lv: &Lv) -> Result<(), String> {
        let ids: Vec<RowId> = match command {
            Command::Delete | Command::Cut | Command::MoveUp | Command::MoveDown => {
                self.selection_ids().into_iter().collect()
            }
            Command::Deeper | Command::Higher => self
                .selection_ids()
                .into_iter()
                .filter(|id| lv.row_by_id(*id).is_some_and(LvRow::is_group))
                .collect(),
            _ => return Ok(()),
        };
        lv.ensure_editable(&ids)
    }

    /// Sets the status of positions (titles: every position below), one undo step.
    /// „GP-Abweichung bestätigen“: removes the GP-Prüfung of the selected
    /// positions (titles: all below); the computed GP stays.
    fn confirm_price_checks(&mut self, project: &mut Project, ids: &[RowId]) -> EditorOutput {
        let lv = project.lv();
        let mut indices: Vec<usize> = ids
            .iter()
            .filter_map(|id| lv.index_of(*id))
            .flat_map(|index| lv.block_range(index))
            .filter(|&index| {
                lv.rows()[index]
                    .as_position()
                    .is_some_and(|position| position.price_check().is_some())
            })
            .collect();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            return EditorOutput::toast("In der Auswahl ist keine GP-Abweichung zu bestätigen.");
        }
        let before = lv.clone();
        for &index in &indices {
            if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
                position.set_price_check(None);
            }
        }
        self.undo.record(&before, None);
        EditorOutput {
            dirty: true,
            toast: Some(format!(
                "{} bestätigt – der errechnete GP bleibt.",
                crate::format::count(indices.len(), "GP-Abweichung", "GP-Abweichungen")
            )),
            ..EditorOutput::default()
        }
    }

    fn set_status(
        &mut self,
        project: &mut Project,
        ids: &[RowId],
        status: EditStatus,
    ) -> EditorOutput {
        if ids.is_empty() {
            return EditorOutput::toast("Bitte zuerst Positionen oder einen Titel wählen.");
        }
        let before = project.lv().clone();
        let changed = project.lv_mut().set_status(ids, status);
        if changed == 0 {
            let has_positions = !before
                .rows()
                .iter()
                .enumerate()
                .filter(|(_, row)| ids.contains(&row.id()))
                .all(|(index, _)| {
                    !before.rows()[before.block_range(index)]
                        .iter()
                        .any(LvRow::is_position)
                });
            return EditorOutput::toast(if has_positions {
                format!("Schon „{}“.", status.label())
            } else {
                "Nur Positionen haben einen Bearbeitungsstatus.".to_string()
            });
        }
        self.undo.record(&before, None);
        EditorOutput {
            dirty: true,
            toast: Some(match changed {
                1 => format!("1 Position „{}“.", status.label()),
                count => format!("{count} Positionen „{}“.", status.label()),
            }),
            ..EditorOutput::default()
        }
    }

    /// Runs a button of the Suchen-und-Ersetzen bar.
    fn replace_action(&mut self, action: BarAction, project: &mut Project) -> EditorOutput {
        let Some(bar) = &self.replace else {
            return EditorOutput::default();
        };
        if action == BarAction::Close {
            self.replace = None;
            return EditorOutput::default();
        }
        let (find, replacement, options) =
            (bar.find.clone(), bar.replacement.clone(), bar.options());
        if find.is_empty() {
            return EditorOutput::toast("Bitte einen Suchtext eingeben.");
        }
        let selected = self.selected_index(project.lv());
        match action {
            BarAction::Close => EditorOutput::default(),
            BarAction::Next => self.replace_next(project, &find, &options, selected),
            BarAction::Replace => {
                let has_hit = selected.is_some_and(|index| {
                    search_replace::hits_in_row(&project.lv().rows()[index], &find, &options) > 0
                });
                let Some(index) = selected.filter(|_| has_hit) else {
                    return self.replace_next(project, &find, &options, selected);
                };
                let before = project.lv().clone();
                match search_replace::replace_in_row(
                    project.lv_mut(),
                    index,
                    &find,
                    &replacement,
                    &options,
                ) {
                    Ok(count) => {
                        let mut output = EditorOutput::default();
                        if count > 0 {
                            self.undo.record(&before, None);
                            self.reload();
                            output.dirty = true;
                        }
                        output.merge(self.replace_next(project, &find, &options, Some(index)));
                        if output.toast.is_none() || count > 0 {
                            output.toast = Some(format!("{count} Treffer ersetzt."));
                        }
                        output
                    }
                    Err(message) => {
                        let mut output = self.replace_next(project, &find, &options, Some(index));
                        output.toast = Some(message);
                        output
                    }
                }
            }
            // Several LVs and no selection as the reach: the shell replaces in
            // all of them, one undo step.
            BarAction::ReplaceAll
                if multi_lv::shown(project).len() > 1 && options.only.is_none() =>
            {
                EditorOutput {
                    replace_all: Some((find, replacement, options)),
                    ..EditorOutput::default()
                }
            }
            BarAction::ReplaceAll => {
                let before = project.lv().clone();
                let report =
                    search_replace::replace_all(project.lv_mut(), &find, &replacement, &options);
                let mut output = EditorOutput::toast(report.message(&find));
                if report.replaced > 0 {
                    self.undo.record(&before, None);
                    self.reload();
                    output.dirty = true;
                }
                output
            }
        }
    }

    /// The next hit of Suchen und Ersetzen: on in the active LV, then in the
    /// LVs below and above it (unless the reach is the selection); a hit in
    /// another LV makes it the active one.
    fn replace_next(
        &mut self,
        project: &Project,
        find: &str,
        options: &search_replace::SearchOptions,
        after: Option<usize>,
    ) -> EditorOutput {
        let lv = project.lv();
        if options.only.is_some() || multi_lv::shown(project).len() <= 1 {
            return match search_replace::find_next(lv, find, options, after) {
                Some(index) => {
                    let id = lv.rows()[index].id();
                    self.select(lv, id);
                    EditorOutput::default()
                }
                None => EditorOutput::toast(format!("„{find}“ nicht gefunden.")),
            };
        }
        let active = project.active_lv();
        let start = after.map(|index| index + 1).unwrap_or(0);
        for (index, range) in multi_lv::search_pieces(project, start) {
            let Some(data) = project.lv_at(index) else {
                continue;
            };
            if let Some(row) = search_replace::find_in(data, find, options, range) {
                let id = data.rows()[row].id();
                if index == active {
                    self.select(data, id);
                    return EditorOutput::default();
                }
                return EditorOutput {
                    jump: Some(LvJump {
                        lv: index,
                        row: Some(id),
                        then: JumpThen::Select,
                    }),
                    toast: Some(format!(
                        "Gefunden in {}.",
                        crate::lv_properties::lv_label(data)
                    )),
                    ..EditorOutput::default()
                };
            }
        }
        EditorOutput::toast(format!("„{find}“ nicht gefunden."))
    }

    /// Delete, copy, cut and move for a multiple selection.
    fn run_multi(&mut self, command: Command, project: &mut Project) -> EditorOutput {
        let roots = lv_ops::selection_roots(project.lv(), &self.selection_ids());
        let count = lv_ops::roots_len(project.lv(), &roots);
        match command {
            Command::Delete => {
                self.dialog = Some(Dialog::ConfirmDeleteMany { count });
                EditorOutput::default()
            }
            Command::Copy | Command::Cut => {
                self.clipboard = lv_ops::copy_roots(project.lv(), &roots);
                self.clipboard_text = lv_ops::rows_as_text(&self.clipboard);
                let ids: Vec<RowId> = roots
                    .iter()
                    .map(|index| project.lv().rows()[*index].id())
                    .collect();
                let copied = self.copied(project, &ids);
                let mut output = if command == Command::Cut {
                    let mut output = self.delete_selection(project);
                    output.toast = Some(format!("{count} Zeilen ausgeschnitten."));
                    output
                } else {
                    EditorOutput::toast(format!("{count} Zeilen kopiert."))
                };
                output.copied = Some(copied);
                output
            }
            Command::MoveUp | Command::MoveDown => {
                let Some(range) = lv_ops::contiguous_range(project.lv(), &roots) else {
                    return EditorOutput::toast(
                        "Verschieben geht nur, wenn die gewählten Zeilen zusammenhängen.",
                    );
                };
                let before = project.lv().clone();
                match lv_ops::move_range(project.lv_mut(), range, command == Command::MoveUp) {
                    Some(_) => {
                        self.undo.record(&before, None);
                        self.scroll_to_selected = true;
                        EditorOutput {
                            dirty: true,
                            ..EditorOutput::default()
                        }
                    }
                    None => EditorOutput::default(),
                }
            }
            _ => EditorOutput::default(),
        }
    }

    fn delete_selection(&mut self, project: &mut Project) -> EditorOutput {
        let roots = lv_ops::selection_roots(project.lv(), &self.selection_ids());
        let count = lv_ops::roots_len(project.lv(), &roots);
        self.undo.record(project.lv(), None);
        let next = lv_ops::delete_roots(project.lv_mut(), &roots);
        self.selected = next.map(|row| project.lv().rows()[row].id());
        self.marked.clear();
        self.anchor = self.selected;
        self.scroll_to_selected = true;
        EditorOutput {
            dirty: true,
            toast: Some(format!("{count} Zeilen gelöscht – Strg+Z holt sie zurück.")),
            ..EditorOutput::default()
        }
    }

    /// The rows `ids` (a title with its block) for the system clipboard:
    /// with their Ausführungsbeschreibungen and images, status „offen“.
    fn copied(&self, project: &Project, ids: &[RowId]) -> Copied {
        let options = proava_domain::transfer::TransferOptions {
            quantities: true,
            prices: true,
        };
        let transfer = proava_domain::transfer::collect(project, project.lv(), ids, options);
        Copied {
            mask: project.lv().oz_mask().clone(),
            plain: self.clipboard_text.clone(),
            html: lv_ops::rows_as_html(&self.clipboard),
            transfer,
        }
    }

    /// Rows of this table are in the clipboard (`text` from Strg+V; empty
    /// from a menu: whatever was copied last).
    pub fn has_internal(&self, text: &str) -> bool {
        !self.clipboard.is_empty()
            && (text.is_empty() || text.replace("\r\n", "\n") == self.clipboard_text)
    }

    /// What lies in this table's own clipboard („3 Positionen“), if anything.
    pub fn internal_summary(&self) -> Option<String> {
        (!self.clipboard.is_empty())
            .then(|| proava_application::clipboard::describe_rows(&self.clipboard))
    }

    /// Pastes the rows copied in this table (references to Ausführungs-
    /// beschreibungen of this LV stay).
    pub fn paste_internal(&mut self, project: &mut Project) -> EditorOutput {
        let mut output = EditorOutput::default();
        if self.clipboard.is_empty() {
            output.toast = Some("Nichts zum Einfügen – erst Zeilen kopieren.".into());
            return output;
        }
        let selected = self.selected_index(project.lv());
        let rows = self.clipboard.clone();
        let before = project.lv().clone();
        match lv_ops::paste(project.lv_mut(), selected, &rows) {
            Ok(first) => {
                self.undo.record(&before, None);
                self.selected = Some(project.lv().rows()[first].id());
                self.single(project.lv().rows()[first].id());
                self.scroll_to_selected = true;
                output.dirty = true;
                output.toast = Some(format!(
                    "{} eingefügt.",
                    crate::format::count(rows.len(), "Zeile", "Zeilen")
                ));
            }
            Err(error) => {
                return self.insert_failed(project, insert_renumbering::Retry::PasteInternal, error)
            }
        }
        output
    }

    /// Which titles come along with a new position: in a library the
    /// position goes into the title of the Schreibmarke, without new titles
    /// below it.
    fn new_position_titles(&self) -> lv_ops::NewTitles {
        if self.library_mode {
            lv_ops::NewTitles::Never
        } else {
            lv_ops::NewTitles::Fill
        }
    }

    /// Pastes rows from another proAVA window or project: new ids and
    /// order numbers of this LV, images into the project, one undo step.
    pub fn paste_transfer(
        &mut self,
        project: &mut Project,
        transfer: &proava_domain::transfer::Transfer,
    ) -> EditorOutput {
        let taken = transfer;
        // No title level: only the content of the titles.
        let transfer = insert_renumbering::fitting(project, taken);
        let selected = self.selected_index(project.lv());
        let before = project.lv().clone();
        match proava_domain::transfer::insert(project, selected, &transfer) {
            Ok(range) => {
                self.undo.record(&before, None);
                let first = project.lv().rows()[range.start].id();
                self.selected = Some(first);
                self.single(first);
                self.scroll_to_selected = true;
                let summary = proava_application::clipboard::describe_rows(&transfer.rows);
                EditorOutput {
                    dirty: true,
                    toast: Some(format!("{summary} eingefügt.")),
                    ..EditorOutput::default()
                }
            }
            Err(error) => self.insert_failed(
                project,
                insert_renumbering::Retry::PasteTransfer(Box::new(taken.clone())),
                error,
            ),
        }
    }

    /// Umschalt+Klick into another LV: a selection stays in one LV, so it
    /// reaches to the last (`down`) or first shown row of the active LV.
    fn extend_to_edge(&mut self, lv: &Lv, down: bool) {
        let visible = self.visible(lv);
        let edge = if down {
            visible.last()
        } else {
            visible.first()
        };
        if let Some(&edge) = edge {
            self.click_select(lv, edge, egui::Modifiers::SHIFT);
            self.scroll_to_selected = true;
        }
    }

    /// Click in the table: plain = single row, Umschalt = range, Strg = toggle.
    fn click_select(&mut self, lv: &Lv, index: usize, modifiers: egui::Modifiers) {
        let id = lv.rows()[index].id();
        if modifiers.shift {
            let visible = self.visible(lv);
            let anchor = self
                .anchor
                .and_then(|anchor| lv.index_of(anchor))
                .or(self.selected_index(lv))
                .unwrap_or(index);
            self.marked = lv_ops::span_ids(lv, &visible, anchor, index)
                .into_iter()
                .collect();
            self.selected = Some(id);
        } else if modifiers.command {
            let mut all = self.selection_ids();
            if all.contains(&id) && all.len() > 1 {
                all.remove(&id);
                self.selected = all.iter().next().copied();
                self.marked = all;
            } else {
                self.marked = all;
                self.selected = Some(id);
                self.anchor = Some(id);
            }
        } else {
            self.selected = Some(id);
            self.single(id);
        }
    }

    /// Result of an insert command (the LV is already changed on success).
    fn inserted(
        &mut self,
        project: &Project,
        before: Lv,
        result: Result<usize, String>,
        focus: Focus,
    ) -> EditorOutput {
        match result {
            Ok(index) => {
                self.undo.record(&before, None);
                self.after_insert(project, index, focus);
                EditorOutput {
                    dirty: true,
                    ..EditorOutput::default()
                }
            }
            Err(error) => EditorOutput::toast(error),
        }
    }

    /// After undo/redo the changed row is selected (or the old selection
    /// kept if it still exists, else the row at its place).
    fn select_after_undo(&mut self, project: &Project, row: Option<RowId>) {
        let lv = project.lv();
        let target = row
            .filter(|id| lv.index_of(*id).is_some())
            .or(self.selected.filter(|id| lv.index_of(*id).is_some()))
            .or_else(|| lv.rows().first().map(LvRow::id));
        if let Some(id) = target {
            for ancestor in lv
                .index_of(id)
                .map(|index| lv_ops::ancestors(lv, index))
                .unwrap_or_default()
            {
                self.collapsed.remove(&ancestor);
            }
            self.selected = Some(id);
            self.single(id);
            self.scroll_to_selected = true;
        }
    }

    fn after_insert(&mut self, project: &Project, index: usize, focus: Focus) {
        let lv = project.lv();
        for ancestor in lv_ops::ancestors(lv, index) {
            self.collapsed.remove(&ancestor);
        }
        self.selected = Some(lv.rows()[index].id());
        self.single(lv.rows()[index].id());
        self.scroll_to_selected = true;
        self.focus = Some(focus);
    }

    /// Hints without text among `ids` become Abstandshalter, one undo step.
    /// `all`: the whole LV was asked for (other message when there is none).
    pub fn notes_to_spacers(
        &mut self,
        project: &mut Project,
        ids: &[RowId],
        all: bool,
    ) -> EditorOutput {
        let before = project.lv().clone();
        let count = project.lv_mut().blank_notes_to_spacers(ids);
        if count == 0 {
            return EditorOutput::toast(if all {
                "Das LV hat keine Hinweise ohne Text."
            } else {
                "Kein Hinweis ohne Text gewählt – Hinweise mit Text bleiben, wie sie sind."
            });
        }
        self.undo.record(&before, None);
        EditorOutput {
            dirty: true,
            toast: Some(format!(
                "{} in Abstandshalter umgewandelt – Strg+Z macht es rückgängig.",
                crate::format::count(count, "Hinweis ohne Text", "Hinweise ohne Text")
            )),
            ..EditorOutput::default()
        }
    }

    fn delete(&mut self, project: &mut Project, index: usize) -> EditorOutput {
        let count = lv_ops::block_len(project.lv(), index);
        self.undo.record(project.lv(), None);
        let next = lv_ops::delete(project.lv_mut(), index);
        self.selected = next.map(|row| project.lv().rows()[row].id());
        self.scroll_to_selected = true;
        EditorOutput {
            dirty: true,
            toast: Some(if count > 1 {
                format!("{count} Zeilen gelöscht – Strg+Z holt sie zurück.")
            } else {
                "Gelöscht – Strg+Z holt die Zeile zurück.".to_string()
            }),
            ..EditorOutput::default()
        }
    }

    /// Strg+F / F3 over all LVs of the table: the rest of the active LV, the
    /// LVs below it, those above, then the active LV from its start. A hit in
    /// another LV makes that LV the active one (and says which).
    fn find(&mut self, project: &Project, query: &str, after: Option<usize>) -> EditorOutput {
        let active = project.active_lv();
        let start = after.map(|index| index + 1).unwrap_or(0);
        for (lv, range) in multi_lv::search_pieces(project, start) {
            let Some(data) = project.lv_at(lv) else {
                continue;
            };
            let Some(index) = lv_ops::find_in(data, query, range) else {
                continue;
            };
            let id = data.rows()[index].id();
            if lv == active {
                self.select(data, id);
                return EditorOutput::default();
            }
            return EditorOutput {
                jump: Some(LvJump {
                    lv,
                    row: Some(id),
                    then: JumpThen::Select,
                }),
                toast: Some(format!(
                    "Gefunden in {}.",
                    crate::lv_properties::lv_label(data)
                )),
                ..EditorOutput::default()
            };
        }
        EditorOutput::toast(format!("„{}“ nicht gefunden.", query.trim()))
    }

    // ----- Bearbeitungsfenster -----

    fn detail_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        output: &mut EditorOutput,
    ) {
        let side = launcher.detail_side();
        self.long_text.field_presets = crate::rich_editor::FieldPresets::of(launcher);
        let available = ui.available_width();
        let table = (available * TABLE_SHARE).clamp(MIN_TABLE_WIDTH, TABLE_WIDTH_KEPT);
        let max_width = (available - table)
            .min(available * 0.7)
            .max(MIN_DETAIL_WIDTH);
        let wanted = self.detail_width.min(max_width);
        // egui keeps a narrowed panel narrow: back to the chosen width once
        // the window has room again (not while the edge is dragged).
        let range = if !self.width_dragging && self.detail_shown + 0.5 < wanted {
            wanted..=wanted
        } else {
            MIN_DETAIL_WIDTH..=max_width
        };
        let panel = match side {
            PanelSide::Left => egui::Panel::left("lv_detail"),
            PanelSide::Right => egui::Panel::right("lv_detail"),
        };
        let inner = panel
            .resizable(true)
            .default_size(wanted)
            .size_range(range)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("detail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.detail_content(ui, palette, project, output));
            });

        let rect = inner.response.rect;
        self.detail_shown = rect.width();
        let edge = match side {
            PanelSide::Left => rect.right(),
            PanelSide::Right => rect.left(),
        };
        let (pressed, down, origin) = ui.ctx().input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.press_origin(),
            )
        });
        if pressed && origin.is_some_and(|pos| (pos.x - edge).abs() < 8.0) {
            self.width_dragging = true;
        }
        if self.width_dragging && !down {
            self.width_dragging = false;
            if (rect.width() - self.detail_width).abs() > 0.5 {
                self.detail_width = rect.width();
                launcher.set_detail_width(self.detail_width);
            }
        }
    }

    fn detail_content(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        if self.is_multi() {
            self.multi_detail(ui, palette, project, output);
            return;
        }
        let Some(index) = self.selected_index(project.lv()) else {
            ui.add_space(24.0);
            ui.label(
                RichText::new(strings::WS_NO_SELECTION)
                    .size(13.0)
                    .color(palette.text_dim),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("F9 legt eine Position an, F12 einen Titel.")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            return;
        };
        let row = project.lv().rows()[index].clone();
        let id = row.id();
        if self.position_window_row() == Some(id) {
            // Die Zeile steht im Fenster „Eigenschaften“ (nicht doppelt bearbeiten).
            self.popped_out(ui, palette, project, index);
            return;
        }
        self.panel_row = Some(id);
        match &row {
            LvRow::Group(group) => {
                detail_heading(
                    ui,
                    palette,
                    &project.lv().level_label(group.level()),
                    group.ordinal(),
                    &[],
                );
                self.ordinal_field(ui, palette, project, index, output);
                let mut text = group.short_text().to_string();
                let response = self.short_text_field(ui, palette, &mut text, id);
                if response.changed() {
                    self.undo
                        .record(project.lv(), Some(EditKey(format!("short:{}", id.0))));
                    if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
                        group.set_short_text(text);
                    }
                    output.dirty = true;
                }
                ui.add_space(10.0);
                let levels = project.lv().level_count();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Stufe {} von {}", group.level() + 1, levels.max(1)))
                            .size(13.0)
                            .color(palette.text),
                    );
                    ui.add_space(8.0);
                    if ui
                        .add_enabled(group.level() > 0, egui::Button::new("Höher"))
                        .on_hover_text("Umschalt+Tab")
                        .clicked()
                    {
                        output.merge(self.run(Command::Higher, project, &ui.ctx().clone()));
                    }
                    if ui
                        .add_enabled(group.level() + 1 < levels, egui::Button::new("Tiefer"))
                        .on_hover_text("Tab")
                        .clicked()
                    {
                        output.merge(self.run(Command::Deeper, project, &ui.ctx().clone()));
                    }
                });
                ui.add_space(10.0);
                self.cost_group_field(ui, palette, project, &[id], output);
                self.point_section_summary(ui, palette, project, index);
                self.title_lump_field(ui, palette, project, index, output);
                let total = project
                    .lv()
                    .group_totals()
                    .into_iter()
                    .find(|total| total.group_row == index)
                    .map(|total| total.total)
                    .unwrap_or(Money::ZERO);
                let range = project.lv().block_range(index);
                let positions = project.lv().rows()[range]
                    .iter()
                    .filter(|row| row.is_position())
                    .count();
                let currency = project.lv().currency().to_string();
                let lump = project.lv().rows()[index]
                    .as_group()
                    .and_then(proava_domain::LvGroup::lump_sum_total);
                ui.label(
                    RichText::new(match lump {
                        // the sum of the positions next to the Pauschalsumme
                        Some(_) => format!(
                            "Summe pauschal {} · {} zusammen {}",
                            proava_domain::price_check::amount(total, &currency),
                            crate::format::count(positions, "Position", "Positionen"),
                            proava_domain::price_check::amount(
                                project.lv().block_positions_total(index),
                                &currency
                            )
                        ),
                        None => format!(
                            "Summe {} · {}",
                            proava_domain::price_check::amount(total, &currency),
                            crate::format::count(positions, "Position", "Positionen")
                        ),
                    })
                    .size(13.0)
                    .color(palette.text_dim),
                );
                if positions > 0 {
                    ui.add_space(12.0);
                    let counts = project
                        .lv()
                        .status_counts_in(project.lv().block_range(index));
                    ui.label(
                        RichText::new(format!(
                            "Bearbeitungsstand: {} von {} fertig, {} in Bearbeitung",
                            counts.done,
                            counts.total(),
                            counts.in_progress
                        ))
                        .size(12.5)
                        .color(palette.text_dim),
                    );
                    ui.add_space(4.0);
                    let mut chosen = None;
                    ui.horizontal_wrapped(|ui| {
                        for status in [EditStatus::Done, EditStatus::InProgress, EditStatus::Open] {
                            if ui
                                .button(format!("Alle {}", status.label()))
                                .on_hover_text(
                                    "Setzt den Status aller Positionen unter diesem Titel",
                                )
                                .clicked()
                            {
                                chosen = Some(status);
                            }
                        }
                    });
                    if let Some(status) = chosen {
                        output.merge(self.set_status(project, &[id], status));
                    }
                }
            }
            LvRow::Spacer(_) => {
                detail_heading(ui, palette, "Abstandshalter", "", &[]);
                ui.label(
                    RichText::new(
                        "Leerzeile nur für die Bearbeitung: keine OZ, kein Text, kein Preis. Erscheint weder im \
                         Druck noch in GAEB oder CSV. Verschieben mit Alt+↑/↓, löschen mit Entf.",
                    )
                    .size(12.0)
                    .color(palette.text_dim),
                );
            }
            LvRow::Note(note) => {
                if let Some(kind) = detail_heading_with_kind(
                    ui,
                    palette,
                    "Hinweis",
                    "",
                    &[],
                    (proava_domain::RowKind::Note, note.id(), true),
                ) {
                    // The rest of the panel belongs to the old kind: next frame.
                    output.merge(self.request_conversion(project, index, kind));
                    return;
                }
                ui.label(
                    RichText::new(
                        "Text ohne OZ und Preis; steht vor dem Teil, auf den er sich bezieht.",
                    )
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(6.0);
                self.note_extra(ui, palette, project, index, note, output);
                self.long_text_section(ui, palette, project, index, note.text().clone(), output);
            }
            LvRow::Performance(performance) => {
                self.performance_detail(ui, palette, project, index, performance, output);
            }
            LvRow::Position(position) => {
                self.position_detail(ui, palette, project, index, position, output);
            }
        }
    }

    /// Edit panel for several rows: shared fields of the selected positions.
    fn multi_detail(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let ids = self.selection_ids();
        let lv = project.lv();
        let positions: Vec<usize> = (0..lv.rows().len())
            .filter(|&index| lv.rows()[index].is_position() && ids.contains(&lv.rows()[index].id()))
            .collect();
        let others = ids.len() - positions.len();
        ui.label(
            RichText::new(format!(
                "{} gewählt",
                crate::format::count(positions.len(), "Position", "Positionen")
            ))
            .size(15.0)
            .strong()
            .color(palette.text),
        );
        if others > 0 {
            ui.label(
                RichText::new(format!(
                    "dazu {others} Titel/Hinweise – die Felder gelten nur für Positionen"
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
        }
        ui.add_space(10.0);
        if !positions.is_empty() {
            let mut chosen = None;
            ui.label(
                RichText::new("Status (alle)")
                    .size(12.0)
                    .color(palette.text_dim),
            );
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                for status in [EditStatus::Done, EditStatus::InProgress, EditStatus::Open] {
                    if ui.button(status.label()).clicked() {
                        chosen = Some(status);
                    }
                }
            });
            if let Some(status) = chosen {
                let ids: Vec<RowId> = self.selection_ids().into_iter().collect();
                output.merge(self.set_status(project, &ids, status));
                return;
            }
            ui.add_space(10.0);
        }
        let lv = project.lv();
        if positions.is_empty() {
            ui.label(
                RichText::new(
                    "Löschen, Kopieren und Verschieben gelten für alle gewählten Zeilen.",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
            return;
        }
        let selected_positions: Vec<&Position> = positions
            .iter()
            .filter_map(|&index| lv.rows()[index].as_position())
            .collect();
        let common_unit = common(&selected_positions, |position| position.unit().to_string());
        let common_kind = common(&selected_positions, |position| kind_label(position.kind()));
        let group = kind_group(selected_positions[0].kind());
        let all_lump = selected_positions
            .iter()
            .all(|position| position.is_lump_sum());
        let all_hourly = selected_positions
            .iter()
            .all(|position| position.is_hourly());

        ui.label(RichText::new("Einheit").size(12.0).color(palette.text_dim));
        ui.add_space(3.0);
        // Eigener Puffer: der Text muss das Tippen über mehrere Bilder überleben.
        let mut unit = self
            .multi_unit
            .clone()
            .unwrap_or_else(|| common_unit.clone().unwrap_or_default());
        let response = ui.add(
            egui::TextEdit::singleline(&mut unit)
                .id(egui::Id::unique("multi_unit"))
                .hint_text(if common_unit.is_none() {
                    "verschieden"
                } else {
                    ""
                })
                .desired_width(160.0)
                .margin(egui::Margin::symmetric(6, 5)),
        );
        let mut new_unit: Option<String> = None;
        if response.lost_focus() {
            self.multi_unit = None;
            if !unit.trim().is_empty() && Some(unit.trim().to_string()) != common_unit {
                new_unit = Some(unit.trim().to_string());
            }
        } else if response.has_focus() {
            self.multi_unit = Some(unit);
        } else {
            self.multi_unit = None;
        }
        ui.horizontal_wrapped(|ui| {
            for suggestion in UNITS {
                if ui
                    .add(
                        egui::Button::new(RichText::new(suggestion).size(12.5)).sense(Sense::CLICK),
                    )
                    .clicked()
                {
                    new_unit = Some(suggestion.to_string());
                }
            }
        });
        ui.add_space(10.0);

        ui.label(
            RichText::new("Positionsart")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        let mut new_kind: Option<PositionKind> = None;
        egui::ComboBox::from_id_salt("multi_kind")
            .width(ui.available_width().min(320.0))
            .selected_text(
                common_kind
                    .clone()
                    .unwrap_or_else(|| "verschieden".to_string()),
            )
            .show_ui(ui, |ui| {
                for option in [
                    PositionKind::Normal,
                    PositionKind::Provisional { with_total: false },
                    PositionKind::Provisional { with_total: true },
                    PositionKind::Base { group },
                    PositionKind::Alternative { group, serial: 1 },
                    PositionKind::Supplement,
                ] {
                    let label = kind_label(option);
                    let active = common_kind.as_deref() == Some(label.as_str());
                    if ui.selectable_label(active, label).clicked() {
                        new_kind = Some(option);
                    }
                }
            });
        ui.add_space(6.0);
        let mut lump = all_lump;
        let mut hourly = all_hourly;
        let mut set_lump = None;
        let mut set_hourly = None;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut lump, "Pauschal (alle)").changed() {
                set_lump = Some(lump);
            }
            if ui.checkbox(&mut hourly, "Stundenlohn (alle)").changed() {
                set_hourly = Some(hourly);
            }
        });

        ui.add_space(8.0);
        let selected: Vec<RowId> = self.selection_ids().into_iter().collect();
        self.cost_group_field(ui, palette, project, &selected, output);

        if new_unit.is_some() || new_kind.is_some() || set_lump.is_some() || set_hourly.is_some() {
            let editable: Vec<usize> = positions
                .iter()
                .copied()
                .filter(|&index| project.lv().ensure_editable_rows(&[index]).is_ok())
                .collect();
            let skipped = positions.len() - editable.len();
            if editable.is_empty() {
                output.toast = Some(format!(
                    "Nichts geändert: alle {skipped} Positionen sind als fertig markiert – {}.",
                    proava_domain::LOCKED_HINT
                ));
                return;
            }
            self.undo.record(project.lv(), None);
            for &index in &editable {
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    // STLB-Bau positions keep their unit (write-protected).
                    if let Some(unit) = new_unit.as_ref().filter(|_| !position.is_text_locked()) {
                        position.set_unit(unit.as_str());
                    }
                    if let Some(kind) = new_kind {
                        position.set_kind(kind);
                    }
                    if let Some(value) = set_lump {
                        position.set_lump_sum(value);
                    }
                    if let Some(value) = set_hourly {
                        position.set_hourly(value);
                    }
                }
            }
            output.dirty = true;
            let stlb_units = new_unit.as_ref().map_or(0, |_| {
                editable
                    .iter()
                    .filter(|&&index| stlb_lock::text_locked(project, index))
                    .count()
            });
            if stlb_units > 0 {
                output.toast = Some(format!(
                    "Einheit nicht geändert bei {} aus STLB-Bau (schreibgeschützt).",
                    crate::format::count(stlb_units, "Position", "Positionen")
                ));
                return;
            }
            output.toast = Some(if skipped > 0 {
                format!(
                    "{} geändert, {skipped} fertige übersprungen.",
                    crate::format::count(editable.len(), "Position", "Positionen")
                )
            } else {
                format!(
                    "{} geändert.",
                    crate::format::count(editable.len(), "Position", "Positionen")
                )
            });
        }
        ui.add_space(10.0);
        self.performance_choice(ui, palette, project, &ids, output);
        ui.add_space(14.0);
        ui.label(
            RichText::new(
                "Umschalt+Klick oder Umschalt+↑/↓ wählt einen Bereich, Strg+Klick einzelne Zeilen. \
                 Entf, Strg+C/X und Alt+↑/↓ wirken auf alle.",
            )
            .size(12.0)
            .color(palette.text_dim),
        );
    }

    fn short_text_field(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        text: &mut String,
        id: RowId,
    ) -> egui::Response {
        let caption = ui.label(RichText::new("Kurztext").size(12.0).color(palette.text_dim));
        ui.add_space(3.0);
        let field_id = egui::Id::unique(("short", id));
        if self.focus == Some(Focus::Short) {
            // Before the field: it gets the keys of this frame already (the
            // first letter typed right after „Neue Position“).
            ui.memory_mut(|memory| memory.request_focus(field_id));
            // A new row may come with a text (STLB-Platzhalter): selected,
            // so that typing replaces it.
            let mut state =
                egui::text_edit::TextEditState::load(ui.ctx(), field_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                )));
            state.store(ui.ctx(), field_id);
        }
        let output = egui::TextEdit::singleline(text)
            .id(field_id)
            .desired_width(f32::INFINITY)
            .font(FontId::proportional(14.0))
            .margin(egui::Margin::symmetric(6, 5))
            .show(ui);
        let mut response = output.response.response.clone().labelled_by(caption.id);
        if crate::spell::text_edit(ui, &output, text) {
            response.mark_changed();
        }
        if self.focus == Some(Focus::Short) {
            response.request_focus();
            self.focus = None;
        }
        response
    }

    fn position_detail(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        position: &Position,
        output: &mut EditorOutput,
    ) {
        let id = position.id();
        let ctx = ui.ctx().clone();
        if let Some(kind) = detail_heading_with_kind(
            ui,
            palette,
            "Position",
            position.ordinal(),
            &position.marks(),
            (proava_domain::RowKind::Position, id, !position.is_locked()),
        ) {
            // The rest of the panel belongs to the old kind: next frame.
            output.merge(self.request_conversion(project, index, kind));
            return;
        }
        if let Some(status) = status_selector(ui, palette, position.status(), id) {
            output.merge(self.set_status(project, &[id], status));
        }
        ui.add_space(8.0);
        if project
            .lv()
            .position_by_id(id)
            .is_some_and(Position::is_locked)
        {
            egui::Frame::new()
                .fill(palette.done.gamma_multiply(0.12))
                .stroke(Stroke::new(1.0, palette.done))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Position ist als fertig markiert – {}.",
                            proava_domain::LOCKED_HINT
                        ))
                        .size(12.5)
                        .color(palette.text),
                    );
                });
            ui.add_space(8.0);
            // Alles Weitere nur lesen; Änderungen kommen nicht an.
            ui.disable();
        }
        self.ordinal_field(ui, palette, project, index, output);
        // The link to an Ausführungsbeschreibung, seen at once.
        self.performance_link_line(ui, palette, project, id, output);
        if position.is_stlb_placeholder() {
            ui.label(
                RichText::new(strings::WS_STLB_DETAIL)
                    .size(12.5)
                    .color(palette.accent),
            );
            ui.add_space(4.0);
            if ui
                .button(strings::STLB_TAKE_BUTTON)
                .on_hover_text(strings::STLB_TAKE_HINT)
                .clicked()
            {
                output.open_stlb = true;
            }
            ui.add_space(6.0);
        }
        if position.is_text_locked() {
            // Kennung mit Version, Schreibschutz und der Weg heraus.
            if stlb_lock::banner(ui, palette, position) {
                output.merge(self.request_free(project, &[id]));
            }
        } else if let Some(stlb) = position.stlb_reference() {
            ui.label(
                RichText::new(stlb.summary())
                    .size(12.0)
                    .color(palette.text_dim),
            );
            if position.stlb_text_changed() {
                ui.label(
                    RichText::new(strings::STLB_TEXT_CHANGED)
                        .size(12.0)
                        .color(palette.warn),
                );
            }
            ui.add_space(6.0);
        }

        let mut text = position.short_text().to_string();
        let response = self.short_text_field(ui, palette, &mut text, id);
        let length = text.chars().count();
        ui.label(
            RichText::new(format!("{length}/{SHORT_TEXT_LIMIT}"))
                .size(11.5)
                .color(if length > SHORT_TEXT_LIMIT {
                    palette.error
                } else {
                    palette.text_dim
                }),
        );
        if response.changed() {
            self.undo
                .record(project.lv(), Some(EditKey(format!("short:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_short_text(text);
            }
            output.dirty = true;
        }
        ui.add_space(8.0);

        // Menge | Einheit | EP – one Tab after the other.
        if self.numbers.loaded != Some((id, self.revision)) {
            self.numbers.loaded = Some((id, self.revision));
            self.numbers.quantity = numbers::quantity_entry_shown(position);
            self.numbers.price = numbers::price_text(position.unit_price());
            self.numbers.quantity_error = None;
            self.numbers.price_error = None;
        }
        let mut unit = position.unit().to_string();
        let mut unit_focused = false;
        let mut quantity_edit = NumberEdit::default();
        let mut price_edit = NumberEdit::default();
        let mut unit_value: Option<String> = None;
        // The unit belongs to the STLB-Bau text (write-protected).
        let unit_locked = position.is_text_locked();
        let mut unit_toast = None;
        ui.columns(3, |columns| {
            quantity_edit = number_field(
                &mut columns[0],
                palette,
                "Menge",
                &mut self.numbers.quantity,
                self.numbers.quantity_error.is_some(),
                ("qty", id),
            );
            let unit_too_long = unit.trim().chars().count() > lv_check::UNIT_LIMIT;
            columns[1].label(RichText::new("Einheit").size(12.0).color(if unit_too_long {
                palette.warn
            } else {
                palette.text_dim
            }));
            columns[1].add_space(3.0);
            let (response, toast) = stlb_lock::locked_or_free(
                &mut columns[1],
                egui::Id::unique(("unit_lock", id)),
                unit_locked,
                |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut unit)
                            .id(egui::Id::unique(("unit", id)))
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(6, 5)),
                    )
                },
            );
            unit_toast = toast;
            select_all_on_focus(&response, &unit);
            if !unit_locked {
                unit_value = crate::widgets::live_text(&response, &unit, position.unit());
            }
            unit_focused = response.has_focus() && !unit_locked;
            price_edit = number_field(
                &mut columns[2],
                palette,
                &format!(
                    "EP ({})",
                    proava_domain::price_check::currency_symbol(project.lv().currency())
                ),
                &mut self.numbers.price,
                self.numbers.price_error.is_some(),
                ("price", id),
            );
        });
        if let Some(toast) = unit_toast {
            output.toast = Some(toast);
        }
        if unit.trim().chars().count() > lv_check::UNIT_LIMIT {
            ui.label(
                RichText::new(format!(
                    "Einheit: GAEB überträgt höchstens {} Zeichen („{}“).",
                    lv_check::UNIT_LIMIT,
                    unit.trim()
                        .chars()
                        .take(lv_check::UNIT_LIMIT)
                        .collect::<String>()
                ))
                .size(12.0)
                .color(palette.warn),
            );
        }
        for error in [&self.numbers.quantity_error, &self.numbers.price_error]
            .into_iter()
            .flatten()
        {
            ui.label(
                RichText::new(error.as_str())
                    .size(12.0)
                    .color(palette.error),
            );
        }

        // Unit suggestions stay until the click on one of them is finished.
        let (pointer_down, pointer_released) =
            ctx.input(|input| (input.pointer.any_down(), input.pointer.any_released()));
        self.unit_chips = unit_focused || (self.unit_chips && (pointer_down || pointer_released));
        if self.unit_chips {
            let needle = unit.trim().to_lowercase();
            ui.horizontal_wrapped(|ui| {
                for suggestion in UNITS {
                    if !needle.is_empty()
                        && !suggestion.to_lowercase().starts_with(&needle)
                        && suggestion.to_lowercase() != needle
                    {
                        continue;
                    }
                    let chip = ui.add(
                        egui::Button::new(RichText::new(suggestion).size(12.5))
                            .sense(Sense::CLICK)
                            .min_size(egui::vec2(34.0, 22.0)),
                    );
                    if chip.clicked() {
                        unit = suggestion.to_string();
                        unit_value = Some(unit.clone());
                        ctx.memory_mut(|memory| {
                            memory.request_focus(egui::Id::unique(("price", id)))
                        });
                    }
                }
            });
        }
        if let Some(unit) = unit_value {
            self.undo
                .record(project.lv(), Some(EditKey(format!("unit:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_unit(&unit);
            }
            output.dirty = true;
        }
        // Gültige Eingaben gelten sofort – Strg+Enter, Strg+S oder ein Klick auf
        // eine andere Zeile verlieren sonst den gerade getippten Wert.
        if let Some((value, formula)) = quantity_edit.apply(
            &self.numbers.quantity,
            &mut self.numbers.quantity_error,
            numbers::parse_quantity_entry,
            "Menge",
        ) {
            if value != position.quantity() || formula.as_deref() != position.quantity_formula() {
                self.undo
                    .record(project.lv(), Some(EditKey(format!("qty:{}", id.0))));
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_quantity(value);
                    position.set_quantity_formula(formula.clone());
                }
                output.dirty = true;
            }
            if quantity_edit.left && formula.is_none() {
                self.numbers.quantity = numbers::quantity_text(value);
            }
        }
        if let Some(formula) = project.lv().rows()[index]
            .as_position()
            .and_then(|position| {
                position
                    .quantity_formula()
                    .map(|f| (f.to_string(), position.quantity()))
            })
        {
            ui.label(
                RichText::new(format!(
                    "Rechenansatz {} = {}",
                    formula.0,
                    numbers::quantity_text(formula.1)
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
        }
        if let Some(value) = price_edit.apply(
            &self.numbers.price,
            &mut self.numbers.price_error,
            numbers::parse_price,
            "EP",
        ) {
            if value != position.unit_price() {
                self.undo
                    .record(project.lv(), Some(EditKey(format!("price:{}", id.0))));
                if let Some(position) = project
                    .lv_mut()
                    .row_mut(index)
                    .and_then(LvRow::as_position_mut)
                {
                    position.set_unit_price(value);
                }
                output.dirty = true;
            }
            if price_edit.left {
                self.numbers.price = numbers::price_text(value);
            }
        }

        let current = project.lv().rows()[index]
            .as_position()
            .cloned()
            .unwrap_or_else(|| position.clone());
        ui.add_space(6.0);
        let total = project.lv().shown_total(&current);
        let counted = current.kind().counts_in_total();
        let currency = project.lv().currency().to_string();
        if current.is_surcharge() {
            // Zuschlagsposition: EP = Prozentsatz, Grundlage = Bezugssumme.
            let base = project
                .lv()
                .surcharge_values()
                .get(&id)
                .map(|value| value.base)
                .unwrap_or_default();
            let word = if current.surcharge().is_some_and(|surcharge| surcharge.split) {
                "Aufteilung: EP = Anteil in %"
            } else {
                "Zuschlag: EP = Prozentsatz"
            };
            ui.label(
                RichText::new(format!(
                    "{word} · Bezugssumme {}",
                    proava_domain::price_check::amount(base, &currency)
                ))
                .size(12.5)
                .color(palette.text_dim),
            );
        }
        // Rechengrundlage of Aufteilungszeilen: its GP is in them.
        let basis = !current.is_surcharge() && project.lv().split_bases().contains(&id);
        let total = proava_domain::price_check::amount(total, &currency);
        ui.label(
            RichText::new(if basis {
                format!(
                    "GP ({total}) – Rechengrundlage, aufgeteilt (zählt in den Aufteilungszeilen)"
                )
            } else if counted {
                format!("GP {total}")
            } else {
                format!("GP ({total}) – zählt nicht in die Summe")
            })
            .size(14.0)
            .strong()
            .color(palette.text),
        );
        // GP-Prüfung of the import: text and „Bestätigen“ right here.
        if let Some(check) = current.price_check().copied() {
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(check.text(&currency))
                        .size(12.5)
                        .color(palette.warn),
                )
                .wrap(),
            );
            if ui
                .button("GP-Abweichung bestätigen")
                .on_hover_text("Der errechnete GP (Menge × EP) bleibt, die orange Marke geht weg (Strg+Z nimmt es zurück).")
                .clicked()
            {
                output.merge(self.confirm_price_checks(project, &[id]));
            }
        }
        ui.add_space(10.0);

        // Positionsart und Merkmale
        let mut kind = current.kind();
        let mut lump = current.is_lump_sum();
        let mut hourly = current.is_hourly();
        // STLB-Platzhalter: no switch (Anwender 18.11.: by hand straight to STLB-Bau);
        // a placeholder from GAEB/MCP stays, removable in the context menu.
        let stlb = current.is_stlb_placeholder();
        ui.label(
            RichText::new("Positionsart")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        egui::ComboBox::from_id_salt(("kind", id))
            .width(ui.available_width().min(320.0))
            .selected_text(kind_label(kind))
            .show_ui(ui, |ui| {
                let group = kind_group(kind);
                for option in [
                    PositionKind::Normal,
                    PositionKind::Provisional { with_total: false },
                    PositionKind::Provisional { with_total: true },
                    PositionKind::Base { group },
                    PositionKind::Alternative { group, serial: 1 },
                    PositionKind::Supplement,
                ] {
                    let same = std::mem::discriminant(&option) == std::mem::discriminant(&kind)
                        && kind_label(option) == kind_label(kind);
                    if ui.selectable_label(same, kind_label(option)).clicked() && !same {
                        kind = option;
                    }
                }
            });
        match &mut kind {
            PositionKind::Base { group } => {
                crate::widgets::row(ui, |ui| {
                    ui.label("Wahlgruppe");
                    ui.add(egui::DragValue::new(group).range(1..=999));
                });
            }
            PositionKind::Alternative { group, serial } => {
                crate::widgets::row(ui, |ui| {
                    ui.label("Wahlgruppe");
                    ui.add(egui::DragValue::new(group).range(1..=999));
                    ui.add_space(8.0);
                    ui.label("Wahl-Nr.");
                    ui.add(egui::DragValue::new(serial).range(1..=9));
                });
            }
            _ => {}
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut lump, "Pauschal");
            ui.checkbox(&mut hourly, "Stundenlohn");
        });
        if kind != current.kind()
            || lump != current.is_lump_sum()
            || hourly != current.is_hourly()
            || stlb != current.is_stlb_placeholder()
        {
            self.undo
                .record(project.lv(), Some(EditKey(format!("kind:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_kind(kind);
                if lump && !position.is_lump_sum() && position.unit().is_empty() {
                    position.set_unit("psch");
                }
                position.set_lump_sum(lump);
                position.set_hourly(hourly);
                position.set_stlb_placeholder(stlb);
            }
            output.dirty = true;
        }
        self.surcharge_fields(ui, palette, project, index, output);
        ui.add_space(10.0);
        self.cost_group_field(ui, palette, project, &[id], output);
        // The Ausführungsbeschreibung is chosen in the line under the OZ
        // (performance_link_line) – one place per window (Anwender 18.11.).
        self.library_fields(ui, palette, project, index, output);
        self.position_extra(ui, palette, project, index, output);
        ui.add_space(6.0);
        ui.label(RichText::new("Langtext").size(12.0).color(palette.text_dim));
        ui.add_space(3.0);
        self.long_text_section(
            ui,
            palette,
            project,
            index,
            current.long_text().clone(),
            output,
        );
    }

    /// „Pauschalsumme netto“ of a title (GAEB TotalLSUM): taken when the
    /// field is left or Enter is pressed; empty = the sum of its block.
    fn title_lump_field(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(group) = project.lv().rows()[index].as_group() else {
            return;
        };
        let id = group.id();
        let current = group.lump_sum_total();
        let shown = |total: Option<Money>| {
            total
                .map(|total| proava_domain::format_decimal_de(total.to_decimal(), 2))
                .unwrap_or_default()
        };
        let key = ui.scope_id().with(("title-lump", id.0, self.revision));
        let mut text = ui
            .data_mut(|data| data.get_temp::<String>(key))
            .unwrap_or_else(|| shown(current));
        ui.add_space(6.0);
        ui.label(
            RichText::new("Pauschalsumme netto (leer = Summe der Positionen)")
                .size(12.0)
                .color(palette.text_dim),
        );
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text("z. B. 15.000,00")
                .desired_width(160.0),
        );
        let parsed = {
            let trimmed = text.trim().trim_end_matches('€').trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                proava_domain::parse_decimal_de(trimmed)
                    .ok()
                    .filter(|value| !value.is_sign_negative())
                    .map(|value| Some(Money::from_decimal(value)))
                    .ok_or_else(|| format!("„{trimmed}“ ist kein Betrag."))
            }
        };
        match &parsed {
            Err(error) => {
                ui.label(RichText::new(error).size(12.0).color(palette.warn));
            }
            Ok(value) if response.lost_focus() && *value != current => {
                self.undo
                    .record(project.lv(), Some(EditKey(format!("lump:{}", id.0))));
                if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
                    group.set_lump_sum_total(*value);
                }
                output.dirty = true;
                text = shown(*value);
            }
            Ok(_) => {}
        }
        if !response.has_focus() && parsed.is_ok() && !response.lost_focus() {
            text = shown(current);
        }
        ui.data_mut(|data| data.insert_temp(key, text));
    }

    /// Zuschlag of a position: „Zuschlagsposition (%)“ with the kind of
    /// base, or „zu bezuschlagen“ (counts for Zuschläge on marked positions).
    fn surcharge_fields(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        use proava_domain::{Surcharge, SurchargeKind};
        let Some(current) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = current.id();
        let mut surcharge = current.surcharge().cloned();
        let mut marked = current.is_marked_for_surcharge();
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let mut is_surcharge = surcharge.is_some();
            if ui
                .checkbox(&mut is_surcharge, "Zuschlagsposition (%)")
                .on_hover_text(
                    "Prozentualer Zuschlag auf andere Positionen (GAEB MarkupItem):                      EP = Prozentsatz, GP = Bezugssumme × Prozentsatz; ohne Menge und Einheit.",
                )
                .changed()
            {
                surcharge = is_surcharge.then(Surcharge::default);
            }
            if surcharge.is_none() {
                ui.checkbox(&mut marked, "zu bezuschlagen").on_hover_text(
                    "Zählt für Zuschlagspositionen „auf die gekennzeichneten Positionen“.",
                );
            }
            // Aufteilungszeile: only in a Kosten-LV (shown elsewhere to take it off).
            if let Some(surcharge) = surcharge.as_mut() {
                if !project.lv().purpose().is_tender() || surcharge.split {
                    ui.checkbox(&mut surcharge.split, "Aufteilung nach KG").on_hover_text(
                        "Aufteilungszeile (Kosten-LV): EP = Anteil in %, die KG der Zeile bekommt \
                         Bezugssumme × Anteil. Die bezogenen Positionen sind nur Rechengrundlage \
                         und zählen nicht selbst.",
                    );
                }
            }
        });
        if let Some(surcharge) = &mut surcharge {
            egui::ComboBox::from_id_salt(("surcharge-kind", id))
                .width(ui.available_width().min(320.0))
                .selected_text(format!(
                    "{} {}",
                    if surcharge.split {
                        "Aufteilung"
                    } else {
                        "Zuschlag"
                    },
                    surcharge.kind.label()
                ))
                .show_ui(ui, |ui| {
                    for kind in [
                        SurchargeKind::AllInList,
                        SurchargeKind::Marked,
                        SurchargeKind::Listed,
                    ] {
                        if ui
                            .selectable_label(surcharge.kind == kind, kind.label())
                            .clicked()
                        {
                            surcharge.kind = kind;
                        }
                    }
                });
            if surcharge.kind == SurchargeKind::Listed {
                let listed: Vec<String> = surcharge
                    .references
                    .iter()
                    .filter_map(|reference| project.lv().position_by_id(reference.position))
                    .map(|position| position.ordinal().to_string())
                    .collect();
                ui.label(
                    RichText::new(if listed.is_empty() {
                        "Bezug: noch keine Positionen – im Fenster Eigenschaften (Alt+Enter), Reiter Allgemein, wählen".to_string()
                    } else {
                        format!("Bezug: {}", listed.join(", "))
                    })
                    .size(12.0)
                    .color(palette.text_dim),
                );
            }
        }
        if surcharge.as_ref() != current.surcharge() || marked != current.is_marked_for_surcharge()
        {
            self.undo
                .record(project.lv(), Some(EditKey(format!("surcharge:{}", id.0))));
            if let Some(position) = project
                .lv_mut()
                .row_mut(index)
                .and_then(LvRow::as_position_mut)
            {
                position.set_surcharge(surcharge);
                position.set_marked_for_surcharge(marked);
            }
            self.numbers.quantity = numbers::quantity_text(proava_domain::Decimal::ZERO);
            output.dirty = true;
        }
    }

    fn long_text_section(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        text: proava_domain::RichText,
        output: &mut EditorOutput,
    ) {
        let id = project.lv().rows()[index].id();
        if self.long_text_owner != Some((id, self.revision)) {
            self.long_text_owner = Some((id, self.revision));
            self.long_text.reset();
        }
        if stlb_lock::text_locked(project, index) {
            // STLB-Bau-Text: shown, but write-protected; the values of the
            // Textergänzungen stay free (fields below).
            self.focus = None;
            self.locked_long_text(ui, palette, project, index, text, output);
            return;
        }
        if self.focus == Some(Focus::Long) {
            self.long_text.request_focus();
            self.focus = None;
        }
        if ui
            .button("In eigenem Fenster bearbeiten …")
            .on_hover_text(
                "Großes, frei verschieb- und vergrößerbares Fenster für lange Texte (Umschalt+F2)",
            )
            .clicked()
        {
            self.open_big_text_for(id);
        }
        ui.add_space(3.0);
        let mut text = text;
        let complements = row_complements(project, index);
        let mut list = complements.clone().unwrap_or_default();
        let result = if complements.is_some() {
            self.long_text.show_with_complements(
                ui,
                palette,
                &mut text,
                &mut list,
                project.images(),
            )
        } else {
            self.long_text
                .show(ui, palette, &mut text, project.images())
        };
        if project.lv().ensure_editable(&[id]).is_err() {
            // Fertige Position: der Langtext bleibt, wie er ist (der Editor lädt ihn neu).
            self.long_text.reset();
            return;
        }
        if result.pick_image {
            output.pick_images = true;
        }
        if result.pick_pdf_clip {
            self.wants_clip = Some((id, pdf_clip::ClipPlace::Panel));
        }
        if result.changed {
            let key = result
                .edit_key
                .map(|key| EditKey(format!("{}:{key}", id.0)));
            self.undo.record(project.lv(), key);
            set_long_text(project, index, text);
            set_complements(project, index, complements.map(|_| list));
            output.dirty = true;
        }
    }

    /// F2: long text of the selected row in its own window.
    fn open_big_text(&mut self, project: &Project) {
        let Some(index) = self.selected_index(project.lv()) else {
            return;
        };
        let row = &project.lv().rows()[index];
        if !row.is_group() && !row.is_spacer() {
            self.open_big_text_for(row.id());
        }
    }

    fn open_big_text_for(&mut self, row: RowId) {
        match &mut self.big_text {
            Some(big) if big.row == row => big.focus = true,
            _ => self.big_text = Some(crate::long_text_window::BigText::new(row)),
        }
    }

    fn big_text_window(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        output: &mut EditorOutput,
    ) {
        let Some(mut big) = self.big_text.take() else {
            return;
        };
        // Follows the selection of the table (the active row) unless held;
        // a row without long text leaves it where it is, with a note.
        if !big.held {
            match self
                .selected
                .filter(|id| *id != big.row)
                .and_then(|id| project.lv().index_of(id))
            {
                Some(at) => match &project.lv().rows()[at] {
                    LvRow::Position(_) | LvRow::Note(_) | LvRow::Performance(_) => {
                        big.pending = Some(project.lv().rows()[at].id());
                        big.skipped = None;
                    }
                    other => {
                        let what = match other {
                            LvRow::Group(_) => format!("Titel {}", other.ordinal()),
                            _ => "Ein Abstandshalter".to_string(),
                        };
                        let here = project
                            .lv()
                            .index_of(big.row)
                            .map(|index| {
                                let row = &project.lv().rows()[index];
                                match row {
                                    LvRow::Performance(performance) => performance.mark(),
                                    _ => row.ordinal().to_string(),
                                }
                            })
                            .unwrap_or_default();
                        big.pending = None;
                        big.skipped = Some(format!(
                            "{what} hat keinen Langtext – das Fenster bleibt bei {here}."
                        ));
                    }
                },
                None => {
                    big.pending = None;
                    big.skipped = None;
                }
            }
            if let Some(next) = big.pending.filter(|_| !big.field_focused()) {
                big.show_row(next);
            }
        }
        let Some(index) = project.lv().index_of(big.row) else {
            return;
        };
        let window_row = big.row;
        let mut result = big.show(ctx, palette, project, index, launcher);
        if result.close {
            if let Some(size) = result.size {
                launcher.set_long_text_window(size);
            }
        } else {
            self.big_text = Some(big);
        }
        if result.pick_image {
            // Images go into the row of the window.
            self.selected = Some(project.lv().rows()[index].id());
            output.pick_images = true;
        }
        if let Some(message) = result.toast.take() {
            output.merge(EditorOutput::toast(message));
        }
        if let Some((prepared, width)) = result.clip {
            let row = project.lv().rows()[index].id();
            output.merge(self.insert_pdf_clip(
                project,
                row,
                prepared,
                width,
                pdf_clip::ClipPlace::Big,
            ));
        }
        if !result.files.is_empty() {
            self.selected = Some(project.lv().rows()[index].id());
            output.merge(self.insert_images(project, result.files));
        }
        if let Some((text, complements, key)) = result.changed {
            let key = key.map(|key| EditKey(format!("{}:big:{key}", big_row_id(project, index))));
            self.undo.record(project.lv(), key);
            set_long_text(project, index, text);
            set_complements(project, index, complements);
            output.dirty = true;
        }
        if result.undo || result.redo {
            let command = if result.undo {
                Command::Undo
            } else {
                Command::Redo
            };
            output.merge(self.run(command, project, ctx));
        }
        if let Some(short) = result.short_text {
            self.undo.record(
                project.lv(),
                Some(EditKey(format!("short:{}", big_row_id(project, index)))),
            );
            if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
                position.set_short_text(short);
            }
            self.revision += 1;
            output.dirty = true;
        }
        if let Some(choice) = result.new_row {
            // After the window's row (the window stays on it); the take-over
            // dialogs open in the main window.
            self.select(project.lv(), window_row);
            output.merge(self.run_choice(choice, project, ctx));
            if matches!(
                choice,
                MenuChoice::Editor(Command::FromStlb | Command::FromAusschreiben)
            ) {
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
        }
    }

    // ----- Tabelle -----

    /// Drag handles between the column titles; double click restores the width.
    fn column_splitters(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        palette: &Palette,
        launcher: &Launcher,
    ) {
        let columns = self.widths.layout(rect.left(), rect.right());
        let mut changed = false;
        for (column, x) in columns.splitters() {
            let handle = Rect::from_center_size(
                egui::pos2(x, rect.center().y),
                Vec2::new(8.0, rect.height()),
            );
            let response = ui
                .interact(
                    handle,
                    egui::Id::unique(("column_splitter", column.key())),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text("Spaltenbreite ziehen · Doppelklick: Standardbreite");
            if response.hovered() || response.dragged() {
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() + 4.0),
                        egui::pos2(x, rect.bottom() - 4.0),
                    ],
                    Stroke::new(2.0, palette.accent),
                );
            }
            if response.dragged() {
                self.widths.drag(column, response.drag_delta().x);
            }
            if response.double_clicked() {
                self.widths.reset(column);
                changed = true;
            }
            if response.drag_stopped() {
                changed = true;
            }
        }
        if changed {
            launcher.set_column_widths(self.widths.to_map());
        }
    }

    /// Menu of the table head (right click): the optional columns and the
    /// widths. Returns the message of a change.
    fn column_menu(&mut self, ui: &mut egui::Ui, launcher: &Launcher) -> Option<String> {
        let mut toast = None;
        for column in Column::OPTIONAL {
            let mut on = self.widths.shown(column);
            if ui.checkbox(&mut on, column.menu_label()).clicked() {
                toast = Some(self.toggle_column(column));
                ui.close();
            }
        }
        let mut share = self.show_share;
        if ui.checkbox(&mut share, TITLE_SHARE_LABEL).clicked() {
            toast = Some(self.toggle_title_share());
            ui.close();
        }
        ui.separator();
        if ui.button("Spaltenbreiten zurücksetzen").clicked() {
            for column in Column::ALL {
                self.widths.reset(column);
            }
            launcher.set_column_widths(self.widths.to_map());
            toast = Some("Spaltenbreiten zurückgesetzt.".to_string());
            ui.close();
        }
        toast
    }

    fn table_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        launcher: &Launcher,
        output: &mut EditorOutput,
    ) {
        let lv_snapshot_totals = project.lv().group_totals();
        let lv_snapshot_net = project.lv().net_total();
        let lv_snapshot_surcharges = project.lv().split_computation();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ui, |ui| {
                if self.search.is_some() {
                    self.search_bar(ui, palette, project, output);
                    ui.add_space(6.0);
                }
                if self.replace.is_some() {
                    self.replace_bar(ui, palette, project, output);
                    ui.add_space(6.0);
                }
                if self.filter != StatusFilter::All {
                    let mut clear = false;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(self.filter.label())
                                .size(12.5)
                                .color(palette.accent),
                        );
                        if ui.small_button("Filter aufheben").clicked() {
                            clear = true;
                        }
                    });
                    if clear {
                        output.merge(self.run(
                            Command::Filter(StatusFilter::All),
                            project,
                            &ui.ctx().clone(),
                        ));
                    }
                    ui.add_space(4.0);
                }
                let footer_height = 30.0;
                let width = ui.available_width();
                let (header_rect, header) =
                    ui.allocate_exact_size(Vec2::new(width, HEADER_HEIGHT), Sense::click());
                header.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::RowHeader, true, HEADER_NAME)
                });
                header_row(ui.painter(), header_rect, palette, &self.widths);
                // Right click on the head: switch columns on/off.
                header.context_menu(|ui| {
                    if let Some(toast) = self.column_menu(ui, launcher) {
                        output.toast = Some(toast);
                    }
                });
                self.column_splitters(ui, header_rect, palette, launcher);

                let table_height = (ui.available_height() - footer_height - 4.0).max(ROW_HEIGHT);
                let visible = self.visible(project.lv());
                // The LVs of the table one below the other (all LVs of the
                // tender, or a Kostenermittlung alone), each with its head row
                // when the project has several LVs.
                let shown = multi_lv::shown(project);
                let lv_count = shown.len();
                let active = project.active_lv();
                let view: Vec<ViewRow> = {
                    let folding = &self.folding;
                    let filter = self.filter;
                    let no_titles = HashSet::new();
                    multi_lv::view_rows(
                        &shown,
                        multi_lv::heads(project),
                        |lv| {
                            if lv == active {
                                return visible.clone();
                            }
                            let (Some(data), Some(key)) = (project.lv_at(lv), project.lv_key(lv))
                            else {
                                return Vec::new();
                            };
                            let collapsed = folding.titles.get(&key).unwrap_or(&no_titles);
                            status_view::filter_rows(
                                data,
                                lv_ops::visible_rows(data, collapsed),
                                filter,
                            )
                        },
                        |lv| {
                            project
                                .lv_key(lv)
                                .is_some_and(|key| folding.lvs.contains(&key))
                        },
                    )
                };
                let hits = match &self.replace {
                    Some(bar) if !bar.find.is_empty() => {
                        search_replace::matching_rows(project.lv(), &bar.find, &bar.options())
                    }
                    _ => HashSet::new(),
                };
                // Suchen und Ersetzen over all LVs: the hits of the other LVs too.
                let other_hits: std::collections::HashMap<usize, HashSet<RowId>> = match &self
                    .replace
                {
                    Some(bar)
                        if !bar.find.is_empty() && lv_count > 1 && bar.options().only.is_none() =>
                    {
                        shown
                            .iter()
                            .copied()
                            .filter(|lv| *lv != active)
                            .filter_map(|lv| {
                                let data = project.lv_at(lv)?;
                                Some((
                                    lv,
                                    search_replace::matching_rows(data, &bar.find, &bar.options()),
                                ))
                            })
                            .collect()
                    }
                    _ => std::collections::HashMap::new(),
                };
                let depths = depths(project.lv());
                let selected = self.selected_index(project.lv());
                let marked = self.marked.clone();

                if self.scroll_to_selected {
                    let target = self
                        .scroll_row
                        .and_then(|id| project.lv().index_of(id))
                        .or(selected);
                    let mut keep_selection = true;
                    if let Some((lv, with_selection)) = self.scroll_head.take() {
                        if let Some(head) = view.iter().position(|row| *row == ViewRow::Head(lv)) {
                            self.scroll_offset = head as f32 * ROW_HEIGHT;
                            keep_selection = with_selection;
                        }
                    }
                    if let Some(position) = target
                        .and_then(|index| {
                            view.iter()
                                .position(|row| *row == ViewRow::Row(active, index))
                        })
                        .filter(|_| keep_selection)
                    {
                        let top = position as f32 * ROW_HEIGHT;
                        let height = self.viewport_height.max(ROW_HEIGHT);
                        if top < self.scroll_offset {
                            self.scroll_offset = top;
                        } else if top + ROW_HEIGHT > self.scroll_offset + height {
                            self.scroll_offset = top + ROW_HEIGHT - height;
                        }
                    }
                }

                let mut clicked: Option<(usize, RowClick)> = None;
                let mut cell_events = table_cells::CellEvents::default();
                let mut context_command: Option<context_menu::MenuChoice> = None;
                let mut free_area_menu = false;
                let selection: Vec<usize> = {
                    let ids = self.selection_ids();
                    let mut indices: Vec<usize> = ids
                        .iter()
                        .filter_map(|id| project.lv().index_of(*id))
                        .collect();
                    indices.sort_unstable();
                    indices
                };
                let menu_state = context_menu::MenuState::of(
                    project.lv(),
                    &selection,
                    !self.clipboard.is_empty(),
                );
                let mut drop_target: Option<usize> = None;
                // Dragged onto another LV: (LV, insert before this row).
                let mut drop_other: Option<(usize, usize)> = None;
                let mut jump: Option<LvJump> = None;
                let mut fold_lv: Option<u64> = None;
                let mut fold_other: Option<(u64, RowId)> = None;
                let mut head_action: Option<crate::lv_list::LvListAction> = None;
                // Umschalt+Klick into another LV: the selection goes to the
                // end (true) or the start of the active LV and stays in it.
                let mut extend_to_edge: Option<bool> = None;
                let mark_empty = launcher.mark_empty_quantities();
                let no_hits = HashSet::new();
                self.other_rects.clear();
                let pointer = ui.ctx().pointer_hover_pos();
                // A drag from another window finds its row anew in every frame.
                self.cells.remote_hover = None;

                ui.allocate_ui(Vec2::new(width, table_height), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    // No rubber band: scrolling to a row just added at the end
                    // sets the offset past last frame's end; egui took that as
                    // overscroll, sprang back and coasted on – the new row
                    // ended up half hidden behind the footer.
                    ui.spacing_mut().scroll.kinetic.rubber_band = false;
                    let mut area = egui::ScrollArea::vertical()
                        .id_salt("lv_table")
                        .auto_shrink([false, false])
                        .max_height(table_height);
                    if self.scroll_to_selected {
                        area = area.vertical_scroll_offset(self.scroll_offset);
                    }
                    self.row_rects.clear();
                    let scroll = area.show_rows(ui, ROW_HEIGHT, view.len(), |ui, range| {
                        if view.is_empty() {
                            return;
                        }
                        // Dragged from another window to the upper or lower
                        // edge: the table scrolls along.
                        if let Some(pointer) = self.cells.remote_pointer {
                            let clip = ui.clip_rect();
                            if clip.x_range().contains(pointer.x) {
                                let speed = if pointer.y < clip.top() + REMOTE_EDGE
                                    && pointer.y > clip.top() - 2.0 * REMOTE_EDGE
                                {
                                    REMOTE_SCROLL
                                } else if pointer.y > clip.bottom() - REMOTE_EDGE
                                    && pointer.y < clip.bottom() + 2.0 * REMOTE_EDGE
                                {
                                    -REMOTE_SCROLL
                                } else {
                                    0.0
                                };
                                if speed != 0.0 {
                                    ui.scroll_with_delta(Vec2::new(0.0, speed));
                                    ui.ctx().request_repaint();
                                }
                            }
                        }
                        for visible_index in range {
                            let index = match view[visible_index] {
                                ViewRow::Row(lv, index) if lv == active => index,
                                ViewRow::Head(lv) => {
                                    let (Some(data), Some(key)) =
                                        (project.lv_at(lv), project.lv_key(lv))
                                    else {
                                        continue;
                                    };
                                    let folded = self.folding.lvs.contains(&key);
                                    let (response, click) = multi_lv::head_row(
                                        ui,
                                        palette,
                                        data,
                                        folded,
                                        lv == active,
                                        &self.widths,
                                    );
                                    match click {
                                        Some(multi_lv::HeadClick::Toggle) => fold_lv = Some(key),
                                        Some(multi_lv::HeadClick::Select) => {
                                            jump = Some(LvJump {
                                                lv,
                                                row: None,
                                                then: JumpThen::Head,
                                            })
                                        }
                                        Some(multi_lv::HeadClick::Properties) => {
                                            head_action =
                                                Some(crate::lv_list::LvListAction::Properties(lv))
                                        }
                                        None => {}
                                    }
                                    response.context_menu(|ui| {
                                        if let Some(action) =
                                            multi_lv::head_menu(ui, lv, project.lv_count(), true)
                                        {
                                            head_action = Some(action);
                                        }
                                    });
                                    // Dragged onto the head row: to the start of that LV.
                                    if self.drag.is_some()
                                        && pointer.is_some_and(|pos| {
                                            response.rect.y_range().contains(pos.y)
                                        })
                                    {
                                        if lv == active {
                                            drop_target = Some(0);
                                        } else {
                                            drop_other = Some((lv, 0));
                                        }
                                        let y = response.rect.bottom();
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(response.rect.left(), y),
                                                egui::pos2(response.rect.right(), y),
                                            ],
                                            Stroke::new(2.5, palette.accent),
                                        );
                                    }
                                    continue;
                                }
                                ViewRow::Row(lv, index) => {
                                    // A row of another LV: drawn, a click makes its LV active.
                                    let (Some(data), Some(key)) =
                                        (project.lv_at(lv), project.lv_key(lv))
                                    else {
                                        continue;
                                    };
                                    let Some(other) = self.others.of(project, lv) else {
                                        continue;
                                    };
                                    let titles = self.folding.titles.get(&key).unwrap_or(&no_hits);
                                    let paint = LvPaint {
                                        lv: data,
                                        depths: &other.depths,
                                        totals: &other.totals,
                                        net_total: other.net_total,
                                        show_share: self.show_share,
                                        surcharges: &other.surcharges,
                                        collapsed: titles,
                                        hits: other_hits.get(&lv).unwrap_or(&no_hits),
                                    };
                                    let info = paint.row_info(
                                        index,
                                        self.show_gaps,
                                        mark_empty,
                                        &self.widths,
                                        false,
                                        false,
                                    );
                                    let row = &data.rows()[index];
                                    let id = row.id();
                                    let collapsed = info.collapsed;
                                    let (response, click) =
                                        lv_row(ui, palette, row, &info, &self.widths);
                                    self.other_rects.push((lv, index, response.rect));
                                    let modifiers = ui.ctx().input(|input| input.modifiers);
                                    match click {
                                        Some(RowClick::Toggle) => fold_other = Some((key, id)),
                                        Some(RowClick::Edit) => {
                                            jump = Some(LvJump {
                                                lv,
                                                row: Some(id),
                                                then: JumpThen::Window,
                                            })
                                        }
                                        Some(RowClick::Select) if modifiers.shift => {
                                            extend_to_edge = Some(lv > active)
                                        }
                                        Some(_) => {
                                            jump = Some(LvJump {
                                                lv,
                                                row: Some(id),
                                                then: JumpThen::Select,
                                            })
                                        }
                                        None => {}
                                    }
                                    if response.secondary_clicked() {
                                        jump = Some(LvJump {
                                            lv,
                                            row: Some(id),
                                            then: JumpThen::Menu,
                                        });
                                    }
                                    if response.drag_started() && self.drag.is_none() {
                                        jump = Some(LvJump {
                                            lv,
                                            row: Some(id),
                                            then: JumpThen::Drag,
                                        });
                                    }
                                    if self.drag.is_some() {
                                        if let Some(pos) = pointer
                                            .filter(|pos| response.rect.y_range().contains(pos.y))
                                        {
                                            let after = pos.y > response.rect.center().y;
                                            let before = match (after, collapsed) {
                                                (false, _) => index,
                                                (true, true) => data.block_range(index).end,
                                                (true, false) => index + 1,
                                            };
                                            drop_other = Some((lv, before));
                                            let y = if after {
                                                response.rect.bottom()
                                            } else {
                                                response.rect.top()
                                            };
                                            ui.painter().line_segment(
                                                [
                                                    egui::pos2(response.rect.left(), y),
                                                    egui::pos2(response.rect.right(), y),
                                                ],
                                                Stroke::new(2.5, palette.accent),
                                            );
                                        }
                                    }
                                    continue;
                                }
                            };
                            let row = &project.lv().rows()[index];
                            let paint = LvPaint {
                                lv: project.lv(),
                                depths: &depths,
                                totals: &lv_snapshot_totals,
                                net_total: lv_snapshot_net,
                                show_share: self.show_share,
                                surcharges: &lv_snapshot_surcharges,
                                collapsed: &self.collapsed,
                                hits: &hits,
                            };
                            let info = paint.row_info(
                                index,
                                self.show_gaps,
                                mark_empty,
                                &self.widths,
                                Some(index) == selected,
                                marked.contains(&row.id()),
                            );
                            let (response, click) = lv_row(ui, palette, row, &info, &self.widths);
                            self.row_rects.push((index, response.rect));
                            self.cell_overlay(
                                ui,
                                palette,
                                row,
                                response.rect,
                                info.depth as f32 * INDENT,
                                info.selected,
                                &menu_state,
                                &mut cell_events,
                            );
                            if let Some(choice) = cell_events.menu.take() {
                                context_command = Some(choice);
                            }
                            if let Some(click) = click {
                                clicked = Some((index, click));
                            }
                            if response.drag_started() {
                                self.drag = Some(row.id());
                            }
                            if self.drag.is_some() {
                                if let Some(pos) = pointer {
                                    if response.rect.y_range().contains(pos.y) {
                                        let after = pos.y > response.rect.center().y;
                                        drop_target = Some(match (after, info.collapsed) {
                                            (false, _) => index,
                                            (true, true) => project.lv().block_range(index).end,
                                            (true, false) => index + 1,
                                        });
                                        let y = if after {
                                            response.rect.bottom()
                                        } else {
                                            response.rect.top()
                                        };
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(response.rect.left(), y),
                                                egui::pos2(response.rect.right(), y),
                                            ],
                                            Stroke::new(2.5, palette.accent),
                                        );
                                    }
                                }
                            }
                            let keyboard_menu = context_menu::keyboard_menu_for(ui.ctx(), row.id());
                            let mut popup = egui::Popup::context_menu(&response);
                            if let Some(first) = keyboard_menu {
                                popup = popup.at_position(
                                    response.rect.left_bottom() + Vec2::new(140.0, 0.0),
                                );
                                if first {
                                    popup =
                                        popup.open_memory(Some(egui::SetOpenCommand::Bool(true)));
                                }
                            }
                            popup.show(|ui| {
                                if let Some(choice) = context_menu::show(ui, &menu_state, Some(row))
                                {
                                    context_command = Some(choice);
                                }
                            });
                            if keyboard_menu.is_some() {
                                context_menu::keyboard_menu_shown(ui.ctx(), &response);
                            }
                            if response.secondary_clicked() {
                                clicked = Some((index, RowClick::Keep));
                            }
                        }
                    });
                    self.viewport_height = scroll.inner_rect.height();
                    self.scroll_offset = scroll.state.clamped_offset().y;
                    // Right click below the last row: insert at the end.
                    let used = (view.len() as f32 * ROW_HEIGHT - self.scroll_offset).max(0.0);
                    let inner = scroll.inner_rect;
                    if inner.height() - used > 6.0 {
                        let free = Rect::from_min_max(
                            egui::pos2(inner.left(), inner.top() + used),
                            inner.right_bottom(),
                        );
                        let response =
                            ui.interact(free, egui::Id::unique("lv_table_free"), Sense::click());
                        response.context_menu(|ui| {
                            if let Some(choice) = context_menu::show(ui, &menu_state, None) {
                                context_command = Some(choice);
                            }
                        });
                        if response.secondary_clicked() || response.clicked() {
                            free_area_menu = true;
                        }
                    }
                    if view.is_empty() {
                        empty_state(ui, palette, scroll.inner_rect);
                    }
                    drop_hint(ui, palette, scroll.inner_rect);
                });
                self.scroll_to_selected = false;
                self.scroll_row = None;
                self.view_rows = view;
                if let Some(key) = fold_lv {
                    self.toggle_lv_fold(key);
                }
                if let Some((key, id)) = fold_other {
                    let titles = self.folding.titles.entry(key).or_default();
                    if !titles.remove(&id) {
                        titles.insert(id);
                    }
                }
                if let Some(down) = extend_to_edge {
                    self.extend_to_edge(project.lv(), down);
                }
                if head_action.is_some() {
                    output.lv_list = head_action;
                }
                if jump.is_some() {
                    output.jump = jump;
                }

                let (footer_rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, footer_height), Sense::hover());
                footer(ui.painter(), footer_rect, palette, project.lv());

                self.cell_events(project, cell_events, output);
                if let Some((index, click)) = clicked {
                    let id = project.lv().rows()[index].id();
                    let pointer_cell = ui.ctx().pointer_interact_pos().and_then(|pos| {
                        let row = &project.lv().rows()[index];
                        self.cell_at_pointer(row, pos.x, depths[index] as f32 * INDENT)
                    });
                    match click {
                        RowClick::Select => {
                            let modifiers = ui.ctx().input(|input| input.modifiers);
                            // A second, slow click on the cell already framed in
                            // the only selected row edits it (like a slow second
                            // click in Explorer); the first click only selects.
                            // Only that cell: a click on Status, GP or a gap
                            // edits nothing (Anwender 18.11. 13:45: GP opened the
                            // Kurztext), on the OZ neither – the OZ changes with
                            // F2 or Strg+Umschalt+O, not by accident.
                            let second_click = modifiers.is_none()
                                && self.selected == Some(id)
                                && !self.is_multi()
                                && !self.is_cell_editing();
                            let edit_cell = pointer_cell.filter(|cell| {
                                second_click
                                    && *cell != crate::cell_edit::Cell::Ordinal
                                    && self.framed_cell(&project.lv().rows()[index]) == Some(*cell)
                            });
                            let fixed_hint = match (second_click, pointer_cell, self.cells.columns)
                            {
                                (true, None, Some(columns)) => {
                                    ui.ctx().pointer_interact_pos().and_then(|pos| {
                                        crate::cell_edit::fixed_hint(
                                            &columns,
                                            &project.lv().rows()[index],
                                            pos.x,
                                        )
                                    })
                                }
                                _ => None,
                            };
                            self.click_select(project.lv(), index, modifiers);
                            self.undo.seal();
                            if let Some(cell) = pointer_cell {
                                self.cells.active = cell;
                            }
                            if let Some(cell) = edit_cell {
                                output.merge(self.begin_cell(project, index, cell, None));
                            } else if let Some(hint) = fixed_hint {
                                output.toast = Some(hint.to_string());
                            }
                        }
                        RowClick::Keep => {
                            if !self.selection_ids().contains(&id) {
                                self.selected = Some(id);
                                self.single(id);
                            }
                        }
                        RowClick::Edit => {
                            // Doppelklick: Eigenschaften im eigenen Fenster (die Zelle
                            // bearbeiten F2, Enter oder Lostippen).
                            self.selected = Some(id);
                            self.single(id);
                            self.cells.edit = None;
                            self.open_row_window(project.lv(), id);
                        }
                        RowClick::Toggle => {
                            if !self.collapsed.remove(&id) {
                                self.collapsed.insert(id);
                            }
                        }
                    }
                    ui.ctx().memory_mut(|memory| {
                        if let Some(focused) = memory.focused() {
                            memory.surrender_focus(focused);
                        }
                    });
                }
                if free_area_menu {
                    self.selected = None;
                    self.marked.clear();
                }
                if let Some(choice) = context_command {
                    // Chosen while a cell is edited (right click into the edit
                    // box): take the edit over first, or drop an invalid one.
                    if self.cells.edit.is_some() && !self.commit_cell(project, output) {
                        self.cells.edit = None;
                    }
                    output.merge(self.run_choice(choice, project, &ui.ctx().clone()));
                }

                let released = ui.ctx().input(|input| {
                    input.pointer.primary_released() || !input.pointer.primary_down()
                });
                if released {
                    // Dropped on another LV: the shell moves (or with Strg copies) the rows.
                    if let (Some(from_id), Some((to_lv, before))) = (self.drag, drop_other) {
                        self.drag = None;
                        let ids: Vec<RowId> = if self.selection_ids().contains(&from_id) {
                            lv_ops::selection_roots(project.lv(), &self.selection_ids())
                                .into_iter()
                                .map(|index| project.lv().rows()[index].id())
                                .collect()
                        } else {
                            vec![from_id]
                        };
                        let copy = ui.ctx().input(|input| input.modifiers.command);
                        output.cross_drop = Some(CrossDrop {
                            to_lv,
                            before,
                            ids,
                            copy,
                        });
                    }
                    let dragged = self
                        .drag
                        .filter(|id| self.is_multi() && self.selection_ids().contains(id));
                    if let (Some(_), Some(target)) = (dragged, drop_target) {
                        self.drag = None;
                        let roots = lv_ops::selection_roots(project.lv(), &self.selection_ids());
                        match lv_ops::contiguous_range(project.lv(), &roots) {
                            None => {
                                output.toast = Some(
                                    "Ziehen geht nur, wenn die gewählten Zeilen zusammenhängen."
                                        .into(),
                                )
                            }
                            Some(range) => {
                                let before = project.lv().clone();
                                match struct_ops::move_rows_to(project.lv_mut(), range, target) {
                                    Ok(moved) => {
                                        self.undo.record(&before, None);
                                        self.scroll_to_selected = true;
                                        output.dirty = true;
                                        output.toast = Some(format!(
                                            "{} verschoben.",
                                            crate::format::count(moved.len(), "Zeile", "Zeilen")
                                        ));
                                    }
                                    Err(message) => output.toast = Some(message),
                                }
                            }
                        }
                    }
                    if let (Some(from_id), Some(target)) = (self.drag.take(), drop_target) {
                        if let Err(message) = project.lv().ensure_editable(&[from_id]) {
                            output.toast = Some(message);
                        } else if let Some(from) = project.lv().index_of(from_id) {
                            let before = project.lv().clone();
                            if let Some(moved) = lv_ops::move_to(project.lv_mut(), from, target) {
                                self.undo.record(&before, None);
                                self.selected = Some(project.lv().rows()[moved].id());
                                output.dirty = true;
                                output.toast = Some("Verschoben.".into());
                            }
                        }
                    }
                    self.drag = None;
                }
            });
    }

    fn replace_bar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let selection = self.selection_ids();
        let Some(bar) = self.replace.as_mut() else {
            return;
        };
        let hit_rows = if bar.find.is_empty() {
            0
        } else if bar.options().only.is_some() {
            search_replace::matching_rows(project.lv(), &bar.find, &bar.options()).len()
        } else {
            // Without a selection as the reach: the hits of all LVs of the table.
            multi_lv::shown(project)
                .into_iter()
                .filter_map(|index| project.lv_at(index))
                .map(|lv| search_replace::matching_rows(lv, &bar.find, &bar.options()).len())
                .sum()
        };
        if let Some(action) = bar.ui(ui, palette, &selection, hit_rows) {
            output.merge(self.replace_action(action, project));
        }
    }

    fn search_bar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let selected = self.selected_index(project.lv());
        let Some(search) = self.search.as_mut() else {
            return;
        };
        let mut close = false;
        let mut next = false;
        let mut typed = false;
        ui.horizontal(|ui| {
            widgets::search_mark(ui, palette);
            let response = ui.add(
                egui::TextEdit::singleline(&mut search.query)
                    .id(egui::Id::unique("lv_search"))
                    .hint_text("OZ oder Kurztext suchen …")
                    .desired_width(280.0),
            );
            if std::mem::take(&mut search.focus) {
                response.request_focus();
            }
            typed = response.changed();
            let (enter, escape) = ui.input(|input| {
                (
                    input.key_pressed(egui::Key::Enter),
                    input.key_pressed(egui::Key::Escape),
                )
            });
            if response.lost_focus() && enter {
                next = true;
                response.request_focus();
            }
            if escape {
                close = true;
            }
            if ui.button("Weiter (F3)").clicked() {
                next = true;
            }
            if ui.button("Schließen").clicked() {
                close = true;
            }
            ui.label(
                RichText::new("Esc schließt")
                    .size(11.5)
                    .color(palette.text_dim),
            );
        });
        let query = search.query.clone();
        if close {
            self.search = None;
            return;
        }
        if typed && !query.trim().is_empty() {
            output.merge(self.find(
                project,
                &query,
                selected.and_then(|index| index.checked_sub(1)),
            ));
        } else if next {
            output.merge(self.find(project, &query, selected));
        }
    }

    fn dialogs(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        if self.insert_renumbering_dialog(ui, palette, project, output) {
            return;
        }
        if self.structure_dialogs(ui, palette, project, output) {
            return;
        }
        if self.cost_group_dialog(ui, palette, project, output) {
            return;
        }
        if self.conversion_dialog(ui, palette, project, output) {
            return;
        }
        if self.free_dialog(ui, palette, project, output) {
            return;
        }
        if let Some(Dialog::ConfirmDeleteMany { count }) = self.dialog {
            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 460.0, |ui| {
                widgets::title(
                    ui,
                    palette,
                    "Gewählte Zeilen löschen?",
                    &format!("{count} Zeilen samt Inhalt der Titel. Strg+Z holt sie zurück."),
                );
                widgets::danger_dialog_buttons(ui, palette, "Löschen")
            });
            if close || cancelled {
                self.dialog = None;
            } else if confirmed || enter {
                self.dialog = None;
                output.merge(self.delete_selection(project));
            }
            return;
        }
        let Some((index, count)) = self.dialog.as_ref().and_then(|dialog| match dialog {
            Dialog::ConfirmDelete { index, count } => Some((*index, *count)),
            _ => None,
        }) else {
            return;
        };
        let text = project
            .lv()
            .rows()
            .get(index)
            .map(|row| format!("{} {}", row.ordinal(), row.short_text()))
            .unwrap_or_default();
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 460.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Titel mit Inhalt löschen?",
                &format!(
                    "„{}“ und alles darunter – {}. Strg+Z holt sie zurück.",
                    text.trim(),
                    crate::format::count(count, "Zeile", "Zeilen")
                ),
            );
            widgets::danger_dialog_buttons(ui, palette, "Löschen")
        });
        if close || cancelled {
            self.dialog = None;
        } else if confirmed || enter {
            self.dialog = None;
            output.merge(self.delete(project, index));
        }
    }
}

/// Strg+Z → Undo, Strg+Y / Strg+Umschalt+Z → Redo, pressed in this frame.
fn field_history_key(ctx: &egui::Context) -> Option<Command> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command => match key {
                egui::Key::Z if modifiers.shift => Some(Command::Redo),
                egui::Key::Z => Some(Command::Undo),
                egui::Key::Y => Some(Command::Redo),
                _ => None,
            },
            _ => None,
        })
    })
}

impl LvEditor {
    /// The „Rechtschreibung prüfen“ window and what it asks for.
    fn spelling_window(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let Some(dialog) = self.spelling.as_mut() else {
            return;
        };
        let Some(speller) = crate::spell::speller(ctx) else {
            self.spelling = None;
            return;
        };
        match dialog.show(ctx, palette, &speller) {
            crate::spell_dialog::SpellOutcome::Open => {}
            crate::spell_dialog::SpellOutcome::Close => self.spelling = None,
            crate::spell_dialog::SpellOutcome::Jump(lv, row) if lv != project.active_lv() => {
                output.jump = Some(LvJump {
                    lv,
                    row: Some(row),
                    then: JumpThen::Select,
                });
            }
            crate::spell_dialog::SpellOutcome::Jump(_, row) => {
                if project.lv().index_of(row).is_some() {
                    self.selected = Some(row);
                    self.single(row);
                    self.scroll_to_selected = true;
                }
            }
            // Several LVs: the shell replaces in all of them (one undo step)
            // and hands the result back (`spelling_replaced`).
            crate::spell_dialog::SpellOutcome::Replace { word, with }
                if multi_lv::shown(project).len() > 1 =>
            {
                output.spell_replace = Some((word, with));
            }
            crate::spell_dialog::SpellOutcome::Replace { word, with } => {
                self.undo.record(project.lv(), None);
                let (replaced, skipped) =
                    proava_application::spelling::replace_word(project.lv_mut(), &word, &with);
                self.revision += 1;
                self.long_text.reset();
                output.dirty |= replaced > 0;
                let mut message = format!("„{word}“ {replaced}× durch „{with}“ ersetzt.");
                if skipped > 0 {
                    message.push_str(&format!(
                        " {skipped}× in fertigen Positionen oder STLB-Bau-Langtexten nicht geändert."
                    ));
                }
                if let Some(dialog) = self.spelling.as_mut() {
                    dialog.refresh(project, &speller);
                    dialog.message = Some(message);
                }
            }
        }
    }

    /// The shell replaced a word in all LVs: check again, show the result.
    pub fn spelling_replaced(&mut self, ctx: &egui::Context, project: &Project, message: String) {
        self.revision += 1;
        self.long_text.reset();
        if let (Some(dialog), Some(speller)) = (self.spelling.as_mut(), crate::spell::speller(ctx))
        {
            dialog.refresh(project, &speller);
            dialog.message = Some(message);
        }
    }
}

fn big_row_id(project: &Project, index: usize) -> u64 {
    project.lv().rows()[index].id().0
}

/// Textergänzungen of row `index` (positions, hints and
/// Ausführungsbeschreibungen have them).
pub(crate) fn row_complements(
    project: &Project,
    index: usize,
) -> Option<Vec<proava_domain::TextComplement>> {
    match project.lv().rows().get(index)? {
        LvRow::Position(position) => Some(position.complements().to_vec()),
        LvRow::Note(note) => Some(note.complements().to_vec()),
        LvRow::Performance(performance) => Some(performance.complements().to_vec()),
        _ => None,
    }
}

/// Textergänzungen of row `index` once block `at` of its text is inserted
/// (`removed` = false) or removed: their places move along.
pub(crate) fn complements_after_block(
    project: &Project,
    index: usize,
    at: usize,
    removed: bool,
) -> Option<Vec<proava_domain::TextComplement>> {
    let mut list = row_complements(project, index)?;
    if removed {
        proava_domain::complement_text::shift_after_remove(&mut list, at);
    } else {
        proava_domain::complement_text::shift_after_insert(&mut list, at);
    }
    Some(list)
}

/// Number of blocks of the long text of row `index`.
pub(crate) fn long_text_blocks(project: &Project, index: usize) -> usize {
    match project.lv().rows().get(index) {
        Some(LvRow::Position(position)) => position.long_text().blocks().len(),
        Some(LvRow::Note(note)) => note.text().blocks().len(),
        Some(LvRow::Performance(performance)) => performance.long_text().blocks().len(),
        _ => 0,
    }
}

/// Stores Textergänzungen of row `index` (new anchors after editing the text).
pub(crate) fn set_complements(
    project: &mut Project,
    index: usize,
    complements: Option<Vec<proava_domain::TextComplement>>,
) {
    let Some(complements) = complements else {
        return;
    };
    match project.lv_mut().row_mut(index) {
        // STLB-Bau-Text: only the values of the fields change.
        Some(LvRow::Position(position)) if position.is_text_locked() => {
            if let Ok(list) = position.locked_complement_values(&complements) {
                position.set_complements(list);
            }
        }
        Some(LvRow::Position(position)) if position.complements() != complements.as_slice() => {
            position.set_complements(complements)
        }
        Some(LvRow::Note(note)) if note.complements() != complements.as_slice() => {
            note.set_complements(complements)
        }
        Some(LvRow::Performance(performance))
            if performance.complements() != complements.as_slice() =>
        {
            performance.set_complements(complements)
        }
        _ => {}
    }
}

/// Stores the Langtext of row `index`; the text of an STLB-Bau position is
/// write-protected and stays (the last net behind the locked editors).
pub(crate) fn set_long_text(project: &mut Project, index: usize, text: proava_domain::RichText) {
    match project.lv_mut().row_mut(index) {
        Some(LvRow::Position(position)) if position.is_text_locked() => {}
        Some(LvRow::Position(position)) => position.set_long_text(text),
        Some(LvRow::Note(note)) => note.set_text(text),
        Some(LvRow::Performance(performance)) => performance.set_long_text(text),
        _ => {}
    }
}

/// What happened to a number field in this frame.
#[derive(Default, Clone, Copy)]
struct NumberEdit {
    /// The text was changed.
    changed: bool,
    /// The field was left (Enter, Tab, click elsewhere).
    left: bool,
}

impl NumberEdit {
    /// Parses the field text when it changed or was left. A valid value is
    /// returned (and the error cleared); an invalid one is only reported when
    /// the field is left, so half-typed numbers do not flash red.
    fn apply<T>(
        self,
        text: &str,
        error: &mut Option<String>,
        parse: impl Fn(&str) -> Result<T, String>,
        label: &str,
    ) -> Option<T> {
        if !self.changed && !self.left {
            return None;
        }
        match parse(text) {
            Ok(value) => {
                *error = None;
                Some(value)
            }
            Err(message) => {
                if self.left {
                    *error = Some(format!("{label}: {message}"));
                }
                None
            }
        }
    }
}

/// Number input; the caller applies valid values right away.
fn number_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    wrong: bool,
    id: (&str, RowId),
) -> NumberEdit {
    let caption = ui.label(RichText::new(label).size(12.0).color(if wrong {
        palette.error
    } else {
        palette.text_dim
    }));
    ui.add_space(3.0);
    let response = ui
        .add(
            egui::TextEdit::singleline(value)
                .id(egui::Id::unique(id))
                .desired_width(f32::INFINITY)
                .align(egui::Align2::RIGHT_CENTER)
                .font(FontId::monospace(NUMBER_SIZE))
                .margin(egui::Margin::symmetric(6, 5)),
        )
        .labelled_by(caption.id);
    select_all_on_focus(&response, value);
    if wrong {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            egui::CornerRadius::same(5),
            Stroke::new(1.5, palette.error),
            egui::StrokeKind::Outside,
        );
    }
    NumberEdit {
        changed: response.changed(),
        left: response.lost_focus(),
    }
}

/// Tab into a short field selects its content, so typing replaces it.
fn select_all_on_focus(response: &egui::Response, text: &str) {
    if response.gained_focus() {
        let mut state =
            egui::text_edit::TextEditState::load(&response.ctx, response.id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(text.chars().count()),
            )));
        state.store(&response.ctx, response.id);
    }
}

/// Segmented choice offen | in Bearbeitung | fertig; returns a new choice.
fn status_selector(
    ui: &mut egui::Ui,
    palette: &Palette,
    current: EditStatus,
    id: RowId,
) -> Option<EditStatus> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Status").size(12.0).color(palette.text_dim));
        ui.add_space(4.0);
        for status in EditStatus::ALL {
            let active = status == current;
            let color = match status {
                EditStatus::Open => palette.text,
                EditStatus::InProgress => palette.in_progress,
                EditStatus::Done => palette.done,
            };
            let text = RichText::new(status.label()).size(12.5).color(if active {
                color
            } else {
                palette.text_dim
            });
            let response = ui
                .push_id(("status", id, status.label()), |ui| {
                    ui.selectable_label(active, text)
                })
                .inner
                .on_hover_text(match status {
                    EditStatus::Done => {
                        "Strg+Umschalt+F – fertige Positionen sind schreibgeschützt"
                    }
                    EditStatus::InProgress => "Strg+Umschalt+E",
                    EditStatus::Open => "Strg+Umschalt+F bei einer fertigen Position",
                });
            if response.clicked() && !active {
                chosen = Some(status);
            }
        }
    });
    chosen
}

/// [`detail_heading`] with the field „Zeilenart“ on the right (positions,
/// hints, Ausführungsbeschreibungen): another choice asks for the conversion.
fn detail_heading_with_kind(
    ui: &mut egui::Ui,
    palette: &Palette,
    kind: &str,
    ordinal: &str,
    marks: &[&str],
    row: (proava_domain::RowKind, RowId, bool),
) -> Option<proava_domain::RowKind> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        heading_parts(ui, palette, kind, ordinal, marks);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            chosen = convert::kind_choice(ui, palette, row.0, row.1, row.2);
        });
    });
    ui.add_space(10.0);
    chosen
}

fn detail_heading(ui: &mut egui::Ui, palette: &Palette, kind: &str, ordinal: &str, marks: &[&str]) {
    ui.horizontal(|ui| heading_parts(ui, palette, kind, ordinal, marks));
    ui.add_space(10.0);
}

/// Kind, number and marks of the edit panel's heading line.
fn heading_parts(ui: &mut egui::Ui, palette: &Palette, kind: &str, ordinal: &str, marks: &[&str]) {
    ui.label(RichText::new(kind).size(12.5).color(palette.text_dim));
    ui.add_space(6.0);
    ui.label(
        RichText::new(ordinal)
            .size(15.0)
            .strong()
            .monospace()
            .color(palette.text),
    );
    for mark in marks {
        ui.label(
            RichText::new(*mark)
                .size(11.5)
                .color(palette.accent)
                .background_color(palette.accent_soft),
        );
    }
}

/// The value all positions share, `None` when they differ.
fn common(positions: &[&Position], value: impl Fn(&Position) -> String) -> Option<String> {
    let first = value(positions.first()?);
    positions
        .iter()
        .all(|position| value(position) == first)
        .then_some(first)
}

fn kind_label(kind: PositionKind) -> String {
    match kind {
        PositionKind::Normal => "Normalposition".into(),
        PositionKind::Provisional { with_total: false } => "Bedarfsposition ohne GB".into(),
        PositionKind::Provisional { with_total: true } => "Bedarfsposition mit GB".into(),
        PositionKind::Base { .. } => "Grundposition (Wahlgruppe)".into(),
        PositionKind::Alternative { .. } => "Wahlposition".into(),
        PositionKind::Supplement => "Zulageposition".into(),
    }
}

fn kind_group(kind: PositionKind) -> u32 {
    match kind {
        PositionKind::Base { group } | PositionKind::Alternative { group, .. } => group,
        _ => 1,
    }
}

/// Indent depth of every row: titles by level, positions and hints one deeper
/// than the title they belong to.
fn depths(lv: &Lv) -> Vec<usize> {
    let mut current = 0;
    lv.rows()
        .iter()
        .map(|row| match row {
            LvRow::Group(group) => {
                current = group.level() + 1;
                group.level()
            }
            _ => current,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowClick {
    Select,
    /// Right click: keep a selection that contains the row.
    Keep,
    Edit,
    Toggle,
}

/// What drawing the rows of one LV needs (active or not).
struct LvPaint<'a> {
    lv: &'a Lv,
    depths: &'a [usize],
    totals: &'a [proava_domain::GroupTotal],
    /// Net total of the LV: the base of the share of the title sums.
    net_total: Money,
    /// Show that share in the title rows.
    show_share: bool,
    /// Zuschläge with Rechengrundlage and Aufteilungsblöcke (planner).
    surcharges: &'a proava_domain::SurchargeComputation,
    collapsed: &'a HashSet<RowId>,
    hits: &'a HashSet<RowId>,
}

impl LvPaint<'_> {
    fn row_info(
        &self,
        index: usize,
        show_gaps: bool,
        mark_empty: bool,
        widths: &ColumnWidths,
        selected: bool,
        marked: bool,
    ) -> RowInfo {
        let lv = self.lv;
        let row = &lv.rows()[index];
        let collapsed = self.collapsed.contains(&row.id());
        let total = self
            .totals
            .iter()
            .find(|total| total.group_row == index)
            .map(|total| total.total);
        RowInfo {
            depth: self.depths[index],
            selected,
            marked,
            collapsed,
            hidden: if collapsed {
                lv.rows()[lv.block_range(index)]
                    .iter()
                    .filter(|row| row.is_position())
                    .count()
            } else {
                0
            },
            total,
            share: total
                .filter(|_| self.show_share)
                .and_then(|total| title_share(total, self.net_total)),
            surcharge: self.surcharges.values.get(&row.id()).copied(),
            basis: self.surcharges.bases.contains(&row.id()),
            // Under the last line of a block of Aufteilungszeilen: its sum.
            split_sum: self
                .surcharges
                .blocks
                .iter()
                .find(|block| block.rows.last() == Some(&row.id()))
                .map(|block| {
                    let percent = proava_domain::surcharge::percent_text(block.percent);
                    if block.is_complete() {
                        (format!("Σ {percent}"), true)
                    } else {
                        let open = proava_domain::Decimal::ONE_HUNDRED - block.percent;
                        (
                            format!(
                                "Σ {percent} – {} {}",
                                proava_domain::surcharge::percent_text(open.abs()),
                                if open.is_sign_negative() {
                                    "zu viel"
                                } else {
                                    "fehlen"
                                }
                            ),
                            false,
                        )
                    }
                }),
            marks: {
                let marks = RowMarks::of(lv, index, show_gaps);
                RowMarks {
                    hit: self.hits.contains(&row.id()),
                    // MERGE(projekt): Extras ▸ Einstellungen ▸ „Positionen ohne Menge markieren“.
                    missing_quantity: marks.missing_quantity && mark_empty,
                    ..marks
                }
            },
            badges: {
                let mut badges = row
                    .as_position()
                    .map(|position| struct_ops::row_badges(lv, position))
                    .unwrap_or_default();
                if self.surcharges.bases.contains(&row.id()) {
                    // Its GP is in the Aufteilungszeilen below.
                    badges.push("Rechengrundlage".to_string());
                }
                badges
            },
            cost_group: if widths.shown(Column::CostGroup) {
                TitleValue::of(lv, index, Column::CostGroup)
            } else {
                None
            },
            work_category: if widths.shown(Column::WorkCategory) {
                TitleValue::of(lv, index, Column::WorkCategory)
            } else {
                None
            },
            price_hint: row.as_position().and_then(|position| {
                position.price_check().map(|check| {
                    format!(
                        "{} – errechneter GP gilt (Kontextmenü: GP-Abweichung bestätigen)",
                        check.text(lv.currency())
                    )
                })
            }),
        }
    }
}

struct RowInfo {
    depth: usize,
    selected: bool,
    marked: bool,
    collapsed: bool,
    hidden: usize,
    total: Option<Money>,
    /// Title rows with the switch on: share of the LV net total („41,0 %“).
    share: Option<String>,
    /// Zuschlagsposition: Bezugssumme and amount (shown instead of Menge/GP).
    surcharge: Option<proava_domain::SurchargeValue>,
    /// Rechengrundlage of Aufteilungszeilen: GP in brackets, counts nothing.
    basis: bool,
    /// Last line of a block of Aufteilungszeilen: „Σ 100 %“ (true) or the
    /// deviation (false).
    split_sum: Option<(String, bool)>,
    marks: RowMarks,
    /// Further marks of a position (A0003, Wdh. 0010, TE …).
    badges: Vec<String>,
    /// Column „KG“: the cost group that applies (None also while the
    /// column is hidden).
    cost_group: Option<TitleValue>,
    /// Column „LB“: the Leistungsbereich that applies.
    work_category: Option<TitleValue>,
    /// Tooltip of a position with an open GP-Prüfung („GP laut Datei …“).
    price_hint: Option<String>,
}

/// Value of the columns „KG“ and „LB“: the row's own, or one a title passes
/// down (shown grey and italic, the tooltip names the title).
struct TitleValue {
    code: String,
    /// OZ of the title it comes from; None = the row's own value.
    from_title: Option<String>,
}

impl TitleValue {
    fn of(lv: &Lv, index: usize, column: Column) -> Option<Self> {
        let (effective, source) = match column {
            Column::CostGroup => (lv.effective_cost_group(index), lv.cost_group_source(index)),
            Column::WorkCategory => (
                lv.effective_work_category(index),
                lv.work_category_source(index),
            ),
            _ => return None,
        };
        let (code, inherited) = effective?;
        let from_title = inherited.then(|| {
            source
                .map(|title| lv.rows()[title].ordinal().to_string())
                .unwrap_or_default()
        });
        Some(Self { code, from_title })
    }

    /// Tooltip: „KG 481 Automationseinrichtungen – vom Titel 01.481“.
    fn tip(&self, column: Column) -> String {
        let name = match column {
            Column::CostGroup => {
                let label = proava_domain::cost_group_label(&self.code);
                if label.is_empty() {
                    format!("KG {}", self.code)
                } else {
                    format!("KG {} {label}", self.code)
                }
            }
            _ => format!("LB {}", proava_domain::work_category_text(&self.code)),
        };
        match &self.from_title {
            Some(title) => format!("{name} – vom Titel {title}"),
            None => name,
        }
    }
}

fn header_row(painter: &egui::Painter, rect: Rect, palette: &Palette, widths: &ColumnWidths) {
    let columns = widths.layout(rect.left(), rect.right());
    let y = rect.center().y;
    let font = FontId::proportional(12.0);
    let color = palette.text_dim;
    let label = |x: f32, align: Align2, text: &str| {
        widgets::bold_text(painter, egui::pos2(x, y), align, text, font.clone(), color);
    };
    label(columns.left, Align2::LEFT_CENTER, strings::WS_COL_OZ);
    label(
        columns.status_x,
        Align2::LEFT_CENTER,
        strings::WS_COL_STATUS,
    );
    label(columns.text_x, Align2::LEFT_CENTER, strings::WS_COL_TEXT);
    if columns.show_kg {
        label(
            columns.kg_left + 4.0,
            Align2::LEFT_CENTER,
            Column::CostGroup.title(),
        );
    }
    if columns.show_lb {
        label(
            columns.lb_left + 4.0,
            Align2::LEFT_CENTER,
            Column::WorkCategory.title(),
        );
    }
    label(columns.qty_right, Align2::RIGHT_CENTER, strings::WS_COL_QTY);
    if columns.show_unit {
        label(columns.unit_x, Align2::LEFT_CENTER, strings::WS_COL_UNIT);
    }
    if columns.show_ep {
        label(columns.ep_right, Align2::RIGHT_CENTER, strings::WS_COL_EP);
    }
    if columns.show_gp {
        label(columns.gp_right, Align2::RIGHT_CENTER, strings::WS_COL_GP);
    }
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom() - 0.5),
            egui::pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(1.0, palette.line),
    );
}

fn lv_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    row: &LvRow,
    info: &RowInfo,
    widths: &ColumnWidths,
) -> (egui::Response, Option<RowClick>) {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click_and_drag());
    // Name for screen readers and UI tests: OZ and Kurztext as shown, the
    // status as in the column „Status“.
    let status = row
        .as_position()
        .and_then(|position| status_view::position_status_label(position, palette))
        .map(|(text, _)| format!(" – {text}"))
        .unwrap_or_default();
    let name = match row {
        LvRow::Spacer(_) => "Abstandshalter".to_string(),
        LvRow::Note(note) if note.is_blank() => "Hinweis ohne Text – leer".to_string(),
        _ => format!(
            "{} {}{status}",
            row.ordinal(),
            crate::format::one_line(&row.short_text())
        ),
    };
    let name = match &info.share {
        Some(share) => format!("{name} – Anteil {share}"),
        None => name,
    };
    let selected = info.selected;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Row, true, selected, name.trim()));
    let painter = ui.painter_at(rect);
    let columns = widths.layout(rect.left(), rect.right());
    let middle = rect.center().y;
    let indent = info.depth as f32 * INDENT;
    // Every cell stays on one line and ends with „…“ when its column is too
    // narrow (Anwender 18.11.); what was cut goes into the tooltip.
    let mut cut_parts: Vec<String> = Vec::new();
    let qty_left = columns.qty_left;

    // Title rows (all levels, also folded) yellow over the whole row
    // (Anwender 18.11.); the selection and the marks stay on top of it.
    let title_fill = row
        .as_group()
        .and_then(|group| palette.title_fill(group.level(), response.hovered()));
    let mark_fill = info.marks.fill(palette);
    let background = if info.selected || info.marked {
        palette.selected
    } else if let Some(fill) = mark_fill {
        fill
    } else if let Some(fill) = title_fill {
        fill
    } else if response.hovered() {
        palette.row_hover
    } else {
        palette.panel
    };
    painter.rect_filled(rect, egui::CornerRadius::ZERO, background);
    // The text colours of the title rows (dark: yellow OZ and title text),
    // on every background the row can have – also selected or hovered.
    let title_palette;
    let palette = match row.as_group() {
        Some(group) if palette.title_rows.is_some() => {
            title_palette = palette.on_title_row(group.level());
            &title_palette
        }
        _ => palette,
    };
    if info.selected {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            egui::CornerRadius::ZERO,
            palette.accent,
        );
    }
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom() - 0.5),
            egui::pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(
            1.0,
            if row.is_group() {
                palette.line
            } else {
                palette.background
            },
        ),
    );

    status_view::paint_overlay(
        &painter,
        rect,
        &info.marks,
        info.selected || info.marked,
        palette,
    );
    // Columns „KG“ and „LB“: own values in black, inherited ones grey and
    // italic (the tooltip names the title).
    let mut cell_tips: Vec<(f32, f32, String)> = Vec::new();
    for (column, shown, left, right, value) in [
        (
            Column::CostGroup,
            columns.show_kg,
            columns.kg_left,
            columns.kg_right(),
            &info.cost_group,
        ),
        (
            Column::WorkCategory,
            columns.show_lb,
            columns.lb_left,
            columns.lb_right(),
            &info.work_category,
        ),
    ] {
        let Some(value) = value.as_ref().filter(|_| shown) else {
            continue;
        };
        let inherited = value.from_title.is_some();
        widgets::cell_text(
            &painter,
            egui::pos2(left + 4.0, middle),
            Align2::LEFT_CENTER,
            &value.code,
            if inherited {
                crate::fonts::italic(12.5)
            } else {
                FontId::proportional(12.5)
            },
            if inherited {
                palette.text_dim
            } else {
                palette.text
            },
            right - left - 8.0,
        );
        cell_tips.push((left, right, value.tip(column)));
    }
    let mut chevron_rect = None;
    let mut full_text: Option<String> = None;
    match row {
        LvRow::Group(group) => {
            let top = group.level() == 0;
            let size = if top { 14.5 } else { 13.5 };
            let (ordinal, cut) = widgets::fit_line(
                &painter,
                group.ordinal(),
                &crate::fonts::bold(12.5),
                columns.text_x - columns.left - 6.0,
            );
            if cut {
                cut_parts.push(format!("OZ {}", group.ordinal()));
            }
            widgets::bold_text(
                &painter,
                egui::pos2(columns.left, middle),
                Align2::LEFT_CENTER,
                &ordinal,
                FontId::monospace(12.5),
                palette.group_text,
            );
            let chevron = Rect::from_center_size(
                egui::pos2(columns.text_x + indent + 7.0, middle),
                Vec2::splat(14.0),
            );
            crate::icons::disclosure(&painter, chevron.center(), info.collapsed, palette.text_dim);
            chevron_rect = Some(chevron.expand(4.0));
            let text_left = columns.text_x + indent + 20.0;
            let right_limit = if columns.show_gp {
                columns.ep_right - 8.0
            } else {
                rect.right() - 12.0
            };
            let suffix = if let Some(points) = group.points() {
                // Datenpunkt-Abschnitt (GU-Fall): no positions, a plant list.
                format!(
                    "  · nach Datenpunkten ({})",
                    crate::format::count(points.result().points as usize, "DP", "DP")
                )
            } else if info.collapsed && info.hidden > 0 {
                format!("  ({} Pos.)", info.hidden)
            } else {
                String::new()
            };
            let shown = if group.short_text().is_empty() {
                "Neuer Titel"
            } else {
                group.short_text()
            };
            let progress = info.marks.title_progress().map(|(text, complete)| {
                painter.layout_no_wrap(
                    text,
                    FontId::proportional(12.0),
                    if complete {
                        palette.done
                    } else {
                        palette.text_dim
                    },
                )
            });
            let right_limit = match &progress {
                Some(galley) => {
                    let width = galley.size().x;
                    painter.galley(
                        egui::pos2(right_limit - width, middle - galley.size().y / 2.0),
                        galley.clone(),
                        palette.text_dim,
                    );
                    right_limit - width - 12.0
                }
                None => right_limit,
            };
            // The title text stops before the columns „KG“ and „LB“.
            let right_limit = if columns.show_kg || columns.show_lb {
                right_limit.min(columns.text_right - 8.0)
            } else {
                right_limit
            };
            let suffix_width = painter
                .layout_no_wrap(suffix.clone(), FontId::proportional(12.0), palette.text_dim)
                .size()
                .x;
            let (text, cut) = fit(
                &painter,
                shown,
                &crate::fonts::bold(size),
                right_limit - text_left - suffix_width,
            );
            if cut {
                full_text = Some(group.short_text().to_string());
            }
            let text_rect = widgets::bold_text(
                &painter,
                egui::pos2(text_left, middle),
                Align2::LEFT_CENTER,
                &text,
                FontId::proportional(size),
                if group.short_text().is_empty() {
                    palette.text_dim
                } else {
                    palette.group_text
                },
            );
            if !suffix.is_empty() {
                painter.text(
                    egui::pos2(text_rect.right() + 2.0, middle),
                    Align2::LEFT_CENTER,
                    suffix,
                    FontId::proportional(12.0),
                    palette.text_dim,
                );
            }
            if let (Some(total), true) = (info.total, columns.show_gp) {
                let (shown, cut) = widgets::fit_line(
                    &painter,
                    &total.plain(),
                    &crate::fonts::bold(NUMBER_SIZE),
                    columns.gp_right - columns.ep_right - 6.0,
                );
                if cut {
                    cut_parts.push(format!("GP: {}", total.plain()));
                }
                widgets::bold_text(
                    &painter,
                    egui::pos2(columns.gp_right, middle),
                    Align2::RIGHT_CENTER,
                    &shown,
                    FontId::monospace(NUMBER_SIZE),
                    palette.text,
                );
                // Pauschalsumme of the title (GAEB TotalLSUM) and the share
                // of the LV total (switch in Ansicht): in the EP column.
                let lump = row
                    .as_group()
                    .is_some_and(|group| group.lump_sum_total().is_some());
                let ep_text = match (lump, &info.share) {
                    (true, Some(share)) => Some((format!("pauschal · {share}"), false)),
                    (true, None) => Some(("pauschal".to_string(), false)),
                    (false, Some(share)) => Some((share.clone(), true)),
                    (false, None) => None,
                };
                if let Some((text, number)) = ep_text {
                    widgets::cell_text(
                        &painter,
                        egui::pos2(columns.ep_right, middle),
                        Align2::RIGHT_CENTER,
                        &text,
                        if number {
                            FontId::monospace(NUMBER_SIZE)
                        } else {
                            FontId::proportional(11.5)
                        },
                        palette.text_dim,
                        columns.ep_right - columns.ep_left - 6.0,
                    );
                }
            }
        }
        LvRow::Performance(performance) => {
            // „A0003“ in the OZ column, like the printout.
            let (_, cut) = widgets::cell_text(
                &painter,
                egui::pos2(columns.left, middle),
                Align2::LEFT_CENTER,
                &performance.mark(),
                FontId::monospace(12.5),
                palette.text_dim,
                columns.text_x - columns.left - 6.0,
            );
            if cut {
                cut_parts.push(performance.mark());
            }
            let heading = performance.heading();
            let text_left = columns.text_x + indent;
            let (text, cut) = fit(
                &painter,
                &heading,
                &FontId::proportional(TEXT_SIZE),
                rect.right() - 12.0 - text_left,
            );
            if cut {
                full_text = Some(heading.clone());
            }
            painter.text(
                egui::pos2(text_left, middle),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(TEXT_SIZE),
                palette.text,
            );
        }
        LvRow::Spacer(_) => {
            // Abstandshalter: an empty line for editing only; hover and
            // selection name it, the tooltip says that it is not printed.
            if info.selected || info.marked || response.hovered() {
                painter.text(
                    egui::pos2(columns.text_x + indent, middle),
                    Align2::LEFT_CENTER,
                    "Abstandshalter – nur für die Bearbeitung",
                    crate::fonts::italic(TEXT_SIZE),
                    palette.text_dim,
                );
            }
            full_text = Some(SPACER_HINT.to_string());
        }
        LvRow::Note(note) => {
            // A hint without text gets a warning like a position without
            // quantity: „leer“ in the column „Status“.
            let blank = note.is_blank();
            if blank {
                status_text(&painter, &columns, middle, "leer", palette.warn);
            } else {
                status_text(&painter, &columns, middle, "Hinweis", palette.text_dim);
            }
            let plain = note_line(note);
            let shown = if blank {
                "Hinweis ohne Text"
            } else if plain.trim().is_empty() {
                "Hinweistext"
            } else {
                plain.as_str()
            };
            let text_left = columns.text_x + indent;
            let (text, cut) = fit(
                &painter,
                shown,
                &crate::fonts::italic(TEXT_SIZE),
                rect.right() - 12.0 - text_left,
            );
            if cut {
                full_text = Some(plain.clone());
            }
            painter.text(
                egui::pos2(text_left, middle),
                Align2::LEFT_CENTER,
                text,
                crate::fonts::italic(TEXT_SIZE),
                palette.text_dim,
            );
        }
        LvRow::Position(position) => {
            let status = status_view::position_status_label(position, palette);
            // The OZ may use the column „Status“ when it is empty.
            let oz_right = if status.is_some() {
                columns.status_x
            } else {
                columns.text_x
            };
            let (_, cut) = widgets::cell_text(
                &painter,
                egui::pos2(columns.left, middle),
                Align2::LEFT_CENTER,
                position.ordinal(),
                FontId::monospace(12.5),
                palette.text_dim,
                oz_right - columns.left - 6.0,
            );
            if cut {
                cut_parts.push(format!("OZ {}", position.ordinal()));
            }
            if let Some((text, color)) = status {
                status_text(&painter, &columns, middle, text, color);
            }
            let mut x = columns.text_x + indent;
            let shown = if position.short_text().is_empty() {
                "Neue Position"
            } else {
                position.short_text()
            };
            let marks = position.marks();
            let mut badge_width = 0.0;
            if position.long_text().has_images() {
                badge_width += IMAGE_BADGE.len() as f32 * 7.0 + 12.0;
            }
            badge_width += marks
                .iter()
                .map(|mark| mark.len() as f32 * 7.0 + 12.0)
                .sum::<f32>();
            badge_width += info
                .badges
                .iter()
                .chain(info.split_sum.iter().map(|(text, _)| text))
                .map(|mark| mark.chars().count() as f32 * 7.0 + 12.0)
                .sum::<f32>();
            let (text, cut) = fit(
                &painter,
                shown,
                &FontId::proportional(TEXT_SIZE),
                (columns.text_right - x - badge_width - 8.0).max(30.0),
            );
            if cut {
                full_text = Some(position.short_text().to_string());
            }
            let text_rect = painter.text(
                egui::pos2(x, middle),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(TEXT_SIZE),
                if position.short_text().is_empty() {
                    palette.text_dim
                } else {
                    palette.text
                },
            );
            x = text_rect.right() + 6.0;
            for mark in marks {
                let (fill, color) = if mark == "STLB" || mark == "STLB-Bau*" {
                    (palette.warn.gamma_multiply(0.18), palette.warn)
                } else {
                    (palette.accent_soft, palette.accent)
                };
                let width = widgets::badge(&painter, egui::pos2(x, middle), mark, fill, color);
                // Over „STLB-Bau“: the key with its version.
                if mark.starts_with("STLB-Bau") {
                    if let Some(line) = proava_domain::stlb::key_line(position) {
                        let protection = if position.stlb_text_changed() {
                            proava_domain::stlb::STLB_CHANGED
                        } else {
                            stlb_lock::LOCKED_TITLE
                        };
                        cell_tips.push((x, x + width, format!("{line}\n{protection}")));
                    }
                }
                x += width + 4.0;
            }
            for mark in &info.badges {
                if proava_domain::is_performance_mark(mark) {
                    // A0001: readable on selected rows too.
                    x += widgets::link_badge(&painter, egui::pos2(x, middle), mark, palette) + 4.0;
                    continue;
                }
                let (fill, color) = (palette.panel_alt, palette.text_dim);
                x += widgets::badge(&painter, egui::pos2(x, middle), mark, fill, color) + 4.0;
            }
            if let Some((text, complete)) = &info.split_sum {
                // Sum of the block of Aufteilungszeilen: 100 % or the gap.
                let (fill, color) = if *complete {
                    (palette.done.gamma_multiply(0.15), palette.done)
                } else {
                    (palette.error.gamma_multiply(0.15), palette.error)
                };
                x += widgets::badge(&painter, egui::pos2(x, middle), text, fill, color) + 4.0;
            }
            if position.long_text().has_images() {
                widgets::badge(
                    &painter,
                    egui::pos2(x, middle),
                    IMAGE_BADGE,
                    palette.panel_alt,
                    palette.text_dim,
                );
            }
            let mut number = |x: f32, width: f32, label: &str, text: String, color: Color32| {
                let (_, cut) = widgets::cell_text(
                    &painter,
                    egui::pos2(x, middle),
                    Align2::RIGHT_CENTER,
                    &text,
                    FontId::monospace(NUMBER_SIZE),
                    color,
                    width,
                );
                if cut {
                    cut_parts.push(format!("{label}: {text}"));
                }
            };
            if let Some(value) = info.surcharge.filter(|_| position.is_surcharge()) {
                // Zuschlagsposition: Bezugssumme where the quantity stands
                // (grey), the percentage as EP.
                number(
                    columns.qty_right,
                    columns.qty_right - qty_left - 4.0,
                    "Bezugssumme",
                    value.base.plain(),
                    palette.text_dim,
                );
            } else if !position.is_stlb_placeholder() || !position.quantity().is_zero() {
                number(
                    columns.qty_right,
                    columns.qty_right - qty_left - 4.0,
                    "Menge",
                    numbers::quantity_text(position.quantity()),
                    palette.text,
                );
            }
            if columns.show_ep {
                number(
                    columns.ep_right,
                    columns.ep_right - columns.ep_left - 6.0,
                    "EP",
                    if position.is_surcharge() {
                        position
                            .unit_price()
                            .map(proava_domain::surcharge::percent_text)
                            .unwrap_or_default()
                    } else {
                        numbers::price_text(position.unit_price())
                    },
                    palette.text,
                );
            }
            if columns.show_gp && position.unit_price().is_some() {
                let total = match info.surcharge {
                    Some(value) => value.amount.plain(),
                    None => position.total_price().plain(),
                };
                let width = columns.gp_right - columns.ep_right - 6.0;
                if position.kind().counts_in_total() && !info.basis {
                    number(columns.gp_right, width, "GP", total, palette.text);
                } else {
                    number(
                        columns.gp_right,
                        width,
                        "GP",
                        format!("({total})"),
                        palette.text_dim,
                    );
                }
            }
            if columns.show_unit {
                let (_, cut) = widgets::cell_text(
                    &painter,
                    egui::pos2(columns.unit_x, middle),
                    Align2::LEFT_CENTER,
                    position.unit(),
                    FontId::proportional(12.5),
                    palette.text,
                    columns.ep_left - columns.unit_x - 4.0,
                );
                if cut {
                    cut_parts.push(format!("Einheit: {}", position.unit()));
                }
            }
        }
    }

    let mut click = None;
    if response.double_clicked() {
        click = Some(RowClick::Edit);
    } else if response.clicked() {
        let on_chevron = chevron_rect
            .zip(response.interact_pointer_pos())
            .is_some_and(|(chevron, pos)| chevron.contains(pos));
        click = Some(if on_chevron {
            RowClick::Toggle
        } else {
            RowClick::Select
        });
    }
    let full_text = match (full_text, &info.price_hint) {
        (Some(text), Some(hint)) => Some(format!("{text}\n{hint}")),
        (None, Some(hint)) => Some(hint.clone()),
        (text, None) => text,
    };
    // Cut OZ, Einheit or numbers first, then the Kurztext.
    let full_text = if cut_parts.is_empty() {
        full_text
    } else {
        cut_parts.extend(full_text);
        Some(cut_parts.join("\n"))
    };
    // Over „KG“ or „LB“ the tooltip tells the value and where it comes from
    // – at the pointer, not at the left end of the long row.
    let cell_tip = response
        .hover_pos()
        .and_then(|pos| {
            cell_tips
                .iter()
                .find(|(left, right, _)| (*left..=*right).contains(&pos.x))
        })
        .map(|(_, _, tip)| tip.clone());
    let response = match (cell_tip, full_text) {
        (Some(tip), _) => response.on_hover_text_at_pointer(tip),
        (None, Some(text)) => response.on_hover_text(text),
        (None, None) => response,
    };
    (response, click)
}

/// Mark of a position whose Langtext holds pictures.
const IMAGE_BADGE: &str = "Bild";

/// Tooltip of an Abstandshalter row (Anwender 18.11.: only for editing).
const SPACER_HINT: &str =
    "Abstandshalter: Leerzeile nur für die Bearbeitung – erscheint weder im Druck noch in GAEB oder CSV.";

/// Text in the column „Status“ (small, left aligned).
fn status_text(
    painter: &egui::Painter,
    columns: &crate::columns::Columns,
    middle: f32,
    text: &str,
    color: Color32,
) {
    painter.text(
        egui::pos2(columns.status_x, middle),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        color,
    );
}

/// Cuts `text` with "…" so that it fits `width` in `font` (measured, not estimated).
/// Line breaks of the text (GAEB-Kurztexte) become spaces: one line per row.
/// Share of a title sum in the LV net total with one decimal („41,0 %“);
/// None without a net total.
fn title_share(total: Money, net: Money) -> Option<String> {
    if net.cents() == 0 {
        return None;
    }
    let share = proava_domain::Decimal::from(total.cents()) * proava_domain::Decimal::ONE_HUNDRED
        / proava_domain::Decimal::from(net.cents());
    Some(format!("{} %", proava_domain::format_decimal_de(share, 1)))
}

fn fit(painter: &egui::Painter, text: &str, font: &FontId, width: f32) -> (String, bool) {
    widgets::fit_line(painter, text, font, width)
}

fn footer(painter: &egui::Painter, rect: Rect, palette: &Palette, lv: &Lv) {
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.top() + 0.5),
            egui::pos2(rect.right(), rect.top() + 0.5),
        ],
        Stroke::new(1.0, palette.line),
    );
    let net = lv.net_total();
    let vat = lv.vat(net);
    let y = rect.center().y + 2.0;
    let vat_label = numbers::quantity_text(lv.vat_percent());
    let totals = widgets::bold_text(
        painter,
        egui::pos2(rect.right() - 12.0, y),
        Align2::RIGHT_CENTER,
        &{
            // In the Währung of the LV (CSV import in CHF/USD).
            let amount = |money| proava_domain::price_check::amount(money, lv.currency());
            // A Pauschalsumme (TotalLSUM) is the LV total; the sum of the
            // positions stays visible in front of it.
            let lump = lv
                .lump_sum_total()
                .map(|_| format!("Positionen {}  ·  Pauschal ", amount(lv.positions_total())))
                .unwrap_or_default();
            format!(
                "{lump}Netto {}  ·  USt {vat_label} % {}  ·  Brutto {}",
                amount(net),
                amount(vat),
                amount(net + vat)
            )
        },
        FontId::proportional(13.0),
        palette.text,
    );
    let counts = format!(
        "{} · {} Titel · {}",
        crate::format::count(lv.position_count(), "Position", "Positionen"),
        lv.group_count(),
        crate::format::count(lv.note_count(), "Hinweis", "Hinweise")
    );
    let (progress, missing) = status_view::footer_status(lv);
    let mut pieces = vec![(counts, palette.text_dim)];
    if let Some(progress) = progress {
        pieces.push((format!("  ·  {progress}"), palette.done));
    }
    if let Some(missing) = missing {
        pieces.push((format!("  ·  {missing}"), palette.error));
    }
    let (checks, _) = proava_domain::price_check::open_checks(lv);
    if checks > 0 {
        pieces.push((format!("  ·  {checks} GP prüfen"), palette.warn));
    }
    let mut x = rect.left() + 12.0;
    for (text, color) in pieces {
        let galley = painter.layout_no_wrap(text, FontId::proportional(12.5), color);
        if x + galley.size().x + 16.0 >= totals.left() {
            break;
        }
        let width = galley.size().x;
        painter.galley(egui::pos2(x, y - galley.size().y / 2.0), galley, color);
        x += width;
    }
}

fn empty_state(ui: &mut egui::Ui, palette: &Palette, rect: Rect) {
    let painter = ui.painter();
    painter.text(
        rect.center_top() + Vec2::new(0.0, 60.0),
        Align2::CENTER_CENTER,
        strings::WS_EMPTY_TITLE,
        FontId::proportional(16.0),
        palette.text,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 88.0),
        Align2::CENTER_CENTER,
        "F12: Titel  ·  F9: Position  ·  F10: Hinweis  ·  F11: Ausführungsbeschreibung",
        FontId::proportional(13.0),
        palette.text_dim,
    );
}

/// What the LV table shows for a hint: its heading (GAEB-Kurztext), else
/// its text on one line.
fn note_line(note: &proava_domain::Note) -> String {
    let heading = note.heading().trim();
    if heading.is_empty() {
        note.text().to_plain().replace('\n', " · ")
    } else {
        heading.to_string()
    }
}

#[cfg(test)]
mod note_line_tests {
    use proava_domain::{Note, RichText};

    use super::note_line;

    #[test]
    fn a_hint_shows_its_heading_else_its_text() {
        let mut only_heading = Note::rich(RichText::default());
        only_heading.set_heading("Hinweis: nachfolgende Leistungsbaugruppe");
        assert_eq!(
            note_line(&only_heading),
            "Hinweis: nachfolgende Leistungsbaugruppe"
        );
        let mut both = Note::new("Erster Absatz\nZweiter");
        both.set_heading("Vorbemerkung");
        assert_eq!(note_line(&both), "Vorbemerkung");
        assert_eq!(
            note_line(&Note::new("Erster Absatz\nZweiter")),
            "Erster Absatz · Zweiter"
        );
        assert_eq!(note_line(&Note::rich(RichText::default())), "");
    }
}

/// „Rückgängig: 01.01.0010 – Kurztext, Menge“, „Wiederholt: 3 Zeilen“ … and
/// the row to select. `before` is the LV before the step, `after` the one
/// it brings back.
fn undo_message(verb: &str, before: &Lv, after: &Lv) -> (String, Option<RowId>) {
    use proava_domain::lv_diff::{self, Aspect, ChangeKind};
    // Two states of the same LV: pair rows by id (lv_diff::compare matches
    // by OZ and text similarity and took 30–40 ms per Strg+Z at 2000
    // positions – Abnahme qa3, 18.11.).
    let diff = lv_diff::compare_states(before, after);
    let first = diff.changes.first();
    let row = diff
        .changes
        .iter()
        .find_map(|change| change.new_id)
        .or_else(|| first.and_then(|change| change.old_id));
    let message = match diff.changes.as_slice() {
        [] => format!("{verb}."),
        [change] => {
            let kind_change = change
                .details
                .iter()
                .find(|detail| detail.aspect == Aspect::Kind);
            let what = match change.kind {
                ChangeKind::Added => format!("{} wieder da", change.row_kind.label()),
                ChangeKind::Removed => format!("{} entfernt", change.row_kind.label()),
                ChangeKind::Moved => format!("{} verschoben", change.row_kind.label()),
                // Umwandeln: „Hinweis → Abstandshalter“.
                ChangeKind::Changed if kind_change.is_some() => {
                    let detail = kind_change.expect("checked");
                    format!("{} → {}", detail.old, detail.new)
                }
                ChangeKind::Changed => {
                    let aspects: Vec<&str> = change
                        .details
                        .iter()
                        .map(|detail| detail.aspect.label())
                        .collect();
                    if aspects.is_empty() {
                        change.row_kind.label().to_string()
                    } else {
                        aspects.join(", ")
                    }
                }
            };
            let ordinal = change.ordinal();
            if ordinal.is_empty() && change.text.trim().is_empty() {
                // Abstandshalter, hint without text: where it stands instead of „“.
                let place = match change.new_id {
                    Some(id) => place_of(after, id),
                    None => change
                        .old_id
                        .map(|id| place_of(before, id))
                        .unwrap_or_default(),
                };
                format!("{verb}: {what} ({place}).")
            } else if ordinal.is_empty() {
                format!(
                    "{verb}: {what} („{}“).",
                    crate::format::truncate_middle(&change.text, 40)
                )
            } else {
                format!("{verb}: {ordinal} – {what}.")
            }
        }
        changes => format!(
            "{verb}: {} ({}).",
            crate::format::count(changes.len(), "Zeile", "Zeilen"),
            diff.summary()
        ),
    };
    (message, row)
}

/// „nach 01.0010“ (the last row with an OZ above `id`), „am Anfang“.
fn place_of(lv: &Lv, id: RowId) -> String {
    let Some(index) = lv.index_of(id) else {
        return String::new();
    };
    lv.rows()[..index]
        .iter()
        .rev()
        .find(|row| row.has_ordinal() && !row.ordinal().trim().is_empty())
        .map(|row| format!("nach {}", row.ordinal()))
        .unwrap_or_else(|| "am Anfang".to_string())
}

#[cfg(test)]
mod undo_message_tests {
    use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position};

    use super::undo_message;

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        lv.push_position(Position::new("", "Modul", Decimal::TWO, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn the_message_names_what_comes_back() {
        let before = lv();
        let mut changed = before.clone();
        if let Some(proava_domain::LvRow::Position(position)) = changed.row_mut(1) {
            position.set_short_text("DDC-Station");
        }
        let (message, row) = undo_message("Rückgängig", &before, &changed);
        assert_eq!(message, "Rückgängig: 01.0010 – Kurztext.");
        assert_eq!(row, Some(before.rows()[1].id()));

        let mut shorter = before.clone();
        shorter.remove_block(2);
        let (message, _) = undo_message("Wiederholt", &before, &shorter);
        assert_eq!(message, "Wiederholt: 01.0020 – Position entfernt.");
        let (message, row) = undo_message("Rückgängig", &shorter, &before);
        assert_eq!(message, "Rückgängig: 01.0020 – Position wieder da.");
        assert_eq!(row, Some(before.rows()[2].id()));

        assert_eq!(
            undo_message("Rückgängig", &before, &before).0,
            "Rückgängig."
        );
    }

    #[test]
    fn spacers_are_named_by_their_place_without_an_empty_oz() {
        use proava_domain::{LvRow, Note, Spacer};
        let plain = lv();
        // Inserted after 01.0010 and taken back / brought again.
        let mut spaced = plain.clone();
        let id = spaced.insert_row(2, LvRow::Spacer(Spacer::new()));
        let (message, row) = undo_message("Rückgängig", &spaced, &plain);
        assert_eq!(
            message,
            "Rückgängig: Abstandshalter entfernt (nach 01.0010)."
        );
        assert_eq!(row, Some(id));
        let (message, row) = undo_message("Wiederholt", &plain, &spaced);
        assert_eq!(
            message,
            "Wiederholt: Abstandshalter wieder da (nach 01.0010)."
        );
        assert_eq!(row, Some(id));
        // Moved below 01.0020 (Alt+↓) and back.
        let mut moved = spaced.clone();
        moved.move_block(2, false).unwrap();
        assert_eq!(moved.index_of(id), Some(3));
        let (message, row) = undo_message("Rückgängig", &moved, &spaced);
        assert_eq!(
            message,
            "Rückgängig: Abstandshalter verschoben (nach 01.0010)."
        );
        assert_eq!(row, Some(id));
        let (message, _) = undo_message("Wiederholt", &spaced, &moved);
        assert_eq!(
            message,
            "Wiederholt: Abstandshalter verschoben (nach 01.0020)."
        );
        // At the very start.
        let mut first = plain.clone();
        first.insert_row(0, LvRow::Spacer(Spacer::new()));
        assert_eq!(
            undo_message("Rückgängig", &first, &plain).0,
            "Rückgängig: Abstandshalter entfernt (am Anfang)."
        );
        // A new position next to a spacer does not move the spacer.
        let mut more = spaced.clone();
        more.insert_row(
            3,
            LvRow::Position(Position::new("", "Netzteil", Decimal::ONE, "St", None)),
        );
        let (message, _) = undo_message("Rückgängig", &more, &spaced);
        assert_eq!(message, "Rückgängig: Position entfernt („Netzteil“).");
        // Hint without text turned into a spacer and back.
        let mut note = plain.clone();
        let blank = note.insert_row(2, LvRow::Note(Note::new("")));
        let mut converted = note.clone();
        converted.blank_notes_to_spacers(&[blank]);
        let (message, row) = undo_message("Rückgängig", &converted, &note);
        assert_eq!(
            message,
            "Rückgängig: Abstandshalter → Hinweis (nach 01.0010)."
        );
        assert_eq!(row, Some(blank));
        assert_eq!(
            undo_message("Wiederholt", &note, &converted).0,
            "Wiederholt: Hinweis → Abstandshalter (nach 01.0010)."
        );
        // A moved hint with text is named too (was „Rückgängig.“).
        let mut texted = plain.clone();
        texted.insert_row(2, LvRow::Note(Note::new("Gilt für alle Module")));
        let mut moved_note = texted.clone();
        moved_note.move_block(2, false).unwrap();
        assert_eq!(
            undo_message("Rückgängig", &moved_note, &texted).0,
            "Rückgängig: Hinweis verschoben („Gilt für alle Module“)."
        );
    }
}

/// GAEB files and zips that open the take-over dialog when dropped.
fn catalog_extensions() -> Vec<String> {
    crate::stlb_dialog::CATALOG_EXTENSIONS
        .iter()
        .map(|extension| extension.to_string())
        .collect()
}

/// The row under `pos` among the rows drawn last; below the last row: the
/// last one (insert at the end), above the first: none.
fn drop_row(rects: &[(usize, Rect)], pos: egui::Pos2) -> Option<usize> {
    let inside = rects
        .iter()
        .find(|(_, rect)| rect.y_range().contains(pos.y) && rect.x_range().contains(pos.x))
        .map(|(index, _)| *index);
    inside.or_else(|| {
        let (index, last) = rects.last()?;
        (pos.y > last.bottom() && last.x_range().contains(pos.x)).then_some(*index)
    })
}

/// While files are dragged over the window: a band at the top of the table
/// says what happens on drop (winit tells no position before the drop).
fn drop_hint(ui: &egui::Ui, palette: &Palette, area: Rect) {
    let hovering = ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
    if !hovering {
        return;
    }
    let band = Rect::from_min_size(area.min, Vec2::new(area.width(), 34.0));
    let painter = ui.painter();
    painter.rect_filled(band, egui::CornerRadius::same(4), palette.accent);
    painter.text(
        band.center(),
        egui::Align2::CENTER_CENTER,
        crate::strings::DROP_HINT,
        egui::FontId::proportional(13.0),
        palette.on_accent,
    );
}

#[cfg(test)]
mod drop_tests {
    use super::*;

    #[test]
    fn the_row_under_the_pointer_or_the_last_one_below_the_table() {
        let row = |top: f32| Rect::from_min_size(egui::pos2(0.0, top), Vec2::new(400.0, 30.0));
        let rects = vec![(4, row(100.0)), (5, row(130.0)), (9, row(160.0))];
        assert_eq!(drop_row(&rects, egui::pos2(50.0, 140.0)), Some(5));
        assert_eq!(
            drop_row(&rects, egui::pos2(50.0, 400.0)),
            Some(9),
            "unter der letzten Zeile"
        );
        assert_eq!(
            drop_row(&rects, egui::pos2(50.0, 50.0)),
            None,
            "über der Tabelle"
        );
        assert_eq!(
            drop_row(&rects, egui::pos2(900.0, 140.0)),
            None,
            "neben der Tabelle"
        );
        assert_eq!(drop_row(&[], egui::pos2(50.0, 140.0)), None);
        assert!(catalog_extensions().contains(&"x81".to_string()));
    }
}

#[cfg(test)]
mod title_share_tests {
    use proava_domain::Money;

    #[test]
    fn share_of_the_lv_total_with_one_decimal() {
        let money = |cents: i64| Money::from_cents(cents);
        assert_eq!(
            super::title_share(money(10_492_038), money(25_588_528)).as_deref(),
            Some("41,0 %")
        );
        assert_eq!(
            super::title_share(money(1), money(3)).as_deref(),
            Some("33,3 %")
        );
        assert_eq!(
            super::title_share(money(0), money(3)).as_deref(),
            Some("0,0 %")
        );
        assert_eq!(
            super::title_share(money(5), money(0)),
            None,
            "ohne LV-Summe kein Anteil"
        );
    }
}
