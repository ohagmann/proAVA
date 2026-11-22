//! The local bridge between the running proAVA windows and the MCP server:
//! the folder where the windows announce themselves, the channel of a window
//! (Unix socket, Windows named pipe) and the client that asks a window.
//!
//! Only the own user reaches it: on Linux the folder is
//! `$XDG_RUNTIME_DIR/proava` (mode 0700, the runtime folder itself belongs
//! to the user and is not on a network drive), entries and sockets 0600; on
//! Windows the entries lie in `%LOCALAPPDATA%\proava\fenster` and the pipe
//! only lets the owner and the system in (`PIPE_REJECT_REMOTE_CLIENTS`, no
//! access over the network). The protocol is in `proava-bridge-protocol`.
//!
//! Tests use a folder of their own ([`BridgeFolder::at`]); nothing here starts
//! a process.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use proava_bridge_protocol::{
    decode, encode, entry_file_name, pid_of_entry, pid_of_socket, read_request, socket_file_name,
    Request, Response, WindowEntry, MAX_LINE,
};

/// What a window does with a request (runs on a thread of the channel; the
/// program hands it to its UI thread and waits for the answer).
pub type Handler = Arc<dyn Fn(Request) -> Response + Send + Sync>;

/// At most so many connections at the same time (one MCP server needs one).
#[cfg(unix)]
const MAX_CONNECTIONS: usize = 8;

/// Milliseconds since 1970 (for entries and focus times).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis() as u64)
        .unwrap_or(0)
}

/// Does a proAVA with this PID run? None: cannot tell.
pub fn proava_running(pid: u32) -> Option<bool> {
    crate::project_lock::process_alive(pid)
}

/// The folder of the window entries (and on Unix of the sockets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeFolder {
    folder: PathBuf,
}

impl BridgeFolder {
    /// The folder of this user: Linux `$XDG_RUNTIME_DIR/proava`, Windows
    /// `%LOCALAPPDATA%\proava\fenster`. None when the system has no such
    /// place (no runtime folder): then there is no bridge.
    pub fn standard() -> Option<Self> {
        #[cfg(windows)]
        {
            let local = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())?;
            Some(Self::at(
                PathBuf::from(local).join("proava").join("fenster"),
            ))
        }
        #[cfg(not(windows))]
        {
            let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty())?;
            let runtime = PathBuf::from(runtime);
            runtime
                .is_absolute()
                .then(|| Self::at(runtime.join("proava")))
        }
    }

    /// A folder of one's own (tests: a temporary folder).
    pub fn at(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.folder
    }

    /// Creates the folder only the own user may enter (Unix 0700). A folder
    /// that cannot be set so (it belongs to someone else) is refused.
    pub fn prepare(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.folder)
            .map_err(|error| format!("{}: {error}", self.folder.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::symlink_metadata(&self.folder)
                .map_err(|error| format!("{}: {error}", self.folder.display()))?;
            if !metadata.is_dir() {
                return Err(format!("{} ist kein Ordner.", self.folder.display()));
            }
            // Only the owner may change the mode: a foreign folder fails here.
            std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| {
                    format!(
                        "{} lässt sich nicht nur für diesen Benutzer einrichten: {error}",
                        self.folder.display()
                    )
                })?;
        }
        Ok(())
    }

    /// Where whole projects pass between the MCP server and a window
    /// (created on demand, inside this folder only the user can enter).
    pub fn exchange_folder(&self) -> PathBuf {
        self.folder.join(proava_bridge_protocol::EXCHANGE_FOLDER)
    }

    /// Removes exchange files older than an hour (left by a crash).
    pub fn tidy_exchange(&self) {
        let Ok(entries) = std::fs::read_dir(self.exchange_folder()) else {
            return;
        };
        let hour = Duration::from_secs(3600);
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age > hour);
            let project = entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "proava");
            if old && project {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    pub fn entry_file(&self, pid: u32) -> PathBuf {
        self.folder.join(entry_file_name(pid))
    }

    pub fn socket_file(&self, pid: u32) -> PathBuf {
        self.folder.join(socket_file_name(pid))
    }

    /// Writes the entry of a window (atomic: other readers see the old or
    /// the new one, never half).
    pub fn publish(&self, entry: &WindowEntry) -> Result<(), String> {
        let file = self.entry_file(entry.pid);
        let temporary = self.folder.join(format!(".{}.tmp", entry.pid));
        let text = encode(entry);
        let write = || -> std::io::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut out = options.open(&temporary)?;
            out.write_all(text.as_bytes())?;
            out.write_all(b"\n")?;
            out.sync_all().ok();
            std::fs::rename(&temporary, &file)
        };
        write().map_err(|error| format!("{}: {error}", file.display()))
    }

    /// Takes the entry (and socket) of a window away.
    pub fn withdraw(&self, pid: u32) {
        let _ = std::fs::remove_file(self.entry_file(pid));
        let _ = std::fs::remove_file(self.socket_file(pid));
    }

    /// The entries of all windows (unreadable ones are left out).
    pub fn entries(&self) -> Vec<WindowEntry> {
        let Ok(list) = std::fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        let mut entries: Vec<WindowEntry> = list
            .flatten()
            .filter(|item| item.file_name().to_str().and_then(pid_of_entry).is_some())
            .filter(|item| item.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|item| std::fs::read_to_string(item.path()).ok())
            .filter_map(|text| decode::<WindowEntry>(&text).ok())
            .collect();
        entries.sort_by_key(|entry| entry.pid);
        entries
    }

    /// Removes entries and sockets of windows that no longer run
    /// (`running(pid)` = Some(false)); the own PID, running and unknown ones
    /// stay. Only files of exactly the pattern. Returns what was removed.
    pub fn remove_stale(
        &self,
        own_pid: u32,
        running: impl Fn(u32) -> Option<bool>,
    ) -> Vec<PathBuf> {
        let Ok(list) = std::fs::read_dir(&self.folder) else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        for item in list.flatten() {
            let name = item.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|name| pid_of_entry(name).or_else(|| pid_of_socket(name)))
            else {
                continue;
            };
            if pid == own_pid || item.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if running(pid) == Some(false) && std::fs::remove_file(item.path()).is_ok() {
                removed.push(item.path());
            }
        }
        removed.sort();
        removed
    }
}

