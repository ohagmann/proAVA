//! Projektvorlagen und Startfenster: Neues Projekt aus der GA-Vorlage,
//! Projekt ▸ Als Vorlage speichern …, Vorschau, Kopie und „Pfad suchen …“.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_application::{ProjectRepository, TemplateSource};
use proava_infrastructure::XmlProjectFile;

/// Settings with the template folder (never the real home folder) and an
/// OZ-Maske with a three-digit title level.
fn prepare(folder: &std::path::Path) -> std::path::PathBuf {
    let templates = folder.join("Vorlagen");
    let launcher = launcher(folder);
    let mut preferences = launcher.preferences();
    preferences.template_folder = Some(templates.clone());
    preferences.new_lv_mask = "11.222.33.PPPP".into();
    launcher.set_preferences(preferences).unwrap();
    // New projects land in the test folder, never in the home folder.
    launcher.set_default_folder(Some(folder.to_path_buf()));
    templates
}

/// The dialog's button, not the one of the header with the same name.
fn click_last(harness: &mut UiHarness, label: &str) {
    harness.get_all_by_label(label).last().expect(label).click();
    settle(harness);
}

fn click_role(harness: &mut UiHarness, role: Role) {
    harness
        .query_all_by(|node| node.role() == role)
        .next()
        .unwrap_or_else(|| panic!("{role:?} fehlt"))
        .click();
    settle(harness);
}

fn titles_of(project: &proava_domain::Project) -> Vec<String> {
    project
        .lv()
        .rows()
        .iter()
        .filter_map(proava_domain::LvRow::as_group)
        .map(|group| format!("{} {}", group.ordinal(), group.short_text()))
        .collect()
}

/// The OZ-Maske field of „Neues Projekt“ (its caption is the label).
fn mask_field(harness: &UiHarness, label: &str) -> Option<String> {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.label().as_deref() == Some(label)
        })
        .next()
        .and_then(|node| node.value())
}

fn fill_mask(harness: &mut UiHarness, label: &str, text: &str) {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.label().as_deref() == Some(label)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{label}“ fehlt"))
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, text);
}

/// The GA template brings its own mask 11.222.PPPPI (the settings say
/// 11.222.33.PPPP here): titles 01.481 … 01.489.
#[test]
fn new_project_from_the_ga_template() {
    let folder = TempDir::new("vorlage-neu");
    prepare(&folder.0);
    let mut harness = harness(&folder.0, None);
    click(&mut harness, "Neues Projekt …");
    type_text(&mut harness, "Schule");
    assert_eq!(
        mask_field(&harness, "OZ-Maske:").as_deref(),
        Some("11.222.33.PPPP"),
        "leer: aus den Einstellungen"
    );
    assert!(shows_part(
        &harness,
        "Ein leeres LV mit Schrittweite 10 aus den Einstellungen."
    ));
    click_role(&mut harness, Role::ComboBox);
    click(&mut harness, "GA nach DIN 276 (481–489)");
    assert_eq!(
        mask_field(&harness, "OZ-Maske der Vorlage:").as_deref(),
        Some("11.222.PPPPI"),
        "Maske der Vorlage, nicht der Einstellungen"
    );
    assert!(
        shows_part(&harness, "Schrittweite 10 · 7 Titel · 1 Hinweis"),
        "Zeile zur Vorlage"
    );
    assert!(
        !shows_part(&harness, "dreistellige Stufe"),
        "Maske passt, kein Hinweis"
    );
    click(&mut harness, "Anlegen");
    let project = project(&harness);
    assert_eq!(project.lv().oz_mask().pattern(), "11.222.PPPPI");
    assert_eq!(
        titles_of(project),
        [
            "01 Gebäudeautomation",
            "01.481 Automationseinrichtungen",
            "01.482 Schaltschränke",
            "01.483 Management- und Bedieneinrichtungen",
            "01.484 Kabel und Leitungen",
            "01.485 Netzwerke",
            "01.489 Sonstiges",
        ]
    );
    assert!(folder.0.join("Schule/Schule.proava").is_file());
    let launcher = launcher(&folder.0);
    assert_eq!(
        launcher.last_template().as_deref(),
        Some("GA nach DIN 276 (481–489)"),
        "beim nächsten Mal vorgewählt"
    );
    assert_eq!(
        launcher.preferences().new_lv_mask,
        "11.222.33.PPPP",
        "Einstellung unverändert"
    );
}

