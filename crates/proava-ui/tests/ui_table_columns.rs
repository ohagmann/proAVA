//! Spalten „KG“ und „LB“ der LV-Tabelle (Anwender 18.11. 13:10): einblendbar
//! über Ansicht und Rechtsklick auf den Spaltenkopf, gemerkt in den
//! Einstellungen (Vorgabe KG an, LB aus); geerbte Werte grau/kursiv mit
//! Tooltip „vom Titel …“; Bearbeiten in der Zelle wie Menge/EP (Zeile wählen,
//! Klick in die Zelle, F2 oder lostippen) mit der Klartext-Liste darunter.
//! Dazu Strg+E = Eigenschaftenfenster, Strg+Umschalt+F = fertig/offen.

mod common;

use common::*;
use egui::accesskit::Role;
use egui::Rect;
use proava_domain::LvRow;
use proava_ui::cell_edit::Cell;
use proava_ui::columns::{Column, ColumnWidths, Columns};

fn shown(harness: &UiHarness, column: Column) -> bool {
    harness.state().workspace().unwrap().column_shown(column)
}

fn editing(harness: &UiHarness) -> Option<Cell> {
    harness.state().workspace().unwrap().editing_cell()
}

fn open_menu(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.label().as_deref() == Some(name) && node.role() == Role::Button)
        .next()
        .unwrap_or_else(|| panic!("Menü {name}"))
        .click();
    settle(harness);
}

/// Clicks a check box (menu entry) whose label starts with `text`.
fn click_check(harness: &mut UiHarness, text: &str) {
    harness
        .query_all_by(|node| node.label().is_some_and(|label| label.starts_with(text)))
        .next()
        .unwrap_or_else(|| panic!("„{text}“ fehlt"))
        .click();
    settle(harness);
}

fn pointer_click(harness: &mut UiHarness, pos: egui::Pos2, button: egui::PointerButton) {
    harness.event(egui::Event::PointerMoved(pos));
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
    // Longer than a double click: the next click is a click of its own.
    harness.run_steps(30);
}

/// Rect of the table row with this OZ.
fn row_rect(harness: &UiHarness, ordinal: &str) -> Rect {
    harness
        .query_all_by(|node| {
            node.role() == Role::Row
                && node
                    .label()
                    .is_some_and(|label| label.starts_with(&format!("{ordinal} ")))
        })
        .next()
        .map(|node| node.rect())
        .unwrap_or_else(|| panic!("Zeile {ordinal}"))
}

/// The columns of a row as the table lays them out (default widths – the
/// test settings hold none).
fn columns(harness: &UiHarness, row: Rect) -> Columns {
    let mut widths = ColumnWidths::default();
    for column in Column::OPTIONAL {
        widths.set_shown(column, shown(harness, column));
    }
    widths.layout(row.left(), row.right())
}

fn cost_group(harness: &UiHarness, index: usize) -> Option<String> {
    match &project(harness).lv().rows()[index] {
        LvRow::Group(group) => group.cost_group().map(str::to_string),
        LvRow::Position(position) => position.cost_group().map(str::to_string),
        _ => None,
    }
}

fn work_category(harness: &UiHarness, index: usize) -> Option<String> {
    match &project(harness).lv().rows()[index] {
        LvRow::Group(group) => group.work_category().map(str::to_string),
        LvRow::Position(position) => position.work_category().map(str::to_string),
        _ => None,
    }
}

/// From the selected position up to its title; its index.
fn up_to_title(harness: &mut UiHarness, position: usize) -> usize {
    let mut title = position;
    while !project(harness).lv().rows()[title].is_group() {
        press(harness, Key::ArrowUp);
        title = selected(harness).expect("gewählte Zeile");
    }
    title
}

fn lock(harness: &mut UiHarness) {
    press_with(harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::F);
}

