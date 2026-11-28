//! „Einfügen und neu nummerieren?“: an insert that finds no free order
//! number between its neighbours (an LV with its own numbers, 0078 | 0079)
//! asks once and shows alt → neu. „Einfügen“ inserts and numbers the
//! positions of that title again in steps – titles and finished positions
//! keep their numbers – as one undo step.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::numbering;
use proava_domain::transfer::{self, Transfer};
use proava_domain::{Project, RowId};

use super::{Dialog, EditorOutput, LvEditor};
use crate::lv_ops::{self, Command};
use crate::theme::Palette;
use crate::{struct_ops, widgets};

/// The insert that is run again after „Einfügen“.
#[derive(Clone)]
pub(super) enum Retry {
    /// Neue Position (danach/davor), Duplizieren.
    Command(Command),
    /// Strg+V of rows copied in this table.
    PasteInternal,
    /// Rows of another window or project (Strg+V, Ziehen, KI).
    PasteTransfer(Box<Transfer>),
    /// Übernahme-Fenster, Bibliothek.
    InsertTransfer(Box<Transfer>),
}

/// The open question.
pub(super) struct PendingRenumber {
    retry: Retry,
    /// „04.10 Hardware“: the title whose positions get new numbers (empty:
    /// an LV without titles).
    title: String,
    /// alt → neu in the order of the LV; new rows have no old number.
    changes: Vec<(Option<String>, String)>,
    warnings: Vec<String>,
    /// False in the frame the question came up: the key that started the
    /// insert must not answer it.
    armed: bool,
}

/// The rows of `transfer` the active LV takes (no titles without a title
/// level in its mask).
pub(super) fn fitting(project: &Project, transfer: &Transfer) -> Transfer {
    let mut transfer = transfer.clone();
    if project.lv().level_count() == 0 {
        transfer.rows.retain(|row| !row.is_group());
    }
    transfer
}

impl LvEditor {
    /// An insert failed with `error`. Was there no free order number and
    /// would numbering the title again give the new rows numbers: the
    /// question; otherwise the error as a message.
    pub(super) fn insert_failed(
        &mut self,
        project: &Project,
        retry: Retry,
        error: String,
    ) -> EditorOutput {
        if !numbering::is_no_free_ordinal(&error) || numbering::title_renumbering() {
            return EditorOutput::toast(error);
        }
        let selected = self.selected_index(project.lv());
        let mut probe = project.clone();
        let tried = numbering::renumbering_title(|| self.apply(&mut probe, selected, &retry));
        if let Err(error) = tried {
            return EditorOutput::toast(error);
        }
        let pending = PendingRenumber::new(project, &probe, retry);
        self.dialog = Some(Dialog::InsertRenumbering(Box::new(pending)));
        EditorOutput::default()
    }

    /// Whether the question „Einfügen und neu nummerieren?“ is open.
    pub fn asks_renumbering(&self) -> bool {
        matches!(self.dialog, Some(Dialog::InsertRenumbering(_)))
    }

    /// Takes the question back (the insert needs more than this editor can
    /// run again); returns the message instead.
    pub(super) fn withdraw_renumbering(&mut self) -> Option<String> {
        if !self.asks_renumbering() {
            return None;
        }
        self.dialog = None;
        Some(numbering::NO_FREE_ORDINAL.to_string())
    }

    /// Only the change of the LV, without selection and undo – for the
    /// preview on a copy of the project.
    fn apply(
        &self,
        project: &mut Project,
        selected: Option<usize>,
        retry: &Retry,
    ) -> Result<(), String> {
        match retry {
            Retry::Command(command @ (Command::NewPosition | Command::NewPositionBefore)) => {
                lv_ops::insert_position_with(
                    project.lv_mut(),
                    selected,
                    *command == Command::NewPositionBefore,
                    self.new_position_titles(),
                )
                .map(drop)
            }
            Retry::Command(Command::Duplicate) => {
                let index = selected.ok_or("Keine Zeile gewählt.")?;
                lv_ops::duplicate(project.lv_mut(), index).map(drop)
            }
            Retry::Command(_) => Err(numbering::NO_FREE_ORDINAL.to_string()),
            Retry::PasteInternal => {
                lv_ops::paste(project.lv_mut(), selected, &self.clipboard).map(drop)
            }
            Retry::PasteTransfer(taken) => {
                let taken = fitting(project, taken);
                transfer::insert(project, selected, &taken).map(drop)
            }
            Retry::InsertTransfer(taken) => transfer::insert(project, selected, taken).map(drop),
        }
    }

