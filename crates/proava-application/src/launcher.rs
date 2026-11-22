use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use proava_domain::{Lv, OzMask, Project, ProjectName};

use crate::clipboard::{Clip, ClipHeader};
use crate::exchange::{ExchangeFiles, NoExchange};
use crate::ports::{Folders, NoWeb, ProjectLock, ProjectRepository, SettingsFiles, WebTexts};
use crate::settings::{PanelSide, Preferences, ProjectKind, RecentProject, Settings, ThemeChoice};

pub const MAX_RECENT_PROJECTS: usize = 12;

mod recent_files;
mod templates;

pub use recent_files::ProjectPreview;
pub use templates::{TemplateEntry, TemplateSource, TEMPLATE_FOLDER_NAME};

/// Use cases of the start screen and the frame around the workspace.
pub struct Launcher {
    projects: Box<dyn ProjectRepository>,
    settings_files: Box<dyn SettingsFiles>,
    folders: Box<dyn Folders>,
    exchange: Arc<dyn ExchangeFiles>,
    web: Arc<dyn WebTexts>,
    settings: RefCell<Settings>,
}

impl Launcher {
    pub fn new(
        projects: Box<dyn ProjectRepository>,
        settings_files: Box<dyn SettingsFiles>,
        folders: Box<dyn Folders>,
    ) -> Self {
        let settings = settings_files.load();
        Self {
            projects,
            settings_files,
            folders,
            exchange: Arc::new(NoExchange),
            web: Arc::new(NoWeb),
            settings: RefCell::new(settings),
        }
    }

    /// What loading the settings had to repair (once; then empty).
    pub fn take_settings_notes(&self) -> Vec<String> {
        std::mem::take(&mut self.settings.borrow_mut().notes)
    }

    /// Plugs in the GAEB/PDF adapter (the composition root does this).
    pub fn with_exchange(mut self, exchange: Box<dyn ExchangeFiles>) -> Self {
        self.exchange = Arc::from(exchange);
        self
    }

    /// Plugs in web access for the catalogue lists of AUSSCHREIBEN.DE.
    pub fn with_web(mut self, web: Box<dyn WebTexts>) -> Self {
        self.web = Arc::from(web);
        self
    }

    /// Web access as a shared handle (the search runs on a worker thread).
    pub fn shared_web(&self) -> Arc<dyn WebTexts> {
        self.web.clone()
    }

    /// GAEB import/export and PDF output.
    pub fn exchange(&self) -> &dyn ExchangeFiles {
        self.exchange.as_ref()
    }

    /// The same adapter as a shared handle, for work on a background thread.
    pub fn shared_exchange(&self) -> Arc<dyn ExchangeFiles> {
        self.exchange.clone()
    }

    pub fn theme(&self) -> ThemeChoice {
        self.settings.borrow().theme.unwrap_or_default()
    }

    pub fn set_theme(&self, theme: ThemeChoice) {
        self.change_settings(|settings| settings.theme = Some(theme));
    }

    pub fn detail_side(&self) -> PanelSide {
        self.settings.borrow().detail_side
    }

    pub fn set_detail_side(&self, side: PanelSide) {
        self.change_settings(|settings| settings.detail_side = side);
    }

    pub fn detail_width(&self) -> Option<f32> {
        self.settings.borrow().detail_width
    }

    pub fn set_detail_width(&self, width: f32) {
        let width = width.clamp(160.0, 5000.0);
        self.change_settings(|settings| settings.detail_width = Some(width));
    }

    /// GAEB-Export: Bilder der Langtexte mitgeben (Vorgabe an, gemerkt).
    /// Einstellung „Vorschau aller PDF-Ausschnitte ausblenden“.
    /// Title rows filled yellow (Extras ▸ Einstellungen ▸ Darstellung).
    pub fn title_rows_filled(&self) -> bool {
        self.settings.borrow().preferences.title_rows_filled
    }

    /// Windows „Eigenschaften“ and „Langtext“ in front of the main window.
    pub fn windows_in_front(&self) -> bool {
        self.settings.borrow().preferences.windows_in_front
    }

