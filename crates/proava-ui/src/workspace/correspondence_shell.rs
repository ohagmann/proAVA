//! The shell side of the Schriftverkehr-Archiv: writing letters with „Im
//! Projekt ablegen“, the list „Schriftverkehr im Projekt“ (open from a
//! temporary copy, save as, own PDFs) and their undo steps.

use std::path::{Path, PathBuf};

use eframe::egui;
use proava_application::{write_letters, LetterJob, LetterRequest, LetterTarget};
use proava_domain::{CorrespondenceEntry, CorrespondenceId, CorrespondenceKind};

use super::{Dialog, FilePurpose, JobOutcome, WorkspaceScreen};
use crate::correspondence_view::{CorrespondenceDialog, CorrespondenceResult};
use crate::file_dialog::{self, FileDialog};
use crate::history::ProjectChange;

impl WorkspaceScreen {
    /// Angebote ▸ Schriftverkehr im Projekt …, optionally only one recipient.
    pub(super) fn open_filed_letters(&mut self, recipient: Option<String>) {
        self.dialog = Some(Dialog::Correspondence(Box::new(
            CorrespondenceDialog::new().with_recipient(recipient),
        )));
    }

    /// Writes the letters chosen in the Schriftverkehr dialog (in the
    /// background); with „Im Projekt ablegen“ the job also returns one
    /// archive entry per recipient.
    pub(super) fn write_letters(&mut self, ctx: &egui::Context, target: PathBuf) {
        self.parked_letters = None;
        let Some((kind, recipients, separate)) = self.pending_letters.take() else {
            return;
        };
        let request = if kind.to_client() {
            LetterRequest::Release {
                invoice: self.pending_release_letter.take(),
            }
        } else {
            LetterRequest::Bidders {
                kind,
                recipients: recipients.clone(),
            }
        };
        let target = if separate && !kind.to_client() {
            LetterTarget::PerRecipient(self.letter_files(kind, &recipients, &target))
        } else {
            LetterTarget::One(target)
        };
        let job = LetterJob {
            request,
            target,
            file_in_project: self.pending_file_letters,
            logo: self.print_logo(ctx),
            frame: self.launcher.preferences().page_frame,
            look: self.take_context_look().look,
            date: crate::format::iso_to_german_date(&self.launcher.now_iso()),
        };
        let exchange = self.launcher.shared_exchange();
        let project = self.project.clone();
        self.start_job(
            ctx,
            format!("{} wird erzeugt …", kind.label()),
            move || {
                let (files, filed, result) = match write_letters(exchange.as_ref(), &project, &job)
                {
                    Ok(output) => (output.written, output.filed, Ok(output.pages)),
                    Err(error) => (Vec::new(), Vec::new(), Err(error)),
                };
                JobOutcome::Letters {
                    kind,
                    recipients,
                    files,
                    filed,
                    result,
                }
            },
        );
    }

    /// After writing: Versanddatum (Aufforderung) and the filed letters as
    /// one undo step.
    pub(super) fn letters_written(
        &mut self,
        kind: proava_domain::LetterKind,
        recipients: &[proava_domain::InviteeId],
        filed: Vec<CorrespondenceEntry>,
    ) -> usize {
        let mut steps = Vec::new();
        if kind == proava_domain::LetterKind::Invitation {
            let key = self.active_key();
            if let Some(before) = self.project.offers_snapshot(self.project.active_lv()) {
                let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
                for id in recipients {
                    if let Some(invitee) = self.project.invitee_mut(*id) {
                        invitee.requested = today.clone();
                    }
                }
                // Sent without an X83 export in proAVA: the state of now is
                // what the bidders got (for „Seit dem Versand geändert“).
                if self.project.lv().sent_fingerprint().is_none() {
                    let fingerprint = self.project.lv().content_fingerprint();
                    self.project
                        .lv_mut()
                        .set_sent_fingerprint(Some(fingerprint));
                }
                if self
                    .project
                    .offers_snapshot(self.project.active_lv())
                    .as_ref()
                    != Some(&before)
                {
                    steps.push(ProjectChange::Offers(key, Box::new(before)));
                }
                self.offers_baseline = self
                    .project
                    .offers_snapshot(self.project.active_lv())
                    .map(|snapshot| (key, snapshot));
            }
        }
        let count = filed.len();
        if count > 0 {
            steps.push(ProjectChange::Correspondence(
                self.project.correspondence_snapshot(),
            ));
            for entry in filed {
                self.project.add_correspondence(entry);
            }
        }
        let label = match (steps.len(), count) {
            (_, 0) => "Versanddatum",
            (1, _) => "Schreiben im Projekt ablegen",
            _ => "Schreiben ablegen und Versanddatum",
        };
        let change = match steps.len() {
            0 => None,
            1 => steps.pop(),
            _ => Some(ProjectChange::Several(steps)),
        };
        if let Some(change) = change {
            self.dirty = true;
            self.record_project(change, label);
        }
        count
    }

