//! `ausschreiben_oeffnen`: opens a page of AUSSCHREIBEN.DE in proAVA's
//! browser window (`proava-web`) – for the user, who exports there himself.
//! The assistant never steers the window (that would be automated use of the
//! site, robots.txt): it only names the page. The window runs on its own
//! (`--allein`), keeps running when the server ends and saves exports in the
//! user's Downloads folder; `stlb_uebernehmen` takes them into the LV.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use proava_web_protocol::{
    decode, encode, helper_program, is_helper_program, FromHelper, Launch, Site, ToHelper,
};

/// Set (any value) in tests and wherever the server must not start other
/// programs: no browser window then.
pub const NO_PROGRAMS: &str = "PROAVA_KEINE_PROGRAMME";

/// Opens pages in proAVA's browser window (the real one in `main.rs`; tests
/// hand in their own and never start anything).
pub trait BrowserWindow {
    /// Opens `url` (a checked page of AUSSCHREIBEN.DE); the answer says
    /// where the exports land and what was saved so far.
    fn open(&mut self, url: &str) -> Result<String, String>;
}

/// The window as a program next to `proava-mcp`, one at a time per server.
pub struct HelperWindow {
    downloads: PathBuf,
    child: Option<Child>,
    input: Option<ChildStdin>,
    saved: Arc<Mutex<Vec<PathBuf>>>,
}

impl HelperWindow {
    /// Exports go to `downloads` (the user's Downloads folder).
    pub fn new(downloads: PathBuf) -> Self {
        Self {
            downloads,
            child: None,
            input: None,
            saved: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn running(&mut self) -> bool {
        let alive = self
            .child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if !alive {
            self.child = None;
            self.input = None;
        }
        alive
    }

    fn send(&mut self, message: &ToHelper) -> bool {
        self.input.as_mut().is_some_and(|input| {
            writeln!(input, "{}", encode(message)).is_ok() && input.flush().is_ok()
        })
    }

    fn answer(&self, url: &str, how: &str) -> String {
        let mut answer = format!(
            "{how} für den Anwender im Browserfenster von proAVA: {url}\nDer Anwender exportiert dort selbst (Export ▸ GAEB XML, mehrere Texte über die Merkliste); das Fenster wird nicht von der KI bedient.\nExporte landen im Download-Ordner {}. Einfügen danach mit stlb_uebernehmen (datei, quelle „AUSSCHREIBEN.DE“).",
            self.downloads.display()
        );
        let saved = self
            .saved
            .lock()
            .map(|list| list.clone())
            .unwrap_or_default();
        if !saved.is_empty() {
            answer.push_str("\nBisher aus diesem Fenster gespeichert:");
            for file in saved {
                answer.push_str(&format!("\n- {}", file.display()));
            }
        }
        answer
    }
}

impl BrowserWindow for HelperWindow {
    fn open(&mut self, url: &str) -> Result<String, String> {
        if std::env::var_os(NO_PROGRAMS).is_some() {
            return Err(format!(
                "Browserfenster abgeschaltet ({NO_PROGRAMS} ist gesetzt)."
            ));
        }
        if self.running() {
            let sent = self.send(&ToHelper::Open {
                url: url.to_string(),
            }) && self.send(&ToHelper::Raise);
            if sent {
                return Ok(self.answer(url, "Im offenen Fenster geöffnet"));
            }
        }
        let program = std::env::current_exe()
            .map_err(|error| format!("Programmdatei nicht gefunden: {error}"))?;
        let helper = helper_program(&program);
        if !helper.is_file() || !is_helper_program(&helper) {
            return Err(format!(
                "Das Browserfenster von proAVA ist nicht installiert ({} fehlt). Der Anwender kann die Seite im System-Browser öffnen: {url}",
                helper.display()
            ));
        }
        let launch = Launch {
            url: Some(url.to_string()),
            downloads: Some(self.downloads.clone()),
            standalone: true,
            ..Launch::new(Site::Ausschreiben)
        };
        let mut child = Command::new(&helper)
            .args(launch.arguments())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Das Browserfenster lässt sich nicht starten: {error}"))?;
        let (first, first_message) = mpsc::channel();
        if let Some(output) = child.stdout.take() {
            let saved = Arc::clone(&self.saved);
            std::thread::spawn(move || {
                for line in BufReader::new(output).lines().map_while(Result::ok) {
                    match decode::<FromHelper>(&line) {
                        Ok(FromHelper::Downloaded { file, .. }) => {
                            if let Ok(mut list) = saved.lock() {
                                list.push(file);
                            }
                        }
                        Ok(message @ (FromHelper::Ready | FromHelper::Error { .. })) => {
                            let _ = first.send(message);
                        }
                        _ => {}
                    }
                }
            });
        }
        self.input = child.stdin.take();
        self.child = Some(child);
        // A second window for the same profile (proAVA has one open) ends
        // at once with a message; wait briefly for it.
        match first_message.recv_timeout(Duration::from_secs(8)) {
            Ok(FromHelper::Error { message }) => {
                self.child = None;
                self.input = None;
                Err(message)
            }
            _ => Ok(self.answer(url, "Geöffnet")),
        }
    }
}