#[test]
fn kg_and_lb_columns_are_switched_and_remembered() {
    let folder = TempDir::new("spalten-kg-lb");
    {
        let mut harness = open_demo(&folder.0);
        assert!(shown(&harness, Column::CostGroup), "KG: Vorgabe an");
        assert!(!shown(&harness, Column::WorkCategory), "LB: Vorgabe aus");

        open_menu(&mut harness, "Ansicht");
        click_check(&mut harness, "Spalte LB (Leistungsbereich)");
        assert!(shown(&harness, Column::WorkCategory), "LB über Ansicht an");

        // Right click on the head of the table: the same switches.
        let head = harness
            .get_by_label(proava_ui::lv_editor::HEADER_NAME)
            .rect();
        pointer_click(
            &mut harness,
            egui::pos2(head.left() + 300.0, head.center().y),
            egui::PointerButton::Secondary,
        );
        assert!(
            shows_part(&harness, "Spaltenbreiten zurücksetzen"),
            "Menü des Spaltenkopfs"
        );
        click_check(&mut harness, "Spalte KG (Kostengruppe)");
        assert!(!shown(&harness, Column::CostGroup), "KG über den Kopf aus");
        let settings = std::fs::read_to_string(folder.0.join("settings.json")).unwrap();
        assert!(settings.contains("\"table_columns\""), "{settings}");
    }
    // Next start: as left.
    let harness = open_demo(&folder.0);
    assert!(!shown(&harness, Column::CostGroup), "KG bleibt aus");
    assert!(shown(&harness, Column::WorkCategory), "LB bleibt an");
}

#[test]
fn kg_and_lb_are_edited_in_the_cell_with_keys() {
    let folder = TempDir::new("zelle-kg-lb");
    let mut harness = open_demo(&folder.0);
    open_menu(&mut harness, "Ansicht");
    click_check(&mut harness, "Spalte LB (Leistungsbereich)");
    let position = select_first_position(&mut harness);

    // Kurztext → KG with →, F2 edits, Enter takes over.
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), Some(Cell::CostGroup));
    type_text(&mut harness, "482");
    press(&mut harness, Key::Enter);
    assert_eq!(editing(&harness), None);
    assert_eq!(cost_group(&harness, position).as_deref(), Some("482"));

    // Strg+Z: one step per cell.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(cost_group(&harness, position), None, "rückgängig");
    press(&mut harness, Key::Home);
    let position = select_first_position(&mut harness);

    // Esc cancels.
    press(&mut harness, Key::F2);
    assert_eq!(
        editing(&harness),
        Some(Cell::CostGroup),
        "aktive Zelle bleibt KG"
    );
    type_text(&mut harness, "483");
    press(&mut harness, Key::Escape);
    assert_eq!(editing(&harness), None);
    assert_eq!(cost_group(&harness, position), None, "Esc ändert nichts");

    // Wrong input stays in the box with the message.
    press(&mut harness, Key::F2);
    type_text(&mut harness, "48");
    press(&mut harness, Key::Enter);
    assert_eq!(
        editing(&harness),
        Some(Cell::CostGroup),
        "Eingabe abgelehnt"
    );
    assert!(shows_part(&harness, "keine Kostengruppe"), "Meldung");
    type_text(&mut harness, "1");
    // Tab takes over and goes on to LB.
    press(&mut harness, Key::Tab);
    assert_eq!(cost_group(&harness, position).as_deref(), Some("481"));
    assert_eq!(editing(&harness), Some(Cell::WorkCategory));

    // The list under the cell: typing filters, ↓ chooses, Enter takes it.
    type_text(&mut harness, "gebäudeauto");
    assert!(shows(&harness, "070  Gebäudeautomation"), "Liste gefiltert");
    assert!(!shows(&harness, "075  Raumlufttechnische Anlagen"));
    press(&mut harness, Key::ArrowDown);
    press(&mut harness, Key::Enter);
    assert_eq!(work_category(&harness, position).as_deref(), Some("070"));
    assert_eq!(editing(&harness), None);
    assert_eq!(
        project(&harness).lv().effective_cost_group(position),
        Some(("481".into(), false)),
        "eigene KG"
    );
}

