//! LV editor: selection with the keys, new positions, editing cells,
//! quantity formulas, undo/redo.

mod common;

use common::*;

#[test]
fn starter_opens_a_project_from_the_recent_list() {
    let folder = TempDir::new("starter");
    let file = save(&folder.0, &proava_ui::demo::demo_project());
    launcher(&folder.0).open_project(&file).unwrap();
    let mut harness = harness(&folder.0, None);
    assert!(harness.state().project().is_none());
    assert!(shows(&harness, "Zuletzt verwendet"), "Startbildschirm");
    // Aktionen fest links, die Projektliste rechts daneben (Anwender, 17.11.).
    let new_project = harness
        .get_all_by_label("Neues Projekt …")
        .next()
        .unwrap()
        .rect();
    let name = proava_ui::demo::demo_project().name().as_str().to_string();
    let recent = harness.get_all_by_label(&name).next().unwrap().rect();
    assert!(
        new_project.right() < recent.left(),
        "{new_project:?} links von {recent:?}"
    );
    assert!(
        shows_part(&harness, "Einstellungen"),
        "Zahnrad-Knopf mit Namen"
    );
    assert!(
        shows(&harness, "Dunkles Aussehen"),
        "Mond-Knopf im Startfenster"
    );
    double_click(&mut harness, &name);
    assert!(
        harness.state().project().is_some(),
        "Projekt per Doppelklick geöffnet"
    );
}

/// Standardsymbole wie in Word (Anwender, 17.11. 22:19): Langtext-Leiste und
/// Kopfleiste zeigen Symbolknöpfe, jeder mit Namen (Tooltip, Screenreader).
#[test]
fn toolbar_and_header_show_named_symbol_buttons() {
    let folder = TempDir::new("symbols");
    let mut harness = open_demo(&folder.0);
    for name in ["Start", "Befehl …", "Einstellungen", "Dunkles Aussehen"] {
        assert!(shows(&harness, name), "Kopfleiste: {name}");
    }
    select_first_position(&mut harness);
    for name in ["Linksbündig", "Zentriert", "Rechtsbündig", "Blocksatz"] {
        assert!(shows(&harness, name), "Langtext-Leiste: {name}");
    }
    for part in [
        "Aufzählung",
        "Nummerierung",
        "Tabelle einfügen",
        "Bild einfügen",
    ] {
        assert!(shows_part(&harness, part), "Langtext-Leiste: {part}");
    }
    assert!(
        !shows(&harness, "links") && !shows(&harness, "Block"),
        "keine Textknöpfe mehr"
    );
}

#[test]
fn status_shows_as_text_in_the_table() {
    let folder = TempDir::new("status");
    let mut harness = open_demo(&folder.0);
    // 01, 01.01, then the first position.
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::ArrowDown);
    let index = harness
        .state()
        .workspace()
        .unwrap()
        .selected_index()
        .unwrap();
    let position = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    let name = format!("{} {}", position.ordinal(), position.short_text());
    assert!(shows(&harness, &name), "Zeile ohne Status: {name}");
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
    assert!(
        shows(&harness, &format!("{name} – fertig")),
        "Spalte Status: fertig"
    );
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::E);
    assert!(
        shows(&harness, &format!("{name} – in Bearb.")),
        "Spalte Status: in Bearb."
    );
}

#[test]
fn arrow_keys_select_and_ctrl_enter_adds_a_position() {
    let folder = TempDir::new("keys");
    let mut harness = open_demo(&folder.0);
    let rows = project(&harness).lv().rows().len();
    let selected = |harness: &UiHarness| harness.state().workspace().unwrap().selected_index();
    press(&mut harness, Key::ArrowDown);
    let first = selected(&harness);
    press(&mut harness, Key::ArrowDown);
    let second = selected(&harness);
    assert!(second > first, "{first:?} → {second:?}");
    press(&mut harness, Key::ArrowUp);
    assert_eq!(selected(&harness), first);
    let positions = project(&harness).lv().position_count();
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    assert_eq!(project(&harness).lv().rows().len(), rows + 1);
    assert_eq!(project(&harness).lv().position_count(), positions + 1);
}