    pub fn clip_previews_hidden(&self) -> bool {
        self.settings.borrow().preferences.clip_previews_hidden
    }

    pub fn set_clip_previews_hidden(&self, hidden: bool) {
        self.change_settings(|settings| settings.preferences.clip_previews_hidden = hidden);
    }

    pub fn gaeb_images(&self) -> bool {
        self.settings.borrow().preferences.gaeb_images
    }

    pub fn set_gaeb_images(&self, on: bool) {
        self.change_settings(|settings| settings.preferences.gaeb_images = on);
    }

    /// CSV-Export: Windows-1252 statt UTF-8 mit BOM (Vorgabe aus, gemerkt).
    pub fn csv_windows_1252(&self) -> bool {
        self.settings.borrow().preferences.csv_windows_1252
    }

    pub fn set_csv_windows_1252(&self, on: bool) {
        self.change_settings(|settings| settings.preferences.csv_windows_1252 = on);
    }

    /// AUSSCHREIBEN.DE and STLB-Bau Online in the system browser instead of
    /// the own browser window (Einstellungen ▸ Neue Projekte ▸ Textquellen).
    pub fn web_pages_in_system_browser(&self) -> bool {
        self.settings
            .borrow()
            .preferences
            .web_pages_in_system_browser
    }

    /// Red/yellow marks of missing values in the LV table (default on).
    pub fn mark_gaps(&self) -> bool {
        self.settings.borrow().preferences.mark_empty_quantities
    }

    pub fn set_mark_gaps(&self, on: bool) {
        self.change_settings(|settings| settings.preferences.mark_empty_quantities = on);
    }

    pub fn column_widths(&self) -> std::collections::BTreeMap<String, f32> {
        self.settings.borrow().column_widths.clone()
    }

    pub fn set_column_widths(&self, widths: std::collections::BTreeMap<String, f32>) {
        self.change_settings(|settings| settings.column_widths = widths);
    }

    /// Whether an optional column of the LV table is shown (`key` as in the
    /// settings, e.g. "kg"); `default` when nothing is stored.
    pub fn table_column(&self, key: &str, default: bool) -> bool {
        self.settings
            .borrow()
            .table_columns
            .get(key)
            .copied()
            .unwrap_or(default)
    }

    pub fn set_table_column(&self, key: &str, on: bool) {
        self.change_settings(|settings| {
            settings.table_columns.insert(key.to_string(), on);
        });
    }

    /// Remembered print dialog options of one kind (defaults if none).
    pub fn print_settings(&self, kind: crate::PrintKind) -> crate::PrintSettings {
        let settings = self.settings.borrow();
        match settings.print.get(kind.key()) {
            Some(line) => crate::PrintSettings::from_line(kind, line),
            None => {
                let mut defaults = crate::PrintSettings::defaults_for(kind);
                if kind == crate::PrintKind::PricedLv {
                    defaults.cover = settings.preferences.cover_on_priced_lv;
                }
                defaults
            }
        }
    }

    pub fn set_print_settings(&self, kind: crate::PrintKind, settings: &crate::PrintSettings) {
        let line = settings.to_line();
        self.change_settings(|stored| {
            stored.print.insert(kind.key().to_string(), line);
        });
    }

    pub fn long_text_window(&self) -> Option<(f32, f32)> {
        self.settings.borrow().long_text_window
    }

    pub fn set_long_text_window(&self, size: (f32, f32)) {
        let size = (size.0.clamp(400.0, 8000.0), size.1.clamp(300.0, 8000.0));
        if self.long_text_window() != Some(size) {
            self.change_settings(|settings| settings.long_text_window = Some(size));
        }
    }

    /// Last size and screen position of the window „Eigenschaften“.
    pub fn position_window(&self) -> crate::WindowPlace {
        self.settings.borrow().position_window
    }

