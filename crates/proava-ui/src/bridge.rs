//! The window side of the local bridge (see `proava_bridge_protocol`):
//! the channel of this window, its entry in the folder of the windows, and
//! what the window shows of the assistant – the KI-Markierung (a frame of its
//! own colour around rows), the line „KI: …“ and „KI verbunden“ in the
//! status bar. Requests are handled on the UI thread, through the same
//! editing as by hand (selection, Strg+V, undo).
//!
//! Only the real program opens a channel (`run_with_bridge`); tests hand in
//! one in a temporary folder ([`crate::App::with_bridge`]).

use std::collections::HashSet;
use std::sync::mpsc::Receiver;

use eframe::egui;
use proava_application::window_bridge::{Incoming, WindowChannel};
use proava_bridge_protocol::{Reply, WindowEntry, WindowInfo, VERSION};
use proava_domain::RowId;

/// „KI verbunden“ stays this long after the last request (seconds).
pub const CONNECTED_SECONDS: f64 = 180.0;
/// After Esc the assistant is refused this long (seconds) – it has to ask.
pub const STOP_SECONDS: f64 = 15.0;
/// How often the entry in the folder is compared with the window (seconds).
const PUBLISH_EVERY: f64 = 0.5;

/// What the assistant does in a workspace right now.
#[derive(Default)]
pub struct AiActivity {
    /// Line in the status bar after „KI: “.
    pub text: Option<String>,
    /// Rows with the KI-Markierung.
    pub rows: HashSet<RowId>,
    /// Until when (egui seconds) mark and line stay.
    pub until: f64,
    /// Time of the last request (egui seconds) for „KI verbunden“.
    pub last_request: Option<f64>,
    /// Esc pressed: requests are refused until then.
    pub stopped_until: f64,
    /// An insert whose answer waits for the end of this frame or for the user.
    pub pending: Option<PendingInsert>,
    /// The state last handed to the MCP server (`stand`): its number and
    /// the project then – a change made on it is taken over only while the
    /// project in the window is still the same.
    pub lent: Option<(u64, proava_domain::Project)>,
    /// Numbers of the states handed out and of the changes taken over.
    pub next_number: u64,
    /// The mark shows a change taken over (its undo step): it goes when
    /// the step is taken back.
    pub step: Option<u64>,
}

/// An insert that is done; the answer waits until no question of proAVA
/// is open any more (or the user takes too long).
pub struct PendingInsert {
    pub incoming: Incoming,
    /// The inserted rows.
    pub rows: Vec<RowId>,
    /// A change taken over from an MCP tool: the number of its undo step
    /// (it counts as kept while the step is done, also without new rows).
    pub step: Option<u64>,
    pub text: String,
    /// egui time of the insert.
    pub since: f64,
}

impl AiActivity {
    /// Shows `text` and marks `rows` for `seconds` from `now`.
    pub fn show(&mut self, now: f64, text: impl Into<String>, rows: HashSet<RowId>, seconds: f64) {
        self.text = Some(text.into());
        self.rows = rows;
        self.until = now + seconds;
        self.step = None;
    }

    /// Takes mark and line away.
    pub fn clear(&mut self) {
        self.text = None;
        self.rows.clear();
        self.until = 0.0;
        self.step = None;
    }

    /// Whether mark or line are shown.
    pub fn visible(&self) -> bool {
        self.text.is_some() || !self.rows.is_empty()
    }

    /// Whether an assistant asked in the last minutes.
    pub fn connected(&self, now: f64) -> bool {
        self.last_request
            .is_some_and(|time| now - time < CONNECTED_SECONDS)
    }

    /// Esc: mark and line go; a waiting insert is answered; for a while the
    /// assistant is refused (it has to ask the user).
    pub fn stop(&mut self, now: f64) {
        self.clear();
        self.stopped_until = now + STOP_SECONDS;
        if let Some(pending) = self.pending.take() {
            pending.incoming.answer(Reply::refused(format!(
                "{} Der Anwender hat danach mit Esc abgebrochen – bitte nachfragen, bevor du weitermachst \
                 (Strg+Z im Fenster nimmt das Einfügen zurück).",
                pending.text
            )));
        }
    }

