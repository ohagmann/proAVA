use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use proava_application::{ProjectLock, ProjectRepository};
use proava_domain::{Lv, OzMask, Project, ProjectName};

use crate::project_xml::{
    read_project_entries, read_project_xml, write_project_xml, CORRESPONDENCE_FOLDER, IMAGE_FOLDER,
};

const PROJECT_EXTENSION: &str = "proava";
const PROJECT_ENTRY: &str = "projekt.xml";
const BACKUP_MARKER: &str = "_Sicherung_";
const RESTORED_MARKER: &str = "_wiederhergestellt_";

/// proAVA projects as `<name>.proava`: a zip container with `projekt.xml`
/// (schema 2), `bilder/<file>` for the images and `schriftverkehr/<file>` for
/// the filed letters (PDF). Plain XML files of schema 1
/// are still read. Backups are zips next to the project.
pub struct XmlProjectFile;

impl Default for XmlProjectFile {
    fn default() -> Self {
        Self::new()
    }
}

impl XmlProjectFile {
    pub fn new() -> Self {
        Self
    }
}

/// The complete project file content (zip container, schema 2).
pub fn project_to_bytes(project: &Project) -> Result<Vec<u8>, String> {
    let xml = write_project_xml(project)?;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(PROJECT_ENTRY, deflated).map_err(zip_error)?;
    zip.write_all(xml.as_bytes()).map_err(zip_error)?;
    let shared = crate::project_xml::shared_pdfs(project);
    for image in project.images() {
        if shared.contains_key(&image.file) {
            // Same PDF as an earlier clip: stored once (see `<pdf-clip pdf>`).
            continue;
        }
        zip.start_file(format!("{IMAGE_FOLDER}{}", image.file), stored)
            .map_err(zip_error)?;
        zip.write_all(&image.bytes).map_err(zip_error)?;
    }
    // Frozen previews of PDF clips (pictures the editor shows without
    // rendering the PDF again).
    for image in project.images() {
        if let Some(png) = image
            .clip
            .as_ref()
            .filter(|clip| clip.preview_frozen)
            .and_then(|clip| clip.frozen_png.as_ref())
        {
            let name = crate::project_xml::frozen_preview_name(&image.file);
            zip.start_file(format!("{IMAGE_FOLDER}{name}"), stored)
                .map_err(zip_error)?;
            zip.write_all(png).map_err(zip_error)?;
        }
    }
    // PDFs are compressed already: stored, like the images.
    for entry in project
        .correspondence()
        .iter()
        .filter(|entry| entry.has_pdf())
    {
        zip.start_file(format!("{CORRESPONDENCE_FOLDER}{}", entry.file), stored)
            .map_err(zip_error)?;
        zip.write_all(&entry.pdf).map_err(zip_error)?;
    }
    Ok(zip.finish().map_err(zip_error)?.into_inner())
}

/// Reads a project file: zip container (schema 2) or plain XML (schema 1).
pub fn project_from_bytes(content: &[u8]) -> Result<Project, String> {
    if content.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(content))
            .map_err(|error| format!("Die Projektdatei ist beschädigt (Zip-Container): {error}"))?;
        let xml = read_entry(&mut archive, PROJECT_ENTRY)?
            .ok_or("In der Projektdatei fehlt projekt.xml.")?;
        let xml =
            String::from_utf8(xml).map_err(|_| "projekt.xml ist kein UTF-8-Text.".to_string())?;
        let mut error: Option<String> = None;
        let project = read_project_entries(&xml, |name| match read_entry(&mut archive, name) {
            Ok(bytes) => bytes,
            Err(message) => {
                error.get_or_insert(message);
                None
            }
        })?;
        return match error {
            Some(message) => Err(message),
            None => Ok(project),
        };
    }

    let text = std::str::from_utf8(content).map_err(|_| {
        "Die Projektdatei ist weder ein proAVA-Container noch UTF-8-XML.".to_string()
    })?;
    let text = text.trim_start_matches('\u{feff}');
    if !text.trim_start().starts_with('<') {
        return Err("Das ist keine proAVA-Projektdatei.".to_string());
    }
    read_project_xml(text, |_| None)
}