#[test]
fn cells_are_edited_with_f2_tab_enter_and_quantity_formulas_and_undone() {
    let folder = TempDir::new("cells");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();

    press(&mut harness, Key::F2);
    type_text(&mut harness, "Neuer Kurztext");
    // Tab: Kurztext → KG (Spalte an, Vorgabe) → Menge.
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "2*3+1");
    press(&mut harness, Key::Enter);

    let position = |harness: &UiHarness| {
        project(harness)
            .lv()
            .position_by_id(before.id())
            .unwrap()
            .clone()
    };
    let after = position(&harness);
    assert_eq!(after.short_text(), "Neuer Kurztext");
    assert_eq!(after.quantity(), "7".parse().unwrap());
    assert_eq!(after.quantity_formula(), Some("2*3+1"));
    assert!(harness.state().workspace().unwrap().is_dirty());
    assert!(shows_part(&harness, "Neuer Kurztext"), "Kurztext sichtbar");

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        position(&harness).quantity(),
        before.quantity(),
        "Menge zurück"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        position(&harness).short_text(),
        before.short_text(),
        "Kurztext zurück"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    assert_eq!(
        position(&harness).quantity(),
        "7".parse().unwrap(),
        "wiederholt"
    );
}

#[test]
fn invalid_quantity_is_refused_and_the_cell_stays_open() {
    let folder = TempDir::new("badqty");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    press(&mut harness, Key::F2);
    // Kurztext → KG → Menge.
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "3*(2");
    press(&mut harness, Key::Enter);
    let after = project(&harness).lv().position_by_id(before.id()).unwrap();
    assert_eq!(after.quantity(), before.quantity());
    assert!(shows_part(&harness, "Menge:"), "Fehlermeldung am Feld");
    press(&mut harness, Key::Escape);
}

#[test]
fn new_title_while_a_cell_is_edited_takes_the_cell_over_and_gets_the_typing() {
    let folder = TempDir::new("title-while-editing");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    press(&mut harness, Key::F2);
    type_text(&mut harness, "Geänderter Kurztext");
    let rows = project(&harness).lv().rows().len();
    press_with(
        &mut harness,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::Enter,
    );
    type_text(&mut harness, "Schaltschränke");
    let lv = project(&harness).lv();
    assert_eq!(lv.rows().len(), rows + 1, "Titel angelegt");
    assert_eq!(
        lv.rows()[lv.index_of(id).unwrap()].short_text(),
        "Geänderter Kurztext",
        "offene Zelle übernommen"
    );
    let title = lv
        .rows()
        .iter()
        .find(|row| row.short_text() == "Schaltschränke")
        .unwrap_or_else(|| panic!("Getipptes nicht im neuen Titel: {:?}", row_texts(&harness)));
    assert!(title.is_group());
}

/// Stichprobe aus der Abnahme: kommen Strg, Enter und das Loslassen von Strg im
/// selben Frame an (schnelle Eingabe), darf Strg+Enter nicht zusätzlich als
/// Enter die gewählte Zeile in Bearbeitung nehmen – sonst überschreibt das
/// Weitertippen deren Kurztext statt den der neuen Position.
#[test]
fn ctrl_enter_within_one_frame_does_not_edit_the_previous_row() {
    let folder = TempDir::new("ctrlenter");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let before = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    // Strg+Enter, wie xdotool es schickt: alles in einem Frame, am Ende ist
    // keine Umschalttaste mehr gedrückt.
    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.event(egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    settle(&mut harness);
    // The demo LV has bids: keep the new position, then type on.
    keep_despite_bids(&mut harness, "einfügen");
    type_text(&mut harness, "Neue Leistung");
    press(&mut harness, Key::Enter);
    let lv = project(&harness).lv();
    let old = lv.position_by_id(before.id()).unwrap();
    assert_eq!(
        old.short_text(),
        before.short_text(),
        "vorige Position unverändert"
    );
    assert_eq!(old.quantity(), before.quantity());
    let new = lv.rows()[lv.index_of(before.id()).unwrap() + 1]
        .as_position()
        .unwrap();
    assert_eq!(new.short_text(), "Neue Leistung");
}

/// STLB-Bau-Platzhalter are no longer made in proAVA (Anwender 18.11.: the
/// built-in window inserts at the chosen place), but those from projects and
/// GAEB files are still filled by „Neue Position aus STLB-Bau …“.
#[test]
fn a_placeholder_from_a_project_is_still_filled_from_stlb() {
    let folder = TempDir::new("stlb-platzhalter");
    let mut demo = proava_ui::demo::demo_project();
    demo.lv_mut()
        .push_position(proava_domain::Position::stlb("Raumbediengerät"));
    demo.lv_mut().renumber();
    let file = save(&folder.0, &demo);
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Project(file)));
    let ordinal = project(&harness)
        .lv()
        .positions()
        .find(|position| position.is_stlb_placeholder())
        .expect("Platzhalter im Projekt")
        .ordinal()
        .to_string();
    // The placeholder is the last row.
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    let index = selected(&harness).expect("Zeile gewählt");
    assert_eq!(project(&harness).lv().rows()[index].ordinal(), ordinal);
    open_bar_menu(&mut harness, "Einfügen");
    assert!(
        !shows_part(&harness, "STLB-Bau-Platzhalter"),
        "kein Eintrag für neue Platzhalter"
    );
    click_menu(&mut harness, "Neue Position aus STLB-Bau …");
    assert!(
        shows_part(
            &harness,
            &format!("Platzhalter {ordinal} „Raumbediengerät“")
        ),
        "der gewählte Platzhalter wird gefüllt"
    );
}

