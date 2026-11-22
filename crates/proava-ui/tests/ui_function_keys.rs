//! Funktionstasten für neue Zeilen (Wunsch 18.11.2026): F5 STLB-Bau, F6
//! AUSSCHREIBEN.DE, F9 Position (Umschalt: davor), F10 Hinweis (Strg: davor),
//! F11 Ausführungsbeschreibung, F12 Titel (Umschalt: Untertitel) – überall in
//! der LV-Ansicht: Tabelle, Zelle beim Bearbeiten (vorher übernommen),
//! Eigenschaftenfenster, Fenster „Eigenschaften“ und „Langtext“ (hinter
//! deren Zeile). Die Übernahmedialoge öffnen sich nur, gestartet wird nichts.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{Decimal, Lv, LvGroup, LvRow, OzMask, Position, Project, ProjectName, RowId};

/// Title 01, subtitle 01.01 with two positions.
fn open(folder: &std::path::Path, panel: bool) -> (UiHarness, RowId) {
    std::fs::write(
        folder.join("settings.json"),
        format!(r#"{{"preferences":{{"detail_visible":{panel}}}}}"#),
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "Stationen"));
    let anchor = lv.push_position(Position::new(
        "01.01.0010",
        "DDC-Station",
        Decimal::ONE,
        "St",
        None,
    ));
    lv.push_position(Position::new(
        "01.01.0020",
        "Fühler",
        Decimal::ONE,
        "St",
        None,
    ));
    let project = Project::new(ProjectName::parse("Tasten").unwrap(), "2026-11-18", lv);
    let file = save(folder, &project);
    let harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    (harness, anchor)
}

