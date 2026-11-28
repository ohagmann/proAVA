//! Long work (GAEB export/import, PDF) on a worker thread, so the window stays usable.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// One running piece of work and the channel its result comes back on.
pub struct Job<T> {
    label: String,
    receiver: Receiver<T>,
}

pub enum Poll<T> {
    Running,
    Done(T),
    /// The worker died without an answer (panic).
    Lost,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(label: impl Into<String>, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(work());
        });
        Self {
            label: label.into(),
            receiver,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn poll(&self) -> Poll<T> {
        match self.receiver.try_recv() {
            Ok(value) => Poll::Done(value),
            Err(TryRecvError::Empty) => Poll::Running,
            Err(TryRecvError::Disconnected) => Poll::Lost,
        }
    }
}

/// Environment variable that counts how many proAVA windows started one
/// another (parent 0, its „Neues Fenster“ 1 …).
const WINDOW_DEPTH: &str = "PROAVA_WINDOW_DEPTH";

/// More chained windows than this are refused – a last brake should a bug
/// ever make proAVA start itself in a loop.
const MAX_WINDOW_DEPTH: u32 = 8;

/// Other programs (PDF viewer, browser, a second proAVA) start only from
/// the real program window: `run_with` switches the system on (like the
/// clipboard). Never in a test – there `current_exe()` is the test program,
/// which would start itself again and again (night of 17./18.11.2026: a
/// random UI test did exactly that until the PC ran out of memory).
fn external_programs_allowed() -> bool {
    crate::system_clipboard::enabled()
}

thread_local! {
    /// Starts this thread refused because other programs are off.
    static REFUSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many starts (second window, PDF viewer, browser) this thread refused
/// because other programs are off – tests use it to prove that a path which
/// would start a program was reached and stopped (per thread, so tests
/// running side by side do not count for each other).
pub fn refused_starts() -> usize {
    REFUSED.with(std::cell::Cell::get)
}

fn refuse() -> Result<(), String> {
    REFUSED.with(|count| count.set(count.get() + 1));
    Ok(())
}

/// Whether `program` is a proAVA program file and not a test program.
/// Cargo names test programs after the test file or crate with a hash
/// („affentest-1a2b…“, „proava_ui-1a2b…“), the program is „proava“
/// (Windows „proava.exe“) or a copy like „proava-7883b83“.
fn is_proava_program(program: &Path) -> bool {
    let stem = program
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    stem == "proava" || (stem.starts_with("proava-") && stem != "proava-mcp")
}

/// The program file of this proAVA. On Linux the running program's path
/// gets „ (deleted)“ appended once its file is replaced while it runs – an
/// update with `install` does that (`scripts/install_linux.sh`). The new
/// file at the same place then counts; without this, a proAVA started
/// before an update no longer knew itself, and the browser window and
/// „Neues Fenster“ were off (Anwender 19.11.2026 00:08).
fn program_file() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map(proava_application::program_update::without_deleted_mark)
        .map_err(|error| format!("Programmdatei nicht gefunden: {error}"))
}

/// The AppImage this proAVA runs from (see
/// [`proava_application::program_update::running_appimage`]).
pub(crate) fn appimage() -> Option<PathBuf> {
    let exe = program_file().ok()?;
    proava_application::program_update::running_appimage(
        &exe,
        std::env::var_os("APPIMAGE").as_deref(),
        std::env::var_os("APPDIR").as_deref(),
    )
}

/// Desktop entry of proAVA in the start menu (`install_linux.sh`).
fn desktop_entry() -> Option<PathBuf> {
    let share = std::env::var_os("XDG_DATA_HOME")
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(share.join("applications/proava.desktop"))
}

/// The AppImage this proAVA runs from when the start menu does not start it
/// yet (no entry, or one for another file or the installed program) – then
/// the start screen offers „Ins Startmenü eintragen“. Never in tests.
pub fn appimage_not_in_menu() -> Option<PathBuf> {
    if !external_programs_allowed() {
        return None;
    }
    let image = appimage()?;
    let text = desktop_entry()
        .and_then(|entry| std::fs::read_to_string(entry).ok())
        .unwrap_or_default();
    (!proava_application::program_update::desktop_entry_starts(&text, &image)).then_some(image)
}