#[test]
fn new_subtitle_on_the_deepest_level_makes_a_title_beside_it() {
    let folder = TempDir::new("deepest");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let lv = project(&harness).lv();
    let parent = lv.rows()[..index]
        .iter()
        .rposition(|row| row.is_group())
        .unwrap();
    let level = lv.rows()[parent].as_group().unwrap().level();
    assert_eq!(
        level + 1,
        lv.level_count(),
        "Demo: Position unter tiefster Stufe"
    );
    press_with(
        &mut harness,
        Modifiers {
            shift: true,
            ..Modifiers::COMMAND
        },
        Key::T,
    );
    type_text(&mut harness, "Neuer Abschnitt");
    let lv = project(&harness).lv();
    let title = lv
        .rows()
        .iter()
        .find(|row| row.short_text() == "Neuer Abschnitt")
        .unwrap_or_else(|| panic!("{:?}", row_texts(&harness)));
    assert_eq!(title.as_group().unwrap().level(), level);
    assert_ne!(lv.rows()[parent].short_text(), "Neuer Abschnitt");
}

#[test]
fn typing_in_the_oz_column_goes_into_the_short_text() {
    let folder = TempDir::new("oz-typing");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let ordinal = project(&harness).lv().rows()[index].ordinal().to_string();
    // Kurztext → OZ (ein zweites ← ginge in den Titel darüber)
    press(&mut harness, Key::ArrowLeft);
    type_text(&mut harness, "Neuer Text");
    press(&mut harness, Key::Enter);
    let row = &project(&harness).lv().rows()[index];
    assert_eq!(row.ordinal(), ordinal, "OZ bleibt");
    assert_eq!(row.short_text(), "Neuer Text");
}

#[test]
fn undo_selects_the_row_it_brings_back() {
    let folder = TempDir::new("undo-select");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    let id = project(&harness).lv().rows()[index].id();
    press(&mut harness, Key::Delete);
    // The bids of the demo price the position: keep the deletion.
    keep_despite_bids(&mut harness, "löschen");
    assert!(project(&harness).lv().index_of(id).is_none(), "gelöscht");
    // Auswahl woanders hin, dann Rückgängig
    press(&mut harness, Key::ArrowDown);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let lv = project(&harness).lv();
    assert_eq!(
        harness.state().workspace().unwrap().selected_index(),
        lv.index_of(id),
        "die wiederhergestellte Zeile ist gewählt"
    );
    assert!(
        shows_part(&harness, "Rückgängig: "),
        "Meldung nennt die Zeile"
    );
}

#[test]
fn a_new_row_at_the_end_is_fully_in_view() {
    let folder = TempDir::new("end-row");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::End);
    let rows = project(&harness).lv().rows().len();
    let shown = harness.state().workspace().unwrap().table_rows_shown();
    assert!(shown.start > 0, "Demo-LV länger als die Tabelle: {shown:?}");
    press_with(&mut harness, Modifiers::COMMAND, Key::Enter);
    // egui's scroll physics would coast on for a while
    harness.run_steps(90);
    assert_eq!(project(&harness).lv().rows().len(), rows + 1);
    let workspace = harness.state().workspace().unwrap();
    assert_eq!(
        workspace.selected_index(),
        Some(rows),
        "neue Zeile am Ende gewählt"
    );
    let shown = workspace.table_rows_shown();
    assert!(
        shown.contains(&rows),
        "neue letzte Zeile ganz sichtbar: {shown:?}"
    );
}

