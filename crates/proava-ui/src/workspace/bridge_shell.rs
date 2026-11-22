//! The workspace side of the local bridge: an assistant shows, marks, reads,
//! copies, searches and inserts rows in this window – through the same
//! editing as by hand (selection, Strg+V, one undo step), with the
//! KI-Markierung and the line „KI: …“ in the status bar so the user sees what
//! happens. Read-only windows, finished positions and sent LVs behave as
//! with a person at the keys: questions appear in the window, the answer
//! waits for the user.
//!
//! Every other MCP tool works in the window too: `stand` hands the whole
//! project (also unsaved) to the MCP server as a file in the exchange
//! folder, the tool runs on it as on the project file, and `uebernehmen`
//! takes its result over as one undo step – only while the project in the
//! window is still the same. `speichern` is Strg+S.

use std::collections::HashSet;

use eframe::egui;
use proava_application::window_bridge::{
    self as bridge, find_row, hit_info, lv_index, pick_rows, row_info, without_contained, Incoming,
    QUESTION_PATIENCE,
};
use proava_bridge_protocol::{
    Adopt, ClipReply, Command, Done, Insert, Mark, ProjectInfo, Read, Reply, RowInfo, RowsReply,
    Search, Show, StateReply, Take, MARK_SECONDS, MARK_SECONDS_MAX,
};
use proava_domain::lv_search::{self, LvQuery};
use proava_domain::transfer::{self, TransferOptions};
use proava_domain::RowId;

use super::{Dialog, Tab, WorkspaceScreen};
use crate::bridge::PendingInsert;
use crate::history::ProjectChange;

/// How long the mark stays after copying or inserting (seconds).
const AFTER_CHANGE_SECONDS: f64 = 30.0;
/// At most so many rows in an answer of „lesen“.
const MAX_READ: usize = 200;

impl WorkspaceScreen {
    /// The project as the assistant sees it (`fenster_lesen`).
    pub(crate) fn bridge_info(&self) -> ProjectInfo {
        let lv = self.project.lv();
        let active = self.project.active_lv();
        let selection = self
            .editor
            .selection_in_order(lv)
            .into_iter()
            .take(50)
            .filter_map(|id| lv.index_of(id))
            .map(|index| row_info(lv, index, false))
            .collect();
        ProjectInfo {
            name: self.project.name().as_str().to_string(),
            datei: self.project_file.to_string_lossy().into_owned(),
            lv: bridge::lv_info(&self.project, active),
            lvs: (0..self.project.lv_count())
                .map(|index| bridge::lv_info(&self.project, index))
                .collect(),
            ungespeichert: self.dirty,
            schreibgeschuetzt: self.read_only,
            reiter: tab_name(self.tab).to_string(),
            auswahl: selection,
            versandt: self.lv_sent(),
        }
    }

    /// The project file of this window.
    pub fn project_file(&self) -> &std::path::Path {
        &self.project_file
    }

    /// Rows with the KI-Markierung (for tests).
    pub fn ai_marked_rows(&self) -> HashSet<RowId> {
        self.ai.rows.clone()
    }

    /// What the assistant is doing (line in the status bar), if anything.
    pub(crate) fn ai_activity(&self) -> Option<String> {
        self.ai.text.clone()
    }

    /// The title of an open dialog (it waits for the user).
    pub(crate) fn dialog_title(&self) -> Option<String> {
        let title = match &self.dialog {
            None if self.editor.has_dialog() => "Rückfrage in der LV-Tabelle",
            None => return None,
            Some(Dialog::ChangeQuestion(question)) => {
                return Some(format!("Rückfrage: {}", question.title));
            }
            Some(Dialog::Leave { .. }) => "Projekt verlassen",
            Some(Dialog::Recover { .. }) => "Wiederherstellung anbieten",
            Some(Dialog::PasteLv { .. }) => "Ganzes LV einfügen",
            Some(Dialog::Settings(_)) => "Einstellungen",
            Some(Dialog::Print(_)) => "Drucken",
            Some(Dialog::File { .. }) => "Dateiauswahl",
            Some(Dialog::Palette { .. }) => "Befehlssuche",
            Some(_) => "Dialog",
        };
        Some(title.to_string())
    }

