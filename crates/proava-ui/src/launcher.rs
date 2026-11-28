//! Start screen: recent projects, new project, backups, settings, folder picker.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Align2, FontId, Rect, RichText, Sense, Stroke, Vec2};
use proava_application::{
    Launcher, ProjectKind, ProjectPreview, RecentProject, TemplateEntry, TemplateSource,
    ThemeChoice,
};
use proava_domain::{Project, ProjectName};

use crate::file_dialog::{DialogResult, FileDialog};
use crate::icons::Icon;
use crate::jobs::{Job, Poll};
use crate::settings_dialog::{SettingsDialog, SettingsResult};
use crate::theme::{self, Palette};
use crate::{format, strings, widgets};

mod recent_row;
mod start_extras;
use recent_row::recent_row;
use start_extras::RowCommand;

/// What a template brings (text) and whether it holds positions, or why it
/// cannot be read.
type TemplateLine = Result<(String, bool), String>;

/// Folder for the new project, the LV read on the worker thread and the
/// protocol of the import.
type ImportResult = (
    PathBuf,
    Result<(Project, proava_application::ImportProtocol), String>,
);

/// A project the host should open, with messages to show right away.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OpenRequest {
    pub path: PathBuf,
    pub messages: Vec<String>,
    /// The protocol of the import that created the project (shown first).
    pub protocol: Option<proava_application::ImportProtocol>,
    /// Open without saving (another window has it open, or only to copy from).
    pub read_only: bool,
    /// The question about another window's lock was answered already.
    pub ignore_lock: bool,
}

const HEADER_HEIGHT: f32 = 64.0;
const STATUS_HEIGHT: f32 = 30.0;
const ACTIONS_WIDTH: f32 = 344.0;

#[derive(Debug, Clone)]
enum Action {
    SetTheme(ThemeChoice),
    Select(PathBuf),
    Open(PathBuf),
    Forget(PathBuf),
    NewProjectDialog,
    OpenDialog,
    BackupDialog,
    RestoreDialog,
    SettingsDialog,
    ChooseFolder(PickerPurpose),
    Picked(PickerPurpose, Vec<PathBuf>),
    ImportGaebDialog,
    ConfirmImportGaeb,
    CreateProject,
    ConfirmOpen,
    ConfirmBackup,
    ConfirmRestore,
    /// Rechtsklick auf ein Projekt der Liste oder Knopf der Vorschau.
    Row(RowCommand, PathBuf),
    ConfirmCopy,
}

/// A button of the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusClick {
    /// „Neu starten“ after an update.
    Restart,
    /// „Ins Startmenü eintragen“ (AppImage).
    PutInMenu,
}

/// Button of the status bar while an AppImage is not in the start menu.
pub const APPIMAGE_MENU: &str = "Ins Startmenü eintragen";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialog {
    NewProject,
    OpenProject,
    Backup,
    Restore,
    Settings,
    PickFolder,
    ImportGaeb,
    CopyProject,
}

/// What the file dialog hands its result to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerPurpose {
    NewProject,
    ImportFolder,
    ProjectFile,
    BackupFile,
    GaebFile,
    /// Folder for „Kopie anlegen …“.
    CopyFolder,
    /// The new place of a project whose file is gone („Pfad suchen …“).
    Locate,
}

struct PickerState {
    dialog: FileDialog,
    purpose: PickerPurpose,
    back: Option<Dialog>,
}

pub struct LauncherScreen {
    launcher: Arc<Launcher>,
    /// „Schreibgeschützt öffnen“ in the open dialog.
    open_read_only: bool,
    theme_choice: ThemeChoice,
    palette: Palette,
    search: String,
    selected: Option<PathBuf>,
    notice: Option<(String, f64)>,
    /// Hints to confirm (e.g. repaired settings), shown as a small dialog.
    notes: Vec<String>,
    dialog: Option<Dialog>,
    picker: Option<PickerState>,
    open_request: Option<OpenRequest>,
    focus_first: bool,
    import_job: Option<Job<ImportResult>>,
    /// Recent projects whose file is gone (checked when the list is shown).
    missing: HashSet<PathBuf>,
    missing_checked_for: usize,
    /// The program file was replaced (update): notice and „Neu starten“.
    program_updated: bool,
    /// „Neu starten“ was clicked (the host restarts).
    restart_requested: bool,
    /// proAVA runs from this AppImage and the start menu does not start it:
    /// the status bar offers „Ins Startmenü eintragen“.
    appimage_not_in_menu: Option<PathBuf>,
    form_name: String,
    form_folder: String,
    form_file: String,
    form_target: String,
    form_error: Option<String>,
    /// Extras ▸ Einstellungen (same dialog as in the workspace).
    settings: Option<SettingsDialog>,
    /// „Vorlage“ in the dialog „Neues Projekt“.
    templates: Vec<TemplateEntry>,
    form_template: usize,
    form_keep_positions: bool,
    /// OZ-Maske of the new project: the template's (or the settings' for
    /// „leer“), changeable for „leer“ and the GA template.
    form_mask: String,
    /// The template `form_mask` was filled for.
    mask_for: Option<usize>,
    /// Line about the chosen template: (index, mask, text and whether it holds positions).
    template_info: Option<(usize, String, TemplateLine)>,
    /// Preview of the chosen row (path, content).
    preview: Option<(PathBuf, Result<ProjectPreview, String>)>,
    /// Source of „Kopie anlegen …“ and the missing project of „Pfad suchen …“.
    copy_source: Option<PathBuf>,
    relocating: Option<PathBuf>,
}

impl LauncherScreen {
    pub fn new(launcher: Arc<Launcher>) -> Self {
        let theme_choice = launcher.theme();
        let start_folder = launcher
            .suggested_folder()
            .unwrap_or_else(|| launcher.home_folder());

        Self {
            launcher,
            theme_choice,
            palette: Palette::of(theme_choice),
            search: String::new(),
            selected: None,
            notice: None,
            notes: Vec::new(),
            dialog: None,
            picker: None,
            open_request: None,
            missing: HashSet::new(),
            missing_checked_for: usize::MAX,
            program_updated: false,
            restart_requested: false,
            appimage_not_in_menu: crate::jobs::appimage_not_in_menu(),
            import_job: None,
            focus_first: false,
            form_name: String::new(),
            form_folder: start_folder.to_string_lossy().to_string(),
            form_file: String::new(),
            open_read_only: false,
            form_target: start_folder.to_string_lossy().to_string(),
            settings: None,
            form_error: None,
            templates: Vec::new(),
            form_template: 0,
            form_keep_positions: true,
            form_mask: String::new(),
            mask_for: None,
            template_info: None,
            preview: None,
            copy_source: None,
            relocating: None,
        }
    }