/// The channel of one window: listens on its own thread, one more thread
/// per connection; ends (and takes its socket away) when dropped.
pub struct BridgeServer {
    channel: String,
    stop: Arc<AtomicBool>,
    #[cfg(unix)]
    socket: PathBuf,
}

impl BridgeServer {
    /// Opens the channel of the window `pid` in `folder` (Unix: socket
    /// `<pid>.sock`, Windows: a named pipe of its own name).
    pub fn start(folder: &BridgeFolder, pid: u32, handler: Handler) -> Result<Self, String> {
        folder.prepare()?;
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let socket = folder.socket_file(pid);
            // A socket left by an ended proAVA with the same PID.
            let _ = std::fs::remove_file(&socket);
            let listener = std::os::unix::net::UnixListener::bind(&socket)
                .map_err(|error| format!("{}: {error}", socket.display()))?;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| format!("{}: {error}", socket.display()))?;
            let flag = stop.clone();
            thread::Builder::new()
                .name("proava-bruecke".into())
                .spawn(move || {
                    let open = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                    for stream in listener.incoming() {
                        if flag.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(stream) = stream else { continue };
                        if open.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                            continue;
                        }
                        let handler = handler.clone();
                        let count = open.clone();
                        count.fetch_add(1, Ordering::SeqCst);
                        let spawned = thread::Builder::new()
                            .name("proava-bruecke-verbindung".into())
                            .spawn(move || {
                                let reader = match stream.try_clone() {
                                    Ok(reader) => reader,
                                    Err(_) => {
                                        count.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                };
                                serve(reader, stream, &handler);
                                count.fetch_sub(1, Ordering::SeqCst);
                            });
                        if spawned.is_err() {
                            open.fetch_sub(1, Ordering::SeqCst);
                        }
                    }
                })
                .map_err(|error| format!("Brücke: {error}"))?;
            Ok(Self {
                channel: socket.to_string_lossy().into_owned(),
                stop,
                socket,
            })
        }
        #[cfg(windows)]
        {
            let name = format!(
                r"\\.\pipe\proava-fenster-{pid}-{}",
                now_ms() % 1_000_000_007
            );
            pipe::listen(&name, stop.clone(), handler)?;
            Ok(Self {
                channel: name,
                stop,
            })
        }
    }

    /// Where the window is reached (for its entry).
    pub fn channel(&self) -> &str {
        &self.channel
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the listener so its thread sees the flag and ends.
        #[cfg(unix)]
        {
            let _ = std::os::unix::net::UnixStream::connect(&self.socket);
            let _ = std::fs::remove_file(&self.socket);
        }
        #[cfg(windows)]
        {
            let _ = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.channel);
        }
    }
}

