//! Tastenkürzel-Übersicht, generated from the definitions – never kept twice:
//!
//! - LV table: every key × modifier combination is put through
//!   [`command_for_key`] (what the table really does), the preferred text
//!   comes from [`Command::shortcut`].
//! - Structure commands (also while typing): [`StructCommand::for_key`].
//! - Whole window: [`ShellAction::shortcut`] of the command palette's actions.
//! - Text editors: the three format keys (bold/italic/underline) of
//!   `rich_editor` and `long_text_editor`, listed in [`TEXT_KEYS`].
//!
//! The tests check that every displayed shortcut is wired and that no two
//! commands share a key – a check for everybody who adds commands.

use std::collections::BTreeMap;

use eframe::egui::{Key, Modifiers};

use crate::lv_editor::StructCommand;
use crate::lv_ops::{command_for_key, Command};
use crate::workspace::{palette_actions, ShellAction};

/// Where a shortcut works (also the filter of the help window and F1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    /// Whole window, every view.
    General,
    /// New rows and the take-over dialogs: everywhere in the LV view (table,
    /// a cell being edited, Eigenschaftenfenster, windows Eigenschaften and
    /// Langtext) – mostly function keys (Anwender 18.11.).
    NewRows,
    /// LV table (keyboard in the table, no text field focused).
    Lv,
    /// Structure of the LV – also works while typing in a field.
    Structure,
    /// While a table cell is being edited (Kurztext, Menge, Einheit, EP …).
    Cell,
    /// While writing a long text, hint or Ausführungsbeschreibung.
    Text,
    /// Position table of an invoice (Reiter Abrechnung).
    Billing,
    /// Anlagenliste of a Kostenermittlung (Reiter Kosten).
    Costs,
    /// The file dialog (Öffnen, Speichern, Ordner wählen) – a window of its own.
    FileDialog,
}

impl Area {
    pub const ALL: [Area; 9] = [
        Area::General,
        Area::NewRows,
        Area::Lv,
        Area::Structure,
        Area::Cell,
        Area::Text,
        Area::Billing,
        Area::Costs,
        Area::FileDialog,
    ];

    /// Areas whose keys work at the same time with this one (one keyboard context).
    pub fn context(self) -> &'static [Area] {
        match self {
            Area::General | Area::Lv => &[Area::General, Area::NewRows, Area::Lv, Area::Structure],
            // Table, cell and text editor each check their keys against these.
            Area::NewRows => &[Area::General, Area::NewRows, Area::Structure],
            Area::Structure => &[Area::General, Area::NewRows, Area::Structure],
            Area::Cell => &[Area::General, Area::NewRows, Area::Structure, Area::Cell],
            Area::Text => &[Area::General, Area::NewRows, Area::Structure, Area::Text],
            Area::Billing => &[Area::General, Area::Billing],
            Area::Costs => &[Area::General, Area::Costs],
            // A modal window: only its own keys.
            Area::FileDialog => &[Area::FileDialog],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Area::General => "Überall",
            Area::NewRows => "Neue Zeilen (überall in der LV-Ansicht)",
            Area::Lv => "LV-Tabelle",
            Area::Structure => "LV-Gliederung (auch beim Schreiben)",
            Area::Cell => "Beim Bearbeiten einer Zelle",
            Area::Text => "Texteditor (Langtext, Hinweis)",
            Area::Billing => "Abrechnung (Positionen einer Rechnung)",
            Area::Costs => "Kosten (Anlagenliste beim Bearbeiten einer Zelle)",
            Area::FileDialog => "Dateidialog (Öffnen, Speichern, Ordner wählen)",
        }
    }
}

/// One line of the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub area: Area,
    pub action: String,
    /// Preferred key first, further keys after it.
    pub keys: Vec<String>,
}

/// Format keys of the text editors (bold, italic, underline).
pub const TEXT_KEYS: [(Key, &str); 3] = [
    (Key::B, "Fett"),
    (Key::I, "Kursiv"),
    (Key::U, "Unterstrichen"),
];

/// Field keys of the long-text editors (`rich_editor`, Strg+Umschalt):
/// Bieterangabe and Angabe des Auftraggebers at the caret.
pub const TEXT_FIELD_KEYS: [(Key, &str); 2] = [
    (Key::B, "Bieterangabe einfügen … (an der Schreibmarke)"),
    (Key::G, "Angabe des Auftraggebers einfügen …"),
];

