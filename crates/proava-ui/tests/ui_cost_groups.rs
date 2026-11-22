//! Kostengruppen in the editor: field „Kostengruppe“ and the question
//! before a title's group changes finished positions.

mod common;

use common::*;
use proava_domain::LvRow;

/// Picks an entry of the list „Kostengruppe (DIN 276)“ in the properties.
fn choose_cost_group(harness: &mut UiHarness, entry: &str) {
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Liste …") || node.value().as_deref() == Some("Liste …")
        })
        .next()
        .expect("Liste der Kostengruppen")
        .click();
    settle(harness);
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.trim() == entry))
        .next()
        .unwrap_or_else(|| panic!("Eintrag „{entry}“ fehlt"))
        // The list may reach below the test window: click without pointer.
        .click_accesskit();
    settle(harness);
}

fn title_group(harness: &UiHarness, index: usize) -> Option<String> {
    match &project(harness).lv().rows()[index] {
        LvRow::Group(group) => group.cost_group().map(str::to_string),
        _ => panic!("kein Titel"),
    }
}

#[test]
fn title_group_over_finished_positions_asks_first() {
    let folder = TempDir::new("kg");
    let mut harness = open_demo(&folder.0);
    let position = select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(project(&harness).lv().rows()[position]
        .as_position()
        .unwrap()
        .is_locked());

    // Up to the title of the position.
    let mut title = position;
    while !project(&harness).lv().rows()[title].is_group() {
        press(&mut harness, Key::ArrowUp);
        title = selected(&harness).expect("gewählte Zeile");
    }
    assert_eq!(title_group(&harness, title), None);

    choose_cost_group(&mut harness, "481  Automationseinrichtungen");
    assert!(
        shows(&harness, "Kostengruppe des Titels ändern?"),
        "Rückfrage offen"
    );
    assert!(shows_part(&harness, "fertige Position"), "Anzahl genannt");
    assert_eq!(title_group(&harness, title), None, "noch nicht geändert");
    click(&mut harness, "Abbrechen");
    assert!(!shows(&harness, "Kostengruppe des Titels ändern?"));
    assert_eq!(title_group(&harness, title), None, "abgebrochen");

    choose_cost_group(&mut harness, "481  Automationseinrichtungen");
    click(&mut harness, "KG ändern");
    assert_eq!(title_group(&harness, title), Some("481".into()));
    assert!(shows(&harness, "481"), "Feld zeigt die neue KG");
    assert_eq!(
        project(&harness).lv().effective_cost_group(position),
        Some(("481".into(), true)),
        "die fertige Position erbt die KG"
    );
    assert!(project(&harness).lv().rows()[position]
        .as_position()
        .unwrap()
        .is_locked());

    // Strg+Z takes it back.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(title_group(&harness, title), None);
}
