//! Window „Eigenschaften“ of an LV row: double click and Alt+Enter open it,
//! Reiter per row kind, Prüfleiste, vorige/nächste Position, changes and undo,
//! closing. The harness has no second native window: egui shows the window
//! embedded in the main window (same content, same code).

mod common;

use common::*;
use proava_domain::{LvRow, RowId};

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().position_window_row()
}

/// Name of a table row as the table reports it (OZ and Kurztext).
fn row_label(harness: &UiHarness, index: usize) -> String {
    let row = &project(harness).lv().rows()[index];
    format!("{} {}", row.ordinal(), row.short_text())
        .trim()
        .to_string()
}

/// The demo project without the edit panel, so that only the window shows
/// the fields of the row.
fn open_demo_without_panel(folder: &std::path::Path) -> UiHarness {
    std::fs::write(
        folder.join("settings.json"),
        r#"{"preferences":{"detail_visible":false}}"#,
    )
    .unwrap();
    open_demo(folder)
}

/// The text field whose content is `value`.
fn click_field_with_value(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld mit „{value}“ fehlt"))
        .click();
    settle(harness);
}

#[test]
fn double_click_on_a_position_opens_its_window_instead_of_editing_the_cell() {
    let folder = TempDir::new("posfenster-doppelklick");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    assert_eq!(window_row(&harness), None);

    let label = row_label(&harness, index);
    double_click(&mut harness, &label);
    assert_eq!(window_row(&harness), Some(id), "Fenster für die Position");
    for tab in ["Allgemein", "Weiteres", "Grafik"] {
        assert!(shows(&harness, tab), "Reiter {tab}");
    }
    assert!(shows(&harness, "Prüfung"), "Prüfleiste");
    assert!(shows(&harness, "Nächste Position"));
    assert_eq!(
        harness.state().workspace().unwrap().selected_index(),
        Some(index),
        "Auswahl bleibt"
    );

    click(&mut harness, "Weiteres");
    assert!(shows(&harness, "Positionsart"), "Reiter Weiteres");
    assert!(shows(&harness, "Weitere Angaben"));
    click(&mut harness, "Grafik");
    assert!(shows_part(&harness, "Bild einfügen"), "Reiter Grafik");

    click(&mut harness, "Schließen (Esc)");
    assert_eq!(window_row(&harness), None, "geschlossen");
}

#[test]
fn alt_enter_opens_the_window_esc_closes_it_and_titles_get_one_tab() {
    let folder = TempDir::new("posfenster-alt-enter");
    let mut harness = open_demo(&folder.0);
    press(&mut harness, Key::Home);
    let index = selected(&harness).unwrap();
    assert!(
        project(&harness).lv().rows()[index].is_group(),
        "erste Zeile ist ein Titel"
    );
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(
        window_row(&harness),
        Some(project(&harness).lv().rows()[index].id())
    );
    assert!(shows(&harness, "Allgemein"));
    assert!(!shows(&harness, "Weiteres"), "Titel: nur Allgemein");
    assert!(!shows(&harness, "Grafik"));
    assert!(
        shows(&harness, "Nächster Titel"),
        "Titel blättern durch Titel"
    );

    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt");
}

#[test]
fn changes_in_the_window_reach_the_table_and_undo_takes_them_back() {
    let folder = TempDir::new("posfenster-aendern");
    let mut harness = open_demo_without_panel(&folder.0);
    let index = select_first_position(&mut harness);
    let before = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(window_row(&harness), Some(before.id()));

    click_field_with_value(&mut harness, before.short_text());
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, "Kurztext aus dem Fenster");
    let short = |harness: &UiHarness| {
        project(harness)
            .lv()
            .position_by_id(before.id())
            .unwrap()
            .short_text()
            .to_string()
    };
    assert_eq!(short(&harness), "Kurztext aus dem Fenster");
    assert!(harness.state().workspace().unwrap().is_dirty());
    assert!(
        shows_part(&harness, "Kurztext aus dem Fenster"),
        "Tabelle zeigt es sofort"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        short(&harness),
        before.short_text(),
        "Strg+Z wie im Hauptfenster"
    );
    assert_eq!(
        window_row(&harness),
        Some(before.id()),
        "Fenster bleibt offen"
    );
}

