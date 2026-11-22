//! LV structure: right-click menu, Ausführungsbeschreibung, status „fertig“
//! and Suchen/Ersetzen.

mod common;

use common::*;

/// Right click on the selected table row (found by its name „OZ Kurztext“).
fn right_click_selected_row(harness: &mut UiHarness) {
    let index = selected(harness).expect("gewählte Zeile");
    let row = &project(harness).lv().rows()[index];
    let name = format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string();
    harness
        .get_all_by_label(&name)
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(harness);
}

/// Right click ▸ Einfügen names what comes (Anwender 18.11.): „Neue Position
/// darunter/davor“, „Neue Position aus STLB-Bau …“ (below the clicked row),
/// no STLB-Bau-Platzhalter any more.
#[test]
fn right_click_insert_names_the_new_rows_and_stlb_goes_below() {
    let folder = TempDir::new("einfuegen-namen");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let ordinal = project(&harness).lv().rows()[index].ordinal().to_string();
    right_click_selected_row(&mut harness);
    open_submenu(&mut harness, "Einfügen");
    for name in [
        "Neue Position darunter",
        "Neue Position davor",
        "Neuer Hinweis darunter",
        "Neuer Hinweis davor",
        "Neuer Abstandshalter darunter (Leerzeile)",
        "Neue Position aus STLB-Bau …",
        "Neue Position aus AUSSCHREIBEN.DE …",
    ] {
        assert!(
            harness
                .query_all_by(|node| node
                    .label()
                    .is_some_and(|label| label == name || label.starts_with(&format!("{name} "))))
                .next()
                .is_some(),
            "{name} fehlt"
        );
    }
    assert!(
        !shows_part(&harness, "STLB-Bau-Platzhalter"),
        "kein Platzhalter mehr"
    );
    click_menu(&mut harness, "Neue Position aus STLB-Bau …");
    assert!(
        shows_part(&harness, &format!("einfügen hinter {ordinal}")),
        "Ziel: unter der angeklickten Zeile"
    );
}

#[test]
fn context_menu_inserts_a_note_below() {
    let folder = TempDir::new("menu");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let rows = project(&harness).lv().rows().len();

    right_click_selected_row(&mut harness);
    assert!(row_menu_open(&harness), "Kontextmenü offen");
    open_submenu(&mut harness, "Einfügen");
    click_menu(&mut harness, "Neuer Hinweis darunter");

    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows + 1);
    assert!(
        lv.rows()[index + 1].is_note(),
        "Hinweis direkt unter der Position"
    );
}

#[test]
fn new_performance_description_is_linked_to_the_selection() {
    let folder = TempDir::new("performance");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    let performances = |harness: &UiHarness| {
        project(harness)
            .lv()
            .rows()
            .iter()
            .filter(|row| row.is_performance())
            .count()
    };
    let before = performances(&harness);

    right_click_selected_row(&mut harness);
    open_submenu(&mut harness, "Verknüpfen");
    click_menu(&mut harness, "Neue Ausführungsbeschreibung für die Auswahl");

    assert_eq!(
        performances(&harness),
        before + 1,
        "neue Ausführungsbeschreibung"
    );
    let linked = project(&harness)
        .lv()
        .position_by_id(id)
        .unwrap()
        .performance()
        .expect("Position verknüpft");
    assert!(project(&harness)
        .lv()
        .rows()
        .iter()
        .any(|row| row.id() == linked && row.is_performance()));
    // Everywhere: badge/OZ column „A000n“ and the line under the OZ of the
    // position „Ausführung nach A000n“ (the only place to choose it).
    let shown = project(&harness)
        .lv()
        .performance_by_id(linked)
        .unwrap()
        .mark();
    assert_eq!(shown.len(), 5, "{shown}");
    assert!(shown.starts_with("A0"), "{shown}");
    assert!(shows_part(&harness, &shown), "Marke {shown}");
    press(&mut harness, Key::Escape);
    select_first_position(&mut harness);
    assert!(
        shows_part(&harness, &format!("Ausführung nach {shown}")),
        "Zeile unter der OZ zeigt {shown}"
    );
    assert!(
        harness
            .query_all_by(|node| node.role() == egui::accesskit::Role::ComboBox
                && node.value().is_some_and(|value| value.starts_with(&shown)))
            .next()
            .is_none(),
        "kein zweites Feld „Ausführungsbeschreibung“ mehr"
    );
}

#[test]
fn done_position_refuses_editing() {
    let folder = TempDir::new("done");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    let text = project(&harness).lv().rows()[index].short_text();

    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(project(&harness)
        .lv()
        .position_by_id(id)
        .unwrap()
        .is_locked());
    assert!(
        shows_part(&harness, "als fertig markiert"),
        "Hinweis sichtbar"
    );

    press(&mut harness, Key::F2);
    type_text(&mut harness, "geändert");
    press(&mut harness, Key::Enter);
    assert_eq!(
        project(&harness)
            .lv()
            .position_by_id(id)
            .unwrap()
            .short_text(),
        text
    );

    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(
        !project(&harness)
            .lv()
            .position_by_id(id)
            .unwrap()
            .is_locked(),
        "wieder offen"
    );
}