#[test]
fn a_click_into_the_kg_cell_of_the_selected_row_edits_it_with_the_list() {
    let folder = TempDir::new("zelle-kg-klick");
    let mut harness = open_demo(&folder.0);
    let position = select_first_position(&mut harness);
    let ordinal = project(&harness).lv().rows()[position]
        .ordinal()
        .to_string();
    let row = row_rect(&harness, &ordinal);
    let at = egui::pos2(columns(&harness, row).kg_left + 12.0, row.center().y);

    // First click: the cell becomes the active one; second click edits.
    pointer_click(&mut harness, at, egui::PointerButton::Primary);
    assert_eq!(editing(&harness), None, "erster Klick wählt nur");
    pointer_click(&mut harness, at, egui::PointerButton::Primary);
    assert_eq!(
        editing(&harness),
        Some(Cell::CostGroup),
        "zweiter Klick bearbeitet"
    );
    assert!(
        shows(&harness, "481  Automationseinrichtungen"),
        "Klartext-Liste unter der Zelle"
    );
    type_text(&mut harness, "48");
    harness
        .get_by_label("482  Schaltschränke, Automationsschwerpunkte")
        .click();
    settle(&mut harness);
    assert_eq!(cost_group(&harness, position).as_deref(), Some("482"));
    assert_eq!(editing(&harness), None, "Klick in die Liste übernimmt");

    // The double click still opens the Positionsfenster (Anwender 13:12).
    assert_eq!(
        harness.state().workspace().unwrap().position_window_row(),
        None
    );
}

#[test]
fn a_titles_kg_is_inherited_grey_and_asks_for_finished_positions() {
    let folder = TempDir::new("zelle-kg-titel");
    let mut harness = open_demo(&folder.0);
    let position = select_first_position(&mut harness);
    lock(&mut harness);
    assert!(project(&harness).lv().rows()[position]
        .as_position()
        .unwrap()
        .is_locked());

    // A finished position: no editing in its cells.
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), None, "fertig = gesperrt");
    assert!(shows_part(&harness, "als fertig markiert"), "Hinweis");
    type_text(&mut harness, "4");
    assert_eq!(editing(&harness), None, "auch Lostippen nicht");

    // The title: its new KG would change the finished position – ask.
    let title = up_to_title(&mut harness, position);
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), Some(Cell::CostGroup), "Titel: KG-Zelle");
    type_text(&mut harness, "484");
    press(&mut harness, Key::Enter);
    assert!(
        shows(&harness, "Kostengruppe des Titels ändern?"),
        "Rückfrage offen"
    );
    assert_eq!(cost_group(&harness, title), None, "noch nicht geändert");
    click(&mut harness, "KG ändern");
    assert_eq!(cost_group(&harness, title).as_deref(), Some("484"));
    let lv = project(&harness).lv();
    assert_eq!(
        lv.effective_cost_group(position),
        Some(("484".into(), true)),
        "die fertige Position erbt"
    );
    assert_eq!(lv.cost_group_source(position), Some(title));

    // Inherited: the tooltip over the cell names the title.
    let title_ordinal = lv.rows()[title].ordinal().to_string();
    let ordinal = lv.rows()[position].ordinal().to_string();
    let row = row_rect(&harness, &ordinal);
    let at = egui::pos2(columns(&harness, row).kg_left + 12.0, row.center().y);
    harness.event(egui::Event::PointerMoved(at));
    harness.run_steps(80);
    assert!(
        shows_part(&harness, &format!("– vom Titel {title_ordinal}")),
        "Tooltip „vom Titel {title_ordinal}“"
    );

    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(cost_group(&harness, title), None, "Strg+Z nimmt es zurück");

    // LB asks the same way.
    open_menu(&mut harness, "Ansicht");
    click_check(&mut harness, "Spalte LB (Leistungsbereich)");
    press(&mut harness, Key::Home);
    let first = select_first_position(&mut harness);
    let title = up_to_title(&mut harness, first);
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), Some(Cell::WorkCategory));
    type_text(&mut harness, "75");
    press(&mut harness, Key::Enter);
    assert!(shows(&harness, "Leistungsbereich des Titels ändern?"));
    click(&mut harness, "LB ändern");
    assert_eq!(work_category(&harness, title).as_deref(), Some("075"));
    assert_eq!(
        project(&harness).lv().effective_work_category(position),
        Some(("075".into(), true))
    );
}

