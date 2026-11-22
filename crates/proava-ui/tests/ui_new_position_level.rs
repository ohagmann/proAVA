//! „Neue Position“ unter einem Titel oberhalb der untersten Gliederungsebene:
//! In der Bibliothek (Maske mit vier Titelebenen, Einträge unter Titeln der
//! Ebene 2) kommt die neue Position in den Titel der Schreibmarke, ohne neue
//! Zwischentitel – über F9, Strg+Enter, Rechtsklick und Menü Einfügen, auf
//! einer Position und auf einem Titel; Strg+Z nimmt sie zurück. Ein normales
//! LV mit derselben Lage verhält sich gleich; ein LV mit Positionen auf der
//! untersten Ebene bekommt unter einem neuen Titel wie bisher Untertitel.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use egui::accesskit::Role;
use proava_application::ProjectRepository;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};
use proava_infrastructure::XmlProjectFile;
use proava_ui::StartFile;

/// Like the library: mask 11.222.33.44.PPPP, positions under level-2 titles.
fn shallow_lv(title: &str) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse("11.222.33.44.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Automation"));
    lv.push_group(LvGroup::new(1, "", "Hardware"));
    lv.push_position(Position::new("", "Bedienstation", Decimal::ONE, "St", None));
    lv.push_position(Position::new("", "Router", Decimal::ONE, "St", None));
    lv.push_group(LvGroup::new(1, "", "Software"));
    lv.push_position(Position::new(
        "",
        "Grundsoftware",
        Decimal::ONE,
        "psch",
        None,
    ));
    lv.renumber();
    lv
}

fn write(file: &Path, lv: Lv) {
    let project = Project::new(ProjectName::parse("Bibliothek").unwrap(), "2026-11-20", lv);
    XmlProjectFile::new().save_project(file, &project).unwrap();
}

/// The library file, registered as the own library (library mode).
fn open_library(folder: &Path) -> UiHarness {
    let file = folder.join("Bibliothek GA.proava");
    write(&file, shallow_lv("Positionsbibliothek"));
    let launcher = launcher(folder);
    let mut preferences = launcher.preferences();
    preferences.library_file = Some(file.clone());
    launcher.set_preferences(preferences).unwrap();
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(
        shows(&harness, "Bibliothek: Bibliothek GA"),
        "Bibliothek offen"
    );
    harness
}

/// The same rows as an ordinary project (no library).
fn open_lv(folder: &Path) -> UiHarness {
    let file: PathBuf = folder.join("Rathaus 3.proava");
    write(&file, shallow_lv("Gebäudeautomation"));
    harness(folder, Some(StartFile::Project(file)))
}

fn rows(harness: &UiHarness) -> Vec<(String, String)> {
    project(harness)
        .lv()
        .rows()
        .iter()
        .map(|row| (row.ordinal().to_string(), row.short_text().to_string()))
        .collect()
}

fn titles(harness: &UiHarness) -> usize {
    project(harness).lv().group_count()
}

fn click_row(harness: &mut UiHarness, name: &str, secondary: bool) {
    let node = harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"));
    if secondary {
        node.click_secondary();
    } else {
        node.click();
    }
    settle(harness);
}

/// The four ways to „Neue Position darunter“ with the Schreibmarke on `row`.
#[derive(Debug, Clone, Copy)]
enum Way {
    F9,
    CtrlEnter,
    RightClick,
    Menu,
}

fn new_position(harness: &mut UiHarness, row: &str, way: Way) {
    click_row(harness, row, false);
    match way {
        Way::F9 => press(harness, Key::F9),
        Way::CtrlEnter => press_with(harness, Modifiers::COMMAND, Key::Enter),
        Way::RightClick => {
            click_row(harness, row, true);
            open_submenu(harness, "Einfügen");
            click_menu(harness, "Neue Position darunter");
        }
        Way::Menu => {
            open_bar_menu(harness, "Einfügen");
            click_menu(harness, "Neue Position darunter");
        }
    }
    settle(harness);
    // Leave the Kurztext of the new position (it has the caret).
    press(harness, Key::Escape);
    settle(harness);
}

