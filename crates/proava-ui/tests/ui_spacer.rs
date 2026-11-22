//! Abstandshalter (empty line for editing only) and the warning for hints
//! without text: insert, convert, undo, copy into another window.

mod common;

use common::*;
use proava_domain::{Lv, LvRow, Note, OzMask, Project, ProjectName};
use proava_ui::StartFile;

fn spacers(harness: &UiHarness) -> usize {
    project(harness).lv().spacer_count()
}

#[test]
fn strg_l_inserts_a_spacer_below_and_the_table_keeps_the_keyboard() {
    let folder = TempDir::new("abstand-taste");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let rows = project(&harness).lv().rows().len();
    let ordinals: Vec<String> = project(&harness)
        .lv()
        .rows()
        .iter()
        .map(|row| row.ordinal().to_string())
        .collect();

    press_with(&mut harness, Modifiers::COMMAND, Key::L);
    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows + 1);
    assert!(
        lv.rows()[index + 1].is_spacer(),
        "Abstandshalter direkt unter der Position"
    );
    assert_eq!(selected(&harness), Some(index + 1), "neue Zeile gewählt");
    let kept: Vec<String> = lv
        .rows()
        .iter()
        .filter(|row| !row.is_spacer())
        .map(|row| row.ordinal().to_string())
        .collect();
    assert_eq!(kept, ordinals, "keine OZ ändert sich");
    assert!(
        shows(&harness, "Abstandshalter"),
        "Tabellenzeile und Eigenschaften"
    );
    assert!(
        shows_part(&harness, "Leerzeile nur für die Bearbeitung"),
        "Eigenschaftenbereich erklärt ihn"
    );

    // Nothing to type into: the arrow keys move on, Enter edits nothing.
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::ArrowDown);
    assert_eq!(
        selected(&harness),
        Some(index + 2),
        "Tabelle behält die Tastatur"
    );

    // Move it, delete it, undo.
    press(&mut harness, Key::ArrowUp);
    press_with(&mut harness, Modifiers::ALT, Key::ArrowDown);
    assert!(
        project(&harness).lv().rows()[index + 2].is_spacer(),
        "Alt+↓ verschiebt"
    );
    press(&mut harness, Key::Delete);
    assert_eq!(spacers(&harness), 0);
    // The undo message names the spacer by its place, never an empty OZ.
    let after = project(&harness).lv().rows()[index].ordinal().to_string();
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(spacers(&harness), 1, "Strg+Z holt ihn zurück");
    let next = project(&harness).lv().rows()[index + 1]
        .ordinal()
        .to_string();
    assert!(
        shows_part(
            &harness,
            &format!("Rückgängig: Abstandshalter wieder da (nach {next}).")
        ),
        "Meldung nach dem Löschen"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(
        shows_part(
            &harness,
            &format!("Rückgängig: Abstandshalter verschoben (nach {after}).")
        ),
        "Meldung nach dem Verschieben"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(spacers(&harness), 0, "Einfügen rückgängig");
    assert!(shows_part(
        &harness,
        &format!("Rückgängig: Abstandshalter entfernt (nach {after}).")
    ));
    assert!(!shows_part(&harness, "(„“)"), "keine leere Angabe");
    assert_eq!(project(&harness).lv().rows().len(), rows);
}

#[test]
fn context_menu_and_lv_menu_insert_a_spacer() {
    let folder = TempDir::new("abstand-menue");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let row = &project(&harness).lv().rows()[index];
    let name = format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string();
    harness
        .get_all_by_label(&name)
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(&mut harness);
    open_submenu(&mut harness, "Einfügen");
    click_menu(&mut harness, "Neuer Abstandshalter darunter (Leerzeile)");
    assert!(project(&harness).lv().rows()[index + 1].is_spacer());

    // Same name in the menu bar.
    menu_path(
        &mut harness,
        "Einfügen",
        "Neuer Abstandshalter darunter (Leerzeile)",
    );
    assert_eq!(spacers(&harness), 2);
    assert!(
        project(&harness).lv().rows()[index + 2].is_spacer(),
        "unter dem gewählten Abstandshalter"
    );
}

#[test]
fn the_context_menu_of_a_spacer_offers_only_what_works_there() {
    let folder = TempDir::new("abstand-kontext");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::L);
    // The table row (the panel and the status bar say „Abstandshalter“ too).
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Abstandshalter")
                && format!("{:?}", node.role()) == "Row"
        })
        .next()
        .expect("Tabellenzeile")
        .click_secondary();
    settle(&mut harness);
    assert!(
        row_menu_open(&harness) && shows(&harness, "Bearbeiten"),
        "Kontextmenü offen"
    );
    assert!(
        !shows(&harness, "Status"),
        "Status wirkt an einem Abstandshalter nicht"
    );
    open_submenu(&mut harness, "Bearbeiten");
    let entry = |harness: &UiHarness, text: &str| {
        harness
            .query_all_by(|node| {
                node.label()
                    .is_some_and(|label| label == text || label.starts_with(&format!("{text} ")))
            })
            .next()
            .is_some()
    };
    assert!(
        entry(&harness, "Nach oben schieben") && entry(&harness, "Duplizieren"),
        "Verschieben, Duplizieren bleiben"
    );
    for absent in [
        "Kurztext bearbeiten",
        "Langtext im eigenen Fenster bearbeiten",
        "Eigenschaften im eigenen Fenster …",
        "Ordnungszahl ändern …",
        "In Bibliothek übernehmen …",
    ] {
        assert!(
            !entry(&harness, absent),
            "„{absent}“ bewirkt am Abstandshalter nichts"
        );
    }
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Escape);

    // A position keeps the full menu.
    let index = selected(&harness).unwrap() - 1;
    let row = &project(&harness).lv().rows()[index];
    let name = format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string();
    harness
        .get_all_by_label(&name)
        .next()
        .expect("Position")
        .click_secondary();
    settle(&mut harness);
    assert!(shows(&harness, "Status"), "Position: Status bleibt");
}