    /// One request of an assistant (on the UI thread). Answers at once, or
    /// later for an insert (end of this frame, or when the user answered a
    /// question of proAVA).
    pub(crate) fn bridge_command(&mut self, ctx: &egui::Context, incoming: Incoming) {
        let now = ctx.input(|input| input.time);
        self.ai.last_request = Some(now);
        ctx.request_repaint();
        if let Some(refusal) = self.ai.stopped(now) {
            incoming.answer(refusal);
            return;
        }
        let command = incoming.request.befehl.clone();
        let reply = match command {
            Command::Zeigen(show) => self.bridge_show(now, &show),
            Command::Markieren(mark) => self.bridge_mark(now, &mark),
            Command::Lesen(read) => self.bridge_read(&read),
            Command::Kopieren(take) => self.bridge_take(now, &take),
            Command::Suchen(search) => self.bridge_search(now, &search),
            Command::Einfuegen(insert) => match self.bridge_insert(ctx, now, &insert) {
                Ok((rows, text)) => {
                    self.ai.pending = Some(PendingInsert {
                        incoming,
                        rows,
                        step: None,
                        text,
                        since: now,
                    });
                    return;
                }
                Err(error) => Reply::refused(error),
            },
            Command::Stand => self.bridge_state(),
            Command::Uebernehmen(adopt) => match self.bridge_adopt(ctx, now, &adopt) {
                Ok((rows, text, Some(step))) => {
                    self.ai.pending = Some(PendingInsert {
                        incoming,
                        rows,
                        step: Some(step),
                        text,
                        since: now,
                    });
                    return;
                }
                Ok((rows, text, None)) => Reply::Erledigt(Done {
                    text,
                    zeilen: rows_of(self.project.lv(), &rows),
                    wartet: false,
                }),
                Err(error) => Reply::refused(error),
            },
            Command::Speichern => self.bridge_save(ctx),
            Command::FensterLesen
            | Command::ProjektOeffnen(_)
            | Command::ZiehenUeber(_)
            | Command::ZiehenEnde { .. }
            | Command::Ablegen(_) => Reply::refused("Dieser Befehl gilt dem ganzen Fenster."),
        };
        incoming.answer(reply);
    }

