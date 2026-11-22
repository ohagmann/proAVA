//! Das Kreuz oben rechts schließt jedes Fenster wie sein Knopf (Anwender
//! 18.11.2026: „Das Bearbeitungsfenster … kann ich nur mit Esc oder dem Knopf
//! schließen, nicht mit dem Kreuz“).
//!
//! Die eigenen Programmfenster „Eigenschaften“ und „Langtext“ bekommen das
//! Kreuz und Alt+F4 vom Fenstermanager als `ViewportEvent::Close`. Der Test
//! zeigt sie eingebettet (ein Viewport): dort lesen sie die Eingabe des
//! Hauptfensters, das Ereignis geht also dorthin (wie in `ui_leave`). Ein
//! Feld, das erst beim Verlassen übernimmt, übernimmt seinen Wert auch beim
//! Kreuz; ein abgelehnter Wert geht mit dem Fenster. Die schwebenden Fenster
//! im Hauptfenster (Übernehmen, Abgleich, Vergleich, Rechtschreibung, Offene
//! Punkte) schließen mit ihrem eigenen Kreuz. Dass der Fenstermanager den
//! Klick aufs Kreuz überhaupt meldet (bis 19.11. verschluckte Cinnamon ihn,
//! wenn das Fensterlevel währenddessen wechselte), zeigt die Sichtprüfung; das
//! Level hängt nicht mehr an Maus und Fokus (`owner_window`).

mod common;

use common::*;
use egui::accesskit::Role;
use egui::{Rect, ViewportEvent, ViewportId};
use proava_domain::RowId;

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().position_window_row()
}

fn long_text_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().long_text_window_row()
}

/// Name of a table row as the table reports it (OZ and Kurztext).
fn row_label(harness: &UiHarness, index: usize) -> String {
    let row = &project(harness).lv().rows()[index];
    format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string()
}

/// The cross of the window (or Alt+F4): the window manager's close request.
fn window_manager_close(harness: &mut UiHarness) {
    harness
        .input_mut()
        .viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(ViewportEvent::Close);
    harness.step();
    settle(harness);
}

/// The field under the caption „Leistungsbereich (Gewerk)“ (Reiter Weiteres).
fn work_category_field(harness: &UiHarness) -> Rect {
    let caption = harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Leistungsbereich (Gewerk)")
                || node.value().as_deref() == Some("Leistungsbereich (Gewerk)")
        })
        .next()
        .expect("Beschriftung Leistungsbereich")
        .rect();
    harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .map(|node| node.rect())
        .filter(|rect| rect.top() >= caption.bottom() - 1.0 && rect.left() < caption.right())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("Feld Leistungsbereich")
}

fn click_at(harness: &mut UiHarness, rect: Rect) {
    let center = rect.center();
    harness.event(egui::Event::PointerMoved(center));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    settle(harness);
}

/// Opens „Eigenschaften“ of the first position on the Reiter Weiteres and
/// types `text` into the field Leistungsbereich (the field keeps the keyboard).
fn type_work_category(harness: &mut UiHarness, text: &str) -> RowId {
    let index = select_first_position(harness);
    let id = project(harness).lv().rows()[index].id();
    let label = row_label(harness, index);
    double_click(harness, &label);
    assert_eq!(window_row(harness), Some(id), "Doppelklick öffnet");
    click(harness, "Weiteres");
    let field = work_category_field(harness);
    click_at(harness, field);
    type_text(harness, text);
    id
}

fn work_category(harness: &UiHarness, id: RowId) -> Option<String> {
    project(harness)
        .lv()
        .position_by_id(id)
        .and_then(|position| position.work_category().map(str::to_string))
}

#[test]
fn the_cross_closes_the_position_window_and_keeps_the_typed_value() {
    let folder = TempDir::new("kreuz-positionsfenster");
    let mut harness = open_demo(&folder.0);
    let id = type_work_category(&mut harness, "070");
    assert_eq!(
        work_category(&harness, id),
        None,
        "noch im Feld, nicht übernommen"
    );
    window_manager_close(&mut harness);
    assert_eq!(window_row(&harness), None, "Kreuz schließt");
    assert_eq!(
        work_category(&harness, id).as_deref(),
        Some("070"),
        "der getippte Wert ist übernommen wie beim Wechsel der Zeile"
    );
}

#[test]
fn a_refused_value_goes_with_the_window() {
    let folder = TempDir::new("kreuz-abgelehnt");
    let mut harness = open_demo(&folder.0);
    // More than 100 characters: the field refuses it and stays red.
    let id = type_work_category(&mut harness, &"x".repeat(101));
    window_manager_close(&mut harness);
    assert_eq!(window_row(&harness), None, "Kreuz schließt");
    assert_eq!(work_category(&harness, id), None, "nicht übernommen");
    // Open again (the row is still selected: Alt+Enter): the field shows
    // the value of the LV, not the refused text.
    press_with(&mut harness, egui::Modifiers::ALT, Key::Enter);
    assert_eq!(window_row(&harness), Some(id), "Alt+Enter öffnet wieder");
    click(&mut harness, "Weiteres");
    let field = work_category_field(&harness);
    let text = harness
        .query_all_by(|node| node.role() == Role::TextInput)
        .find(|node| node.rect() == field)
        .and_then(|node| node.value())
        .unwrap_or_default();
    assert_eq!(text, "", "abgelehnter Text verworfen");
}

#[test]
fn the_cross_closes_the_long_text_window() {
    let folder = TempDir::new("kreuz-langtext");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    press_with(&mut harness, egui::Modifiers::SHIFT, Key::F2);
    assert_eq!(long_text_row(&harness), Some(id), "Umschalt+F2 öffnet");
    window_manager_close(&mut harness);
    assert_eq!(long_text_row(&harness), None, "Kreuz schließt");
}

#[test]
fn floating_windows_close_with_their_cross() {
    let folder = TempDir::new("kreuz-schwebend");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    // „Kosten nach Kostengruppen“ and the Aufmaß have no cross, only their
    // own Schließen; Esc for all of them: `ui_window_esc`.
    let windows: [(&str, &str); 5] = [
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
        press_with(&mut harness, egui::Modifiers::COMMAND, Key::K);
        type_text(&mut harness, query);
        press(&mut harness, Key::Enter);
        assert!(shows(&harness, title), "„{title}“ offen (Befehl „{query}“)");
        click(&mut harness, "Close window");
        assert!(!shows(&harness, title), "„{title}“: das Kreuz schließt");
    }
}