/// A two-digit mask typed into the dialog: the GA titles become 01, 02 …,
/// the dialog says so and names the template's mask.
#[test]
fn ga_template_with_two_digit_mask_explains_the_title_numbers() {
    let folder = TempDir::new("vorlage-maske");
    prepare(&folder.0);
    let mut harness = harness(&folder.0, None);
    click(&mut harness, "Neues Projekt …");
    type_text(&mut harness, "Schule");
    click_role(&mut harness, Role::ComboBox);
    click(&mut harness, "GA nach DIN 276 (481–489)");
    fill_mask(&mut harness, "OZ-Maske der Vorlage:", "11.22.PPPPI");
    assert!(
        shows_part(&harness, "Schrittweite 10 · 6 Titel"),
        "Zeile folgt der Maske"
    );
    assert!(shows_part(
        &harness,
        "Die Titel bekommen 01, 02 … (KG 481–489 steht am Titel)"
    ));
    assert!(shows_part(&harness, "Vorgabe der Vorlage 11.222.PPPPI"));
    fill_mask(&mut harness, "OZ-Maske der Vorlage:", "11.x");
    assert!(
        shows_part(&harness, "OZ-Maske: Unbekanntes Zeichen „x“"),
        "Fehler statt Zeile"
    );
    fill_mask(&mut harness, "OZ-Maske der Vorlage:", "11.22.PPPPI");
    click(&mut harness, "Anlegen");
    let project = project(&harness);
    assert_eq!(project.lv().oz_mask().pattern(), "11.22.PPPPI");
    let first = project
        .lv()
        .rows()
        .iter()
        .find_map(proava_domain::LvRow::as_group)
        .unwrap();
    assert_eq!(first.ordinal(), "01");
    assert_eq!(first.cost_group(), Some("481"));
}

/// „leer“ takes the mask typed into the dialog; an own template shows its
/// mask without a field and keeps it.
#[test]
fn empty_and_own_templates_and_the_mask() {
    let folder = TempDir::new("vorlage-eigen");
    let templates = prepare(&folder.0);
    let launcher = launcher(&folder.0);
    let ga = proava_domain::template::ga_template(
        proava_domain::OzMask::parse("111.22.PPPP").unwrap(),
        10,
        proava_domain::Decimal::from(19),
        "2026-11-18T09:00:00",
    );
    launcher
        .save_as_template(&ga, "GA Schulen", false, false)
        .unwrap();

    let mut empty = harness(&folder.0, None);
    click(&mut empty, "Neues Projekt …");
    type_text(&mut empty, "Leer");
    fill_mask(&mut empty, "OZ-Maske:", "11.PPPP");
    click(&mut empty, "Anlegen");
    assert_eq!(
        project(&empty).lv().oz_mask().pattern(),
        "11.PPPP",
        "Maske aus dem Dialog"
    );
    assert_eq!(
        launcher.preferences().new_lv_mask,
        "11.222.33.PPPP",
        "Einstellung unverändert"
    );
    drop(empty);

    let mut own = harness(&folder.0, None);
    click(&mut own, "Neues Projekt …");
    type_text(&mut own, "Eigen");
    click_role(&mut own, Role::ComboBox);
    click(&mut own, "GA Schulen");
    assert!(shows(&own, "OZ-Maske der Vorlage:"));
    assert!(shows(&own, "111.22.PPPP"), "Maske der eigenen Vorlage");
    assert_eq!(
        mask_field(&own, "OZ-Maske der Vorlage:"),
        None,
        "kein Feld: die Vorlage bringt ihre Gliederung mit"
    );
    click(&mut own, "Anlegen");
    assert_eq!(project(&own).lv().oz_mask().pattern(), "111.22.PPPP");
    assert_eq!(titles_of(project(&own))[0], "481 Automationseinrichtungen");

    let file = templates.join("GA Schulen/GA Schulen.proava");
    let refused = launcher
        .create_project_from_template_with_mask(
            &folder.0,
            "Anders",
            &TemplateSource::File(file),
            true,
            Some("11.22.PPPPI"),
        )
        .unwrap_err();
    assert!(
        refused.contains("bringt ihre OZ-Maske mit (111.22.PPPP)"),
        "{refused}"
    );
    assert_eq!(
        launcher.template_mask(&TemplateSource::BuiltIn).unwrap(),
        proava_domain::template::GA_TEMPLATE_MASK
    );
}