    /// One frame of the list „Schriftverkehr im Projekt“; false closes it.
    pub(super) fn correspondence_step(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        dialog: &mut CorrespondenceDialog,
    ) -> bool {
        // Strg+Z/Y in the list: only steps of the archive.
        let (undo, redo) = ctx.input_mut(|input| {
            let command = egui::Modifiers::COMMAND;
            let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            let redo = self.history.correspondence_next(true)
                && (input.consume_key(shift, egui::Key::Z)
                    || input.consume_key(command, egui::Key::Y));
            let undo =
                self.history.correspondence_next(false) && input.consume_key(command, egui::Key::Z);
            (undo, redo)
        });
        if undo {
            self.undo(ctx);
        }
        if redo {
            self.redo(ctx);
        }
        let palette = self.palette;
        match dialog.show(ui, &palette, &mut self.project) {
            CorrespondenceResult::Open => true,
            CorrespondenceResult::Closed => false,
            CorrespondenceResult::Changed {
                before,
                label,
                join,
            } => {
                self.dirty = true;
                self.note_editor();
                let now = ctx.input(|input| input.time);
                self.history.record_correspondence(before, label, now, join);
                true
            }
            CorrespondenceResult::OpenPdf(id) => {
                self.open_filed_letter(ctx, id);
                true
            }
            CorrespondenceResult::SaveAs(id) => {
                self.parked_correspondence = Some(Box::new(std::mem::take(dialog)));
                self.open_file_dialog(FilePurpose::SaveFiledLetter(id));
                // The file dialog replaced this one.
                true
            }
            CorrespondenceResult::AddPdf => {
                self.parked_correspondence = Some(Box::new(std::mem::take(dialog)));
                self.open_file_dialog(FilePurpose::AddFiledLetters);
                true
            }
        }
    }

    /// „Öffnen“: a temporary copy for the system's PDF viewer (never in tests,
    /// see `jobs::open_with_system`).
    fn open_filed_letter(&mut self, ctx: &egui::Context, id: CorrespondenceId) {
        let Some(entry) = self.project.correspondence_entry(id) else {
            return;
        };
        // One folder per process; removed at the end of the program, and by
        // the next start if proAVA ended without (see viewer_copies). The
        // name is cleaned (a project from elsewhere) and an unchanged copy
        // is opened again without writing (Windows: the viewer holds it).
        let exchange = self.launcher.shared_exchange();
        let result = crate::viewer_copies::place(
            &crate::viewer_copies::folder(),
            &entry.file,
            &entry.pdf,
            |file, bytes| exchange.write_file(file, bytes),
        )
        .and_then(|file| crate::jobs::open_with_system(&file));
        match result {
            Ok(()) => self.say(ctx, format!("Geöffnet: {} (Kopie)", entry.file)),
            Err(error) => self.say(ctx, format!("Öffnen fehlgeschlagen: {error}")),
        }
    }

    pub(super) fn filed_letter_dialog(&self, purpose: FilePurpose) -> FileDialog {
        let folder = self.project_folder();
        match purpose {
            FilePurpose::SaveFiledLetter(id) => {
                let name = self
                    .project
                    .correspondence_entry(id)
                    .map(|entry| entry.file.clone())
                    .unwrap_or_else(|| "Schreiben.pdf".into());
                FileDialog::save(
                    &self.launcher,
                    "Schreiben speichern unter",
                    "Kopie des abgelegten PDF, z. B. zum Versenden; im Projekt bleibt es.",
                    folder,
                    &name,
                    "pdf",
                )
            }
            _ => FileDialog::open(
                &self.launcher,
                "PDF im Projekt ablegen",
                "Eigenes Schreiben (z. B. Protokoll, Mail als PDF) – mit Strg+Klick mehrere wählen.",
                folder,
                &["pdf"],
                true,
            ),
        }
    }

    /// A file for a filed letter was chosen (save as / own PDFs); then the
    /// list comes back.
    pub(super) fn filed_letter_file_chosen(
        &mut self,
        ctx: &egui::Context,
        purpose: FilePurpose,
        files: Vec<PathBuf>,
    ) {
        let mut dialog = self.parked_correspondence.take().unwrap_or_default();
        match purpose {
            FilePurpose::SaveFiledLetter(id) => {
                if let (Some(file), Some(entry)) =
                    (files.first(), self.project.correspondence_entry(id))
                {
                    match self.launcher.shared_exchange().write_file(file, &entry.pdf) {
                        Ok(()) => {
                            let text = format!("Gespeichert: {}", file_dialog::file_name(file));
                            self.say_with_file(ctx, text, Some(file.clone()));
                        }
                        Err(error) => self.say(ctx, error),
                    }
                }
            }
            _ => {
                let before = self.project.correspondence_snapshot();
                let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
                let mut added = Vec::new();
                let mut refused = Vec::new();
                for path in &files {
                    let name = file_dialog::file_name(path);
                    match self.launcher.read_file(path) {
                        Ok(bytes) if bytes.starts_with(b"%PDF") => {
                            let mut entry = CorrespondenceEntry::new(
                                CorrespondenceKind::Own,
                                today.clone(),
                                stem(path),
                                bytes,
                            );
                            entry.file = name;
                            added.push(self.project.add_correspondence(entry));
                        }
                        Ok(_) => refused.push(format!("{name} (kein PDF)")),
                        Err(error) => refused.push(format!("{name} ({error})")),
                    }
                }
                if !added.is_empty() {
                    self.dirty = true;
                    self.record_project(
                        ProjectChange::Correspondence(before),
                        "PDF im Projekt ablegen",
                    );
                    dialog.select(added.last().copied());
                }
                let count = crate::format::count(added.len(), "PDF", "PDFs");
                let text = if refused.is_empty() {
                    format!("{count} im Projekt abgelegt.")
                } else {
                    format!(
                        "{count} im Projekt abgelegt; übergangen: {}",
                        refused.join(", ")
                    )
                };
                self.say(ctx, text);
            }
        }
        self.dialog = Some(Dialog::Correspondence(dialog));
    }

    /// The file dialog of the list was cancelled: back to the list.
    pub(super) fn filed_letter_cancelled(&mut self) {
        let dialog = self.parked_correspondence.take().unwrap_or_default();
        self.dialog = Some(Dialog::Correspondence(dialog));
    }
}

/// File name without „.pdf“ as the subject of an own letter.
fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().replace('_', " "))
        .unwrap_or_default()
}