#[test]
fn ctrl_e_shows_and_hides_the_properties_panel_and_ctrl_shift_f_finishes() {
    let folder = TempDir::new("strg-e");
    let mut harness = open_demo(&folder.0);
    let position = select_first_position(&mut harness);
    let visible = |harness: &UiHarness| harness.state().workspace().unwrap().detail_visible();
    let locked = |harness: &UiHarness| {
        project(harness).lv().rows()[position]
            .as_position()
            .unwrap()
            .is_locked()
    };
    assert!(visible(&harness));
    press_with(&mut harness, Modifiers::COMMAND, Key::E);
    assert!(!visible(&harness), "Strg+E blendet aus");
    assert!(!locked(&harness), "Strg+E ändert den Status nicht mehr");
    press_with(&mut harness, Modifiers::COMMAND, Key::E);
    assert!(visible(&harness), "Strg+E blendet wieder ein");

    lock(&mut harness);
    assert!(locked(&harness), "Strg+Umschalt+F: fertig");
    lock(&mut harness);
    assert!(!locked(&harness), "Strg+Umschalt+F: wieder offen");
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::E);
    assert_eq!(
        project(&harness).lv().rows()[position]
            .as_position()
            .unwrap()
            .status(),
        proava_domain::EditStatus::InProgress,
        "Strg+Umschalt+E bleibt „in Bearbeitung“"
    );
    assert!(visible(&harness), "Strg+Umschalt+E ist nicht Strg+E");

    open_menu(&mut harness, "Ansicht");
    assert!(
        shows_part(&harness, "Eigenschaftenfenster anzeigen")
            && harness
                .query_all_by(|node| node
                    .label()
                    .is_some_and(|label| label.contains("Eigenschaftenfenster anzeigen")
                        && label.contains("Strg+E")))
                .next()
                .is_some(),
        "Menü zeigt Strg+E"
    );
}

/// Two slow clicks at `at`: the first selects/activates, the second edits
/// only the cell under the pointer (Anwender 18.11. 13:45: a second click on
/// GP opened the Kurztext).
fn two_clicks(harness: &mut UiHarness, at: egui::Pos2) -> Option<Cell> {
    pointer_click(harness, at, egui::PointerButton::Primary);
    pointer_click(harness, at, egui::PointerButton::Primary);
    let cell = editing(harness);
    if cell.is_some() {
        press(harness, Key::Escape);
        assert_eq!(editing(harness), None);
    }
    cell
}

