//! Rechtsklick in die gerade bearbeitete Zelle der LV-Tabelle: das Zeilenmenü
//! erscheint (das Eingabefeld liegt über der Zeile und hat den Klick bisher
//! geschluckt – nur „Keine Rechtschreibfehler hier.“). Rechtschreibvorschläge
//! nur, wenn unter der Maus ein markiertes Wort steht, dann oben, das
//! Zeilenmenü darunter (Durchsicht 18.11.2026).

mod common;

use common::*;
use egui::accesskit::Role;
use egui::Rect;

fn secondary_click(harness: &mut UiHarness, pos: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

/// The edit box of the cell being edited.
fn edit_box(harness: &UiHarness) -> Rect {
    harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .map(|node| node.rect())
        .filter(|rect| rect.top() > 80.0 && rect.left() > 300.0)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Eingabefeld der Zelle")
}

fn row_menu_shown(harness: &UiHarness) -> bool {
    shows(harness, "Leistungsverzeichnis") && shows(harness, "Verknüpfen")
}

/// Waits for the dictionary of the system (loaded in the background); false
/// when there is none.
fn speller_ready(harness: &mut UiHarness) -> bool {
    for _ in 0..300 {
        if proava_ui::spell::speller(&harness.ctx).is_some() {
            return true;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

#[test]
fn right_click_into_the_edited_short_text_shows_the_row_menu() {
    let folder = TempDir::new("zellmenue");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let rows = project(&harness).lv().rows().len();
    press(&mut harness, Key::F2);
    type_text(&mut harness, "Neue Leuchte");
    let field = edit_box(&harness);
    // Right of the text: no word there.
    secondary_click(
        &mut harness,
        egui::pos2(field.right() - 12.0, field.center().y),
    );
    assert!(row_menu_shown(&harness), "Zeilenmenü im Eingabefeld");
    assert!(!shows(&harness, "Keine Rechtschreibfehler hier."));
    assert!(
        !shows(&harness, "Ignorieren"),
        "kein Rechtschreibteil ohne Wort"
    );

    // A command from there takes the typing over first.
    open_submenu(&mut harness, "Bearbeiten");
    click_menu(&mut harness, "Duplizieren");
    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows + 1, "dupliziert");
    assert_eq!(
        lv.rows()[index].short_text(),
        "Neue Leuchte",
        "Eingabe übernommen"
    );
    assert_eq!(
        lv.rows()[index + 1].short_text(),
        "Neue Leuchte",
        "Kopie mit dem neuen Text"
    );
}

#[test]
fn a_marked_word_gets_suggestions_above_the_row_menu() {
    let folder = TempDir::new("zellmenue-wort");
    let mut harness = open_demo(&folder.0);
    if !speller_ready(&mut harness) {
        eprintln!("kein deutsches Wörterbuch auf diesem Rechner – Vorschläge nicht geprüft");
        return;
    }
    select_first_position(&mut harness);
    press(&mut harness, Key::F2);
    // The word being typed is not marked yet; the one before is.
    type_text(&mut harness, "Leuchtte mit Gehäuse");
    harness.run_steps(4);
    let field = edit_box(&harness);
    secondary_click(
        &mut harness,
        egui::pos2(field.left() + 16.0, field.center().y),
    );
    assert!(shows(&harness, "Leuchte"), "Vorschlag zum markierten Wort");
    assert!(shows(&harness, "Ignorieren"));
    assert!(row_menu_shown(&harness), "Zeilenmenü darunter");
    let suggestion = harness
        .get_all_by_label("Ignorieren")
        .next()
        .unwrap()
        .rect();
    let row_menu = harness
        .get_all_by_label("Leistungsverzeichnis")
        .next()
        .unwrap()
        .rect();
    assert!(suggestion.top() < row_menu.top(), "Vorschläge oben");
    // Choosing the suggestion replaces the word.
    click(&mut harness, "Leuchte");
    press(&mut harness, Key::Enter);
    assert!(
        project(&harness)
            .lv()
            .rows()
            .iter()
            .any(|row| row.short_text() == "Leuchte mit Gehäuse"),
        "Vorschlag übernommen"
    );
}
