//! Verknüpfung zur Ausführungsbeschreibung auf einen Blick (Wunsch
//! 18.11.2026): Zeile „Ausführung nach A0001 – …“ im Reiter „Allgemein“ des
//! Fensters „Eigenschaften“, oben in der Seitenleiste und im Kopf des großen
//! Langtextfensters; Anzeigen, Ändern …, Lösen, Verknüpfen … und Strg+Z.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{
    Decimal, Lv, LvGroup, LvRow, OzMask, PerformanceDescription, Position, Project, ProjectName,
    RichText, RowId,
};

const LINE: &str = "Ausführung nach A0001 – Kabeltrassen";

/// Rows of the test LV.
struct Rows {
    first: RowId,
    second: RowId,
    linked: RowId,
    free: RowId,
}

/// A small LV: two descriptions, one position linked to the first, one free.
fn project_file(folder: &std::path::Path, panel: bool) -> (std::path::PathBuf, Rows) {
    std::fs::write(
        folder.join("settings.json"),
        format!(r#"{{"preferences":{{"detail_visible":{panel}}}}}"#),
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Trassen"));
    let first = lv.push_row(LvRow::Performance(PerformanceDescription::new(
        "1",
        "Kabeltrassen",
        RichText::plain("Verzinkt."),
    )));
    let second = lv.push_row(LvRow::Performance(PerformanceDescription::new(
        "2",
        "Kabelkanäle",
        RichText::plain("Aus Kunststoff."),
    )));
    let linked = lv.push_position(Position::new("01.0010", "Trasse", Decimal::ONE, "m", None));
    let free = lv.push_position(Position::new("01.0020", "Kanal", Decimal::ONE, "m", None));
    lv.link_performance(linked, Some(first)).unwrap();
    let project = Project::new(
        ProjectName::parse("Verknuepfung").unwrap(),
        "2026-11-18",
        lv,
    );
    let file = save(folder, &project);
    (
        file,
        Rows {
            first,
            second,
            linked,
            free,
        },
    )
}

fn open(folder: &std::path::Path, file: std::path::PathBuf) -> UiHarness {
    let harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    assert!(harness.state().project().is_some(), "Projekt offen");
    harness
}

/// Moves the table selection to `row`.
fn select(harness: &mut UiHarness, row: RowId) {
    press(harness, Key::Home);
    for _ in 0..20 {
        let at = selected(harness).unwrap();
        if project(harness).lv().rows()[at].id() == row {
            return;
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile nicht erreicht");
}

fn link_of(harness: &UiHarness, position: RowId) -> Option<RowId> {
    project(harness)
        .lv()
        .position_by_id(position)
        .unwrap()
        .performance()
}

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().position_window_row()
}

/// Clicks the last button named `label` (the window's when panel and window
/// both have one).
fn click_last(harness: &mut UiHarness, label: &str) {
    harness
        .query_all_by(|node| node.role() == Role::Button && node.label().as_deref() == Some(label))
        .last()
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .click();
    settle(harness);
}

fn undo(harness: &mut UiHarness) {
    press_with(harness, Modifiers::COMMAND, Key::Z);
}

#[test]
fn the_window_shows_the_link_on_the_general_tab() {
    let folder = TempDir::new("ab-zeile-fenster");
    let (file, rows) = project_file(&folder.0, false);
    let mut harness = open(&folder.0, file);
    select(&mut harness, rows.linked);
    assert!(
        !shows(&harness, LINE),
        "ohne Seitenleiste und Fenster noch nicht zu sehen"
    );
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(window_row(&harness), Some(rows.linked));
    assert!(shows(&harness, "Allgemein"));
    assert!(
        shows(&harness, LINE),
        "Zeile mit Nummer und Bezeichnung im Reiter Allgemein"
    );

    // Lösen, then Strg+Z.
    click_last(&mut harness, "Lösen");
    assert_eq!(link_of(&harness, rows.linked), None, "Verknüpfung gelöst");
    assert!(shows(&harness, "keine Ausführungsbeschreibung"));
    assert!(!shows(&harness, LINE));
    undo(&mut harness);
    assert_eq!(
        link_of(&harness, rows.linked),
        Some(rows.first),
        "Strg+Z bringt sie zurück"
    );
    assert!(shows(&harness, LINE));

    // Ändern …: the same choice as the field on „Weiteres“.
    click_last(&mut harness, "Ändern …");
    click(&mut harness, "A0002 – Kabelkanäle");
    assert_eq!(
        link_of(&harness, rows.linked),
        Some(rows.second),
        "geändert"
    );
    assert!(shows(&harness, "Ausführung nach A0002 – Kabelkanäle"));
    undo(&mut harness);
    assert_eq!(
        link_of(&harness, rows.linked),
        Some(rows.first),
        "Strg+Z nimmt die Änderung zurück"
    );

    // Anzeigen: the window (and the table) go to the description.
    click_last(&mut harness, "Anzeigen");
    assert_eq!(
        window_row(&harness),
        Some(rows.first),
        "Fenster zeigt die Ausführungsbeschreibung"
    );
}

#[test]
fn a_free_position_can_be_linked_from_the_general_tab() {
    let folder = TempDir::new("ab-zeile-verknuepfen");
    let (file, rows) = project_file(&folder.0, false);
    let mut harness = open(&folder.0, file);
    select(&mut harness, rows.free);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(
        shows(&harness, "keine Ausführungsbeschreibung"),
        "dezenter Hinweis"
    );
    click_last(&mut harness, "Verknüpfen …");
    click(&mut harness, "A0001 – Kabeltrassen");
    assert_eq!(link_of(&harness, rows.free), Some(rows.first));
    assert!(shows(&harness, LINE));
    assert_eq!(
        link_of(&harness, rows.linked),
        Some(rows.first),
        "die andere Position bleibt"
    );
    undo(&mut harness);
    assert_eq!(link_of(&harness, rows.free), None, "Strg+Z löst sie wieder");
}

#[test]
fn the_edit_panel_shows_the_link_at_the_top() {
    let folder = TempDir::new("ab-zeile-seitenleiste");
    let (file, rows) = project_file(&folder.0, true);
    let mut harness = open(&folder.0, file);
    select(&mut harness, rows.linked);
    assert!(shows(&harness, LINE), "Seitenleiste zeigt die Verknüpfung");
    // Above the Kurztext field: the line comes before the field in the panel.
    let line_top = harness.get_by_label(LINE).rect().top();
    let short_top = harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some("Trasse")
        })
        .next()
        .expect("Kurztext-Feld")
        .rect()
        .top();
    assert!(
        line_top < short_top,
        "oben bei der Position, vor dem Kurztext"
    );

    click_last(&mut harness, "Lösen");
    assert_eq!(link_of(&harness, rows.linked), None);
    undo(&mut harness);
    assert_eq!(link_of(&harness, rows.linked), Some(rows.first));

    select(&mut harness, rows.free);
    assert!(shows(&harness, "keine Ausführungsbeschreibung"));
    assert!(!shows(&harness, LINE));
}

#[test]
fn the_long_text_window_names_the_link_in_its_head() {
    let folder = TempDir::new("ab-zeile-langtext");
    let (file, rows) = project_file(&folder.0, false);
    let mut harness = open(&folder.0, file);
    select(&mut harness, rows.linked);
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    assert!(shows_part(&harness, "Langtext – "), "Langtextfenster offen");
    assert!(shows(&harness, LINE), "Kopf des Langtextfensters");
    press(&mut harness, Key::Escape);

    select(&mut harness, rows.free);
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    assert!(shows_part(&harness, "Langtext – "));
    assert!(
        !shows_part(&harness, "Ausführung nach"),
        "ohne Verknüpfung nichts"
    );
}