/// The channel of the program window as the UI needs it (port
/// `WindowChannel`): folder, socket or pipe, entry.
pub struct SocketChannel {
    folder: BridgeFolder,
    server: Option<BridgeServer>,
    pid: u32,
    /// Does a proAVA with this PID run? (tests: windows of one test
    /// program with made-up PIDs count as running)
    running: Arc<dyn Fn(u32) -> Option<bool> + Send + Sync>,
}

impl SocketChannel {
    /// In the folder of this user (None without one, see [`BridgeFolder::standard`]).
    pub fn standard() -> Option<Self> {
        BridgeFolder::standard().map(Self::in_folder)
    }

    /// In a folder of one's own (tests: a temporary folder).
    pub fn in_folder(folder: BridgeFolder) -> Self {
        Self {
            folder,
            server: None,
            pid: 0,
            running: Arc::new(proava_running),
        }
    }

    /// Another check whether a window's process runs (tests: several
    /// windows in one test program, each with a PID of its own).
    pub fn with_process_check(
        mut self,
        running: Arc<dyn Fn(u32) -> Option<bool> + Send + Sync>,
    ) -> Self {
        self.running = running;
        self
    }
}

impl proava_application::window_bridge::WindowChannel for SocketChannel {
    fn open(
        &mut self,
        pid: u32,
        wake: proava_application::window_bridge::Wake,
    ) -> Result<std::sync::mpsc::Receiver<proava_application::window_bridge::Incoming>, String>
    {
        self.folder.prepare()?;
        // Entries of windows that ended without tidying up (crash).
        let running = self.running.clone();
        self.folder.remove_stale(pid, move |pid| running(pid));
        self.folder.tidy_exchange();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handler: Handler = Arc::new(proava_application::window_bridge::forward(
            sender,
            wake,
            proava_application::window_bridge::WINDOW_PATIENCE,
        ));
        self.server = Some(BridgeServer::start(&self.folder, pid, handler)?);
        self.pid = pid;
        Ok(receiver)
    }

    fn publish(&self, entry: &WindowEntry) {
        if self.server.is_some() {
            let _ = self.folder.publish(entry);
        }
    }

    fn close(&mut self) {
        if self.server.take().is_some() {
            self.folder.withdraw(self.pid);
        }
    }

    fn address(&self) -> String {
        self.server
            .as_ref()
            .map(|server| server.channel().to_string())
            .unwrap_or_default()
    }

    fn window_list(&self) -> proava_application::window_bridge::WindowList {
        let folder = self.folder.clone();
        let running = self.running.clone();
        Arc::new(move || {
            folder
                .entries()
                .into_iter()
                .filter(|entry| running(entry.pid) != Some(false))
                .collect()
        })
    }

    fn messenger(&self) -> proava_application::window_bridge::Messenger {
        Arc::new(|channel: &str, request: &Request, timeout: Duration| {
            ask(channel, request, timeout)
        })
    }

    fn exchange_folder(&self) -> Option<PathBuf> {
        Some(self.folder.exchange_folder())
    }
}

impl Drop for SocketChannel {
    fn drop(&mut self) {
        use proava_application::window_bridge::WindowChannel;
        self.close();
    }
}

/// The open windows as the MCP server sees them: the entries of windows
/// that still run (a window whose process is gone is left out).
pub fn open_windows(folder: &BridgeFolder) -> Vec<WindowEntry> {
    folder
        .entries()
        .into_iter()
        .filter(|entry| proava_running(entry.pid) != Some(false))
        .collect()
}