    /// Draws the start screen. Returns a project when the workspace should open it.
    /// Opens the dialog „Projekt öffnen“ (Projekt ▸ Projekt öffnen … in a project).
    pub fn show_open_dialog(&mut self) {
        self.dialog = Some(Dialog::OpenProject);
        self.focus_first = true;
        self.form_error = None;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<OpenRequest> {
        let ctx = ui.ctx().clone();
        theme::apply(&ctx, self.theme_choice);

        if let Some((_, started)) = self.notice {
            if ctx.input(|input| input.time) - started > 6.0 {
                self.notice = None;
            }
        }

        self.poll_import(&ctx);
        if self.missing_checked_for != self.launcher.recent().len() {
            self.refresh_missing();
        }

        let mut actions: Vec<Action> = Vec::new();
        if self.dialog.is_none() && !ctx.text_edit_focused() {
            if let Some(path) = self.selected.clone() {
                if ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                    actions.push(Action::Open(path));
                }
            }
        }

        self.header(ui, &mut actions);
        match self.status_bar(ui) {
            Some(StatusClick::Restart) => self.restart_requested = true,
            Some(StatusClick::PutInMenu) => self.put_appimage_in_menu(&ctx),
            None => {}
        }
        self.action_panel(ui, &mut actions);
        self.preview_panel(ui, &mut actions);
        self.recent_panel(ui, &mut actions);

        for action in actions {
            self.apply(&ctx, action);
        }

        let mut dialog_actions: Vec<Action> = Vec::new();
        self.dialogs(ui, &mut dialog_actions);
        for action in dialog_actions {
            self.apply(&ctx, action);
        }
        self.notes_dialog(ui);

        self.open_request.take()
    }