    /// Before the views: expiry, Esc and the marks for the table.
    pub(super) fn bridge_frame_start(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        if self.ai.visible() && self.ai.pending.is_none() && now >= self.ai.until {
            self.ai.clear();
        }
        // A change taken over was taken back (Strg+Z): mark and line go.
        if self
            .ai
            .step
            .is_some_and(|step| self.ai.pending.is_none() && !self.history.holds_whole(step))
        {
            self.ai.clear();
        }
        // The marked rows are gone (Strg+Z, deleted, other LV): the line too.
        if !self.ai.rows.is_empty()
            && self.ai.pending.is_none()
            && !self
                .ai
                .rows
                .iter()
                .any(|id| self.project.lv().index_of(*id).is_some())
        {
            self.ai.clear();
        }
        // Esc stops the assistant (only while it shows something and no
        // dialog or cell edit has the key).
        if (self.ai.visible() || self.ai.pending.is_some())
            && self.dialog.is_none()
            && !self.editor.has_dialog()
            && !self.editor.cell_editing()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.ai.stop(now);
            self.say(ctx, "KI-Aktion abgebrochen.");
        }
        self.editor.set_ai_marks(self.ai.rows.clone());
        if self.ai.visible() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (self.ai.until - now).clamp(0.05, 1.0),
            ));
        }
    }

    /// After the views and the question after sending: the answer of a
    /// waiting insert.
    pub(super) fn bridge_frame_end(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        if let Some(pending) = self.ai.pending.take() {
            self.bridge_settle(ctx, now, pending);
        }
        self.editor.set_ai_marks(self.ai.rows.clone());
    }

    /// Answers a done insert once no question of proAVA is open (or after
    /// [`QUESTION_PATIENCE`] with „wartet auf den Anwender“).
    fn bridge_settle(&mut self, ctx: &egui::Context, now: f64, pending: PendingInsert) {
        if let Some(waiting) = self.settle_insert(ctx, now, pending, true) {
            self.ai.pending = Some(waiting);
        }
    }

    /// The answer of an insert (assistant or drop from another window) once
    /// no question of proAVA is open; Some = still waiting. `ai`: the line
    /// „KI: …“ says that it waits.
    pub(super) fn settle_insert(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        pending: PendingInsert,
        ai: bool,
    ) -> Option<PendingInsert> {
        let lv = self.project.lv();
        let still: Vec<RowId> = pending
            .rows
            .iter()
            .copied()
            .filter(|id| lv.index_of(*id).is_some())
            .collect();
        // A change taken over is kept while its undo step is done (also
        // without rows: deleting, Stammdaten …).
        let kept = match pending.step {
            Some(step) => self.history.holds_whole(step),
            None => !still.is_empty(),
        };
        if let Some(question) = self.dialog_title().filter(|_| kept) {
            if now - pending.since < QUESTION_PATIENCE.as_secs_f64() {
                if ai {
                    self.ai.text = Some(format!("wartet auf den Anwender – {question}"));
                    self.ai.until = self.ai.until.max(now + 1.0);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
                return Some(pending);
            }
            pending.incoming.answer(Reply::Erledigt(Done {
                text: format!(
                    "{} proAVA fragt den Anwender im Fenster („{question}“) – wartet auf den Anwender.",
                    pending.text
                ),
                zeilen: rows_of(lv, &still),
                wartet: true,
            }));
            return None;
        }
        if !kept {
            pending.incoming.answer(Reply::refused(format!(
                "{} Der Anwender hat es gleich wieder zurückgenommen (Rückfrage im Fenster).",
                pending.text
            )));
            if ai {
                self.ai.clear();
            }
            return None;
        }
        pending.incoming.answer(Reply::Erledigt(Done {
            text: pending.text.clone(),
            zeilen: rows_of(lv, &still),
            wartet: false,
        }));
        None
    }

    /// Back to the LV table and to the LV named in `lv`.
    fn bridge_lv(&mut self, lv: Option<&str>) -> Result<(), String> {
        let index = lv_index(&self.project, lv)?;
        if index != self.project.active_lv() {
            self.activate_lv(index);
        }
        self.tab = Tab::Lv;
        Ok(())
    }

    fn bridge_show(&mut self, now: f64, show: &Show) -> Reply {
        if let Err(error) = self.bridge_lv(show.zeilen.lv.as_deref()) {
            return Reply::refused(error);
        }
        let lv = self.project.lv();
        let selection = self.editor.selection_in_order(lv);
        let picked = match pick_rows(lv, &show.zeilen, &selection) {
            Ok(picked) => picked,
            Err(error) => return Reply::refused(error),
        };
        let ids: Vec<RowId> = picked.iter().map(|&index| lv.rows()[index].id()).collect();
        let rows = rows_of(lv, &ids);
        self.editor.select_rows(lv, &ids);
        if show.positionsfenster {
            self.editor.show_position_window(ids[0]);
        }
        let text = show
            .markierung
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| format!("zeigt {}", describe(&rows)));
        self.ai.show(
            now,
            text.clone(),
            ids.iter().copied().collect(),
            f64::from(MARK_SECONDS),
        );
        Reply::Zeilen(RowsReply {
            lv: Some(bridge::lv_info(&self.project, self.project.active_lv())),
            gesamt: rows.len(),
            zeilen: rows,
            text: format!("Im Fenster gewählt und markiert ({text})."),
        })
    }

    fn bridge_mark(&mut self, now: f64, mark: &Mark) -> Reply {
        if mark.aufheben {
            self.ai.clear();
            return Reply::Erledigt(Done {
                text: "Markierung aufgehoben.".to_string(),
                ..Done::default()
            });
        }
        if let Err(error) = self.bridge_lv(mark.zeilen.lv.as_deref()) {
            return Reply::refused(error);
        }
        let lv = self.project.lv();
        let ids: Vec<RowId> = if mark.zeilen.is_empty() {
            Vec::new()
        } else {
            let selection = self.editor.selection_in_order(lv);
            match pick_rows(lv, &mark.zeilen, &selection) {
                Ok(picked) => picked.iter().map(|&index| lv.rows()[index].id()).collect(),
                Err(error) => return Reply::refused(error),
            }
        };
        let seconds = mark
            .sekunden
            .unwrap_or(MARK_SECONDS)
            .clamp(1, MARK_SECONDS_MAX);
        let text = if mark.text.trim().is_empty() {
            format!("markiert {}", describe(&rows_of(lv, &ids)))
        } else {
            mark.text.trim().to_string()
        };
        if let Some(&first) = ids.first() {
            // Into view, without changing the selection.
            self.editor.scroll_to_row(lv, first);
        }
        let rows = rows_of(lv, &ids);
        self.ai
            .show(now, text, ids.into_iter().collect(), f64::from(seconds));
        Reply::Zeilen(RowsReply {
            lv: Some(bridge::lv_info(&self.project, self.project.active_lv())),
            gesamt: rows.len(),
            zeilen: rows,
            text: format!("Markiert für {seconds} s."),
        })
    }

    fn bridge_read(&mut self, read: &Read) -> Reply {
        let index = match lv_index(&self.project, read.zeilen.lv.as_deref()) {
            Ok(index) => index,
            Err(error) => return Reply::refused(error),
        };
        let Some(lv) = self.project.lv_at(index) else {
            return Reply::refused("LV nicht gefunden.");
        };
        let indices: Vec<usize> = if let Some(start) = &read.ab {
            let first = match find_row(lv, start) {
                Ok(first) => first,
                Err(error) => return Reply::refused(error),
            };
            let count = read.anzahl.unwrap_or(20).clamp(1, MAX_READ);
            (first..lv.rows().len()).take(count).collect()
        } else {
            let mut rows = read.zeilen.clone();
            if rows.is_empty() {
                rows.auswahl = true;
            }
            let selection = if index == self.project.active_lv() {
                self.editor.selection_in_order(lv)
            } else {
                Vec::new()
            };
            match pick_rows(lv, &rows, &selection) {
                Ok(picked) => picked,
                Err(error) => return Reply::refused(error),
            }
        };
        let total = indices.len();
        let rows: Vec<RowInfo> = indices
            .into_iter()
            .take(MAX_READ)
            .map(|index| row_info(lv, index, read.langtext))
            .collect();
        Reply::Zeilen(RowsReply {
            lv: Some(bridge::lv_info(&self.project, index)),
            gesamt: total,
            zeilen: rows,
            text: String::new(),
        })
    }

    fn bridge_take(&mut self, now: f64, take: &Take) -> Reply {
        if let Err(error) = self.bridge_lv(take.zeilen.lv.as_deref()) {
            return Reply::refused(error);
        }
        let lv = self.project.lv();
        let selection = self.editor.selection_in_order(lv);
        let picked = match pick_rows(lv, &take.zeilen, &selection) {
            Ok(picked) => picked,
            Err(error) => return Reply::refused(error),
        };
        let roots = without_contained(lv, &picked);
        let ids: Vec<RowId> = roots.iter().map(|&index| lv.rows()[index].id()).collect();
        let options = TransferOptions {
            quantities: take.mengen,
            prices: take.preise,
        };
        let taken = transfer::collect(&self.project, lv, &ids, options);
        let clip = proava_application::Clip::Rows(taken);
        let header = match self.launcher.clipboard_header(&clip, lv.oz_mask()) {
            Ok((header, _)) => header,
            Err(error) => return Reply::refused(error),
        };
        let marked: Vec<RowId> = picked.iter().map(|&index| lv.rows()[index].id()).collect();
        let rows = rows_of(lv, &marked);
        let text = take
            .markierung
            .clone()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| format!("kopiert {}", header.summary));
        self.editor.select_rows(lv, &marked);
        self.ai.show(
            now,
            text,
            marked.into_iter().collect(),
            AFTER_CHANGE_SECONDS,
        );
        Reply::Kopie(ClipReply {
            zusammenfassung: header.summary.clone(),
            daten: header.line(),
            lv: bridge::lv_info(&self.project, self.project.active_lv()),
            zeilen: rows,
            projekt: self.project.name().as_str().to_string(),
        })
    }

    fn bridge_search(&mut self, now: f64, search: &Search) -> Reply {
        if let Err(error) = self.bridge_lv(search.lv.as_deref()) {
            return Reply::refused(error);
        }
        let query = LvQuery {
            text: search.suche.clone(),
            cost_group: search.kg.clone(),
            unit: search.einheit.clone(),
        };
        if query.is_empty() {
            return Reply::refused("Bitte suche, kg oder einheit angeben.");
        }
        let lv = self.project.lv();
        let hits = lv_search::search(lv, &query);
        let words = [
            search.suche.trim(),
            &if search.kg.trim().is_empty() {
                String::new()
            } else {
                format!("KG {}", search.kg.trim())
            },
            &if search.einheit.trim().is_empty() {
                String::new()
            } else {
                format!("Einheit {}", search.einheit.trim())
            },
        ]
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(", ");
        let ids: Vec<RowId> = hits.iter().map(|hit| hit.id).collect();
        let count = crate::format::count(hits.len(), "Treffer", "Treffer");
        if ids.is_empty() {
            self.ai.show(
                now,
                format!("keine Treffer für „{words}“"),
                HashSet::new(),
                f64::from(MARK_SECONDS),
            );
        } else {
            self.editor.select_rows(lv, &ids);
            self.ai.show(
                now,
                format!("{count} für „{words}“"),
                ids.iter().copied().collect(),
                f64::from(MARK_SECONDS),
            );
        }
        let limit = search.max_treffer.unwrap_or(50).max(1);
        Reply::Zeilen(RowsReply {
            lv: Some(bridge::lv_info(&self.project, self.project.active_lv())),
            gesamt: hits.len(),
            zeilen: hits
                .iter()
                .take(limit)
                .map(|hit| hit_info(lv, hit))
                .collect(),
            text: format!("{count} für „{words}“ – im Fenster gewählt und markiert."),
        })
    }

    /// `stand`: the whole project as it is now (also unsaved), written to a
    /// file in the exchange folder; the MCP server reads and removes it.
    fn bridge_state(&mut self) -> Reply {
        let Some(folder) = self.exchange_folder() else {
            return Reply::refused("Dieses Fenster hat keinen Austauschordner für die KI-Brücke.");
        };
        if let Err(error) = std::fs::create_dir_all(&folder) {
            return Reply::refused(format!("Austauschordner {}: {error}", folder.display()));
        }
        self.ai.next_number += 1;
        let number = self.ai.next_number;
        let pid = self.bridge.as_ref().map(|handle| handle.pid).unwrap_or(0);
        let file = folder.join(format!("{pid}-stand-{number}.proava"));
        if let Err(error) = self.launcher.save_project(&file, &self.project) {
            let _ = std::fs::remove_file(&file);
            return Reply::refused(format!("Stand nicht geschrieben: {error}"));
        }
        self.ai.lent = Some((number, self.project.clone()));
        Reply::Stand(StateReply {
            datei: file.to_string_lossy().into_owned(),
            stand: number,
            projekt: self.project_file.to_string_lossy().into_owned(),
            ungespeichert: self.dirty,
        })
    }

    fn exchange_folder(&self) -> Option<std::path::PathBuf> {
        self.bridge
            .as_ref()
            .and_then(|handle| handle.exchange.clone())
    }

    /// `uebernehmen`: the project an MCP tool changed on the last `stand`
    /// becomes the project of the window – one undo step, the changed rows
    /// marked, the question of proAVA where a person would be asked.
    /// Returns the rows, the text of the answer and the number of the step
    /// (None: nothing changed).
    fn bridge_adopt(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        adopt: &Adopt,
    ) -> Result<(Vec<RowId>, String, Option<u64>), String> {
        let folder = self
            .exchange_folder()
            .ok_or("Dieses Fenster hat keinen Austauschordner für die KI-Brücke.")?;
        let file = std::path::PathBuf::from(adopt.datei.trim());
        let inside = |path: &std::path::Path| std::fs::canonicalize(path).ok();
        let in_folder = file
            .parent()
            .and_then(inside)
            .is_some_and(|parent| Some(parent) == inside(&folder));
        let project_file = file
            .extension()
            .is_some_and(|extension| extension == "proava");
        if !in_folder || !project_file {
            return Err("datei: nur eine Projektdatei aus dem Austauschordner der Brücke.".into());
        }
        if self.read_only {
            return Err(
                "Das Projekt ist in diesem Fenster schreibgeschützt geöffnet – Änderungen gehen nur in \
                 einem Fenster, das es bearbeitet."
                    .into(),
            );
        }
        if let Some(question) = self.dialog_title() {
            return Err(format!(
                "In diesem Fenster ist ein Dialog offen („{question}“) – er wartet auf den Anwender. \
                 Danach noch einmal."
            ));
        }
        if self.editor.cell_editing() {
            return Err(
                "Der Anwender schreibt gerade in einer Zelle dieses Fensters – danach noch einmal."
                    .into(),
            );
        }
        let same = self.ai.lent.as_ref().is_some_and(|(number, lent)| {
            *number == adopt.stand && lent.same_content(&self.project)
        });
        if !same {
            return Err(
                "Das Projekt wurde im Fenster inzwischen geändert (oder der Stand ist nicht der \
                 neueste) – nichts übernommen. Das Werkzeug bitte noch einmal aufrufen."
                    .into(),
            );
        }
        let (mut changed, _) = self.launcher.read_source(&file)?;
        self.ai.lent = None;
        changed.adopt_session_keys(&self.project);
        let tool = adopt.werkzeug.trim();
        let tool = if tool.is_empty() { "KI" } else { tool };
        if changed.same_content(&self.project) {
            return Ok((Vec::new(), "Im Fenster war nichts zu ändern.".into(), None));
        }
        let lvs: Vec<usize> = (0..changed.lv_count())
            .filter(|&index| !changed.same_lv_entry(index, &self.project, index))
            .collect();
        // The LVs before, for the marks and the question of proAVA.
        let before_lvs: Vec<(usize, proava_domain::Lv)> = lvs
            .iter()
            .filter_map(|&index| Some((index, self.project.lv_at(index)?.clone())))
            .collect();
        self.note_editor();
        let previous = self.active_key();
        let before = self.project.swap_state(changed);
        self.ai.next_number += 1;
        let step = self.ai.next_number;
        self.history.record_project(
            ProjectChange::Whole(step, Box::new(before)),
            "Änderung der KI",
        );
        self.follow_active_lv(previous);
        self.after_costs_step(false);
        self.after_tracking_step(false);
        self.dirty = true;
        // The window shows the LV the tool changed (as by hand).
        let rows_changed: Vec<usize> = lvs
            .iter()
            .copied()
            .filter(|&index| {
                let old = before_lvs
                    .iter()
                    .find(|(known, _)| *known == index)
                    .map(|(_, lv)| lv.rows());
                self.project.lv_at(index).map(|lv| lv.rows()) != old
            })
            .collect();
        if let Some(&first) = rows_changed.first() {
            if first != self.project.active_lv()
                && !rows_changed.contains(&self.project.active_lv())
            {
                self.activate_lv(first);
            }
            self.tab = Tab::Lv;
        }
        let active = self.project.active_lv();
        let old_active = before_lvs
            .iter()
            .find(|(index, _)| *index == active)
            .map(|(_, lv)| lv);
        let (marked, summary) = match old_active {
            Some(old) => change_summary(old, self.project.lv()),
            None => (Vec::new(), "Projekt geändert".to_string()),
        };
        let lv = self.project.lv();
        if !marked.is_empty() {
            self.editor.select_rows(lv, &marked);
        }
        self.ai.show(
            now,
            format!("{tool}: {summary} – Strg+Z nimmt es zurück"),
            marked.iter().copied().collect(),
            AFTER_CHANGE_SECONDS,
        );
        self.ai.step = Some(step);
        let place = if rows_changed.is_empty() {
            String::new()
        } else {
            format!(" in {}", bridge::lv_info(&self.project, active).label())
        };
        let text = format!(
            "Im Fenster übernommen{place}: {summary} – ein Schritt, Strg+Z im Fenster nimmt ihn \
             zurück; nicht gespeichert (fenster_speichern)."
        );
        // The question of proAVA, as after the same change by hand.
        if let Some(old) = old_active {
            let source = before_lvs
                .iter()
                .find(|(index, _)| *index != active && rows_changed.contains(index))
                .map(|(index, lv)| (*index, lv));
            let old = old.clone();
            self.ask_after_drop(ctx, &old, source);
        }
        Ok((marked, text, Some(step)))
    }

    /// `speichern`: like Strg+S.
    fn bridge_save(&mut self, ctx: &egui::Context) -> Reply {
        if let Some(question) = self.dialog_title() {
            return Reply::refused(format!(
                "In diesem Fenster ist ein Dialog offen („{question}“) – er wartet auf den Anwender. \
                 Danach speichern."
            ));
        }
        match self.try_save(ctx) {
            Ok(()) => {
                let now = self.launcher.now_iso();
                let time = now.get(11..16).unwrap_or("");
                Reply::Erledigt(Done {
                    text: format!(
                        "Gespeichert: {} (Stand {}, {time} Uhr).",
                        self.project_file.display(),
                        crate::format::iso_to_german_date(&now)
                    ),
                    ..Done::default()
                })
            }
            Err(error) => Reply::refused(error),
        }
    }

    /// Inserts the rows of `insert` at the cursor (or after `nach`) as
    /// Strg+V does; returns the new rows and the text of the answer.
    fn bridge_insert(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        insert: &Insert,
    ) -> Result<(Vec<RowId>, String), String> {
        let place = match insert
            .nach
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            Some(after) => InsertPlace::After(after.to_string()),
            None => InsertPlace::Cursor,
        };
        let inserted = self.insert_clip(ctx, &insert.daten, insert.lv.as_deref(), place)?;
        let origin = insert.herkunft.trim();
        let Inserted {
            rows: new,
            summary,
            place,
        } = inserted;
        let status = if origin.is_empty() {
            format!("hat {summary} eingefügt{place} – Strg+Z nimmt es zurück")
        } else {
            format!("hat {summary} aus {origin} eingefügt{place} – Strg+Z nimmt es zurück")
        };
        self.ai.show(
            now,
            status,
            new.iter().copied().collect(),
            AFTER_CHANGE_SECONDS,
        );
        let text = format!(
            "{summary} eingefügt{place} in {} (ein Schritt: Strg+Z im Fenster nimmt es zurück; \
             gespeichert wird beim Speichern in proAVA).",
            bridge::lv_info(&self.project, self.project.active_lv()).label()
        );
        Ok((new, text))
    }

    /// Inserts rows given as proAVA clipboard text the way Strg+V does (one
    /// undo step, images, Bieterangaben, AB, KG, STLB) – for the assistant
    /// and for a drag from another window. Read-only windows and open
    /// dialogs refuse, as by hand.
    pub(super) fn insert_clip(
        &mut self,
        ctx: &egui::Context,
        data: &str,
        lv: Option<&str>,
        place: InsertPlace,
    ) -> Result<Inserted, String> {
        if self.read_only {
            return Err(
                "Das Projekt ist in diesem Fenster schreibgeschützt geöffnet – Einfügen geht nur in \
                 einem Fenster, das es bearbeitet."
                    .to_string(),
            );
        }
        if let Some(question) = self.dialog_title() {
            return Err(format!(
                "In diesem Fenster ist ein Dialog offen („{question}“) – er wartet auf den Anwender. \
                 Erst danach einfügen."
            ));
        }
        let Some(header) = proava_application::ClipHeader::find(data) else {
            return Err(
                "daten: keine proAVA-Zeilen (erwartet den Text aus „kopieren“).".to_string(),
            );
        };
        let transfer =
            match self.launcher.clipboard_content(&header) {
                Ok(proava_application::Clip::Rows(transfer)) => transfer,
                Ok(proava_application::Clip::Lv { .. }) => return Err(
                    "daten enthält ein ganzes LV – das fügt der Anwender über die LV-Liste ein."
                        .to_string(),
                ),
                Err(error) => return Err(error),
            };
        self.bridge_lv(lv)?;
        let mut after_index: Option<Option<usize>> = None;
        match &place {
            InsertPlace::Cursor => {}
            InsertPlace::After(after) => {
                let lv = self.project.lv();
                let index = find_row(lv, after)?;
                let id = lv.rows()[index].id();
                self.editor.select(lv, id);
            }
            InsertPlace::Drop(after) => after_index = Some(*after),
        }
        let before: HashSet<RowId> = self
            .project
            .lv()
            .rows()
            .iter()
            .map(|row| row.id())
            .collect();
        self.note_editor();
        let mut output = match after_index {
            Some(after) => self
                .editor
                .paste_transfer_at(&mut self.project, &transfer, after),
            None => self.editor.paste_transfer(&mut self.project, &transfer),
        };
        let error = output.toast.clone();
        let changed = output.dirty;
        if changed {
            // The caller says it (line „KI: …“ or the message of the drop).
            output.toast = None;
        }
        self.take_editor_output(ctx, output);
        let lv = self.project.lv();
        let new: Vec<RowId> = lv
            .rows()
            .iter()
            .map(|row| row.id())
            .filter(|id| !before.contains(id))
            .collect();
        if !changed || new.is_empty() {
            return Err(error.unwrap_or_else(|| "Es wurde nichts eingefügt.".to_string()));
        }
        let place = new
            .first()
            .and_then(|id| lv.index_of(*id))
            .map(|index| match lv.rows()[index].ordinal() {
                "" => String::new(),
                ordinal => format!(" ab {ordinal}"),
            })
            .unwrap_or_default();
        Ok(Inserted {
            rows: new,
            summary: header.summary.clone(),
            place,
        })
    }
}