    /// Remembers size and position of the window „Eigenschaften“ (written
    /// only when they changed; an unknown position keeps the old one).
    pub fn set_position_window(&self, place: crate::WindowPlace) {
        let old = self.position_window();
        let new = crate::WindowPlace {
            size: place
                .size
                .map(|(width, height)| (width.clamp(480.0, 8000.0), height.clamp(360.0, 8000.0)))
                .or(old.size),
            at: place
                .at
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .or(old.at),
        };
        if new != old {
            self.change_settings(|settings| settings.position_window = new);
        }
    }

    /// What the user chose in Extras ▸ Einstellungen.
    pub fn personal_words(&self) -> Vec<String> {
        self.settings.borrow().personal_words.clone()
    }

    /// Adds a word to the personal spelling dictionary (kept sorted, once).
    pub fn add_personal_word(&self, word: &str) {
        let word = word.trim().to_string();
        if word.is_empty() || self.personal_words().contains(&word) {
            return;
        }
        self.change_settings(|settings| {
            settings.personal_words.push(word);
            settings.personal_words.sort();
        });
    }

    pub fn preferences(&self) -> Preferences {
        self.settings.borrow().preferences.clone()
    }

    /// Vortexte of the quick choice for Textergänzungen (Bieter, Auftraggeber)
    /// – cheap enough for every frame.
    pub fn complement_captions(&self) -> (Vec<String>, Vec<String>) {
        let settings = self.settings.borrow();
        (
            settings.preferences.bidder_captions.clone(),
            settings.preferences.owner_captions.clone(),
        )
    }

    /// Stores valid preferences; returns the problems otherwise.
    pub fn set_preferences(&self, preferences: Preferences) -> Result<(), Vec<String>> {
        let problems = preferences.problems();
        if !problems.is_empty() {
            return Err(problems);
        }
        self.change_settings(|settings| settings.preferences = preferences);
        Ok(())
    }

    /// Zoom of the interface from the settings (1.0 = 100 %).
    /// Zeilenabstand and Abstand nach Absatz of the texts (editor and print)
    /// – cheap enough for every frame.
    pub fn text_spacing(&self) -> crate::TextSpacing {
        self.settings.borrow().preferences.print_look.text
    }

    pub fn ui_zoom(&self) -> f32 {
        self.settings.borrow().preferences.ui_zoom
    }

    /// Whether positions without quantity are marked in the LV.
    pub fn mark_empty_quantities(&self) -> bool {
        self.settings.borrow().preferences.mark_empty_quantities
    }

    /// Whether the workspace shows the edit panel (Eigenschaftenfenster).
    pub fn detail_visible(&self) -> bool {
        self.settings.borrow().preferences.detail_visible
    }

    /// Shows or hides the workspace's edit panel (Eigenschaftenfenster).
    pub fn set_detail_visible(&self, visible: bool) {
        self.change_settings(|settings| settings.preferences.detail_visible = visible);
    }

    /// Forgets the dragged width of the edit panel (back to the default).
    pub fn reset_detail_width(&self) {
        self.change_settings(|settings| settings.detail_width = None);
    }

    pub fn recent(&self) -> Vec<RecentProject> {
        self.settings.borrow().recent.clone()
    }

    pub fn forget(&self, path: &Path) {
        self.change_settings(|settings| {
            settings
                .recent
                .retain(|entry| !same_file(&entry.path, path))
        });
    }

    pub fn last_folder(&self) -> Option<PathBuf> {
        self.settings.borrow().last_dir.clone()
    }

    pub fn default_folder(&self) -> Option<PathBuf> {
        self.settings.borrow().default_folder.clone()
    }

    pub fn set_default_folder(&self, folder: Option<PathBuf>) {
        self.change_settings(|settings| settings.default_folder = folder);
    }

    /// Folder offered for a new project: the default folder, else the last used one.
    pub fn suggested_folder(&self) -> Option<PathBuf> {
        self.default_folder().or_else(|| self.last_folder())
    }

    pub fn home_folder(&self) -> PathBuf {
        self.folders.home()
    }

    pub fn subfolders(&self, folder: &Path) -> Vec<PathBuf> {
        self.folders.subfolders(folder)
    }

