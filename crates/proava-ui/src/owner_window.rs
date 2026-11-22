//! The windows „Eigenschaften“ and „Langtext“ belong to the main window, as
//! the dialog of a program belongs to it: the window manager keeps them in
//! front of the main window – a click into the table does not hide them – but
//! not in front of other programs, and that without any rule of ours about
//! the mouse or the focus (Anwender 19.11.).
//!
//! Before, the windows went „always on top“ while the mouse was over
//! proAVA. Under Cinnamon that swung to and fro – the window manager reports
//! the resting mouse gone for a moment now and then, every change brought the
//! next one, and the mouse pointer changed its size (Anwender 19.11. 00:08) –
//! and a change while the cross was pressed swallowed the click (18.11.).
//!
//! egui creates the windows from its `ViewportBuilder`, which knows no owner;
//! so the relation is set once the window exists: X11 `WM_TRANSIENT_FOR`
//! (ICCCM; Muffin and Cinnamon keep a transient window above its main window,
//! it can still be minimised, maximised and closed), Windows the owner
//! (`GWLP_HWNDPARENT`). Every other top-level window of this process gets it –
//! there are no others than these two. Wayland: nothing (no window of another
//! client can be addressed; the windows stay normal windows).
//!
//! While a dialog is open in the main window, the relation goes and the
//! windows go to the bottom (`window_level`): else they would cover the dialog.
//! In this order – Cinnamon ignores „bottom“ for a transient window and does
//! not catch up when the relation goes afterwards (test screen 19.11.); so the
//! level waits for [`released`].

use eframe::egui;

/// How long to look for a new window: the window manager lists it only once
/// it is mapped (a few frames).
const LOOK_SECONDS: f64 = 1.5;

fn released_id() -> egui::Id {
    egui::Id::unique("owner_window_released")
}

/// The other windows do not belong to the main window (relation taken away
/// for a dialog, or never set): they may go to the bottom.
pub fn released(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(released_id()))
        .unwrap_or(true)
}

/// Keeps the relation of the program's other windows to the main window.
#[derive(Default)]
pub struct Owner {
    link: Option<platform::Link>,
    /// The platform has no link (Wayland, no X server): stop trying.
    unavailable: bool,
    /// Number of viewports last seen (a new window appears).
    viewports: usize,
    /// Relation wanted last frame.
    wanted: bool,
    /// Look again until this time (input time).
    look_until: f64,
    /// The relation is set (last successful look with `own`).
    related: bool,
}

