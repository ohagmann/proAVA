//! Befund 5 (17.11.): keys typed into a text field or the Langtext editor
//! belong to that field – the LV table must not start editing a cell, move
//! the selection or delete a row.

mod common;

use common::*;

fn cell_editing(harness: &UiHarness) -> bool {
    harness.state().workspace().unwrap().cell_editing()
}

/// A single click at a screen point (press and release in two frames).
fn click_at(harness: &mut UiHarness, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

fn focus_field(harness: &mut UiHarness, label: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{label}“ fehlt"))
        .click();
    settle(harness);
}

/// Clicks into the running text of the Langtext editor (below its toolbar).
fn click_into_long_text(harness: &mut UiHarness) {
    let toolbar = harness
        .get_all_by_label("Tabelle einfügen …")
        .next()
        .expect("Werkzeugleiste des Langtexts")
        .rect();
    click_at(
        harness,
        egui::pos2(toolbar.left() + 40.0, toolbar.bottom() + 70.0),
    );
}

#[test]
fn enter_in_the_long_text_makes_a_paragraph_and_leaves_the_table_alone() {
    let folder = TempDir::new("langtext-enter");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    let short = project(&harness).lv().rows()[index]
        .short_text()
        .to_string();
    let rows = project(&harness).lv().rows().len();

    click_into_long_text(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "Neuer Absatz im Langtext");
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "Noch einer");

    assert!(
        !cell_editing(&harness),
        "Enter im Langtext startet keine Zellbearbeitung"
    );
    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows, "keine Zeile dazu");
    let row = lv.row_by_id(id).unwrap();
    assert_eq!(row.short_text(), short, "Kurztext unverändert");
    let position = row.as_position().unwrap();
    let plain = position.long_text().to_plain();
    assert!(
        plain.contains("Neuer Absatz im Langtext\nNoch einer"),
        "{plain:?}"
    );
    assert_eq!(
        harness.state().workspace().unwrap().selected_index(),
        Some(index),
        "Auswahl bleibt"
    );
}

#[test]
fn keys_in_short_text_and_quantity_fields_stay_in_the_field() {
    let folder = TempDir::new("felder-enter");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let rows = project(&harness).lv().rows().len();

    for label in ["Kurztext", "Menge"] {
        focus_field(&mut harness, label);
        press(&mut harness, Key::Enter);
        assert!(!cell_editing(&harness), "Enter im Feld {label}");
        focus_field(&mut harness, label);
        press(&mut harness, Key::F2);
        press(&mut harness, Key::Delete);
        press(&mut harness, Key::ArrowDown);
        type_text(&mut harness, "1");
        assert!(!cell_editing(&harness), "F2/Tippen im Feld {label}");
        assert_eq!(
            project(&harness).lv().rows().len(),
            rows,
            "Entf im Feld {label} löscht keine Zeile"
        );
        assert_eq!(
            harness.state().workspace().unwrap().selected_index(),
            Some(index),
            "Pfeil im Feld {label} bewegt die Auswahl nicht"
        );
        press(&mut harness, Key::Escape);
    }
}

#[test]
fn enter_in_the_search_field_searches_instead_of_editing() {
    let folder = TempDir::new("suche-enter");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    let rows = project(&harness).lv().rows().len();
    press_with(&mut harness, Modifiers::COMMAND, Key::F);
    type_text(&mut harness, "Regler");
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Delete);
    assert!(
        !cell_editing(&harness),
        "Enter im Suchfeld startet keine Zellbearbeitung"
    );
    assert_eq!(
        project(&harness).lv().rows().len(),
        rows,
        "Entf im Suchfeld löscht keine Zeile"
    );
}

