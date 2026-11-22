//! Undo/redo as a stack of snapshots.

/// What the last change edited, so typing into one field makes one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditKey(pub String);

pub struct UndoStack<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
    last_key: Option<EditKey>,
    /// Counts every `record` call, also coalesced ones: "did anything change?"
    records: u64,
    /// Counts the steps really pushed by `record` (not coalesced ones, not
    /// undo/redo): the workspace orders steps of several LVs with it.
    pushes: u64,
    /// Steps taken back / forward (undo and redo that did something): the
    /// workspace keeps its project order in step with them.
    moves: (u64, u64),
}

impl<T: Clone> UndoStack<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            last_key: None,
            records: 0,
            pushes: 0,
            moves: (0, 0),
        }
    }

    /// Records the state *before* a change. Changes with the same key in a
    /// row (typing into one field) share one step.
    pub fn record(&mut self, before: &T, key: Option<EditKey>) {
        self.records += 1;
        if key.is_some() && key == self.last_key {
            return;
        }
        self.last_key = key;
        self.pushes += 1;
        self.undo.push(before.clone());
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Ends the current typing step: the next keyed change starts a new one.
    pub fn seal(&mut self) {
        self.last_key = None;
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let previous = self.undo.pop()?;
        self.redo.push(current.clone());
        self.last_key = None;
        self.moves.0 += 1;
        Some(previous)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current.clone());
        self.last_key = None;
        self.moves.1 += 1;
        Some(next)
    }

    /// The change recorded last was a text field taking back its own typing
    /// (Strg+Z in the field): it counts as an undo, not as a new edit –
    /// otherwise the next Strg+Z would take back the taking back, and Strg+Z
    /// would swing between two states for ever. `pushed`: that recording
    /// pushed a snapshot (else it joined the typing step); `before`: the
    /// state before the field's undo (for redo); `current`: the state now.
    pub fn field_undone(&mut self, pushed: bool, before: T, current: &T)
    where
        T: PartialEq,
    {
        if pushed {
            self.undo.pop();
            self.pushes = self.pushes.saturating_sub(1);
        }
        // The typing is gone completely: its snapshot is the state now.
        if self.undo.last() == Some(current) {
            self.undo.pop();
            self.moves.0 += 1;
        }
        self.redo.push(before);
        self.last_key = None;
    }

    /// How many undo and redo steps were taken so far.
    pub fn moves(&self) -> (u64, u64) {
        self.moves
    }

    pub fn push_count(&self) -> u64 {
        self.pushes
    }

    pub fn record_count(&self) -> u64 {
        self.records
    }

    /// The state before the newest recorded change.
    pub fn last(&self) -> Option<&T> {
        self.undo.last()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {

    /// Gefunden mit dem Affentest: Strg+Z im Textfeld pendelte zwischen zwei
    /// Zuständen, weil das Zurücknehmen im Feld als neue Änderung zählte.
    #[test]
    fn a_field_taking_back_its_typing_is_an_undo() {
        let mut stack: UndoStack<String> = UndoStack::new(10);
        stack.record(&"Haus".to_string(), Some(EditKey("kurz".into())));
        // The field undoes „Haus neu“ → „Haus“; the editor recorded it as a change.
        stack.record(&"Haus neu".to_string(), None);
        stack.field_undone(true, "Haus neu".to_string(), &"Haus".to_string());
        assert!(!stack.can_undo(), "Tippen ganz zurückgenommen");
        assert_eq!(stack.moves(), (1, 0));
        assert_eq!(
            stack.push_count(),
            1,
            "die Aufzeichnung des Zurücknehmens zählt nicht"
        );
        assert_eq!(stack.redo(&"Haus".to_string()).as_deref(), Some("Haus neu"));
        // Partly taken back: the typing step stays.
        let mut stack: UndoStack<String> = UndoStack::new(10);
        stack.record(&"a".to_string(), Some(EditKey("kurz".into())));
        stack.record(&"abc".to_string(), Some(EditKey("kurz".into())));
        stack.field_undone(false, "abc".to_string(), &"ab".to_string());
        assert!(stack.can_undo());
        assert_eq!(stack.undo(&"ab".to_string()).as_deref(), Some("a"));
    }

    use super::{EditKey, UndoStack};

    #[test]
    fn undo_and_redo_walk_the_snapshots() {
        let mut stack = UndoStack::new(100);
        let mut value = 1;
        stack.record(&value, None);
        value = 2;
        stack.record(&value, None);
        value = 3;
        value = stack.undo(&value).unwrap();
        assert_eq!(value, 2);
        value = stack.undo(&value).unwrap();
        assert_eq!(value, 1);
        assert!(stack.undo(&value).is_none());
        value = stack.redo(&value).unwrap();
        assert_eq!(value, 2);
        stack.record(&value, None);
        assert!(!stack.can_redo());
    }

    #[test]
    fn typing_in_one_field_is_one_step() {
        let mut stack = UndoStack::new(100);
        let key = || Some(EditKey("kurztext 7".into()));
        let mut text = String::new();
        for character in ["a", "b", "c"] {
            stack.record(&text, key());
            text.push_str(character);
        }
        assert_eq!(stack.undo(&text).unwrap(), "");
        let mut text = "abc".to_string();
        stack.record(&text, key());
        text.push('d');
        stack.seal();
        stack.record(&text, key());
        text.push('e');
        assert_eq!(stack.undo(&text).unwrap(), "abcd");
        assert_eq!(stack.record_count(), 5);
    }

    #[test]
    fn limit_drops_the_oldest() {
        let mut stack = UndoStack::new(2);
        for value in 0..5 {
            stack.record(&value, None);
        }
        assert_eq!(stack.undo(&9), Some(4));
        assert_eq!(stack.undo(&4), Some(3));
        assert_eq!(stack.undo(&3), None);
    }
}