impl Owner {
    /// Every frame of the main window: `main` is its handle, `own` whether
    /// the other windows belong to it (setting on, no dialog open).
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        main: Option<raw_window_handle::RawWindowHandle>,
        own: bool,
    ) {
        self.look(ctx, main, own);
        let released = !self.related;
        ctx.data_mut(|data| data.insert_temp(released_id(), released));
    }

    fn look(
        &mut self,
        ctx: &egui::Context,
        main: Option<raw_window_handle::RawWindowHandle>,
        own: bool,
    ) {
        if self.unavailable {
            self.related = false;
            return;
        }
        let (now, viewports) = ctx.input(|input| (input.time, input.raw.viewports.len()));
        if viewports != self.viewports || own != self.wanted {
            self.viewports = viewports;
            self.wanted = own;
            self.look_until = now + LOOK_SECONDS;
        }
        // Only the main window open, or nothing new: nothing to do.
        if viewports < 2 {
            self.related = false;
            return;
        }
        if now > self.look_until {
            return;
        }
        let Some(main) = main.and_then(platform::main_window) else {
            self.unavailable = true;
            self.related = false;
            return;
        };
        if self.link.is_none() {
            self.link = platform::Link::open();
            if self.link.is_none() {
                self.unavailable = true;
                self.related = false;
                return;
            }
        }
        if let Some(link) = &self.link {
            if link.apply(main, own).is_ok() {
                self.related = own;
            } else {
                // The connection broke: open a new one next time.
                self.link = None;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// What to do with the other windows of this process: each window and its
/// current owner (`None` = none); returns the windows to change and their new
/// owner. Only windows without an owner get the main window, only those of
/// the main window lose it – a window with another owner stays as it is.
pub fn plan(main: u64, windows: &[(u64, Option<u64>)], own: bool) -> Vec<(u64, Option<u64>)> {
    windows
        .iter()
        .filter(|(window, _)| *window != main)
        .filter_map(|&(window, owner)| match (own, owner) {
            (true, None) => Some((window, Some(main))),
            (false, Some(current)) if current == main => Some((window, None)),
            _ => None,
        })
        .collect()
}

#[cfg(target_os = "linux")]
mod platform {
    use raw_window_handle::RawWindowHandle;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    // `c_ulong` is u64 here, u32 on 32-bit Linux: the cast is for those.
    #[allow(clippy::unnecessary_cast)]
    pub fn main_window(handle: RawWindowHandle) -> Option<u64> {
        match handle {
            RawWindowHandle::Xlib(handle) => Some(handle.window as u64),
            RawWindowHandle::Xcb(handle) => Some(u64::from(handle.window.get())),
            _ => None,
        }
    }

    /// An own connection to the X server (properties belong to the server,
    /// any client may set them on any window).
    pub struct Link {
        connection: RustConnection,
        root: u32,
        client_list: u32,
        pid: u32,
    }

    type Failed = Box<dyn std::error::Error>;

    impl Link {
        pub fn open() -> Option<Self> {
            std::env::var_os("DISPLAY")?;
            let (connection, screen) = x11rb::connect(None).ok()?;
            let root = connection.setup().roots.get(screen)?.root;
            let atom = |name: &[u8]| -> Option<u32> {
                Some(connection.intern_atom(false, name).ok()?.reply().ok()?.atom)
            };
            let client_list = atom(b"_NET_CLIENT_LIST")?;
            let pid = atom(b"_NET_WM_PID")?;
            Some(Self {
                connection,
                root,
                client_list,
                pid,
            })
        }

        /// Sets or removes `WM_TRANSIENT_FOR` on the other windows of this
        /// process (as the window manager lists them).
        pub fn apply(&self, main: u64, own: bool) -> Result<usize, Failed> {
            let connection = &self.connection;
            let list = connection
                .get_property(
                    false,
                    self.root,
                    self.client_list,
                    AtomEnum::WINDOW,
                    0,
                    4096,
                )?
                .reply()?;
            let me = std::process::id();
            let first = |reply: Option<x11rb::protocol::xproto::GetPropertyReply>| {
                reply.and_then(|reply| reply.value32().and_then(|mut values| values.next()))
            };
            // All questions first, then the answers (one round trip); a
            // window of another program may be gone meanwhile – skip it.
            let others: Vec<u32> = list
                .value32()
                .into_iter()
                .flatten()
                .filter(|window| u64::from(*window) != main)
                .collect();
            let mut asked = Vec::new();
            for window in &others {
                let pid =
                    connection.get_property(false, *window, self.pid, AtomEnum::CARDINAL, 0, 1)?;
                let owner = connection.get_property(
                    false,
                    *window,
                    AtomEnum::WM_TRANSIENT_FOR,
                    AtomEnum::WINDOW,
                    0,
                    1,
                )?;
                asked.push((*window, pid, owner));
            }
            let mut windows = Vec::new();
            for (window, pid, owner) in asked {
                let pid = first(pid.reply().ok());
                let owner = first(owner.reply().ok());
                if pid == Some(me) {
                    windows.push((
                        u64::from(window),
                        owner.filter(|owner| *owner != 0).map(u64::from),
                    ));
                }
            }
            let changes = super::plan(main, &windows, own);
            for (window, owner) in &changes {
                let window = *window as u32;
                match owner {
                    Some(owner) => {
                        connection.change_property32(
                            PropMode::REPLACE,
                            window,
                            AtomEnum::WM_TRANSIENT_FOR,
                            AtomEnum::WINDOW,
                            &[*owner as u32],
                        )?;
                    }
                    None => {
                        connection
                            .delete_property(window, u32::from(AtomEnum::WM_TRANSIENT_FOR))?;
                    }
                }
            }
            connection.flush()?;
            Ok(changes.len())
        }
    }
}

#[cfg(windows)]
mod platform {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindowVisible,
        SetWindowLongPtrW, GWLP_HWNDPARENT,
    };

    pub fn main_window(handle: RawWindowHandle) -> Option<u64> {
        match handle {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as u64),
            _ => None,
        }
    }

    /// Nothing to open on Windows.
    pub struct Link;

    unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
        let list = unsafe { &mut *(list as *mut Vec<HWND>) };
        list.push(window);
        1
    }

    impl Link {
        pub fn open() -> Option<Self> {
            Some(Self)
        }

        /// Sets or removes the owner (`GWLP_HWNDPARENT`) of the other
        /// top-level windows of this process.
        pub fn apply(&self, main: u64, own: bool) -> Result<usize, String> {
            let mut all: Vec<HWND> = Vec::new();
            // SAFETY: `collect` only pushes into the vector passed as LPARAM,
            // which lives for the whole call.
            unsafe { EnumWindows(Some(collect), &mut all as *mut Vec<HWND> as LPARAM) };
            let me = std::process::id();
            let mut windows = Vec::new();
            for window in all {
                let mut pid = 0u32;
                // SAFETY: plain queries of a window handle from EnumWindows.
                unsafe { GetWindowThreadProcessId(window, &mut pid) };
                // Hidden windows of the process (input method) stay as they are.
                if pid != me || unsafe { IsWindowVisible(window) } == 0 {
                    continue;
                }
                // SAFETY: as above.
                let owner = unsafe { GetWindowLongPtrW(window, GWLP_HWNDPARENT) };
                windows.push((window as u64, (owner != 0).then_some(owner as u64)));
            }
            let changes = super::plan(main, &windows, own);
            for (window, owner) in &changes {
                // SAFETY: the handle came from EnumWindows for this process;
                // GWLP_HWNDPARENT on a top-level window sets its owner.
                unsafe {
                    SetWindowLongPtrW(
                        *window as HWND,
                        GWLP_HWNDPARENT,
                        owner.unwrap_or(0) as isize,
                    )
                };
            }
            Ok(changes.len())
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use raw_window_handle::RawWindowHandle;

    pub fn main_window(_handle: RawWindowHandle) -> Option<u64> {
        None
    }

    pub struct Link;

    impl Link {
        pub fn open() -> Option<Self> {
            None
        }

        pub fn apply(&self, _main: u64, _own: bool) -> Result<usize, String> {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plan;

    #[test]
    fn the_other_windows_get_the_main_window_as_owner() {
        // Main window 1, „Eigenschaften“ 2 (no owner yet), „Langtext“ 3
        // (already owned).
        let windows = [(1, None), (2, None), (3, Some(1))];
        assert_eq!(plan(1, &windows, true), vec![(2, Some(1))]);
        // Nothing left to do the next time.
        let windows = [(1, None), (2, Some(1)), (3, Some(1))];
        assert!(plan(1, &windows, true).is_empty());
    }

    #[test]
    fn a_dialog_in_the_main_window_takes_the_relation_away() {
        let windows = [(1, None), (2, Some(1)), (3, Some(1))];
        assert_eq!(plan(1, &windows, false), vec![(2, None), (3, None)]);
        assert!(plan(1, &[(2, None)], false).is_empty(), "schon frei");
    }

    #[test]
    fn a_window_with_another_owner_stays_as_it_is() {
        // E.g. a dialog of a toolkit that belongs to window 2.
        let windows = [(1, None), (2, Some(1)), (4, Some(2))];
        assert!(plan(1, &windows, true).is_empty());
        assert_eq!(plan(1, &windows, false), vec![(2, None)]);
    }

    #[test]
    fn the_main_window_never_gets_an_owner() {
        assert!(plan(1, &[(1, None)], true).is_empty());
    }
}