fn select(harness: &mut UiHarness, row: RowId) {
    press(harness, Key::Home);
    for _ in 0..20 {
        let at = selected(harness).unwrap();
        if project(harness).lv().rows()[at].id() == row {
            return;
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile nicht erreicht");
}

fn kind(row: &LvRow) -> char {
    match row {
        LvRow::Group(_) => 'T',
        LvRow::Position(_) => 'P',
        LvRow::Note(_) => 'H',
        LvRow::Performance(_) => 'A',
        LvRow::Spacer(_) => 'L',
    }
}

fn kinds(harness: &UiHarness) -> String {
    project(harness).lv().rows().iter().map(kind).collect()
}

fn index_of(harness: &UiHarness, row: RowId) -> usize {
    project(harness).lv().index_of(row).unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Place {
    After,
    Before,
    /// Where the command puts it (AB, titles).
    Somewhere,
}

/// Every new-row key: modifiers, key, kind of the new row, place.
const NEW_ROWS: [(Modifiers, Key, char, Place); 7] = [
    (Modifiers::NONE, Key::F9, 'P', Place::After),
    (Modifiers::SHIFT, Key::F9, 'P', Place::Before),
    (Modifiers::NONE, Key::F10, 'H', Place::After),
    (Modifiers::COMMAND, Key::F10, 'H', Place::Before),
    (Modifiers::NONE, Key::F11, 'A', Place::Somewhere),
    (Modifiers::NONE, Key::F12, 'T', Place::Somewhere),
    (Modifiers::SHIFT, Key::F12, 'T', Place::Somewhere),
];

/// Where the keyboard is when the key comes.
#[derive(Clone, Copy, Debug)]
enum Context {
    Table,
    Cell,
    Panel,
    Window,
    LongText,
}

/// Opens the project with the keyboard in `context` at the anchor position
/// (typing „X“ where there is a text field) and returns the anchor.
fn prepare(folder: &std::path::Path, context: Context) -> (UiHarness, RowId) {
    let panel = matches!(context, Context::Panel);
    let (mut harness, anchor) = open(folder, panel);
    select(&mut harness, anchor);
    match context {
        Context::Table => {}
        Context::Cell => {
            press(&mut harness, Key::F2);
            assert!(
                harness
                    .state()
                    .workspace()
                    .unwrap()
                    .editing_cell()
                    .is_some(),
                "Zelle in Bearbeitung"
            );
            press(&mut harness, Key::End);
            type_text(&mut harness, "X");
        }
        Context::Panel => {
            harness
                .query_all_by(|node| {
                    node.role() == Role::TextInput && node.value().as_deref() == Some("DDC-Station")
                })
                .next()
                .expect("Kurztext im Eigenschaftenfenster")
                .click();
            settle(&mut harness);
            press(&mut harness, Key::End);
            type_text(&mut harness, "X");
        }
        Context::Window => {
            press_with(&mut harness, Modifiers::ALT, Key::Enter);
            assert_eq!(
                harness.state().workspace().unwrap().position_window_row(),
                Some(anchor),
                "Fenster „Eigenschaften“ offen"
            );
        }
        Context::LongText => {
            press_with(&mut harness, Modifiers::SHIFT, Key::F2);
            assert!(shows_part(&harness, "Langtext"), "Langtextfenster offen");
        }
    }
    (harness, anchor)
}

#[test]
fn every_new_row_key_works_everywhere_in_the_lv_view() {
    let contexts = [
        Context::Table,
        Context::Cell,
        Context::Panel,
        Context::Window,
        Context::LongText,
    ];
    for context in contexts {
        for (modifiers, key, new, place) in NEW_ROWS {
            let name = format!("{context:?} {modifiers:?} {key:?}");
            let folder = TempDir::new("f-tasten");
            let (mut harness, anchor) = prepare(&folder.0, context);
            let before = kinds(&harness);
            let at = index_of(&harness, anchor);
            press_with(&mut harness, modifiers, key);
            settle(&mut harness);
            let after = kinds(&harness);
            assert_eq!(
                after.len(),
                before.len() + 1,
                "{name}: eine neue Zeile ({before} → {after})"
            );
            assert_eq!(
                after.matches(new).count(),
                before.matches(new).count() + 1,
                "{name}: neue Zeile vom Typ {new} ({before} → {after})"
            );
            let new_at = match place {
                Place::After => Some(at + 1),
                Place::Before => Some(at),
                Place::Somewhere => None,
            };
            if let Some(new_at) = new_at {
                assert_eq!(
                    after.as_bytes()[new_at] as char,
                    new,
                    "{name}: Platz ({after})"
                );
            }
            // A typed value was taken over first (not lost, not cut).
            if matches!(context, Context::Cell | Context::Panel) {
                let short = project(&harness)
                    .lv()
                    .position_by_id(anchor)
                    .unwrap()
                    .short_text()
                    .to_string();
                assert_eq!(short, "DDC-StationX", "{name}: Eingabe übernommen");
            }
            // The window „Eigenschaften“ shows the new row.
            if matches!(context, Context::Window) {
                if let Some(new_at) = new_at {
                    let id = project(&harness).lv().rows()[new_at].id();
                    assert_eq!(
                        harness.state().workspace().unwrap().position_window_row(),
                        Some(id),
                        "{name}: Fenster zeigt die neue Zeile"
                    );
                }
            }
        }
    }
}

#[test]
fn f5_and_f6_open_the_take_over_dialogs_everywhere() {
    for context in [
        Context::Table,
        Context::Cell,
        Context::Panel,
        Context::Window,
        Context::LongText,
    ] {
        for (key, title) in [
            (Key::F5, proava_ui::strings::STLB_DIALOG_TITLE),
            (Key::F6, proava_ui::strings::AUSSCHREIBEN_DIALOG_TITLE),
        ] {
            let folder = TempDir::new("f5-f6");
            let (mut harness, anchor) = prepare(&folder.0, context);
            let rows = project(&harness).lv().rows().len();
            press(&mut harness, key);
            settle(&mut harness);
            assert!(
                shows(&harness, title),
                "{context:?} {key:?}: „{title}“ offen"
            );
            // Only the dialog: no row yet, nothing started (the browser
            // window opens by a click in the dialog; tests start nothing).
            assert_eq!(project(&harness).lv().rows().len(), rows);
            if matches!(context, Context::Cell | Context::Panel) {
                let short = project(&harness)
                    .lv()
                    .position_by_id(anchor)
                    .unwrap()
                    .short_text()
                    .to_string();
                assert_eq!(short, "DDC-StationX", "{context:?}: Eingabe übernommen");
            }
        }
    }
}

/// Strg as a real keyboard sends it on Linux and Windows (ctrl and command).
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

#[test]
fn text_fields_do_not_also_act_on_the_keys() {
    // Strg+H deletes a character in an egui field: the Kurztext typed in the
    // window „Eigenschaften“ stays whole. (Embedded, as here, the new row
    // takes the focus in the same frame; as a window of its own the window
    // keeps its focus a frame longer and takes the keys from its fields
    // itself – `new_row_keys` – which a test with one window cannot reach.)
    for (modifiers, key) in [(CTRL, Key::H), (Modifiers::NONE, Key::F10)] {
        let folder = TempDir::new("strg-h");
        let (mut harness, anchor) = prepare(&folder.0, Context::Window);
        harness
            .query_all_by(|node| {
                node.role() == Role::TextInput && node.value().as_deref() == Some("DDC-Station")
            })
            .next()
            .expect("Kurztext im Fenster")
            .click();
        settle(&mut harness);
        press(&mut harness, Key::End);
        type_text(&mut harness, "X");
        let before = kinds(&harness);
        press_with(&mut harness, modifiers, key);
        settle(&mut harness);
        assert_eq!(
            project(&harness)
                .lv()
                .position_by_id(anchor)
                .unwrap()
                .short_text(),
            "DDC-StationX",
            "{key:?}: kein Zeichen gelöscht"
        );
        assert_eq!(
            kinds(&harness).matches('H').count(),
            before.matches('H').count() + 1,
            "{key:?}: Hinweis angelegt"
        );
    }
}

#[test]
fn shortcuts_show_the_function_key_first() {
    let folder = TempDir::new("f-anzeige");
    let (mut harness, _) = open(&folder.0, true);
    click(&mut harness, "Einfügen");
    for (entry, key) in [
        ("Neue Position darunter", "F9"),
        ("Neue Position davor", "Umschalt+F9"),
        ("Neuer Hinweis darunter", "F10"),
        ("Neuer Hinweis davor", "Strg+F10"),
        ("Neue Ausführungsbeschreibung", "F11"),
        ("Neuer Titel", "F12"),
        ("Neuer Untertitel", "Umschalt+F12"),
        ("Neue Position aus STLB-Bau …", "F5"),
        ("Neue Position aus AUSSCHREIBEN.DE …", "F6"),
    ] {
        assert!(
            harness
                .query_all_by(|node| node
                    .label()
                    .is_some_and(|label| label.starts_with(entry) && label.contains(key)))
                .next()
                .is_some(),
            "Menü Einfügen: „{entry}“ mit {key}"
        );
    }
}
