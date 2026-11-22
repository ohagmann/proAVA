//! Menüleiste (Anwender 18.11.): Reihenfolge wie in den meisten Programmen,
//! jedes Menü passt bei 768 px Fensterhöhe ohne Scrollen. Ist das Fenster
//! noch niedriger, öffnet ein Menü trotzdem direkt unter der Menüleiste, ohne
//! sie zu überdecken; was nicht passt, scrollt: Pfeil unten, Überfahren
//! scrollt, dann Pfeil oben, Mausrad und Pfeiltasten.

mod common;

use common::*;
use egui::Rect;

/// Rect of the node whose label starts with `text` (menu entries carry their
/// shortcut in the name).
fn entry(harness: &UiHarness, text: &str) -> Rect {
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label == text || label.starts_with(&format!("{text} ")))
        })
        .next()
        .unwrap_or_else(|| panic!("„{text}“ fehlt"))
        .rect()
}

/// The menu button „LV“ in the menu bar (not the tab „LV“ above it).
fn lv_menu(harness: &UiHarness) -> Rect {
    menu_bar_entry(harness, strings::MENU_LV)
}

fn shows_arrow(harness: &UiHarness, name: &str) -> Option<Rect> {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name))
        .next()
        .map(|node| node.rect())
}

use proava_ui::strings;

/// A window so low that even the menu „LV“ (about 15 entries) scrolls.
const LOW: f32 = 360.0;

#[test]
fn long_menu_opens_below_the_bar_and_scrolls() {
    let folder = TempDir::new("menue-scrollt");
    let mut harness = open_demo(&folder.0);
    harness.set_size(egui::vec2(1366.0, LOW));
    settle(&mut harness);
    let bar = lv_menu(&harness);
    let window_bottom = LOW;
    harness
        .query_all_by(|node| node.label().as_deref() == Some(strings::MENU_LV))
        .find(|node| node.rect() == bar)
        .unwrap()
        .click();
    harness.run_steps(6);

    // Opens below its entry in the bar; nothing reaches above the bar.
    let opened = entry(&harness, "Neues LV …");
    assert!(
        opened.top() >= bar.bottom(),
        "Menü unter der Menüleiste: {opened:?}, Leiste {bar:?}"
    );
    let up = shows_arrow(&harness, "Nach oben scrollen").expect("Menü scrollt: Pfeilzeile oben");
    let down = shows_arrow(&harness, "Nach unten scrollen").expect("Pfeilzeile unten");
    assert!(
        up.top() >= bar.bottom() - 0.5,
        "Oberkante {up:?} unter der Menüleiste {bar:?}"
    );
    assert!(down.bottom() <= window_bottom, "unten im Fenster: {down:?}");
    let first = entry(&harness, "Neues LV …");
    assert!(
        first.top() >= up.bottom() - 0.5,
        "erster Eintrag sichtbar: {first:?}"
    );
    let last = entry(&harness, "Mit anderem Stand vergleichen …");
    assert!(
        last.top() > down.top(),
        "letzter Eintrag noch verdeckt: {last:?}"
    );

    // Resting on the lower arrow scrolls evenly down to the last entry.
    harness.hover_at(down.center());
    harness.run_steps(15);
    let after_short = entry(&harness, "Neues LV …");
    assert!(
        after_short.top() < first.top() - 20.0,
        "scrollt beim Überfahren: {after_short:?}"
    );
    harness.run_steps(240);
    let last = entry(&harness, "Mit anderem Stand vergleichen …");
    assert!(
        last.bottom() <= down.top() + 0.5 && last.top() >= up.bottom(),
        "letzter Eintrag sichtbar: {last:?} zwischen {up:?} und {down:?}"
    );
    assert!(
        entry(&harness, "Neues LV …").bottom() < up.bottom(),
        "erster Eintrag oben herausgescrollt"
    );

    // The mouse wheel scrolls back up.
    let middle = Rect::from_min_max(up.left_bottom(), down.right_top()).center();
    harness.hover_at(middle);
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 2000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
        source: egui::MouseWheelSource::Wheel,
    });
    harness.run_steps(30);
    let first = entry(&harness, "Neues LV …");
    assert!(
        first.top() >= up.bottom() - 0.5,
        "Mausrad nach oben: {first:?}"
    );

    // A click on an arrow keeps the menu open (it only scrolls). First the
    // pointer to the arrow, so that no submenu under the wheel stays open.
    harness.hover_at(down.center());
    harness.run_steps(10);
    harness
        .query_all_by(|node| node.label().as_deref() == Some("Nach unten scrollen"))
        .next()
        .unwrap()
        .click();
    harness.run_steps(10);
    assert!(
        shows_arrow(&harness, "Nach unten scrollen").is_some(),
        "Klick auf den Pfeil schließt das Menü nicht"
    );

    // The menu still starts below the bar (never pushed upwards).
    let up_now = shows_arrow(&harness, "Nach oben scrollen").unwrap();
    assert_eq!(up_now.top(), up.top(), "Oberkante bleibt");

    // A click on an entry still closes the menu.
    harness.hover_at(middle);
    harness.run_steps(4);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Ganzes LV kopieren"))
        })
        .next()
        .expect("Eintrag")
        .click();
    harness.run_steps(6);
    assert!(
        shows_arrow(&harness, "Nach unten scrollen").is_none(),
        "Menü nach der Wahl zu"
    );
}

