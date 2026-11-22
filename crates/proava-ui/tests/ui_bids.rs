//! Angebote and Preisspiegel keep computed results between frames: changes
//! made in the program must still show at once.

mod common;

use common::*;
use egui_kittest::kittest::NodeT;

fn click_starting(harness: &mut UiHarness, text: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(text)))
        .next()
        .unwrap_or_else(|| panic!("„{text}…“ fehlt"))
        .click();
    settle(harness);
}

/// A plain label (not the text field itself) contains `text`.
fn label_shows(harness: &UiHarness, text: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::Label
                && node.value().is_some_and(|value| value.contains(text))
        })
        .next()
        .is_some()
}

/// Opens the bidder menu of the Preisspiegel (it lists the names).
fn open_bidder_menu(harness: &mut UiHarness) {
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Bieter ") && label.ends_with('…'))
        })
        .next()
        .expect("Bieter-Menü im Preisspiegel")
        .click();
    settle(harness);
}

#[test]
fn bidder_name_change_shows_in_bid_list_and_comparison() {
    let folder = TempDir::new("bids-cache");
    let mut harness = open_demo(&folder.0);
    // Both views computed once before the change.
    click(&mut harness, "Preisspiegel");
    harness.run_steps(5);
    click_starting(&mut harness, "Angebote (");
    harness.run_steps(5);
    assert!(!harness.state().workspace().unwrap().is_dirty());

    let old = project(&harness)
        .bids()
        .iter()
        .map(|bid| bid.bidder().name1.clone())
        .find(|name| label_shows(&harness, &format!("günstigstes: {name}")))
        .expect("günstigster Bieter in der Zeile über der Liste");
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some(old.as_str())
        })
        .next()
        .expect("Namensfeld des gewählten Bieters")
        .click();
    settle(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Umbenannt Prüfbau GmbH");

    let stored: Vec<String> = project(&harness)
        .bids()
        .iter()
        .map(|bid| bid.bidder().name1.clone())
        .collect();
    assert!(
        stored.contains(&"Umbenannt Prüfbau GmbH".to_string()),
        "{stored:?}"
    );
    assert!(!stored.contains(&old));
    assert!(harness.state().workspace().unwrap().is_dirty());
    harness.run_steps(3);
    assert!(
        label_shows(&harness, "günstigstes: Umbenannt Prüfbau GmbH"),
        "neuer Name über der Angebotsliste"
    );
    assert!(!label_shows(&harness, &format!("günstigstes: {old}")));

    click(&mut harness, "Preisspiegel");
    harness.run_steps(3);
    open_bidder_menu(&mut harness);
    assert!(
        shows_part(&harness, "Umbenannt Prüfbau GmbH (Rang"),
        "neuer Name im Preisspiegel"
    );
    assert!(!shows_part(&harness, &format!("{old} (Rang")));
}

/// Whether the checkbox „Planerpreise“ of the Preisspiegel can be switched.
fn planner_prices_switchable(harness: &UiHarness) -> bool {
    !harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::CheckBox
                && node.label().as_deref() == Some("Planerpreise")
        })
        .next()
        .expect("Schalter Planerpreise")
        .accesskit_node()
        .is_disabled()
}

/// Kostenvoranschlag 0,00 €: keine Spalte „Planerpreis“ – der Schalter ist
/// aus, im Druck fehlen Spalte und Erklärung (Anwender 20.11.).
#[test]
fn without_planner_prices_the_comparison_has_no_planner_column() {
    let folder = TempDir::new("ps-ohne-planerpreise");
    let mut harness = open_demo(&folder.0);
    click(&mut harness, "Preisspiegel");
    harness.run_steps(3);
    assert!(
        planner_prices_switchable(&harness),
        "Demo-LV hat Planerpreise"
    );

    let other = TempDir::new("ps-ohne-planerpreise-2");
    let mut project = proava_ui::demo::demo_project();
    let rows = project.lv().rows().len();
    for index in 0..rows {
        if let Some(proava_domain::LvRow::Position(position)) = project.lv_mut().row_mut(index) {
            position.set_unit_price(None);
        }
    }
    assert!(!project.lv().has_planner_prices());
    let file = save(&other.0, &project);
    let mut harness = common::harness(&other.0, Some(proava_ui::StartFile::Project(file)));
    click(&mut harness, "Preisspiegel");
    harness.run_steps(3);
    assert!(
        !planner_prices_switchable(&harness),
        "ohne Planerpreise aus"
    );
}
