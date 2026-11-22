//! The question before a change of the LV is kept (Anwender 18./19.11.):
//! the LV was sent and order numbers move, or it has bids or an Auftrag and
//! positions come or go – the bids and the Auftrag then miss them. One
//! dialog names every reason; „Abbrechen“ takes the change back, with an
//! Auftrag „Als Nachtrag anlegen …“ assigns new positions to a new Nachtrag.

use eframe::egui::{self, RichText};
use proava_domain::{
    position_ids, BidsOrOrder, ChangeBasis, ChangeOrder, ChangeOrderId, Lv, RowId,
};

use super::{Dialog, Tab, WorkspaceScreen};
use crate::{lv_check, strings, struct_ops, widgets};

/// What the question asks about (the open dialog [`Dialog::ChangeQuestion`]).
#[derive(Debug, Clone)]
pub(crate) struct ChangeQuestion {
    pub title: &'static str,
    pub question: String,
    /// Label of the button that keeps the change.
    pub keep: String,
    /// Also the question after sending: keeping it ends those questions for
    /// the active LV until the next X83 export.
    pub sent: bool,
    /// LVs (keys) with bids or an Auftrag and their state: keeping the change
    /// ends the questions there until bids or Auftrag change.
    pub bids: Vec<(u64, BidsOrOrder)>,
    /// New positions of the active LV that has an Auftrag: „Als Nachtrag
    /// anlegen …“ makes them the positions of a new Nachtrag.
    pub change_order: Vec<RowId>,
}

/// One LV whose positions the change leaves uncovered.
struct Uncovered {
    key: u64,
    state: BidsOrOrder,
    added: Vec<RowId>,
    removed: usize,
    note: String,
}

impl WorkspaceScreen {
    /// After every recorded change of the LV editor: ask if the LV was sent
    /// and order numbers moved (Duplizieren, Einfügen, Löschen, Verschieben,
    /// Neu nummerieren …), or if it has bids or an Auftrag and positions came
    /// or went (Einfügen, Strg+V, Duplizieren, Löschen, KI-Brücke …).
    pub(super) fn guard_sent_ordinals(&mut self, ctx: &egui::Context) {
        let key = self.active_key();
        let records = self.editor.undo_records();
        let seen = self.sent_guard.replace((key, records));
        // The editor warned already (Neu nummerieren, Umwandeln): no second
        // question for this change.
        let warned = self.editor.take_sent_warned();
        if warned {
            self.sent_confirmed.insert(key);
        }
        let asked = self.editor.take_change_asked();
        let Some((seen_key, seen_records)) = seen else {
            return;
        };
        if warned || asked || seen_key != key || seen_records == records || self.dialog.is_some() {
            return;
        }
        let Some(before) = self.editor.last_undo_lv() else {
            return;
        };
        if let Some(question) = self.change_question(before, None) {
            self.dialog = Some(Dialog::ChangeQuestion(Box::new(question)));
            ctx.request_repaint();
        }
    }

    /// The same question for rows dragged in from another LV (a step outside
    /// the editor): `target_before` is the active LV before, `source` the LV
    /// the rows were moved out of (index, its state before) – it loses them.
    pub(super) fn ask_after_drop(
        &mut self,
        ctx: &egui::Context,
        target_before: &Lv,
        source: Option<(usize, &Lv)>,
    ) {
        if self.dialog.is_some() {
            return;
        }
        if let Some(question) = self.change_question(target_before, source) {
            self.dialog = Some(Dialog::ChangeQuestion(Box::new(question)));
            ctx.request_repaint();
        }
    }

    /// The positions of the LV at `index` a change since `before` leaves
    /// uncovered – unless that was kept already for the same bids and Auftrag.
    /// `source`: the LV the rows were moved out of (named in the text).
    fn uncovered(&self, index: usize, before: &Lv, source: bool) -> Option<Uncovered> {
        let key = self.project.lv_key(index)?;
        let state = self.project.bids_or_order(index);
        if state.is_empty() || self.bids_confirmed.get(&key) == Some(&state) {
            return None;
        }
        let uncovered = self
            .project
            .uncovered_positions(index, &position_ids(before));
        let note = if source {
            let name = crate::lv_properties::lv_label(self.project.lv_at(index)?);
            let subject = format!("„{name}“, aus dem die Zeilen kommen,");
            self.project
                .uncovered_note_for(index, &uncovered, &subject, "verschobene")?
        } else {
            self.project.uncovered_note(index, &uncovered)?
        };
        Some(Uncovered {
            key,
            state,
            removed: uncovered.removed.len(),
            added: uncovered.added,
            note,
        })
    }

    /// For a step inside a dialog that stays open („Neue Position aus
    /// STLB-Bau …“ goes on with the next one): the reason as a note in its
    /// message instead of a question, unless kept already.
    pub(super) fn uncovered_hint(&self, before: &Lv) -> Option<String> {
        let part = self.uncovered(self.project.active_lv(), before, false)?;
        Some(format!("{} Strg+Z nimmt es zurück.", part.note))
    }

