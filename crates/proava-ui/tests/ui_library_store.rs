//! „In Bibliothek übernehmen …“ per Rechtsklick (welche Bibliothek, welcher
//! Titel, Stichwörter, neue Bibliothek, Rückgängig) und „Bibliothek
//! bearbeiten …“ (eigenes Fenster bzw. Band „Bibliothek“ mit „Zurück zu …“).
//! Alles in Temp-Ordnern; ein zweites Fenster startet im Test nie, es wird
//! nur gezählt (`jobs::refused_starts`).

mod common;

use std::path::{Path, PathBuf};

use common::*;
use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;
use proava_application::{LibrarySetting, ProjectRepository};
use proava_domain::library::EntryInfo;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_infrastructure::XmlProjectFile;
use proava_ui::StartFile;

const ROW: &str = "01.01.0010 DDC-Automationsstation, frei programmierbar";

/// A library with the title „Raumautomation“ and two positions.
fn library_file(file: &Path) {
    let mut lv = Lv::new(
        "Positionsbibliothek",
        OzMask::parse("11.222.33.44.PPPP").unwrap(),
    );
    lv.push_group(LvGroup::new(0, "", "Raumautomation"));
    let mut knx = Position::new("", "Raumbediengerät", Decimal::ONE, "St", None);
    knx.set_library_info(EntryInfo {
        keywords: vec!["KNX".into()],
        ..EntryInfo::default()
    });
    lv.push_position(knx);
    lv.push_position(Position::new("", "Präsenzmelder", Decimal::ONE, "St", None));
    lv.renumber();
    let project = Project::new(ProjectName::parse("Bibliothek").unwrap(), "2026-11-19", lv);
    XmlProjectFile::new().save_project(file, &project).unwrap();
}

/// Settings: the own library at `own` (written when `exists`), a further
/// one read-only, and the library window as set.
fn settings(folder: &Path, own: &Path, exists: bool, own_window: bool) {
    if exists {
        library_file(own);
    }
    let office = folder.join("Buero.proava");
    library_file(&office);
    let launcher = launcher(folder);
    let mut preferences = launcher.preferences();
    preferences.library_file = Some(own.to_path_buf());
    preferences.libraries = vec![LibrarySetting {
        path: office,
        label: "Büro".into(),
        read_only: true,
    }];
    preferences.library_own_window = own_window;
    launcher.set_preferences(preferences).unwrap();
}

fn read(file: &Path) -> Project {
    XmlProjectFile::new().load_project(file).unwrap()
}

/// Short texts of the library's rows.
fn texts(file: &Path) -> Vec<String> {
    read(file)
        .lv()
        .rows()
        .iter()
        .map(|row| row.short_text().to_string())
        .collect()
}

fn right_click_row(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"))
        .click_secondary();
    settle(harness);
}

/// Clicks the text field right below the caption `caption`.
fn field_below(harness: &mut UiHarness, caption: &str) {
    let above = harness
        .get_all_by_label(caption)
        .next()
        .expect(caption)
        .rect();
    harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .filter(|node| node.rect().top() >= above.bottom() - 1.0)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("Eingabefeld")
        .click();
    settle(harness);
}

/// The text field with this value (focus, all selected).
fn field_with(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{value}“"))
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
}

/// The open project file of the window.
fn open_file(harness: &UiHarness) -> PathBuf {
    harness
        .state()
        .workspace()
        .expect("Projekt offen")
        .project_file()
        .to_path_buf()
}