#[test]
fn find_and_replace_all() {
    let folder = TempDir::new("replace");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let word = project(&harness).lv().rows()[index]
        .short_text()
        .split_whitespace()
        .find(|word| word.chars().count() >= 4 && word.chars().all(char::is_alphabetic))
        .expect("Wort im Kurztext")
        .to_string();
    let count = |harness: &UiHarness, text: &str| {
        row_texts(harness)
            .iter()
            .filter(|short| short.contains(text))
            .count()
    };
    let hits = count(&harness, &word);
    assert!(hits > 0);

    press_with(&mut harness, Modifiers::COMMAND, Key::R);
    assert!(shows(&harness, "Alle ersetzen"), "Leiste offen");
    type_text(&mut harness, &word);
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "Ersatzwort");
    click(&mut harness, "Alle ersetzen");

    assert_eq!(count(&harness, &word), 0, "alle ersetzt");
    assert_eq!(count(&harness, "Ersatzwort"), hits);

    click(&mut harness, "Schließen");
    assert!(!shows(&harness, "Alle ersetzen"), "Leiste zu");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(count(&harness, &word), hits, "ein Schritt zurück");
}

/// Befund 7 (17.11.): Ausführungsbeschreibungen count by themselves –
/// 1, 2, 3 … by place, also when one is put in front; the number field
/// appears only on „Von Hand …“.
#[test]
fn performance_descriptions_are_numbered_automatically_by_place() {
    use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
    let folder = TempDir::new("ab-nummern");
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Kabel"));
    lv.push_position(Position::new("", "Kabel 1", Decimal::ONE, "m", None));
    lv.push_group(LvGroup::new(0, "", "Trassen"));
    lv.push_position(Position::new("", "Trasse 1", Decimal::ONE, "m", None));
    lv.renumber();
    let file = save(
        &folder.0,
        &Project::new(ProjectName::parse("AB").unwrap(), "2026-11-17", lv),
    );
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Project(file)));

    let numbers = |harness: &UiHarness| -> Vec<String> {
        project(harness)
            .lv()
            .performances()
            .map(|performance| performance.number().to_string())
            .collect()
    };
    // Under „Trassen“ (third row), then under „Kabel“ (first row).
    press(&mut harness, Key::Home);
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowDown);
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    press(&mut harness, Key::Escape);
    assert_eq!(numbers(&harness), ["1"]);
    let trassen = project(&harness).lv().performances().next().unwrap().id();
    press(&mut harness, Key::Home);
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    press(&mut harness, Key::Escape);
    assert_eq!(numbers(&harness), ["1", "2"], "nach Lage durchgezählt");
    assert_eq!(
        project(&harness)
            .lv()
            .performance_by_id(trassen)
            .unwrap()
            .number(),
        "2",
        "die frühere Nr. 1 steht jetzt weiter unten"
    );
    assert!(project(&harness).lv().performances_in_order());

    // The panel shows the mark („A0001“, Anwender 18.11.) and no
    // number field until „Von Hand …“.
    assert!(
        shows(&harness, "A0001"),
        "Kennung im Bearbeitungsfenster/Tabelle"
    );
    assert!(!shows(&harness, "AB 1") && !shows(&harness, "AB 0001"));
    assert!(shows(&harness, "zählt automatisch"));
    let number_fields = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.role() == egui::accesskit::Role::TextInput
                    && node.value().as_deref() == Some("0001")
            })
            .count()
    };
    assert_eq!(
        number_fields(&harness),
        0,
        "keine Pflichteingabe der Nummer"
    );
    click(&mut harness, "Von Hand …");
    assert_eq!(
        number_fields(&harness),
        1,
        "Nummernfeld erst auf Wunsch, vierstellig"
    );

    // By hand: „A0007“ (also „7“, „0007“, „AB 0007“) is stored as GAEB has
    // it (7) and shown as A0007.
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some("0001")
        })
        .next()
        .unwrap()
        .click();
    settle(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "A0007");
    press(&mut harness, Key::Enter);
    let first = project(&harness).lv().performances().next().unwrap();
    assert_eq!((first.number(), first.mark().as_str()), ("7", "A0007"));
    assert!(shows(&harness, "A0007"), "Tabelle zeigt die neue Nummer");
    assert!(
        !project(&harness).lv().performances_in_order(),
        "eigene Nummern"
    );
}

/// Strg+Umschalt+O selects the own number, so typing replaces it
/// („0015“, not „00100015“).
#[test]
fn edit_ordinal_shortcut_replaces_the_number() {
    let folder = TempDir::new("oz-taste");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = project(&harness).lv().rows()[index].ordinal().to_string();
    assert!(before.ends_with(".0010"), "{before}");

    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::O);
    type_text(&mut harness, "0015");
    press(&mut harness, Key::Enter);

    let after = project(&harness).lv().rows()[index].ordinal().to_string();
    assert_eq!(after, before.replace(".0010", ".0015"));
}