/// Further keys handled by the window itself (not in the palette).
const GENERAL_EXTRA: [(&str, &str); 3] = [
    ("Befehl suchen", "Strg+K"),
    ("Hilfe zur aktuellen Ansicht", "F1"),
    ("Wiederholen", "Strg+Umschalt+Z"),
];

/// Further keys of the LV table handled outside [`command_for_key`].
const LV_EXTRA: [(&str, &str); 1] = [("Kontextmenü der Zeile (wie Rechtsklick)", "Umschalt+F10")];

/// The new rows in the order of their function keys (F5/F6 texts, F9–F12
/// rows); listed once under [`Area::NewRows`], not again in the table.
pub const NEW_ROWS: [crate::lv_editor::MenuChoice; 9] = {
    use crate::lv_editor::MenuChoice::{Editor, Struct};
    [
        Editor(Command::FromStlb),
        Editor(Command::FromAusschreiben),
        Editor(Command::NewPosition),
        Editor(Command::NewPositionBefore),
        Editor(Command::NewNote),
        Struct(StructCommand::NewNoteBefore),
        Struct(StructCommand::NewPerformance),
        Editor(Command::NewTitle),
        Struct(StructCommand::NewSubtitle),
    ]
};

/// Commands of the LV table that work in the whole window: listed once,
/// under „Überall“.
const ALSO_GENERAL: [Command; 3] = [Command::Palette, Command::Undo, Command::Redo];

/// Cell keys of the LV table (`lv_editor/table_cells.rs`, `cell_keys`): they run
/// before [`command_for_key`] and take these keys over.
pub const TABLE_CELL_KEYS: [(&str, &[&str]); 3] = [
    (
        "Aktive Zelle bearbeiten (oder einfach lostippen)",
        &["F2", "Enter"],
    ),
    ("Zelle nach links, auf Titeln: zuklappen", &["←"]),
    ("Zelle nach rechts, auf Titeln: aufklappen", &["→"]),
];

/// Keys of the position table in the Reiter Abrechnung (`billing_view/table.rs`).
pub const BILLING_KEYS: [(&str, &[&str]); 5] = [
    (
        "Position wählen; im Feld: übernehmen, gleiche Spalte darunter/darüber",
        &["↓", "↑"],
    ),
    ("15 Positionen weiter/zurück", &["Bild↓", "Bild↑"]),
    ("Erste/letzte Position", &["Pos1", "Ende"]),
    (
        "„geprüft“ bearbeiten (Tab: nächstes Feld, Enter: übernehmen)",
        &["Enter", "F2"],
    ),
    ("Aufmaß der Position", &["Strg+M"]),
];

/// Keys of the file dialog (`file_dialog.rs`): like a file manager.
pub const FILE_DIALOG_KEYS: [(&str, &[&str]); 4] = [
    ("Ordner oder Datei wählen", &["↓", "↑"]),
    ("Gewählten Ordner öffnen, Datei übernehmen", &["Enter"]),
    ("Eine Ebene höher", &["Rücktaste", "Alt+↑"]),
    (
        "Neuer Ordner (Speichern, Ordner wählen)",
        &[crate::file_dialog::NEW_FOLDER_KEY],
    ),
];

/// Keys of the Anlagenliste in the Reiter Kosten (`costs_view/table.rs`).
pub const COST_KEYS: [(&str, &[&str]); 6] = [
    ("Übernehmen, nächste Zelle", &["Tab"]),
    ("Übernehmen, vorige Zelle", &["Umschalt+Tab"]),
    ("Übernehmen, Zelle darunter", &["Enter"]),
    ("Übernehmen, Zelle darüber", &["Umschalt+Enter"]),
    ("Übernehmen und neue Zeile darunter", &["Strg+Enter"]),
    ("Bearbeiten abbrechen", &["Esc"]),
];

/// Keys while a cell is edited (`lv_editor/table_cells.rs`, `cell_keys`).
/// (Strg+Enter, F9 … take the value over and make the new row: listed under
/// [`Area::NewRows`].)
pub const CELL_EDIT_KEYS: [(&str, &str); 6] = [
    ("Übernehmen, nächste Zelle", "Tab"),
    ("Übernehmen, vorige Zelle", "Umschalt+Tab"),
    ("Übernehmen, Zeile darunter", "Enter"),
    ("Bearbeiten abbrechen", "Esc"),
    ("KG/LB: Eintrag der Liste wählen", "↓"),
    ("KG/LB: Eintrag der Liste wählen", "↑"),
];