/// The user's case: a new position, its Langtext still empty (placeholder
/// shown), click into it, type, Enter.
#[test]
fn enter_in_an_empty_long_text_of_a_new_position_stays_in_the_text() {
    let folder = TempDir::new("langtext-leer");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    // The demo LV has bids: the new position needs a „Trotzdem einfügen“,
    // then the short text cell of the new row takes the typing.
    keep_despite_bids(&mut harness, "einfügen");
    type_text(&mut harness, "Neue Position");
    let index = harness
        .state()
        .workspace()
        .unwrap()
        .selected_index()
        .unwrap();
    let id = project(&harness).lv().rows()[index].id();

    click_into_long_text(&mut harness);
    assert!(
        !cell_editing(&harness),
        "Klick in den Langtext schließt die Zelle"
    );
    let short = project(&harness)
        .lv()
        .row_by_id(id)
        .unwrap()
        .short_text()
        .to_string();
    assert_eq!(short, "Neue Position");
    type_text(&mut harness, "Erster Absatz");
    press(&mut harness, Key::Enter);
    assert!(
        !cell_editing(&harness),
        "Enter im leeren Langtext startet keine Zellbearbeitung"
    );
    type_text(&mut harness, "Zweiter Absatz");
    press(&mut harness, Key::Enter);
    assert!(!cell_editing(&harness));

    let row = project(&harness).lv().row_by_id(id).unwrap();
    assert_eq!(row.short_text(), "Neue Position", "Kurztext unverändert");
    let plain = row.as_position().unwrap().long_text().to_plain();
    assert!(
        plain.starts_with("Erster Absatz\nZweiter Absatz"),
        "{plain:?}"
    );
}

/// A title that holds only a hint (the user's „01 Gebäudeautomation“ with
/// „Projektbeschreibung“), like a new project.
fn title_with_hint(folder: &std::path::Path) -> UiHarness {
    use proava_domain::{Lv, LvGroup, LvRow, Note, OzMask, Project, ProjectName};
    let mut lv = Lv::new(
        "Leistungsverzeichnis",
        OzMask::parse("11.22.PPPPI").unwrap(),
    );
    lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Gebäudeautomation")));
    lv.push_row(LvRow::Note(Note::new("Projektbeschreibung")));
    let project = Project::new(
        ProjectName::parse("Befund5").unwrap(),
        "2026-11-17T22:00:00",
        lv,
    );
    let file = save(folder, &project);
    let harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    assert!(harness.state().project().is_some());
    harness
}

#[test]
fn new_position_under_a_title_with_a_hint_takes_the_typed_text_not_the_title() {
    let folder = TempDir::new("titel-hinweis");
    let mut harness = title_with_hint(&folder.0);
    // Like the user: click the title row with the mouse (on the GP column).
    let row = harness
        .get_all_by_label("01 Gebäudeautomation")
        .next()
        .expect("Titelzeile")
        .rect();
    click_at(&mut harness, egui::pos2(row.right() - 30.0, row.center().y));
    assert_eq!(
        harness.state().workspace().unwrap().selected_index(),
        Some(0),
        "Titel gewählt"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    type_text(&mut harness, "Kabel");
    press(&mut harness, Key::Enter);

    let lv = project(&harness).lv();
    let texts: Vec<String> = lv
        .rows()
        .iter()
        .map(|row| row.short_text().to_string())
        .collect();
    assert_eq!(
        lv.rows()[0].short_text(),
        "Gebäudeautomation",
        "Überschrift bleibt: {texts:?}"
    );
    assert!(
        lv.rows()
            .iter()
            .any(|row| row.is_position() && row.short_text() == "Kabel"),
        "Kurztext an der neuen Position: {texts:?}"
    );

    // Then into the (empty) Langtext of the new position: Enter stays there.
    click_into_long_text(&mut harness);
    type_text(&mut harness, "Absatz eins");
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "Absatz zwei");
    assert!(
        !cell_editing(&harness),
        "Enter im Langtext öffnet keine Zelle"
    );
    let lv = project(&harness).lv();
    assert_eq!(
        lv.rows()[0].short_text(),
        "Gebäudeautomation",
        "Überschrift bleibt"
    );
    let position = lv.rows().iter().find_map(|row| row.as_position()).unwrap();
    assert!(
        position
            .long_text()
            .to_plain()
            .starts_with("Absatz eins\nAbsatz zwei"),
        "{:?}",
        position.long_text().to_plain()
    );
}