#[test]
fn next_and_previous_step_through_positions_and_move_the_table_selection() {
    let folder = TempDir::new("posfenster-blaettern");
    let mut harness = open_demo(&folder.0);
    let first = select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    let lv = project(&harness).lv();
    let second = (first + 1..lv.rows().len())
        .find(|at| lv.rows()[*at].is_position())
        .expect("zweite Position");
    let second_id = lv.rows()[second].id();

    click(&mut harness, "Nächste Position");
    assert_eq!(window_row(&harness), Some(second_id), "nächste Position");
    assert_eq!(selected(&harness), Some(second), "Tabelle folgt");
    click(&mut harness, "Vorige Position");
    assert_eq!(
        window_row(&harness),
        Some(project(&harness).lv().rows()[first].id())
    );

    // The window follows the table.
    press(&mut harness, Key::ArrowDown);
    let now = selected(&harness).unwrap();
    assert_eq!(
        window_row(&harness),
        Some(project(&harness).lv().rows()[now].id()),
        "Fenster folgt der Auswahl"
    );
}

#[test]
fn the_check_bar_names_what_a_new_position_lacks_and_follows_the_edits() {
    let folder = TempDir::new("posfenster-pruefung");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    // The demo LV has bids: keep the new position.
    keep_despite_bids(&mut harness, "einfügen");
    let index = selected(&harness).unwrap();
    let id = project(&harness).lv().rows()[index].id();
    assert!(matches!(
        project(&harness).lv().rows()[index],
        LvRow::Position(_)
    ));
    // Leave the cell the new position opened for typing.
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(window_row(&harness), Some(id));
    assert!(shows(&harness, "Ohne Kurztext."), "Prüfleiste: Kurztext");
    assert!(shows(&harness, "Warnung"));

    // The field „Kurztext“ of the window (the edit panel shows only a note now).
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node.label().as_deref() == Some("Kurztext")
        })
        .next()
        .expect("Feld Kurztext im Fenster")
        .click();
    settle(&mut harness);
    type_text(&mut harness, "Raumbediengerät");
    assert_eq!(
        project(&harness)
            .lv()
            .position_by_id(id)
            .unwrap()
            .short_text(),
        "Raumbediengerät"
    );
    assert!(!shows(&harness, "Ohne Kurztext."), "Befund verschwindet");
    assert!(
        shows(
            &harness,
            "Steht im Fenster „Eigenschaften“ – dort bearbeiten."
        ),
        "Bearbeitungsbereich"
    );
}

#[test]
fn hints_and_descriptions_show_their_own_fields() {
    let folder = TempDir::new("posfenster-hinweis");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::H);
    let index = selected(&harness).unwrap();
    assert!(
        project(&harness).lv().rows()[index].is_note(),
        "neuer Hinweis"
    );
    // The new hint puts the caret into its text; leave it first.
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(
        window_row(&harness),
        Some(project(&harness).lv().rows()[index].id())
    );
    assert!(shows(&harness, "Allgemein") && shows(&harness, "Grafik"));
    assert!(
        !shows(&harness, "Weiteres"),
        "Hinweis: kein Reiter Weiteres"
    );
    assert!(shows(&harness, "Überschrift (optional)"));
    assert!(shows(&harness, "Nächster Hinweis"));
    assert!(
        shows_part(&harness, "Ohne Text nach"),
        "Prüfleiste: Hinweis ohne Text"
    );
    click(&mut harness, "Schließen (Esc)");

    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    let index = selected(&harness).unwrap();
    assert!(
        project(&harness).lv().rows()[index].is_performance(),
        "neue Ausführungsbeschreibung"
    );
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert_eq!(
        window_row(&harness),
        Some(project(&harness).lv().rows()[index].id())
    );
    assert!(shows(&harness, "Bezeichnung"));
    assert!(shows(&harness, "Noch von keiner Position verwendet"));
    assert!(!shows(&harness, "Weiteres"));
}

#[test]
fn the_work_category_is_chosen_on_the_tab_more() {
    let folder = TempDir::new("posfenster-lb");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Weiteres");
    assert!(shows(&harness, "Leistungsbereich (Gewerk)"));
    // Second list on the tab: the first is the Kostengruppe.
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Liste …") || node.value().as_deref() == Some("Liste …")
        })
        .nth(1)
        .expect("Liste der Leistungsbereiche")
        .click();
    settle(&mut harness);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.trim() == "070 Gebäudeautomation")
        })
        .next()
        .expect("Eintrag 070")
        .click_accesskit();
    settle(&mut harness);
    let work = |harness: &UiHarness| {
        project(harness)
            .lv()
            .position_by_id(id)
            .unwrap()
            .work_category()
            .map(str::to_string)
    };
    assert_eq!(work(&harness).as_deref(), Some("070"));
    assert!(shows(&harness, "Gebäudeautomation"), "Name unter dem Feld");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(work(&harness), None, "Strg+Z");
}

