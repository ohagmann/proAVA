//! Anwender 18.11.: Bieter beim Angebotsimport ins Bieterverzeichnis, Menge
//! nach Angebotseingang geändert (Massenmehrung statt Rechenfehler) und
//! Rückfrage, wenn ein versandtes LV seine Ordnungszahlen ändert.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use proava_domain::{Decimal, Project};

/// An X84 of the demo LV from the firm `name` (all prices of the demo LV).
fn bid_file(folder: &Path, file: &str, name: &str) -> PathBuf {
    let project = proava_ui::demo::demo_project();
    let (mut bid, _) = proava_exchange::lv_to_gaeb(&project, proava_gaeb::Phase::Dp84);
    bid.phase = proava_gaeb::Phase::Dp84;
    bid.award.bidder = Some(proava_gaeb::model::Address {
        name1: name.into(),
        city: "Musterstadt".into(),
        ..Default::default()
    });
    let path = folder.join(file);
    std::fs::write(
        &path,
        proava_gaeb::xml::write(&bid, proava_gaeb::Phase::Dp84).unwrap(),
    )
    .unwrap();
    path
}

/// Picks `file` in the file dialog: path into „Pfad“, Enter, the file, „Öffnen“.
fn pick_file(harness: &mut UiHarness, file: &Path) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Pfad")
        })
        .next()
        .expect("Pfadfeld")
        .click();
    settle(harness);
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, &file.parent().unwrap().to_string_lossy());
    press(harness, Key::Enter);
    click(harness, &file.file_name().unwrap().to_string_lossy());
    click(harness, "Öffnen");
}