#[test]
fn right_click_stores_rows_under_a_title_with_keywords_and_rueckgaengig_takes_it_back() {
    let folder = TempDir::new("bibliothek-ablegen");
    let own = folder.0.join("Meine-Bibliothek.proava");
    settings(&folder.0, &own, true, true);
    let mut harness = open_demo(&folder.0);
    let before = project(&harness).clone();

    // Straight in the right-click menu, not under „Bearbeiten“.
    right_click_row(&mut harness, ROW);
    assert!(row_menu_open(&harness), "Kontextmenü offen");
    assert!(
        shows_part(&harness, "In Bibliothek übernehmen …"),
        "Eintrag im Kontextmenü"
    );
    click_menu(&mut harness, "In Bibliothek übernehmen …");
    assert!(shows(&harness, "In welche Bibliothek?"), "Dialog offen");
    assert!(shows(&harness, "Eigene Bibliothek „Meine-Bibliothek“"));
    let office_disabled = harness
        .query_all_by(|node| node.label().as_deref() == Some("Büro „Buero“ (schreibgeschützt)"))
        .next()
        .expect("schreibgeschützte Bibliothek in der Liste")
        .accesskit_node()
        .is_disabled();
    assert!(office_disabled, "schreibgeschützt: grau");
    assert!(shows(&harness, "Neue Bibliothek anlegen …"));

    click(&mut harness, "01 Raumautomation");
    field_below(&mut harness, "Stichwörter (optional, mit Komma getrennt)");
    type_text(&mut harness, "DDC, BACnet");
    click(&mut harness, "Übernehmen");

    assert!(!shows(&harness, "In welche Bibliothek?"), "Dialog zu");
    assert!(
        shows_part(
            &harness,
            "In „Meine-Bibliothek“ abgelegt: 1 Zeile unter „Raumautomation“."
        ),
        "Meldung"
    );
    let library = read(&own);
    let stored = library
        .lv()
        .positions()
        .find(|position| position.short_text() == "DDC-Automationsstation, frei programmierbar")
        .expect("Position in der Bibliothek");
    assert_eq!(stored.library_info().keywords, ["DDC", "BACnet"]);
    assert_eq!(
        texts(&own),
        [
            "Raumautomation",
            "Raumbediengerät",
            "Präsenzmelder",
            "DDC-Automationsstation, frei programmierbar"
        ],
        "unter dem Titel Raumautomation"
    );
    assert_eq!(*project(&harness), before, "Projekt unverändert");

    click(&mut harness, "Rückgängig");
    assert_eq!(
        texts(&own),
        ["Raumautomation", "Raumbediengerät", "Präsenzmelder"],
        "Bibliotheksdatei wie vorher"
    );
    assert!(shows_part(
        &harness,
        "Ablegen in „Meine-Bibliothek“ zurückgenommen."
    ));
}

#[test]
fn without_a_library_the_dialog_creates_one_and_the_next_time_finds_the_title() {
    let folder = TempDir::new("bibliothek-neu");
    // The own library is set but not there yet (no file in the home folder).
    let own = folder.0.join("Bibliothek GA.proava");
    settings(&folder.0, &own, false, true);
    let mut harness = open_demo(&folder.0);

    right_click_row(&mut harness, ROW);
    click_menu(&mut harness, "In Bibliothek übernehmen …");
    assert!(
        !shows_part(&harness, "Eigene Bibliothek"),
        "keine eigene Bibliothek zur Wahl"
    );
    assert!(shows(&harness, "Ort (Ordner)"), "Name und Ort der neuen");
    field_with(&mut harness, "Bibliothek GA");
    type_text(&mut harness, "Neue-Bibliothek");
    click(&mut harness, "Übernehmen");

    let created = folder.0.join("Neue-Bibliothek.proava");
    assert_eq!(
        texts(&created),
        [
            "Automationsstationen",
            "DDC-Automationsstation, frei programmierbar"
        ],
        "neue Bibliothek, Titel wie im LV"
    );
    assert_eq!(
        launcher(&folder.0).library_file(),
        created,
        "als eigene Bibliothek eingetragen"
    );
    assert!(shows_part(
        &harness,
        "In „Neue-Bibliothek“ abgelegt: 1 Zeile"
    ));

    // Again: now the own library is there and its title is chosen.
    right_click_row(&mut harness, "01.01.0020 E/A-Modul 16 DI");
    click_menu(&mut harness, "In Bibliothek übernehmen …");
    assert!(shows(&harness, "Eigene Bibliothek „Neue-Bibliothek“"));
    click(&mut harness, "Übernehmen");
    assert_eq!(
        texts(&created),
        [
            "Automationsstationen",
            "DDC-Automationsstation, frei programmierbar",
            "E/A-Modul 16 DI"
        ],
        "unter demselben Titel"
    );
}

