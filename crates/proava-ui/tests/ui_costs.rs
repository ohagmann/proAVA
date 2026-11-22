//! Reiter Kosten: Kostenermittlung anlegen, Anlagenliste mit der Tastatur
//! füllen, Rückgängig, Kostenberechnung aus Kostenschätzung, Druckdialog.

mod common;

use common::*;
use proava_domain::{CostStage, Money};

fn open_costs(folder: &TempDir) -> UiHarness {
    let mut harness = open_demo(&folder.0);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num6);
    harness
}

#[test]
fn new_calculation_and_typing_in_the_plant_list() {
    let folder = TempDir::new("kosten");
    let mut harness = open_costs(&folder);
    assert!(shows(&harness, "Noch keine Kostenermittlung im Projekt"));
    click(&mut harness, "Kostenberechnung nach Datenpunkten");
    let estimate = &project(&harness).cost_estimates()[0];
    assert_eq!(estimate.stage, CostStage::Calculation);
    assert_eq!(
        estimate.sections[0].rows.len(),
        1,
        "Vorlage: ein Bauteil mit einer leeren Zeile"
    );
    assert!(shows(&harness, "Gesamtkosten KG 481–489 (netto)"));

    // „+ Zeile“ opens the new line at „Ort“; Tab walks right, Enter ends at the last line.
    click(&mut harness, "Zeile");
    type_text(&mut harness, "ASP01");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "RLT");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "RLT-01 Klassen");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "12");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "1.030");
    press(&mut harness, Key::Enter);

    let estimate = &project(&harness).cost_estimates()[0];
    let row = &estimate.sections[0].rows[1];
    assert_eq!(
        (
            row.location.as_str(),
            row.trade.as_str(),
            row.plant.as_str()
        ),
        ("ASP01", "RLT", "RLT-01 Klassen")
    );
    assert_eq!(row.counts[..2], [12, 1030]);
    assert_eq!(
        estimate.result().total,
        Money::from_cents((12 + 1030) * 40_000)
    );
    assert!(shows_part(&harness, "416.800,00 €"), "Summe sichtbar");

    // A wrong count stays in the cell with a message, nothing changes.
    click(&mut harness, "Zeile");
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "zwölf");
    press(&mut harness, Key::Tab);
    assert!(
        shows_part(&harness, "Anzahl Datenpunkte"),
        "Meldung zur Anzahl"
    );
    press(&mut harness, Key::Escape);
    assert_eq!(
        project(&harness).cost_estimates()[0].sections[0].rows[2].points(),
        0
    );

    // Strg+Z takes the inserted line back, then the last typed count.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness).cost_estimates()[0].sections[0].rows.len(),
        2
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness).cost_estimates()[0].sections[0].rows[1].count(1),
        0
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    assert_eq!(
        project(&harness).cost_estimates()[0].sections[0].rows[1].count(1),
        1030
    );
}

#[test]
fn calculation_from_estimate_and_print_dialog() {
    let folder = TempDir::new("kosten-druck");
    let mut harness = open_costs(&folder);
    // The menu „Kosten“ (the tab of the same name is shown while it is open).
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Kosten") && node.role() != egui::accesskit::Role::Tab
        })
        .next()
        .expect("Menü Kosten")
        .click();
    settle(&mut harness);
    click_menu(&mut harness, "Neue Kostenschätzung nach Datenpunkten (GA)");
    assert_eq!(
        project(&harness).cost_estimates()[0].stage,
        CostStage::Estimate
    );
    assert!(shows(
        &harness,
        "KG 480 Gebäude- und Anlagenautomation (netto)"
    ));
    click(&mut harness, "Kostenberechnung daraus");
    let estimates = project(&harness).cost_estimates();
    assert_eq!(estimates.len(), 2);
    assert_eq!(estimates[1].stage, CostStage::Calculation);
    assert_eq!(
        estimates[1].sections, estimates[0].sections,
        "Anlagenliste übernommen"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::P);
    assert!(
        shows(&harness, "Deckblatt mit den Kennzahlen"),
        "Druckdialog mit den Kosten-Optionen"
    );
    assert!(
        shows_part(&harness, "Kostenberechnung"),
        "die gezeigte Kostenermittlung ist gewählt"
    );
    click(&mut harness, "Abbrechen");

    // Undo removes the new Berechnung again and stays in the Reiter Kosten.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num1);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).cost_estimates().len(), 1);
    assert!(
        shows(&harness, "KG 480 Gebäude- und Anlagenautomation (netto)"),
        "Reiter Kosten gezeigt"
    );
}

/// Rectangle of the (first) widget with exactly this label or text.
fn rect_of(harness: &UiHarness, text: &str) -> egui::Rect {
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(text) || node.value().as_deref() == Some(text)
        })
        .next()
        .unwrap_or_else(|| panic!("„{text}“ fehlt"))
        .rect()
}

/// The result column docks right under the button row and keeps its place
/// when the DP-Arten open (Anwender 18.11.: „bis oben andocken“); in a narrow
/// window it moves under the plant list. The DP-Arten show the whole
/// Bezeichnung and no „virtuell“ any more.
#[test]
fn result_column_docks_under_the_buttons() {
    let folder = TempDir::new("kosten-spalte");
    let file = save(&folder.0, &proava_ui::demo::cost_demo_project());
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num6);
    let result = "Gesamtkosten KG 481–489 (netto)";
    let buttons = rect_of(&harness, "Drucken …");
    let side = rect_of(&harness, result);
    assert!(side.top() > buttons.bottom(), "unter der Knopfzeile");
    assert!(
        side.top() - buttons.bottom() < 40.0,
        "direkt darunter: {side:?} / {buttons:?}"
    );
    assert!(
        side.left() > rect_of(&harness, "Anlagenliste").right() + 300.0,
        "rechts neben der Liste"
    );

    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("DP-Arten und Preise"))
        })
        .next()
        .expect("DP-Arten")
        .click();
    settle(&mut harness);
    let field = rect_of(&harness, "Analoge Eingabe");
    assert!(
        field.width() >= 300.0,
        "Bezeichnung mit freier Breite: {field:?}"
    );
    assert!(!shows(&harness, "virtuell"), "kein Haken „virtuell“ mehr");
    let opened = rect_of(&harness, result);
    assert!(
        (opened.top() - side.top()).abs() < 1.0,
        "Spalte bleibt oben: {opened:?} / {side:?}"
    );
    assert!(field.right() < opened.left(), "DP-Arten links der Spalte");

    harness.set_size(egui::vec2(900.0, 800.0));
    settle(&mut harness);
    let narrow = rect_of(&harness, result);
    assert!(
        narrow.top() > rect_of(&harness, "Anlagenliste").bottom(),
        "schmal: unter der Liste"
    );
    assert!(narrow.left() < 100.0, "schmal: volle Breite {narrow:?}");
}