#[test]
fn arrow_keys_scroll_the_chosen_entry_into_view() {
    let folder = TempDir::new("menue-tasten");
    let mut harness = open_demo(&folder.0);
    harness.set_size(egui::vec2(1366.0, LOW));
    settle(&mut harness);
    let bar = lv_menu(&harness);
    harness
        .query_all_by(|node| node.label().as_deref() == Some(strings::MENU_LV))
        .find(|node| node.rect() == bar)
        .unwrap()
        .click();
    harness.run_steps(6);
    let down = shows_arrow(&harness, "Nach unten scrollen").expect("Menü scrollt");
    // Up from the first entry wraps to the last one (submenu „Bibliothek“):
    // it must come into view.
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowUp);
    harness.run_steps(30);
    let last = harness
        .query_all_by(|node| node.label().as_deref() == Some("Bibliothek"))
        .map(|node| node.rect())
        .find(|rect| rect.left() >= down.left() - 1.0 && rect.right() <= down.right() + 1.0)
        .expect("Untermenü Bibliothek im Menü LV");
    assert!(
        last.bottom() <= down.top() + 0.5,
        "gewählter letzter Eintrag sichtbar: {last:?}, Pfeil {down:?}"
    );
}

#[test]
fn short_menus_and_tall_windows_do_not_scroll() {
    let folder = TempDir::new("menue-hoch");
    let mut harness = open_demo(&folder.0);
    harness.set_size(egui::vec2(1366.0, 1200.0));
    settle(&mut harness);
    let bar = lv_menu(&harness);
    harness
        .query_all_by(|node| node.label().as_deref() == Some(strings::MENU_LV))
        .find(|node| node.rect() == bar)
        .unwrap()
        .click();
    harness.run_steps(6);
    assert!(
        shows_arrow(&harness, "Nach unten scrollen").is_none(),
        "passt ganz: keine Pfeile"
    );
    let first = entry(&harness, "Neues LV …");
    let last = entry(&harness, "Mit anderem Stand vergleichen …");
    assert!(
        first.top() >= bar.bottom(),
        "unter der Menüleiste: {first:?}"
    );
    assert!(last.bottom() <= 1200.0, "ganz sichtbar: {last:?}");
}

/// The menus of the menu bar from left to right.
const BAR: [&str; 12] = [
    "Projekt",
    "Bearbeiten",
    "Einfügen",
    "LV",
    "Prüfen",
    "GAEB",
    "Angebote",
    "Kosten",
    "Drucken",
    "Ansicht",
    "Extras",
    "Hilfe",
];

#[test]
fn the_menu_bar_has_the_usual_order_and_every_menu_fits_768_px() {
    let folder = TempDir::new("menueleiste");
    let mut harness = open_demo(&folder.0);
    harness.set_size(egui::vec2(1366.0, 768.0));
    settle(&mut harness);
    let mut lefts: Vec<(f32, &str)> = BAR
        .iter()
        .map(|name| (menu_bar_entry(&harness, name).left(), *name))
        .collect();
    lefts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let order: Vec<&str> = lefts.into_iter().map(|(_, name)| name).collect();
    assert_eq!(order, BAR, "Reihenfolge der Menüleiste");

    // What goes where (names and keys unchanged).
    let contents: [(&str, &[&str]); 4] = [
        (
            "Bearbeiten",
            &[
                "Rückgängig",
                "Wiederholen",
                "Suchen",
                "Suchen und Ersetzen",
                "Bearbeitungsstatus",
                "Alle Titel zuklappen",
                "Alle Titel aufklappen",
            ],
        ),
        (
            "Einfügen",
            &[
                "Neue Position darunter",
                "Neue Position davor",
                "Neuer Titel",
                "Neuer Untertitel",
                "Neuer Hinweis darunter",
                "Neuer Hinweis davor",
                "Neuer Abstandshalter darunter (Leerzeile)",
                "Neue Ausführungsbeschreibung",
                "Neue Position aus STLB-Bau …",
                "Neue Position aus AUSSCHREIBEN.DE …",
                "Aus anderem Projekt / GAEB übernehmen …",
            ],
        ),
        (
            "LV",
            &[
                "Neues LV …",
                "LV-Eigenschaften …",
                "Gehe zu LV",
                "LV duplizieren (ohne Angebote)",
                "Ganzes LV kopieren",
                "LV aus Zwischenablage einfügen …",
                "LV nach oben",
                "LV nach unten",
                "LV löschen …",
                "Gliederung und OZ-Maske …",
                "Neu nummerieren …",
                "Mit anderem Stand vergleichen …",
                "Bibliothek",
            ],
        ),
        (
            "Prüfen",
            &[
                "LV prüfen …",
                "Rechtschreibung prüfen …",
                "Leere Mengen finden",
                "Offene Punkte …",
                "Kosten nach Kostengruppen …",
                "Leere Hinweise in Abstandshalter umwandeln",
                "STLB-Bau-Positionen zum Prüfen ausgeben (X81) …",
                "Aktualisierte STLB-Bau-Positionen einlesen …",
            ],
        ),
    ];
    for name in BAR {
        open_bar_menu(&mut harness, name);
        assert!(
            shows_arrow(&harness, "Nach unten scrollen").is_none(),
            "Menü {name} passt bei 768 px ohne Pfeile"
        );
        if let Some((_, wanted)) = contents.iter().find(|(menu, _)| *menu == name) {
            let below = menu_bar_entry(&harness, name).bottom();
            for text in *wanted {
                let rect = harness
                    .query_all_by(|node| {
                        node.label().is_some_and(|label| {
                            label == *text || label.starts_with(&format!("{text} "))
                        })
                    })
                    .map(|node| node.rect())
                    .find(|rect| rect.top() >= below)
                    .unwrap_or_else(|| panic!("{name} ▸ {text} fehlt"));
                assert!(rect.bottom() <= 768.0, "{name} ▸ {text} sichtbar: {rect:?}");
            }
        }
        press(&mut harness, Key::Escape);
    }
}
