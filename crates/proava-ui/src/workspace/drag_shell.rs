//! Dragging rows between two proAVA windows (see `crate::window_drag`):
//! this window as the one the drag starts in (rows of the LV table, of the
//! window „Übernehmen“ or of the side window „Bibliothek“) and as the one the
//! rows are let go over (mark at the row under the pointer, insert there the
//! way of Strg+V, one Strg+Z). Between windows rows are always copied. This
//! is the user's own work: it goes also when assistants may not work in the
//! window.

use std::sync::Arc;

use eframe::egui;
use proava_application::window_bridge::{BridgeHandle, Incoming};
use proava_bridge_protocol::{
    window_at, Command, DragDrop, DragOver, DropTarget, Reply, WindowEntry,
};
use proava_domain::transfer::{self, Transfer, TransferOptions};
use proava_domain::RowId;

use super::bridge_shell::{InsertPlace, Inserted};
use super::{Tab, WorkspaceScreen};
use crate::bridge::PendingInsert;
use crate::transfer_view::{TransferDrag, TransferView};
use crate::window_drag::{
    desktop_point, window_point, Kind, Messages, ENTRIES_EVERY, OVER_EVERY, OVER_FORGET,
    OVER_REPEAT,
};

/// How long the hint „Zwischen Fenstern ziehen: Strg+C/Strg+V“ stays (seconds).
const HINT_SECONDS: f64 = 5.0;

/// A drag that left this window.
pub(crate) struct DragOut {
    id: u64,
    data: String,
    summary: String,
    origin: String,
    messages: Messages,
    /// The window under the pointer (its entry).
    target: Option<WindowEntry>,
    point: [f32; 2],
    last_sent: f64,
    entries: Vec<WindowEntry>,
    entries_at: f64,
    /// Let go over another window: waiting for its answer.
    released: bool,
}

/// A drag of another window over this one.
pub(crate) struct DragIn {
    id: u64,
    summary: String,
    origin: String,
    /// Pointer in points of this window.
    point: egui::Pos2,
    seen: f64,
}

