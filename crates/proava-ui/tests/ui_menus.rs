//! Menüs aus einem Guss: was es als Knopf in einer Ansicht gibt, steht auch im
//! Menü (Durchsicht 18.11.2026).

mod common;

use common::*;
use egui::accesskit::Role;

fn open_menu(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name) && node.role() == Role::Button)
        .next()
        .unwrap_or_else(|| panic!("Menü {name}"))
        .click();
    settle(harness);
}

#[test]
fn a_bid_by_hand_is_also_in_the_menu() {
    let folder = TempDir::new("menue-angebot");
    let mut harness = open_demo(&folder.0);
    open_menu(&mut harness, "Angebote");
    click_menu(&mut harness, "Angebot von Hand anlegen …");
    assert_eq!(
        harness.state().workspace().unwrap().tab(),
        proava_ui::Tab::Bids
    );
    assert!(
        shows_part(&harness, "Für einen Bieter ohne GAEB-Datei"),
        "Dialog offen"
    );
    let before = project(&harness).bids().len();
    type_text(&mut harness, "Muster Regeltechnik");
    press(&mut harness, Key::Enter);
    assert_eq!(
        project(&harness).bids().len(),
        before + 1,
        "Angebot angelegt"
    );
}