    pub fn is_folder(&self, folder: &Path) -> bool {
        self.folders.is_folder(folder)
    }

    pub fn is_file(&self, file: &Path) -> bool {
        self.folders.is_file(file)
    }

    /// New folder `name` in `parent` (file dialog ▸ Neuer Ordner).
    pub fn create_folder(&self, parent: &Path, name: &str) -> Result<PathBuf, String> {
        self.folders.create_folder(parent, name)
    }

    /// Current time as ISO-8601 (for file names and dates).
    pub fn now_iso(&self) -> String {
        self.settings_files.now_iso()
    }

    pub fn files(&self, folder: &Path) -> Vec<PathBuf> {
        self.folders.files(folder)
    }

    pub fn read_file(&self, file: &Path) -> Result<Vec<u8>, String> {
        self.folders.read_file(file)
    }

    /// New project from a GAEB file (X81/X82/X83/D8x): reads the LV, creates the
    /// project folder like a fresh project and saves the imported content there.
    /// Returns the new entry and the importer's warnings.
    pub fn create_project_from_gaeb(
        &self,
        gaeb_file: &Path,
        folder: &Path,
        raw_name: &str,
    ) -> Result<(RecentProject, Vec<String>), String> {
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        let (project, warnings) =
            self.exchange
                .import_lv(gaeb_file, &name, &self.settings_files.now_iso())?;
        Ok((self.create_project_with(folder, &project)?, warnings))
    }

    /// Creates the project folder like a fresh project and saves `project` into it
    /// (e.g. an LV imported on a background thread).
    pub fn create_project_with(
        &self,
        folder: &Path,
        project: &Project,
    ) -> Result<RecentProject, String> {
        let created = self.projects.create_project(folder, project.name())?;
        self.projects.save_project(&created, project)?;
        self.remember(&created)
    }

    pub fn create_project(&self, folder: &Path, raw_name: &str) -> Result<RecentProject, String> {
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        let created = self.projects.create_project(folder, &name)?;
        // Vorgaben aus den Einstellungen (OZ-Maske, Schrittweite, MwSt, Aufsteller).
        if let Ok(mut project) = self.projects.load_project(&created) {
            if self.apply_new_project_defaults(&mut project) {
                self.projects.save_project(&created, &project)?;
            }
        }
        self.remember(&created)
    }

    /// Fills a fresh project with the defaults from the settings; true when
    /// anything changed.
    pub fn apply_new_project_defaults(&self, project: &mut Project) -> bool {
        let before = project.clone();
        let preferences = self.preferences();
        let fresh = self.empty_lv();
        let lv = project.lv_mut();
        if lv.rows().is_empty() {
            lv.set_oz_mask(fresh.oz_mask().clone());
            lv.set_position_step(fresh.position_step());
            lv.set_vat_percent(fresh.vat_percent());
        }
        let info = project.info_mut();
        if info.planner == proava_domain::Address::default() {
            info.planner = preferences.default_planner.clone();
        }
        *project != before
    }

    pub fn open_project(&self, path: &Path) -> Result<RecentProject, String> {
        if !path.is_file() {
            return Err(format!("Datei nicht gefunden: {}", path.display()));
        }
        self.projects.load_project(path)?;
        self.remember(path)
    }

    /// Loads a project file, also for the workspace.
    pub fn load_project(&self, path: &Path) -> Result<Project, String> {
        let project = self.projects.load_project(path)?;
        self.remember(path)?;
        Ok(project)
    }