/// Angebote ▸ Angebote einlesen … with `file`, waits for the result dialog.
fn read_bid(harness: &mut UiHarness, file: &Path) {
    click(harness, "Angebote");
    click_menu(harness, "Angebote einlesen (X84/D84) …");
    pick_file(harness, file);
    for _ in 0..400 {
        if shows(harness, "Angebote eingelesen") {
            settle(harness);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        harness.step();
    }
    panic!("Dialog „Angebote eingelesen“ erscheint nicht");
}

fn newest_bid(harness: &UiHarness) -> proava_domain::BidId {
    project(harness)
        .bids()
        .iter()
        .map(|bid| bid.id())
        .max()
        .unwrap()
}

#[test]
fn reading_bids_puts_the_firm_into_the_bidder_list_unless_unticked() {
    let folder = TempDir::new("bieter-aufnehmen");
    let mueller = bid_file(&folder.0, "Mueller.X84", "Müller Regeltechnik GmbH");
    let schulze = bid_file(&folder.0, "Schulze.X84", "Schulze GA KG");
    let mut harness = open_demo(&folder.0);
    let listed = project(&harness).invitees().len();

    read_bid(&mut harness, &mueller);
    assert!(
        shows(&harness, "Ins Bieterverzeichnis aufnehmen"),
        "Haken im Dialog"
    );
    // Was gelesen wurde: eine Angabe je Zeile (Bezeichnung und Wert).
    for label in [
        "Datei",
        "Angebotssumme netto",
        "Positionen ohne EP",
        "Rechenfehler",
    ] {
        assert!(shows(&harness, label), "Zeile „{label}“ fehlt");
    }
    assert!(shows(&harness, "Mueller.X84"), "Datei als Wert");
    click(&mut harness, "Fertig");
    let id = newest_bid(&harness);
    let entry = project(&harness)
        .invitee_of_bid(id)
        .expect("Bieter angelegt und verknüpft")
        .clone();
    assert_eq!(entry.name(), "Müller Regeltechnik GmbH");
    assert_eq!(entry.address.city, "Musterstadt");
    assert_eq!(project(&harness).invitees().len(), listed + 1);

    // Haken abgewählt: kein Eintrag.
    read_bid(&mut harness, &schulze);
    click(&mut harness, "Ins Bieterverzeichnis aufnehmen");
    click(&mut harness, "Fertig");
    let id = newest_bid(&harness);
    assert!(
        project(&harness).invitee_of_bid(id).is_none(),
        "nicht aufgenommen"
    );
    assert_eq!(project(&harness).invitees().len(), listed + 1);

    // Strg+Z nimmt die Aufnahme mit zurück.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(project(&harness).invitees().len() <= listed + 1);
}

/// The demo project with the LV marked as sent to the bidders (X83 export).
fn sent_demo(folder: &Path) -> UiHarness {
    let mut project = proava_ui::demo::demo_project();
    project
        .lv_mut()
        .mark_sent("2026-11-18T09:00:00".into(), None);
    let file = save(folder, &project);
    harness(folder, Some(proava_ui::StartFile::Project(file)))
}

fn rows(harness: &UiHarness) -> usize {
    project(harness).lv().rows().len()
}

#[test]
fn moving_order_numbers_of_a_sent_lv_asks_first() {
    let folder = TempDir::new("versandt");
    let mut harness = sent_demo(&folder.0);
    assert!(!shows(
        &harness,
        "Seit dem Versand geändert – neu versenden"
    ));
    let index = select_first_position(&mut harness);
    let before = rows(&harness);

    // Verschieben ändert OZ, aber keine Position kommt oder geht: nur die
    // Rückfrage nach dem Versand, auch mit den vier Angeboten des Demo-LV.
    press_with(&mut harness, Modifiers::ALT, Key::ArrowDown);
    settle(&mut harness);
    assert!(shows(&harness, "LV bereits versandt"), "Rückfrage Versand");
    assert!(
        !shows_part(&harness, "Angebote –"),
        "Angebote nicht betroffen"
    );
    click(&mut harness, "Abbrechen");
    assert_eq!(selected(&harness), Some(index), "zurück an ihrem Platz");
    assert!(!project(&harness).lv().changed_since_sent());

    // Duplizieren schiebt die OZ der folgenden Positionen und bringt eine
    // Position, die den Angeboten fehlt: eine Rückfrage mit beiden Gründen.
    press_with(&mut harness, Modifiers::COMMAND, Key::D);
    settle(&mut harness);
    assert!(shows(&harness, "LV hat bereits Angebote"), "Rückfrage");
    assert!(shows_part(
        &harness,
        "Das LV hat bereits 4 Angebote – die neue Position fehlt dort."
    ));
    assert!(
        shows_part(&harness, "Das LV wurde am 18.11.2026 versandt. Die OZ ab"),
        "Text"
    );
    assert!(shows_part(
        &harness,
        "neu versendet werden. Trotzdem einfügen?"
    ));
    click(&mut harness, "Abbrechen");
    assert_eq!(
        rows(&harness),
        before,
        "Abbrechen nimmt die Änderung zurück"
    );
    assert!(!project(&harness).lv().changed_since_sent());

    press_with(&mut harness, Modifiers::COMMAND, Key::D);
    settle(&mut harness);
    click(&mut harness, "Trotzdem einfügen");
    assert_eq!(rows(&harness), before + 1);
    settle(&mut harness);
    assert!(
        shows(&harness, "Seit dem Versand geändert – neu versenden"),
        "dauerhafter Hinweis"
    );

    // Bis zum nächsten Versand bzw. neuen Angeboten keine weitere Rückfrage.
    press_with(&mut harness, Modifiers::COMMAND, Key::D);
    settle(&mut harness);
    assert!(!shows(&harness, "LV bereits versandt"));
    assert!(!shows(&harness, "LV hat bereits Angebote"));
    assert_eq!(rows(&harness), before + 2);
}

#[test]
fn content_changes_of_a_sent_lv_only_show_the_hint() {
    let folder = TempDir::new("versandt-inhalt");
    let mut harness = sent_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = rows(&harness);
    // Neue Kurztext über die Tabelle: keine OZ ändert sich.
    type_text(&mut harness, "Geänderter Kurztext");
    press(&mut harness, Key::Enter);
    settle(&mut harness);
    assert!(!shows(&harness, "LV bereits versandt"), "keine Rückfrage");
    assert_eq!(rows(&harness), before);
    assert_eq!(
        project(&harness).lv().rows()[index].short_text(),
        "Geänderter Kurztext"
    );
    assert!(shows(&harness, "Seit dem Versand geändert – neu versenden"));
}

/// Demo project whose first priced position got a bigger LV quantity after
/// the bids came in; `award` gives the contract to the first bid.
fn changed_quantity_demo(folder: &Path, award: bool) -> (UiHarness, String) {
    let mut project: Project = proava_ui::demo::demo_project();
    let (index, ordinal, quantity) = {
        let lv = project.lv();
        let position = lv
            .positions()
            .find(|position| !position.quantity().is_zero())
            .unwrap();
        (
            lv.index_of(position.id()).unwrap(),
            position.ordinal().to_string(),
            position.quantity(),
        )
    };
    let position = project
        .lv_mut()
        .row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap();
    position.set_quantity(quantity + Decimal::from(2));
    if award {
        let bid = project.bids()[0].id();
        project.award_bid(bid, "18.11.2026").unwrap();
    }
    let file = save(folder, &project);
    (
        harness(folder, Some(proava_ui::StartFile::Project(file))),
        ordinal,
    )
}

#[test]
fn quantity_changed_after_the_bids_is_a_mass_increase_in_the_price_comparison() {
    let folder = TempDir::new("massenmehrung");
    let (mut harness, ordinal) = changed_quantity_demo(&folder.0, false);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    assert!(
        shows_part(&harness, "Achtung Massenmehrung nach Angebotseingang"),
        "Hinweisleiste"
    );
    assert!(shows_part(&harness, &ordinal));
    assert!(
        !shows(&harness, "Als Mehr-/Mindermenge in Nachtrag übernehmen …"),
        "ohne Auftrag kein Nachtrag"
    );
    // Rechenfehler wie vorher (das Demo-Projekt hat absichtlich einen) – die
    // geänderte Menge macht keinen neuen.
    let errors = |project: &Project| -> Vec<usize> {
        proava_domain::PriceComparison::build(
            project.lv(),
            project.bids(),
            proava_domain::ComparisonOptions::default(),
        )
        .bids
        .iter()
        .map(|summary| summary.arithmetic_errors)
        .collect()
    };
    assert_eq!(
        errors(project(&harness)),
        errors(&proava_ui::demo::demo_project()),
        "kein neues RF"
    );
}

#[test]
fn awarded_bid_takes_the_mass_increase_into_a_change_order() {
    let folder = TempDir::new("massenmehrung-nachtrag");
    let (mut harness, _) = changed_quantity_demo(&folder.0, true);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num3);
    click(
        &mut harness,
        "Als Mehr-/Mindermenge in Nachtrag übernehmen …",
    );
    let project = project(&harness);
    let contract = project.contract().expect("Auftrag übernommen");
    let order = project.change_orders().last().expect("Nachtrag angelegt");
    assert_eq!(order.title, "Massenmehrung nach Angebotseingang");
    assert_eq!(order.items.len(), 1);
    assert_eq!(order.items[0].offered_quantity, Decimal::from(2));
    assert!(contract.item(order.items[0].position).is_some());
    assert!(
        shows_part(&harness, "als Mehr-/Mindermenge angelegt"),
        "Meldung"
    );
}
