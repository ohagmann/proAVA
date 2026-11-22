//! Auswahllisten in Dialogen, die man ein zweites Mal öffnet: die Liste muss
//! über dem Dialog liegen und Klicks annehmen (Befund 18.11.2026, siehe
//! `widgets::keep_popups_above`). Geklickt wird wie mit einer schnellen
//! Maus: Drücken und Loslassen im selben Bild – nur dann trat der Fehler auf.

mod common;

use common::*;
use egui::accesskit::Role;

/// Press and release in one frame (`Harness::event` would give each event
/// its own frame).
fn quick_click(harness: &mut UiHarness, at: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    settle(harness);
}

/// Centre of the list (ComboBox) that shows `value`.
fn list_showing(harness: &UiHarness, value: &str) -> egui::Pos2 {
    harness
        .query_all_by(|node| {
            node.role() == Role::ComboBox && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Auswahl „{value}“ fehlt"))
        .rect()
        .center()
}

/// Where the entry `label` of an open list can be clicked: on top at its
/// place, not covered by the dialog titled `dialog`.
fn entry_on_top(harness: &UiHarness, dialog: &str, label: &str) -> Option<egui::Pos2> {
    let title = harness
        .get_all_by_label(dialog)
        .last()
        .unwrap_or_else(|| panic!("Dialog „{dialog}“ fehlt"))
        .rect()
        .center();
    let dialog_layer = harness.ctx.layer_id_at(title);
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some(label) && node.role() != Role::ComboBox
        })
        .map(|node| node.rect().center())
        .find(|center| {
            let layer = harness.ctx.layer_id_at(*center);
            layer.is_some() && layer != dialog_layer
        })
}

/// Opens the list showing `value`, picks `entry` with the mouse.
fn pick(harness: &mut UiHarness, dialog: &str, value: &str, entry: &str, when: &str) {
    let at = list_showing(harness, value);
    quick_click(harness, at);
    let at = entry_on_top(harness, dialog, entry)
        .unwrap_or_else(|| panic!("{when}: Liste von „{value}“ liegt nicht über dem Dialog"));
    quick_click(harness, at);
    assert!(
        harness
            .query_all_by(
                |node| node.role() == Role::ComboBox && node.value().as_deref() == Some(entry)
            )
            .next()
            .is_some(),
        "{when}: „{entry}“ gewählt"
    );
}

/// Befund 18.11.: nach „Anlegen“ → „Start“ → „Neues Projekt …“ ging die
/// Vorlagen-Auswahl auf, blieb aber unter dem Dialog.
#[test]
fn new_project_template_list_after_a_project() {
    let folder = TempDir::new("popup-vorlage");
    let launcher = launcher(&folder.0);
    let mut preferences = launcher.preferences();
    preferences.template_folder = Some(folder.0.join("Vorlagen"));
    launcher.set_preferences(preferences).unwrap();
    launcher.set_default_folder(Some(folder.0.clone()));

    let mut harness = harness(&folder.0, None);
    click(&mut harness, "Neues Projekt …");
    type_text(&mut harness, "Erstes");
    pick(
        &mut harness,
        "Neues Projekt",
        "leer",
        "GA nach DIN 276 (481–489)",
        "erstes Öffnen",
    );
    click(&mut harness, "Anlegen");
    assert!(harness.state().project().is_some(), "Projekt offen");

    click(&mut harness, "Start");
    click(&mut harness, "Neues Projekt …");
    type_text(&mut harness, "Zweites");
    pick(
        &mut harness,
        "Neues Projekt",
        "GA nach DIN 276 (481–489)",
        "leer",
        "nach einem Projekt",
    );
    assert!(shows(&harness, "OZ-Maske:"), "Feld zu „leer“");
}

/// Einstellungen ▸ Preisspiegel ▸ Bezugsgröße, zweimal geöffnet.
#[test]
fn settings_list_after_reopening() {
    let folder = TempDir::new("popup-einstellungen");
    let mut harness = open_demo(&folder.0);
    for (round, (from, to)) in [("Mittelwert", "Günstigster"), ("Günstigster", "Mittelwert")]
        .iter()
        .enumerate()
    {
        click(&mut harness, "Einstellungen");
        harness
            .get_all_by_label("Preisspiegel")
            .last()
            .expect("Reiter")
            .click();
        settle(&mut harness);
        pick(
            &mut harness,
            "Einstellungen",
            from,
            to,
            &format!("Runde {}", round + 1),
        );
        click(&mut harness, "Übernehmen");
    }
}

/// Drucken (Strg+P) ▸ LV mit den Preisen eines Bieters ▸ Angebot, zweimal geöffnet.
#[test]
fn print_dialog_list_after_reopening() {
    let folder = TempDir::new("popup-druck");
    let mut harness = open_demo(&folder.0);
    let bids: Vec<String> = project(&harness)
        .bids()
        .iter()
        .map(|bid| bid.name().to_string())
        .collect();
    assert!(bids.len() >= 2, "Demo hat Angebote");
    for round in 1..=2 {
        press_with(&mut harness, Modifiers::COMMAND, Key::P);
        click(&mut harness, "LV mit den Preisen eines Bieters");
        let shown = harness
            .query_all_by(|node| node.role() == Role::ComboBox)
            .filter_map(|node| node.value())
            .find(|value| value == "Angebot wählen" || bids.contains(value))
            .expect("Auswahl Angebot");
        let other = bids.iter().find(|name| **name != shown).unwrap().clone();
        pick(
            &mut harness,
            "Drucken",
            &shown,
            &other,
            &format!("Runde {round}"),
        );
        click(&mut harness, "Abbrechen");
    }
}