/// Modifier combinations the overview looks at.
const MODIFIERS: [Modifiers; 6] = [
    Modifiers::NONE,
    Modifiers::SHIFT,
    Modifiers::COMMAND,
    Modifiers {
        shift: true,
        ..Modifiers::COMMAND
    },
    Modifiers::ALT,
    Modifiers {
        shift: true,
        ..Modifiers::ALT
    },
];

/// German key text as in the menus: „Strg+Umschalt+Enter“, „Alt+↑“, „Entf“.
pub fn key_text(key: Key, modifiers: Modifiers) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if modifiers.command || modifiers.ctrl {
        parts.push("Strg");
    }
    if modifiers.shift {
        parts.push("Umschalt");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    let name = match key {
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Delete => "Entf",
        Key::Insert => "Einfg",
        Key::Home => "Pos1",
        Key::End => "Ende",
        Key::PageUp => "Bild↑",
        Key::PageDown => "Bild↓",
        Key::Escape => "Esc",
        Key::Backspace => "Rücktaste",
        Key::Space => "Leertaste",
        other => other.name(),
    };
    parts.push(name);
    parts.join("+")
}

/// All key combinations of the overview's modifier set, as text → (key, modifiers).
fn combinations() -> Vec<(Key, Modifiers, String)> {
    let mut out = Vec::new();
    for &key in Key::ALL {
        for modifiers in MODIFIERS {
            out.push((key, modifiers, key_text(key, modifiers)));
        }
    }
    out
}

/// Keys that really trigger `command` in the LV table, preferred one first.
/// Variants that only add a modifier to another key of the same command
/// (tolerance like Umschalt+Alt+F2 next to Umschalt+F2) are left out.
fn table_keys(command: Command, combos: &[(Key, Modifiers, String)]) -> Vec<String> {
    let hits: Vec<&(Key, Modifiers, String)> = combos
        .iter()
        .filter(|(key, modifiers, _)| command_for_key(*key, *modifiers) == Some(command))
        .collect();
    let covers = |inner: Modifiers, outer: Modifiers| {
        inner != outer
            && (!inner.command || outer.command)
            && (!inner.shift || outer.shift)
            && (!inner.alt || outer.alt)
    };
    let mut keys: Vec<String> = hits
        .iter()
        .filter(|(key, modifiers, _)| {
            !hits
                .iter()
                .any(|(other, inner, _)| other == key && covers(*inner, *modifiers))
        })
        .map(|(_, _, text)| text.clone())
        .collect();
    let preferred = command.shortcut();
    // Copy, cut and paste come as clipboard events, not as keys.
    if !preferred.is_empty() && !keys.iter().any(|key| key == preferred) {
        keys.insert(0, preferred.to_string());
    } else if let Some(at) = keys.iter().position(|key| key == preferred) {
        let key = keys.remove(at);
        keys.insert(0, key);
    }
    // Keys the cell handling takes first are listed there.
    keys.retain(|key| {
        !TABLE_CELL_KEYS
            .iter()
            .any(|(_, cell_keys)| cell_keys.contains(&key.as_str()))
    });
    keys
}