#[test]
fn takeoff_lines_sum_up_and_become_the_quantity() {
    let folder = TempDir::new("posfenster-me");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Mengenermittlung");
    click(&mut harness, "Zeile hinzufügen");
    // The new line's field „Ort / Bezeichnung“ has the caret.
    type_text(&mut harness, "EG Strang 1");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "2*12,5");
    let position = |harness: &UiHarness| project(harness).lv().position_by_id(id).unwrap().clone();
    let lines = position(&harness).takeoff().to_vec();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].label, "EG Strang 1");
    assert_eq!(lines[0].formula, "2*12,5");
    assert!(shows_part(&harness, "Summe 25"), "Summe sichtbar");
    assert!(
        shows_part(&harness, "weicht von der Mengenermittlung (25) ab"),
        "Prüfleiste"
    );

    click(&mut harness, "Summe als Menge übernehmen");
    assert_eq!(position(&harness).quantity(), "25".parse().unwrap());
    assert!(
        !shows_part(&harness, "weicht von der Mengenermittlung"),
        "Befund weg"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_ne!(
        position(&harness).quantity(),
        "25".parse().unwrap(),
        "Strg+Z nimmt die Übernahme zurück"
    );
    assert_eq!(position(&harness).takeoff().len(), 1, "die Zeile bleibt");
}

/// Picks an entry of the n-th list „Liste …“ in the window.
fn choose_from_list(harness: &mut UiHarness, nth: usize, entry: &str) {
    harness
        .query_all_by(|node| {
            node.label().as_deref() == Some("Liste …") || node.value().as_deref() == Some("Liste …")
        })
        .nth(nth)
        .expect("Liste")
        .click();
    settle(harness);
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.trim() == entry))
        .next()
        .unwrap_or_else(|| panic!("Eintrag „{entry}“ fehlt"))
        .click_accesskit();
    settle(harness);
}

#[test]
fn a_title_passes_its_work_category_down_and_asks_for_finished_positions() {
    let folder = TempDir::new("posfenster-lb-titel");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    // The first position is finished: its title's new LB must ask first.
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(project(&harness)
        .lv()
        .position_by_id(id)
        .unwrap()
        .is_locked());
    press(&mut harness, Key::Home);
    let title = selected(&harness).unwrap();
    assert!(project(&harness).lv().rows()[title].is_group());
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(shows(
        &harness,
        "Leistungsbereich (Gewerk) – Vorgabe für alles darunter"
    ));
    // Lists on the title: Kostengruppe, Leistungsbereich.
    choose_from_list(&mut harness, 1, "070 Gebäudeautomation");
    let title_lb = |harness: &UiHarness| {
        project(harness).lv().rows()[title]
            .as_group()
            .unwrap()
            .work_category()
            .map(str::to_string)
    };
    assert_eq!(title_lb(&harness), None, "erst die Rückfrage");
    assert!(shows(
        &harness,
        "1 fertige Position erbt den neuen Leistungsbereich 070 Gebäudeautomation."
    ));
    click(&mut harness, "Trotzdem ändern");
    assert_eq!(title_lb(&harness).as_deref(), Some("070"));
    assert_eq!(
        project(&harness).lv().effective_work_category(index),
        Some(("070".to_string(), true))
    );
    // The position shows where it comes from.
    click(&mut harness, "Schließen (Esc)");
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    click(&mut harness, "Weiteres");
    assert!(shows_part(
        &harness,
        "Gilt vom Titel 01: 070 Gebäudeautomation"
    ));
}

#[test]
fn opening_puts_the_caret_into_the_short_text_and_the_last_tab_is_kept() {
    let folder = TempDir::new("posfenster-fokus");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    let before = project(&harness)
        .lv()
        .position_by_id(id)
        .unwrap()
        .short_text()
        .to_string();
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    // Typing goes straight into the Kurztext of the window.
    press(&mut harness, Key::End);
    type_text(&mut harness, " Zusatz");
    assert_eq!(
        project(&harness)
            .lv()
            .position_by_id(id)
            .unwrap()
            .short_text(),
        format!("{before} Zusatz")
    );
    click(&mut harness, "Weiteres");
    click(&mut harness, "Schließen (Esc)");
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(
        shows(&harness, "Positionsart"),
        "wieder auf dem Reiter Weiteres"
    );
}