    fn header(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let palette = self.palette;
        let dark = self.theme_choice == ThemeChoice::Dark;

        egui::Panel::top("header")
            .exact_size(HEADER_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(strings::APP_TITLE)
                                .size(17.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.label(
                            RichText::new(strings::APP_SUBTITLE)
                                .size(11.0)
                                .color(palette.text_dim),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::square_icon_button(
                            ui,
                            &palette,
                            Icon::Gear,
                            strings::SETTINGS_HINT,
                        )
                        .clicked()
                        {
                            actions.push(Action::SettingsDialog);
                        }
                        ui.add_space(4.0);
                        let (icon, hint) = if dark {
                            (Icon::Sun, "Helles Aussehen")
                        } else {
                            (Icon::Moon, "Dunkles Aussehen")
                        };
                        if widgets::square_icon_button(ui, &palette, icon, hint).clicked() {
                            actions.push(Action::SetTheme(self.theme_choice.flipped()));
                        }
                    });
                });
            });
    }

    /// Right column: what is in the chosen project (read when the choice changes).
    fn preview_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let palette = self.palette;
        let item = self.selected.as_ref().and_then(|path| {
            self.launcher
                .recent()
                .into_iter()
                .find(|entry| entry.path == *path)
        });
        let missing = item
            .as_ref()
            .is_some_and(|item| self.missing.contains(&item.path));
        match &item {
            Some(item) if !missing => {
                if self.preview.as_ref().map(|(path, _)| path) != Some(&item.path) {
                    self.preview = Some((item.path.clone(), self.launcher.preview(&item.path)));
                }
            }
            _ => self.preview = None,
        }
        egui::Panel::right("preview")
            .exact_size(start_extras::PREVIEW_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.line))
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let command = start_extras::preview_panel(
                            ui,
                            &palette,
                            item.as_ref(),
                            self.preview.as_ref().map(|(_, preview)| preview),
                            missing,
                        );
                        if let (Some(command), Some(item)) = (command, &item) {
                            actions.push(Action::Row(command, item.path.clone()));
                        }
                    });
            });
    }

    fn recent_panel(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let palette = self.palette;
        let recent = self.launcher.recent();
        let selected = self.selected.clone();
        let mut search = self.search.clone();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(strings::SECTION_RECENT)
                            .size(14.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(crate::format::count(
                                recent.len(),
                                "Projekt",
                                "Projekte",
                            ))
                            .size(11.0)
                            .color(palette.text_dim),
                        );
                    });
                });

                ui.add_space(8.0);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text(strings::SEARCH_HINT)
                        .desired_width(ui.available_width())
                        .margin(egui::Margin {
                            left: 28,
                            right: 8,
                            top: 7,
                            bottom: 7,
                        }),
                );
                // Magnifier inside the field, like a usual search box.
                crate::icons::draw(
                    ui.painter(),
                    Rect::from_center_size(
                        egui::pos2(field.rect.left() + 15.0, field.rect.center().y),
                        Vec2::splat(14.0),
                    ),
                    Icon::Search,
                    palette.text_dim,
                    palette.background,
                );
                ui.add_space(10.0);

                let needle = search.trim().to_lowercase();
                let visible: Vec<RecentProject> = recent
                    .iter()
                    .filter(|item| {
                        needle.is_empty()
                            || item.name.to_lowercase().contains(&needle)
                            || item.path.to_string_lossy().to_lowercase().contains(&needle)
                    })
                    .cloned()
                    .collect();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if visible.is_empty() {
                            // No project yet (first start): also the disclaimer.
                            empty_state(ui, &palette, recent.is_empty());
                        } else {
                            for item in &visible {
                                let is_selected = selected.as_deref() == Some(item.path.as_path());
                                let missing = self.missing.contains(&item.path);
                                let (response, clicks) =
                                    recent_row(ui, &palette, item, is_selected, missing);
                                ui.add_space(6.0);
                                if let Some(command) = start_extras::row_menu(&response, missing) {
                                    actions.push(Action::Select(item.path.clone()));
                                    actions.push(Action::Row(command, item.path.clone()));
                                } else if response.secondary_clicked() {
                                    actions.push(Action::Select(item.path.clone()));
                                }
                                if clicks.remove {
                                    actions.push(Action::Forget(item.path.clone()));
                                } else if clicks.locate {
                                    actions
                                        .push(Action::Row(RowCommand::Locate, item.path.clone()));
                                } else if response.double_clicked() {
                                    actions.push(Action::Open(item.path.clone()));
                                } else if response.clicked() {
                                    actions.push(Action::Select(item.path.clone()));
                                }
                            }
                        }
                    });
            });

        self.search = search;
    }

    fn action_panel(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let palette = self.palette;
        let selected = self.selected.clone();
        let selected_name = selected
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "Kein Projekt gewählt".to_string());

        // Fest links (Anwender, 17.11.): Aktionen links, die Projektliste rechts daneben.
        egui::Panel::left("actions")
            .exact_size(ACTIONS_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(20, 20)),
            )
            .show(ui, |ui| {
                card(ui, &palette, strings::SECTION_PROJECT, |ui| {
                    if action_button(ui, &palette, Icon::Plus, strings::NEW_PROJECT, true).clicked()
                    {
                        actions.push(Action::NewProjectDialog);
                    }
                    ui.add_space(8.0);
                    if action_button(ui, &palette, Icon::Folder, strings::OPEN_PROJECT, false)
                        .clicked()
                    {
                        actions.push(Action::OpenDialog);
                    }
                    ui.add_space(8.0);
                    if action_button(ui, &palette, Icon::Import, strings::IMPORT_GAEB, false)
                        .clicked()
                    {
                        actions.push(Action::ImportGaebDialog);
                    }
                });

                card(ui, &palette, strings::SECTION_BACKUP, |ui| {
                    ui.label(
                        RichText::new(&selected_name)
                            .size(12.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(10.0);
                    if action_button(ui, &palette, Icon::Save, strings::CREATE_BACKUP, false)
                        .clicked()
                    {
                        actions.push(Action::BackupDialog);
                    }
                    ui.add_space(8.0);
                    if action_button(ui, &palette, Icon::Restore, strings::RESTORE_BACKUP, false)
                        .clicked()
                    {
                        actions.push(Action::RestoreDialog);
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(strings::BACKUP_HINT)
                            .size(10.5)
                            .color(palette.text_dim),
                    );
                });

                if let Some(path) = selected {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(strings::FORGET_PROJECT)
                                    .size(11.5)
                                    .color(palette.text_dim),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        actions.push(Action::Forget(path));
                    }
                }
            });
    }

    /// Looks once per list change which project files are gone.
    fn refresh_missing(&mut self) {
        let recent = self.launcher.recent();
        self.missing_checked_for = recent.len();
        self.missing = recent
            .into_iter()
            .map(|entry| entry.path)
            .filter(|path| {
                let Some(folder) = path.parent() else {
                    return true;
                };
                !self.launcher.is_folder(folder) || !self.launcher.files(folder).contains(path)
            })
            .collect();
    }

    fn poll_import(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.import_job else { return };
        match job.poll() {
            Poll::Running => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Poll::Lost => {
                self.import_job = None;
                self.toast(ctx, "Das Einlesen ist abgebrochen (interner Fehler).");
            }
            Poll::Done((folder, result)) => {
                self.import_job = None;
                let created = result.and_then(|(project, protocol)| {
                    self.launcher
                        .create_project_with(&folder, &project)
                        .map(|entry| (entry, protocol))
                });
                match created {
                    Ok((entry, protocol)) => {
                        self.dialog = None;
                        self.form_error = None;
                        self.selected = Some(entry.path.clone());
                        self.open_request = Some(OpenRequest {
                            path: entry.path,
                            protocol: Some(protocol),
                            ..OpenRequest::default()
                        });
                    }
                    Err(error) => {
                        self.form_error = Some(error);
                        self.dialog = Some(Dialog::ImportGaeb);
                    }
                }
            }
        }
    }

    /// The status bar and which of its buttons was clicked.
    fn status_bar(&self, ui: &mut egui::Ui) -> Option<StatusClick> {
        let palette = self.palette;
        let mut clicked = None;
        let notice = self.notice.as_ref().map(|(text, _)| text.clone());
        let running = self.import_job.as_ref().map(|job| job.label().to_string());

        egui::Panel::bottom("status")
            .exact_size(STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(20, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Version {} · Über proAVA …",
                                    env!("CARGO_PKG_VERSION")
                                ))
                                .size(10.5)
                                .color(palette.text_dim),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        crate::about::request(ui.ctx());
                    }
                    ui.add_space(14.0);
                    if ui
                        .add(
                            egui::Label::new(
                                RichText::new("Hilfe (F1) · Handbuch …")
                                    .size(10.5)
                                    .color(palette.text_dim),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        crate::help::request(ui.ctx(), crate::help::Topic::Start);
                    }
                    if let Some(label) = running {
                        ui.add_space(14.0);
                        ui.add(egui::Spinner::new().size(14.0).color(palette.accent));
                        ui.label(RichText::new(label).size(12.5).color(palette.accent));
                    }
                    if let Some(text) = notice {
                        ui.add_space(14.0);
                        ui.label(RichText::new(text).size(12.5).color(palette.accent));
                    }
                    if self.program_updated {
                        ui.add_space(14.0);
                        ui.label(
                            RichText::new(crate::update_notice::TEXT)
                                .size(12.5)
                                .strong()
                                .color(palette.warn),
                        );
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(crate::update_notice::RESTART).size(12.5),
                                )
                                .small(),
                            )
                            .on_hover_text(crate::update_notice::RESTART_HOVER)
                            .clicked()
                        {
                            clicked = Some(StatusClick::Restart);
                        }
                    }
                    if let Some(image) = &self.appimage_not_in_menu {
                        ui.add_space(14.0);
                        if ui
                            .add(egui::Button::new(RichText::new(APPIMAGE_MENU).size(12.5)).small())
                            .on_hover_text(format!(
                                "proAVA läuft als AppImage ({}) und steht noch nicht im Startmenü. \
                                 Eintragen legt den Eintrag im Startmenü an und öffnet .proava- und \
                                 GAEB-Dateien mit diesem AppImage.",
                                image.display()
                            ))
                            .clicked()
                        {
                            clicked = Some(StatusClick::PutInMenu);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(strings::STATUS_MCP)
                                .size(10.5)
                                .color(palette.text_dim),
                        );
                    });
                });
            });
        clicked
    }

    /// „Ins Startmenü eintragen“: `install_linux.sh --appimage` from the image.
    fn put_appimage_in_menu(&mut self, ctx: &egui::Context) {
        match crate::jobs::put_appimage_in_menu() {
            Ok(_) => {
                self.appimage_not_in_menu = crate::jobs::appimage_not_in_menu();
                self.toast(
                    ctx,
                    "proAVA steht im Startmenü; .proava-Dateien öffnen dieses AppImage.",
                );
            }
            Err(error) => self.toast(ctx, error),
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::SetTheme(theme_choice) => {
                self.theme_choice = theme_choice;
                self.palette = Palette::of(theme_choice);
                theme::apply(ctx, theme_choice);
                self.launcher.set_theme(theme_choice);
            }
            Action::Select(path) => {
                self.selected = Some(path);
            }
            Action::Open(path) => {
                if ProjectKind::from_path(&path) == ProjectKind::Other {
                    self.selected = Some(path.clone());
                    let name = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.toast(
                        ctx,
                        format!("„{name}“ ist keine proAVA-Projektdatei – Leistungsverzeichnisse anderer Programme über den GAEB-Import übernehmen."),
                    );
                    return;
                }
                match self.launcher.open_project(&path) {
                    Ok(entry) => {
                        self.selected = Some(entry.path.clone());
                        self.open_request = Some(OpenRequest {
                            path: entry.path,
                            read_only: std::mem::take(&mut self.open_read_only),
                            ..OpenRequest::default()
                        });
                    }
                    Err(error) => self.toast(ctx, error),
                }
            }
            Action::Forget(path) => {
                self.launcher.forget(&path);
                if self.selected.as_deref() == Some(path.as_path()) {
                    self.selected = None;
                }
            }
            Action::Row(command, path) => match command {
                RowCommand::Open => self.apply(ctx, Action::Open(path)),
                RowCommand::OpenReadOnly => {
                    // Nur lesen und kopieren, wie „Schreibgeschützt öffnen“ im Dialog Öffnen.
                    self.open_read_only = true;
                    self.apply(ctx, Action::Open(path));
                }
                RowCommand::Forget => self.apply(ctx, Action::Forget(path)),
                RowCommand::ShowFolder => {
                    let folder = nearest_folder(self.launcher.as_ref(), &path)
                        .unwrap_or_else(|| self.launcher.home_folder());
                    if let Err(error) = crate::jobs::open_with_system(&folder) {
                        self.toast(ctx, error);
                    }
                }
                RowCommand::Copy => {
                    let stem = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.form_name = format!("{stem} Kopie");
                    if let Some(folder) = projects_folder(&path) {
                        self.form_folder = folder.to_string_lossy().to_string();
                    }
                    self.copy_source = Some(path);
                    self.form_error = None;
                    self.focus_first = true;
                    self.dialog = Some(Dialog::CopyProject);
                }
                RowCommand::Locate => {
                    self.relocating = Some(path);
                    self.apply(ctx, Action::ChooseFolder(PickerPurpose::Locate));
                }
            },
            Action::ConfirmCopy => {
                let Some(source) = self.copy_source.clone() else {
                    self.dialog = None;
                    return;
                };
                let folder = PathBuf::from(self.form_folder.trim());
                if let Err(error) = check_folder(self.launcher.as_ref(), &self.form_folder) {
                    self.form_error = Some(error);
                    return;
                }
                match self
                    .launcher
                    .copy_project(&source, &folder, &self.form_name)
                {
                    Ok(entry) => {
                        self.dialog = None;
                        self.form_error = None;
                        self.copy_source = None;
                        self.selected = Some(entry.path.clone());
                        self.toast(ctx, format!("Kopie angelegt: {}", entry.path.display()));
                    }
                    Err(error) => self.form_error = Some(error),
                }
            }
            Action::NewProjectDialog => {
                self.dialog = Some(Dialog::NewProject);
                self.focus_first = true;
                self.form_error = None;
                self.form_name.clear();
                self.templates = self.launcher.templates();
                let last = self.launcher.last_template();
                self.form_template = self
                    .templates
                    .iter()
                    .position(|entry| Some(&entry.name) == last.as_ref())
                    .unwrap_or(0);
                self.form_keep_positions = true;
                self.mask_for = None;
                self.template_info = None;
                if let Some(folder) = self.launcher.suggested_folder() {
                    self.form_folder = folder.to_string_lossy().to_string();
                }
            }
            Action::OpenDialog => {
                self.dialog = Some(Dialog::OpenProject);
                self.focus_first = true;
                self.form_error = None;
            }
            Action::ImportGaebDialog => {
                self.dialog = None;
                self.form_error = None;
                self.apply(ctx, Action::ChooseFolder(PickerPurpose::GaebFile));
            }
            Action::ConfirmImportGaeb => {
                if self.import_job.is_some() {
                    return;
                }
                let folder = PathBuf::from(self.form_folder.trim());
                let file = PathBuf::from(self.form_file.trim());
                let name = match ProjectName::parse(&self.form_name) {
                    Ok(name) => name,
                    Err(error) => {
                        self.form_error = Some(error.to_string());
                        return;
                    }
                };
                let exchange = self.launcher.shared_exchange();
                let created = self.launcher.now_iso();
                self.dialog = None;
                self.form_error = None;
                self.import_job = Some(Job::spawn(
                    format!("{} wird eingelesen …", crate::file_dialog::file_name(&file)),
                    move || {
                        // Read like „GAEB ▸ Importieren …“, with its protocol.
                        use proava_application::gaeb_import::{self, ImportTarget};
                        let name = name.as_str();
                        let result = gaeb_import::read(exchange.as_ref(), &file, &created)
                            .and_then(|import| {
                                let project = gaeb_import::as_project(&import, name, true)?;
                                let target = ImportTarget::Project {
                                    file: folder.join(name).join(format!("{name}.proava")),
                                };
                                let protocol = gaeb_import::protocol(&import, &target, true);
                                Ok((project, protocol))
                            });
                        (folder, result)
                    },
                ));
            }
            Action::BackupDialog => {
                let Some(project) = self.selected.clone() else {
                    self.toast(ctx, "Bitte zuerst ein Projekt in der Liste wählen.");
                    return;
                };
                if let Some(folder) = project.parent() {
                    self.form_target = folder.to_string_lossy().to_string();
                }
                self.form_error = None;
                self.focus_first = true;
                self.dialog = Some(Dialog::Backup);
            }
            Action::RestoreDialog => {
                self.dialog = Some(Dialog::Restore);
                self.focus_first = true;
                self.form_error = None;
                self.form_file.clear();
            }
            Action::SettingsDialog => {
                self.settings = Some(SettingsDialog::of(&self.launcher));
                self.dialog = Some(Dialog::Settings);
                self.form_error = None;
            }
            Action::ChooseFolder(purpose) => {
                let typed_folder = PathBuf::from(self.form_folder.trim());
                let fallback = self
                    .launcher
                    .suggested_folder()
                    .unwrap_or_else(|| self.launcher.home_folder());
                let start = match purpose {
                    PickerPurpose::NewProject
                    | PickerPurpose::ImportFolder
                    | PickerPurpose::CopyFolder => {
                        if self.launcher.is_folder(&typed_folder) {
                            typed_folder
                        } else {
                            fallback
                        }
                    }
                    PickerPurpose::Locate => self
                        .relocating
                        .as_deref()
                        .and_then(|missing| nearest_folder(self.launcher.as_ref(), missing))
                        .unwrap_or(fallback),
                    PickerPurpose::ProjectFile
                    | PickerPurpose::BackupFile
                    | PickerPurpose::GaebFile => {
                        let typed = PathBuf::from(self.form_file.trim());
                        typed
                            .parent()
                            .filter(|parent| self.launcher.is_folder(parent))
                            .map(Path::to_path_buf)
                            .or_else(|| {
                                self.selected
                                    .as_ref()
                                    .and_then(|path| path.parent().map(Path::to_path_buf))
                            })
                            .unwrap_or(fallback)
                    }
                };
                let launcher = self.launcher.as_ref();
                let dialog = match purpose {
                    PickerPurpose::NewProject
                    | PickerPurpose::ImportFolder
                    | PickerPurpose::CopyFolder => {
                        FileDialog::folder(launcher, strings::PICKER_TITLE, start)
                    }
                    PickerPurpose::Locate => FileDialog::open(
                        launcher,
                        "Pfad suchen",
                        "Die Projektdatei an ihrem neuen Ort wählen; der Eintrag in der Liste zeigt dann dorthin.",
                        start,
                        &["proava"],
                        false,
                    ),
                    PickerPurpose::ProjectFile => FileDialog::open(
                        launcher,
                        strings::DIALOG_OPEN_TITLE,
                        strings::DIALOG_OPEN_HINT,
                        start,
                        &["proava"],
                        false,
                    ),
                    PickerPurpose::BackupFile => FileDialog::open(
                        launcher,
                        strings::DIALOG_RESTORE_TITLE,
                        "Sicherung (Zip) wählen.",
                        start,
                        &["zip"],
                        false,
                    ),
                    PickerPurpose::GaebFile => FileDialog::open(
                        launcher,
                        strings::IMPORT_GAEB_TITLE,
                        "GAEB-Datei mit dem LV wählen (X81 … X86, D81 … D86) oder eine CSV-Tabelle.",
                        start,
                        &strings::NEW_PROJECT_EXTENSIONS,
                        false,
                    ),
                };
                self.picker = Some(PickerState {
                    dialog,
                    purpose,
                    back: self.dialog,
                });
                self.dialog = Some(Dialog::PickFolder);
            }
            Action::Picked(purpose, paths) => {
                let back = self.picker.take().and_then(|picker| picker.back);
                self.dialog = back;
                let Some(path) = paths.first().cloned() else {
                    return;
                };
                let text = path.to_string_lossy().to_string();
                match purpose {
                    PickerPurpose::NewProject
                    | PickerPurpose::ImportFolder
                    | PickerPurpose::CopyFolder => self.form_folder = text,
                    PickerPurpose::Locate => {
                        let Some(missing) = self.relocating.take() else {
                            return;
                        };
                        match self.launcher.relocate(&missing, &path) {
                            Ok(entry) => {
                                self.selected = Some(entry.path);
                                self.preview = None;
                                self.missing_checked_for = usize::MAX;
                                self.toast(ctx, "Eintrag zeigt jetzt auf die gewählte Datei.");
                            }
                            Err(error) => self.toast(ctx, error),
                        }
                    }
                    PickerPurpose::ProjectFile => {
                        self.dialog = None;
                        self.apply(ctx, Action::Open(path));
                    }
                    PickerPurpose::BackupFile => self.form_file = text,
                    PickerPurpose::GaebFile => self.import_gaeb_file(&path),
                }
            }
            Action::CreateProject => {
                let folder = PathBuf::from(self.form_folder.trim());
                let (template_name, source) = self
                    .templates
                    .get(self.form_template)
                    .map(|entry| (entry.name.clone(), entry.source.clone()))
                    .unwrap_or_else(|| ("leer".to_string(), TemplateSource::Empty));
                let keep = self.form_keep_positions;
                let mask = source.mask_changeable().then_some(self.form_mask.as_str());
                match self.launcher.create_project_from_template_with_mask(
                    &folder,
                    &self.form_name,
                    &source,
                    keep,
                    mask,
                ) {
                    Ok(entry) => {
                        self.launcher.set_last_template(&template_name);
                        self.selected = Some(entry.path.clone());
                        self.form_folder = folder.to_string_lossy().to_string();
                        self.form_target = folder.to_string_lossy().to_string();
                        self.dialog = None;
                        self.form_error = None;
                        self.open_request = Some(OpenRequest {
                            path: entry.path,
                            read_only: false,
                            ..OpenRequest::default()
                        });
                    }
                    Err(error) => self.form_error = Some(error),
                }
            }
            Action::ConfirmOpen => {
                let path = PathBuf::from(self.form_file.trim());
                self.dialog = None;
                self.apply(ctx, Action::Open(path));
            }
            Action::ConfirmBackup => {
                let Some(project) = self.selected.clone() else {
                    self.toast(ctx, "Bitte zuerst ein Projekt in der Liste wählen.");
                    return;
                };
                let target = PathBuf::from(self.form_target.trim());
                if let Err(error) = check_folder(self.launcher.as_ref(), &self.form_target) {
                    self.form_error = Some(error);
                    return;
                }
                match self.launcher.backup(&project, &target) {
                    Ok(file) => {
                        self.dialog = None;
                        self.form_error = None;
                        self.toast(ctx, format!("Sicherung angelegt: {}", file.display()));
                    }
                    Err(error) => self.form_error = Some(error),
                }
            }
            Action::ConfirmRestore => {
                let backup = PathBuf::from(self.form_file.trim());
                let target = PathBuf::from(self.form_target.trim());
                if self.form_file.trim().is_empty() {
                    self.form_error = Some("Bitte die Sicherungsdatei (.zip) wählen.".to_string());
                    return;
                }
                if let Err(error) = check_folder(self.launcher.as_ref(), &self.form_target) {
                    self.form_error = Some(error);
                    return;
                }
                match self.launcher.restore(&backup, &target) {
                    Ok(file) => {
                        self.dialog = None;
                        self.form_error = None;
                        // In die Liste aufnehmen und markieren – Doppelklick öffnet.
                        if let Ok(entry) = self.launcher.open_project(&file) {
                            self.selected = Some(entry.path);
                        }
                        self.toast(ctx, format!("Wiederhergestellt: {}", file.display()));
                    }
                    Err(error) => self.form_error = Some(error),
                }
            }
        }
    }

    /// Shows a message in the status bar, e.g. when a project cannot be read.
    /// Opens „Neues Projekt aus GAEB“ for this file (also from the command line).
    /// Whether a GAEB import still runs in the background.
    pub fn busy(&self) -> bool {
        self.import_job.is_some()
    }

    pub fn import_gaeb_file(&mut self, path: &Path) {
        self.form_file = path.to_string_lossy().to_string();
        self.form_name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim().to_string())
            .unwrap_or_default();
        if let Some(folder) = self.launcher.suggested_folder() {
            self.form_folder = folder.to_string_lossy().to_string();
        }
        self.form_error = None;
        self.focus_first = true;
        self.dialog = Some(Dialog::ImportGaeb);
    }

    /// Shows `lines` in a dialog „Hinweise“ until the user confirms them.
    pub fn notify(&mut self, lines: Vec<String>) {
        self.notes.extend(lines);
    }

    fn notes_dialog(&mut self, ui: &mut egui::Ui) {
        if self.notes.is_empty() {
            return;
        }
        let palette = self.palette;
        let (done, close) = widgets::modal(ui, &palette, 620.0, |ui| {
            widgets::title(ui, &palette, strings::WARNINGS_TITLE, "");
            for line in &self.notes {
                ui.label(
                    RichText::new(format!("• {line}"))
                        .size(13.0)
                        .color(palette.text),
                );
            }
            ui.add_space(12.0);
            widgets::accent_button(ui, &palette, strings::DONE)
        });
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        if done || close || enter {
            self.notes.clear();
        }
    }

    pub fn failed(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.toast(ctx, message);
    }

    /// „Schrittweite 10 · 7 Titel · …“ for the chosen template with the
    /// OZ-Maske of the dialog.
    fn describe_template(&self) -> TemplateLine {
        let Some(entry) = self.templates.get(self.form_template) else {
            return Ok((String::new(), false));
        };
        let mask = if entry.source.mask_changeable() {
            Some(
                proava_domain::OzMask::parse(self.form_mask.trim())
                    .map_err(|problem| format!("OZ-Maske: {problem}"))?,
            )
        } else {
            None
        };
        match self
            .launcher
            .template_project_with_mask(&entry.source, mask.as_ref())?
        {
            None => {
                let preferences = self.launcher.preferences();
                Ok((
                    format!(
                        "Ein leeres LV mit Schrittweite {} aus den Einstellungen.",
                        preferences.new_lv_step
                    ),
                    false,
                ))
            }
            Some(project) => {
                let summary = proava_domain::template::summary(&project);
                let lv = project.lv();
                let mut text = format!(
                    "Schrittweite {} · {}",
                    lv.position_step(),
                    summary.describe()
                );
                if entry.source == TemplateSource::BuiltIn
                    && proava_domain::template::cost_group_level(lv.oz_mask()).is_none()
                {
                    text.push_str(strings::GA_TEMPLATE_MASK_HINT);
                }
                Ok((text, summary.positions > 0))
            }
        }
    }

    /// An update waits for a restart (`update_notice`): the status bar says so.
    pub fn set_program_updated(&mut self, updated: bool) {
        self.program_updated = updated;
    }

    /// Whether „Neu starten“ was clicked since the last call.
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }

    /// A short message in the status bar (e.g. a restart that failed).
    pub fn say(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.toast(ctx, message);
    }

    fn toast(&mut self, ctx: &egui::Context, message: impl Into<String>) {
        self.notice = Some((message.into(), ctx.input(|input| input.time)));
        ctx.request_repaint_after(Duration::from_secs(7));
    }

    fn dialogs(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(dialog) = self.dialog else {
            return;
        };

        let palette = self.palette;
        let ctx = ui.ctx().clone();

        if dialog == Dialog::Settings {
            let Some(settings) = self.settings.as_mut() else {
                self.dialog = None;
                return;
            };
            match settings.show(ui, &palette, &self.launcher) {
                SettingsResult::Open => {}
                SettingsResult::Cancelled => {
                    self.settings = None;
                    self.dialog = None;
                }
                SettingsResult::Applied => {
                    self.settings = None;
                    self.dialog = None;
                    let theme_choice = self.launcher.theme();
                    self.theme_choice = theme_choice;
                    self.palette = Palette::of(theme_choice);
                    theme::apply(&ctx, theme_choice);
                    self.toast(&ctx, "Einstellungen übernommen.");
                }
            }
            return;
        }

        if dialog == Dialog::PickFolder {
            let Some(picker) = self.picker.as_mut() else {
                self.dialog = None;
                return;
            };
            match picker.dialog.show(ui, &palette, self.launcher.as_ref()) {
                DialogResult::Open => {}
                DialogResult::Cancelled => {
                    let back = self.picker.take().and_then(|picker| picker.back);
                    self.dialog = back;
                }
                DialogResult::Chosen(paths) => {
                    let purpose = picker.purpose;
                    self.apply(&ctx, Action::Picked(purpose, paths));
                }
            }
            return;
        }

        if dialog == Dialog::NewProject {
            if self.mask_for != Some(self.form_template) {
                let mask = self
                    .templates
                    .get(self.form_template)
                    .and_then(|entry| self.launcher.template_mask(&entry.source).ok())
                    .unwrap_or_default();
                self.form_mask = mask;
                self.mask_for = Some(self.form_template);
            }
            let mask = self.form_mask.trim().to_string();
            let current = self
                .template_info
                .as_ref()
                .is_some_and(|(index, shown, _)| *index == self.form_template && *shown == mask);
            if !current {
                self.template_info = Some((self.form_template, mask, self.describe_template()));
            }
        }

        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let mut focus = std::mem::take(&mut self.focus_first);
        let mut cancelled = false;
        let mut confirmed = false;
        let (_, close) = widgets::modal(ui, &palette, 500.0, |ui| match dialog {
            Dialog::NewProject => {
                widgets::title(
                    ui,
                    &palette,
                    strings::DIALOG_NEW_TITLE,
                    strings::DIALOG_NEW_HINT,
                );
                widgets::focus_field(
                    ui,
                    &palette,
                    strings::FIELD_NAME,
                    &mut self.form_name,
                    &mut focus,
                );
                folder_field(
                    ui,
                    &palette,
                    strings::FIELD_FOLDER,
                    &mut self.form_folder,
                    PickerPurpose::NewProject,
                    actions,
                );
                start_extras::template_choice(
                    ui,
                    &palette,
                    &self.templates,
                    &mut self.form_template,
                    &mut self.form_mask,
                    self.template_info.as_ref().map(|(_, _, info)| info),
                    &mut self.form_keep_positions,
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, strings::CREATE);
                cancelled = no;
                confirmed = yes;
            }
            Dialog::ImportGaeb => {
                let csv = crate::import_dialog::CSV_EXTENSIONS
                    .iter()
                    .any(|extension| {
                        Path::new(self.form_file.trim())
                            .extension()
                            .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
                    });
                let (title, hint) = if csv {
                    (strings::IMPORT_CSV_TITLE, strings::IMPORT_CSV_HINT)
                } else {
                    (strings::IMPORT_GAEB_TITLE, strings::IMPORT_GAEB_HINT)
                };
                widgets::title(ui, &palette, title, hint);
                ui.label(
                    RichText::new(strings::FIELD_FILE)
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.label(
                    RichText::new(format::truncate_middle(&self.form_file, 70))
                        .size(13.0)
                        .color(palette.text),
                );
                ui.add_space(10.0);
                widgets::focus_field(ui, &palette, "Projektname", &mut self.form_name, &mut focus);
                folder_field(
                    ui,
                    &palette,
                    strings::FIELD_FOLDER,
                    &mut self.form_folder,
                    PickerPurpose::ImportFolder,
                    actions,
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, strings::CREATE);
                cancelled = no;
                confirmed = yes;
            }
            Dialog::OpenProject => {
                widgets::title(
                    ui,
                    &palette,
                    strings::DIALOG_OPEN_TITLE,
                    strings::DIALOG_OPEN_HINT,
                );
                file_field(
                    ui,
                    &palette,
                    &mut self.form_file,
                    &mut focus,
                    PickerPurpose::ProjectFile,
                    actions,
                );
                ui.add_space(6.0);
                ui.checkbox(
                    &mut self.open_read_only,
                    "Schreibgeschützt öffnen (nur lesen und kopieren)",
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, strings::OPEN);
                cancelled = no;
                confirmed = yes;
            }
            Dialog::Backup => {
                widgets::title(
                    ui,
                    &palette,
                    strings::DIALOG_BACKUP_TITLE,
                    strings::DIALOG_BACKUP_HINT,
                );
                if let Some(project) = &self.selected {
                    ui.label(
                        RichText::new(format::truncate_middle(&project.to_string_lossy(), 60))
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add_space(10.0);
                }
                widgets::focus_field(
                    ui,
                    &palette,
                    strings::FIELD_TARGET_FOLDER,
                    &mut self.form_target,
                    &mut focus,
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, strings::CREATE);
                cancelled = no;
                confirmed = yes;
            }
            Dialog::Restore => {
                widgets::title(
                    ui,
                    &palette,
                    strings::DIALOG_RESTORE_TITLE,
                    strings::DIALOG_RESTORE_HINT,
                );
                file_field(
                    ui,
                    &palette,
                    &mut self.form_file,
                    &mut focus,
                    PickerPurpose::BackupFile,
                    actions,
                );
                widgets::form_field(
                    ui,
                    &palette,
                    strings::FIELD_TARGET_FOLDER,
                    &mut self.form_target,
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, strings::RESTORE);
                cancelled = no;
                confirmed = yes;
            }
            Dialog::CopyProject => {
                widgets::title(
                    ui,
                    &palette,
                    "Kopie anlegen",
                    "Das ganze Projekt mit allen LVs, Angeboten und Bildern unter neuem Namen; die Kopie bekommt einen eigenen Ordner.",
                );
                if let Some(source) = &self.copy_source {
                    ui.label(
                        RichText::new(format::truncate_middle(&source.to_string_lossy(), 70))
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add_space(10.0);
                }
                widgets::focus_field(
                    ui,
                    &palette,
                    "Name der Kopie",
                    &mut self.form_name,
                    &mut focus,
                );
                folder_field(
                    ui,
                    &palette,
                    strings::FIELD_FOLDER,
                    &mut self.form_folder,
                    PickerPurpose::CopyFolder,
                    actions,
                );
                widgets::error_line(ui, &palette, self.form_error.as_deref());
                let (no, yes) = widgets::dialog_buttons(ui, &palette, "Kopie anlegen");
                cancelled = no;
                confirmed = yes;
            }
            Dialog::Settings => {}
            Dialog::PickFolder => {}
        });

        if close || cancelled {
            self.dialog = None;
            self.form_error = None;
            return;
        }

        if confirmed || (enter && dialog != Dialog::Settings && actions.is_empty()) {
            match dialog {
                Dialog::NewProject => self.apply(&ctx, Action::CreateProject),
                Dialog::ImportGaeb => self.apply(&ctx, Action::ConfirmImportGaeb),
                Dialog::OpenProject => self.apply(&ctx, Action::ConfirmOpen),
                Dialog::Backup => self.apply(&ctx, Action::ConfirmBackup),
                Dialog::Restore => self.apply(&ctx, Action::ConfirmRestore),
                Dialog::CopyProject => self.apply(&ctx, Action::ConfirmCopy),
                Dialog::Settings => self.dialog = None,
                Dialog::PickFolder => {}
            }
            // Enter nimmt einem einzeiligen Feld den Fokus; bei einem Fehler gleich weitertippen können.
            if self.dialog == Some(dialog) && self.form_error.is_some() {
                self.focus_first = true;
            }
        }
    }
}

/// A target folder typed by hand must exist (nothing is created silently).
fn check_folder(launcher: &Launcher, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Bitte einen Zielordner eintragen.".to_string());
    }
    if !launcher.is_folder(Path::new(text)) {
        return Err(format!("Den Ordner „{text}“ gibt es nicht."));
    }
    Ok(())
}

/// Path field with a button that opens the file dialog.
fn file_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    value: &mut String,
    focus: &mut bool,
    purpose: PickerPurpose,
    actions: &mut Vec<Action>,
) {
    ui.label(
        RichText::new(strings::FIELD_FILE)
            .size(12.0)
            .color(palette.text_dim),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let button_width = 132.0;
        let field_width = (ui.available_width() - button_width - 8.0).max(120.0);
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(field_width)
                .margin(egui::Margin::symmetric(8, 7)),
        );
        if std::mem::take(focus) {
            response.request_focus();
        }
        if ui
            .add(
                egui::Button::new(RichText::new("Datei wählen …").size(12.5))
                    .min_size(egui::vec2(button_width, 30.0)),
            )
            .clicked()
        {
            actions.push(Action::ChooseFolder(purpose));
        }
    });
    ui.add_space(10.0);
}

