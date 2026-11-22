//! Side window „Bibliothek“: F8, full-text search, insert with one undo
//! step and the origin of the inserted position.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use proava_application::ProjectRepository;
use proava_domain::library::EntryInfo;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName, RichText};
use proava_infrastructure::XmlProjectFile;

/// A small library file and settings that point to it.
fn library(folder: &Path) -> PathBuf {
    let mut lv = Lv::new(
        "Positionsbibliothek",
        OzMask::parse("11.222.33.44.PPPP").unwrap(),
    );
    lv.push_group(LvGroup::new(0, "", "Raumautomation"));
    let mut knx = Position::new(
        "",
        "Raumbediengerät",
        Decimal::ONE,
        "St",
        Some(Decimal::from(180)),
    );
    knx.set_long_text(RichText::plain("Taster mit Farbdisplay für KNX, reinweiß"));
    knx.set_library_info(EntryInfo {
        keywords: vec!["KNX".into()],
        as_of: "01.11.2026".into(),
        price_date: "10/2026".into(),
    });
    lv.push_position(knx);
    lv.push_position(Position::new("", "Präsenzmelder", Decimal::ONE, "St", None));
    lv.renumber();
    let project = Project::new(ProjectName::parse("Bibliothek").unwrap(), "2026-11-17", lv);
    let file = folder.join("Meine-Bibliothek.proava");
    XmlProjectFile::new().save_project(&file, &project).unwrap();
    let launcher = launcher(folder);
    let mut preferences = launcher.preferences();
    preferences.library_file = Some(file.clone());
    launcher.set_preferences(preferences).unwrap();
    file
}

#[test]
fn f8_search_and_enter_insert_with_origin_and_one_undo_step() {
    let folder = TempDir::new("library-panel");
    library(&folder.0);
    let mut harness = open_demo(&folder.0);
    let rows = project(&harness).lv().rows().len();
    assert!(!shows(&harness, "Bibliothek"), "Seitenfenster zu");

    press(&mut harness, Key::F8);
    assert!(shows(&harness, "Bibliothek"), "F8 zeigt das Seitenfenster");
    assert!(
        shows_part(&harness, "Raumbediengerät"),
        "Baum der Bibliothek"
    );
    // F8 put the keyboard into the search field: type, ↓ to the hit, Enter.
    type_text(&mut harness, "farbdisplay");
    assert!(
        shows(&harness, "1 Position gefunden"),
        "Volltext im Langtext"
    );
    assert!(!shows_part(&harness, "Präsenzmelder"), "nur Treffer");
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowDown);
    assert!(
        shows_part(&harness, "Erfahrungspreis 180,00 € (Stand 10/2026)"),
        "Vorschau"
    );
    assert!(shows_part(&harness, "Stichwörter: KNX"));
    press(&mut harness, Key::Enter);

    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows + 1, "eine Zeile eingefügt");
    let inserted = lv
        .positions()
        .find(|position| position.short_text() == "Raumbediengerät")
        .expect("eingefügt");
    let origin = inserted.origin().expect("Herkunft gemerkt");
    assert_eq!(origin.library, "Meine-Bibliothek");
    assert_eq!(origin.as_of, "01.11.2026");
    assert_eq!(
        inserted.quantity(),
        Decimal::ZERO,
        "Mengen standardmäßig nicht"
    );
    assert_eq!(
        inserted.unit_price(),
        Some(Decimal::from(180)),
        "Erfahrungspreis als Planerpreis"
    );

    // One undo step takes it out again (keys back to the table first).
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).lv().rows().len(), rows, "Strg+Z-Schritt");

    press(&mut harness, Key::F8);
    assert!(!shows(&harness, "1 Position gefunden"), "F8 blendet aus");
}

