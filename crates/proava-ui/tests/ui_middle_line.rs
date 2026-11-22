//! Text und Knopf einer Zeile liegen auf einer gemeinsamen Mittellinie
//! (Wunsch 18.11.2026, `widgets::row`): egui zentrierte einen Text nur in der
//! Zeilenhöhe, die beim Einfügen bekannt war – vor einem höheren Knopf saß
//! er zu hoch.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{
    Decimal, Lv, LvGroup, LvRow, OzMask, PerformanceDescription, Position, Project, ProjectName,
    RichText, RowId,
};

/// One position linked to „Kabeltrassen“, one free; without the edit panel
/// (`panel` false) only the window shows the rows' fields.
fn open(folder: &std::path::Path, panel: bool) -> (UiHarness, RowId, RowId) {
    std::fs::write(
        folder.join("settings.json"),
        format!(r#"{{"preferences":{{"detail_visible":{panel}}}}}"#),
    )
    .unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Trassen"));
    let ab = lv.push_row(LvRow::Performance(PerformanceDescription::new(
        "1",
        "Kabeltrassen",
        RichText::plain("Verzinkt."),
    )));
    let linked = lv.push_position(Position::new("01.0010", "Trasse", Decimal::ONE, "m", None));
    let free = lv.push_position(Position::new("01.0020", "Kanal", Decimal::ONE, "m", None));
    lv.link_performance(linked, Some(ab)).unwrap();
    let project = Project::new(ProjectName::parse("Mittellinie").unwrap(), "2026-11-18", lv);
    let file = save(folder, &project);
    let harness = harness(folder, Some(proava_ui::StartFile::Project(file)));
    (harness, linked, free)
}

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

/// Vertical middles of every text (label or value) `text`.
fn text_middles(harness: &UiHarness, text: &str) -> Vec<f32> {
    harness
        .query_all_by(|node| {
            node.role() != Role::Button
                && (node.label().as_deref() == Some(text) || node.value().as_deref() == Some(text))
        })
        .map(|node| node.rect().center().y)
        .collect()
}

/// Vertical middle of the (last) button `label`.
fn button_middle(harness: &UiHarness, label: &str) -> f32 {
    harness
        .query_all_by(|node| node.role() == Role::Button && node.label().as_deref() == Some(label))
        .last()
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .rect()
        .center()
        .y
}

/// The text `text` next to the button `button` shares its middle (±1 px).
fn assert_same_middle(harness: &UiHarness, text: &str, button: &str) {
    let button_y = button_middle(harness, button);
    let nearest = text_middles(harness, text)
        .into_iter()
        .map(|y| (y - button_y).abs())
        .fold(f32::INFINITY, f32::min);
    assert!(
        nearest <= 1.0,
        "„{text}“ und Knopf „{button}“: Mitten {nearest:.1} px auseinander"
    );
}

#[test]
fn the_link_line_centres_text_and_buttons() {
    let folder = TempDir::new("mitte-ab");
    let (mut harness, linked, free) = open(&folder.0, true);
    // Edit panel: without a link, text and „Verknüpfen …“.
    select(&mut harness, free);
    assert_same_middle(&harness, "keine Ausführungsbeschreibung", "Verknüpfen …");
    // Window „Eigenschaften“ (wide): text and buttons in one line.
    select(&mut harness, linked);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_same_middle(&harness, "Ausführung nach A0001 – Kabeltrassen", "Anzeigen");
    assert_same_middle(&harness, "Ausführung nach A0001 – Kabeltrassen", "Lösen");
}

#[test]
fn the_head_of_the_position_window_centres_kind_and_close() {
    let folder = TempDir::new("mitte-kopf");
    let (mut harness, linked, _) = open(&folder.0, false);
    select(&mut harness, linked);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_same_middle(&harness, "Position", "Schließen (Esc)");
    assert_same_middle(&harness, "01.0010", "Schließen (Esc)");
}

#[test]
fn the_head_of_the_long_text_window_centres_kind_link_and_close() {
    let folder = TempDir::new("mitte-langtext");
    let (mut harness, linked, _) = open(&folder.0, false);
    select(&mut harness, linked);
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    assert_same_middle(&harness, "Langtext", "Schließen (Esc)");
    assert_same_middle(
        &harness,
        "Ausführung nach A0001 – Kabeltrassen",
        "Schließen (Esc)",
    );
}
