//! The shell side of „GAEB ▸ Importieren …“: carries out what the import
//! dialog asks for – new LV (with undo), new project (optionally opened),
//! the takeover window with the file as source, the own library, or reading
//! the file as a bid – and hands the protocol back to the dialog.

use eframe::egui;
use proava_application::gaeb_import::{self, ImportTarget};

use super::{Dialog, FilePurpose, Tab, WorkspaceScreen};
use crate::history::ProjectChange;
use crate::import_dialog::{self, ImportDialog, ImportRequest, ImportResult};
use crate::OpenRequest;

impl WorkspaceScreen {
    pub(super) fn open_import(&mut self) {
        self.dialog = Some(Dialog::Import(Box::new(ImportDialog::new(
            &self.launcher,
            self.dirty,
        ))));
    }

    /// „CSV importieren …“: the import dialog for CSV tables.
    pub(super) fn open_import_csv(&mut self) {
        self.dialog = Some(Dialog::Import(Box::new(ImportDialog::new_csv(
            &self.launcher,
            self.dirty,
        ))));
    }

    /// One frame of the import dialog; false when it closes.
    pub(super) fn import_step(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        dialog: &mut ImportDialog,
    ) -> bool {
        let palette = self.palette;
        match dialog.show(ui, &palette, &self.launcher, &self.project) {
            ImportResult::Open => true,
            ImportResult::Closed => false,
            ImportResult::Do(request) => self.carry_out(ctx, dialog, request),
        }
    }

    fn carry_out(
        &mut self,
        ctx: &egui::Context,
        dialog: &mut ImportDialog,
        request: ImportRequest,
    ) -> bool {
        match request {
            ImportRequest::AddLv {
                number,
                title,
                prices,
            } => {
                let Some(import) = dialog.import().cloned() else {
                    return true;
                };
                let before = self.project.lv_list_snapshot();
                let previous = self.active_key();
                self.note_editor();
                match gaeb_import::add_as_lv(&mut self.project, &import, &number, &title, prices) {
                    Ok((index, protocol)) => {
                        self.history.record_project(
                            ProjectChange::LvList(Box::new(before)),
                            if import.is_csv() {
                                "CSV importieren"
                            } else {
                                "GAEB importieren"
                            },
                        );
                        // A replaced empty LV keeps index 0: show the new content.
                        self.follow_active_lv(previous);
                        self.activate_lv(index);
                        self.tab = Tab::Lv;
                        self.dirty = true;
                        self.say(
                            ctx,
                            format!(
                                "{} importiert.",
                                crate::lv_properties::lv_label(self.project.lv())
                            ),
                        );
                        dialog.finished(protocol);
                    }
                    Err(error) => dialog.failed(error),
                }
                true
            }
            ImportRequest::NewProject {
                name,
                folder,
                open,
                prices,
            } => {
                let Some(import) = dialog.import().cloned() else {
                    return true;
                };
                let created = gaeb_import::as_project(&import, &name, prices)
                    .and_then(|project| self.launcher.create_project_with(&folder, &project));
                match created {
                    Ok(entry) => {
                        let protocol = gaeb_import::protocol(
                            &import,
                            &ImportTarget::Project {
                                file: entry.path.clone(),
                            },
                            prices,
                        );
                        if open {
                            if self.dirty {
                                self.save_now(ctx);
                                if self.dirty {
                                    dialog.failed(
                                        "Das neue Projekt ist angelegt, das offene ließ sich aber nicht speichern – \
                                         es bleibt offen."
                                            .to_string(),
                                    );
                                    dialog.finished(protocol);
                                    return true;
                                }
                            }
                            import_dialog::request_open(
                                ctx,
                                OpenRequest {
                                    path: entry.path,
                                    protocol: Some(protocol.clone()),
                                    ..OpenRequest::default()
                                },
                            );
                            return false;
                        }
                        self.say_with_file(
                            ctx,
                            format!(
                                "Projekt angelegt: {}",
                                crate::file_dialog::file_name(&entry.path)
                            ),
                            None,
                        );
                        dialog.finished(protocol);
                    }
                    Err(error) => dialog.failed(error),
                }
                true
            }
            ImportRequest::Transfer { prices } => {
                let Some(import) = dialog.take_import() else {
                    return true;
                };
                let protocol = gaeb_import::protocol(&import, &ImportTarget::Transfer, prices);
                // The findings without the pure information about images.
                let warnings = gaeb_import::ImportPreview::of(&import).warnings;
                self.tab = Tab::Lv;
                self.open_transfer();
                if let Some(view) = self.transfer.as_mut() {
                    view.options.prices = prices;
                    view.loaded(&import.file, false, Ok((import.read.project, warnings)));
                }
                dialog.finished(protocol);
                true
            }
            ImportRequest::Library { prices } => {
                let Some(import) = dialog.take_import() else {
                    return true;
                };
                self.open_library_for_import(ctx, import, prices);
                false
            }
            ImportRequest::ReadBid(file) => {
                self.tab = Tab::Bids;
                self.file_chosen(ctx, FilePurpose::ImportBids, vec![file]);
                false
            }
        }
    }
}
