//! Temporary copies of filed letters for the system's PDF viewer
//! („Öffnen“ in Schriftverkehr im Projekt): one folder per proAVA process,
//! `<temp>/proava-schriftverkehr-<pid>`. The own folder goes at the end of
//! the program; at the start, folders of proAVA processes that no longer
//! run are removed – only folders of exactly this pattern, nothing else.
//!
//! Tests give every test its own folder ([`use_folder_on_this_thread`]):
//! tests of one test program run side by side in one process and would
//! otherwise share – and tidy away – the same folder (flaky test 18.11.).

use std::cell::RefCell;
use std::path::{Path, PathBuf};

const PREFIX: &str = "proava-schriftverkehr-";

thread_local! {
    /// Folder of the copies for this thread instead of the one of the process (tests).
    static OWN_FOLDER: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The folder of this process in `temp`.
pub fn folder_in(temp: &Path, pid: u32) -> PathBuf {
    temp.join(format!("{PREFIX}{pid}"))
}

/// The folder of the copies: the one of this process in the system's temp
/// folder, or the one a test chose for its thread.
pub fn folder() -> PathBuf {
    OWN_FOLDER
        .with(|own| own.borrow().clone())
        .unwrap_or_else(|| folder_in(&std::env::temp_dir(), std::process::id()))
}

/// Tests: copies made on this thread go to `folder` (None = the folder of
/// the process again). The UI of a test harness runs on the test's thread,
/// so each test gets its own folder, e.g. inside its temporary folder.
pub fn use_folder_on_this_thread(folder: Option<PathBuf>) {
    OWN_FOLDER.with(|own| *own.borrow_mut() = folder);
}

/// PID of a folder name of the pattern, else None („proava-schriftverkehr-123“ → 123).
fn pid_of(name: &str) -> Option<u32> {
    let digits = name.strip_prefix(PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Name of the copy for the viewer. A project file from elsewhere could
/// carry any name (`..\..\x`, `C:\…`, `x.bat`): only the last part counts,
/// letters, digits, space and `-_.()`, no leading or trailing dot, no
/// Windows device name (`CON`, `NUL` …), at most 100 characters, and always
/// `.pdf` – so the system hands it to the PDF viewer and never runs it
/// (Windows `ShellExecute` „open“ would start a `.bat` or `.exe`).
pub fn copy_name(stored: &str) -> String {
    let last = stored.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut stem = cleaned.trim_matches(['.', ' ']).to_string();
    if stem.to_ascii_lowercase().ends_with(".pdf") {
        stem.truncate(stem.len() - ".pdf".len());
    }
    let mut stem: String = stem.trim_matches(['.', ' ']).chars().take(100).collect();
    if stem.is_empty() {
        stem = "Schreiben".to_string();
    }
    // Windows reserves these names, also with an extension („NUL.pdf“).
    let device = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    let numbered = |prefix: &str| {
        device
            .strip_prefix(prefix)
            .is_some_and(|digit| digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit())
    };
    if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || numbered("COM")
        || numbered("LPT")
    {
        stem.insert(0, '_');
    }
    format!("{stem}.pdf")
}

/// Puts the PDF `bytes` of a filed letter (`stored` name) into `folder` for
/// the viewer and returns the file. A copy with the same content is used as
/// it is: on Windows the PDF viewer keeps an opened file locked, so opening
/// the same letter a second time must not write it again. Another letter of
/// the same name that cannot be written (held by the viewer) goes to
/// `…_2.pdf` and so on.
pub fn place(
    folder: &Path,
    stored: &str,
    bytes: &[u8],
    write: impl Fn(&Path, &[u8]) -> Result<(), String>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let name = copy_name(stored);
    let stem = name.strip_suffix(".pdf").unwrap_or(&name);
    let mut last_error = String::new();
    for number in 1..=9 {
        let file = if number == 1 {
            folder.join(&name)
        } else {
            folder.join(format!("{stem}_{number}.pdf"))
        };
        if std::fs::read(&file).is_ok_and(|known| known == bytes) {
            return Ok(file);
        }
        match write(&file, bytes) {
            Ok(()) => return Ok(file),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// End of the program: the own copies go (a viewer that still holds one on
/// Windows keeps it; the next start removes it).
pub fn remove_own() {
    let _ = std::fs::remove_dir_all(folder());
}

/// Removes the folders in `temp` of proAVA processes that no longer run
/// (`running(pid)` = Some(false)); the own PID, running processes and
/// unknown states (None) stay. Only real folders (no links) whose name is
/// exactly the pattern. Returns what was removed.
pub fn remove_stale(
    temp: &Path,
    own_pid: u32,
    running: impl Fn(u32) -> Option<bool>,
) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(temp) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(pid_of) else {
            continue;
        };
        // file_type does not follow links.
        if pid == own_pid || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        if running(pid) == Some(false) && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed.push(entry.path());
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wackelnder Test 18.11.: die Tests eines Testprogramms laufen in einem
    /// Prozess nebeneinander; mit einem Ordner je PID räumte das Ende einer
    /// Affentest-Sitzung die Kopie eines anderen Tests weg. Ein Thread kann
    /// seinen eigenen Ordner haben, die übrigen behalten den des Prozesses.
    #[test]
    fn every_thread_can_have_its_own_folder() {
        let process = folder_in(&std::env::temp_dir(), std::process::id());
        assert_eq!(folder(), process);
        let own = std::env::temp_dir().join("proava-test-eigener-ordner");
        use_folder_on_this_thread(Some(own.clone()));
        assert_eq!(folder(), own);
        let other = std::thread::spawn(folder).join().unwrap();
        assert_eq!(
            other, process,
            "andere Threads behalten den Ordner des Prozesses"
        );
        use_folder_on_this_thread(None);
        assert_eq!(folder(), process);
    }

    #[test]
    fn folder_names() {
        assert_eq!(pid_of("proava-schriftverkehr-4711"), Some(4711));
        assert_eq!(pid_of("proava-schriftverkehr-"), None);
        assert_eq!(pid_of("proava-schriftverkehr-12a"), None);
        assert_eq!(pid_of("proava-schriftverkehr-+12"), None);
        assert_eq!(
            pid_of("proava-schriftverkehr-99999999999"),
            None,
            "keine gültige PID"
        );
        assert_eq!(pid_of("xproava-schriftverkehr-1"), None);
        assert_eq!(
            folder_in(Path::new("/tmp"), 7),
            Path::new("/tmp/proava-schriftverkehr-7")
        );
    }

    /// Names from a project file of unknown origin stay in the folder and
    /// stay PDFs (the system must never run them).
    #[test]
    fn copy_names_stay_pdfs_in_the_folder() {
        assert_eq!(
            copy_name("0002_Absage_Schmidt_KG.pdf"),
            "0002_Absage_Schmidt_KG.pdf"
        );
        assert_eq!(
            copy_name("0001_Absage_Müller_Söhne_GmbH.pdf"),
            "0001_Absage_Müller_Söhne_GmbH.pdf"
        );
        assert_eq!(
            copy_name("Protokoll Bieter (2).PDF"),
            "Protokoll Bieter (2).pdf"
        );
        assert_eq!(copy_name("..\\..\\Autostart\\start.bat"), "start.bat.pdf");
        assert_eq!(copy_name("../../.bashrc"), "bashrc.pdf");
        assert_eq!(copy_name("C:\\Windows\\x.exe"), "x.exe.pdf");
        assert_eq!(copy_name("a:b*c?.pdf"), "a_b_c_.pdf");
        assert_eq!(copy_name("NUL.pdf"), "_NUL.pdf");
        assert_eq!(copy_name("com1.txt"), "_com1.txt.pdf");
        assert_eq!(copy_name("COM10.pdf"), "COM10.pdf");
        assert_eq!(copy_name(""), "Schreiben.pdf");
        assert_eq!(copy_name(".."), "Schreiben.pdf");
        assert_eq!(copy_name(" . . "), "Schreiben.pdf");
        assert_eq!(copy_name("x.pdf ."), "x.pdf");
        assert_eq!(copy_name(&"ä".repeat(300)).chars().count(), 104);
    }

    /// The second „Öffnen“ of the same letter must not write again (Windows:
    /// the viewer holds the first copy); a held file of another letter with
    /// the same name gives way to `…_2.pdf`.
    #[test]
    fn same_copy_is_used_again_and_a_held_one_gives_way() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-test-viewer-place-")
            .tempdir()
            .unwrap();
        let temp = scratch.path().to_path_buf();
        let writes = std::cell::Cell::new(0);
        let write = |file: &Path, bytes: &[u8]| {
            writes.set(writes.get() + 1);
            std::fs::write(file, bytes).map_err(|error| error.to_string())
        };
        let first = place(&temp, "0001_Absage.pdf", b"%PDF A", write).unwrap();
        assert_eq!(first, temp.join("0001_Absage.pdf"));
        assert_eq!(
            place(&temp, "0001_Absage.pdf", b"%PDF A", write).unwrap(),
            first
        );
        assert_eq!(
            writes.get(),
            1,
            "gleicher Inhalt wird nicht noch einmal geschrieben"
        );
        // Held by the viewer (like Windows error 32): another letter of that name.
        let held = |file: &Path, bytes: &[u8]| {
            if file == first {
                Err("in einem anderen Programm geöffnet".to_string())
            } else {
                std::fs::write(file, bytes).map_err(|error| error.to_string())
            }
        };
        let second = place(&temp, "0001_Absage.pdf", b"%PDF B", held).unwrap();
        assert_eq!(second, temp.join("0001_Absage_2.pdf"));
        assert_eq!(std::fs::read(&second).unwrap(), b"%PDF B");
        assert_eq!(std::fs::read(&first).unwrap(), b"%PDF A");
        let never = |_: &Path, _: &[u8]| Err("gesperrt".to_string());
        assert_eq!(
            place(&temp, "0003_Zuschlag.pdf", b"%PDF C", never),
            Err("gesperrt".to_string())
        );
        std::fs::remove_dir_all(&temp).unwrap();
    }

    /// Only folders of the pattern whose proAVA is gone; never anything else.
    #[test]
    fn only_folders_of_ended_processes_go() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-test-viewer-copies-")
            .tempdir()
            .unwrap();
        let temp = scratch.path().to_path_buf();
        let make = |name: &str| {
            let folder = temp.join(name);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("0001_Absage.pdf"), b"%PDF").unwrap();
            folder
        };
        let ended = make("proava-schriftverkehr-111");
        let running = make("proava-schriftverkehr-222");
        let unknown = make("proava-schriftverkehr-333");
        let own = make("proava-schriftverkehr-444");
        let not_digits = make("proava-schriftverkehr-abc");
        let foreign = make("fremd-111");
        std::fs::write(
            temp.join("proava-schriftverkehr-555"),
            b"Datei, kein Ordner",
        )
        .unwrap();
        // A link of the pattern to a foreign folder: neither the link target nor its content goes.
        let target = make("ziel");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, temp.join("proava-schriftverkehr-666")).unwrap();

        let removed = remove_stale(&temp, 444, |pid| match pid {
            222 => Some(true),
            333 => None,
            _ => Some(false),
        });
        assert_eq!(removed, vec![ended.clone()]);
        assert!(!ended.exists());
        for kept in [&running, &unknown, &own, &not_digits, &foreign, &target] {
            assert!(
                kept.join("0001_Absage.pdf").exists(),
                "{} bleibt",
                kept.display()
            );
        }
        assert!(temp.join("proava-schriftverkehr-555").is_file());
        #[cfg(unix)]
        assert!(
            temp.join("proava-schriftverkehr-666")
                .symlink_metadata()
                .is_ok(),
            "Verknüpfung bleibt"
        );
        assert!(remove_stale(&temp.join("gibt-es-nicht"), 1, |_| Some(false)).is_empty());
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