/// Only `projekt.xml` of a project file (no images, no filed letters): the
/// zip is read through its directory, the other entries are never touched.
fn project_texts_from_file(file: &Path) -> Result<Project, String> {
    let mut handle = File::open(file).map_err(|error| error.to_string())?;
    let mut head = [0u8; 2];
    let is_zip = handle.read_exact(&mut head).is_ok() && &head == b"PK";
    if !is_zip {
        // Schema 1: plain XML.
        return project_from_bytes(&fs::read(file).map_err(|error| error.to_string())?);
    }
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(
        File::open(file).map_err(|error| error.to_string())?,
    ))
    .map_err(|error| format!("Die Projektdatei ist beschädigt (Zip-Container): {error}"))?;
    let mut entry = archive
        .by_name(PROJECT_ENTRY)
        .map_err(|_| "In der Projektdatei fehlt projekt.xml.".to_string())?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|_| "projekt.xml ist kein lesbarer UTF-8-Text.".to_string())?;
    read_project_entries(&xml, |_| None)
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("Die Projektdatei ist beschädigt ({name}): {error}")),
    };
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Die Projektdatei ist beschädigt ({name}): {error}"))?;
    Ok(Some(bytes))
}

/// Writes `<file>.tmp` first and renames it, so a crash never leaves half a file.
fn write_atomically(file: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Ordner {}: {error}", parent.display()))?;
    }
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);

    let result = (|| {
        let mut handle = File::create(&temporary)?;
        handle.write_all(content)?;
        handle.sync_all()?;
        drop(handle);
        replace_file(&temporary, file)
    })();
    result.map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("{}: {error}", file.display())
    })
}

/// Puts `temporary` in place of `file` (an existing file is replaced). On
/// Windows a virus scanner, the search indexer or a sync client may hold the
/// file for a moment and the system refuses the rename (errors 5, 32, 33):
/// then it is tried again for up to a second.
pub(crate) fn replace_file(temporary: &Path, file: &Path) -> std::io::Result<()> {
    let mut attempts = 0;
    loop {
        match fs::rename(temporary, file) {
            Err(error) if attempts < 10 && is_held_by_another_program(&error, cfg!(windows)) => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            result => return result,
        }
    }
}

