//! Whether the running program was updated on disk while it runs (pure
//! rules; the file system is read by the callers: `proava-ui`
//! `update_notice`, `proava-infrastructure` `program_update` for the MCP
//! server).
//!
//! An update replaces the program files (`scripts/install_linux.sh` uses
//! `install`, which removes the old file and writes a new one). A program that
//! is still running keeps the old code; proAVA and the MCP server then ask to
//! be restarted, so nobody tests the old version by mistake (two false error
//! reports on 18./19.11.2026). Not needed on Windows: there the running
//! program file is locked.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What Linux appends to the path of a running program whose file was
/// removed or replaced (`/proc/self/exe`).
#[cfg(target_os = "linux")]
const DELETED_MARK: &[u8] = b" (deleted)";

/// The path of a program file without Linux's „ (deleted)“ mark; other paths
/// stay as they are. After an update the new file at the same place counts.
pub fn without_deleted_mark(program: PathBuf) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let bytes = program.as_os_str().as_bytes();
        if let Some(path) = bytes.strip_suffix(DELETED_MARK) {
            return PathBuf::from(std::ffi::OsString::from_vec(path.to_vec()));
        }
    }
    program
}

/// Whether the program behind `exe` (path of the running program, as
/// `std::env::current_exe` reads it) was replaced since `started`:
/// - Linux marks a replaced file „ (deleted)“; with a new file at the same
///   place (`exists`) that is an update – without one the program was
///   removed, not updated (no notice);
/// - otherwise the file at the path is newer (`modified`) than the start.
pub fn replaced(
    exe: &Path,
    modified: Option<SystemTime>,
    started: SystemTime,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    let plain = without_deleted_mark(exe.to_path_buf());
    if plain.as_path() != exe {
        return exists(&plain);
    }
    modified.is_some_and(|modified| modified > started)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + seconds)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_program_replaced_by_an_update_is_recognised_by_its_mark() {
        let exe = Path::new("/home/x/.local/bin/proava (deleted)");
        let installed = |path: &Path| path == Path::new("/home/x/.local/bin/proava");
        assert!(
            replaced(exe, None, at(0), installed),
            "neue Datei am selben Ort"
        );
        assert!(
            !replaced(exe, None, at(0), |_| false),
            "deinstalliert: kein Hinweis auf ein Update"
        );
        assert_eq!(
            without_deleted_mark(PathBuf::from("/opt/a (deleted)/proava-mcp (deleted)")),
            PathBuf::from("/opt/a (deleted)/proava-mcp"),
            "nur das Ende"
        );
    }

    #[test]
    fn a_program_file_newer_than_the_start_is_an_update() {
        let exe = Path::new("/usr/bin/proava");
        let exists = |_: &Path| true;
        assert!(
            replaced(exe, Some(at(61)), at(60), exists),
            "nach dem Start ersetzt"
        );
        assert!(!replaced(exe, Some(at(60)), at(60), exists), "gleich alt");
        assert!(
            !replaced(exe, Some(at(10)), at(60), exists),
            "vor dem Start installiert"
        );
        assert!(!replaced(exe, None, at(60), exists), "Zeit unbekannt");
        assert_eq!(
            without_deleted_mark(PathBuf::from("C:/Programme/proAVA/proava.exe")),
            PathBuf::from("C:/Programme/proAVA/proava.exe")
        );
    }
}