#[test]
fn a_project_saved_as_template_starts_the_next_one() {
    let folder = TempDir::new("vorlage-speichern");
    let templates = prepare(&folder.0);
    let mut harness = open_demo(&folder.0);
    let groups = project(&harness).lv().group_count();
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Als Vorlage speichern …");
    assert!(shows_part(&harness, "Nicht mit kommen: Bauherr"));
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "GA Schulen");
    click_last(&mut harness, "Speichern");
    let file = templates.join("GA Schulen/GA Schulen.proava");
    assert!(file.is_file(), "Vorlage im Vorlagenordner");
    let template = XmlProjectFile::new().load_project(&file).unwrap();
    assert_eq!(template.lv().group_count(), groups);
    assert_eq!(
        template.lv().position_count(),
        0,
        "ohne Positionen (Vorgabe)"
    );
    assert!(template.bids().is_empty(), "keine Angebote");
    assert!(template.info().owner.name1.is_empty(), "kein Bauherr");

    // Second try with the same name: asks before replacing.
    click(&mut harness, "Projekt");
    click_menu(&mut harness, "Als Vorlage speichern …");
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "GA Schulen");
    click_last(&mut harness, "Speichern");
    assert!(shows(&harness, "Eine Vorlage „GA Schulen“ gibt es schon."));
    click(&mut harness, "Vorhandene Vorlage gleichen Namens ersetzen");
    click_last(&mut harness, "Speichern");
    assert!(!shows_part(&harness, "Nicht mit kommen"), "Dialog zu");

    let launcher = launcher(&folder.0);
    let names: Vec<String> = launcher
        .templates()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        names,
        [
            "leer",
            "GA nach DIN 276 (481–489)",
            "Kostenschätzung DIN 276 (2. Ebene)",
            "Kostenberechnung DIN 276 (3. Ebene)",
            "GA Schulen"
        ]
    );
    let entry = launcher
        .create_project_from_template(&folder.0, "Neue Schule", &TemplateSource::File(file), true)
        .unwrap();
    let created = XmlProjectFile::new().load_project(&entry.path).unwrap();
    assert_eq!(created.name().as_str(), "Neue Schule");
    assert_eq!(created.lv().group_count(), groups);
}

#[test]
fn start_screen_shows_a_preview_and_copies_a_project() {
    let folder = TempDir::new("vorschau");
    prepare(&folder.0);
    let demo = proava_ui::demo::demo_project();
    let name = demo.name().as_str().to_string();
    let file = save(&folder.0, &demo);
    launcher(&folder.0).open_project(&file).unwrap();
    let mut harness = harness(&folder.0, None);
    assert!(shows_part(&harness, "Ein Projekt in der Liste wählen"));
    click(&mut harness, &name);
    assert!(shows(&harness, "Bauherr"), "Vorschau");
    assert!(shows(&harness, "Speicherort"));
    let net = demo.lv().net_total().to_string();
    assert!(shows(&harness, &net), "Netto des LV: {net}");

    // Rechtsklick ▸ Kopie anlegen …
    // The row (a button), not the name in the preview.
    harness
        .query_all_by(|node| {
            node.role() == Role::Button && node.label().as_deref() == Some(name.as_str())
        })
        .next()
        .expect("Zeile der Liste")
        .click_secondary();
    settle(&mut harness);
    assert!(shows(&harness, "Im Dateimanager zeigen"));
    click(&mut harness, "Kopie anlegen …");
    assert!(
        shows(&harness, &format!("{name} Kopie")),
        "Name vorgeschlagen"
    );
    click_last(&mut harness, "Kopie anlegen");
    let copy = folder.0.join(format!("{name} Kopie/{name} Kopie.proava"));
    assert!(copy.is_file(), "Kopie unter neuem Namen");
    let copied = XmlProjectFile::new().load_project(&copy).unwrap();
    assert_eq!(copied.name().as_str(), format!("{name} Kopie"));
    assert_eq!(
        copied.bids().len(),
        demo.bids().len(),
        "die ganze Kopie mit Angeboten"
    );
    assert_eq!(launcher(&folder.0).recent()[0].path, copy);
}

#[test]
fn right_click_opens_read_only() {
    let folder = TempDir::new("nur-lesen");
    let demo = proava_ui::demo::demo_project();
    let name = demo.name().as_str().to_string();
    let file = save(&folder.0, &demo);
    launcher(&folder.0).open_project(&file).unwrap();
    let mut harness = harness(&folder.0, None);
    harness
        .query_all_by(|node| {
            node.role() == Role::Button && node.label().as_deref() == Some(name.as_str())
        })
        .next()
        .expect("Zeile der Liste")
        .click_secondary();
    settle(&mut harness);
    click(&mut harness, "Schreibgeschützt öffnen");
    let workspace = harness.state().workspace().expect("Projekt offen");
    assert!(workspace.is_read_only(), "nur lesen und kopieren");
}

#[test]
fn a_missing_project_can_be_found_again() {
    let folder = TempDir::new("pfad-suchen");
    let demo = proava_ui::demo::demo_project();
    let file = save(&folder.0, &demo);
    launcher(&folder.0).open_project(&file).unwrap();
    let moved = folder.0.join("verschoben.proava");
    std::fs::rename(&file, &moved).unwrap();

    let mut harness = harness(&folder.0, None);
    assert!(shows(&harness, "Pfad suchen …"), "Knopf in der Zeile");
    let name = demo.name().as_str().to_string();
    click(&mut harness, &name);
    assert!(shows_part(&harness, "Die Projektdatei fehlt."));

    let launcher = launcher(&folder.0);
    launcher.relocate(&file, &moved).unwrap();
    let recent: Vec<_> = launcher
        .recent()
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    assert_eq!(recent, [moved]);
}