/// Where rows of [`WorkspaceScreen::insert_clip`] go.
pub(super) enum InsertPlace {
    /// Behind the selected row (Strg+V).
    Cursor,
    /// Behind this OZ or `#id`.
    After(String),
    /// A drop: behind this row index (None = before the first row).
    Drop(Option<usize>),
}

/// What [`WorkspaceScreen::insert_clip`] inserted.
pub(super) struct Inserted {
    pub rows: Vec<RowId>,
    /// „3 Positionen“.
    pub summary: String,
    /// „ ab 01.482.0010“ (empty for rows without OZ).
    pub place: String,
}

/// The rows of `after` that are new or changed against `before` (to mark)
/// and „2 Zeilen geändert, 1 neu, 3 entfernt“.
fn change_summary(before: &proava_domain::Lv, after: &proava_domain::Lv) -> (Vec<RowId>, String) {
    let mut marked = Vec::new();
    let (mut changed, mut new) = (0, 0);
    for row in after.rows() {
        match before.index_of(row.id()) {
            None => {
                new += 1;
                marked.push(row.id());
            }
            Some(index) if before.rows()[index] != *row => {
                changed += 1;
                marked.push(row.id());
            }
            Some(_) => {}
        }
    }
    let removed = before
        .rows()
        .iter()
        .filter(|row| after.index_of(row.id()).is_none())
        .count();
    let mut parts = Vec::new();
    if changed > 0 {
        parts.push(format!(
            "{} geändert",
            crate::format::count(changed, "Zeile", "Zeilen")
        ));
    }
    if new > 0 {
        parts.push(format!(
            "{} neu",
            crate::format::count(new, "Zeile", "Zeilen")
        ));
    }
    if removed > 0 {
        parts.push(format!(
            "{} entfernt",
            crate::format::count(removed, "Zeile", "Zeilen")
        ));
    }
    let summary = if parts.is_empty() {
        "LV geändert".to_string()
    } else {
        parts.join(", ")
    };
    (marked, summary)
}

fn rows_of(lv: &proava_domain::Lv, ids: &[RowId]) -> Vec<RowInfo> {
    ids.iter()
        .filter_map(|id| lv.index_of(*id))
        .map(|index| row_info(lv, index, false))
        .collect()
}

/// „3 Zeilen (01.0010 … 01.0030)“, „Position 01.0010“.
fn describe(rows: &[RowInfo]) -> String {
    match rows {
        [] => "keine Zeile".to_string(),
        [row] if row.oz.is_empty() => row.art.clone(),
        [row] => format!("{} {}", row.art, row.oz),
        [first, .., last] => {
            let range = if first.oz.is_empty() || last.oz.is_empty() {
                String::new()
            } else {
                format!(" ({} … {})", first.oz, last.oz)
            };
            format!("{} Zeilen{range}", rows.len())
        }
    }
}

fn tab_name(tab: Tab) -> &'static str {
    match tab {
        Tab::Lv => "LV",
        Tab::Bids => "Angebote",
        Tab::Comparison => "Preisspiegel",
        Tab::Billing => "Abrechnung",
        Tab::ChangeOrders => "Nachträge",
        Tab::Costs => "Kosten",
        Tab::Tracking => "Kostenverfolgung",
    }
}
