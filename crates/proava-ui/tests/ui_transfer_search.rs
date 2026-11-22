//! „Übernehmen aus Projekt, GAEB oder Bibliothek“ with the LV search:
//! search text, „In allen Projekten unter … suchen“ (folder choice, search
//! on a worker thread, old versions left out), a click on a hit loads that
//! project as the source with the hit shown, „Treffer ankreuzen“ and
//! „Übernehmen“ bring it into the open LV.

mod common;

use std::path::Path;

use common::*;
use proava_application::ProjectRepository;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName, RichText};
use proava_infrastructure::XmlProjectFile;

fn sample(name: &str, short: &str, long: &str) -> Project {
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Raumautomation"));
    lv.push_group(LvGroup::new(1, "", "Bediengeräte"));
    let mut position = Position::new("", short, Decimal::from(4), "St", None);
    position.set_long_text(RichText::plain(long));
    lv.push_position(position);
    lv.push_position(Position::new(
        "",
        "Präsenzmelder Zz",
        Decimal::ONE,
        "St",
        None,
    ));
    lv.renumber();
    Project::new(ProjectName::parse(name).unwrap(), "2026-11-18", lv)
}

fn store(folder: &Path, project: &Project) {
    std::fs::create_dir_all(folder).unwrap();
    XmlProjectFile::new()
        .save_project(
            &folder.join(format!("{}.proava", project.name().as_str())),
            project,
        )
        .unwrap();
}

fn search_field(harness: &mut UiHarness) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Suche")
        })
        .next()
        .expect("Suchfeld im Übernehmen-Fenster")
        .click();
    settle(harness);
}

fn click_part(harness: &mut UiHarness, text: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.contains(text)))
        .next()
        .unwrap_or_else(|| panic!("„…{text}…“ fehlt"))
        .click();
    settle(harness);
}

#[test]
fn projects_below_a_folder_are_searched_and_a_hit_is_taken_over() {
    let folder = TempDir::new("uebernehmen-suche");
    let projects = folder.0.join("Projekte");
    store(
        &projects.join("Schule"),
        &sample(
            "Schule",
            "Raumbediengerät Typ Qx7",
            "Taster mit Display für KNX, Farbe reinweiß, Temperaturfühler eingebaut.",
        ),
    );
    store(
        &projects.join("00- Alte Versionen"),
        &sample("Altstand", "Raumbediengerät Typ Qx7", "Display alt"),
    );
    store(
        &projects.join("Buero"),
        &sample("Buero", "Lichtschalter Qx9", "ohne Bildschirm"),
    );
    std::fs::write(projects.join("Kaputt.proava"), b"kein Projekt").unwrap();
    let mut harness = open_demo(&folder.0);
    let before = project_positions(&harness);

    press_with(&mut harness, Modifiers::COMMAND, Key::K);
    type_text(&mut harness, "aus anderem projekt");
    press(&mut harness, Key::Enter);
    assert!(shows(
        &harness,
        "Übernehmen aus Projekt, GAEB oder Bibliothek"
    ));

    // Words from the long text, umlauts written out.
    search_field(&mut harness);
    type_text(&mut harness, "display reinweiss");
    click(&mut harness, "In allen Projekten unter … suchen");
    assert!(
        shows(&harness, "Projekte unter diesem Ordner durchsuchen"),
        "Ordnerwahl"
    );
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Pfad")
        })
        .next()
        .expect("Pfadfeld")
        .click();
    settle(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, &projects.to_string_lossy());
    press(&mut harness, Key::Enter);
    // The button names the folder shown (no subfolder chosen).
    let name = projects.file_name().unwrap().to_string_lossy().to_string();
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Ordner „") && label.ends_with("“ wählen"))
        })
        .next()
        .unwrap_or_else(|| panic!("Knopf „Ordner „{name}“ wählen“"))
        .click();
    settle(&mut harness);
    for _ in 0..400 {
        harness.step();
        if shows_part(&harness, "durchsucht") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    settle(&mut harness);
    assert!(
        shows_part(
            &harness,
            "„display reinweiss“: 1 Treffer in 1 Projekt unter"
        ),
        "Ergebnis"
    );
    assert!(
        shows_part(&harness, "2 Projekte durchsucht, 1 Datei nicht lesbar"),
        "Alte Versionen übergangen, kaputte Datei genannt"
    );
    assert!(shows(&harness, "Schule (1 Treffer)"));
    assert!(!shows_part(&harness, "Altstand"), "Alte Versionen");
    assert!(
        shows(&harness, "In allen Projekten unter „Projekte“ suchen"),
        "Ordner gemerkt"
    );

    // A click on the hit loads the project as the source and shows it.
    click(&mut harness, "01.01.0010  Raumbediengerät Typ Qx7");
    assert!(shows_part(&harness, "Schule.proava"), "Quelle geladen");
    assert!(
        shows_part(&harness, "Farbe reinweiß, Temperaturfühler eingebaut."),
        "Langtext unten"
    );
    assert!(shows(&harness, "1 Treffer"), "Suche gilt in der Quelle");
    assert!(
        !shows_part(&harness, "Präsenzmelder Zz"),
        "nur Treffer und Titel darüber"
    );
    click(&mut harness, "Treffer ankreuzen");
    click_part(&mut harness, "Übernehmen (1 Position, 1 Zeile)");
    // The demo LV has bids: the taken over position needs a „Trotzdem einfügen“.
    keep_despite_bids(&mut harness, "einfügen");
    let after = project_positions(&harness);
    assert_eq!(after.len(), before.len() + 1);
    assert!(after.iter().any(|text| text == "Raumbediengerät Typ Qx7"));

    click(&mut harness, "Liste schließen");
    assert!(!shows(&harness, "Schule (1 Treffer)"));
}

fn project_positions(harness: &UiHarness) -> Vec<String> {
    project(harness)
        .lv()
        .positions()
        .map(|position| position.short_text().to_string())
        .collect()
}
