//! The running text of the editor: all paragraphs share one caret and one
//! selection (across paragraph borders, over tables and images), laid out
//! and painted here – egui's text field knows only one alignment per field.
//!
//! Keys: arrows (Strg = word, Umschalt = select), Pos1/Ende (Strg = text
//! start/end), Strg+A, Enter (new paragraph), Umschalt+Enter (line break),
//! Rücktaste/Entf (Strg = word), Strg+C/X/V with formatting (HTML).
//!
//! The caret blinks like egui's text fields (`Visuals::text_cursor`) and
//! starts visible again after every move. Input methods (IME, composed
//! characters): the pre-edit text stands underlined at the caret and only
//! reaches the LV with the commit; the candidate window sits at the caret.

use std::sync::Arc;

use eframe::egui::{
    self, epaint::text::cursor::CCursor, Event, EventFilter, Galley, Key, Modifiers, Pos2, Rect,
    Sense, Stroke, Vec2,
};

use super::fields::{self, FieldMap, FieldSpot};
use super::html;
use super::model::{ordered, Doc, DocBlock, Fmt, Para, ParaKind, Pos, Spot};
use super::{Action, FmtChangeKind, RichEditor, RichEditorOutput, PX_PER_PT};
use crate::theme::Palette;

/// Indent of list paragraphs (marker column).
const LIST_INDENT: f32 = 22.0;

pub(super) struct ParaLayout {
    galley: Arc<Galley>,
    /// Where the galley is painted (its x = 0 line; centre/right for
    /// centred/right-aligned text).
    origin: Pos2,
    rect: Rect,
    /// Field labels in the galley (text index ↔ galley index).
    fields: FieldMap,
}

/// The last copy of any text editor of this program (all windows share the
/// egui context): plain text, fragment, the Textergänzungen of its field
/// chars – pasted in any editor with all formats and fields.
pub(super) type Copied = (String, Doc, Vec<(char, proava_domain::TextComplement)>);

fn shared_copy_id() -> egui::Id {
    egui::Id::unique("proava_rich_editor_copy")
}

/// The last copy of the text editors, if any.
pub(super) fn shared_copy(ctx: &egui::Context) -> Option<std::sync::Arc<Copied>> {
    ctx.data(|data| data.get_temp::<std::sync::Arc<Copied>>(shared_copy_id()))
}

/// Unknown word under a right click: block, chars, word, suggestions.
type MenuWord = (usize, std::ops::Range<usize>, String, Option<Vec<String>>);

#[derive(Default)]
pub(super) struct Flow {
    pub anchor: Pos,
    pub head: Pos,
    layouts: Vec<Option<ParaLayout>>,
    dragging: bool,
    /// Screen x the caret keeps on ↑/↓.
    h_pos: Option<f32>,
    scroll: bool,
    /// Set focus to the running text on the next frame.
    pub want_focus: bool,
    clipboard: Option<arboard::Clipboard>,
    /// Unknown word under the last right click: block, chars, word,
    /// suggestions (computed when the menu opens).
    menu_word: Option<MenuWord>,
    /// Field under the last right click: its char position and index.
    menu_field: Option<(Pos, usize)>,
    /// Pre-edit text of an input method (start, end; one paragraph): in
    /// the doc for layout and caret, left out of what the editor hands
    /// back until it is committed.
    pub composing: Option<(Pos, Pos)>,
    /// Time from which the caret blinks (`None` = from the next paint:
    /// visible right after a move or edit).
    blink_since: Option<f64>,
    /// Caret on screen this frame (for the input method's window).
    caret_rect: Option<Rect>,
}

impl Flow {
    /// New text: caret to the start, clipboard state stays.
    pub fn reset(&mut self) {
        let clipboard = self.clipboard.take();
        *self = Flow {
            clipboard,
            ..Flow::default()
        };
    }

    pub fn selection(&self) -> (Pos, Pos) {
        ordered(self.anchor, self.head)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.head
    }
}

impl RichEditor {
    pub(super) fn flow_id(&self) -> egui::Id {
        self.salt.with("flow")
    }