#[test]
fn a_second_click_edits_only_the_cell_under_the_pointer() {
    let folder = TempDir::new("zweiter-klick");
    std::fs::write(
        folder.0.join("settings.json"),
        r#"{"table_columns":{"kg":true,"lb":true}}"#,
    )
    .unwrap();
    let mut harness = open_demo(&folder.0);
    let position = select_first_position(&mut harness);
    let before = project(&harness).lv().clone();
    let ordinal = project(&harness).lv().rows()[position]
        .ordinal()
        .to_string();
    let row = row_rect(&harness, &ordinal);
    let c = columns(&harness, row);
    assert!(c.show_kg && c.show_lb && c.show_unit && c.show_ep && c.show_gp);
    let y = row.center().y;
    let x_of = |cell: Cell| match cell {
        Cell::Ordinal => c.left + 10.0,
        Cell::Short => (c.text_x + c.text_right) / 2.0,
        Cell::CostGroup => c.kg_left + 12.0,
        Cell::WorkCategory => c.lb_left + 12.0,
        Cell::Quantity => c.qty_right - 10.0,
        Cell::Unit => c.unit_left + 12.0,
        Cell::Price => c.ep_right - 10.0,
    };

    // Editable cells open themselves.
    for cell in [
        Cell::Short,
        Cell::CostGroup,
        Cell::WorkCategory,
        Cell::Quantity,
        Cell::Unit,
        Cell::Price,
    ] {
        assert_eq!(
            two_clicks(&mut harness, egui::pos2(x_of(cell), y)),
            Some(cell),
            "zweiter Klick auf {}",
            cell.label()
        );
        assert_eq!(selected(&harness), Some(position), "Zeile bleibt gewählt");
    }
    // GP, Status and OZ open nothing – not the Kurztext either.
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(c.gp_right - 10.0, y)),
        None,
        "GP"
    );
    assert!(shows_part(&harness, "GP = Menge × EP"), "Hinweis zum GP");
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(c.status_x + 10.0, y)),
        None,
        "Status"
    );
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(x_of(Cell::Ordinal), y)),
        None,
        "OZ: nichts (ändern mit F2 oder Strg+Umschalt+O)"
    );
    assert_eq!(selected(&harness), Some(position), "Zeile bleibt gewählt");
    // The OZ stays reachable with the keyboard: the click made it the active cell.
    press(&mut harness, Key::F2);
    assert_eq!(
        editing(&harness),
        Some(Cell::Ordinal),
        "F2 bearbeitet die OZ"
    );
    press(&mut harness, Key::Escape);
    assert_eq!(project(&harness).lv(), &before, "nichts geändert");

    // A title: KG/Kurztext are cells, its sum and the number columns are not.
    let title = up_to_title(&mut harness, position);
    let title_ordinal = project(&harness).lv().rows()[title].ordinal().to_string();
    let row = row_rect(&harness, &title_ordinal);
    let y = row.center().y;
    for (x, what) in [
        (c.gp_right - 10.0, "Titelsumme"),
        (c.qty_right - 10.0, "Menge am Titel"),
        (c.ep_right - 10.0, "EP am Titel"),
        (c.status_x + 10.0, "Status am Titel"),
    ] {
        assert_eq!(two_clicks(&mut harness, egui::pos2(x, y)), None, "{what}");
    }
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(c.kg_left + 12.0, y)),
        Some(Cell::CostGroup)
    );
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(c.text_right - 30.0, y)),
        Some(Cell::Short),
        "Kurztext des Titels"
    );

    // A finished position: its cells stay closed, the hint says why.
    press(&mut harness, Key::Home);
    let position = select_first_position(&mut harness);
    lock(&mut harness);
    let row = row_rect(&harness, &ordinal);
    let y = row.center().y;
    for cell in [Cell::Short, Cell::Quantity, Cell::Price, Cell::CostGroup] {
        assert_eq!(
            two_clicks(&mut harness, egui::pos2(x_of(cell), y)),
            None,
            "fertig: {}",
            cell.label()
        );
    }
    assert!(shows_part(&harness, "als fertig markiert"));
    assert_eq!(selected(&harness), Some(position));
}

/// The table row whose AccessKit name starts with `start`.
fn row_named(harness: &UiHarness, start: &str) -> Rect {
    harness
        .query_all_by(|node| {
            node.role() == Role::Row && node.label().is_some_and(|label| label.starts_with(start))
        })
        .next()
        .map(|node| node.rect())
        .unwrap_or_else(|| panic!("Zeile „{start}…“ fehlt"))
}