/// The whole overview in a fixed order: Überall, LV-Tabelle, Gliederung, Texteditor.
pub fn all() -> Vec<Shortcut> {
    let combos = combinations();
    let mut out: Vec<Shortcut> = Vec::new();
    let mut push = |area: Area, action: String, keys: Vec<String>| {
        if keys.is_empty() {
            return;
        }
        if let Some(entry) = out
            .iter_mut()
            .find(|entry| entry.area == area && entry.action == action)
        {
            for key in keys {
                if !entry.keys.contains(&key) {
                    entry.keys.push(key);
                }
            }
        } else {
            out.push(Shortcut { area, action, keys });
        }
    };

    for action in general_actions() {
        push(
            Area::General,
            action.label(),
            vec![action.shortcut().to_string()],
        );
    }
    for (action, key) in GENERAL_EXTRA {
        push(Area::General, action.to_string(), vec![key.to_string()]);
    }

    let new_row_editors: Vec<Command> = NEW_ROWS
        .iter()
        .filter_map(|choice| match choice {
            crate::lv_editor::MenuChoice::Editor(command) => Some(*command),
            crate::lv_editor::MenuChoice::Struct(_) => None,
        })
        .collect();
    let new_row_structs: Vec<StructCommand> = NEW_ROWS
        .iter()
        .filter_map(|choice| match choice {
            crate::lv_editor::MenuChoice::Struct(command) => Some(*command),
            crate::lv_editor::MenuChoice::Editor(_) => None,
        })
        .collect();
    for choice in NEW_ROWS {
        match choice {
            crate::lv_editor::MenuChoice::Editor(command) => push(
                Area::NewRows,
                command.label().to_string(),
                table_keys(command, &combos),
            ),
            crate::lv_editor::MenuChoice::Struct(command) => {
                let mut keys: Vec<String> = combos
                    .iter()
                    .filter(|(key, modifiers, _)| {
                        StructCommand::for_key(*key, *modifiers) == Some(command)
                    })
                    .map(|(_, _, text)| text.clone())
                    .collect();
                if let Some(at) = keys.iter().position(|key| key == command.shortcut()) {
                    let key = keys.remove(at);
                    keys.insert(0, key);
                }
                push(
                    Area::NewRows,
                    shell_label(ShellAction::Structure(command)),
                    keys,
                );
            }
        }
    }

    // LV table: every command that some key reaches, in the order of the enum
    // as the palette and menus know it.
    let mut seen: Vec<Command> = Vec::new();
    for (key, modifiers, _) in &combos {
        if let Some(command) = command_for_key(*key, *modifiers) {
            if !seen.contains(&command) {
                seen.push(command);
            }
        }
    }
    for command in [Command::Copy, Command::Cut, Command::Paste] {
        if !seen.contains(&command) {
            seen.push(command);
        }
    }
    seen.sort_by_key(|command| order_of(*command));
    for (action, keys) in TABLE_CELL_KEYS {
        push(
            Area::Lv,
            action.to_string(),
            keys.iter().map(|key| key.to_string()).collect(),
        );
    }
    for (action, key) in LV_EXTRA {
        push(Area::Lv, action.to_string(), vec![key.to_string()]);
    }
    for command in seen
        .into_iter()
        .filter(|command| !ALSO_GENERAL.contains(command) && !new_row_editors.contains(command))
    {
        push(
            Area::Lv,
            command.label().to_string(),
            table_keys(command, &combos),
        );
    }

    let mut structure: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut structure_order: Vec<String> = Vec::new();
    for (key, modifiers, text) in &combos {
        if let Some(command) = StructCommand::for_key(*key, *modifiers)
            .filter(|command| !new_row_structs.contains(command))
        {
            let label = shell_label(ShellAction::Structure(command));
            if !structure_order.contains(&label) {
                structure_order.push(label.clone());
            }
            structure.entry(label).or_default().push(text.clone());
        }
    }
    for label in structure_order {
        let keys = structure.remove(&label).unwrap_or_default();
        push(Area::Structure, label, keys);
    }

    for (action, key) in CELL_EDIT_KEYS {
        push(Area::Cell, action.to_string(), vec![key.to_string()]);
    }
    for (key, action) in TEXT_KEYS {
        push(
            Area::Text,
            action.to_string(),
            vec![key_text(key, Modifiers::COMMAND)],
        );
    }
    for (key, action) in TEXT_FIELD_KEYS {
        push(
            Area::Text,
            action.to_string(),
            vec![key_text(key, Modifiers::COMMAND | Modifiers::SHIFT)],
        );
    }
    for (action, keys) in BILLING_KEYS {
        push(
            Area::Billing,
            action.to_string(),
            keys.iter().map(|key| key.to_string()).collect(),
        );
    }
    for (action, keys) in COST_KEYS {
        push(
            Area::Costs,
            action.to_string(),
            keys.iter().map(|key| key.to_string()).collect(),
        );
    }
    for (action, keys) in FILE_DIALOG_KEYS {
        push(
            Area::FileDialog,
            action.to_string(),
            keys.iter().map(|key| key.to_string()).collect(),
        );
    }
    out
}

/// Actions of the whole window that have a key (Speichern, Ansichten …).
fn general_actions() -> Vec<ShellAction> {
    palette_actions()
        .into_iter()
        .filter(|action| {
            !matches!(action, ShellAction::Editor(_) | ShellAction::Structure(_))
                || matches!(action, ShellAction::Editor(Command::Palette))
        })
        .filter(|action| !action.shortcut().is_empty())
        .collect()
}

fn shell_label(action: ShellAction) -> String {
    action.label()
}

