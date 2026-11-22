//! The system clipboard with HTML besides plain text (arboard), one per
//! program run – it has to stay alive, otherwise X11 forgets what we copied.
//! Without a clipboard (tests, no display) everything answers „nothing“.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
    /// Stand-in while the system clipboard is off (tests): HTML and text of
    /// this thread only – two `App`s in one test share it like two windows.
    static STAND_IN: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Off until the program window starts: automatic tests must never touch
/// the clipboard of the person working at the computer.
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// The program runs with a window (not in a test).
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn with<R>(action: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    CLIPBOARD.with(|cell| {
        let mut clipboard = cell.borrow_mut();
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        clipboard.as_mut().map(action)
    })
}

/// HTML (Excel/Word, other proAVA windows) and the plain text alternative.
pub fn set_html(html: &str, plain: &str) -> bool {
    if !enabled() {
        STAND_IN.with(|cell| *cell.borrow_mut() = Some((html.to_string(), plain.to_string())));
        return true;
    }
    with(|clipboard| clipboard.set_html(html, Some(plain)).is_ok()).unwrap_or(false)
}

/// The HTML of the clipboard, if any.
pub fn html() -> Option<String> {
    if !enabled() {
        return STAND_IN.with(|cell| cell.borrow().as_ref().map(|(html, _)| html.clone()));
    }
    with(|clipboard| clipboard.get().html().ok()).flatten()
}

/// The plain text of the clipboard, if any.
pub fn text() -> Option<String> {
    if !enabled() {
        return STAND_IN.with(|cell| cell.borrow().as_ref().map(|(_, plain)| plain.clone()));
    }
    with(|clipboard| clipboard.get_text().ok()).flatten()
}