    /// The refusal while Esc is in force.
    pub fn stopped(&self, now: f64) -> Option<Reply> {
        (now < self.stopped_until).then(|| {
            Reply::refused(
                "Der Anwender hat die KI-Aktion in diesem Fenster mit Esc abgebrochen – bitte nachfragen, \
                 bevor du weitermachst.",
            )
        })
    }
}

/// Channel, folder entry and focus times of this window.
pub(crate) struct HostBridge {
    channel: Box<dyn WindowChannel>,
    inbox: Receiver<Incoming>,
    pid: u32,
    started_ms: u64,
    /// The entry as last written (without its time).
    last_entry: String,
    next_check: f64,
    focused: Option<bool>,
    focus_ms: u64,
    /// For the workspace: other windows and the way to them (dragging rows).
    handle: proava_application::window_bridge::BridgeHandle,
}

impl HostBridge {
    pub(crate) fn open(
        mut channel: Box<dyn WindowChannel>,
        pid: u32,
        wake: proava_application::window_bridge::Wake,
    ) -> Result<Self, String> {
        let inbox = channel.open(pid, wake)?;
        let handle = proava_application::window_bridge::BridgeHandle {
            pid,
            windows: channel.window_list(),
            messenger: channel.messenger(),
            exchange: channel.exchange_folder(),
        };
        Ok(Self {
            handle,
            channel,
            inbox,
            pid,
            started_ms: now_ms(),
            last_entry: String::new(),
            next_check: 0.0,
            focused: None,
            focus_ms: 0,
        })
    }

    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }

    /// Other windows and the way to them (for the workspace).
    pub(crate) fn handle(&self) -> proava_application::window_bridge::BridgeHandle {
        self.handle.clone()
    }

    /// Requests that came in since the last frame.
    pub(crate) fn take_requests(&self) -> Vec<Incoming> {
        self.inbox.try_iter().collect()
    }

    /// When the window last had the focus (gained or lost it).
    pub(crate) fn focus_ms(&self) -> u64 {
        self.focus_ms
    }

    /// Notes focus changes (for „aktiv“ = the window used last).
    pub(crate) fn note_focus(&mut self, focused: bool) {
        if self.focused != Some(focused) {
            if self.focused.is_some() || focused {
                self.focus_ms = now_ms();
            }
            self.focused = Some(focused);
        }
    }

    /// Writes the entry when something changed (checked twice a second).
    pub(crate) fn publish_if_changed(
        &mut self,
        ctx: &egui::Context,
        info: impl FnOnce() -> WindowInfo,
    ) {
        let now = ctx.input(|input| input.time);
        if now < self.next_check {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(PUBLISH_EVERY));
            return;
        }
        self.next_check = now + PUBLISH_EVERY;
        let mut entry = WindowEntry {
            v: VERSION,
            pid: self.pid,
            kanal: self.channel.address(),
            gestartet_ms: self.started_ms,
            stand_ms: 0,
            fenster: info(),
        };
        let text = proava_bridge_protocol::encode(&entry);
        if text != self.last_entry {
            self.last_entry = text;
            entry.stand_ms = now_ms();
            self.channel.publish(&entry);
        }
    }

    pub(crate) fn close(&mut self) {
        self.channel.close();
    }
}

impl Drop for HostBridge {
    fn drop(&mut self) {
        self.channel.close();
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis() as u64)
        .unwrap_or(0)
}

/// The answer while assistants may not work in this window.
pub(crate) fn switched_off() -> Reply {
    Reply::Aus {
        text: "In diesem proAVA-Fenster ist die Bedienung durch KI-Assistenten ausgeschaltet \
               (Extras › Einstellungen › Darstellung › KI-Assistenten). Die Datei-Werkzeuge \
               gehen weiter, solange das Projekt dort gespeichert ist."
            .to_string(),
    }
}
