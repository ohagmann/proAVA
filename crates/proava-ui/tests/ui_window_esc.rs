//! Esc wirkt sofort nach dem Öffnen, ohne erst ins Fenster zu klicken
//! (Anwender 18.11.2026: „ein Klick zu viel“): Positionsfenster per
//! Doppelklick, Alt+Enter, Kontextmenü und nach dem Blättern, auch für eine
//! Ausführungsbeschreibung; großes Langtextfenster; Druckdialog,
//! Schriftverkehr, Kosten. The harness shows the own windows embedded (one
//! viewport); that the native window gets the keyboard focus at all is
//! `window_focus::FocusRequest` (unit tests there, proof on the test screen).

mod common;

use common::*;
use proava_domain::RowId;

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().position_window_row()
}

fn dialog_open(harness: &UiHarness) -> bool {
    harness.state().workspace().unwrap().dialog_open()
}

/// Name of a table row as the table reports it (OZ and Kurztext).
fn row_label(harness: &UiHarness, index: usize) -> String {
    let row = &project(harness).lv().rows()[index];
    format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string()
}

/// Opens a menu of the menu bar (not the tab „LV“ of the same name) and
/// clicks an entry.
fn menu(harness: &mut UiHarness, menu: &str, entry: &str) {
    menu_path(harness, menu, entry);
}

#[test]
fn the_position_window_closes_with_esc_right_after_opening() {
    let folder = TempDir::new("esc-positionsfenster");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();

    // Doppelklick (real pointer events), then Esc – no click in between.
    let label = row_label(&harness, index);
    double_click(&mut harness, &label);
    assert_eq!(window_row(&harness), Some(id), "Doppelklick öffnet");
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt nach Doppelklick");

    // Alt+Enter.
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(window_row(&harness), Some(id), "Alt+Enter öffnet");
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt nach Alt+Enter");

    // Kontextmenü (right click with the pointer on the row).
    let at = harness
        .get_all_by_label(&label)
        .next()
        .expect("Tabellenzeile")
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    settle(&mut harness);
    open_submenu(&mut harness, "Bearbeiten");
    click_menu(&mut harness, "Eigenschaften im eigenen Fenster …");
    assert_eq!(window_row(&harness), Some(id), "Kontextmenü öffnet");
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt nach Kontextmenü");

    // Blättern: after „Nächste Position“ Esc still closes.
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Nächste Position");
    assert_ne!(window_row(&harness), Some(id), "nächste Position");
    assert!(window_row(&harness).is_some());
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt nach dem Blättern");
}

#[test]
fn an_ausfuehrungsbeschreibung_window_closes_with_esc() {
    let folder = TempDir::new("esc-ab");
    let mut harness = open_demo(&folder.0);
    // A new one at the first position (Strg+Umschalt+A), then back to the table.
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    press(&mut harness, Key::Escape);
    press(&mut harness, Key::Home);
    for _ in 0..60 {
        let at = selected(&harness).unwrap();
        if project(&harness).lv().rows()[at].is_performance() {
            break;
        }
        press(&mut harness, Key::ArrowDown);
    }
    let at = selected(&harness).unwrap();
    assert!(
        project(&harness).lv().rows()[at].is_performance(),
        "Demo hat eine Ausführungsbeschreibung"
    );
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(window_row(&harness).is_some());
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt");
}

#[test]
fn the_long_text_window_closes_with_esc_right_after_opening() {
    let folder = TempDir::new("esc-langtext");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::SHIFT, Key::F2);
    assert!(shows_part(&harness, "Langtext – "), "Langtextfenster offen");
    press(&mut harness, Key::Escape);
    assert!(!shows_part(&harness, "Langtext – "), "Esc schließt");
}

#[test]
fn dialogs_close_with_esc_right_after_opening() {
    let folder = TempDir::new("esc-dialoge");
    let mut harness = open_demo(&folder.0);
    let opened: [(&str, &str); 3] = [
        ("Drucken", "Drucken …"),
        ("Angebote", "Schriftverkehr (Anschreiben an Bieter) …"),
        (
            "Angebote",
            "Schriftverkehr im Projekt (abgelegte Schreiben) …",
        ),
    ];
    for (top, entry) in opened {
        menu(&mut harness, top, entry);
        assert!(dialog_open(&harness), "{entry} öffnet");
        press(&mut harness, Key::Escape);
        assert!(!dialog_open(&harness), "{entry}: Esc schließt sofort");
    }
    press_with(&mut harness, Modifiers::COMMAND, Key::P);
    assert!(dialog_open(&harness), "Strg+P");
    press(&mut harness, Key::Escape);
    assert!(!dialog_open(&harness), "Druckdialog: Esc schließt");
}

#[test]
fn cost_windows_close_with_esc_right_after_opening() {
    let folder = TempDir::new("esc-kosten");
    let mut harness = open_demo(&folder.0);
    menu(&mut harness, "Prüfen", "Kosten nach Kostengruppen …");
    assert!(
        shows(&harness, "Kosten nach Kostengruppen"),
        "Fenster offen"
    );
    press(&mut harness, Key::Escape);
    assert!(
        !shows(&harness, "Kosten nach Kostengruppen"),
        "Esc schließt"
    );

    // Printing is under Drucken (once in the menu bar).
    menu(
        &mut harness,
        "Drucken",
        "Kostenschätzung / Kostenberechnung (DIN 276) …",
    );
    if dialog_open(&harness) {
        press(&mut harness, Key::Escape);
        assert!(
            !dialog_open(&harness),
            "Kostenermittlung drucken: Esc schließt"
        );
    }
}

/// Every floating window over the LV closes with Esc right after opening
/// (Anwender 18.11.: „alle“), opened through the command palette (Strg+K).
#[test]
fn floating_windows_close_with_esc_right_after_opening() {
    let folder = TempDir::new("esc-schwebend");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    let windows: [(&str, &str); 6] = [
        ("rechtschreibung prüfen", "Rechtschreibung prüfen"),
        (
            "mit anderem stand vergleichen",
            "Mit anderem Stand vergleichen",
        ),
        (
            "aus anderem projekt",
            "Übernehmen aus Projekt, GAEB oder Bibliothek",
        ),
        ("mit bibliothek abgleichen", "Mit Bibliothek abgleichen"),
        ("offene punkte", "Offene Punkte"),
        ("kosten nach kostengruppen", "Kosten nach Kostengruppen"),
    ];
    // The dictionary loads in the background (system Hunspell de_DE).
    let mut speller = false;
    for _ in 0..100 {
        harness.step();
        if proava_ui::spell::speller(&harness.ctx).is_some() {
            speller = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    for (query, title) in windows {
        if title == "Rechtschreibung prüfen" && !speller {
            eprintln!("Kein Wörterbuch auf diesem Rechner: „{title}“ übersprungen");
            continue;
        }
        press_with(&mut harness, Modifiers::COMMAND, Key::K);
        type_text(&mut harness, query);
        press(&mut harness, Key::Enter);
        assert!(shows(&harness, title), "„{title}“ offen (Befehl „{query}“)");
        press(&mut harness, Key::Escape);
        assert!(!shows(&harness, title), "„{title}“: Esc schließt sofort");
    }
}
