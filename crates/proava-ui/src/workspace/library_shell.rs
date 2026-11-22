//! The shell side of the side window „Bibliothek“: showing it next to the
//! LV table, inserting entries (one undo step), storing rows, a GAEB file
//! into the library, editing a library as a project (in a window of its
//! own, or here with the band „Bibliothek“ and „Zurück zu …“), export as
//! GAEB X81.

use std::path::{Path, PathBuf};

use eframe::egui;
use proava_application::library as library_use;
use proava_application::{GaebExport, Launcher, PanelSide};
use proava_domain::library;
use proava_domain::transfer::{self, Transfer};
use proava_domain::{Lv, OzMask, Project, ProjectName};

use super::{Dialog, FilePurpose, ShellAction, Tab, WorkspaceScreen};
use crate::import_dialog::{ImportChoice, ImportDialog};
use crate::library_sync::{LibrarySync, SyncRequest};
use crate::library_view::{LibraryPanel, LibraryRequest};
use crate::OpenRequest;

const MIN_WIDTH: f32 = 300.0;
const DEFAULT_WIDTH: f32 = 380.0;
/// Height of the band „Bibliothek“ above the header.
const BAND_HEIGHT: f32 = 36.0;

/// While a library is edited: the project opened last before it (not a
/// library, the file still there) – where „Zurück zu …“ leads. Also when the
/// library came from the start page or opened in a window of its own.
pub(super) fn back_target(launcher: &Launcher, file: &Path) -> Option<(PathBuf, String)> {
    launcher.library_at(file)?;
    launcher
        .recent()
        .into_iter()
        .find(|entry| {
            entry.path != file
                && launcher.library_at(&entry.path).is_none()
                && launcher.is_file(&entry.path)
        })
        .map(|entry| (entry.path, entry.name))
}

/// Commands of the library in menus and the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryCommand {
    /// Ansicht ▸ Bibliothek (F8).
    Toggle,
    /// The rows selected in the LV into the own library.
    Store,
    /// A GAEB file into the own library.
    ImportGaeb,
    /// Open the own library as project.
    Edit,
    /// Positions from a library against their entries.
    Sync,
}

impl LibraryCommand {
    pub fn label(self) -> &'static str {
        match self {
            LibraryCommand::Toggle => "Bibliothek ein-/ausblenden",
            LibraryCommand::Store => crate::strings::LIBRARY_STORE,
            LibraryCommand::ImportGaeb => "GAEB-Datei in die Bibliothek …",
            LibraryCommand::Edit => "Bibliothek bearbeiten …",
            LibraryCommand::Sync => "Mit Bibliothek abgleichen …",
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            LibraryCommand::Toggle => "F8",
            _ => "",
        }
    }
}

impl WorkspaceScreen {
    pub(super) fn library_command(&mut self, ctx: &egui::Context, command: LibraryCommand) {
        match command {
            LibraryCommand::Toggle => {
                let visible = !self.launcher.library_panel();
                self.launcher.set_library_panel(visible);
                if visible {
                    self.tab = Tab::Lv;
                    let panel = self
                        .library
                        .get_or_insert_with(|| LibraryPanel::new(&self.launcher));
                    panel.focus_search();
                }
            }
            LibraryCommand::Store => self.open_library_form(ctx),
            LibraryCommand::ImportGaeb => {
                let mut dialog = ImportDialog::new(&self.launcher, self.dirty);
                dialog.choice = ImportChoice::Library;
                self.dialog = Some(Dialog::Import(Box::new(dialog)));
            }
            LibraryCommand::Edit => {
                if let Some(own) = self.launcher.libraries().into_iter().next() {
                    self.edit_library(ctx, own.path);
                }
            }
            LibraryCommand::Sync => {
                self.tab = Tab::Lv;
                let mut libraries = Vec::new();
                let mut problems = Vec::new();
                for source in self.launcher.libraries() {
                    match self.launcher.read_library(&source) {
                        Ok(project) => libraries.push((source.name(), project)),
                        Err(error) => problems.push(error),
                    }
                }
                self.library_sync = Some(LibrarySync::new(libraries, problems, &self.project));
            }
        }
    }

    /// Window „Mit Bibliothek abgleichen“ (if open).
    pub(super) fn library_sync_ui(&mut self, ctx: &egui::Context) {
        let palette = self.palette;
        let Some(sync) = self.library_sync.as_mut() else {
            return;
        };
        if !sync.is_open() {
            self.library_sync = None;
            return;
        }
        let request = sync.show(ctx, &palette, &self.project);
        match request {
            Some(SyncRequest::Take { chosen, prices }) => {
                let before = self.project.lv().clone();
                let (updated, finished) = {
                    let libraries = sync.libraries();
                    library::refresh_from_library(
                        &mut self.project,
                        &libraries,
                        sync.links(),
                        &chosen,
                        prices,
                    )
                };
                if updated > 0 {
                    self.editor.record(&before);
                    self.editor.reload();
                    self.dirty = true;
                }
                let mut text = format!(
                    "{} auf den Bibliotheksstand gebracht – Strg+Z macht es rückgängig.",
                    crate::format::count(updated, "Position", "Positionen")
                );
                if !finished.is_empty() {
                    text.push_str(&format!(" Fertige blieben: {}.", finished.join(", ")));
                }
                sync.refresh(&self.project);
                self.say(ctx, text);
            }
            Some(SyncRequest::Jump(id)) => {
                self.tab = Tab::Lv;
                self.editor.select(self.project.lv(), id);
            }
            None => {}
        }
    }