    pub(super) fn flow_focused(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|memory| memory.has_focus(self.flow_id()))
    }

    /// Caret (and selection) to `pos`.
    pub(super) fn flow_set(&mut self, anchor: Pos, head: Pos) {
        self.flow.anchor = self.doc.clamp(anchor);
        self.flow.head = self.doc.clamp(head);
        self.active = Spot::block(self.flow.head.block);
        self.flow.scroll = true;
        self.flow.blink_since = None;
    }

    /// Removes the pre-edit text of an input method from the doc (the
    /// caret goes where it started). True when there was one.
    pub(super) fn drop_composition(&mut self) -> bool {
        let Some((start, end)) = self.flow.composing.take() else {
            return false;
        };
        let caret = self.doc.delete_range(start, end);
        self.flow_set(caret, caret);
        true
    }

    /// The doc without pre-edit text (what the LV gets).
    pub(super) fn committed_doc(&self) -> std::borrow::Cow<'_, Doc> {
        match self.flow.composing {
            Some((start, end)) => {
                let mut doc = self.doc.clone();
                doc.delete_range(start, end);
                std::borrow::Cow::Owned(doc)
            }
            None => std::borrow::Cow::Borrowed(&self.doc),
        }
    }

    /// One input method event; true when the committed text changed.
    fn ime_event(&mut self, event: &egui::ImeEvent) -> bool {
        match event {
            egui::ImeEvent::Preedit { text, .. } => {
                // Empty pre-edits without a composition come from some
                // integrations when the input method is switched on.
                if (text.is_empty() && self.flow.composing.is_none())
                    || text == "\n"
                    || text == "\r"
                {
                    return false;
                }
                self.drop_composition();
                if text.is_empty() {
                    return false;
                }
                let fmt = self.typing_fmt();
                // A selection is replaced by what is being composed.
                let changed = self.delete_selection();
                let start = self.flow.head;
                let end = self.doc.insert_text(start, text, &fmt);
                self.flow_set(end, end);
                self.flow.composing = Some((start, end));
                changed
            }
            egui::ImeEvent::Commit(text) => {
                if (text.is_empty() && self.flow.composing.is_none())
                    || text == "\n"
                    || text == "\r"
                {
                    return false;
                }
                self.drop_composition();
                if text.is_empty() {
                    return false;
                }
                self.type_text(text);
                true
            }
            egui::ImeEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            } => {
                self.drop_composition();
                let head = self.flow.head;
                let len = self.doc.para(Spot::block(head.block)).map_or(0, Para::len);
                let from = Pos::new(head.block, head.offset.saturating_sub(*before_chars));
                let to = Pos::new(head.block, (head.offset + after_chars).min(len));
                if from == to {
                    return false;
                }
                let caret = self.doc.delete_range(from, to);
                self.flow_set(caret, caret);
                true
            }
            #[allow(deprecated)]
            egui::ImeEvent::Enabled | egui::ImeEvent::Disabled => false,
        }
    }

    /// Format the next typed char gets.
    fn typing_fmt(&self) -> Fmt {
        let head = self.flow.head;
        if let Some((spot, caret, fmt)) = &self.pending {
            if *spot == Spot::block(head.block) && *caret == head.offset {
                return fmt.clone();
            }
        }
        self.doc
            .para(Spot::block(head.block))
            .map(|para| para.fmt_at(head.offset))
            .unwrap_or_default()
    }

    /// Removes the selection; true when there was one.
    fn delete_selection(&mut self) -> bool {
        if !self.flow.has_selection() {
            return false;
        }
        let caret = self.doc.delete_range(self.flow.anchor, self.flow.head);
        self.flow_set(caret, caret);
        true
    }

    fn type_text(&mut self, text: &str) {
        let fmt = self.typing_fmt();
        self.delete_selection();
        let caret = self.doc.insert_text(self.flow.head, text, &fmt);
        self.flow_set(caret, caret);
        if let Some((spot, at, _)) = &mut self.pending {
            if *spot == Spot::block(caret.block) {
                *at = caret.offset;
            }
        }
        self.flow.h_pos = None;
    }

    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if !crate::system_clipboard::enabled() {
            // Tests: never the clipboard of the person at the computer.
            return None;
        }
        if self.flow.clipboard.is_none() {
            self.flow.clipboard = arboard::Clipboard::new().ok();
        }
        self.flow.clipboard.as_mut()
    }

    /// Selection to the clipboard as HTML and plain text.
    fn copy(&mut self, ctx: &egui::Context) {
        if !self.flow.has_selection() {
            return;
        }
        let (a, b) = self.flow.selection();
        let raw = self.doc.plain_text(a, b);
        let fragment = self.doc.fragment(a, b);
        // Outside proAVA a field is its text („Fabrikat: ………… (vom Bieter)“);
        // pasted back here it is a field again (with its Textergänzung).
        let plain = self.fields_as_text(&raw);
        let html = html::to_html_with_fields(&fragment.to_rich(), &self.fields);
        let result = self
            .clipboard()
            .map(|clipboard| clipboard.set_html(html, Some(plain.clone())));
        let done = matches!(result, Some(Ok(())));
        if !done {
            ctx.copy_text(plain.clone());
        }
        let snapshot = self.field_snapshot(&raw);
        let copy: Copied = (plain, fragment, snapshot);
        ctx.data_mut(|data| data.insert_temp(shared_copy_id(), std::sync::Arc::new(copy)));
    }

    /// Pastes: HTML from the clipboard with formatting, else our own copy,
    /// else `text` as paragraphs in the current format.
    fn paste(&mut self, text: &str, ctx: &egui::Context) {
        let html = self
            .clipboard()
            .and_then(|clipboard| clipboard.get().html().ok())
            .filter(|html| !html.trim().is_empty());
        self.paste_with(text, html, ctx);
    }

    /// [`Self::paste`] with the HTML of the clipboard (tests: another
    /// proAVA's copy without the system clipboard).
    pub(super) fn paste_with(&mut self, text: &str, html: Option<String>, ctx: &egui::Context) {
        // Without stray control characters (also CR → line break).
        let normalized = proava_domain::strip_control_chars(text).into_owned();
        // A copy of this program first (any editor, any window): it brings
        // its fields along.
        let own = shared_copy(ctx)
            .filter(|copy| copy.0 == normalized)
            .map(|copy| self.adopt_fields(copy.1.clone(), &copy.2));
        // HTML: from another proAVA its Bieterangaben come as fields.
        let fragment = own.or_else(|| {
            let html = html?;
            let (mut rich, fields) = html::from_html_with_fields(&html);
            rich.strip_control_chars();
            if rich.is_empty() {
                return None;
            }
            let snapshot: Vec<_> = fields
                .into_iter()
                .enumerate()
                .map(|(index, complement)| {
                    (
                        proava_domain::complement_text::field_char(index),
                        complement,
                    )
                })
                .collect();
            Some(self.adopt_fields(Doc::from_rich(&rich), &snapshot))
        });
        let normalized = proava_domain::complement_text::strip_field_chars(&normalized);
        let fragment = fragment.unwrap_or_else(|| {
            let fmt = self.typing_fmt();
            Doc {
                blocks: normalized
                    .split('\n')
                    .map(|line| {
                        let mut para = Para::default();
                        para.insert(0, line, &fmt);
                        para.empty_fmt = fmt.clone();
                        DocBlock::Para(para)
                    })
                    .collect(),
            }
        });
        self.delete_selection();
        let caret = self.doc.insert_fragment(self.flow.head, &fragment);
        self.flow_set(caret, caret);
    }

    /// Keys and clipboard events while the running text has the focus.
    /// Returns true when the text changed; `typing` says whether it was
    /// plain typing (grouped into one undo step).
    pub(super) fn flow_input(
        &mut self,
        ui: &egui::Ui,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let ctx = ui.ctx().clone();
        if !self.flow_focused(&ctx) {
            return false;
        }
        let mut changed = false;
        let mut typing = true;
        let events = ui.input(|input| input.events.clone());
        for event in events {
            // Anything else than the input method ends a composition (its
            // pre-edit text goes; the input method commits or drops it).
            if self.flow.composing.is_some()
                && matches!(
                    event,
                    Event::Text(_)
                        | Event::Cut
                        | Event::Paste(_)
                        | Event::Key { pressed: true, .. }
                )
            {
                self.drop_composition();
                ctx.memory_mut(|memory| memory.interrupt_ime());
            }
            match event {
                Event::Text(text) if !text.chars().any(char::is_control) => {
                    self.type_text(&text);
                    changed = true;
                }
                Event::Ime(ime) => {
                    changed |= self.ime_event(&ime);
                }
                Event::Copy => self.copy(&ctx),
                Event::Cut => {
                    self.copy(&ctx);
                    changed |= self.delete_selection();
                    typing = false;
                }
                Event::Paste(text) => {
                    self.paste(&text, &ctx);
                    changed = true;
                    typing = false;
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(edit) = self.flow_key(key, modifiers, actions) {
                        changed |= edit;
                        typing &= key != Key::Enter && key != Key::Tab;
                    }
                }
                _ => {}
            }
        }
        if changed {
            self.touched = true;
            output.edit_key = if typing {
                Some(format!("rt-type:{}", self.flow.head.block))
            } else {
                None
            };
        }
        changed
    }

    /// One key; `Some(true)` = text changed, `Some(false)` = handled.
    fn flow_key(
        &mut self,
        key: Key,
        modifiers: Modifiers,
        actions: &mut Vec<Action>,
    ) -> Option<bool> {
        let shift = modifiers.shift;
        let word = modifiers.command || modifiers.alt;
        let head = self.flow.head;
        let (start, end) = self.flow.selection();
        let move_to = |editor: &mut Self, pos: Pos, keep_h: bool| {
            let anchor = if shift { editor.flow.anchor } else { pos };
            editor.flow_set(anchor, pos);
            if !keep_h {
                editor.flow.h_pos = None;
            }
            editor.pending = None;
        };
        match key {
            // Strg+A only: Strg+Umschalt+A is „Neue Ausführungsbeschreibung“.
            Key::A if modifiers.command && !shift => {
                let (a, b) = (self.doc.start(), self.doc.end());
                self.flow_set(a, b);
                Some(false)
            }
            Key::B | Key::G if modifiers.command && shift => {
                actions.push(Action::OpenField {
                    owner: key == Key::G,
                });
                Some(false)
            }
            Key::B | Key::I | Key::U if modifiers.command => {
                actions.push(Action::Format(match key {
                    Key::B => FmtChangeKind::Bold,
                    Key::I => FmtChangeKind::Italic,
                    _ => FmtChangeKind::Underline,
                }));
                Some(false)
            }
            Key::ArrowLeft => {
                let target = if self.flow.has_selection() && !shift {
                    start
                } else if word {
                    self.doc.word_left(head)
                } else {
                    self.doc.left(head)
                };
                move_to(self, target, false);
                Some(false)
            }
            Key::ArrowRight => {
                let target = if self.flow.has_selection() && !shift {
                    end
                } else if word {
                    self.doc.word_right(head)
                } else {
                    self.doc.right(head)
                };
                move_to(self, target, false);
                Some(false)
            }
            Key::ArrowUp | Key::ArrowDown => {
                let up = key == Key::ArrowUp;
                if let Some(target) = self.vertical(head, up) {
                    move_to(self, target, true);
                }
                Some(false)
            }
            Key::Home => {
                let target = if modifiers.command {
                    self.doc.start()
                } else {
                    self.row_edge(head, true)
                };
                move_to(self, target, false);
                Some(false)
            }
            Key::End => {
                let target = if modifiers.command {
                    self.doc.end()
                } else {
                    self.row_edge(head, false)
                };
                move_to(self, target, false);
                Some(false)
            }
            Key::Enter => {
                if shift {
                    self.type_text("\n");
                } else {
                    self.delete_selection();
                    // A format switched at the caret (Strg+B/I/U without a
                    // selection) goes into the new paragraph, as in a word
                    // processor: italic switched off, Enter → not italic.
                    let head = self.flow.head;
                    let switched = self
                        .pending
                        .as_ref()
                        .filter(|(spot, caret, _)| {
                            *spot == Spot::block(head.block) && *caret == head.offset
                        })
                        .map(|(_, _, fmt)| fmt.clone());
                    let caret = self.doc.break_paragraph(head);
                    self.flow_set(caret, caret);
                    self.pending = switched.map(|fmt| {
                        if let Some(para) = self.doc.para_mut(Spot::block(caret.block)) {
                            if para.is_empty() {
                                para.empty_fmt = fmt.clone();
                            }
                        }
                        (Spot::block(caret.block), caret.offset, fmt)
                    });
                }
                Some(true)
            }
            // Tab / Umschalt+Tab in a list: Unterpunkt one level in / out.
            // Elsewhere the key does nothing (it stays in the text).
            Key::Tab if !modifiers.command && !modifiers.alt => {
                let (a, b) = (self.flow.anchor, self.flow.head);
                if self.doc.shift_level(a, b, !shift) {
                    self.pending = None;
                    Some(true)
                } else {
                    None
                }
            }
            Key::Backspace => {
                if !self.delete_selection() {
                    let caret = if word && head.offset > 0 {
                        let from = self.doc.word_left(head);
                        self.doc.delete_range(from, head)
                    } else {
                        self.doc.backspace(head)
                    };
                    self.flow_set(caret, caret);
                }
                self.pending = None;
                Some(true)
            }
            Key::Delete => {
                if !self.delete_selection() {
                    let caret = if word {
                        let to = self.doc.word_right(head);
                        self.doc.delete_range(head, to)
                    } else {
                        self.doc.delete_forward(head)
                    };
                    self.flow_set(caret, caret);
                }
                self.pending = None;
                Some(true)
            }
            _ => None,
        }
    }

    /// Start or end of the screen row of `pos`.
    fn row_edge(&self, pos: Pos, start: bool) -> Pos {
        let Some(Some(layout)) = self.flow.layouts.get(pos.block) else {
            let len = self.doc.para(Spot::block(pos.block)).map_or(0, Para::len);
            return Pos::new(pos.block, if start { 0 } else { len });
        };
        let cursor = CCursor::new(layout.fields.to_galley(pos.offset));
        let edge = if start {
            layout.galley.cursor_begin_of_row(&cursor)
        } else {
            layout.galley.cursor_end_of_row(&cursor)
        };
        Pos::new(pos.block, layout.fields.to_model(edge.index.0))
    }

    /// One row up/down, across paragraph borders; into a table that is in
    /// the way (sets the cell focus and returns `None`).
    fn vertical(&mut self, pos: Pos, up: bool) -> Option<Pos> {
        let layout = self.flow.layouts.get(pos.block)?.as_ref()?;
        let galley = &layout.galley;
        let caret = galley.pos_from_cursor(CCursor::new(layout.fields.to_galley(pos.offset)));
        let x = self
            .flow
            .h_pos
            .unwrap_or(layout.origin.x + caret.center().x);
        self.flow.h_pos = Some(x);
        let row = galley
            .rows
            .iter()
            .position(|row| caret.center().y <= row.rect().max.y)
            .unwrap_or(galley.rows.len().saturating_sub(1));
        let inside = if up {
            row > 0
        } else {
            row + 1 < galley.rows.len()
        };
        if inside {
            let target = galley.rows[if up { row - 1 } else { row + 1 }]
                .rect()
                .center()
                .y;
            let cursor = galley.cursor_from_pos(Vec2::new(x - layout.origin.x, target));
            return Some(Pos::new(pos.block, layout.fields.to_model(cursor.index.0)));
        }
        // Neighbour block.
        let neighbour = if up {
            pos.block.checked_sub(1)?
        } else {
            pos.block + 1
        };
        match self.doc.blocks.get(neighbour)? {
            DocBlock::Table(table) => {
                let row = if up {
                    table.rows.len().saturating_sub(1)
                } else {
                    0
                };
                self.focus = Some((
                    Spot {
                        block: neighbour,
                        cell: Some((row, 0)),
                    },
                    0,
                    0,
                ));
                None
            }
            _ => {
                let target = if up {
                    self.doc.para_before(pos.block)?
                } else {
                    self.doc.para_after(pos.block)?
                };
                let Some(Some(next)) = self.flow.layouts.get(target) else {
                    return Some(Pos::new(target, 0));
                };
                let rows = &next.galley.rows;
                let y = if up {
                    rows.last().map_or(0.0, |row| row.rect().center().y)
                } else {
                    rows.first().map_or(0.0, |row| row.rect().center().y)
                };
                let cursor = next.galley.cursor_from_pos(Vec2::new(x - next.origin.x, y));
                Some(Pos::new(target, next.fields.to_model(cursor.index.0)))
            }
        }
    }

    /// Paragraph position under a screen point (nearest paragraph by height).
    fn hit(&self, point: Pos2) -> Option<Pos> {
        let mut best: Option<(f32, usize)> = None;
        for (block, layout) in self.flow.layouts.iter().enumerate() {
            let Some(layout) = layout else { continue };
            let distance = if point.y < layout.rect.top() {
                layout.rect.top() - point.y
            } else if point.y > layout.rect.bottom() {
                point.y - layout.rect.bottom()
            } else {
                0.0
            };
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, block));
            }
        }
        let (_, block) = best?;
        let layout = self.flow.layouts[block].as_ref()?;
        let relative = point - layout.origin;
        let y = relative.y.clamp(0.0, layout.galley.rect.height().max(0.0));
        let cursor = layout.galley.cursor_from_pos(Vec2::new(relative.x, y));
        Some(Pos::new(block, layout.fields.to_model(cursor.index.0)))
    }

    /// Start of a frame: layouts of the last frame become stale, focus
    /// requests for the running text are applied.
    pub(super) fn flow_begin(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx().clone();
        // egui keeps the focus only on ids used as widgets in a frame, and
        // drops it on a press elsewhere: registered before the paragraphs,
        // a press into the text takes the focus again below.
        ui.interact(
            Rect::from_min_size(ui.cursor().min, Vec2::splat(1.0)),
            self.flow_id(),
            Sense::focusable_noninteractive(),
        );
        if let Some((spot, start, end)) = self.focus {
            if spot.cell.is_none() {
                self.focus = None;
                self.flow_set(Pos::new(spot.block, start), Pos::new(spot.block, end));
                self.flow.want_focus = true;
            }
        }
        if std::mem::take(&mut self.flow.want_focus) {
            ctx.memory_mut(|memory| memory.request_focus(self.flow_id()));
            self.touched = true;
        }
        if self.flow.composing.is_some() && !self.flow_focused(&ctx) {
            self.drop_composition();
        }
        self.flow.anchor = self.doc.clamp(self.flow.anchor);
        self.flow.head = self.doc.clamp(self.flow.head);
        self.flow.caret_rect = None;
    }

    /// After the keys: this frame's layouts are filled while painting
    /// (↑/↓ and Pos1/Ende used those of the last frame).
    pub(super) fn flow_layouts_reset(&mut self) {
        self.flow.layouts.clear();
        self.flow
            .layouts
            .resize_with(self.doc.blocks.len(), || None);
    }

    /// Lays out, paints and handles the mouse for paragraph `block`.
    pub(super) fn flow_para_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        block: usize,
        width: f32,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) {
        let Some(DocBlock::Para(para)) = self.doc.blocks.get(block) else {
            return;
        };
        let indent = if para.kind == ParaKind::Text {
            0.0
        } else {
            LIST_INDENT * (f32::from(para.level) + 1.0)
        };
        let text_width = (width - indent).max(40.0);
        let spacing = super::text_spacing(ui.ctx());
        let (mut job, field_map, spell_text) =
            fields::flow_layout_job(para, text_width, self.zoom, palette.text, &self.fields);
        super::apply_line_spacing(ui, &mut job, spacing.line.factor());
        let galley = super::with_trailing_spaces(ui.fonts_mut(|fonts| fonts.layout_job(job)));
        // Lines of a paragraph and paragraphs at the same distance, plus the
        // Abstand nach Absatz – as printed (Extras ▸ Einstellungen).
        let size_pt = para
            .fmt_at(0)
            .size_pt
            .unwrap_or(proava_domain::DEFAULT_SIZE_PT);
        let height =
            galley.rect.height().max(4.0) + super::paragraph_gap_px(spacing, size_pt, self.zoom);
        let id = self.flow_id().with(block);
        let item_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        ui.spacing_mut().item_spacing.y = item_spacing;
        let response = ui
            .interact(rect, id, Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Text);
        let text_rect = Rect::from_min_max(egui::pos2(rect.left() + indent, rect.top()), rect.max);
        let origin = match para.align {
            proava_domain::Align::Center => egui::pos2(text_rect.center().x, text_rect.top()),
            proava_domain::Align::Right => egui::pos2(text_rect.right(), text_rect.top()),
            _ => text_rect.left_top(),
        };
        let kind = para.kind;
        let first_fmt = para.fmt_at(0);
        let para_len = para.len();
        if self.flow.layouts.len() <= block {
            self.flow.layouts.resize_with(block + 1, || None);
        }
        self.flow.layouts[block] = Some(ParaLayout {
            galley: galley.clone(),
            origin,
            rect,
            fields: field_map.clone(),
        });
        // The field under the pointer (tooltip, double click, menu).
        let field_under = |point: Option<Pos2>| {
            point.and_then(|point| fields::span_at(&galley, &field_map, (point - origin).to_pos2()))
        };

        // Mouse: press sets the caret (Umschalt extends), double click a
        // word, triple click the paragraph; dragging is followed in flow_end.
        // The press position from the events: press and release can come in
        // the same frame, then egui has no press origin any more.
        let (press, shift) = ui.input(|input| {
            let press = input.events.iter().find_map(|event| match event {
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            });
            (press, input.modifiers.shift)
        });
        let press =
            press.filter(|p| ui.layer_id() == ui.ctx().layer_id_at(*p).unwrap_or(ui.layer_id()));
        if press.is_some_and(|p| rect.contains(p) && ui.clip_rect().contains(p)) {
            if let Some(mut pos) = press.and_then(|p| self.hit_in(block, p)) {
                if let Some((start, end)) = self.flow.composing {
                    // The click ends the composition; positions behind it
                    // move back by its length.
                    self.drop_composition();
                    ui.ctx().memory_mut(|memory| memory.interrupt_ime());
                    if pos.block == start.block && pos.offset > start.offset {
                        let len = end.offset - start.offset;
                        pos.offset = pos.offset.saturating_sub(len).max(start.offset);
                    }
                }
                let anchor = if shift { self.flow.anchor } else { pos };
                self.flow_set(anchor, pos);
                self.flow.scroll = false;
                self.flow.dragging = true;
                self.flow.h_pos = None;
                self.pending = None;
                self.touched = true;
                ui.ctx()
                    .memory_mut(|memory| memory.request_focus(self.flow_id()));
            }
        }
        // egui takes the focus away when the click ends (release) outside the
        // tiny focus widget of flow_begin – take it again for this text.
        if response.clicked() {
            ui.ctx()
                .memory_mut(|memory| memory.request_focus(self.flow_id()));
        }
        let hovered_field = field_under(response.hover_pos());
        if response.double_clicked() {
            if let Some(span) = hovered_field {
                // Doppelklick auf ein Feld: Vortext, Nachtext, Kennung bearbeiten.
                self.flow_set(Pos::new(block, span.model), Pos::new(block, span.model + 1));
                self.open_field(span.field);
            } else {
                let (a, b) = self.doc.word_at(self.flow.head);
                self.flow_set(a, b);
            }
            self.flow.dragging = false;
        }
        if response.triple_clicked() {
            self.flow_set(Pos::new(block, 0), Pos::new(block, para_len));
            self.flow.dragging = false;
        }

        // Painting: selection, list marker, text, caret.
        let painter = ui.painter_at(rect.expand(2.0));
        let focused = self.flow_focused(ui.ctx());
        let (a, b) = self.flow.selection();
        fields::paint_fields(&painter, palette, &galley, origin, &field_map, &self.fields);
        if a != b && a.block <= block && block <= b.block {
            let from = if block == a.block {
                field_map.to_galley(a.offset)
            } else {
                0
            };
            let to = if block == b.block {
                field_map.to_galley(b.offset)
            } else {
                usize::MAX
            };
            let fill = if focused {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().selection.bg_fill.gamma_multiply(0.5)
            };
            let mut index = 0usize;
            for row in &galley.rows {
                let row_rect = row.rect();
                let count = row.char_count_including_newline().0;
                let row_from = from.max(index);
                let row_to = to.min(index + count);
                if row_from < row_to || (count == 0 && from <= index && index < to) {
                    let x0 = row
                        .glyphs
                        .get(row_from - index)
                        .map_or(row_rect.max.x, |g| row.pos.x + g.pos.x);
                    let x1 = if row_to >= index + row.char_count_excluding_newline().0 {
                        row_rect.max.x
                            + if selection_passes_row_end(to, index, count) {
                                4.0
                            } else {
                                0.0
                            }
                    } else {
                        row.glyphs
                            .get(row_to - index)
                            .map_or(row_rect.max.x, |g| row.pos.x + g.pos.x)
                    };
                    let selected = Rect::from_min_max(
                        origin + Vec2::new(x0, row_rect.min.y),
                        origin + Vec2::new(x1.max(x0 + 3.0), row_rect.max.y),
                    );
                    painter.rect_filled(selected, 0.0, fill);
                }
                index += count;
            }
        }
        if let Some(marker) = self.doc.marker_of(block).filter(|_| kind != ParaKind::Text) {
            let level = self
                .doc
                .para(Spot::block(block))
                .map_or(0.0, |para| f32::from(para.level));
            let size = first_fmt.size_pt.unwrap_or(10.0) * PX_PER_PT * self.zoom;
            painter.text(
                egui::pos2(rect.left() + 2.0 + LIST_INDENT * level, rect.top()),
                egui::Align2::LEFT_TOP,
                marker,
                egui::FontId::proportional(size),
                palette.accent,
            );
        }
        painter.galley(origin, galley.clone(), palette.text);
        if let Some((start, end)) = self
            .flow
            .composing
            .filter(|(start, _)| start.block == block)
        {
            let (from, to) = (
                field_map.to_galley(start.offset),
                field_map.to_galley(end.offset),
            );
            for (x0, x1, y) in range_lines(&galley, from, to) {
                painter.line_segment(
                    [origin + Vec2::new(x0, y), origin + Vec2::new(x1, y)],
                    Stroke::new(1.0, palette.text),
                );
            }
        }
        if focused && self.flow.head.block == block {
            let caret = galley
                .pos_from_cursor(CCursor::new(field_map.to_galley(self.flow.head.offset)))
                .translate(origin.to_vec2());
            let caret = Rect::from_min_max(caret.min, egui::pos2(caret.min.x + 1.5, caret.max.y));
            if caret_visible(ui, &mut self.flow.blink_since) {
                painter.rect_filled(caret, 0.0, palette.text);
            }
            self.flow.caret_rect = Some(caret);
            if std::mem::take(&mut self.flow.scroll) {
                ui.scroll_to_rect(caret.expand2(Vec2::new(10.0, 14.0)), None);
            }
        }

        // Spelling: wavy lines; a right click on an unknown word offers
        // suggestions in the menu. Only paragraphs in view are checked: the
        // lines of the others would be clipped anyway (long texts: hundreds
        // of paragraphs every frame).
        let speller = crate::spell::speller(ui.ctx()).filter(|_| ui.is_rect_visible(rect));
        let right_click = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    ..
                } if rect.contains(*pos) => Some(*pos),
                _ => None,
            })
        });
        if let Some(point) = right_click {
            self.flow.menu_field =
                field_under(Some(point)).map(|span| (Pos::new(block, span.model), span.field));
        }
        if hovered_field.is_some() && !response.context_menu_opened() {
            response.clone().on_hover_text_at_pointer(
                hovered_field
                    .and_then(|span| self.fields.get(span.field))
                    .map(|complement| {
                        format!(
                            "{} {} – Doppelklick: bearbeiten; Entf/Rücktaste: entfernen",
                            complement.kind_name(),
                            complement.marker.trim()
                        )
                    })
                    .unwrap_or_default(),
            );
        }
        if let Some(speller) = &speller {
            let skip = (focused && self.flow.head.block == block)
                .then(|| field_map.to_galley(self.flow.head.offset));
            let unknown: Vec<(std::ops::Range<usize>, String)> =
                crate::spell::mark(&painter, speller, &spell_text, &galley, origin, skip)
                    .into_iter()
                    .map(|(range, word)| {
                        (
                            field_map.to_model(range.start)..field_map.to_model(range.end),
                            word,
                        )
                    })
                    .collect();
            if let Some(point) = right_click {
                let at = self
                    .hit_in(block, point)
                    .map_or(usize::MAX, |pos| pos.offset);
                self.flow.menu_word = unknown
                    .into_iter()
                    .find(|(range, _)| range.start <= at && at <= range.end)
                    .map(|(range, word)| (block, range, word, None));
            }
        }

        response.context_menu(|ui| {
            if let (Some(speller), Some((menu_block, range, word, suggestions))) =
                (&speller, self.flow.menu_word.clone())
            {
                if menu_block == block {
                    let suggestions = suggestions.unwrap_or_else(|| {
                        let list = speller.suggest(&word, 6);
                        self.flow.menu_word =
                            Some((block, range.clone(), word.clone(), Some(list.clone())));
                        list
                    });
                    if let Some(choice) =
                        crate::spell::suggestion_items(ui, speller, &word, &suggestions)
                    {
                        let fmt = self
                            .doc
                            .para(Spot::block(block))
                            .map(|para| para.fmt_at(range.start + 1))
                            .unwrap_or_default();
                        let start = self
                            .doc
                            .delete_range(Pos::new(block, range.start), Pos::new(block, range.end));
                        let caret = self.doc.insert_text(start, &choice, &fmt);
                        self.flow_set(caret, caret);
                        output.changed = true;
                        output.edit_key = None;
                    }
                    ui.separator();
                }
            }
            if let Some((pos, index)) = self.flow.menu_field.filter(|(pos, _)| pos.block == block) {
                let name = self
                    .fields
                    .get(index)
                    .map_or("Textergänzung", |complement| complement.kind_name());
                if ui.button(format!("{name} bearbeiten …")).clicked() {
                    self.flow_set(pos, Pos::new(pos.block, pos.offset + 1));
                    self.open_field(index);
                    ui.close();
                }
                if ui.button(format!("{name} entfernen")).clicked() {
                    self.remove_field_at(pos);
                    output.edit_key = None;
                    ui.close();
                }
                ui.separator();
            }
            let has_selection = self.flow.has_selection();
            if ui
                .add_enabled(has_selection, egui::Button::new("Kopieren   Strg+C"))
                .clicked()
            {
                self.copy(ui.ctx());
                ui.close();
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("Ausschneiden   Strg+X"))
                .clicked()
            {
                self.copy(ui.ctx());
                if self.delete_selection() {
                    output.changed = true;
                    output.edit_key = None;
                }
                ui.close();
            }
            if ui.button("Einfügen   Strg+V").clicked() {
                let text = self
                    .clipboard()
                    .and_then(|clipboard| clipboard.get_text().ok())
                    .unwrap_or_default();
                self.paste(&text, ui.ctx());
                output.changed = true;
                output.edit_key = None;
                ui.close();
            }
            ui.separator();
            if ui.button("Bild einfügen …").clicked() {
                output.pick_image = true;
                ui.close();
            }
            if self.pdf_clips && ui.button(super::CLIP_MENU).clicked() {
                output.pick_pdf_clip = true;
                ui.close();
            }
            if ui.button("Tabelle einfügen (3 × 3)").clicked() {
                actions.push(Action::InsertTable(3, 3));
                ui.close();
            }
            if self.fields_on {
                ui.separator();
                crate::menus::menu(ui, super::BIDDER_MENU, |ui| {
                    self.field_menu_items(ui, FieldSpot::Caret);
                });
                crate::menus::menu(ui, super::BIDDER_BELOW_MENU, |ui| {
                    self.field_menu_items(ui, FieldSpot::End);
                });
            }
        });
    }

    fn hit_in(&self, block: usize, point: Pos2) -> Option<Pos> {
        let layout = self.flow.layouts.get(block)?.as_ref()?;
        let cursor = layout.galley.cursor_from_pos(point - layout.origin);
        Some(Pos::new(block, layout.fields.to_model(cursor.index.0)))
    }

    /// Covers a table or image inside the selection.
    pub(super) fn flow_mark_block(&self, ui: &egui::Ui, block: usize, rect: Rect) {
        let (a, b) = self.flow.selection();
        if a.block < block && block < b.block {
            ui.painter().rect_filled(
                rect,
                2.0,
                ui.visuals().selection.bg_fill.gamma_multiply(0.35),
            );
            ui.painter().rect_stroke(
                rect,
                2.0,
                Stroke::new(1.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// End of a frame: dragging, focus bookkeeping.
    pub(super) fn flow_end(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx().clone();
        let id = self.flow_id();
        ctx.memory_mut(|memory| memory.interested_in_focus(id, ui.layer_id()));
        if self.flow.dragging {
            let (down, pointer) =
                ui.input(|input| (input.pointer.primary_down(), input.pointer.interact_pos()));
            if down {
                if let Some(point) = pointer {
                    if let Some(pos) = self.hit(point) {
                        if pos != self.flow.head {
                            self.flow.head = pos;
                            self.active = Spot::block(pos.block);
                            self.flow.blink_since = None;
                        }
                    }
                    // Scroll while dragging past the visible area.
                    let clip = ui.clip_rect();
                    if point.y > clip.bottom() - 4.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, -12.0));
                    } else if point.y < clip.top() + 4.0 {
                        ui.scroll_with_delta(Vec2::new(0.0, 12.0));
                    }
                    ctx.request_repaint();
                }
            } else {
                self.flow.dragging = false;
            }
        }
        if self.flow_focused(&ctx) {
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    id,
                    EventFilter {
                        // Tab stays in the text: leaving by Tab let the next
                        // keys land in the LV table (Kurztext overwritten).
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                )
            });
            // The input method may be used; its window sits at the caret.
            if let Some(caret) = self.flow.caret_rect {
                let to_global = ctx
                    .layer_transform_to_global(ui.layer_id())
                    .unwrap_or_default();
                let rect = ui.min_rect();
                ctx.output_mut(|output| {
                    output.ime = Some(egui::output::IMEOutput {
                        purpose: egui::IMEPurpose::Normal,
                        rect: to_global * rect,
                        cursor_rect: to_global * caret,
                        should_interrupt_composition: false,
                    });
                });
            }
            // Lets the program know a text is being edited (shortcuts of
            // the LV table stay off, Strg+Z goes to the LV undo).
            if egui::text_edit::TextEditState::load(&ctx, id).is_none() {
                egui::text_edit::TextEditState::default().store(&ctx, id);
            }
        }
    }
}

