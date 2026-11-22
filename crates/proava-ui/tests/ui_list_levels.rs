//! Listen mit Unterpunkten im Langtext (Anwender 18.11.2026: Zulieferungen
//! originalgetreu): Tab rückt einen Listenpunkt eine Ebene ein, Umschalt+Tab
//! aus; Strg+Z nimmt es zurück. Im Text außerhalb einer Liste tut Tab nichts.

mod common;

use common::*;

/// A single click at a screen point (press and release in two frames).
fn click_at(harness: &mut UiHarness, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

/// Clicks into the running text of the Langtext editor (below its toolbar).
fn click_into_long_text(harness: &mut UiHarness) {
    let toolbar = harness
        .get_all_by_label("Tabelle einfügen …")
        .next()
        .expect("Werkzeugleiste des Langtexts")
        .rect();
    click_at(
        harness,
        egui::pos2(toolbar.left() + 40.0, toolbar.bottom() + 70.0),
    );
}

/// List items of the Langtext: text and level.
fn items(harness: &UiHarness, index: usize) -> Vec<(String, u8)> {
    project(harness).lv().rows()[index]
        .as_position()
        .expect("Position")
        .long_text()
        .blocks()
        .iter()
        .filter_map(|block| {
            let (_, level) = block.list_kind()?;
            Some((proava_domain::runs_text(block.runs()?), level))
        })
        .collect()
}

#[test]
fn tab_and_shift_tab_move_a_list_item_one_level_in_and_out() {
    let folder = TempDir::new("listenebenen");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = items(&harness, index);

    click_into_long_text(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    press(&mut harness, Key::Enter);
    // Tab in a plain paragraph: nothing happens, the text stays.
    type_text(&mut harness, "Punkt");
    press(&mut harness, Key::Tab);
    assert_eq!(
        items(&harness, index),
        before,
        "Tab im Absatz ändert nichts"
    );
    click(
        &mut harness,
        "Aufzählung (noch einmal: normaler Absatz; Tab/Umschalt+Tab: Unterpunkt ein/aus)",
    );
    click_into_long_text(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "Unterpunkt");
    press(&mut harness, Key::Tab);
    let mut expected = before.clone();
    expected.push(("Punkt".to_string(), 0));
    expected.push(("Unterpunkt".to_string(), 1));
    assert_eq!(items(&harness, index), expected, "Tab: eine Ebene ein");

    press(&mut harness, Key::Tab);
    expected.last_mut().unwrap().1 = 2;
    assert_eq!(items(&harness, index), expected, "noch eine Ebene");

    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    expected.last_mut().unwrap().1 = 1;
    assert_eq!(items(&harness, index), expected, "Umschalt+Tab: eine aus");

    // Strg+Z: back to level 2.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    expected.last_mut().unwrap().1 = 2;
    assert_eq!(items(&harness, index), expected, "Strg+Z");

    // Enter on an empty Unterpunkt goes one level out (word processor).
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "zurück");
    expected.push(("zurück".to_string(), 1));
    assert_eq!(
        items(&harness, index),
        expected,
        "leerer Unterpunkt + Enter"
    );
}