/// One click with the primary button at `pos`.
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
}

#[test]
fn a_slow_second_click_on_the_framed_cell_edits_it() {
    let folder = TempDir::new("slow-click");
    let mut harness = open_demo(&folder.0);
    let index = select_first_position(&mut harness);
    // Erst eine andere Zeile wählen: der erste Klick ist dann ein Auswahlklick.
    press(&mut harness, Key::ArrowDown);
    let position = project(&harness).lv().rows()[index]
        .as_position()
        .unwrap()
        .clone();
    let name = format!("{} {}", position.ordinal(), position.short_text());
    let fields = |harness: &UiHarness| {
        harness
            .query_all_by(|node| {
                node.role() == egui::accesskit::Role::TextInput
                    && node.value().as_deref() == Some(position.short_text())
            })
            .count()
    };
    let center = harness
        .get_all_by_label(&name)
        .next()
        .expect(&name)
        .rect()
        .center();
    click_at(&mut harness, center);
    harness.run_steps(40); // Pause: länger als ein Doppelklick
    assert_eq!(selected(&harness), Some(index), "erster Klick wählt");
    let only_panel = fields(&harness);
    assert_eq!(
        only_panel, 1,
        "erster Klick bearbeitet nichts (nur das Eigenschaftenfeld)"
    );
    click_at(&mut harness, center);
    settle(&mut harness);
    assert_eq!(
        fields(&harness),
        2,
        "zweiter Klick nach Pause: Zelle in Bearbeitung"
    );
    assert_eq!(
        project(&harness).lv().rows()[index].short_text(),
        position.short_text()
    );
}

/// Abnahme qa3 (18.11.): Hinweis-Zeile wählen und Enter drücken setzte eine
/// leere Zeile an den Anfang des Hinweistexts – Enter führt nur in den Text.
#[test]
fn enter_on_a_hint_row_goes_into_its_text_without_an_empty_line() {
    let folder = TempDir::new("hinweis-enter");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::H);
    let index = selected(&harness).unwrap();
    type_text(&mut harness, "Montage im ASP01");
    press(&mut harness, Key::Escape);
    let hint_text = |harness: &UiHarness| match &project(harness).lv().rows()[index] {
        proava_domain::LvRow::Note(note) => note.text().clone(),
        other => panic!("kein Hinweis: {other:?}"),
    };
    let before = hint_text(&harness);
    assert_eq!(before.to_plain(), "Montage im ASP01");

    press(&mut harness, Key::Enter);
    assert_eq!(
        hint_text(&harness),
        before,
        "Enter allein ändert den Hinweis nicht"
    );
    type_text(&mut harness, "Vorab: ");
    let after = hint_text(&harness);
    assert_eq!(
        after.blocks().len(),
        1,
        "kein zusätzlicher Absatz: {after:?}"
    );
    assert!(
        after.to_plain().contains("Vorab: "),
        "Enter führt in den Hinweistext: {}",
        after.to_plain()
    );
}

/// Schlussbemerkung (Anwender 18.11., „Zulieferungen originalgetreu“): am Hinweis
/// anzukreuzen wie die Vorbemerkung; eins schließt das andere aus.
#[test]
fn a_hint_becomes_a_closing_text_and_back() {
    let folder = TempDir::new("schlussbemerkung");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);
    press_with(&mut harness, Modifiers::COMMAND, Key::H);
    let index = selected(&harness).unwrap();
    type_text(&mut harness, "Schlussblatt: Angaben des Bieters");
    press(&mut harness, Key::Escape);
    let kind = |harness: &UiHarness| match &project(harness).lv().rows()[index] {
        proava_domain::LvRow::Note(note) => (note.is_preliminary(), note.is_closing()),
        other => panic!("kein Hinweis: {other:?}"),
    };
    assert_eq!(kind(&harness), (false, false));
    click(
        &mut harness,
        "Schlussbemerkung (steht in GAEB und im Druck nach dem LV)",
    );
    assert_eq!(kind(&harness), (false, true));
    click(&mut harness, "Vorbemerkung (steht in GAEB vor dem LV)");
    assert_eq!(
        kind(&harness),
        (true, false),
        "Vorbemerkung löst die Schlussbemerkung ab"
    );
    // Die Angaben eines Hinweises sind ein Rückgängig-Schritt (wie Überschrift
    // und Vorbemerkung bisher).
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(kind(&harness), (false, false), "Strg+Z");
}