/// Whether a selection ending at `to` goes past the end of a layout row that
/// starts at `index` with `count` characters (line break included) – then
/// the highlight reaches a bit further. An empty row (`count` 0, e.g. an
/// empty paragraph inside a selection) must not underflow.
fn selection_passes_row_end(to: usize, index: usize, count: usize) -> bool {
    to >= index + count
}

#[cfg(test)]
mod selection_tests {
    use super::selection_passes_row_end;

    #[test]
    fn empty_rows_in_a_selection_do_not_underflow() {
        // Stichprobe aus der Abnahme: Strg+Umschalt+Pos1 über einen leeren Absatz.
        assert!(selection_passes_row_end(0, 0, 0));
        assert!(selection_passes_row_end(usize::MAX, 0, 0));
        assert!(!selection_passes_row_end(4, 0, 5));
        assert!(selection_passes_row_end(5, 0, 5));
        assert!(selection_passes_row_end(usize::MAX, 10, 5));
    }
}

/// Whether the caret is drawn now: blinking as set in egui's visuals
/// (`text_cursor.blink`, on/off times), visible right after a move;
/// without window focus it stays visible and nothing is repainted.
fn caret_visible(ui: &egui::Ui, since: &mut Option<f64>) -> bool {
    let style = &ui.visuals().text_cursor;
    let now = ui.input(|input| input.time);
    let start = *since.get_or_insert(now);
    if !style.blink || !ui.input(|input| input.focused) {
        return true;
    }
    let (visible, wake) = blink_phase(now - start, style.on_duration, style.off_duration);
    ui.ctx().request_repaint_after_secs(wake);
    visible
}

