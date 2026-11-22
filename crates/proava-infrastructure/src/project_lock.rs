//! Lock file of an opened project, like LibreOffice: `.~lock.<Datei>#` next
//! to the project with `proAVA;<Benutzer>;<Rechner>;<PID>;<TT.MM.JJJJ HH:MM>`.
//! A second proAVA sees it and offers to open the project read-only.
//!
//! A lock of this computer whose PID runs no proAVA any more (killed,
//! crashed) is stale: proAVA removes it without asking, and the MCP server
//! does not count it. On another computer (network drive) nobody can tell –
//! the lock counts.

use std::fs;
use std::path::{Path, PathBuf};

use proava_application::ProjectLock;

/// `.~lock.Demo.proava#` next to `Demo.proava`.
pub fn lock_path(project_file: &Path) -> PathBuf {
    let name = project_file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    project_file.with_file_name(format!(".~lock.{name}#"))
}

pub fn write(project_file: &Path) -> Result<(), String> {
    let since = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    let line = format!(
        "proAVA;{};{};{};{since}\n",
        user(),
        host(),
        std::process::id()
    );
    let path = lock_path(project_file);
    fs::write(&path, line).map_err(|error| format!("Sperrdatei {}: {error}", path.display()))
}

pub fn read(project_file: &Path) -> Option<ProjectLock> {
    let text = fs::read_to_string(lock_path(project_file)).ok()?;
    parse(&text, std::process::id(), &host(), process_alive)
}

/// Removes the lock (only call for an own or stale lock).
pub fn remove(project_file: &Path) {
    let _ = fs::remove_file(lock_path(project_file));
}

fn parse(
    text: &str,
    own_pid: u32,
    own_host: &str,
    alive: impl Fn(u32) -> Option<bool>,
) -> Option<ProjectLock> {
    let mut fields = text.trim().split(';');
    if fields.next()? != "proAVA" {
        return None;
    }
    let user = fields.next().unwrap_or_default().to_string();
    let host = fields.next().unwrap_or_default().to_string();
    let pid: u32 = fields.next().unwrap_or_default().parse().ok()?;
    let since = fields.next().unwrap_or_default().to_string();
    let same_host = host == own_host;
    let own = same_host && pid == own_pid;
    // On another computer nobody can tell: the lock counts.
    let stale = same_host && !own && alive(pid) == Some(false);
    Some(ProjectLock {
        user,
        host,
        pid,
        since,
        own,
        stale,
    })
}

fn user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
        .replace(';', " ")
}

fn host() -> String {
    let from_file = fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| fs::read_to_string("/etc/hostname"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    from_file
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unbekannt".to_string())
        .replace(';', " ")
}

/// Does a proAVA with this PID run? None: cannot tell (not Linux).
#[cfg(not(windows))]
pub(crate) fn process_alive(pid: u32) -> Option<bool> {
    if !Path::new("/proc/self").exists() {
        return None;
    }
    let command = fs::read(format!("/proc/{pid}/cmdline")).ok();
    Some(command.is_some_and(|bytes| proava_command(&bytes)))
}

/// Whether a Linux command line (`/proc/<pid>/cmdline`, arguments separated
/// by NUL) runs a proAVA: only the program file counts, as on Windows – a
/// PID given out again to a viewer of `proava-handbuch.pdf` or an editor in
/// a folder „proAVA“ is no proAVA. A process that ended but was not
/// collected yet has an empty command line.
#[cfg_attr(windows, allow(dead_code))]
fn proava_command(cmdline: &[u8]) -> bool {
    let program = cmdline.split(|byte| *byte == 0).next().unwrap_or_default();
    is_proava_file(&String::from_utf8_lossy(program))
}

/// Windows: does a process with this PID run, and is it a proAVA? None
/// when Windows does not tell (e.g. a process of another user on a
/// terminal server).
#[cfg(windows)]
pub(crate) fn process_alive(pid: u32) -> Option<bool> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: plain Win32 calls with a handle that is checked for null and
    // closed again; the name buffer and its length belong to this frame.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // No such process: ERROR_INVALID_PARAMETER; anything else
            // (access denied) leaves it open.
            return (GetLastError() == ERROR_INVALID_PARAMETER).then_some(false);
        }
        let mut code = 0u32;
        let running = GetExitCodeProcess(handle, &mut code) != 0 && code == STILL_ACTIVE as u32;
        let mut name = [0u16; 1024];
        let mut length = name.len() as u32;
        let named =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, name.as_mut_ptr(), &mut length)
                != 0;
        CloseHandle(handle);
        if !named {
            // Ended: stale. Runs, but the program is unknown: cannot tell.
            return if running { None } else { Some(false) };
        }
        let program = String::from_utf16_lossy(&name[..length as usize]);
        // The program file, not the whole path: Windows gives PIDs out again
        // soon, and a folder „proAVA“ must not make any program count.
        Some(running && is_proava_file(&program))
    }
}

