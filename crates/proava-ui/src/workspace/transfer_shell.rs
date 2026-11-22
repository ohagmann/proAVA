//! The shell side of „Übernehmen“: the window with the source outline and
//! the file choice („In Bibliothek übernehmen“ is in `library_store`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui;
use proava_application::lv_search;
use proava_domain::lv_search::LvQuery;

use super::{FilePurpose, WorkspaceScreen};
use crate::file_dialog::FileDialog;
use crate::jobs::Job;
use crate::strings;
use crate::transfer_view::{TransferRequest, TransferView};

impl WorkspaceScreen {
    pub(super) fn open_transfer(&mut self) {
        match self.transfer.as_mut() {
            Some(view) => view.reopen(),
            None => self.transfer = Some(TransferView::new()),
        }
    }

    pub(super) fn pick_transfer_source(&mut self) -> FileDialog {
        let mut extensions: Vec<&str> = vec!["proava"];
        extensions.extend(strings::GAEB_EXTENSIONS);
        FileDialog::open(
            &self.launcher,
            "Quelle für die Übernahme",
            "proAVA-Projekt oder GAEB-LV (X81–X86, D81–D86) – wird nur gelesen.",
            self.project_folder(),
            &extensions,
            false,
        )
    }

    pub(super) fn load_transfer_source(&mut self, path: PathBuf, library: bool) {
        let result = if library && !self.launcher.is_file(&path) {
            Err(format!(
                "Die Bibliothek „{}“ gibt es noch nicht – „In Bibliothek übernehmen …“ legt sie an.",
                path.display()
            ))
        } else {
            self.launcher.read_source(&path)
        };
        if result.is_ok() && !library {
            self.launcher.remember_transfer_source(&path);
        }
        if let Some(view) = self.transfer.as_mut() {
            view.loaded(&path, library, result);
        }
    }

    /// Draws the window (if open) and handles its requests.
    pub(super) fn transfer_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let library = self.launcher.library_file();
        let recent = self.launcher.transfer_sources();
        let request = match self.transfer.as_mut() {
            Some(view) if view.is_open() => view.show(ctx, &self.palette, &recent, &library),
            _ => None,
        };
        // Rows dragged from the window (or the side window „Bibliothek“) onto the table.
        if let Some(after) = self.editor.take_transfer_drop(&self.project) {
            if let Some(taken) = self.library_drop(ctx) {
                self.insert_from_library(ctx, &taken, Some(after));
            } else if let Some(taken) = self.transfer.as_ref().and_then(TransferView::transfer) {
                self.tab = super::Tab::Lv;
                let output = self
                    .editor
                    .insert_transfer_after(&mut self.project, &taken, after);
                self.take_editor_output(ctx, output);
            }
            egui::DragAndDrop::clear_payload(ctx);
        }
        match request {
            Some(TransferRequest::PickFile) => {
                let dialog = self.pick_transfer_source();
                self.dialog = Some(super::Dialog::File {
                    dialog,
                    purpose: FilePurpose::TransferSource,
                });
            }
            Some(TransferRequest::Load { path, library }) => {
                self.load_transfer_source(path, library)
            }
            Some(TransferRequest::Take(taken)) => {
                self.tab = super::Tab::Lv;
                let output = self.editor.insert_transfer(&mut self.project, &taken);
                self.take_editor_output(ctx, output);
            }
            Some(TransferRequest::SearchFolder(Some(folder))) => {
                self.start_transfer_search(ctx, folder)
            }
            Some(TransferRequest::SearchFolder(None)) => {
                let start = self
                    .transfer
                    .as_ref()
                    .and_then(|view| view.search_folder().map(Path::to_path_buf))
                    .unwrap_or_else(|| self.project_folder());
                self.dialog = Some(super::Dialog::File {
                    dialog: FileDialog::folder(
                        &self.launcher,
                        "Projekte unter diesem Ordner durchsuchen",
                        start,
                    ),
                    purpose: FilePurpose::TransferSearchFolder,
                });
            }
            None => {}
        }
        self.library_ui(ui);
    }

    /// „In allen Projekten unter … suchen“: every project below `folder`
    /// on a background thread (only their outline and texts are read).
    pub(super) fn start_transfer_search(&mut self, ctx: &egui::Context, folder: PathBuf) {
        let Some(view) = self.transfer.as_mut() else {
            return;
        };
        let text = view.search_text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some((repository, exchange)) = self.launcher.search_handles() else {
            self.say(ctx, "Die Suche in Ordnern geht mit diesem Speicher nicht.");
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let now = self.launcher.now_iso();
        let root = folder.clone();
        let query = LvQuery::text(text.clone());
        let job = Job::spawn("Projekte durchsuchen", move || {
            let scan = lv_search::project_files(repository.as_ref(), &root, false, &|_| true);
            let outcome = lv_search::search_files(
                repository.as_ref(),
                exchange.as_ref(),
                &scan.files,
                &query,
                &now,
                &mut || stop.load(Ordering::Relaxed),
            );
            (outcome, scan.more)
        });
        view.searching(folder, text, job, cancel);
        ctx.request_repaint();
    }
}
