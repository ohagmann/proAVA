//! The windows „Eigenschaften“ and „Langtext“ stay in front of the main window
//! because they belong to it (`owner_window`: X11 `WM_TRANSIENT_FOR`, Windows
//! owner) – the window manager does that, without rules of ours. Only while a
//! dialog is open in the main window they must step back, else they would
//! cover it: the relation goes (`owner_window`) and they go to the bottom
//! („always on bottom“); afterwards both come back and they are in front of
//! the main window again. The main window gets the focus when the dialog
//! opens (Muffin does not raise a main window that already has it, and the
//! dialog wants the keys).
//!
//! Until 19.11. the level followed the mouse and the focus („always on top“
//! while the mouse was over proAVA). Under Cinnamon that swung to and fro and
//! swallowed clicks on the cross; see `owner_window`.

use eframe::egui::{self, ViewportId, WindowLevel};

/// The level of the extra windows: at the bottom while a dialog is open in
/// the main window (with the setting on), else normal.
pub fn level(setting: bool, main_dialog: bool) -> WindowLevel {
    if setting && main_dialog {
        WindowLevel::AlwaysOnBottom
    } else {
        WindowLevel::Normal
    }
}

/// A modal dialog is open in the main window.
pub fn main_dialog(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.top_modal_layer().is_some())
}

/// Remembers whether a dialog was open, to bring the main window forward when
/// one opens.
#[derive(Debug, Default)]
pub struct InFront {
    dialog_before: bool,
}

impl InFront {
    /// The level for this frame; `windows` are the program's windows (main
    /// window and the extra windows).
    pub fn level(
        &mut self,
        ctx: &egui::Context,
        setting: bool,
        windows: &[ViewportId],
    ) -> WindowLevel {
        let dialog = main_dialog(ctx);
        if dialog && !self.dialog_before && setting {
            let focused = windows
                .iter()
                .any(|id| ctx.input_for(*id, |input| input.viewport().focused == Some(true)));
            if focused {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
        }
        self.dialog_before = dialog;
        // To the bottom only once the relation is gone (`owner_window`).
        level(setting, dialog && crate::owner_window::released(ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_the_bottom_only_while_a_dialog_is_open() {
        assert_eq!(
            level(true, false),
            WindowLevel::Normal,
            "vor dem Hauptfenster über die Zugehörigkeit"
        );
        assert_eq!(
            level(true, true),
            WindowLevel::AlwaysOnBottom,
            "Dialog vorn"
        );
        assert_eq!(
            level(false, true),
            WindowLevel::Normal,
            "Einstellung aus: wie früher"
        );
        assert_eq!(level(false, false), WindowLevel::Normal);
    }
}