#[test]
fn a_new_library_besides_the_own_one_goes_to_the_further_libraries() {
    let folder = TempDir::new("bibliothek-weitere");
    let own = folder.0.join("Meine-Bibliothek.proava");
    settings(&folder.0, &own, true, true);
    let mut harness = open_demo(&folder.0);
    right_click_row(&mut harness, ROW);
    click_menu(&mut harness, "In Bibliothek übernehmen …");
    click(&mut harness, "Neue Bibliothek anlegen …");
    field_with(&mut harness, "Meine-Bibliothek");
    type_text(&mut harness, "Herstellertexte");
    click(&mut harness, "Übernehmen");
    let created = folder.0.join("Herstellertexte.proava");
    assert!(created.exists(), "angelegt");
    let launcher = launcher(&folder.0);
    assert_eq!(launcher.library_file(), own, "eigene bleibt");
    assert!(
        launcher
            .library_at(&created)
            .is_some_and(|source| !source.read_only && !source.own),
        "bei den weiteren Bibliotheken, beschreibbar"
    );
}

#[test]
fn bibliothek_bearbeiten_opens_a_window_of_its_own_and_the_project_stays() {
    let folder = TempDir::new("bibliothek-fenster");
    let own = folder.0.join("Meine-Bibliothek.proava");
    settings(&folder.0, &own, true, true);
    let mut harness = open_demo(&folder.0);
    let project_file = open_file(&harness);
    let started = proava_ui::jobs::refused_starts();
    open_bar_menu(&mut harness, "LV");
    open_submenu(&mut harness, "Bibliothek");
    click_menu(&mut harness, "Bibliothek bearbeiten …");
    assert_eq!(
        proava_ui::jobs::refused_starts(),
        started + 1,
        "ein neues Fenster verlangt (im Test nur gezählt)"
    );
    assert_eq!(open_file(&harness), project_file, "Projekt bleibt offen");
    assert!(shows_part(
        &harness,
        "„Meine-Bibliothek“ wird in einem neuen Fenster geöffnet"
    ));
    assert!(!shows_part(&harness, "Bibliothek: "), "kein Band hier");
}

#[test]
fn in_this_window_the_band_says_bibliothek_and_zurueck_saves_and_opens_the_project() {
    let folder = TempDir::new("bibliothek-zurueck");
    let own = folder.0.join("Meine-Bibliothek.proava");
    settings(&folder.0, &own, true, false);
    let mut harness = open_demo(&folder.0);
    let project_file = open_file(&harness);
    assert!(
        !shows_part(&harness, "Bibliothek: "),
        "kein Band im Projekt"
    );

    open_bar_menu(&mut harness, "LV");
    open_submenu(&mut harness, "Bibliothek");
    click_menu(&mut harness, "Bibliothek bearbeiten …");
    settle(&mut harness);
    assert_eq!(open_file(&harness), own, "Bibliothek in diesem Fenster");
    assert!(shows(&harness, "Bibliothek: Meine-Bibliothek"), "Band");
    assert!(shows_part(
        &harness,
        "Eigene Bibliothek – Einträge pflegen, kein Projekt"
    ));

    // A change in the library, then back: saved like on closing.
    select_first_position(&mut harness);
    press(&mut harness, Key::Delete);
    settle(&mut harness);
    assert_eq!(project(&harness).lv().position_count(), 1, "gelöscht");
    assert_eq!(
        read(&own).lv().position_count(),
        2,
        "gespeichert wird erst beim Zurück"
    );
    click(&mut harness, "Zurück zu Demo Preisspiegel GA");
    settle(&mut harness);
    assert_eq!(open_file(&harness), project_file, "Projekt wieder offen");
    assert_eq!(
        read(&own).lv().position_count(),
        1,
        "Bibliothek gespeichert"
    );
    assert!(!shows_part(&harness, "Bibliothek: "), "Band weg");
}

#[test]
fn a_library_opened_from_the_start_page_leads_back_to_the_last_project() {
    let folder = TempDir::new("bibliothek-start");
    let own = folder.0.join("Meine-Bibliothek.proava");
    settings(&folder.0, &own, true, true);
    let harness = open_demo(&folder.0);
    let project_file = open_file(&harness);
    drop(harness);

    let mut harness = harness_with(&folder.0, &own);
    assert!(shows(&harness, "Bibliothek: Meine-Bibliothek"), "Band");
    click(&mut harness, "Zurück zu Demo Preisspiegel GA");
    settle(&mut harness);
    assert_eq!(open_file(&harness), project_file);
}

fn harness_with(folder: &Path, file: &Path) -> UiHarness {
    harness(folder, Some(StartFile::Project(file.to_path_buf())))
}