    /// Reads a source for a takeover without touching the recent list: an
    /// proAVA project or a GAEB LV (X8x/D8x, read into a project in memory).
    pub fn read_source(&self, path: &Path) -> Result<(Project, Vec<String>), String> {
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if extension == "proava" {
            return self
                .projects
                .load_project(path)
                .map(|project| (project, Vec::new()));
        }
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "GAEB".to_string());
        let name = ProjectName::parse(&name)
            .or_else(|_| ProjectName::parse("GAEB"))
            .map_err(|error| error.to_string())?;
        // read_lv also takes a GAEB file out of a zip.
        self.exchange
            .read_lv(path, &name, &self.now_iso())
            .map(|read| (read.project, read.warnings))
    }

    /// Sources of the last takeovers, newest first (existing files only).
    pub fn transfer_sources(&self) -> Vec<PathBuf> {
        self.settings.borrow().transfer_sources.clone()
    }

    pub fn remember_transfer_source(&self, path: &Path) {
        let path = path.to_path_buf();
        self.change_settings(|settings| {
            settings
                .transfer_sources
                .retain(|other| !same_file(other, &path));
            settings.transfer_sources.insert(0, path);
            settings.transfer_sources.truncate(8);
        });
    }

    /// Folders last used in a file dialog, newest first, existing ones only.
    pub fn recent_file_folders(&self) -> Vec<PathBuf> {
        self.settings
            .borrow()
            .file_dialog_folders
            .iter()
            .filter(|folder| self.folders.is_folder(folder))
            .cloned()
            .collect()
    }

    /// Notes the folder of a confirmed file dialog (at most eight are kept).
    pub fn remember_file_folder(&self, folder: &Path) {
        let folder = folder.to_path_buf();
        if self.settings.borrow().file_dialog_folders.first() == Some(&folder) {
            return;
        }
        self.change_settings(|settings| {
            settings
                .file_dialog_folders
                .retain(|other| *other != folder);
            settings.file_dialog_folders.insert(0, folder);
            settings.file_dialog_folders.truncate(8);
        });
    }

    pub fn user_folders(&self) -> Vec<(crate::UserFolder, PathBuf)> {
        self.folders.user_folders()
    }

    pub fn drives(&self) -> Vec<PathBuf> {
        self.folders.drives()
    }

    /// Size and last change of an existing file, `None` when it is missing.
    pub fn file_details(&self, file: &Path) -> Option<crate::FileDetails> {
        self.folders.file_details(file)
    }

    /// The own position library (created on first use).
    pub fn library_file(&self) -> PathBuf {
        self.preferences()
            .library_file
            .unwrap_or_else(|| self.home_folder().join("proAVA-Bibliothek.proava"))
    }

    pub fn save_project(&self, path: &Path, project: &Project) -> Result<(), String> {
        self.projects.save_project(path, project)
    }

    /// Writes the recovery copy `.<Datei>.autosave` next to the project.
    pub fn autosave(&self, project_file: &Path, project: &Project) -> Result<PathBuf, String> {
        let file = autosave_path(project_file);
        self.projects.save_project(&file, project)?;
        Ok(file)
    }

    /// For the recovery copy on a background thread: its file and a storage
    /// handle that may be used there; `None` when the storage works on the
    /// calling thread only (then [`Launcher::autosave`]).
    #[allow(clippy::type_complexity)]
    pub fn background_autosave(
        &self,
        project_file: &Path,
    ) -> Option<(PathBuf, Arc<dyn ProjectRepository + Send + Sync>)> {
        Some((autosave_path(project_file), self.projects.shared()?))
    }

    /// Storage and GAEB reader for the LV-Suche over a folder tree on a
    /// background thread; `None` when the storage works on the calling
    /// thread only.
    #[allow(clippy::type_complexity)]
    pub fn search_handles(
        &self,
    ) -> Option<(
        Arc<dyn ProjectRepository + Send + Sync>,
        Arc<dyn ExchangeFiles>,
    )> {
        Some((self.projects.shared()?, self.exchange.clone()))
    }

    /// Removes the recovery copy (after saving or discarding on purpose).
    pub fn discard_autosave(&self, project_file: &Path) -> Result<(), String> {
        self.projects.remove_file(&autosave_path(project_file))
    }

    /// The recovery copy and its time, when it is newer than the project file.
    pub fn newer_autosave(&self, project_file: &Path) -> Option<(PathBuf, String)> {
        let file = autosave_path(project_file);
        let autosaved = self.projects.modified_iso(&file)?;
        match self.projects.modified_iso(project_file) {
            Some(saved) if saved >= autosaved => None,
            _ => Some((file, autosaved)),
        }
    }

    /// Loads a recovery copy without adding it to the recent list.
    pub fn load_autosave(&self, file: &Path) -> Result<Project, String> {
        self.projects.load_project(file)
    }

    pub fn backup(&self, project_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        self.projects.backup(project_file, target_folder)
    }

    pub fn restore(&self, backup_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        self.projects.restore(backup_file, target_folder)
    }

    /// Default content of a fresh project: one empty bill of quantities.
    pub fn empty_lv(&self) -> Lv {
        let preferences = self.preferences();
        let mask = OzMask::parse(&preferences.new_lv_mask).unwrap_or_else(|_| {
            OzMask::parse(OzMask::NEW_LV_PATTERN).expect("Vorgabe-OZ-Maske ist gültig")
        });
        let mut lv = Lv::new("Leistungsverzeichnis", mask);
        lv.set_position_step(preferences.new_lv_step);
        lv.set_vat_percent(preferences.new_lv_vat);
        lv
    }

    fn remember(&self, path: &Path) -> Result<RecentProject, String> {
        let name = path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Ungültiger Dateiname: {}", path.display()))?;

        let entry = RecentProject {
            name,
            path: path.to_path_buf(),
            opened_at: self.settings_files.now_iso(),
            kind: ProjectKind::from_path(path),
        };

        self.change_settings(|settings| {
            settings
                .recent
                .retain(|known| !same_file(&known.path, &entry.path));
            settings.recent.insert(0, entry.clone());
            settings.recent.truncate(MAX_RECENT_PROJECTS);
            settings.last_dir = projects_folder_of(&entry.path);
        });

        Ok(entry)
    }

    /// A value of the settings (for other use-case modules of this crate).
    pub(crate) fn settings_value<T>(&self, read: impl FnOnce(&Settings) -> T) -> T {
        read(&self.settings.borrow())
    }

    /// Changes and stores the settings (for other use-case modules).
    pub(crate) fn change(&self, change: impl FnOnce(&mut Settings)) {
        self.change_settings(change);
    }

    /// Every change starts from the stored settings: a second proAVA may
    /// have added recent projects meanwhile, they must not get lost.
    fn change_settings(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.borrow_mut();
        if let Some(stored) = self.settings_files.load_stored() {
            *settings = stored;
        }
        change(&mut settings);
        self.settings_files.save(&settings);
    }

    /// Takes over what a second proAVA stored (e.g. when the start screen
    /// comes back): recent projects, preferences …
    pub fn reload_settings(&self) {
        if let Some(stored) = self.settings_files.load_stored() {
            *self.settings.borrow_mut() = stored;
        }
    }

    // ----- clipboard between proAVA windows -----

    /// Header line for the clipboard (see [`crate::clipboard`]); `true` when
    /// the images had to stay behind (too large).
    pub fn clipboard_header(
        &self,
        clip: &Clip,
        mask: &OzMask,
    ) -> Result<(ClipHeader, bool), String> {
        crate::clipboard::encode(self.projects.as_ref(), clip, mask)
    }

    /// The rows or LV behind a header line found in the clipboard.
    pub fn clipboard_content(&self, header: &ClipHeader) -> Result<Clip, String> {
        crate::clipboard::decode(self.projects.as_ref(), header)
    }

    // ----- lock of an opened project -----

    /// Who else has the project open; `None` when nobody (or this window).
    pub fn foreign_lock(&self, project_file: &Path) -> Option<ProjectLock> {
        self.projects
            .read_lock(project_file)
            .filter(|lock| !lock.own)
    }

    /// Marks the project as opened here (a missing lock is no reason not to work).
    pub fn lock_project(&self, project_file: &Path) -> Result<(), String> {
        self.projects.write_lock(project_file)
    }

    /// Removes the own lock when closing (a foreign one stays).
    pub fn unlock_project(&self, project_file: &Path) {
        if self
            .projects
            .read_lock(project_file)
            .is_some_and(|lock| lock.own)
        {
            self.projects.remove_lock(project_file);
        }
    }

    /// Does a proAVA with this process id run? None: cannot tell.
    pub fn proava_running(&self, pid: u32) -> Option<bool> {
        self.projects.proava_running(pid)
    }

    /// Removes a lock whose program no longer runs.
    pub fn remove_stale_lock(&self, project_file: &Path) {
        if self
            .projects
            .read_lock(project_file)
            .is_some_and(|lock| lock.stale || lock.own)
        {
            self.projects.remove_lock(project_file);
        }
    }
}

