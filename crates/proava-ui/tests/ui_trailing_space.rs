//! A space typed at the end of a text field shows at once and stays while typing on – in the library and
//! in an ordinary LV, in every text field there is. Cleaning (trim, splitting
//! the Stichwörter) happens only when the value is taken over.

mod common;

use std::path::Path;

use common::*;
use egui::accesskit::Role;
use proava_application::ProjectRepository;
use proava_domain::library::EntryInfo;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_infrastructure::XmlProjectFile;
use proava_ui::StartFile;

/// A library (or an ordinary LV) with a title and one position.
fn project(name: &str) -> Project {
    let mut lv = Lv::new("Positionsbibliothek", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Raumautomation"));
    let mut position = Position::new("", "Raumbediengerät", Decimal::ONE, "St", None);
    position.set_library_info(EntryInfo {
        keywords: vec!["KNX".into()],
        as_of: "01.11.2026".into(),
        price_date: "11/2026".into(),
        ..EntryInfo::default()
    });
    lv.push_position(position);
    lv.renumber();
    Project::new(ProjectName::parse(name).unwrap(), "2026-11-20", lv)
}

/// Opens `file`; as the own library of the settings when `library`.
fn open(folder: &Path, library: bool) -> UiHarness {
    let file = folder.join(if library {
        "Meine-Bibliothek.proava"
    } else {
        "Projekt.proava"
    });
    XmlProjectFile::new()
        .save_project(&file, &project("Test"))
        .unwrap();
    if library {
        let launcher = launcher(folder);
        let mut preferences = launcher.preferences();
        preferences.library_file = Some(file.clone());
        launcher.set_preferences(preferences).unwrap();
    }
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(harness.state().project().is_some(), "geöffnet");
    assert_eq!(
        shows_part(&harness, "Bibliothek: Meine-Bibliothek"),
        library,
        "Bibliotheksmodus"
    );
    harness
}

/// Value of the text field that has the keyboard.
fn focused(harness: &UiHarness) -> Option<String> {
    harness
        .query_all_by(|node| node.role() == Role::TextInput && node.is_focused())
        .next()
        .and_then(|node| node.value())
}

/// Puts the caret at the end of the field showing `value`.
fn focus_field(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{value}“"))
        .click();
    settle(harness);
    press(harness, Key::End);
}

/// In the focused field: „ “ shows at once, „x“ follows it.
fn space_stays(harness: &mut UiHarness, before: &str, what: &str) -> Result<(), String> {
    type_text(harness, " ");
    let after_space = focused(harness);
    type_text(harness, "x");
    let after_x = focused(harness);
    let expected = (format!("{before} "), format!("{before} x"));
    if after_space.as_deref() == Some(expected.0.as_str())
        && after_x.as_deref() == Some(expected.1.as_str())
    {
        Ok(())
    } else {
        Err(format!(
            "{what}: nach Leerzeichen {after_space:?}, nach „x“ {after_x:?}"
        ))
    }
}

fn select_position(harness: &mut UiHarness) {
    harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with("01.0010 "))
        })
        .next()
        .expect("Zeile 01.0010")
        .click();
    settle(harness);
}

/// Every field of the edit panel of the position and the table cells.
fn check_all(library: bool) -> Vec<String> {
    let folder = TempDir::new(if library {
        "leerzeichen-bib"
    } else {
        "leerzeichen-lv"
    });
    let mut problems = Vec::new();
    let fields: Vec<(&str, &str)> = {
        let mut list = vec![
            ("Kurztext (Bearbeitungsbereich)", "Raumbediengerät"),
            ("Einheit", "St"),
        ];
        if library {
            list.push(("Stichwörter", "KNX"));
            list.push(("Stand", "01.11.2026"));
            list.push(("Preis vom", "11/2026"));
        }
        list
    };
    for (what, value) in fields {
        let mut harness = open(&folder.0, library);
        select_position(&mut harness);
        focus_field(&mut harness, value);
        if let Err(problem) = space_stays(&mut harness, value, what) {
            problems.push(problem);
        }
    }
    // Table cell: F2 on the Kurztext.
    let mut harness = open(&folder.0, library);
    select_position(&mut harness);
    press(&mut harness, Key::F2);
    press(&mut harness, Key::End);
    if let Err(problem) = space_stays(&mut harness, "Raumbediengerät", "Tabellenzelle Kurztext") {
        problems.push(problem);
    }
    problems
}