/// Anwender 18.11. 15:12: Klick 1, Klick 2 bearbeitet den Text wie bei einer
/// Position – auch bei Hinweisen (Überschrift) und Ausführungsbeschreibungen
/// (Bezeichnung); Abstandshalter bleiben ohne Zelle.
#[test]
fn second_clicks_edit_the_text_of_hints_and_performance_descriptions_not_spacers() {
    let folder = TempDir::new("zweiter-klick-hinweis");
    let mut harness = open_demo(&folder.0);
    select_first_position(&mut harness);

    // A new hint: its text cell edits the heading.
    press_with(&mut harness, Modifiers::COMMAND, Key::H);
    let hint = selected(&harness).expect("Hinweis gewählt");
    let heading = |harness: &UiHarness| match &project(harness).lv().rows()[hint] {
        LvRow::Note(note) => note.heading().to_string(),
        _ => panic!("kein Hinweis"),
    };
    let row = row_named(&harness, "Hinweis ohne Text");
    let c = columns(&harness, row);
    let text_at = egui::pos2((c.text_x + c.text_right) / 2.0, row.center().y);
    pointer_click(&mut harness, text_at, egui::PointerButton::Primary);
    pointer_click(&mut harness, text_at, egui::PointerButton::Primary);
    assert_eq!(editing(&harness), Some(Cell::Short), "Hinweis: Textzelle");
    type_text(&mut harness, "Vorbemerkung Kabel");
    press(&mut harness, Key::Escape);
    assert_eq!(heading(&harness), "", "Esc nimmt es zurück");
    pointer_click(&mut harness, text_at, egui::PointerButton::Primary);
    assert_eq!(editing(&harness), Some(Cell::Short));
    type_text(&mut harness, "Vorbemerkung Kabel");
    press(&mut harness, Key::Enter);
    assert_eq!(editing(&harness), None);
    assert_eq!(heading(&harness), "Vorbemerkung Kabel", "Enter übernimmt");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(heading(&harness), "", "Strg+Z");
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    assert_eq!(heading(&harness), "Vorbemerkung Kabel", "Strg+Y");
    // The hint text runs over the whole row: also a click over GP means it.
    let row = row_named(&harness, "Vorbemerkung Kabel");
    assert_eq!(
        two_clicks(&mut harness, egui::pos2(c.gp_right - 10.0, row.center().y)),
        Some(Cell::Short)
    );

    // A new Ausführungsbeschreibung: its text cell edits the Bezeichnung,
    // with the GAEB limit of 55 characters as a hint.
    press_with(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::A);
    let performance = selected(&harness).expect("AB gewählt");
    let label = |harness: &UiHarness| match &project(harness).lv().rows()[performance] {
        LvRow::Performance(performance) => performance.label().to_string(),
        _ => panic!("keine Ausführungsbeschreibung"),
    };
    let row = row_named(&harness, "Ausführungsbeschreibung A");
    let text_at = egui::pos2((c.text_x + c.text_right) / 2.0, row.center().y);
    assert_eq!(
        two_clicks(&mut harness, text_at),
        Some(Cell::Short),
        "AB: Textzelle"
    );
    pointer_click(&mut harness, text_at, egui::PointerButton::Primary);
    assert_eq!(editing(&harness), Some(Cell::Short));
    let long = "Kabeltrassen und Leitungswege im gesamten Gebäude, alle Geschosse";
    assert!(long.chars().count() > 55);
    // Select all, then type over it.
    press_with(&mut harness, Modifiers::COMMAND, Key::A);
    type_text(&mut harness, long);
    assert!(shows_part(&harness, "höchstens 55"), "GAEB-Grenze genannt");
    press(&mut harness, Key::Enter);
    assert_eq!(label(&harness), long, "übernommen trotz Hinweis");
    assert!(project(&harness).lv().rows()[performance]
        .short_text()
        .ends_with(long));

    // An Abstandshalter has no cell.
    press_with(&mut harness, Modifiers::COMMAND, Key::L);
    let spacer = selected(&harness).expect("Abstandshalter gewählt");
    assert!(matches!(
        project(&harness).lv().rows()[spacer],
        LvRow::Spacer(_)
    ));
    let row = row_named(&harness, "Abstandshalter");
    for x in [
        c.left + 10.0,
        (c.text_x + c.text_right) / 2.0,
        c.qty_right - 10.0,
        c.gp_right - 10.0,
    ] {
        assert_eq!(
            two_clicks(&mut harness, egui::pos2(x, row.center().y)),
            None,
            "Abstandshalter bei x = {x}"
        );
    }
    assert_eq!(selected(&harness), Some(spacer));
}