/// Sort key of the LV commands: navigation, insert, edit, clipboard, history, search, status.
fn order_of(command: Command) -> usize {
    const ORDER: [Command; 34] = [
        Command::SelectUp,
        Command::SelectDown,
        Command::ExtendUp,
        Command::ExtendDown,
        Command::PageUp,
        Command::PageDown,
        Command::First,
        Command::Last,
        Command::Collapse,
        Command::Expand,
        Command::NewPosition,
        Command::NewPositionBefore,
        Command::NewTitle,
        Command::NewNote,
        Command::NewSpacer,
        Command::EditShort,
        Command::EditLong,
        Command::Properties,
        Command::Delete,
        Command::Duplicate,
        Command::MoveUp,
        Command::MoveDown,
        Command::Deeper,
        Command::Higher,
        Command::SelectAll,
        Command::Copy,
        Command::Cut,
        Command::Paste,
        Command::Undo,
        Command::Redo,
        Command::Find,
        Command::FindNext,
        Command::FindReplace,
        Command::NextMissingQuantity,
    ];
    ORDER
        .iter()
        .position(|other| *other == command)
        .unwrap_or(ORDER.len())
}

/// The overview as CSV (`Bereich;Befehl;Tasten`) for the Handbuch
/// (`docs/handbuch/tastenkuerzel.csv`, written by `scripts/handbuch.sh`).
pub fn csv() -> String {
    let mut out = String::from("Bereich;Befehl;Tasten\n");
    for entry in all() {
        out.push_str(&format!(
            "{};{};{}\n",
            entry.area.label(),
            entry.action.replace(';', ","),
            entry.keys.join(" oder ")
        ));
    }
    out
}