#[test]
fn a_trailing_space_shows_at_once_in_the_library() {
    let problems = check_all(true);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_trailing_space_shows_at_once_in_an_ordinary_lv() {
    let problems = check_all(false);
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Reiter Nachträge, neue Position im Nachtrag: Kurztext and Einheit of the
/// item form were trimmed with every key (Kurztext `trim_end`, Einheit
/// `trim`); now as typed while the field has the keyboard, trimmed when it is
/// left.
#[test]
fn change_order_item_keeps_a_trailing_space_and_trims_on_leaving() {
    let folder = TempDir::new("leerzeichen-nachtrag");
    let project = proava_ui::demo_change_orders::change_order_demo_project();
    let file = save(&folder.0, &project);
    let mut harness = harness(&folder.0, Some(StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num5);
    click(&mut harness, "Neuer Nachtrag");
    click(&mut harness, "Neue Position");
    type_text(&mut harness, "Zusatzfühler");
    harness.get_all_by_label("Einfügen").last().unwrap().click();
    settle(&mut harness);
    click(&mut harness, "Abbrechen");
    let item = |harness: &UiHarness| {
        let project = proava_ui_project(harness);
        let order = project.change_orders().last().unwrap();
        let position = project
            .lv()
            .position_by_id(order.items[0].position)
            .unwrap();
        (
            position.short_text().to_string(),
            position.unit().to_string(),
        )
    };
    let (short, unit) = item(&harness);
    assert_eq!(short, "Zusatzfühler");

    let mut problems = Vec::new();
    focus_field(&mut harness, &short);
    if let Err(problem) = space_stays(&mut harness, &short, "Nachtrag Kurztext") {
        problems.push(problem);
    }
    focus_field(&mut harness, &unit);
    if let Err(problem) = space_stays(&mut harness, &unit, "Nachtrag Einheit") {
        problems.push(problem);
    }
    assert!(problems.is_empty(), "{problems:#?}");

    // Leaving the field (Tab to the Einheit): cleaned.
    focus_field(&mut harness, &format!("{short} x"));
    type_text(&mut harness, "  ");
    assert_eq!(
        item(&harness).0,
        format!("{short} x  "),
        "während der Eingabe"
    );
    press(&mut harness, Key::Tab);
    assert_eq!(
        item(&harness).0,
        format!("{short} x"),
        "beim Verlassen bereinigt"
    );
}

fn proava_ui_project(harness: &UiHarness) -> &Project {
    harness.state().project().expect("Projekt offen")
}

fn position(harness: &UiHarness) -> proava_domain::Position {
    proava_ui_project(harness)
        .lv()
        .positions()
        .next()
        .unwrap()
        .clone()
}

/// What is cleaned stays cleaned – only later: the Einheit is trimmed, the
/// Stichwörter are split at comma and semicolon (blanks around them gone,
/// doubles once) and the Stand becomes TT.MM.JJJJ, each when the field is left.
#[test]
fn cleaning_happens_when_the_field_is_left() {
    let folder = TempDir::new("leerzeichen-bereinigen");
    let mut harness = open(&folder.0, true);
    select_position(&mut harness);

    focus_field(&mut harness, "St");
    type_text(&mut harness, "k  ");
    assert_eq!(
        position(&harness).unit(),
        "Stk  ",
        "während der Eingabe wie getippt"
    );
    press(&mut harness, Key::Tab);
    assert_eq!(position(&harness).unit(), "Stk", "beim Verlassen getrimmt");

    focus_field(&mut harness, "KNX");
    type_text(&mut harness, " ,  Präsenz ; knx;Licht ");
    assert_eq!(
        focused(&harness).as_deref(),
        Some("KNX ,  Präsenz ; knx;Licht "),
        "Stichwörter wie getippt"
    );
    assert_eq!(
        position(&harness).library_info().keywords,
        ["KNX"],
        "noch nicht übernommen"
    );
    press(&mut harness, Key::Tab);
    assert_eq!(
        position(&harness).library_info().keywords,
        ["KNX", "Präsenz", "Licht"],
        "beim Verlassen getrennt"
    );

    focus_field(&mut harness, "01.11.2026");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, " 2.12.2026 ");
    press(&mut harness, Key::Tab);
    assert_eq!(position(&harness).library_info().as_of, "02.12.2026");
}