    /// The insert again, the way it ran the first time (selection, undo
    /// step, message).
    fn run_again(
        &mut self,
        project: &mut Project,
        ctx: &egui::Context,
        retry: Retry,
    ) -> EditorOutput {
        match retry {
            Retry::Command(command) => self.run(command, project, ctx),
            Retry::PasteInternal => self.paste_internal(project),
            Retry::PasteTransfer(taken) => self.paste_transfer(project, &taken),
            Retry::InsertTransfer(taken) => self.insert_transfer(project, &taken),
        }
    }

    /// The question; true when it is open.
    pub(super) fn insert_renumbering_dialog(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) -> bool {
        if !self.asks_renumbering() {
            return false;
        }
        let Some(Dialog::InsertRenumbering(mut pending)) = self.dialog.take() else {
            return false;
        };
        let enter = pending.armed
            && ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            repeat: false,
                            modifiers,
                            ..
                        } if modifiers.is_none()
                    )
                })
            });
        pending.armed = true;
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 560.0, |ui| {
            let whose = if pending.title.is_empty() {
                "die Positionen des LV".to_string()
            } else {
                format!("die Positionen in Titel {}", pending.title)
            };
            widgets::title(
                ui,
                palette,
                "Einfügen und neu nummerieren?",
                &format!(
                    "An dieser Stelle ist keine Ordnungszahl frei. Beim Einfügen werden {whose} neu \
                     nummeriert; Titelnummern und fertige Positionen bleiben. Strg+Z macht beides \
                     rückgängig."
                ),
            );
            for warning in &pending.warnings {
                ui.label(RichText::new(warning).size(12.5).color(palette.warn));
            }
            ui.add_space(6.0);
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_height(180.0);
                    ui.set_max_height(180.0);
                    egui::ScrollArea::vertical()
                        .id_salt("insert_renumbering_preview")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let moved = pending.changes.iter().filter(|(old, _)| old.is_some());
                            ui.label(
                                RichText::new(format!(
                                    "{} ändern sich (alt → neu), dazu die neue Zeile:",
                                    crate::format::count(
                                        moved.count(),
                                        "Ordnungszahl",
                                        "Ordnungszahlen"
                                    )
                                ))
                                .size(12.5)
                                .color(palette.text_dim),
                            );
                            for (old, new) in pending.changes.iter().take(500) {
                                let line = match old {
                                    Some(old) => RichText::new(format!("{old:<18} → {new}")),
                                    None => RichText::new(format!("{:<18} → {new}  neu", ""))
                                        .color(palette.accent),
                                };
                                ui.label(line.monospace().size(12.5));
                            }
                            if pending.changes.len() > 500 {
                                ui.label(format!("… und {} weitere", pending.changes.len() - 500));
                            }
                        });
                });
            ui.add_space(10.0);
            widgets::dialog_buttons(ui, palette, "Einfügen")
        });
        if close || cancelled {
            return true;
        }
        if confirmed || enter {
            let ctx = ui.ctx().clone();
            let warned = !pending.warnings.is_empty();
            let done =
                numbering::renumbering_title(|| self.run_again(project, &ctx, pending.retry));
            if done.dirty {
                // The question named the sent X83 and the bids already.
                self.sent_warned = warned;
            }
            output.merge(done);
            return true;
        }
        self.dialog = Some(Dialog::InsertRenumbering(pending));
        true
    }
}