/// Numbers of drags (unique with the PID of the window).
static NEXT_DRAG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl WorkspaceScreen {
    /// The local bridge of the window (None without one): for dragging rows
    /// into other windows.
    pub(crate) fn set_bridge(&mut self, handle: Option<BridgeHandle>) {
        self.bridge = handle;
    }

    /// Where a drag from another window would land (row, below it) – for tests.
    pub fn drag_mark(&self) -> Option<(RowId, bool)> {
        self.drag_in
            .as_ref()
            .and_then(|_| self.editor.remote_drop_place())
    }

    /// Whether rows of this window are being dragged into another one (tests).
    pub fn dragging_out(&self) -> bool {
        self.drag_out.as_ref().is_some_and(|drag| !drag.released)
    }

    /// Before the views: a drag leaving this window, and the mark of a drag
    /// from another window for the table.
    pub(super) fn drag_frame_start(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        self.drag_out_frame(ctx, now);
        if self
            .drag_in
            .as_ref()
            .is_some_and(|drag| now - drag.seen > OVER_FORGET)
        {
            self.drag_in = None;
        }
        let pointer = self
            .drag_in
            .as_ref()
            .filter(|_| self.tab == Tab::Lv)
            .map(|drag| drag.point);
        self.editor.set_remote_pointer(pointer);
        if self.drag_in.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// After the views: the answer of a drop that waited for a question, and
    /// the hint at the pointer of a drag from another window.
    pub(super) fn drag_frame_end(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        if let Some(pending) = self.drag_pending.take() {
            self.drag_pending = self.settle_insert(ctx, now, pending, false);
        }
        if let Some(drag) = &self.drag_in {
            let (text, possible) = self.drop_preview(drag.point);
            let palette = self.palette;
            let line = format!(
                "{} aus {}\n{text}",
                drag.summary,
                if drag.origin.is_empty() {
                    "einem anderen Fenster"
                } else {
                    drag.origin.as_str()
                }
            );
            egui::Area::new(egui::Id::unique("proava_drag_from_window"))
                .order(egui::Order::Tooltip)
                .fixed_pos(drag.point + egui::vec2(18.0, 18.0))
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(egui::RichText::new(line).size(12.5).color(if possible {
                            palette.text
                        } else {
                            palette.warn
                        }));
                    });
                });
        }
        if let Some((pos, since)) = self.drag_hint {
            if now - since > HINT_SECONDS {
                self.drag_hint = None;
            } else {
                let palette = self.palette;
                let screen = ctx.content_rect();
                let place = egui::pos2(
                    pos.x.clamp(screen.left() + 8.0, screen.right() - 330.0),
                    pos.y.clamp(screen.top() + 8.0, screen.bottom() - 40.0),
                );
                egui::Area::new(egui::Id::unique("proava_drag_hint"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(place)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Zwischen Fenstern ziehen: Strg+C/Strg+V")
                                    .size(12.5)
                                    .color(palette.text),
                            );
                        });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }

    // ----- dieses Fenster als Quelle -----

    fn drag_out_frame(&mut self, ctx: &egui::Context, now: f64) {
        self.drag_answers(ctx);
        let Some(handle) = self.bridge.clone() else {
            return;
        };
        let (pointer, down, inside, pixels_per_point, escape) = ctx.input(|input| {
            (
                input.pointer.latest_pos(),
                input.pointer.primary_down(),
                input.viewport().inner_rect,
                input.pixels_per_point(),
                input.key_pressed(egui::Key::Escape),
            )
        });
        let screen = ctx.content_rect();
        let outside = pointer.is_none_or(|pos| !screen.contains(pos));
        let payload = egui::DragAndDrop::payload::<TransferDrag>(ctx);
        let rows_dragged =
            self.tab == Tab::Lv && self.editor.dragged_rows(self.project.lv()).is_some();
        let dragging = rows_dragged || payload.is_some();

        // Released (or Esc): an open drag ends.
        if self.drag_out.as_ref().is_some_and(|drag| !drag.released) {
            if escape {
                ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                self.end_drag_out(false);
                self.editor.cancel_drag();
                egui::DragAndDrop::clear_payload(ctx);
                self.say(ctx, "Ziehen abgebrochen.");
                return;
            }
            if !down {
                self.let_go(ctx, outside);
                return;
            }
        }
        if !dragging || !down {
            return;
        }
        if !outside {
            // Back in this window: the other one forgets its mark.
            if let Some(drag) = self.drag_out.as_mut() {
                if let Some(target) = drag.target.take() {
                    drag.messages.send(
                        Kind::End,
                        &target,
                        Command::ZiehenEnde { kennung: drag.id },
                    );
                }
            }
            return;
        }
        let Some(inside) = inside else {
            // Wayland: the system does not tell where the windows are.
            if self.drag_hint.is_none() {
                self.drag_hint = Some((pointer.unwrap_or(screen.center()), now));
            }
            return;
        };
        if self.drag_out.is_none() {
            let Some(drag) = self.start_drag_out(ctx, &handle, payload.as_deref()) else {
                return;
            };
            self.drag_out = Some(drag);
        }
        let Some(drag) = self.drag_out.as_mut() else {
            return;
        };
        if let Some(pointer) = pointer {
            drag.point = desktop_point(inside, pointer, pixels_per_point);
        }
        if now - drag.entries_at > ENTRIES_EVERY {
            drag.entries = (handle.windows)();
            drag.entries_at = now;
        }
        let target = window_at(&drag.entries, handle.pid, drag.point).cloned();
        let mut nothing_below = false;
        if target.as_ref().map(|entry| entry.pid) != drag.target.as_ref().map(|entry| entry.pid) {
            if let Some(old) = drag.target.take() {
                drag.messages
                    .send(Kind::End, &old, Command::ZiehenEnde { kennung: drag.id });
            }
            drag.target = target;
            drag.last_sent = f64::NEG_INFINITY;
            nothing_below = drag.target.is_none();
        }
        if let Some(target) = &drag.target {
            if now - drag.last_sent >= OVER_EVERY {
                drag.messages.send(
                    Kind::Over,
                    target,
                    Command::ZiehenUeber(DragOver {
                        kennung: drag.id,
                        punkt_px: drag.point,
                        zusammenfassung: drag.summary.clone(),
                        herkunft: drag.origin.clone(),
                    }),
                );
                drag.last_sent = now;
            }
        }
        // Without movement there are no frames: send the position again now
        // and then, so the other window keeps its mark and scrolls along.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(OVER_REPEAT));
        if nothing_below {
            self.say(
                ctx,
                "Kein proAVA-Fenster unter dem Zeiger – loslassen bricht ab.",
            );
        }
    }

    /// The rows of the drag as proAVA clipboard text (as Strg+C).
    fn start_drag_out(
        &mut self,
        ctx: &egui::Context,
        handle: &BridgeHandle,
        payload: Option<&TransferDrag>,
    ) -> Option<DragOut> {
        let taken: Transfer = match payload {
            Some(payload) if payload.from_library => self.library_drop(ctx)?,
            Some(_) => self.transfer.as_ref().and_then(TransferView::transfer)?,
            None => {
                let lv = self.project.lv();
                let ids = self.editor.dragged_rows(lv)?;
                let options = TransferOptions {
                    quantities: true,
                    prices: true,
                };
                transfer::collect(&self.project, lv, &ids, options)
            }
        };
        if taken.rows.is_empty() {
            return None;
        }
        let clip = proava_application::Clip::Rows(taken);
        let header = match self
            .launcher
            .clipboard_header(&clip, self.project.lv().oz_mask())
        {
            Ok((header, _)) => header,
            Err(error) => {
                self.say(
                    ctx,
                    format!("Ziehen in ein anderes Fenster geht nicht: {error}"),
                );
                return None;
            }
        };
        let wake_ctx = ctx.clone();
        let messages = Messages::start(
            handle.messenger.clone(),
            Arc::new(move || wake_ctx.request_repaint()),
        );
        let id = (u64::from(handle.pid) << 24)
            | NEXT_DRAG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(DragOut {
            id,
            data: header.line(),
            summary: header.summary,
            origin: self.project.name().as_str().to_string(),
            messages,
            target: None,
            point: [0.0, 0.0],
            last_sent: f64::NEG_INFINITY,
            entries: (handle.windows)(),
            entries_at: ctx.input(|input| input.time),
            released: false,
        })
    }

    /// The mouse button went up during a drag that had left this window.
    fn let_go(&mut self, ctx: &egui::Context, outside: bool) {
        let Some(drag) = self.drag_out.as_mut() else {
            return;
        };
        match (&drag.target, outside) {
            (Some(target), true) => {
                let name = target_name(target);
                drag.messages.send(
                    Kind::Drop,
                    target,
                    Command::Ablegen(DragDrop {
                        kennung: drag.id,
                        punkt_px: drag.point,
                        daten: drag.data.clone(),
                        herkunft: drag.origin.clone(),
                    }),
                );
                drag.released = true;
                let summary = drag.summary.clone();
                self.say(ctx, format!("{summary} werden in {name} eingefügt …"));
            }
            (_, false) => self.end_drag_out(false),
            (None, true) => {
                self.end_drag_out(false);
                self.say(
                    ctx,
                    "Ziehen abgebrochen – kein proAVA-Fenster unter dem Zeiger.",
                );
            }
        }
    }

    /// Ends the drag; the other window forgets its mark (not after a drop).
    fn end_drag_out(&mut self, dropped: bool) {
        if let Some(mut drag) = self.drag_out.take() {
            if !dropped {
                if let Some(target) = drag.target.take() {
                    drag.messages.send(
                        Kind::End,
                        &target,
                        Command::ZiehenEnde { kennung: drag.id },
                    );
                }
            }
        }
    }

    /// What the other window answered.
    fn drag_answers(&mut self, ctx: &egui::Context) {
        let Some(drag) = self.drag_out.as_ref() else {
            return;
        };
        let name = drag.target.as_ref().map(target_name).unwrap_or_default();
        let released = drag.released;
        let mut messages = Vec::new();
        let mut finished = false;
        for answer in drag.messages.answers() {
            match (answer.kind, answer.result) {
                (Kind::Over, Ok(Reply::Ziel(DropTarget { text, .. }))) if !released => {
                    messages.push(format!("Ziehen nach {name}: {text}"));
                }
                (Kind::Drop, Ok(Reply::Erledigt(done))) => {
                    messages.push(format!("{name}: {}", done.text));
                    finished = true;
                }
                (Kind::Drop, Ok(reply)) => {
                    messages.push(format!(
                        "{name}: {}",
                        reply.refusal().unwrap_or("nicht eingefügt")
                    ));
                    finished = true;
                }
                (Kind::Drop, Err(error)) => {
                    messages.push(format!("Ablegen in {name} ging nicht: {error}"));
                    finished = true;
                }
                (Kind::Over, Err(error)) if !released => {
                    messages.push(format!("{name} antwortet nicht: {error}"));
                }
                _ => {}
            }
        }
        if let Some(text) = messages.pop() {
            self.say(ctx, text);
        }
        if finished {
            self.drag_out = None;
        }
    }

    // ----- dieses Fenster als Ziel -----

    /// `ziehen_ueber`, `ziehen_ende` and `ablegen` from another window.
    pub(crate) fn drag_command(&mut self, ctx: &egui::Context, incoming: Incoming) {
        let now = ctx.input(|input| input.time);
        ctx.request_repaint();
        match incoming.request.befehl.clone() {
            Command::ZiehenUeber(over) => {
                let Some(point) = self.local_point(ctx, over.punkt_px) else {
                    incoming.answer(Reply::Ziel(DropTarget {
                        text: "die Lage dieses Fensters ist nicht bekannt".to_string(),
                        moeglich: false,
                    }));
                    return;
                };
                self.drag_in = Some(DragIn {
                    id: over.kennung,
                    summary: over.zusammenfassung,
                    origin: over.herkunft,
                    point,
                    seen: now,
                });
                let (text, possible) = self.drop_preview(point);
                incoming.answer(Reply::Ziel(DropTarget {
                    text,
                    moeglich: possible,
                }));
            }
            Command::ZiehenEnde { kennung } => {
                if self.drag_in.as_ref().is_some_and(|drag| drag.id == kennung) {
                    self.drag_in = None;
                    self.editor.set_remote_pointer(None);
                }
                incoming.answer(Reply::Ziel(DropTarget::default()));
            }
            Command::Ablegen(drop) => {
                self.drag_in = None;
                self.editor.set_remote_pointer(None);
                match self.drop_rows(ctx, &drop) {
                    Ok((rows, text)) => {
                        self.drag_pending = Some(PendingInsert {
                            incoming,
                            rows,
                            step: None,
                            text,
                            since: now,
                        });
                    }
                    Err(error) => {
                        self.say(ctx, format!("Nicht eingefügt: {error}"));
                        incoming.answer(Reply::refused(error));
                    }
                }
            }
            _ => incoming.answer(Reply::refused("Kein Befehl zum Ziehen.")),
        }
    }

    /// A point of the desktop (physical pixels) in points of this window.
    fn local_point(&self, ctx: &egui::Context, desktop: [f32; 2]) -> Option<egui::Pos2> {
        let (inside, pixels_per_point) =
            ctx.input(|input| (input.viewport().inner_rect, input.pixels_per_point()));
        inside.map(|inside| window_point(inside, desktop, pixels_per_point))
    }

    /// What letting go at `point` would do here, and whether it inserts.
    fn drop_preview(&self, point: egui::Pos2) -> (String, bool) {
        if self.read_only {
            return (
                "schreibgeschützt geöffnet – hier wird nichts eingefügt".to_string(),
                false,
            );
        }
        if let Some(dialog) = self.dialog_title() {
            return (format!("Dialog offen („{dialog}“) – erst schließen"), false);
        }
        if self.tab != Tab::Lv {
            return ("einfügen nur im Reiter LV".to_string(), false);
        }
        match self.editor.row_at(self.project.lv(), point) {
            Some((row, after)) => (
                format!(
                    "loslassen fügt {} {} ein (Kopie)",
                    if after { "hinter" } else { "vor" },
                    self.row_label(row)
                ),
                true,
            ),
            None => ("über eine Zeile der Tabelle ziehen".to_string(), false),
        }
    }

    /// „01.482.0010“, „dem Hinweis „Vorbemerkung““.
    fn row_label(&self, id: RowId) -> String {
        let lv = self.project.lv();
        let Some(row) = lv.row_by_id(id) else {
            return "der Zeile".to_string();
        };
        if !row.ordinal().is_empty() {
            return row.ordinal().to_string();
        }
        let text = row.short_text();
        if text.trim().is_empty() {
            "der Zeile".to_string()
        } else {
            format!("„{}“", text.trim())
        }
    }

    /// Inserts dropped rows at the row under the drop point (one Strg+Z).
    fn drop_rows(
        &mut self,
        ctx: &egui::Context,
        drop: &DragDrop,
    ) -> Result<(Vec<RowId>, String), String> {
        if self.tab != Tab::Lv {
            return Err("Einfügen nur im Reiter LV.".to_string());
        }
        let point = self
            .local_point(ctx, drop.punkt_px)
            .ok_or("Die Lage dieses Fensters ist nicht bekannt.")?;
        let lv = self.project.lv();
        let (row, after) = self
            .editor
            .row_at(lv, point)
            .or_else(|| self.editor.remote_drop_place())
            .ok_or("Nicht über einer Zeile der Tabelle losgelassen.")?;
        let index = lv.index_of(row).ok_or("Die Zeile gibt es nicht mehr.")?;
        let place = if after {
            Some(index)
        } else {
            index.checked_sub(1)
        };
        let Inserted {
            rows,
            summary,
            place,
        } = self.insert_clip(ctx, &drop.daten, None, InsertPlace::Drop(place))?;
        let lv = self.project.lv();
        self.editor.select_rows(lv, &rows);
        let origin = drop.herkunft.trim();
        let from = if origin.is_empty() {
            String::new()
        } else {
            format!(" aus {origin}")
        };
        self.say(
            ctx,
            format!("{summary}{from} eingefügt{place} – Strg+Z nimmt es zurück."),
        );
        Ok((
            rows,
            format!("{summary} eingefügt{place} (Strg+Z dort nimmt es zurück)."),
        ))
    }
}

/// „Rathaus 3“ (project of a window), else „PID 4711“.
fn target_name(entry: &WindowEntry) -> String {
    entry
        .fenster
        .projekt
        .as_ref()
        .map(|project| project.name.clone())
        .unwrap_or_else(|| format!("PID {}", entry.pid))
}