/// Whether two paths name the same file: on Windows without regard to case
/// and to `/` or `\\` (`C:\\Projekte\\A.proava` = `c:/projekte/a.proava`).
pub fn same_file(a: &Path, b: &Path) -> bool {
    same_file_on(a, b, cfg!(windows))
}

fn same_file_on(a: &Path, b: &Path, windows: bool) -> bool {
    if !windows {
        return a == b;
    }
    let key = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
    key(a) == key(b)
}

/// `<Ordner>/Projekt.proava` → `<Ordner>/.Projekt.proava.autosave`
pub fn autosave_path(project_file: &Path) -> PathBuf {
    let name = project_file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "projekt.proava".to_string());
    project_file.with_file_name(format!(".{name}.autosave"))
}

/// Folder that holds the project: for `<Ordner>/<Name>/<Name>.proava` the
/// `<Ordner>`, so the next new project lands next to this one, not inside it.
fn projects_folder_of(project_file: &Path) -> Option<PathBuf> {
    let parent = project_file.parent()?;
    let same_name = parent.file_name().map(|name| name.to_os_string())
        == project_file.file_stem().map(|stem| stem.to_os_string());
    match (same_name, parent.parent()) {
        (true, Some(grandparent)) => Some(grandparent.to_path_buf()),
        _ => Some(parent.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn same_file_ignores_case_and_slashes_only_on_windows() {
        use super::same_file_on;
        let (a, b) = (
            Path::new("C:\\Projekte\\A.proava"),
            Path::new("c:/projekte/a.proava"),
        );
        assert!(same_file_on(a, b, true));
        assert!(!same_file_on(a, b, false));
        assert!(same_file_on(Path::new("/a/B"), Path::new("/a/B"), false));
        assert!(!same_file_on(Path::new("/a/B"), Path::new("/a/b"), false));
        assert!(same_file_on(
            Path::new("\\\\Server\\Freigabe\\LV.proava"),
            Path::new("//server/freigabe/lv.proava"),
            true
        ));
    }
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use proava_domain::{Lv, LvGroup, OzMask, Project, ProjectName};

    use super::Launcher;
    use crate::exchange::{BidImport, ExchangeFiles, GaebExport, PdfDocument};
    use crate::ports::{Folders, ProjectRepository, SettingsFiles};
    use crate::settings::Settings;

    /// Saved files (path, row count) and file times (path, ISO time).
    #[derive(Default)]
    #[allow(clippy::type_complexity)]
    struct Saved(
        Rc<RefCell<Vec<(PathBuf, usize)>>>,
        Rc<RefCell<Vec<(PathBuf, String)>>>,
    );

    impl ProjectRepository for Saved {
        fn create_project(&self, folder: &Path, name: &ProjectName) -> Result<PathBuf, String> {
            let file = folder
                .join(name.as_str())
                .join(format!("{}.proava", name.as_str()));
            self.0.borrow_mut().push((file.clone(), 0));
            Ok(file)
        }
        fn load_project(&self, _: &Path) -> Result<Project, String> {
            Err("nicht benutzt".into())
        }
        fn save_project(&self, file: &Path, project: &Project) -> Result<(), String> {
            self.0
                .borrow_mut()
                .push((file.to_path_buf(), project.lv().rows().len()));
            Ok(())
        }
        fn backup(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            Err("nicht benutzt".into())
        }
        fn restore(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            Err("nicht benutzt".into())
        }
        fn modified_iso(&self, file: &Path) -> Option<String> {
            self.1
                .borrow()
                .iter()
                .find(|(known, _)| known == file)
                .map(|(_, time)| time.clone())
        }
        fn remove_file(&self, file: &Path) -> Result<(), String> {
            self.1.borrow_mut().retain(|(known, _)| known != file);
            Ok(())
        }
    }

    struct NoSettings;

    impl SettingsFiles for NoSettings {
        fn load(&self) -> Settings {
            Settings::default()
        }
        fn save(&self, _: &Settings) {}
        fn now_iso(&self) -> String {
            "2026-11-17T18:00:00".into()
        }
    }

    struct NoFolders;

    impl Folders for NoFolders {
        fn home(&self) -> PathBuf {
            PathBuf::from("/")
        }
        fn subfolders(&self, _: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
        fn is_folder(&self, _: &Path) -> bool {
            true
        }
        fn files(&self, _: &Path) -> Vec<PathBuf> {
            Vec::new()
        }
        fn read_file(&self, _: &Path) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    struct OneGroupLv;

    impl ExchangeFiles for OneGroupLv {
        fn export_gaeb(&self, _: &Project, _: GaebExport, _: &Path) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
        fn import_bid(&self, _: &Project, _: &Path) -> Result<BidImport, String> {
            Err("nicht benutzt".into())
        }
        fn import_lv(
            &self,
            _: &Path,
            name: &ProjectName,
            created_iso: &str,
        ) -> Result<(Project, Vec<String>), String> {
            let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
            lv.push_group(LvGroup::new(0, "01", "Titel"));
            Ok((
                Project::new(name.clone(), created_iso, lv),
                vec!["Hinweis".into()],
            ))
        }
        fn export_pdf(&self, _: &Project, _: PdfDocument, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn autosave_is_offered_only_when_newer() {
        let times: Rc<RefCell<Vec<(PathBuf, String)>>> = Rc::default();
        let launcher = Launcher::new(
            Box::new(Saved(Rc::default(), times.clone())),
            Box::new(NoSettings),
            Box::new(NoFolders),
        );
        let project = Path::new("/p/Schule/Schule.proava");
        let autosave = super::autosave_path(project);
        assert_eq!(autosave, PathBuf::from("/p/Schule/.Schule.proava.autosave"));
        assert!(launcher.newer_autosave(project).is_none());
        times
            .borrow_mut()
            .push((project.to_path_buf(), "2026-11-17T10:00:00".into()));
        times
            .borrow_mut()
            .push((autosave.clone(), "2026-11-17T10:05:00".into()));
        assert_eq!(
            launcher.newer_autosave(project),
            Some((autosave.clone(), "2026-11-17T10:05:00".to_string()))
        );
        times.borrow_mut()[0].1 = "2026-11-17T10:06:00".into();
        assert!(launcher.newer_autosave(project).is_none());
        launcher.discard_autosave(project).unwrap();
        assert_eq!(times.borrow().len(), 1);
    }

    #[test]
    fn project_from_gaeb_is_created_saved_and_remembered() {
        let saved = Rc::new(RefCell::new(Vec::new()));
        let launcher = Launcher::new(
            Box::new(Saved(saved.clone(), Rc::default())),
            Box::new(NoSettings),
            Box::new(NoFolders),
        )
        .with_exchange(Box::new(OneGroupLv));
        let (entry, warnings) = launcher
            .create_project_from_gaeb(Path::new("/in/lv.x83"), Path::new("/p"), "Schule")
            .unwrap();
        assert_eq!(entry.path, PathBuf::from("/p/Schule/Schule.proava"));
        assert_eq!(warnings, vec!["Hinweis".to_string()]);
        assert_eq!(saved.borrow().last().unwrap().1, 1);
        assert_eq!(launcher.recent().len(), 1);
        assert_eq!(launcher.last_folder(), Some(PathBuf::from("/p")));
        assert!(launcher
            .create_project_from_gaeb(Path::new("/in/lv.x83"), Path::new("/p"), "")
            .is_err());
    }
}