fn card(ui: &mut egui::Ui, palette: &Palette, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(palette.panel)
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .size(11.5)
                    .strong()
                    .color(palette.text_dim),
            );
            ui.add_space(8.0);
            body(ui);
        });
    ui.add_space(14.0);
}

fn action_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    primary: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::click());
    widgets::name_button(ui, &response, label);

    let (background, foreground) = if primary {
        (
            if response.hovered() {
                palette.accent_hover
            } else {
                palette.accent
            },
            palette.on_accent,
        )
    } else if response.hovered() {
        (palette.button_hover, palette.text)
    } else {
        (palette.button, palette.text)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(8), background);
    if !primary {
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            Stroke::new(1.0, palette.line),
            egui::StrokeKind::Inside,
        );
    }

    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 22.0, rect.center().y),
        Vec2::splat(16.0),
    );
    crate::icons::draw(painter, icon_rect, icon, foreground, background);
    painter.text(
        egui::pos2(rect.left() + 40.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        foreground,
    );

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The folder of a file, or the nearest one above that still exists.
fn nearest_folder(launcher: &Launcher, file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|folder| launcher.is_folder(folder))
        .map(Path::to_path_buf)
}

/// For `<Ordner>/<Name>/<Name>.proava` the `<Ordner>` (next to the project),
/// otherwise the folder of the file.
fn projects_folder(file: &Path) -> Option<PathBuf> {
    let parent = file.parent()?;
    let same = parent.file_name().map(|name| name.to_os_string())
        == file.file_stem().map(|stem| stem.to_os_string());
    match (same, parent.parent()) {
        (true, Some(grandparent)) => Some(grandparent.to_path_buf()),
        _ => Some(parent.to_path_buf()),
    }
}