/// Every way, on a position and on a title of level 2: one row more, no
/// title more, the new position in that title with the next OZ; Strg+Z.
fn no_new_titles(harness: &mut UiHarness, kind: &str) {
    let before = rows(harness);
    let groups = titles(harness);
    for way in [Way::F9, Way::CtrlEnter, Way::RightClick, Way::Menu] {
        // On a position: next to it, in its title.
        new_position(harness, "01.001.00.00.0020 Router", way);
        assert_eq!(
            titles(harness),
            groups,
            "{kind} {way:?} auf Position: kein neuer Titel"
        );
        let after = rows(harness);
        assert_eq!(
            after.len(),
            before.len() + 1,
            "{kind} {way:?}: eine Zeile mehr"
        );
        assert_eq!(
            after[4].0, "01.001.00.00.0030",
            "{kind} {way:?}: OZ im Titel Hardware"
        );
        assert_eq!(
            after[5].1, "Software",
            "{kind} {way:?}: Software bleibt danach"
        );
        press_with(harness, Modifiers::COMMAND, Key::Z);
        settle(harness);
        assert_eq!(rows(harness), before, "{kind} {way:?}: Strg+Z");

        // On a title of level 2: right below it, on its level.
        new_position(harness, "01.002 Software", way);
        assert_eq!(
            titles(harness),
            groups,
            "{kind} {way:?} auf Titel: kein neuer Titel"
        );
        let after = rows(harness);
        assert_eq!(after[4].1, "Software");
        assert_eq!(
            after[5].0, "01.002.00.00.0010",
            "{kind} {way:?}: erste im Titel Software"
        );
        assert_eq!(after[5].1, "", "{kind} {way:?}: die neue Position");
        assert_eq!(
            after[6],
            ("01.002.00.00.0020".into(), "Grundsoftware".into())
        );
        press_with(harness, Modifiers::COMMAND, Key::Z);
        settle(harness);
        assert_eq!(rows(harness), before, "{kind} {way:?}: Strg+Z");
    }
}

#[test]
fn in_the_library_a_new_position_goes_into_the_title_without_new_titles() {
    let folder = TempDir::new("bibpos-bibliothek");
    let mut harness = open_library(&folder.0);
    no_new_titles(&mut harness, "Bibliothek");

    // On the top title (it has subtitles): directly under it, no title either.
    let groups = titles(&harness);
    new_position(&mut harness, "01 Automation", Way::F9);
    assert_eq!(titles(&harness), groups, "kein Zwischentitel unter 01");
    assert_eq!(rows(&harness)[1].0, "01.000.00.00.0010");
}

#[test]
fn an_lv_with_positions_on_that_level_takes_the_new_one_there_too() {
    let folder = TempDir::new("bibpos-lv");
    let mut harness = open_lv(&folder.0);
    assert!(!shows_part(&harness, "Bibliothek: "), "normales Projekt");
    no_new_titles(&mut harness, "LV");
}

#[test]
fn an_lv_with_positions_on_the_deepest_level_keeps_its_subtitles() {
    // Gegenprobe: positions only on the deepest level – a new top title gets
    // the levels below it with the first position, as before.
    let folder = TempDir::new("bibpos-tief");
    let mut lv = Lv::new("GA", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Automation"));
    lv.push_group(LvGroup::new(1, "", "ASP01"));
    lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
    lv.push_group(LvGroup::new(0, "", "Feldgeräte"));
    lv.renumber();
    let file = folder.0.join("Tief.proava");
    write(&file, lv);
    let mut harness = harness(&folder.0, Some(StartFile::Project(file)));
    let groups = titles(&harness);
    new_position(&mut harness, "01.01.0010 DDC", Way::F9);
    assert_eq!(titles(&harness), groups, "neben DDC kein Titel");
    assert_eq!(rows(&harness)[3].0, "01.01.0020");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    new_position(&mut harness, "02 Feldgeräte", Way::F9);
    assert_eq!(titles(&harness), groups + 1, "Untertitel wie bisher");
    let after = rows(&harness);
    assert_eq!(after[4].0, "02.01");
    assert_eq!(after[5].0, "02.01.0010");
    assert!(project(&harness).lv().rows()[5].is_position());
}
