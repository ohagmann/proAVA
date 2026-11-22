//! The shell side of „Mit anderem Stand vergleichen“: the window, reading
//! the other state, jump, PDF and the marks in the table.

use std::path::PathBuf;

use eframe::egui;

use super::{FilePurpose, WorkspaceScreen};
use crate::compare_view::{self, CompareRequest, CompareView};
use crate::file_dialog::{self, FileDialog};
use crate::strings;

impl WorkspaceScreen {
    pub(super) fn open_compare(&mut self) {
        match self.compare.as_mut() {
            Some(view) => view.reopen(),
            None => self.compare = Some(CompareView::new()),
        }
    }

    pub(super) fn pick_compare_source(&self) -> FileDialog {
        let mut extensions: Vec<&str> = vec!["proava"];
        extensions.extend(strings::GAEB_EXTENSIONS);
        FileDialog::open(
            &self.launcher,
            "Anderer Stand zum Vergleich",
            "proAVA-Projekt oder GAEB-LV (z. B. die verschickte X83) – wird nur gelesen.",
            self.project_folder(),
            &extensions,
            false,
        )
    }

    pub(super) fn compare_pdf_dialog(&self) -> FileDialog {
        let name = self.project.name().as_str().to_string();
        FileDialog::save(
            &self.launcher,
            "Änderungsliste als PDF",
            "Änderungen des LV gegenüber dem anderen Stand.",
            self.project_folder(),
            &file_dialog::suggested_name(&name, "Aenderungsliste", &self.launcher.now_iso(), "pdf"),
            "pdf",
        )
    }

    pub(super) fn load_compare_source(&mut self, path: PathBuf) {
        let result = self.launcher.read_source(&path);
        let Some(view) = self.compare.as_mut() else {
            return;
        };
        match result {
            Ok((project, _)) => {
                self.launcher.remember_transfer_source(&path);
                let lv = project.lv().clone();
                let label = if project.lv_count() > 1 {
                    format!(
                        "{} ({})",
                        compare_view::file_label(&path),
                        compare_view::lv_label(&lv)
                    )
                } else {
                    compare_view::file_label(&path)
                };
                view.set_other(label, lv, Some(path), self.project.lv());
            }
            Err(error) => view.error(error),
        }
    }

    pub(super) fn export_compare_pdf(&mut self, ctx: &egui::Context, file: PathBuf) {
        let Some(view) = self.compare.as_ref() else {
            return;
        };
        let against = view.against();
        let diff = view.diff().clone();
        let chosen = self.take_context_look();
        let logo = self.print_logo(ctx);
        let frame = self.launcher.preferences().page_frame;
        let result = self
            .launcher
            .shared_exchange()
            .export_change_list_pdf_styled(
                &self.project,
                &against,
                &diff,
                logo.as_ref(),
                &chosen.look,
                &frame,
                chosen.landscape,
                &file,
            );
        match result {
            Ok(()) => self.say_with_file(
                ctx,
                format!(
                    "Änderungsliste gespeichert: {}",
                    file_dialog::file_name(&file)
                ),
                Some(file),
            ),
            Err(error) => self.say(ctx, error),
        }
    }

    pub(super) fn compare_ui(&mut self, ctx: &egui::Context) {
        let Some(view) = self.compare.as_mut() else {
            return;
        };
        // Compare again after every change of the open LV.
        let pushes = self.editor.undo_pushes();
        if pushes != self.compare_seen {
            self.compare_seen = pushes;
            view.refresh(self.project.lv());
        }
        let recent = self.launcher.transfer_sources();
        let request = if view.is_open() {
            view.show(ctx, &self.palette, &self.project, &recent)
        } else {
            None
        };
        self.editor.set_highlight(view.marked_rows());
        match request {
            Some(CompareRequest::PickFile) => {
                let dialog = self.pick_compare_source();
                self.dialog = Some(super::Dialog::File {
                    dialog,
                    purpose: FilePurpose::CompareSource,
                });
            }
            Some(CompareRequest::Load(path)) => self.load_compare_source(path),
            Some(CompareRequest::OtherLv(index)) => {
                if let (Some(lv), Some(view)) =
                    (self.project.lv_at(index).cloned(), self.compare.as_mut())
                {
                    let label = format!("dieses Projekt, {}", compare_view::lv_label(&lv));
                    view.set_other(label, lv, None, self.project.lv());
                }
            }
            Some(CompareRequest::Jump(id)) => {
                self.tab = super::Tab::Lv;
                self.editor.select(self.project.lv(), id);
            }
            // The print dialog first (Stil, Seite …), then the file.
            Some(CompareRequest::Pdf) => self.open_change_list_print(),
            None => {}
        }
    }
}