/// Whether the program file of a full path (`C:\Programme\proAVA\proava.exe`)
/// is a proAVA: its name holds „proava“ (also a renamed copy like
/// `proava-0.2.exe`); the folders do not count.
fn is_proava_file(program: &str) -> bool {
    program
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|file| file.to_lowercase().contains("proava"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_program_file_counts() {
        assert!(is_proava_file("C:\\Programme\\proAVA\\proava.exe"));
        assert!(is_proava_file("D:\\Werkzeuge\\ProAVA-0.2.EXE"));
        assert!(is_proava_file("/usr/bin/proava"));
        assert!(!is_proava_file("C:\\Programme\\proAVA\\AcroRd32.exe"));
        assert!(!is_proava_file("C:\\Users\\proava\\AppData\\Local\\x.exe"));
    }

    #[test]
    fn on_linux_only_the_program_of_the_command_line_counts() {
        assert!(proava_command(
            b"/usr/local/bin/proava\0/p/Rathaus.proava\0"
        ));
        assert!(proava_command(b"proava\0"));
        assert!(proava_command(b"/home/nutzer/proava/target/debug/proava\0"));
        assert!(!proava_command(b"evince\0/p/proava-handbuch.pdf\0"));
        assert!(!proava_command(b"/usr/bin/vim\0/home/u/proAVA/notiz.txt\0"));
        assert!(!proava_command(b""), "beendet, noch nicht abgeholt");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_lock_of_a_dead_process_on_this_computer_is_stale() {
        // A folder only this test uses (unique, created exclusively).
        let scratch = tempfile::Builder::new()
            .prefix("proava-lock-tot-")
            .tempdir()
            .unwrap();
        let project = scratch.path().join("Rathaus.proava");
        write(&project).unwrap();
        // The same lock, but from a process that no longer runs.
        let line = fs::read_to_string(lock_path(&project)).unwrap();
        let mut fields: Vec<&str> = line.trim().split(';').collect();
        let dead = (u32::MAX - 1).to_string();
        fields[3] = &dead;
        fs::write(lock_path(&project), fields.join(";") + "\n").unwrap();
        let lock = read(&project).unwrap();
        assert!(lock.stale && !lock.own, "{lock:?}");
        // Another computer: it counts, whatever the PID.
        fields[2] = "anderer-rechner";
        fs::write(lock_path(&project), fields.join(";") + "\n").unwrap();
        let lock = read(&project).unwrap();
        assert!(!lock.stale && !lock.own, "{lock:?}");
        assert!(
            lock.who().contains("auf anderer-rechner seit"),
            "{}",
            lock.who()
        );
    }

    #[test]
    fn own_foreign_and_stale_locks() {
        let line = "proAVA;anwender;rechner1;4711;17.11.2026 22:30\n";
        let own = parse(line, 4711, "rechner1", |_| Some(true)).unwrap();
        assert!(own.own && !own.stale);
        assert_eq!(own.who(), "anwender auf rechner1 seit 17.11.2026 22:30");
        let other = parse(line, 1, "rechner1", |_| Some(true)).unwrap();
        assert!(!other.own && !other.stale);
        let stale = parse(line, 1, "rechner1", |_| Some(false)).unwrap();
        assert!(stale.stale);
        let elsewhere = parse(line, 1, "laptop", |_| Some(false)).unwrap();
        assert!(!elsewhere.stale, "anderer Rechner: Sperre gilt");
        assert!(
            parse("Max;host;1;x", 1, "host", |_| None).is_none(),
            "fremde Sperrdatei"
        );
    }

    #[test]
    fn lock_file_round_trip_next_to_the_project() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-lock-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        fs::create_dir_all(&folder).unwrap();
        let project = folder.join("Demo Haus.proava");
        assert_eq!(lock_path(&project), folder.join(".~lock.Demo Haus.proava#"));
        assert!(read(&project).is_none());
        write(&project).unwrap();
        let lock = read(&project).unwrap();
        assert!(lock.own, "{lock:?}");
        remove(&project);
        assert!(read(&project).is_none());
        let _ = fs::remove_dir_all(&folder);
    }
}
