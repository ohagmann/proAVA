//! „proAVA wurde aktualisiert – bitte neu starten“: a proAVA whose program
//! file was replaced while it runs keeps the old code (18./19.11.2026: two
//! false error reports because the old program was still open). The status
//! bar says so for good and offers „Neu starten“ (save question as when
//! quitting, then the new program with the same project).
//!
//! Only the real program window watches its file (never a test – there a
//! switch stands in, [`pretend_updated`]); the file system is read once a
//! minute, not every frame.

use std::cell::Cell;
use std::path::Path;
use std::time::{Duration, SystemTime};

use eframe::egui;
use proava_application::program_update::{replaced, without_deleted_mark};

/// The line in the status bar.
pub const TEXT: &str = "proAVA wurde aktualisiert – bitte neu starten (Projekt vorher speichern)";
/// The button beside it.
pub const RESTART: &str = "Neu starten";
/// Hover text of the button.
pub const RESTART_HOVER: &str =
    "Fragt wie beim Beenden nach dem Speichern und startet dann die neue \
     Version mit demselben Projekt.";

/// Seconds between two looks at the program file.
const EVERY_SECONDS: f64 = 60.0;

thread_local! {
    /// Tests: as if the program file had been replaced.
    static PRETEND: Cell<bool> = const { Cell::new(false) };
}

/// Tests: behave as if the program had been updated (the UI test of the
/// notice; „Neu starten“ starts nothing there, see `jobs::restart`). Per
/// thread, so tests running side by side do not see each other's switch.
pub fn pretend_updated(on: bool) {
    PRETEND.with(|pretend| pretend.set(on));
}

/// Watches the program file of this window.
#[derive(Debug)]
pub struct UpdateWatch {
    /// Start of the program (an update is a newer program file).
    started: SystemTime,
    checked_at: Option<f64>,
    replaced: bool,
}

impl Default for UpdateWatch {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateWatch {
    /// Remembers the start of the program (create it at program start).
    pub fn new() -> Self {
        Self {
            started: SystemTime::now(),
            checked_at: None,
            replaced: false,
        }
    }

    /// Whether an update waits for a restart; looks at the file at most once
    /// a minute (and wakes the window for that while it waits).
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        if PRETEND.with(Cell::get) {
            return true;
        }
        if self.replaced || !crate::jobs::real_program() {
            return self.replaced;
        }
        let now = ctx.input(|input| input.time);
        if self
            .checked_at
            .is_none_or(|checked| now - checked >= EVERY_SECONDS)
        {
            self.checked_at = Some(now);
            self.replaced = program_file_replaced(self.started);
        }
        if !self.replaced {
            ctx.request_repaint_after(Duration::from_secs_f64(EVERY_SECONDS));
        }
        self.replaced
    }
}

/// Reads the program file (path of the running program and its time) and
/// applies the rules of `proava_application::program_update`.
fn program_file_replaced(started: SystemTime) -> bool {
    // From an AppImage: the image file (the program inside never changes).
    let Some(exe) = crate::jobs::appimage().or_else(|| std::env::current_exe().ok()) else {
        return false;
    };
    let modified = std::fs::metadata(without_deleted_mark(exe.clone()))
        .and_then(|metadata| metadata.modified())
        .ok();
    replaced(&exe, modified, started, Path::exists)
}