/// Reads request lines from `input` and writes one response line each,
/// until the other side closes or sends something too long.
fn serve(input: impl Read, mut output: impl Write, handler: &Handler) {
    let mut reader = BufReader::new(input);
    loop {
        let mut line = Vec::new();
        let read = (&mut reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line);
        match read {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.len() > MAX_LINE {
            let answer = Response::new(
                0,
                proava_bridge_protocol::Reply::refused("Anfrage zu lang."),
            );
            let _ = writeln!(output, "{}", encode(&answer));
            return;
        }
        let text = String::from_utf8_lossy(&line);
        if text.trim().is_empty() {
            continue;
        }
        let answer = match read_request(&text) {
            Ok(request) => handler(request),
            Err(answer) => *answer,
        };
        if writeln!(output, "{}", encode(&answer)).is_err() || output.flush().is_err() {
            return;
        }
    }
}

/// Asks a window over its channel (`entry.kanal`) and waits at most
/// `timeout` for the answer.
pub fn ask(channel: &str, request: &Request, timeout: Duration) -> Result<Response, String> {
    let line = encode(request);
    #[cfg(unix)]
    {
        let mut stream = std::os::unix::net::UnixStream::connect(channel)
            .map_err(|error| format!("Fenster nicht erreichbar ({channel}): {error}"))?;
        stream.set_read_timeout(Some(timeout)).ok();
        stream.set_write_timeout(Some(timeout)).ok();
        exchange(
            &mut stream.try_clone().map_err(|e| e.to_string())?,
            &mut stream,
            &line,
            request.id,
        )
    }
    #[cfg(windows)]
    {
        // Pipes have no timeouts: the exchange runs on a thread of its own,
        // a window that does not answer is given up (the thread ends when
        // the window closes the pipe).
        let channel = channel.to_string();
        let id = request.id;
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let result = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&channel)
                .map_err(|error| format!("Fenster nicht erreichbar ({channel}): {error}"))
                .and_then(|mut pipe| {
                    let mut reader = pipe.try_clone().map_err(|e| e.to_string())?;
                    exchange(&mut reader, &mut pipe, &line, id)
                });
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(timeout)
            .unwrap_or_else(|_| Err(LATE.to_string()))
    }
}

/// The window did not answer in time.
const LATE: &str = "Das Fenster hat nicht rechtzeitig geantwortet (beschäftigt oder hängt).";

fn exchange(
    input: &mut impl Read,
    output: &mut impl Write,
    line: &str,
    id: u64,
) -> Result<Response, String> {
    let sent = writeln!(output, "{line}").and_then(|_| output.flush());
    sent.map_err(|error| format!("Anfrage nicht gesendet: {error}"))?;
    let mut reader = BufReader::new(input);
    let mut answer = Vec::new();
    match (&mut reader)
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut answer)
    {
        Ok(0) => return Err("Das Fenster hat die Verbindung ohne Antwort beendet.".to_string()),
        Ok(_) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Err(LATE.to_string())
        }
        Err(error) => return Err(format!("Antwort nicht lesbar: {error}")),
    }
    let response: Response = decode(&String::from_utf8_lossy(&answer))?;
    if response.id != id && response.id != 0 {
        return Err(format!(
            "Antwort auf eine andere Anfrage ({} statt {id}).",
            response.id
        ));
    }
    Ok(response)
}

