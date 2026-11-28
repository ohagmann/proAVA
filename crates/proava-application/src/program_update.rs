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

/// The AppImage this program runs from, if any. The AppImage runtime mounts
/// the image and sets `APPIMAGE` (the `.AppImage` file) and `APPDIR` (where it
/// is mounted); programs started from there inherit both, so they only count
/// when `exe` lies inside `APPDIR`. An AppImage is updated by replacing that
/// file, and a second window or a restart starts that file – the mount
/// ends with the window that made it.
pub fn running_appimage(
    exe: &Path,
    appimage: Option<&std::ffi::OsStr>,
    appdir: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    let appimage = PathBuf::from(appimage?);
    let appdir = Path::new(appdir?);
    let inside = appdir.is_absolute() && exe.starts_with(appdir);
    (inside && appimage.is_absolute()).then_some(appimage)
}

/// Whether the desktop entry `desktop` (text of `proava.desktop`) starts the
/// AppImage `appimage` – then it is in the start menu with its file types.
pub fn desktop_entry_starts(desktop: &str, appimage: &Path) -> bool {
    let path = appimage.to_string_lossy();
    desktop
        .lines()
        .filter_map(|line| line.strip_prefix("Exec="))
        .any(|exec| exec_program(exec) == path)
}

/// The program of an `Exec=` value: the first argument, quoted or not, with
/// the escapes of the Desktop Entry Specification undone (`\\\\` = `\`,
/// `\"`, `` \` ``, `\$` inside quotes, `%%` = `%`).
fn exec_program(exec: &str) -> String {
    let exec = exec.trim_start();
    let Some(quoted) = exec.strip_prefix('"') else {
        return exec
            .split(' ')
            .next()
            .unwrap_or_default()
            .replace("%%", "%");
    };
    let mut program = String::new();
    let mut chars = quoted.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => break,
            '\\' => match chars.next() {
                // Four backslashes: one literal backslash.
                Some('\\') => {
                    for _ in 0..2 {
                        chars.next_if_eq(&'\\');
                    }
                    program.push('\\');
                }
                Some(escaped) => program.push(escaped),
                None => {}
            },
            other => program.push(other),
        }
    }
    program.replace("%%", "%")
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

    #[test]
    fn an_appimage_counts_only_for_the_program_inside_it() {
        use std::ffi::OsStr;
        let image = OsStr::new("/home/x/Programme/proAVA-x86_64.AppImage");
        let mount = OsStr::new("/tmp/.mount_proAVAab12");
        let inside = Path::new("/tmp/.mount_proAVAab12/usr/bin/proava");
        assert_eq!(
            running_appimage(inside, Some(image), Some(mount)),
            Some(PathBuf::from(image))
        );
        assert_eq!(
            running_appimage(
                Path::new("/home/x/.local/bin/proava"),
                Some(image),
                Some(mount)
            ),
            None,
            "von dort gestartet, aber ein anderes proAVA: erbt nur die Umgebung"
        );
        assert_eq!(running_appimage(inside, None, Some(mount)), None);
        assert_eq!(running_appimage(inside, Some(image), None), None);
        assert_eq!(
            running_appimage(inside, Some(OsStr::new("proAVA.AppImage")), Some(mount)),
            None,
            "nur ein voller Pfad"
        );
    }

    #[test]
    fn the_desktop_entry_names_the_appimage_it_starts() {
        let image = Path::new("/home/x/Programme/proAVA-x86_64.AppImage");
        let entry =
            |exec: &str| format!("[Desktop Entry]\nName=proAVA\nExec={exec}\nIcon=proava\n");
        assert!(desktop_entry_starts(
            &entry("\"/home/x/Programme/proAVA-x86_64.AppImage\" %f"),
            image
        ));
        assert!(desktop_entry_starts(
            &entry("/home/x/Programme/proAVA-x86_64.AppImage %f"),
            image
        ));
        assert!(!desktop_entry_starts(
            &entry("/home/x/.local/bin/proava %f"),
            image
        ));
        assert!(!desktop_entry_starts(
            &entry("\"/home/x/Downloads/proAVA-x86_64.AppImage\" %f"),
            image
        ));
        // Spaces, $, ", % and \ the way install_linux.sh writes them.
        let odd = Path::new("/home/x/Meine Programme/pro$AVA \"neu\" 100% a\\b.AppImage");
        assert!(desktop_entry_starts(
            &entry(
                "\"/home/x/Meine Programme/pro\\$AVA \\\"neu\\\" 100%% a\\\\\\\\b.AppImage\" %f"
            ),
            odd
        ));
    }
}