/// Puts the running AppImage into the start menu with the file types
/// .proava and GAEB (`install_linux.sh --appimage` from inside the image);
/// returns what it says. Same brakes as [`new_window`].
pub fn put_appimage_in_menu() -> Result<String, String> {
    if !external_programs_allowed() {
        refuse()?;
        return Ok(String::new());
    }
    let image = appimage().ok_or("proAVA läuft nicht als AppImage.")?;
    let script = std::env::var_os("APPDIR")
        .map(|folder| PathBuf::from(folder).join("usr/share/proava/install_linux.sh"))
        .filter(|script| script.is_file())
        .ok_or("Im AppImage fehlt install_linux.sh.")?;
    let output = std::process::Command::new(&script)
        .arg("--appimage")
        .arg(&image)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Eintragen ging nicht: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "Eintragen ging nicht: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Depth of this window in a chain of „Neues Fenster“ (0 = started by the user).
fn window_depth() -> u32 {
    std::env::var(WINDOW_DEPTH)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

/// Starts a second proAVA (same program file) with `arguments`.
///
/// Three independent brakes: only from the real program window, only if the
/// program file is proAVA (not a test program), and at most
/// [`MAX_WINDOW_DEPTH`] windows started one from another.
pub fn new_window(arguments: &[std::ffi::OsString]) -> Result<(), String> {
    start_proava(arguments, 1)
}

/// „Neu starten“ after an update: starts the (new) program file with
/// `arguments`, the caller then closes its window. Same brakes as
/// [`new_window`]; the new program takes this window's place, so the
/// depth of chained windows stays as it is.
pub fn restart(arguments: &[std::ffi::OsString]) -> Result<(), String> {
    start_proava(arguments, 0)
}

/// Whether this is the real proAVA program window (not a test, not another
/// program): only then it watches its program file for updates.
pub(crate) fn real_program() -> bool {
    external_programs_allowed() && program_file().is_ok_and(|program| is_proava_program(&program))
}

/// [`new_window`] and [`restart`]: the started program's window depth is
/// this one's plus `deeper`.
fn start_proava(arguments: &[std::ffi::OsString], deeper: u32) -> Result<(), String> {
    if !external_programs_allowed() {
        return refuse();
    }
    let program = program_file()?;
    if !is_proava_program(&program) {
        return Err(format!(
            "Neues Fenster nur aus proAVA selbst ({} ist kein proAVA-Programm).",
            program.display()
        ));
    }
    let depth = window_depth();
    if depth >= MAX_WINDOW_DEPTH {
        return Err(format!(
            "Neues Fenster abgelehnt: schon {depth} Fenster nacheinander geöffnet."
        ));
    }
    // From an AppImage the image itself: the program inside lives only as
    // long as the window that mounted it.
    let program = appimage().unwrap_or(program);
    std::process::Command::new(&program)
        .args(arguments)
        .env(WINDOW_DEPTH, (depth + deeper).to_string())
        .spawn()
        .map(reap)
        .map_err(|error| format!("Neues Fenster lässt sich nicht starten: {error}"))
}

/// Waits for a started program on a helper thread, so that it does not stay
/// behind as a zombie process until proAVA ends (Linux/macOS; closed
/// second windows and finished `xdg-open` calls – Abnahme qa3, 18.11.).
fn reap(mut child: std::process::Child) {
    let _ = thread::Builder::new()
        .name("proava-child".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

/// Opens a file (or a web address) with the program the system uses for it
/// (PDF viewer, browser …). Does nothing in tests (see
/// [`external_programs_allowed`]).
pub fn open_with_system(file: &Path) -> Result<(), String> {
    if !external_programs_allowed() {
        return refuse();
    }
    #[cfg(windows)]
    {
        windows_open(file)
    }
    #[cfg(not(windows))]
    {
        system_open_command(file)
            .spawn()
            .map(reap)
            .map_err(|error| format!("{} lässt sich nicht öffnen: {error}", file.display()))
    }
}

/// Windows: `ShellExecuteW` with „open“ – the program registered for the
/// file type, without a console window and without `cmd`, which would
/// read `&`, `^` and `%` in paths and addresses as commands.
#[cfg(windows)]
fn windows_open(file: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{ShellExecuteW, SE_ERR_NOASSOC};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |text: &std::ffi::OsStr| text.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide(std::ffi::OsStr::new("open"));
    let target = wide(file.as_os_str());
    // SAFETY: both strings are NUL-terminated UTF-16 and live until the call
    // returns; no window handle, parameters or working folder are passed.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    // Values above 32 mean success.
    match result {
        code if code > 32 => Ok(()),
        code if code == SE_ERR_NOASSOC as isize => Err(format!(
            "{}: Unter Windows ist kein Programm für diesen Dateityp eingerichtet.",
            file.display()
        )),
        2 | 3 => Err(format!("{} nicht gefunden.", file.display())),
        code => Err(format!(
            "{} lässt sich nicht öffnen (Windows-Fehler {code}).",
            file.display()
        )),
    }
}

#[cfg(not(windows))]
fn system_open_command(file: &Path) -> std::process::Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(file);
        command
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(file);
        command
    }
}

/// proAVA's browser window (`proava-web`) runs at most once at a time.
static HELPER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The browser window next to this proAVA program file, or why there is
/// none (never in tests: the test program is no proAVA program). The
/// reason is shown wherever the system browser takes over.
fn helper_file() -> Result<PathBuf, String> {
    let program = program_file()?;
    if !is_proava_program(&program) {
        return Err(format!(
            "Das Browserfenster startet nur aus proAVA selbst ({} ist kein proAVA-Programm).",
            program.display()
        ));
    }
    let helper = proava_web_protocol::helper_program(&program);
    if helper.is_file() && proava_web_protocol::is_helper_program(&helper) {
        Ok(helper)
    } else {
        Err(format!(
            "Das Browserfenster von proAVA ist nicht installiert ({} fehlt).",
            helper.display()
        ))
    }
}

/// Whether proAVA's own browser window can be used (installed next to the
/// program); otherwise the system browser does the job.
pub fn helper_available() -> bool {
    helper_file().is_ok()
}

/// Why proAVA's browser window cannot be used (`None`: it can).
pub fn helper_missing() -> Option<String> {
    helper_file().err()
}

/// The running browser window: its input (one [`proava_web_protocol::ToHelper`]
/// per line) and output. Dropping it closes the input – the window closes
/// and ends – and frees the place for the next one.
pub struct HelperProcess {
    pub input: Option<std::process::ChildStdin>,
    pub output: Option<std::process::ChildStdout>,
    child: Option<std::process::Child>,
}

impl Drop for HelperProcess {
    fn drop(&mut self) {
        // End of input = the window closes (it never outlives proAVA).
        self.input = None;
        if let Some(mut child) = self.child.take() {
            let _ = thread::Builder::new()
                .name("proava-web".into())
                .spawn(move || {
                    let _ = child.wait();
                    HELPER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
                });
        } else {
            HELPER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// Starts proAVA's browser window with `arguments`
/// ([`proava_web_protocol::Launch::arguments`]). Same brakes as
/// [`new_window`]: only from the real program window (`Ok(None)` in tests),
/// only the helper program next to a proAVA program file, and only one
/// window at a time.
pub fn start_helper(arguments: &[std::ffi::OsString]) -> Result<Option<HelperProcess>, String> {
    if !external_programs_allowed() {
        refuse()?;
        return Ok(None);
    }
    let helper = helper_file()?;
    if HELPER_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Das Browserfenster ist schon offen.".into());
    }
    let mut command = std::process::Command::new(&helper);
    command
        .args(arguments)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    match command.spawn() {
        Ok(mut child) => Ok(Some(HelperProcess {
            input: child.stdin.take(),
            output: child.stdout.take(),
            child: Some(child),
        })),
        Err(error) => {
            HELPER_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
            Err(format!(
                "Das Browserfenster lässt sich nicht starten: {error}"
            ))
        }
    }
}

/// A status message that may offer to open the file it is about, or
/// „Rückgängig“ for a change outside the project (a library file).
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub started: f64,
    pub file: Option<PathBuf>,
    pub undo: bool,
}

impl Message {
    /// Messages with a file or „Rückgängig“ stay longer, so there is time
    /// to click.
    pub fn expired(&self, now: f64) -> bool {
        let seconds = if self.file.is_some() || self.undo {
            15.0
        } else {
            5.0
        };
        now - self.started > seconds
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{Job, Message, Poll};

    #[test]
    fn job_result_comes_back() {
        let job = Job::spawn("Rechnen", || 6 * 7);
        assert_eq!(job.label(), "Rechnen");
        let start = Instant::now();
        loop {
            match job.poll() {
                Poll::Done(value) => {
                    assert_eq!(value, 42);
                    break;
                }
                Poll::Running if start.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => panic!("keine Antwort"),
            }
        }
    }

    #[test]
    fn panicking_job_is_lost_not_hanging() {
        let job: Job<u8> = Job::spawn("kaputt", || panic!("Absicht"));
        let start = Instant::now();
        loop {
            match job.poll() {
                Poll::Lost => break,
                Poll::Running if start.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => panic!("falscher Zustand"),
            }
        }
    }

    #[test]
    fn messages_with_files_stay_longer() {
        let plain = Message {
            text: "x".into(),
            started: 0.0,
            file: None,
            undo: false,
        };
        let with_file = Message {
            file: Some(PathBuf::from("/a.pdf")),
            ..plain.clone()
        };
        assert!(plain.expired(6.0));
        assert!(!with_file.expired(6.0));
        assert!(with_file.expired(16.0));
        let with_undo = Message {
            undo: true,
            ..plain.clone()
        };
        assert!(!with_undo.expired(6.0));
    }

    /// Night of 17./18.11.2026: a random UI test clicked „Neues Fenster“, the
    /// test program started itself again and again until the PC ran out of
    /// memory. In a test nothing may start – neither a window nor a viewer.
    #[test]
    fn tests_never_start_programs() {
        assert!(
            !super::external_programs_allowed(),
            "Tests laufen ohne run_with"
        );
        assert_eq!(super::new_window(&[]), Ok(()));
        assert_eq!(
            super::open_with_system(std::path::Path::new("/gibt/es/nicht.pdf")),
            Ok(())
        );
        let before = super::refused_starts();
        assert!(
            matches!(super::start_helper(&[]), Ok(None)),
            "kein Browserfenster"
        );
        assert_eq!(super::refused_starts(), before + 1);
        assert!(
            !super::helper_available(),
            "Testprogramm hat kein proava-web"
        );
        let me = std::env::current_exe().unwrap();
        assert!(
            !super::is_proava_program(&me),
            "Testprogramm {} gilt nicht als proAVA",
            me.display()
        );
    }

    #[test]
    fn only_proava_program_files_start_new_windows() {
        let yes = |name: &str| super::is_proava_program(std::path::Path::new(name));
        assert!(yes("/usr/bin/proava"));
        assert!(yes("C:/Programme/proAVA/proava.exe"));
        assert!(yes("/tmp/kopie/proava-7883b83"));
        // The browser window is a proAVA program file too (it may be
        // started next to proava, see `start_helper`).
        assert!(yes("/usr/lib/proava/proava-web"));
        assert!(!yes("/x/target/debug/deps/affentest-1a2b3c4d5e6f"));
        assert!(!yes("/x/target/debug/deps/proava_ui-1a2b3c4d5e6f"));
        assert!(!yes("/x/target/debug/deps/ui_templates-1a2b3c4d5e6f"));
        assert!(!yes("/x/target/release/proava-mcp"));
    }

    /// A proAVA updated while it runs (Linux, `install` replaces the file):
    /// its path reads „…/proava (deleted)“. It must still know itself, so
    /// the browser window and „Neues Fenster“ stay on (19.11.2026 00:08:
    /// the take-over dialog silently used the system browser).
    #[test]
    fn a_program_replaced_while_it_runs_still_knows_itself() {
        use std::path::PathBuf;
        let program = |path: &str| {
            proava_application::program_update::without_deleted_mark(PathBuf::from(path))
        };
        #[cfg(target_os = "linux")]
        {
            let replaced = program("/home/x/.local/bin/proava (deleted)");
            assert_eq!(replaced, PathBuf::from("/home/x/.local/bin/proava"));
            assert!(super::is_proava_program(&replaced));
            assert_eq!(
                proava_web_protocol::helper_program(&replaced),
                PathBuf::from("/home/x/.local/bin/proava-web")
            );
            assert_eq!(
                program("/opt/open ava (deleted)/proava (deleted)"),
                PathBuf::from("/opt/open ava (deleted)/proava"),
                "nur das Ende"
            );
            // Test programs stay test programs.
            assert!(!super::is_proava_program(&program(
                "/x/target/debug/deps/proava_ui-1a2b3c4d5e6f (deleted)"
            )));
        }
        assert_eq!(program("/usr/bin/proava"), PathBuf::from("/usr/bin/proava"));
        assert_eq!(
            program("C:/Programme/proAVA/proava.exe"),
            PathBuf::from("C:/Programme/proAVA/proava.exe")
        );
        // In a test the reason why the window is off is named.
        let reason = super::helper_missing().expect("Testprogramm");
        assert!(reason.contains("kein proAVA-Programm"), "{reason}");
    }
}