/// `2026-11-17T17:56:24` → `17.11.2026 17:56`
fn german_date_time(iso: &str) -> String {
    let date = format::iso_to_german_date(iso);
    match iso.get(11..16) {
        Some(time) if time.as_bytes().get(2) == Some(&b':') => format!("{date} {time}"),
        _ => date,
    }
}

/// The empty list; `welcome` (no project used yet, the first start) adds the
/// disclaimer of warranty as a quiet line – no dialog.
fn empty_state(ui: &mut egui::Ui, palette: &Palette, welcome: bool) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(strings::EMPTY_TITLE)
                .size(15.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(strings::EMPTY_HINT)
                .size(11.5)
                .color(palette.text_dim),
        );
        if welcome {
            ui.add_space(28.0);
            ui.set_max_width(520.0);
            for text in [crate::about::WARRANTY_EN, crate::about::WARRANTY_DE] {
                ui.label(RichText::new(text).size(11.0).color(palette.text_dim));
                ui.add_space(4.0);
            }
        }
    });
}

/// Text field with a button that opens the folder picker.
fn folder_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    purpose: PickerPurpose,
    actions: &mut Vec<Action>,
) {
    let caption = ui.label(RichText::new(label).size(11.0).color(palette.text_dim));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let button_width = 132.0;
        let field_width = (ui.available_width() - button_width - 8.0).max(120.0);
        // Named after its caption (screen readers, UI tests).
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(field_width)
                .margin(egui::Margin::symmetric(8, 7)),
        )
        .labelled_by(caption.id);
        if ui
            .add(
                egui::Button::new(RichText::new(strings::CHOOSE_FOLDER).size(11.5))
                    .min_size(egui::vec2(button_width, 30.0)),
            )
            .clicked()
        {
            actions.push(Action::ChooseFolder(purpose));
        }
    });
    ui.add_space(10.0);
}

#[cfg(test)]
mod tests {
    use super::german_date_time;

    #[test]
    fn opened_shows_date_and_time() {
        assert_eq!(german_date_time("2026-11-17T17:56:24"), "17.11.2026 17:56");
        assert_eq!(german_date_time("2026-11-17"), "17.11.2026");
    }
}