/// Windows: access denied (5), sharing violation (32), lock violation (33).
fn is_held_by_another_program(error: &std::io::Error, windows: bool) -> bool {
    windows && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

impl ProjectRepository for XmlProjectFile {
    fn create_project(&self, folder: &Path, name: &ProjectName) -> Result<PathBuf, String> {
        let project_folder = folder.join(name.as_str());
        fs::create_dir_all(&project_folder)
            .map_err(|error| format!("Ordner {}: {error}", project_folder.display()))?;

        let project_file = project_folder.join(format!("{}.{PROJECT_EXTENSION}", name.as_str()));
        if project_file.exists() {
            return Err(format!(
                "Dort gibt es schon ein Projekt: {}",
                project_file.display()
            ));
        }

        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN)?;
        let project = Project::new(
            name.clone(),
            now_iso(),
            Lv::new("Leistungsverzeichnis", mask),
        );
        self.save_project(&project_file, &project)?;
        Ok(project_file)
    }

    fn load_project(&self, file: &Path) -> Result<Project, String> {
        let content = fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
        project_from_bytes(&content).map_err(|error| format!("{}: {error}", file.display()))
    }

    fn load_project_texts(&self, file: &Path) -> Result<Project, String> {
        project_texts_from_file(file).map_err(|error| format!("{}: {error}", file.display()))
    }

    fn folder_entries(&self, folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(folder) else {
            return (Vec::new(), Vec::new());
        };
        let (mut folders, mut files) = (Vec::new(), Vec::new());
        for entry in entries.flatten() {
            let path = entry.path();
            // Links are followed (is_dir/is_file); the depth limit of the
            // search stops loops.
            if path.is_dir() {
                folders.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
        let by_name = |path: &PathBuf| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        };
        folders.sort_by_key(by_name);
        files.sort_by_key(by_name);
        (folders, files)
    }

    fn save_project(&self, file: &Path, project: &Project) -> Result<(), String> {
        write_atomically(file, &project_to_bytes(project)?)
    }

    fn modified_iso(&self, file: &Path) -> Option<String> {
        let modified = fs::metadata(file).ok()?.modified().ok()?;
        Some(
            chrono::DateTime::<chrono::Local>::from(modified)
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string(),
        )
    }

    fn shared(&self) -> Option<Arc<dyn ProjectRepository + Send + Sync>> {
        Some(Arc::new(XmlProjectFile))
    }

    fn remove_file(&self, file: &Path) -> Result<(), String> {
        match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("{}: {error}", file.display())),
        }
    }

    fn project_bytes(&self, project: &Project) -> Result<Vec<u8>, String> {
        project_to_bytes(project)
    }

    fn read_project_bytes(&self, content: &[u8]) -> Result<Project, String> {
        project_from_bytes(content)
    }

    fn read_lock(&self, file: &Path) -> Option<ProjectLock> {
        crate::project_lock::read(file)
    }

    fn write_lock(&self, file: &Path) -> Result<(), String> {
        crate::project_lock::write(file)
    }

    fn remove_lock(&self, file: &Path) {
        crate::project_lock::remove(file);
    }

    fn proava_running(&self, pid: u32) -> Option<bool> {
        crate::project_lock::process_alive(pid)
    }

    fn backup(&self, project_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        if !project_file.is_file() {
            return Err(format!(
                "Projekt nicht gefunden: {}",
                project_file.display()
            ));
        }

        fs::create_dir_all(target_folder)
            .map_err(|error| format!("Ordner {}: {error}", target_folder.display()))?;

        let content = fs::read(project_file).map_err(|error| format!("Projekt lesen: {error}"))?;
        let backup_file =
            match free_backup_file(target_folder, &file_stem(project_file), &stamp(), &content) {
                BackupSlot::Existing(file) => return Ok(file),
                BackupSlot::Free(file) => file,
            };

        let entry_name = format!("{}.{PROJECT_EXTENSION}", file_stem(project_file));

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(entry_name, options)
            .map_err(|error| format!("Sicherung schreiben: {error}"))?;
        zip.write_all(&content)
            .map_err(|error| format!("Sicherung schreiben: {error}"))?;
        let bytes = zip
            .finish()
            .map_err(|error| format!("Sicherung abschließen: {error}"))?
            .into_inner();
        write_atomically(&backup_file, &bytes)?;

        Ok(backup_file)
    }

    fn restore(&self, backup_file: &Path, target_folder: &Path) -> Result<PathBuf, String> {
        if !backup_file.is_file() {
            return Err(format!(
                "Sicherung nicht gefunden: {}",
                backup_file.display()
            ));
        }

        fs::create_dir_all(target_folder)
            .map_err(|error| format!("Ordner {}: {error}", target_folder.display()))?;

        let base = base_name(backup_file);
        let is_zip = backup_file
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase() == "zip")
            .unwrap_or(false);

        let content = if is_zip {
            project_entry_of_backup(backup_file)?
        } else {
            fs::read(backup_file).map_err(|error| format!("Sicherung lesen: {error}"))?
        };
        project_from_bytes(&content)
            .map_err(|error| format!("Die Sicherung enthält kein lesbares Projekt: {error}"))?;

        let stamp = stamp();
        let restored = numbered_free(|number| {
            target_folder.join(format!(
                "{base}{RESTORED_MARKER}{stamp}{}.{PROJECT_EXTENSION}",
                counter_suffix(number)
            ))
        });
        write_atomically(&restored, &content)
            .map_err(|error| format!("Wiederherstellen: {error}"))?;
        Ok(restored)
    }
}

fn project_entry_of_backup(backup_file: &Path) -> Result<Vec<u8>, String> {
    let handle = File::open(backup_file).map_err(|error| format!("Sicherung öffnen: {error}"))?;
    let mut archive =
        zip::ZipArchive::new(handle).map_err(|error| format!("Sicherung lesen: {error}"))?;

    let mut entry_name: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("Sicherung lesen: {error}"))?;
        let name = entry
            .name()
            .map_err(|error| format!("Sicherung lesen: {error}"))?;
        if name
            .to_lowercase()
            .ends_with(&format!(".{PROJECT_EXTENSION}"))
        {
            entry_name = Some(name.to_string());
            break;
        }
    }
    let Some(entry_name) = entry_name else {
        return Err("In der Sicherung liegt kein Projekt.".to_string());
    };

    let mut entry = archive
        .by_name(&entry_name)
        .map_err(|error| format!("Sicherung lesen: {error}"))?;
    let mut content = Vec::new();
    entry
        .read_to_end(&mut content)
        .map_err(|error| format!("Sicherung lesen: {error}"))?;
    Ok(content)
}