    /// The question for a change of the active LV since `before`, None when
    /// nothing needs one.
    fn change_question(&self, before: &Lv, source: Option<(usize, &Lv)>) -> Option<ChangeQuestion> {
        let key = self.active_key();
        let active = self.project.active_lv();
        let sent = (!self.sent_confirmed.contains(&key) && self.lv_sent())
            .then(|| struct_ops::first_moved_ordinal(before, self.project.lv()))
            .flatten()
            .map(|first| {
                format!(
                    "Das LV wurde{} versandt. Die OZ ab {first} ändern sich – das LV muss danach neu \
                     versendet werden.",
                    lv_check::sent_when(&self.project)
                )
            });
        let mut parts: Vec<Uncovered> = self.uncovered(active, before, false).into_iter().collect();
        if let Some((index, source_before)) = source.filter(|(index, _)| *index != active) {
            parts.extend(self.uncovered(index, source_before, true));
        }
        if parts.is_empty() {
            let question = sent?;
            return Some(ChangeQuestion {
                title: strings::SENT_CHANGE_TITLE,
                question,
                keep: strings::SENT_CHANGE_KEEP.to_string(),
                sent: true,
                bids: Vec::new(),
                change_order: Vec::new(),
            });
        }
        let added = parts.iter().any(|part| !part.added.is_empty());
        let removed = parts.iter().any(|part| part.removed > 0);
        let only_source = parts.iter().all(|part| part.key != key);
        let verb = match (added, removed) {
            (true, false) => "einfügen",
            (false, true) if only_source => "verschieben",
            (false, true) => "löschen",
            _ => "ändern",
        };
        let order = parts.iter().any(|part| part.state.order);
        let mut question: Vec<String> = parts.iter().map(|part| part.note.clone()).collect();
        question.extend(sent.clone());
        question.push(format!("Trotzdem {verb}?"));
        let change_order = parts
            .iter()
            .find(|part| part.key == key && part.state.order)
            .map(|part| part.added.clone())
            .unwrap_or_default();
        Some(ChangeQuestion {
            title: if order {
                strings::ORDER_CHANGE_TITLE
            } else {
                strings::BIDS_CHANGE_TITLE
            },
            question: question.join(" "),
            keep: format!("Trotzdem {verb}"),
            sent: sent.is_some(),
            bids: parts.iter().map(|part| (part.key, part.state)).collect(),
            change_order,
        })
    }

    /// The dialog [`Dialog::ChangeQuestion`]; true = still open.
    pub(super) fn change_question_dialog(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        question: &ChangeQuestion,
    ) -> bool {
        let palette = self.palette;
        let ((cancelled, confirmed, as_order), close) = widgets::modal_with_id(
            ui,
            &palette,
            egui::Id::unique("proava_sent_change_question"),
            580.0,
            |ui| {
                widgets::title(ui, &palette, question.title, "");
                ui.label(
                    RichText::new(question.question.as_str())
                        .size(13.0)
                        .color(palette.text),
                );
                ui.add_space(14.0);
                if question.change_order.is_empty() {
                    let (cancelled, confirmed) =
                        widgets::dialog_buttons(ui, &palette, &question.keep);
                    (cancelled, confirmed, false)
                } else {
                    widgets::dialog_buttons_with_extra(
                        ui,
                        &palette,
                        &question.keep,
                        strings::AS_CHANGE_ORDER,
                    )
                }
            },
        );
        if confirmed || as_order {
            if question.sent {
                self.sent_confirmed.insert(self.active_key());
            }
            for (key, state) in &question.bids {
                self.bids_confirmed.insert(*key, *state);
            }
            if as_order {
                self.new_positions_as_change_order(ctx, &question.change_order);
            } else if question.bids.is_empty() {
                self.say(ctx, strings::SENT_CHANGE_KEPT);
            } else if question.sent {
                self.say(
                    ctx,
                    format!(
                        "{} Das LV muss neu versendet werden.",
                        strings::BIDS_CHANGE_KEPT
                    ),
                );
            } else {
                self.say(ctx, strings::BIDS_CHANGE_KEPT);
            }
            false
        } else if cancelled || close {
            // Take the change back (one step, like Strg+Z).
            self.undo(ctx);
            self.sent_guard = Some((self.active_key(), self.editor.undo_records()));
            let undone = if question.bids.is_empty() {
                strings::SENT_CHANGE_UNDONE
            } else {
                strings::BIDS_CHANGE_UNDONE
            };
            self.say(ctx, undone);
            false
        } else {
            true
        }
    }

    /// „Als Nachtrag anlegen …“: a new Nachtrag (zusätzliche Leistung, today)
    /// with the new positions of the active LV, where they are; without a
    /// festgeschriebenen Auftrag the awarded bid is taken over first. One
    /// undo step; the Reiter Nachträge shows it for title, reason and prices.
    fn new_positions_as_change_order(&mut self, ctx: &egui::Context, positions: &[RowId]) {
        self.ensure_contract_baseline();
        let mut taken_over = "";
        if self.project.contract().is_none() && self.project.awarded_bid().is_some() {
            if let Err(error) = self.project.take_over_contract() {
                self.say(ctx, error);
                return;
            }
            taken_over = " (Auftrag aus dem beauftragten Angebot übernommen)";
        }
        let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "");
        order.announced_date = crate::format::iso_to_german_date(&self.launcher.now_iso());
        order.basis = ChangeBasis::AdditionalPerformance;
        let id = self.project.add_change_order(order);
        let mut assigned = 0;
        for row in positions {
            match self.project.assign_to_change_order(id, *row) {
                Ok(count) => assigned += count,
                Err(error) => {
                    self.say(ctx, error);
                }
            }
        }
        let label = self
            .project
            .change_order(id)
            .map(|order| order.label())
            .unwrap_or_default();
        if let Some(order) = self.project.change_order_mut(id) {
            order.title = format!("Nachtrag {label}");
        }
        self.dirty = true;
        self.contract_changed(ctx, "Neue Positionen als Nachtrag");
        self.change_orders.open(id);
        self.tab = Tab::ChangeOrders;
        self.say(
            ctx,
            format!(
                "{label}: {} als neue Leistung angelegt{taken_over} – Bezeichnung, Begründung und Preise hier eintragen.",
                crate::format::count(assigned, "Position", "Positionen")
            ),
        );
    }
}