/// Project with one hint without text after the first position.
fn project_with_blank_note() -> Project {
    let mut project = proava_ui::demo::demo_project();
    let lv = project.lv_mut();
    let first = lv.rows().iter().position(LvRow::is_position).unwrap();
    lv.insert_numbered(first + 1, LvRow::Note(Note::new(" ")))
        .unwrap();
    project
}

fn open(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    harness
}

#[test]
fn a_hint_without_text_is_marked_and_turns_into_a_spacer() {
    let folder = TempDir::new("abstand-leer");
    let mut harness = open(&folder.0, &project_with_blank_note());
    let blank = project(&harness).lv().blank_notes()[0];
    assert!(
        shows(&harness, "Hinweis ohne Text – leer"),
        "Zeilenmarke in der Tabelle"
    );
    let is_spacer = |harness: &UiHarness| {
        project(harness)
            .lv()
            .row_by_id(blank)
            .is_some_and(LvRow::is_spacer)
    };
    let is_blank_note = |harness: &UiHarness| {
        project(harness)
            .lv()
            .row_by_id(blank)
            .is_some_and(LvRow::is_blank_note)
    };

    // Prüfen ▸ LV prüfen …: warning and the button for all of them.
    menu_path(&mut harness, "Prüfen", "LV prüfen …");
    assert!(
        shows_part(&harness, "Klick auf eine Zeile zeigt sie im LV"),
        "LV prüfen offen"
    );
    click(
        &mut harness,
        "Leere Hinweise in Abstandshalter umwandeln (1)",
    );
    assert!(is_spacer(&harness), "gleiche Zeile, jetzt Abstandshalter");
    assert!(project(&harness).lv().blank_notes().is_empty());
    assert!(
        !shows_part(&harness, "Leere Hinweise in Abstandshalter umwandeln ("),
        "Prüfung neu: nichts mehr umzuwandeln"
    );
    press(&mut harness, Key::Escape);
    // One undo step brings the hint back.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(is_blank_note(&harness));

    // The single one: right-click ▸ Bearbeiten ▸ In Abstandshalter umwandeln.
    harness
        .get_all_by_label("Hinweis ohne Text – leer")
        .next()
        .unwrap()
        .click_secondary();
    settle(&mut harness);
    open_submenu(&mut harness, "Bearbeiten");
    click_menu(&mut harness, "In Abstandshalter umwandeln");
    assert!(is_spacer(&harness));
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(is_blank_note(&harness));

    // Offene Punkte name it and convert it too.
    menu_path(&mut harness, "Prüfen", "Offene Punkte …");
    assert!(
        shows_part(&harness, "Hinweis ohne Text (1)"),
        "Offene Punkte"
    );
    click(
        &mut harness,
        "Hinweise ohne Text in Abstandshalter umwandeln (1)",
    );
    assert!(is_spacer(&harness));
}

#[test]
fn spacers_go_into_another_window_with_the_rows() {
    let (folder_a, folder_b) = (TempDir::new("abstand-a"), TempDir::new("abstand-b"));
    let mut source = proava_ui::demo::demo_project();
    let first = source
        .lv()
        .rows()
        .iter()
        .position(LvRow::is_position)
        .unwrap();
    source
        .lv_mut()
        .insert_numbered(first + 1, LvRow::Spacer(proava_domain::Spacer::new()))
        .unwrap();
    let mut a = open(&folder_a.0, &source);
    let target = Project::new(
        ProjectName::parse("Ziel").unwrap(),
        "2026-11-18T10:00:00",
        Lv::new("Ziel", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
    );
    let mut b = open(&folder_b.0, &target);

    // A: whole LV (Strg+A) → Strg+C; B: Strg+V.
    press_with(&mut a, Modifiers::COMMAND, Key::A);
    a.event(egui::Event::Copy);
    settle(&mut a);
    let plain = proava_ui::system_clipboard::text().unwrap();
    b.event(egui::Event::Paste(plain));
    settle(&mut b);
    assert_eq!(
        project(&b).lv().spacer_count(),
        1,
        "Abstandshalter kommt mit"
    );
    assert_eq!(
        project(&b).lv().position_count(),
        source.lv().position_count()
    );
}
