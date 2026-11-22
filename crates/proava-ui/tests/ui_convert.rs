//! Zeilenart umwandeln (Anwender 18.11.): Rechtsklick ▸ Bearbeiten ▸
//! „Umwandeln in“ mit Rückfrage und Strg+Z, die Zeilenart im
//! Bearbeitungsbereich und die Übernahme einer STLB-Bau-Standardbeschreibung
//! als Hinweis.

mod common;

use common::*;
use proava_domain::LvRow;

/// A file dropped on the window (egui 0.37 takes a handle).
#[derive(Debug)]
struct Dropped(std::path::PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

#[test]
fn right_click_turns_a_position_into_a_hint_after_asking_and_strg_z_brings_it_back() {
    let folder = TempDir::new("umwandeln-menue");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let position = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    let rows = project(&harness).lv().rows().len();
    let name = format!("{} {}", position.ordinal(), position.short_text())
        .trim()
        .to_string();
    harness
        .get_all_by_label(&name)
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(&mut harness);
    open_submenu(&mut harness, "Bearbeiten");
    open_submenu(&mut harness, "Umwandeln in");
    click_menu(&mut harness, "Hinweis (Text)");

    // The demo position has quantity and unit: the question names them.
    assert!(shows(&harness, "In Hinweis umwandeln?"), "Rückfrage");
    assert!(shows_part(&harness, "Fällt weg"), "nennt, was wegfällt");
    assert!(
        shows_part(&harness, &format!("Ordnungszahl {}", position.ordinal())),
        "OZ"
    );
    assert!(
        project(&harness).lv().rows()[index].is_position(),
        "erst nach dem Klick"
    );
    click(&mut harness, "Umwandeln");
    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows, "keine Zeile mehr oder weniger");
    let LvRow::Note(note) = &lv.rows()[index] else {
        panic!("Hinweis erwartet: {:?}", lv.rows()[index])
    };
    assert_eq!(note.id(), position.id(), "dieselbe Zeile");
    assert_eq!(note.heading(), position.short_text());
    assert_eq!(note.text(), position.long_text());
    assert!(shows_part(&harness, "In Hinweis umgewandelt"), "Meldung");
    assert!(
        shows_part(&harness, "Zeilenart"),
        "Bearbeitungsbereich zeigt die Zeilenart"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let back = project(&harness).lv().rows()[index].as_position().cloned();
    assert_eq!(
        back.as_ref(),
        Some(&position),
        "Strg+Z: Position wie vorher"
    );
}

#[test]
fn a_hint_becomes_a_position_without_question_and_gets_a_free_number() {
    let folder = TempDir::new("umwandeln-hinweis");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::H);
    type_text(&mut harness, "Allgemeiner Text");
    press(&mut harness, Key::Escape);
    let note = (index..project(&harness).lv().rows().len())
        .find(|&row| project(&harness).lv().rows()[row].is_note())
        .expect("neuer Hinweis");
    let id = project(&harness).lv().rows()[note].id();
    // Over the menu bar: Bearbeiten ▸ Umwandeln in ▸ Position.
    open_bar_menu(&mut harness, "Bearbeiten");
    open_submenu(&mut harness, "Umwandeln in");
    click_menu(&mut harness, "Position");
    let lv = project(&harness).lv();
    let index = lv.index_of(id).unwrap();
    let position = lv.rows()[index].as_position().expect("Position");
    assert!(!position.ordinal().is_empty(), "freie OZ");
    assert!(position.long_text().to_plain().contains("Allgemeiner Text"));
    assert!(
        !shows(&harness, "In Position umwandeln?"),
        "nichts fällt weg – keine Rückfrage"
    );
}

#[test]
fn a_standard_description_from_stlb_bau_fills_the_placeholder_as_a_hint() {
    let folder = TempDir::new("umwandeln-stlb");
    // A placeholder from a project (proAVA makes no new ones), the last row.
    let mut demo = proava_ui::demo::demo_project();
    demo.lv_mut()
        .push_position(proava_domain::Position::stlb("Standardbeschreibung"));
    demo.lv_mut().renumber();
    let file = save(&folder.0, &demo);
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    let placeholder = selected(&harness).expect("Platzhalter gewählt");
    assert!(project(&harness).lv().rows()[placeholder]
        .as_position()
        .is_some_and(|position| position.is_stlb_placeholder()));
    let id = project(&harness).lv().rows()[placeholder].id();

    // The file as STLB-Bau Online writes it (synthetic, see tests/data/README.md).
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn8_stlb_standardbeschreibung_33.X81");
    harness
        .input_mut()
        .dropped_files
        .push(std::sync::Arc::new(Dropped(file)));
    harness.run_steps(40);
    assert!(shows(&harness, "Übernehmen als"), "Spalte im Dialog");
    assert!(
        shows(&harness, "Hinweis (Text)"),
        "Vorschlag für die Standardbeschreibung"
    );
    assert!(
        shows_part(&harness, "als Text"),
        "Zählzeile nennt die Texte"
    );
    click_menu(&mut harness, "Übernehmen (4)");

    let lv = project(&harness).lv();
    let index = lv.index_of(id).expect("Zeile des Platzhalters");
    let LvRow::Note(note) = &lv.rows()[index] else {
        panic!("Platzhalter als Hinweis: {:?}", lv.rows()[index])
    };
    assert_eq!(note.heading(), "Standardbesch Testfunktion");
    assert_eq!(
        note.catalog().stlb.as_ref().map(|stlb| stlb.group),
        Some(9301)
    );
    let next = lv.rows()[index + 1]
        .as_position()
        .expect("gewöhnliche Position");
    assert_eq!(next.short_text(), "Testfühler Raum");
    assert!(lv.rows()[index + 2].is_note(), "Hinweistext der Datei");
    assert!(
        lv.rows()[index + 3].is_performance(),
        "Ausführungsbeschreibung der Datei"
    );
}
