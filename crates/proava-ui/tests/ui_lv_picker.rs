//! LVs des Projekts ohne Umschalter (Anwender 18.11.2026 22:16): alle LVs
//! stehen untereinander in der LV-Tabelle. Die Kopfleiste hat keine
//! LV-Auswahl mehr; die Befehle für LVs stehen im Menü LV und im
//! Kontextmenü der LV-Kopfzeile, „Gehe zu LV“ springt zur Kopfzeile.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{Lv, OzMask, Project, ProjectName};

/// Entries of the menu LV and of the context menu of an LV head row.
const LV_COMMANDS: [&str; 8] = [
    "Neues LV …",
    "LV-Eigenschaften …",
    "LV duplizieren (ohne Angebote)",
    "Ganzes LV kopieren",
    "LV aus Zwischenablage einfügen …",
    "LV nach oben",
    "LV nach unten",
    "LV löschen …",
];

fn project_with(titles: &[&str]) -> Project {
    let lv = |title: &str| Lv::new(title, OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
    let mut project = Project::new(
        ProjectName::parse("Auswahl").unwrap(),
        "2026-11-18",
        lv(titles[0]),
    );
    for title in &titles[1..] {
        project.add_lv(lv(title));
    }
    project
}

fn open(folder: &std::path::Path, titles: &[&str]) -> UiHarness {
    let file = save(folder, &project_with(titles));
    harness(folder, Some(proava_ui::StartFile::Project(file)))
}

/// A button in the header that names an LV (the former LV choice).
fn header_choice(harness: &UiHarness, title: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.role() == Role::Button && node.label().is_some_and(|label| label.contains(title))
        })
        .next()
        .is_some()
}

#[test]
fn the_header_has_no_lv_choice_any_more() {
    for titles in [
        &["Gebäudeautomation"][..],
        &["Gebäudeautomation", "Elektro", "Sanitär"],
    ] {
        let folder = TempDir::new("auswahl-kopf");
        let mut harness = open(&folder.0, titles);
        settle(&mut harness);
        assert!(!header_choice(&harness, "Gebäudeautomation"), "{titles:?}");
        assert!(!shows(&harness, "Es gibt keine weiteren LVs im Projekt"));
    }
}

#[test]
fn gehe_zu_lv_makes_the_lv_active_and_shows_its_head_row() {
    let folder = TempDir::new("auswahl-gehe-zu");
    let mut harness = open(&folder.0, &["Gebäudeautomation", "Elektro", "Sanitär"]);
    open_bar_menu(&mut harness, "LV");
    open_submenu(&mut harness, "Gehe zu LV");
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.contains("Sanitär")))
        .last()
        .expect("Eintrag Sanitär")
        .click();
    settle(&mut harness);
    assert_eq!(project(&harness).active_lv(), 2, "gewechselt");
    assert_eq!(project(&harness).lv().title(), "Sanitär");
    assert!(shows(&harness, "LV-Kopf Sanitär"), "Kopfzeile im Blick");
}

#[test]
fn the_menu_lv_has_every_lv_command() {
    let folder = TempDir::new("auswahl-menue");
    let mut harness = open(&folder.0, &["Gebäudeautomation", "Elektro"]);
    open_bar_menu(&mut harness, "LV");
    for command in LV_COMMANDS.iter().chain(["Gehe zu LV"].iter()) {
        assert!(shows_part(&harness, command), "Menü LV: {command}");
    }
}

#[test]
fn the_head_row_has_the_lv_commands_in_its_context_menu() {
    let folder = TempDir::new("auswahl-kopfmenue");
    let mut harness = open(&folder.0, &["Gebäudeautomation", "Elektro"]);
    settle(&mut harness);
    harness
        .query_all_by(|node| {
            node.role() == Role::Row && node.label().as_deref() == Some("LV-Kopf Elektro")
        })
        .next()
        .expect("Kopfzeile Elektro")
        .click_secondary();
    settle(&mut harness);
    for command in LV_COMMANDS {
        assert!(
            shows_part(&harness, command),
            "Kontextmenü der Kopfzeile: {command}"
        );
    }
    click_menu(&mut harness, "LV nach oben");
    assert_eq!(
        project(&harness).lv_at(0).unwrap().title(),
        "Elektro",
        "nach oben verschoben"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness).lv_at(0).unwrap().title(),
        "Gebäudeautomation",
        "Strg+Z"
    );
}