impl PendingRenumber {
    /// The question for `retry`: `project` as it is, `probe` after the
    /// insert with renumbering.
    fn new(project: &Project, probe: &Project, retry: Retry) -> Self {
        let before: HashMap<RowId, &str> = project
            .lv()
            .rows()
            .iter()
            .map(|row| (row.id(), row.ordinal()))
            .collect();
        let after = probe.lv().rows();
        let changes = after
            .iter()
            .filter(|row| row.has_ordinal())
            .filter_map(|row| match before.get(&row.id()) {
                Some(old) if *old != row.ordinal() => {
                    Some((Some(old.to_string()), row.ordinal().to_string()))
                }
                Some(_) => None,
                None => Some((None, row.ordinal().to_string())),
            })
            .collect();
        let title = after
            .iter()
            .position(|row| row.is_position() && !before.contains_key(&row.id()))
            .and_then(|new| after[..new].iter().rev().find(|row| row.is_group()))
            .map(|title| {
                format!("{} {}", title.ordinal(), title.short_text())
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        Self {
            retry,
            title,
            changes,
            warnings: struct_ops::renumber_warnings(project.lv(), project.bids().len()),
            armed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{Decimal, Lv, LvGroup, LvRow, OzMask, Position, ProjectName};

    fn ordinal_of(lv: &Lv, id: RowId) -> String {
        lv.row_by_id(id)
            .map(LvRow::ordinal)
            .unwrap_or_default()
            .to_string()
    }

    fn editor(project: &Project) -> LvEditor {
        LvEditor::new(project.lv(), None, &Default::default())
    }

    /// Title 01.01 with 0078 | 0079 – no number between.
    fn tight_project() -> Project {
        let mut lv = Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "01", "Hardware"));
        lv.push_group(LvGroup::new(1, "01.01", "Leitungen"));
        for (ordinal, text) in [
            ("01.01.0070", "NYM 5x2,5"),
            ("01.01.0075", "NYM 5x1,5 UP"),
            ("01.01.0078", "NYM 5x1,5 Rinne"),
            ("01.01.0079", "NYM 5x1,5 Rohr"),
            ("01.01.0080", "J-Y(St)Y"),
        ] {
            lv.push_position(Position::new(ordinal, text, Decimal::ONE, "m", None));
        }
        Project::new(ProjectName::parse("Musterstadt").unwrap(), "", lv)
    }

    #[test]
    fn a_new_position_without_free_number_asks_and_inserts_after_yes() {
        let mut project = tight_project();
        let mut editor = editor(&project);
        let rinne = project.lv().rows()[4].id();
        editor.select(project.lv(), rinne);
        let ctx = egui::Context::default();
        let before = project.lv().clone();

        let output = editor.run(Command::NewPosition, &mut project, &ctx);
        assert!(!output.dirty);
        assert!(output.toast.is_none(), "keine Meldung, die Frage");
        assert!(editor.asks_renumbering());
        assert_eq!(project.lv(), &before, "erst nach der Antwort");
        let Some(Dialog::InsertRenumbering(pending)) = &editor.dialog else {
            unreachable!()
        };
        assert_eq!(pending.title, "01.01 Leitungen");
        assert_eq!(
            pending.changes,
            [
                (Some("01.01.0070".into()), "01.01.0010".into()),
                (Some("01.01.0075".into()), "01.01.0020".into()),
                (Some("01.01.0078".into()), "01.01.0030".into()),
                (None, "01.01.0040".into()),
                (Some("01.01.0079".into()), "01.01.0050".into()),
                (Some("01.01.0080".into()), "01.01.0060".into()),
            ]
        );

        let Some(Dialog::InsertRenumbering(pending)) = editor.dialog.take() else {
            unreachable!()
        };
        let output =
            numbering::renumbering_title(|| editor.run_again(&mut project, &ctx, pending.retry));
        assert!(output.dirty);
        let new = editor.selected.unwrap();
        assert_eq!(ordinal_of(project.lv(), new), "01.01.0040");
        assert_eq!(project.lv().index_of(new), Some(5), "hinter 0078");
        assert_eq!(ordinal_of(project.lv(), rinne), "01.01.0030");

        // One step back: everything as before.
        editor.run(Command::Undo, &mut project, &ctx);
        assert_eq!(project.lv(), &before);
    }

    #[test]
    fn where_a_number_is_free_nothing_is_asked() {
        let mut project = tight_project();
        let mut editor = editor(&project);
        let first = project.lv().rows()[2].id();
        editor.select(project.lv(), first);
        let ctx = egui::Context::default();
        let output = editor.run(Command::NewPosition, &mut project, &ctx);
        assert!(output.dirty);
        assert!(!editor.asks_renumbering());
        assert_eq!(
            ordinal_of(project.lv(), editor.selected.unwrap()),
            "01.01.0072"
        );
    }

    #[test]
    fn a_finished_neighbour_gives_the_message_instead() {
        let mut project = tight_project();
        project
            .lv_mut()
            .row_mut(5)
            .and_then(LvRow::as_position_mut)
            .unwrap()
            .set_status(proava_domain::EditStatus::Done);
        let mut editor = editor(&project);
        let rinne = project.lv().rows()[4].id();
        editor.select(project.lv(), rinne);
        let ctx = egui::Context::default();
        let output = editor.run(Command::NewPosition, &mut project, &ctx);
        assert!(!editor.asks_renumbering());
        assert!(output.toast.unwrap().contains("fertige Positionen"));
    }
}