#[test]
fn several_libraries_are_searched_together_and_settings_list_them() {
    let folder = TempDir::new("library-several");
    library(&folder.0);
    // A second, read-only library.
    let mut lv = Lv::new("Büro", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Feldgeräte"));
    lv.push_position(Position::new(
        "",
        "Präsenzmelder Büro-Standard",
        Decimal::ONE,
        "St",
        None,
    ));
    lv.renumber();
    let office = folder.0.join("Buero-Bibliothek.proava");
    XmlProjectFile::new()
        .save_project(
            &office,
            &Project::new(ProjectName::parse("Buero").unwrap(), "2026-11-17", lv),
        )
        .unwrap();
    let launcher = launcher(&folder.0);
    let mut preferences = launcher.preferences();
    preferences
        .libraries
        .push(proava_application::LibrarySetting {
            path: office,
            label: "Büro".into(),
            read_only: true,
        });
    launcher.set_preferences(preferences).unwrap();

    let mut harness = open_demo(&folder.0);
    press(&mut harness, Key::F8);
    type_text(&mut harness, "präsenzmelder");
    assert!(
        shows(&harness, "2 Positionen gefunden"),
        "beide Bibliotheken"
    );
    assert!(shows_part(&harness, "Präsenzmelder Büro-Standard"));
    assert!(shows(&harness, "Büro"), "Kopf der zweiten Bibliothek");
}

#[test]
fn a_library_opened_as_project_shows_its_fields_and_the_lv_is_compared_with_it() {
    use proava_domain::library::LibraryOrigin;
    use proava_ui::StartFile;

    let folder = TempDir::new("library-sync");
    let file = library(&folder.0);
    // Opened as project: the edit panel has the library fields.
    let mut library_window = harness(&folder.0, Some(StartFile::Project(file.clone())));
    select_first_position(&mut library_window);
    assert!(
        shows(&library_window, "Stichwörter (mit Komma getrennt)"),
        "Bibliotheksfelder"
    );
    assert!(shows(&library_window, "Stand auf heute"));
    drop(library_window);

    // A project with a position from that library, the library text changed since.
    let library = proava_infrastructure::XmlProjectFile::new()
        .load_project(&file)
        .unwrap();
    let entry = library.lv().positions().next().unwrap().clone();
    let mut lv = Lv::new("GA", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Raumautomation"));
    let mut taken = Position::new("", "Raumbediengerät alt", Decimal::from(12), "St", None);
    taken.set_long_text(RichText::plain("Taster"));
    taken.set_origin(Some(LibraryOrigin {
        library: "Meine-Bibliothek".into(),
        entry: entry.id(),
        as_of: "01.01.2026".into(),
    }));
    lv.push_position(taken);
    lv.renumber();
    let project = Project::new(ProjectName::parse("Schule").unwrap(), "2026-11-17", lv);
    let path = save(&folder.0, &project);
    let mut harness = harness(&folder.0, Some(StartFile::Project(path)));
    select_first_position(&mut harness);
    assert!(
        shows_part(
            &harness,
            "Aus der Bibliothek „Meine-Bibliothek“, Stand 01.01.2026"
        ),
        "Herkunft"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::K);
    type_text(&mut harness, "mit bibliothek abgleichen");
    press(&mut harness, Key::Enter);
    assert!(
        shows(&harness, "Mit Bibliothek abgleichen"),
        "Fenster offen"
    );
    assert!(
        shows_part(&harness, "Kurztext: Raumbediengerät alt → Raumbediengerät"),
        "Unterschied"
    );
    click(&mut harness, "Bibliotheksstand übernehmen (1)");
    let position = project_of(&harness)
        .lv()
        .positions()
        .next()
        .unwrap()
        .clone();
    assert_eq!(position.short_text(), "Raumbediengerät");
    assert!(position.long_text().to_plain().contains("Farbdisplay"));
    assert_eq!(position.quantity(), Decimal::from(12), "Menge bleibt");
    assert!(shows(&harness, "Alles auf dem Stand der Bibliothek."));
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project_of(&harness)
            .lv()
            .positions()
            .next()
            .unwrap()
            .short_text(),
        "Raumbediengerät alt",
        "ein Rückgängig-Schritt"
    );
}

fn project_of(harness: &UiHarness) -> &Project {
    harness.state().project().expect("Projekt offen")
}