/// Blink phase `elapsed` seconds after the last move: visible?, seconds
/// to the next change.
fn blink_phase(elapsed: f64, on: f32, off: f32) -> (bool, f32) {
    let total = (on + off).max(0.01);
    let at = (elapsed.max(0.0) % total as f64) as f32;
    if at < on {
        (true, on - at)
    } else {
        (false, total - at)
    }
}

/// Lines under the chars `from..to` of a galley: (x0, x1, y) relative to
/// the galley origin, one per screen row.
fn range_lines(galley: &Galley, from: usize, to: usize) -> Vec<(f32, f32, f32)> {
    let mut lines = Vec::new();
    let mut index = 0usize;
    for row in &galley.rows {
        let count = row.char_count_including_newline().0;
        let visible = row.char_count_excluding_newline().0;
        let row_from = from.max(index);
        let row_to = to.min(index + visible);
        if row_from < row_to {
            let x_at = |at: usize| {
                row.glyphs
                    .get(at - index)
                    .map_or(row.rect().max.x, |glyph| row.pos.x + glyph.pos.x)
            };
            lines.push((x_at(row_from), x_at(row_to), row.rect().max.y - 1.0));
        }
        index += count;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::blink_phase;
    use crate::rich_editor::RichEditor;
    use eframe::egui;
    use proava_domain::RichText;

    /// One frame of the editor with these input events.
    fn frame(
        ctx: &egui::Context,
        editor: &mut RichEditor,
        text: &mut RichText,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let palette = crate::theme::Palette::of(proava_application::ThemeChoice::Light);
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            editor.show(ui, &palette, text, &[]);
        });
        output.textures_delta.clear();
        output
    }

    fn key(key: egui::Key, shift: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: if shift {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            },
        }
    }

    fn cells(text: &RichText) -> Vec<Vec<String>> {
        text.blocks()
            .iter()
            .find_map(|block| match block {
                proava_domain::TextBlock::Table(table) => Some(table.texts()),
                _ => None,
            })
            .expect("Tabelle")
    }

    /// A space typed at the end of a justified, centred
    /// or right-aligned paragraph moves the caret at once (egui left it out
    /// of the row width; the caret moved only with the next character); a
    /// left-aligned one as before.
    #[test]
    fn a_space_at_the_end_moves_the_caret_in_every_alignment() {
        use proava_domain::{Align, Run, TextBlock};
        for align in [Align::Left, Align::Justify, Align::Center, Align::Right] {
            let ctx = egui::Context::default();
            crate::fonts::install(&ctx);
            let mut editor = RichEditor::new("space");
            let mut text = RichText::new(vec![TextBlock::Paragraph {
                runs: vec![Run::plain("Wort")],
                align,
            }]);
            editor.request_focus();
            frame(&ctx, &mut editor, &mut text, vec![]);
            frame(
                &ctx,
                &mut editor,
                &mut text,
                vec![key(egui::Key::End, false)],
            );
            frame(&ctx, &mut editor, &mut text, vec![]);
            let caret = |editor: &RichEditor| editor.flow.caret_rect.expect("Schreibmarke").left();
            // Where the first letter stands (row place + glyph).
            let first = |editor: &RichEditor| {
                let galley = &editor.flow.layouts[0].as_ref().expect("Absatz").galley;
                galley.rows[0].pos.x + galley.rows[0].row.glyphs[0].pos.x
            };
            let (caret_before, first_before) = (caret(&editor), first(&editor));
            frame(
                &ctx,
                &mut editor,
                &mut text,
                vec![egui::Event::Text(" ".into())],
            );
            frame(&ctx, &mut editor, &mut text, vec![]);
            assert_eq!(text.to_plain(), "Wort ", "{align:?}");
            let (caret_after, first_after) = (caret(&editor), first(&editor));
            let shown = match align {
                // Right aligned the text moves left by the space, the caret
                // stays at the right edge (else it stood outside the text).
                Align::Right => {
                    first_after < first_before - 2.0 && (caret_after - caret_before).abs() < 1.0
                }
                _ => caret_after > caret_before + 2.0 && (first_after - first_before).abs() < 1.0,
            };
            assert!(
                shown,
                "{align:?}: Schreibmarke {caret_before} → {caret_after}, Text {first_before} → {first_after}"
            );
            frame(
                &ctx,
                &mut editor,
                &mut text,
                vec![egui::Event::Text("Z".into())],
            );
            assert_eq!(text.to_plain(), "Wort Z", "{align:?}");
        }
    }

    #[test]
    fn tab_moves_between_table_cells_and_never_leaves_the_table() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("tab");
        let table = proava_domain::Table::from_texts(vec![
            vec!["a".into(), "b".into()],
            vec!["c".into(), "d".into()],
        ]);
        let mut text = RichText::new(vec![proava_domain::TextBlock::Table(table)]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        let block = editor
            .doc
            .blocks
            .iter()
            .position(|block| matches!(block, super::DocBlock::Table(_)))
            .unwrap();
        editor.focus = Some((
            super::Spot {
                block,
                cell: Some((0, 0)),
            },
            1,
            1,
        ));
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Tab, false)],
        );
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("Wert".into())],
        );
        assert_eq!(
            cells(&text)[0],
            vec!["a", "Wert"],
            "Tab wählt die nächste Zelle ganz"
        );

        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Tab, true)],
        );
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("A".into())],
        );
        assert_eq!(cells(&text)[0], vec!["A", "Wert"], "Umschalt+Tab zurück");

        for _ in 0..3 {
            frame(
                &ctx,
                &mut editor,
                &mut text,
                vec![key(egui::Key::Tab, false)],
            );
            frame(&ctx, &mut editor, &mut text, vec![]);
        }
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Tab, false)],
        );
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("neu".into())],
        );
        let rows = cells(&text);
        assert_eq!(
            rows.len(),
            3,
            "Tab in der letzten Zelle hängt eine Zeile an: {rows:?}"
        );
        assert_eq!(rows[2][0], "neu");
    }

    /// Runs of paragraph `index` as (text, italic).
    fn italic_runs(text: &RichText, index: usize) -> Vec<(String, bool)> {
        match &text.blocks()[index] {
            proava_domain::TextBlock::Paragraph { runs, .. } => runs
                .iter()
                .map(|run| (run.text.clone(), run.italic))
                .collect(),
            other => panic!("kein Absatz: {other:?}"),
        }
    }

    #[test]
    fn italic_switched_off_before_enter_stays_off_in_the_new_paragraph() {
        // Abnahme qa3 (18.11.): Strg+I aus, Enter – die neue Zeile war wieder kursiv.
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("enterfmt");
        let mut text = RichText::plain("Fabrikat");
        editor.request_focus();
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        let italic = egui::Event::Key {
            key: egui::Key::I,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::End, false)],
        );
        frame(&ctx, &mut editor, &mut text, vec![italic.clone()]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text(" Typ".into())],
        );
        frame(&ctx, &mut editor, &mut text, vec![italic.clone()]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Enter, false)],
        );
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("gerade".into())],
        );
        assert_eq!(
            italic_runs(&text, 0),
            vec![("Fabrikat".to_string(), false), (" Typ".to_string(), true)]
        );
        assert_eq!(italic_runs(&text, 1), vec![("gerade".to_string(), false)]);

        // Umgekehrt: kursiv eingeschaltet, Enter – die neue Zeile beginnt kursiv.
        frame(&ctx, &mut editor, &mut text, vec![italic]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Enter, false)],
        );
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("schräg".into())],
        );
        assert_eq!(italic_runs(&text, 2), vec![("schräg".to_string(), true)]);
    }

    #[test]
    fn tab_in_the_running_text_keeps_the_focus() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("tabflow");
        let mut text = RichText::plain("Haus");
        editor.request_focus();
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![key(egui::Key::Tab, false)],
        );
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("x".into())],
        );
        assert_eq!(text.to_plain(), "xHaus");
    }

    #[test]
    fn fast_typing_around_tab_stays_in_its_cell() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("tabfast");
        let table = proava_domain::Table::from_texts(vec![
            vec!["".into(), "".into(), "".into()],
            vec!["".into(), "".into(), "".into()],
        ]);
        let mut text = RichText::new(vec![proava_domain::TextBlock::Table(table)]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        let block = editor
            .doc
            .blocks
            .iter()
            .position(|block| matches!(block, super::DocBlock::Table(_)))
            .unwrap();
        editor.focus = Some((
            super::Spot {
                block,
                cell: Some((0, 0)),
            },
            0,
            0,
        ));
        frame(&ctx, &mut editor, &mut text, vec![]);
        let mut events = Vec::new();
        for word in ["Merkmal", "Wert", "Einheit", "Datenpunkte", "256", "DP"] {
            for ch in word.chars() {
                events.push(egui::Event::Text(ch.to_string()));
            }
            events.push(key(egui::Key::Tab, false));
        }
        // xdotool-like: a few events per frame.
        for chunk in events.chunks(3) {
            frame(&ctx, &mut editor, &mut text, chunk.to_vec());
        }
        frame(&ctx, &mut editor, &mut text, vec![]);
        assert_eq!(
            cells(&text),
            vec![
                vec!["Merkmal", "Wert", "Einheit"],
                vec!["Datenpunkte", "256", "DP"],
                vec!["", "", ""]
            ],
            "das letzte Tab hängt eine Zeile an"
        );
    }

    #[test]
    fn input_method_text_reaches_the_lv_only_when_committed() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("ime");
        let mut text = RichText::plain("Haus");
        editor.request_focus();
        frame(&ctx, &mut editor, &mut text, vec![]);
        let output = frame(&ctx, &mut editor, &mut text, vec![]);
        assert!(
            output.platform_output.ime.is_some(),
            "Eingabemethode erlaubt"
        );

        let preedit = |text: &str| {
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: text.to_string(),
                active_range_chars: None,
            })
        };
        frame(&ctx, &mut editor, &mut text, vec![preedit("a")]);
        frame(&ctx, &mut editor, &mut text, vec![preedit("ab")]);
        assert_eq!(text.to_plain(), "Haus", "Vorschautext nicht im LV");
        assert_eq!(
            editor.flow.composing.map(|(a, b)| b.offset - a.offset),
            Some(2)
        );

        let commit = egui::Event::Ime(egui::ImeEvent::Commit("Ä".to_string()));
        frame(&ctx, &mut editor, &mut text, vec![commit]);
        assert_eq!(text.to_plain(), "ÄHaus");
        assert!(editor.flow.composing.is_none());

        // Abgebrochen (leerer Vorschautext): nichts bleibt.
        frame(&ctx, &mut editor, &mut text, vec![preedit("x")]);
        frame(&ctx, &mut editor, &mut text, vec![preedit("")]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            vec![egui::Event::Text("e".into())],
        );
        assert_eq!(text.to_plain(), "ÄeHaus");
    }

    /// Gefunden mit dem Affentest: „Alles markieren“ bei einem leeren
    /// ersten Absatz (z. B. leere Nummerierung im Schriftverkehr) lief beim
    /// Malen der Markierung unter null (Überlauf im Debug-Build).
    /// Tops of the paragraph blocks and the row pitch inside the first one,
    /// laid out 400 px wide with `spacing`.
    fn paragraph_tops(spacing: proava_application::TextSpacing) -> (Vec<f32>, f32) {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        crate::rich_editor::set_text_spacing(&ctx, spacing);
        let mut editor = RichEditor::new("abstand");
        let mut text = RichText::new(
            [
                "Schaltschrank einschließlich Schaltschranksockel, geeignet als Kabelrangierraum, \
                 mindestens zweiseitig zugänglich, mit Befestigung für Zugentlastungen.",
                "",
                "Art: Wand- oder Standschrank",
                "Platzreserve: 20 %",
                "Tiefe: max. 400 mm",
            ]
            .into_iter()
            .map(proava_domain::TextBlock::text)
            .collect(),
        );
        let palette = crate::theme::Palette::of(proava_application::ThemeChoice::Light);
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 800.0),
                )),
                ..egui::RawInput::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                editor.show(ui, &palette, &mut text, &[]);
            });
            output.textures_delta.clear();
        }
        let tops = editor
            .flow
            .layouts
            .iter()
            .map(|layout| layout.as_ref().expect("Absatz").rect.top())
            .collect();
        let first = &editor.flow.layouts[0].as_ref().unwrap().galley;
        assert!(first.rows.len() > 1, "erster Absatz bricht um");
        let pitch = first.rows[1].rect().top() - first.rows[0].rect().top();
        (tops, pitch)
    }

    /// Short lines as paragraphs of their own must not stand further apart
    /// than the lines of a paragraph. Default (as AVA printouts): the
    /// same distance, an empty paragraph is one empty line; Abstand nach
    /// Absatz and Zeilenabstand follow the setting (Editor = Druck).
    #[test]
    fn paragraphs_stand_like_lines_unless_a_gap_is_set() {
        use proava_application::{LineSpacing, ParagraphGap, TextSpacing};
        let (tops, row) = paragraph_tops(TextSpacing::default());
        let gaps: Vec<f32> = tops.windows(2).map(|pair| pair[1] - pair[0]).collect();
        // Block 0 has several rows; from block 1 on (empty, three short lines).
        assert_eq!(gaps[1], row, "Leerabsatz = eine Zeile: {gaps:?}");
        assert_eq!(gaps[2], row, "kurze Absätze wie Zeilen: {gaps:?}");
        assert_eq!(gaps[3], row, "{gaps:?}");

        let (tops, same_row) = paragraph_tops(TextSpacing {
            line: LineSpacing::Single,
            gap: ParagraphGap::Points6,
        });
        assert_eq!(same_row, row, "Zeilen im Absatz bleiben");
        let gap = (6.0 * super::super::PX_PER_PT).round();
        assert_eq!(tops[3] - tops[2], row + gap, "6 pt nach jedem Absatz");

        let (tops, wide) = paragraph_tops(TextSpacing {
            line: LineSpacing::OneHalf,
            gap: ParagraphGap::None,
        });
        // Whole pixels: the font's line height is rounded either way.
        assert!(
            (wide - row * 1.5).abs() <= 1.0,
            "1,5-fach: {wide} bei {row}"
        );
        assert_eq!(tops[3] - tops[2], wide, "Absätze im selben Abstand");

        let (tops, line) = paragraph_tops(TextSpacing {
            line: LineSpacing::OneFifteen,
            gap: ParagraphGap::HalfLine,
        });
        assert!(
            (line - row * 1.15).abs() <= 1.0,
            "1,15-fach: {line} bei {row}"
        );
        let half = (0.5 * 1.15 * 1.15 * 10.0 * super::super::PX_PER_PT).round();
        assert_eq!(
            tops[3] - tops[2],
            line + half,
            "halbe Zeile nach dem Absatz"
        );
    }

    #[test]
    fn select_all_with_an_empty_first_paragraph_paints() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("leer");
        let mut text = RichText::plain("\nzweiter Absatz");
        editor.request_focus();
        frame(&ctx, &mut editor, &mut text, vec![]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        let select_all = egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        frame(&ctx, &mut editor, &mut text, vec![select_all]);
        frame(&ctx, &mut editor, &mut text, vec![]);
        assert!(editor.flow.has_selection());
    }

    #[test]
    fn caret_blinks_and_is_visible_after_a_move() {
        assert_eq!(blink_phase(0.0, 0.5, 0.5), (true, 0.5));
        let (visible, wake) = blink_phase(0.7, 0.5, 0.5);
        assert!(!visible && (wake - 0.3).abs() < 1e-4);
        assert!(blink_phase(1.1, 0.5, 0.5).0, "nächster Zyklus");
    }
}