/// Lines matching a search (action or key, case-insensitive), optionally one area.
pub fn filtered(query: &str, area: Option<Area>) -> Vec<Shortcut> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    all()
        .into_iter()
        .filter(|entry| area.is_none_or(|area| entry.area == area))
        .filter(|entry| {
            let text = format!("{} {}", entry.action, entry.keys.join(" ")).to_lowercase();
            words.iter().all(|word| text.contains(word))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// key text → (area, action) of everything that claims it.
    fn claims() -> HashMap<String, Vec<(Area, String)>> {
        let mut map: HashMap<String, Vec<(Area, String)>> = HashMap::new();
        for entry in all() {
            for key in &entry.keys {
                map.entry(key.clone())
                    .or_default()
                    .push((entry.area, entry.action.clone()));
            }
        }
        map
    }

    #[test]
    fn no_two_commands_share_a_key() {
        let mut conflicts = Vec::new();
        for (key, users) in claims() {
            for area in Area::ALL {
                let mut actions: Vec<&String> = users
                    .iter()
                    .filter(|(other, _)| area.context().contains(other))
                    .map(|(_, action)| action)
                    .collect();
                actions.sort();
                actions.dedup();
                // The same action may be reachable from the window and the table
                // (Rückgängig, Befehl suchen): that is one command, not a conflict.
                if actions.len() > 1 {
                    conflicts.push(format!("{key} ({}): {actions:?}", area.label()));
                }
            }
        }
        conflicts.dedup();
        conflicts.sort();
        assert!(
            conflicts.is_empty(),
            "Doppelt belegte Tasten:\n{}",
            conflicts.join("\n")
        );
    }

    #[test]
    fn every_displayed_lv_shortcut_is_wired() {
        let combos = combinations();
        let events = [Command::Copy, Command::Cut, Command::Paste];
        let mut missing = Vec::new();
        let mut seen: Vec<Command> = Vec::new();
        for (key, modifiers, _) in &combos {
            if let Some(command) = command_for_key(*key, *modifiers) {
                if !seen.contains(&command) {
                    seen.push(command);
                }
            }
        }
        // Every command the palette or menus show with a key, and every one a key reaches.
        let mut commands: Vec<Command> = palette_actions()
            .into_iter()
            .filter_map(|action| match action {
                ShellAction::Editor(command) => Some(command),
                _ => None,
            })
            .collect();
        commands.extend(seen);
        for command in commands {
            let shown = command.shortcut();
            if shown.is_empty() || events.contains(&command) {
                continue;
            }
            let wired = combos.iter().any(|(key, modifiers, text)| {
                text == shown && command_for_key(*key, *modifiers) == Some(command)
            });
            if !wired {
                missing.push(format!(
                    "{:?} zeigt „{shown}“, die Taste löst ihn nicht aus",
                    command
                ));
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    #[test]
    fn structure_shortcuts_match_their_keys() {
        // The key a structure command shows triggers it (further keys, the
        // older Strg+Umschalt ones, may do so as well).
        let combos = combinations();
        for (key, modifiers, _) in &combos {
            if let Some(command) = StructCommand::for_key(*key, *modifiers) {
                let shown = command.shortcut();
                assert!(
                    combos.iter().any(|(key, modifiers, text)| text == shown
                        && StructCommand::for_key(*key, *modifiers) == Some(command)),
                    "{command:?} zeigt „{shown}“"
                );
            }
        }
    }

    #[test]
    fn new_rows_have_function_keys_first_and_keep_the_old_ones() {
        let rows = filtered("", Some(Area::NewRows));
        let keys = |action: &str| {
            rows.iter()
                .find(|entry| entry.action == action)
                .unwrap_or_else(|| panic!("{action} fehlt: {rows:?}"))
                .keys
                .clone()
        };
        let expected: [(&str, &[&str]); 9] = [
            ("Neue Position aus STLB-Bau …", &["F5"]),
            ("Neue Position aus AUSSCHREIBEN.DE …", &["F6"]),
            ("Neue Position darunter", &["F9", "Strg+Enter", "Einfg"]),
            ("Neue Position davor", &["Umschalt+F9", "Umschalt+Einfg"]),
            ("Neuer Hinweis darunter", &["F10", "Strg+H"]),
            ("Neuer Hinweis davor", &["Strg+F10", "Strg+Umschalt+H"]),
            ("Neue Ausführungsbeschreibung", &["F11", "Strg+Umschalt+A"]),
            ("Neuer Titel", &["F12", "Strg+Umschalt+Enter", "Strg+T"]),
            ("Neuer Untertitel", &["Umschalt+F12", "Strg+Umschalt+T"]),
        ];
        assert_eq!(rows.len(), expected.len(), "{rows:?}");
        for (action, wanted) in expected {
            let have = keys(action);
            assert_eq!(have[0], wanted[0], "{action}: F-Taste zuerst");
            for key in wanted {
                assert!(
                    have.iter().any(|other| other == key),
                    "{action}: {key} fehlt: {have:?}"
                );
            }
        }
        // Listed once: not again in the table or the structure.
        for entry in all() {
            if entry.area != Area::NewRows {
                assert!(
                    !expected.iter().any(|(action, _)| *action == entry.action),
                    "{} doppelt ({:?})",
                    entry.action,
                    entry.area
                );
            }
        }
        // F1 help, Umschalt+F10 the context menu.
        let f10 = claims().remove("Umschalt+F10").unwrap_or_default();
        assert_eq!(f10.len(), 1, "{f10:?}");
    }

    #[test]
    fn window_shortcuts_are_valid_key_texts_and_f1_is_free() {
        let texts: Vec<String> = combinations()
            .into_iter()
            .map(|(_, _, text)| text)
            .collect();
        for entry in all() {
            for key in &entry.keys {
                assert!(
                    texts.contains(key),
                    "„{key}“ ({}) ist keine bekannte Taste",
                    entry.action
                );
            }
        }
        let f1 = claims().remove("F1").unwrap_or_default();
        assert_eq!(f1.len(), 1, "F1 gehört nur der Hilfe: {f1:?}");
    }

    #[test]
    fn handbook_table_is_current() {
        let shipped = include_str!("../../../docs/handbuch/tastenkuerzel.csv");
        assert!(
            shipped == csv(),
            "Tastenkürzel geändert – Handbuch neu bauen: scripts/handbuch.sh (CSV und PDF committen)"
        );
    }

    #[test]
    fn overview_has_all_areas_and_finds_by_key() {
        let entries = all();
        for area in Area::ALL {
            assert!(
                entries.iter().any(|entry| entry.area == area),
                "{area:?} leer"
            );
        }
        let save = filtered("strg+s", None);
        assert!(
            save.iter().any(|entry| entry.action == "Speichern"),
            "{save:?}"
        );
        let new_title = entries
            .iter()
            .find(|entry| entry.action == "Neuer Titel")
            .unwrap();
        assert_eq!(new_title.keys[0], "F12", "bevorzugte Taste zuerst");
        assert!(new_title.keys.contains(&"Strg+T".to_string()));
        assert_eq!(
            filtered("", Some(Area::Text)).len(),
            5,
            "Fett, Kursiv, Unterstrichen, Bieterangabe, Angabe des Auftraggebers"
        );
        assert_eq!(
            filtered("Bieterangabe", Some(Area::Text))[0].keys,
            ["Strg+Umschalt+B"]
        );
        let long = entries
            .iter()
            .find(|entry| entry.action == Command::EditLong.label())
            .unwrap();
        assert_eq!(long.keys, ["Umschalt+F2"], "keine Toleranz-Varianten");
    }
}
