//! Keyboard for the lists of the Angebote tab (Bieterverzeichnis, Angebote,
//! Schriftverkehr im Projekt): after a click into the list, ↑/↓ move the
//! selection, Entf removes (with a question), Enter opens – as long as no
//! text field has the keyboard and no menu is open. A click somewhere else
//! ends it, so two lists on one page do not both react.

use eframe::egui;

/// What a key asks the list to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKey {
    Up,
    Down,
    Delete,
    Enter,
}

/// Whether the list has the keyboard.
#[derive(Debug, Default, Clone, Copy)]
pub struct ListFocus {
    active: bool,
    /// ↑/↓ moved the selection: bring the row into view next frame.
    scroll: bool,
}

impl ListFocus {
    /// A row was clicked: the list takes the keyboard.
    pub fn take(&mut self) {
        self.active = true;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Whether the selected row should be scrolled into view (once).
    pub fn take_scroll(&mut self) -> bool {
        std::mem::take(&mut self.scroll)
    }

    /// After drawing: a press outside `area` (the list with its tool row)
    /// gives the keyboard back.
    pub fn track(&mut self, ctx: &egui::Context, area: egui::Rect) {
        let outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !area.contains(pos))
        });
        if outside {
            self.active = false;
        }
    }

    /// The key for the list in this frame, consumed; none while a text field
    /// is focused, a menu is open, a dialog lies above the list (`ui` is the
    /// list's ui) or the list does not have the keyboard.
    pub fn key(&mut self, ui: &egui::Ui) -> Option<ListKey> {
        let ctx = ui.ctx();
        let covered = ctx
            .memory(|memory| memory.top_modal_layer())
            .is_some_and(|top| top != ui.layer_id());
        if !self.active || covered || ctx.text_edit_focused() || egui::Popup::is_any_open(ctx) {
            return None;
        }
        let key = ctx.input_mut(|input| {
            let none = egui::Modifiers::NONE;
            [
                (egui::Key::ArrowUp, ListKey::Up),
                (egui::Key::ArrowDown, ListKey::Down),
                (egui::Key::Delete, ListKey::Delete),
                (egui::Key::Enter, ListKey::Enter),
            ]
            .into_iter()
            .find(|(key, _)| input.consume_key(none, *key))
            .map(|(_, action)| action)
        });
        if key.is_some() {
            // The list reacts after it was drawn: show the result.
            ctx.request_repaint();
            self.scroll = matches!(key, Some(ListKey::Up | ListKey::Down));
        }
        key
    }
}

/// The neighbour of `current` in `ids` for ↑/↓ (the first one without a
/// selection).
pub fn step<T: Copy + PartialEq>(ids: &[T], current: Option<T>, down: bool) -> Option<T> {
    let index = current.and_then(|current| ids.iter().position(|id| *id == current));
    let target = match (index, down) {
        (None, _) => 0,
        (Some(index), true) => (index + 1).min(ids.len().checked_sub(1)?),
        (Some(index), false) => index.saturating_sub(1),
    };
    ids.get(target).copied()
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn arrows_walk_the_list_and_stop_at_the_ends() {
        let ids = [3, 5, 9];
        assert_eq!(step(&ids, None, true), Some(3));
        assert_eq!(step(&ids, Some(3), true), Some(5));
        assert_eq!(step(&ids, Some(9), true), Some(9));
        assert_eq!(step(&ids, Some(3), false), Some(3));
        assert_eq!(step(&ids, Some(9), false), Some(5));
        assert_eq!(step::<u8>(&[], None, true), None);
    }
}
