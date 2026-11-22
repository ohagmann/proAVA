//! Reiter „Nachträge“: Liste mit Summen, Statuswechsel mit Datum und
//! Verlauf, neuer Nachtrag mit Position im Nachtragsbereich, Rückgängig.

mod common;

use common::*;
use proava_domain::ChangeOrderStatus;
use proava_ui::StartFile;

fn open(folder: &std::path::Path) -> UiHarness {
    let project = proava_ui::demo_change_orders::change_order_demo_project();
    let file = save(folder, &project);
    let mut harness = harness(folder, Some(StartFile::Project(file)));
    press_with(&mut harness, Modifiers::COMMAND, Key::Num5);
    assert!(shows(&harness, "Nachtragsliste"), "Reiter Nachträge offen");
    harness
}

fn status_of(harness: &UiHarness, number: u32) -> ChangeOrderStatus {
    project(harness)
        .change_orders()
        .iter()
        .find(|order| order.number == number)
        .unwrap()
        .status
}

#[test]
fn list_shows_sums_by_status_and_forecast() {
    let folder = TempDir::new("co-list");
    let mut harness = open(&folder.0);
    assert!(shows(&harness, "Summe (6 Nachträge)"));
    let overview = proava_domain::change_order_overview(
        project(&harness).contract(),
        project(&harness).change_orders(),
    );
    assert!(
        shows(&harness, &overview.offered.to_string()),
        "angeboten gesamt"
    );
    assert!(
        shows(&harness, &overview.total_with_change_orders.to_string()),
        "Auftragssumme neu"
    );
    assert!(shows(&harness, &overview.forecast.to_string()), "Prognose");
    assert!(shows(&harness, "Summen je Status"));
    assert!(shows(&harness, "beauftragt") && shows(&harness, "abgelehnt"));

    click(&mut harness, "Nachtragsübersicht");
    assert!(shows(&harness, "Gutschrift"));
    assert!(shows(&harness, &overview.forecast.to_string()));
}

#[test]
fn status_change_with_date_history_and_undo() {
    let folder = TempDir::new("co-status");
    let mut harness = open(&folder.0);
    click(&mut harness, "N04");
    assert!(shows_part(&harness, "N04 Beschleunigung Inbetriebnahme"));
    assert_eq!(status_of(&harness, 4), ChangeOrderStatus::Offered);
    let steps_before = project(&harness).change_orders()[3].history.len();

    click(&mut harness, "Status ändern …");
    assert!(shows(&harness, "Neuer Status"), "Dialog offen");
    // Vorschlag: der nächste Status (in Prüfung), Datum heute.
    click(&mut harness, "Übernehmen");
    assert_eq!(status_of(&harness, 4), ChangeOrderStatus::InReview);
    let order = &project(&harness).change_orders()[3];
    assert_eq!(order.history.len(), steps_before + 1);
    assert!(
        !order.history.last().unwrap().date.is_empty(),
        "Datum im Verlauf"
    );
    assert!(shows(&harness, "in Prüfung"));

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert_eq!(
        status_of(&harness, 4),
        ChangeOrderStatus::Offered,
        "Rückgängig"
    );
    assert_eq!(
        project(&harness).change_orders()[3].history.len(),
        steps_before
    );
}

#[test]
fn new_change_order_with_position_in_the_change_order_area() {
    let folder = TempDir::new("co-new");
    let mut harness = open(&folder.0);
    let rows_before = project(&harness).lv().rows().len();
    click(&mut harness, "Neuer Nachtrag");
    let order = project(&harness).change_orders().last().unwrap().clone();
    assert_eq!(order.label(), "N07");
    assert_eq!(order.status, ChangeOrderStatus::Announced);

    click(&mut harness, "Neue Position");
    assert!(
        shows_part(&harness, "Neue Position in"),
        "Formular mit Nachtragsbereich"
    );
    type_text(&mut harness, "Zusatzfühler Technikraum");
    // The button of the form (the menu bar has „Einfügen“ too).
    harness.get_all_by_label("Einfügen").last().unwrap().click();
    settle(&mut harness);
    click(&mut harness, "Abbrechen");
    let order = project(&harness).change_orders().last().unwrap().clone();
    assert_eq!(order.items.len(), 1);
    let lv = project(&harness).lv();
    let position = lv.position_by_id(order.items[0].position).unwrap();
    assert_eq!(position.short_text(), "Zusatzfühler Technikraum");
    // Ein Titel „N07 …“ im vorhandenen Sammeltitel „Nachträge“ und die Position.
    assert_eq!(lv.rows().len(), rows_before + 2);
    assert!(
        position.ordinal().ends_with(".0010"),
        "{}",
        position.ordinal()
    );
    let contract = project(&harness).contract().unwrap();
    for (row, item) in &contract.items {
        assert_eq!(
            lv.position_by_id(*row).unwrap().ordinal(),
            item.ordinal,
            "Auftrags-OZ unverändert"
        );
    }

    // Rückgängig nimmt Position und Bereich wieder aus dem LV.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut harness);
    assert!(project(&harness)
        .change_orders()
        .last()
        .unwrap()
        .items
        .is_empty());
    assert_eq!(project(&harness).lv().rows().len(), rows_before);
}

/// Rows the Abrechnung may bill (the table there is painted, so the test
/// reads the same source the view uses).
fn billable(harness: &UiHarness) -> Vec<String> {
    let project = project(harness);
    let context = project.billing_context().expect("Auftrag");
    proava_domain::billable_items(context.contract, context.change_orders, context.lv)
        .values()
        .map(|item| item.short_text.clone())
        .collect()
}

#[test]
fn approved_change_order_positions_appear_in_the_billing() {
    let folder = TempDir::new("co-billing");
    let mut harness = open(&folder.0);
    // Abrechnung: N01 (beauftragt) ist abrechenbar, N03 (in Prüfung) nicht.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num4);
    click(&mut harness, "Neue Rechnung");
    assert_eq!(project(&harness).billing().invoices().len(), 1);
    let rows = billable(&harness);
    assert!(
        rows.iter()
            .any(|text| text.starts_with("Wetterstation mit Wind")),
        "{rows:?}"
    );
    assert!(!rows.iter().any(|text| text == "IO-Erweiterung ASP02"));

    // N03 über „Status ändern …“ prüfen und beauftragen.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num5);
    click(&mut harness, "N03");
    click(&mut harness, "Status ändern …");
    click(&mut harness, "Übernehmen");
    assert_eq!(status_of(&harness, 3), ChangeOrderStatus::Reviewed);
    click(&mut harness, "Status ändern …");
    click(&mut harness, "Übernehmen");
    assert_eq!(status_of(&harness, 3), ChangeOrderStatus::Approved);
    let rows = billable(&harness);
    assert!(
        rows.iter().any(|text| text == "IO-Erweiterung ASP02"),
        "nach Beauftragung abrechenbar"
    );
    assert!(
        !rows.iter().any(|text| text == "Zuschlag Wochenendarbeit"),
        "abgelehnte Position bleibt draußen"
    );
    // Die Abrechnung zeigt den neuen Stand ohne Fehler.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num4);
    assert!(shows(&harness, "Neue Rechnung"));
}