#[cfg(windows)]
mod pipe {
    //! Named pipe of a window: only the owner (and the system) may open it,
    //! no access from other computers.

    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use windows_sys::Win32::Foundation::{
        GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    use super::{serve, Handler};

    /// Owner rights and the system, nobody else (no inherited entries).
    const SDDL: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// One instance of the pipe; the first one must be new (nobody else may
    /// have taken the name before).
    fn instance(name: &[u16], first: bool) -> Result<std::fs::File, String> {
        // SAFETY: the SDDL string and the name are NUL-terminated buffers of
        // this frame; the descriptor is freed with LocalFree after use, the
        // handle is checked against INVALID_HANDLE_VALUE and then owned by
        // the File.
        unsafe {
            let sddl = wide(SDDL);
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(format!(
                    "Sicherheitsbeschreibung: Fehler {}",
                    GetLastError()
                ));
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            let mut mode = PIPE_ACCESS_DUPLEX;
            if first {
                mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
            }
            let handle: HANDLE = CreateNamedPipeW(
                name.as_ptr(),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &attributes,
            );
            LocalFree(descriptor as _);
            if handle == INVALID_HANDLE_VALUE {
                return Err(format!("Named Pipe: Fehler {}", GetLastError()));
            }
            Ok(std::fs::File::from_raw_handle(handle as _))
        }
    }

    pub(super) fn listen(
        name: &str,
        stop: Arc<AtomicBool>,
        handler: Handler,
    ) -> Result<(), String> {
        let wide_name = wide(name);
        let mut next = instance(&wide_name, true)?;
        std::thread::Builder::new()
            .name("proava-bruecke".into())
            .spawn(move || loop {
                let file = next;
                // SAFETY: the handle of `file` is a valid pipe instance created above.
                let connected = unsafe {
                    ConnectNamedPipe(file.as_raw_handle() as HANDLE, std::ptr::null_mut()) != 0
                        || GetLastError() == ERROR_PIPE_CONNECTED
                };
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                next = match instance(&wide_name, false) {
                    Ok(file) => file,
                    Err(_) => return,
                };
                if !connected {
                    continue;
                }
                let handler = handler.clone();
                let _ = std::thread::Builder::new()
                    .name("proava-bruecke-verbindung".into())
                    .spawn(move || {
                        if let Ok(reader) = file.try_clone() {
                            serve(reader, file, &handler);
                        }
                    });
            })
            .map(|_| ())
            .map_err(|error| format!("Brücke: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_bridge_protocol::{Command, Reply, WindowInfo};

    fn temp(name: &str) -> PathBuf {
        // Unique and created exclusively (also against other test runs at the
        // same time); the test removes it.
        let folder = tempfile::Builder::new()
            .prefix(&format!("proava-test-bruecke-{name}-"))
            .tempdir()
            .unwrap()
            .keep();
        // The code under test creates it.
        std::fs::remove_dir_all(&folder).ok();
        folder
    }

    fn entry(pid: u32) -> WindowEntry {
        WindowEntry {
            v: 1,
            pid,
            kanal: format!("/x/{pid}.sock"),
            fenster: WindowInfo {
                pid,
                ..WindowInfo::default()
            },
            ..WindowEntry::default()
        }
    }

    #[cfg(unix)]
    #[test]
    fn only_the_own_user_reaches_the_folder() {
        use std::os::unix::fs::PermissionsExt;
        let folder = BridgeFolder::at(temp("rechte"));
        std::fs::create_dir_all(folder.path()).unwrap();
        std::fs::set_permissions(folder.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        folder.prepare().unwrap();
        let mode = std::fs::metadata(folder.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "Ordner nur für den Benutzer");
        folder.publish(&entry(41)).unwrap();
        let mode = std::fs::metadata(folder.entry_file(41))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "Eintrag nur für den Benutzer");
        std::fs::remove_dir_all(folder.path()).unwrap();
    }

    #[test]
    fn entries_are_written_read_and_withdrawn() {
        let folder = BridgeFolder::at(temp("eintraege"));
        folder.prepare().unwrap();
        folder.publish(&entry(12)).unwrap();
        folder.publish(&entry(7)).unwrap();
        let mut changed = entry(12);
        changed.fenster.ki_erlaubt = true;
        folder.publish(&changed).unwrap();
        std::fs::write(folder.path().join("fremd.json"), "{}").unwrap();
        std::fs::write(folder.path().join("13.json"), "kaputt").unwrap();
        let entries = folder.entries();
        assert_eq!(entries.iter().map(|e| e.pid).collect::<Vec<_>>(), [7, 12]);
        assert!(
            entries[1].fenster.ki_erlaubt,
            "neuer Stand ersetzt den alten"
        );
        assert!(
            !folder.path().join(".12.tmp").exists(),
            "keine halbe Datei bleibt liegen"
        );
        folder.withdraw(12);
        assert_eq!(folder.entries().len(), 1);
        std::fs::remove_dir_all(folder.path()).unwrap();
    }

    #[test]
    fn stale_entries_of_ended_windows_go() {
        let folder = BridgeFolder::at(temp("verwaist"));
        folder.prepare().unwrap();
        for pid in [111, 222, 333, 444] {
            folder.publish(&entry(pid)).unwrap();
        }
        std::fs::write(folder.socket_file(111), b"").unwrap();
        std::fs::write(folder.path().join("111.txt"), b"fremd").unwrap();
        std::fs::create_dir(folder.path().join("555.json")).unwrap();
        let removed = folder.remove_stale(444, |pid| match pid {
            222 => Some(true),
            333 => None,
            _ => Some(false),
        });
        assert_eq!(
            removed,
            vec![folder.entry_file(111), folder.socket_file(111)]
        );
        let left: Vec<u32> = folder.entries().iter().map(|e| e.pid).collect();
        assert_eq!(left, [222, 333, 444]);
        assert!(
            folder.path().join("111.txt").exists(),
            "fremde Dateien bleiben"
        );
        assert!(folder.path().join("555.json").is_dir(), "Ordner bleiben");
        assert!(BridgeFolder::at(temp("gibt-es-nicht"))
            .remove_stale(1, |_| Some(false))
            .is_empty());
        std::fs::remove_dir_all(folder.path()).unwrap();
    }

    #[test]
    fn requests_are_served_line_by_line() {
        let handler: Handler = Arc::new(|request: Request| {
            Response::new(
                request.id,
                Reply::Aus {
                    text: request.befehl.name().to_string(),
                },
            )
        });
        let input = format!(
            "{}\n\nkaputt\n{}\n",
            encode(&Request::new(3, Command::FensterLesen)),
            encode(&Request::new(4, Command::FensterLesen))
        );
        let mut output = Vec::new();
        serve(input.as_bytes(), &mut output, &handler);
        let lines: Vec<Response> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| decode(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].id, 3);
        assert_eq!(lines[0].antwort.refusal(), Some("fenster_lesen"));
        assert!(lines[1]
            .antwort
            .refusal()
            .unwrap()
            .contains("unverständlich"));
        assert_eq!(lines[2].id, 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_window_answers_over_its_socket_and_cleans_up() {
        use std::os::unix::fs::PermissionsExt;
        let folder = BridgeFolder::at(temp("socket"));
        let handler: Handler = Arc::new(|request: Request| {
            let mut info = WindowInfo {
                pid: 99,
                ..WindowInfo::default()
            };
            info.programm = format!("Anfrage {}", request.id);
            Response::new(request.id, Reply::Fenster(info))
        });
        let server = BridgeServer::start(&folder, 99, handler).unwrap();
        let socket = folder.socket_file(99);
        assert_eq!(server.channel(), socket.to_string_lossy());
        let mode = std::fs::metadata(&socket).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        for id in [1, 2] {
            let answer = ask(
                server.channel(),
                &Request::new(id, Command::FensterLesen),
                Duration::from_secs(5),
            )
            .unwrap();
            assert_eq!(answer.id, id);
            match answer.antwort {
                Reply::Fenster(info) => assert_eq!(info.programm, format!("Anfrage {id}")),
                other => panic!("{other:?}"),
            }
        }
        drop(server);
        assert!(!socket.exists(), "Socket geht mit dem Fenster");
        let error = ask(
            &socket.to_string_lossy(),
            &Request::new(3, Command::FensterLesen),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("nicht erreichbar"), "{error}");
        std::fs::remove_dir_all(folder.path()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_window_that_does_not_answer_is_given_up() {
        let folder = BridgeFolder::at(temp("haengt"));
        let handler: Handler = Arc::new(|request: Request| {
            thread::sleep(Duration::from_millis(1500));
            Response::new(request.id, Reply::refused("zu spät"))
        });
        let server = BridgeServer::start(&folder, 98, handler).unwrap();
        let error = ask(
            server.channel(),
            &Request::new(1, Command::FensterLesen),
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(error.contains("nicht rechtzeitig geantwortet"), "{error}");
        drop(server);
        std::fs::remove_dir_all(folder.path()).unwrap();
    }
}
