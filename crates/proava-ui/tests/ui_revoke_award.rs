//! Angebote ▸ Auftrag zurücknehmen …: Rückfrage je nach Datenlage, ein
//! Rückgängig-Schritt (Durchsicht 18.11.2026).

mod common;

use common::*;
use egui::accesskit::Role;
use proava_ui::StartFile;

fn open_menu(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name) && node.role() == Role::Button)
        .next()
        .unwrap_or_else(|| panic!("Menü {name}"))
        .click();
    settle(harness);
}

fn revoke(harness: &mut UiHarness) {
    open_menu(harness, "Angebote");
    click_menu(harness, "Auftrag zurücknehmen …");
}

/// The demo project with the first bid awarded.
fn awarded_demo() -> proava_domain::Project {
    let mut demo = proava_ui::demo::demo_project();
    let bid = demo.bids()[0].id();
    demo.award_bid(bid, "01.12.2026").expect("Angebot da");
    demo
}

fn open(folder: &std::path::Path, project: &proava_domain::Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(harness.state().project().is_some());
    harness
}

#[test]
fn a_plain_award_is_taken_back_and_undone() {
    let folder = TempDir::new("auftrag-zurueck");
    let mut harness = open(&folder.0, &awarded_demo());
    let awarded = project(&harness)
        .awarded_bid()
        .expect("beauftragt")
        .name()
        .to_string();
    revoke(&mut harness);
    assert!(shows(&harness, "Auftrag zurücknehmen?"));
    assert!(
        shows_part(&harness, &format!("Auftrag an {awarded} vom")),
        "nennt Bieter und Datum"
    );
    harness
        .get_all_by_label("Zurücknehmen")
        .last()
        .unwrap()
        .click();
    settle(&mut harness);
    assert!(project(&harness).awarded_bid().is_none(), "zurückgenommen");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness)
            .awarded_bid()
            .map(|bid| bid.name().to_string()),
        Some(awarded.clone())
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    assert!(project(&harness).awarded_bid().is_none(), "wiederholt");
}

#[test]
fn a_fixed_contract_goes_along_and_comes_back_with_undo() {
    let folder = TempDir::new("auftrag-fest");
    let mut demo = awarded_demo();
    demo.take_over_contract().expect("beauftragt");
    let mut harness = open(&folder.0, &demo);
    revoke(&mut harness);
    assert!(
        shows_part(&harness, "festgeschrieben"),
        "warnt vor der Festschreibung"
    );
    harness
        .get_all_by_label("Zurücknehmen")
        .last()
        .unwrap()
        .click();
    settle(&mut harness);
    assert!(project(&harness).awarded_bid().is_none());
    assert!(
        project(&harness).contract().is_none(),
        "Festschreibung mit zurückgenommen"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(project(&harness).awarded_bid().is_some(), "Auftrag zurück");
    assert!(
        project(&harness).contract().is_some(),
        "Festschreibung zurück – ein Schritt"
    );
}

#[test]
fn invoices_or_change_orders_block_it() {
    let folder = TempDir::new("auftrag-gesperrt");
    let mut harness = open(&folder.0, &proava_ui::demo::billing_demo_project());
    revoke(&mut harness);
    assert!(shows(&harness, "Auftrag lässt sich nicht zurücknehmen"));
    assert!(shows_part(&harness, "Rechnung"), "nennt die Rechnungen");
    assert!(
        !shows(&harness, "Zurücknehmen"),
        "kein Knopf zum Zurücknehmen"
    );
    press(&mut harness, Key::Escape);
    assert!(project(&harness).awarded_bid().is_some(), "Auftrag bleibt");
    assert!(project(&harness).contract().is_some());
}

#[test]
fn the_entry_is_off_without_an_award() {
    let folder = TempDir::new("auftrag-keiner");
    let mut harness = open(&folder.0, &proava_ui::demo::demo_project());
    revoke(&mut harness);
    assert!(
        !shows(&harness, "Auftrag zurücknehmen?"),
        "ausgegraut, keine Rückfrage"
    );
}