fn zip_error(error: impl std::fmt::Display) -> String {
    format!("Projektdatei schreiben: {error}")
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Where a backup of `content` goes.
#[derive(Debug, PartialEq)]
enum BackupSlot {
    /// A backup of exactly this state with this time stamp is there already
    /// (two saves in the same second, e.g. two MCP calls): reuse it.
    Existing(PathBuf),
    /// Free name: `<stem>_Sicherung_<stamp>.zip`, else `…_2.zip`, `…_3.zip`.
    Free(PathBuf),
}

fn free_backup_file(target_folder: &Path, stem: &str, stamp: &str, content: &[u8]) -> BackupSlot {
    let name = |number: usize| {
        target_folder.join(format!(
            "{stem}{BACKUP_MARKER}{stamp}{}.zip",
            counter_suffix(number)
        ))
    };
    let mut number = 1;
    loop {
        let file = name(number);
        if !file.exists() {
            return BackupSlot::Free(file);
        }
        if project_entry_of_backup(&file).is_ok_and(|saved| saved == content) {
            return BackupSlot::Existing(file);
        }
        number += 1;
    }
}

/// „“ for the first file of a second, „_2“, „_3“ … for further ones.
fn counter_suffix(number: usize) -> String {
    if number <= 1 {
        String::new()
    } else {
        format!("_{number}")
    }
}

/// The first name `name(1)`, `name(2)` … that is not taken.
fn numbered_free(name: impl Fn(usize) -> PathBuf) -> PathBuf {
    (1..)
        .map(&name)
        .find(|file| !file.exists())
        .expect("a free name")
}

fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "Projekt".to_string())
}

fn base_name(path: &Path) -> String {
    let stem = file_stem(path);
    match stem.find(BACKUP_MARKER) {
        Some(index) => stem[..index].to_string(),
        None => stem,
    }
}

#[cfg(test)]
mod backup_name_tests {
    use super::*;

    fn zip_with(file: &Path, entry: &str, content: &[u8]) {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(entry, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content).unwrap();
        fs::write(file, zip.finish().unwrap().into_inner()).unwrap();
    }

    #[test]
    fn backups_in_the_same_second_get_their_own_name_or_reuse_the_same_state() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-backup-name-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        fs::create_dir_all(&folder).unwrap();
        let stamp = "2026-11-18_103000";
        let first = folder.join("P_Sicherung_2026-11-18_103000.zip");
        assert_eq!(
            free_backup_file(&folder, "P", stamp, b"A"),
            BackupSlot::Free(first.clone())
        );
        zip_with(&first, "P.proava", b"A");
        // Same state again in the same second: the existing backup is it.
        assert_eq!(
            free_backup_file(&folder, "P", stamp, b"A"),
            BackupSlot::Existing(first)
        );
        // Another state: next free number.
        let second = folder.join("P_Sicherung_2026-11-18_103000_2.zip");
        assert_eq!(
            free_backup_file(&folder, "P", stamp, b"B"),
            BackupSlot::Free(second.clone())
        );
        zip_with(&second, "P.proava", b"B");
        assert_eq!(
            free_backup_file(&folder, "P", stamp, b"C"),
            BackupSlot::Free(folder.join("P_Sicherung_2026-11-18_103000_3.zip"))
        );
        assert_eq!(
            base_name(&second),
            "P",
            "Wiederherstellen kennt den Projektnamen"
        );
        let _ = fs::remove_dir_all(&folder);
    }
}

#[cfg(test)]
mod replace_tests {
    use super::{is_held_by_another_program, replace_file};

    #[test]
    fn only_windows_lock_errors_are_tried_again() {
        let error = |code| std::io::Error::from_raw_os_error(code);
        for code in [5, 32, 33] {
            assert!(is_held_by_another_program(&error(code), true));
            assert!(!is_held_by_another_program(&error(code), false));
        }
        assert!(!is_held_by_another_program(&error(2), true));
    }

    #[test]
    fn replaces_an_existing_file() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-replace-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        std::fs::create_dir_all(&folder).unwrap();
        let (temporary, file) = (folder.join("a.tmp"), folder.join("a.proava"));
        std::fs::write(&file, "alt").unwrap();
        std::fs::write(&temporary, "neu").unwrap();
        replace_file(&temporary, &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "neu");
        assert!(!temporary.exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
