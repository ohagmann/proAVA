//! Bieterangaben im Langtext (Anwender, 18.11.2026: „Felder einfügen unter der
//! Leistungsposition oder unter dem Langtext – Angabe des Bieters, Fabrikat
//! des Bieters …“): an der Schreibmarke mit Strg+Umschalt+B, unter dem
//! Langtext über den Knopf der Leiste; das Feld ist eine Einheit
//! (Rücktaste nimmt es ganz), Strg+Z holt es zurück.

mod common;

use common::*;
use proava_domain::{ComplementKind, TextComplement};

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

fn complements(harness: &UiHarness, index: usize) -> Vec<TextComplement> {
    project(harness).lv().rows()[index]
        .as_position()
        .expect("Position")
        .complements()
        .to_vec()
}

fn long_text(harness: &UiHarness, index: usize) -> String {
    project(harness).lv().rows()[index]
        .as_position()
        .expect("Position")
        .long_text()
        .to_plain()
}

#[test]
fn bidder_field_goes_in_at_the_caret_is_one_unit_and_comes_back_with_undo() {
    let folder = TempDir::new("bieterangabe");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = complements(&harness, index);

    click_into_long_text(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    press(&mut harness, Key::Enter);
    type_text(&mut harness, "Angebotenes Gerät, ");
    let text = long_text(&harness, index);
    assert!(text.ends_with("Angebotenes Gerät, "), "{text:?}");
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    assert!(shows(&harness, "Bieterangabe einfügen"), "Dialog offen");
    click(&mut harness, "Fabrikat:");
    // Enter übernimmt – auch direkt nach dem Klick auf die Schnellauswahl.
    press(&mut harness, Key::Enter);
    assert!(!shows(&harness, "Bieterangabe einfügen"), "Dialog zu");

    let after = complements(&harness, index);
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    let field = after.last().unwrap();
    assert_eq!(field.caption, "Fabrikat: ", "mit Leerzeichen wie STLB-Bau");
    assert_eq!(field.kind, ComplementKind::Bidder);
    assert!(!field.marker.trim().is_empty());
    let anchor = field.anchor.expect("Stelle im Text");
    assert!(
        !anchor.own_line,
        "an der Schreibmarke im Absatz: {anchor:?}"
    );
    assert_eq!(
        long_text(&harness, index),
        text,
        "der Langtext selbst bleibt"
    );

    // Rücktaste direkt hinter dem Feld nimmt das ganze Feld, keinen Buchstaben.
    press(&mut harness, Key::Backspace);
    assert_eq!(
        complements(&harness, index),
        before,
        "Feld als Einheit entfernt"
    );
    assert_eq!(long_text(&harness, index), text);

    // Strg+Z: das Feld ist wieder da.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(complements(&harness, index).len(), before.len() + 1);
    assert_eq!(long_text(&harness, index), text);
}

#[test]
fn bidder_field_below_the_long_text_from_the_toolbar() {
    let folder = TempDir::new("bieterangabe-unten");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = complements(&harness, index).len();
    let text = long_text(&harness, index);

    harness
        .get_all_by_label("Bieterangabe unter dem Langtext …")
        .next()
        .expect("Knopf in der Leiste")
        .click();
    settle(&mut harness);
    click(&mut harness, "Typ:");

    let after = complements(&harness, index);
    assert_eq!(after.len(), before + 1, "{after:?}");
    let field = after.last().unwrap();
    assert_eq!(field.caption, "Typ: ");
    assert_eq!(field.anchor, None, "unter dem Langtext = nach dem Text");
    assert_eq!(
        field.print_text(),
        "Typ: …………",
        "gedruckt ohne „(vom Bieter)“"
    );
    assert_eq!(
        field.field_label(),
        "Typ: ………… (vom Bieter)",
        "im Editor mit"
    );
    assert_eq!(long_text(&harness, index), text);

    // Angabe des Auftraggebers mit Wert über den Dialog.
    harness
        .get_all_by_label("Bieterangabe unter dem Langtext …")
        .next()
        .unwrap()
        .click();
    settle(&mut harness);
    click(&mut harness, "Angabe des Auftraggebers einfügen …");
    assert!(shows(&harness, "Angabe des Auftraggebers einfügen"));
    click(&mut harness, "Nennweite:");
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Wert")
        })
        .next()
        .expect("Feld Wert")
        .click();
    settle(&mut harness);
    type_text(&mut harness, "DN 50");
    // The button of the dialog (the menu bar has „Einfügen“ too).
    harness.get_all_by_label("Einfügen").last().unwrap().click();
    settle(&mut harness);
    let owner = complements(&harness, index)
        .into_iter()
        .find(|complement| complement.kind == ComplementKind::Owner)
        .expect("Angabe des Auftraggebers");
    assert_eq!(owner.print_text(), "Nennweite: DN 50");
}
