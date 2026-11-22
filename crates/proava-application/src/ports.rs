use std::path::{Path, PathBuf};
use std::sync::Arc;

use proava_domain::{Project, ProjectName};

use crate::settings::Settings;

/// Storage of project files: create, load, save, back up, restore.
pub trait ProjectRepository {
    fn create_project(&self, folder: &Path, name: &ProjectName) -> Result<PathBuf, String>;
    fn load_project(&self, file: &Path) -> Result<Project, String>;
    /// The project for a text search: the outline and texts only – without
    /// images and filed letters, so many files are read quickly (LV-Suche).
    /// Never save or take rows from it. Default: the whole project.
    fn load_project_texts(&self, file: &Path) -> Result<Project, String> {
        self.load_project(file)
    }
    /// Subfolders and files directly in `folder`, each sorted by name (for
    /// searching a folder tree). Default: nothing.
    fn folder_entries(&self, folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let _ = folder;
        (Vec::new(), Vec::new())
    }
    fn save_project(&self, file: &Path, project: &Project) -> Result<(), String>;
    fn backup(&self, project_file: &Path, target_folder: &Path) -> Result<PathBuf, String>;
    fn restore(&self, backup_file: &Path, target_folder: &Path) -> Result<PathBuf, String>;
    /// Last change of a file as local ISO time (`2026-11-17T18:30:00`), `None` when missing.
    fn modified_iso(&self, file: &Path) -> Option<String>;
    /// Deletes a file; a missing file is fine.
    fn remove_file(&self, file: &Path) -> Result<(), String>;
    /// The same storage as a handle for a background thread (automatic
    /// recovery copy); `None` when it works on the calling thread only.
    fn shared(&self) -> Option<Arc<dyn ProjectRepository + Send + Sync>> {
        None
    }
    /// The project as file content (for the clipboard between instances).
    fn project_bytes(&self, _project: &Project) -> Result<Vec<u8>, String> {
        Err("Dieses Speicherformat kennt keine Zwischenablage.".to_string())
    }
    /// A project from file content (see [`ProjectRepository::project_bytes`]).
    fn read_project_bytes(&self, _content: &[u8]) -> Result<Project, String> {
        Err("Dieses Speicherformat kennt keine Zwischenablage.".to_string())
    }
    /// Who has the project open (lock file next to it), `None` without lock.
    fn read_lock(&self, _file: &Path) -> Option<ProjectLock> {
        None
    }
    /// Marks the project as opened by this program instance.
    fn write_lock(&self, _file: &Path) -> Result<(), String> {
        Ok(())
    }
    /// Removes the lock file (closing, stale lock); a missing one is fine.
    fn remove_lock(&self, _file: &Path) {}
    /// Does a proAVA with this process id run? None: cannot tell
    /// (e.g. no process list on this system).
    fn proava_running(&self, _pid: u32) -> Option<bool> {
        None
    }
}

/// Lock of an opened project (`.~lock.<Datei>#` next to it, like LibreOffice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLock {
    pub user: String,
    pub host: String,
    pub pid: u32,
    /// Local time the project was opened, TT.MM.JJJJ HH:MM.
    pub since: String,
    /// The lock belongs to this program instance.
    pub own: bool,
    /// The program that set it no longer runs (same computer).
    pub stale: bool,
}

impl ProjectLock {
    /// „anwender auf rechner1 seit 17.11.2026 22:30“.
    pub fn who(&self) -> String {
        let user = if self.user.is_empty() {
            "unbekannt"
        } else {
            &self.user
        };
        format!("{user} auf {} seit {}", self.host, self.since)
    }
}

/// Storage of the program's own settings.
pub trait SettingsFiles {
    fn load(&self) -> Settings;
    /// The settings as they are stored right now – `None` when there is no
    /// readable file. A second proAVA may have changed them meanwhile.
    fn load_stored(&self) -> Option<Settings> {
        None
    }
    fn save(&self, settings: &Settings);
    /// Current time as ISO-8601, provided by the outer layer (it owns the clock).
    fn now_iso(&self) -> String;
}

/// Access to the folders and files of this machine, for the file dialogs:
/// reading, and making a new folder („Neuer Ordner“).
pub trait Folders {
    fn home(&self) -> PathBuf;
    fn subfolders(&self, folder: &Path) -> Vec<PathBuf>;
    fn is_folder(&self, folder: &Path) -> bool;
    /// Whether a file exists (the library before it is created).
    fn is_file(&self, file: &Path) -> bool {
        file.is_file()
    }
    /// Files (not folders) directly inside `folder`, sorted by name.
    fn files(&self, folder: &Path) -> Vec<PathBuf>;
    /// Reads a whole file, e.g. an image for a long text.
    fn read_file(&self, file: &Path) -> Result<Vec<u8>, String>;
    /// The usual folders of the user that exist (Schreibtisch, Dokumente …),
    /// for the places column of the file dialog.
    fn user_folders(&self) -> Vec<(UserFolder, PathBuf)> {
        Vec::new()
    }
    /// Size and last change of an existing file (for „Datei ersetzen?“).
    fn file_details(&self, _file: &Path) -> Option<FileDetails> {
        None
    }
    /// Roots of the drives and mounted disks: `C:\`, `D:\` … on Windows,
    /// `/` and the mounted data disks on Linux.
    fn drives(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Makes the folder `name` in `parent` (name already checked); the error
    /// is a sentence for the dialog („gibt es schon“, no right to write …).
    fn create_folder(&self, parent: &Path, name: &str) -> Result<PathBuf, String> {
        let _ = (parent, name);
        Err("Hier lassen sich keine Ordner anlegen.".to_string())
    }
}

/// What the replace question shows about an existing file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDetails {
    pub size: u64,
    /// Local time `2026-11-17T22:05:00`, when known.
    pub modified_iso: Option<String>,
}

/// A well-known folder of the user, named by the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFolder {
    Desktop,
    Documents,
    Pictures,
    Downloads,
}

/// Text files from the web – only the published catalogue lists of a text
/// source (see `tender_index`), with a local cache. Read only, no login.
pub trait WebTexts: Send + Sync {
    /// The text at `url`; a cached copy younger than `max_age_days` is taken
    /// without asking the server.
    fn get_text(&self, url: &str, max_age_days: u32) -> Result<String, String>;
}

/// No web access (tests, or when none is set up).
pub struct NoWeb;

impl WebTexts for NoWeb {
    fn get_text(&self, _url: &str, _max_age_days: u32) -> Result<String, String> {
        Err("Kein Internetzugang eingerichtet.".to_string())
    }
}