    /// Draws the side window (if shown) before the LV editor takes the rest.
    pub(super) fn library_panel_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.tab != Tab::Lv || !self.launcher.library_panel() {
            return;
        }
        let palette = self.palette;
        let can_store = !self.editor.selected_rows(&self.project).is_empty();
        let panel = self
            .library
            .get_or_insert_with(|| LibraryPanel::new(&self.launcher));
        let side = match self.launcher.detail_side() {
            PanelSide::Left => egui::Panel::right("library_panel"),
            PanelSide::Right => egui::Panel::left("library_panel"),
        };
        let max = (ui.available_width() * 0.5).max(MIN_WIDTH);
        let request = side
            .resizable(true)
            .default_size(DEFAULT_WIDTH.min(max))
            .size_range(MIN_WIDTH..=max)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show(ui, |ui| panel.ui(ui, &palette, can_store))
            .inner;
        if let Some(request) = request {
            self.library_request(ctx, request);
        }
    }

    fn library_request(&mut self, ctx: &egui::Context, request: LibraryRequest) {
        match request {
            LibraryRequest::Insert(taken) => self.insert_from_library(ctx, &taken, None),
            LibraryRequest::Store => self.open_library_form(ctx),
            LibraryRequest::ImportGaeb => self.library_command(ctx, LibraryCommand::ImportGaeb),
            LibraryRequest::Edit(path) => self.edit_library(ctx, path),
            LibraryRequest::Export { taken, label } => {
                self.export_library_part(ctx, &taken, &label)
            }
            LibraryRequest::Close => self.launcher.set_library_panel(false),
            LibraryRequest::Reload => self.reload_library(),
            LibraryRequest::Sync => self.library_command(ctx, LibraryCommand::Sync),
        }
    }

    /// Inserts library entries at the selection (or after `after` for a
    /// drop); one undo step.
    pub(super) fn insert_from_library(
        &mut self,
        ctx: &egui::Context,
        taken: &Transfer,
        after: Option<Option<usize>>,
    ) {
        self.tab = Tab::Lv;
        let output = match after {
            Some(after) => self
                .editor
                .insert_transfer_after(&mut self.project, taken, after),
            None => self.editor.insert_transfer(&mut self.project, taken),
        };
        self.take_editor_output(ctx, output);
    }

    /// The entries of a drop from the side window, if it came from there.
    pub(super) fn library_drop(&self, ctx: &egui::Context) -> Option<Transfer> {
        let from_library = egui::DragAndDrop::payload::<crate::transfer_view::TransferDrag>(ctx)
            .is_some_and(|payload| payload.from_library);
        if !from_library {
            return None;
        }
        self.library.as_ref().and_then(LibraryPanel::transfer)
    }

    /// Reads the libraries again (after storing or importing).
    pub(super) fn reload_library(&mut self) {
        if let Some(panel) = self.library.as_mut() {
            panel.reload(&self.launcher);
        }
    }

    /// Band above the header while a library is edited: „Bibliothek: Name“
    /// in a colour of its own (no project) and „Zurück zu <Projekt>“.
    pub(super) fn library_band(&mut self, ui: &mut egui::Ui, actions: &mut Vec<ShellAction>) {
        let Some(source) = self.library_source.clone() else {
            return;
        };
        let palette = self.palette;
        let back = self.back_to.clone();
        egui::Panel::top("ws_library_band")
            .exact_size(BAND_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(palette.library_band)
                    .inner_margin(egui::Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Bibliothek: {}", source.name()))
                            .size(14.5)
                            .strong()
                            .color(palette.on_library_band),
                    );
                    ui.add_space(10.0);
                    let note = if source.own {
                        "Eigene Bibliothek – Einträge pflegen, kein Projekt".to_string()
                    } else {
                        format!("{} – Einträge pflegen, kein Projekt", source.label)
                    };
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(note)
                                .size(12.5)
                                .color(palette.on_library_band_dim),
                        )
                        .truncate(),
                    );
                    let Some((path, name)) = back else {
                        return;
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text =
                            format!("Zurück zu {}", crate::format::truncate_middle(&name, 48));
                        let button = egui::Button::new(
                            egui::RichText::new(text)
                                .size(13.0)
                                .strong()
                                .color(palette.library_band),
                        )
                        .fill(palette.on_library_band)
                        .min_size(egui::vec2(0.0, 26.0));
                        if ui
                            .add(button)
                            .on_hover_text(format!(
                                "Speichert die Bibliothek und öffnet wieder {}",
                                path.display()
                            ))
                            .clicked()
                        {
                            actions.push(ShellAction::BackToProject);
                        }
                    });
                });
            });
    }

    /// „Zurück zu <Projekt>“: save the library like on closing, then the
    /// project – in this window, or (it is open in the window that opened the
    /// library) close this one.
    pub(super) fn back_to_project(&mut self, ctx: &egui::Context) {
        let Some((path, name)) = self.back_to.clone() else {
            return;
        };
        if self.dirty && !self.read_only {
            self.save_now(ctx);
            if self.dirty {
                return;
            }
        }
        if self
            .launcher
            .foreign_lock(&path)
            .is_some_and(|lock| !lock.stale)
        {
            self.say(
                ctx,
                format!("Bibliothek gespeichert – „{name}“ ist im anderen proAVA-Fenster offen."),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        crate::import_dialog::request_open(
            ctx,
            OpenRequest {
                path,
                messages: Vec::new(),
                protocol: None,
                read_only: false,
                ignore_lock: false,
            },
        );
    }

    /// Opens a library as project (created when it does not exist yet): in
    /// a new proAVA window (the project stays open here, unsaved changes
    /// too), or – switched off or not possible – in this window after
    /// saving the project; the band „Bibliothek“ then leads back.
    fn edit_library(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self
            .launcher
            .library_at(&path)
            .is_some_and(|source| source.read_only)
        {
            self.say(
                ctx,
                "Diese Bibliothek ist schreibgeschützt (Einstellungen, Reiter Neue Projekte).",
            );
            return;
        }
        if path == self.project_file {
            return;
        }
        if !self.launcher.is_file(&path) {
            let library = library_use::new_library(&self.launcher.now_iso());
            if let Err(error) = self.launcher.save_project(&path, &library) {
                self.say(ctx, format!("Bibliothek anlegen fehlgeschlagen: {error}"));
                return;
            }
        }
        let name = library_use::library_name(&path);
        if let Some(lock) = self.launcher.foreign_lock(&path).filter(|lock| !lock.stale) {
            self.say(
                ctx,
                format!(
                    "„{name}“ ist schon in einem anderen proAVA-Fenster offen ({}).",
                    lock.who()
                ),
            );
            return;
        }
        let mut messages = Vec::new();
        if self.launcher.preferences().library_own_window {
            match crate::jobs::new_window(&[path.clone().into_os_string()]) {
                Ok(()) => {
                    self.say(
                        ctx,
                        format!(
                            "„{name}“ wird in einem neuen Fenster geöffnet – das Projekt bleibt hier offen."
                        ),
                    );
                    return;
                }
                Err(error) => messages.push(format!(
                    "{error} Die Bibliothek ist deshalb in diesem Fenster offen; „Zurück zu …“ oben öffnet das Projekt wieder."
                )),
            }
        }
        if self.dirty {
            self.save_now(ctx);
            if self.dirty {
                return;
            }
        }
        crate::import_dialog::request_open(
            ctx,
            OpenRequest {
                path,
                messages,
                protocol: None,
                read_only: false,
                ignore_lock: false,
            },
        );
    }

    /// Entries of a library as GAEB X81: they go into a small project of
    /// their own (numbered), then the file dialog asks for the name.
    fn export_library_part(&mut self, ctx: &egui::Context, taken: &Transfer, label: &str) {
        let name = ProjectName::parse("Bibliothek").expect("valid name");
        // As many title levels as the entries use (GAEB would fill unused ones).
        let levels = taken
            .rows
            .iter()
            .filter_map(|row| row.as_group().map(|group| group.level() + 1))
            .max()
            .unwrap_or(1);
        let pattern = ["11", "22", "33", "44", "55"][..levels.clamp(1, 5)].join(".") + ".PPPP";
        let mask = OzMask::parse(&pattern).expect("valid mask");
        let mut part = Project::new(name, self.launcher.now_iso(), Lv::new(label, mask));
        if let Err(error) = transfer::insert(&mut part, None, taken) {
            self.say(ctx, error);
            return;
        }
        part.lv_mut().renumber();
        self.pending_library_export = Some(Box::new(part));
        self.open_file_dialog(FilePurpose::ExportLibrary);
    }

    /// The file for „Auswahl als GAEB X81 …“ was chosen.
    pub(super) fn write_library_export(&mut self, ctx: &egui::Context, file: PathBuf) {
        let Some(part) = self.pending_library_export.take() else {
            return;
        };
        let exchange = self.launcher.shared_exchange();
        self.start_job(ctx, "X81 wird erzeugt …".to_string(), move || {
            let result = exchange.export_gaeb(&part, GaebExport::Description, &file);
            super::JobOutcome::Gaeb {
                kind: GaebExport::Description,
                file,
                result,
                lv: usize::MAX,
            }
        });
    }
}
