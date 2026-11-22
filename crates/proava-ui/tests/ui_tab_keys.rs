//! Tab and Umschalt+Tab while editing (Anwender 18.11. 23:16: „Tab zum nächsten
//! Feld beim Editieren geht nicht mehr“): table cells, the Eigenschaftenfenster
//! and the position window – with one LV and with several LVs one below the
//! other.

mod common;

use common::*;
use egui::accesskit::Role;
use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName, RichText};
use proava_ui::cell_edit::Cell;
use proava_ui::StartFile;

fn lv(number: &str, title: &str, mask: &str, rows: &[&str]) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse(mask).unwrap());
    lv.set_number(number);
    for level in 0..lv.level_count() {
        lv.push_group(LvGroup::new(level, "", "Titel"));
    }
    for short in rows {
        let mut position = Position::new("", *short, Decimal::TWO, "St", Some(Decimal::TEN));
        position.set_long_text(RichText::plain("Langtext"));
        lv.push_position(position);
    }
    lv.renumber();
    lv
}

fn one_lv() -> Project {
    Project::new(
        ProjectName::parse("Ein LV").unwrap(),
        "2026-11-18",
        lv(
            "1",
            "Gebäudeautomation",
            "11.PPPP",
            &["DDC-Station", "E/A-Modul"],
        ),
    )
}

fn two_lvs() -> Project {
    let mut project = one_lv();
    project.add_lv(lv(
        "2",
        "Elektro",
        "11.22.PPPP",
        &["Mantelleitung", "Abzweigdose"],
    ));
    project
}

fn open(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    let harness = harness(folder, Some(StartFile::Project(file)));
    assert!(harness.state().project().is_some(), "Projekt offen");
    harness
}

/// Wide like ui_multi_lv: the position window (embedded in tests) leaves
/// the rows free.
fn open_wide(folder: &std::path::Path, project: &Project) -> UiHarness {
    let file = save(folder, project);
    proava_ui::viewer_copies::use_folder_on_this_thread(Some(folder.join("pdf-anzeige")));
    let app = proava_ui::App::new(launcher(folder), Some(StartFile::Project(file)));
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(2600.0, 1100.0))
        .with_context_setup(proava_ui::App::setup)
        .with_accessibility_check(false)
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut proava_ui::App| app.ui(ui), app);
    settle(&mut harness);
    harness
}

fn editing(harness: &UiHarness) -> Option<Cell> {
    harness.state().workspace().unwrap().editing_cell()
}

fn click_row(harness: &mut UiHarness, name: &str) {
    harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"))
        .click();
    settle(harness);
}

/// F2 on the Kurztext of `row`, then Tab, Tab, Umschalt+Tab.
fn tab_through_cells(harness: &mut UiHarness, row: &str) {
    click_row(harness, row);
    press(harness, Key::F2);
    let start = editing(harness);
    assert!(start.is_some(), "{row}: F2 öffnet eine Zelle");
    press(harness, Key::Tab);
    let first = editing(harness);
    assert!(
        first.is_some() && first != start,
        "{row}: Tab geht von {start:?} zur nächsten Zelle, ist {first:?}"
    );
    press(harness, Key::Tab);
    let second = editing(harness);
    assert!(
        second.is_some() && second != first,
        "{row}: zweites Tab, ist {second:?}"
    );
    press_with(harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(editing(harness), first, "{row}: Umschalt+Tab zurück");
    press(harness, Key::Escape);
}

#[test]
fn tab_goes_from_cell_to_cell_with_one_lv() {
    let folder = TempDir::new("tab-ein-lv");
    let mut harness = open(&folder.0, &one_lv());
    tab_through_cells(&mut harness, "01.0010 DDC-Station");
    tab_through_cells(&mut harness, "01.0020 E/A-Modul");
}

#[test]
fn tab_goes_from_cell_to_cell_with_two_lvs() {
    let folder = TempDir::new("tab-zwei-lv");
    let mut harness = open(&folder.0, &two_lvs());
    tab_through_cells(&mut harness, "01.0010 DDC-Station");
    tab_through_cells(&mut harness, "01.01.0010 Mantelleitung");
    tab_through_cells(&mut harness, "01.0020 E/A-Modul");
}

/// Value of the text field that has the keyboard.
fn focused_value(harness: &UiHarness) -> Option<String> {
    harness
        .query_all_by(|node| node.role() == Role::TextInput && node.is_focused())
        .next()
        .and_then(|node| node.value())
}

/// Puts the caret into the text field showing `value`.
fn focus_field(harness: &mut UiHarness, value: &str) {
    harness
        .query_all_by(|node| {
            node.role() == Role::TextInput && node.value().as_deref() == Some(value)
        })
        .next()
        .unwrap_or_else(|| panic!("Feld „{value}“"))
        .click();
    settle(harness);
}

/// Kurztext → Menge → Einheit → EP with Tab, back with Umschalt+Tab – in
/// the field set that shows `short` (Eigenschaftenfenster or position window).
fn tab_through_fields(harness: &mut UiHarness, short: &str) {
    focus_field(harness, short);
    assert_eq!(
        focused_value(harness).as_deref(),
        Some(short),
        "Kurztext hat den Fokus"
    );
    let mut seen = Vec::new();
    for _ in 0..3 {
        press(harness, Key::Tab);
        seen.push(focused_value(harness));
    }
    assert_eq!(
        seen,
        [
            Some("2".to_string()),
            Some("St".to_string()),
            Some("10,00".to_string())
        ],
        "{short}: Tab Kurztext → Menge → Einheit → EP"
    );
    press_with(harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        focused_value(harness).as_deref(),
        Some("St"),
        "{short}: Umschalt+Tab"
    );
    press_with(harness, Modifiers::SHIFT, Key::Tab);
    press_with(harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        focused_value(harness).as_deref(),
        Some(short),
        "{short}: zurück im Kurztext"
    );
}

#[test]
fn tab_walks_the_property_panel_with_one_and_two_lvs() {
    for (label, project) in [("ein LV", one_lv()), ("zwei LVs", two_lvs())] {
        let folder = TempDir::new("tab-panel");
        let mut harness = open(&folder.0, &project);
        click_row(&mut harness, "01.0020 E/A-Modul");
        tab_through_fields(&mut harness, "E/A-Modul");
        if label == "zwei LVs" {
            click_row(&mut harness, "01.01.0020 Abzweigdose");
            tab_through_fields(&mut harness, "Abzweigdose");
        }
    }
}

#[test]
fn tab_walks_the_position_window_with_one_and_two_lvs() {
    for (label, start) in [("ein LV", one_lv()), ("zwei LVs", two_lvs())] {
        let folder = TempDir::new("tab-posfenster");
        let mut harness = open_wide(&folder.0, &start);
        click_row(&mut harness, "01.0020 E/A-Modul");
        press_with(&mut harness, Modifiers::ALT, Key::Enter);
        assert!(
            shows(&harness, "Allgemein"),
            "{label}: Positionsfenster offen"
        );
        tab_through_fields(&mut harness, "E/A-Modul");
        if label == "zwei LVs" {
            // The window follows the table into LV 2.
            click_row(&mut harness, "01.01.0020 Abzweigdose");
            tab_through_fields(&mut harness, "Abzweigdose");
        }
    }
}

/// Anwender 18.11. 23:34: positions A, B (from STLB-Bau: unit locked), C.
fn with_stlb() -> Project {
    let mut lv = lv(
        "1",
        "Gebäudeautomation",
        "11.PPPP",
        &["Anfang", "Schrank", "Ende"],
    );
    let id = lv.positions().nth(1).unwrap().id();
    let index = lv.index_of(id).unwrap();
    if let Some(position) = lv
        .row_mut(index)
        .and_then(proava_domain::LvRow::as_position_mut)
    {
        position.attach_stlb(proava_domain::stlb::StlbReference {
            catalog: "STLB-Bau".into(),
            ..Default::default()
        });
        assert!(position.is_text_locked());
    }
    Project::new(ProjectName::parse("STLB").unwrap(), "2026-11-18", lv)
}

fn quantity(harness: &UiHarness, short: &str) -> Decimal {
    project(harness)
        .lv()
        .positions()
        .find(|position| position.short_text() == short)
        .unwrap()
        .quantity()
}

#[test]
fn tab_passes_the_locked_unit_of_an_stlb_position_in_the_table() {
    let folder = TempDir::new("tab-stlb");
    let mut harness = open(&folder.0, &with_stlb());
    click_row(&mut harness, "01.0020 Schrank");
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), Some(Cell::Short));
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    assert_eq!(
        editing(&harness),
        Some(Cell::Quantity),
        "Kurztext → KG → Menge"
    );
    type_text(&mut harness, "5");
    press(&mut harness, Key::Tab);
    assert_eq!(
        editing(&harness),
        Some(Cell::Price),
        "Tab überspringt die gesperrte Einheit"
    );
    assert_eq!(
        quantity(&harness, "Schrank"),
        Decimal::from(5),
        "Menge übernommen"
    );
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        editing(&harness),
        Some(Cell::Quantity),
        "Umschalt+Tab zurück zur Menge"
    );
    press(&mut harness, Key::Escape);
}

#[test]
fn a_tab_sequence_over_several_positions_passes_the_locked_row() {
    let folder = TempDir::new("tab-stlb-folge");
    let mut harness = open(&folder.0, &with_stlb());
    // A (frei): Kurztext → KG → Menge → Einheit → EP.
    click_row(&mut harness, "01.0010 Anfang");
    press(&mut harness, Key::F2);
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut harness, Key::Tab);
        seen.push(editing(&harness));
    }
    assert_eq!(
        seen,
        [
            Some(Cell::CostGroup),
            Some(Cell::Quantity),
            Some(Cell::Unit),
            Some(Cell::Price)
        ]
    );
    // Enter: B (STLB) unter dem EP; F2 öffnet den EP, Umschalt+Tab die Menge.
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::F2);
    assert_eq!(editing(&harness), Some(Cell::Price));
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        editing(&harness),
        Some(Cell::Quantity),
        "B: Einheit übersprungen"
    );
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(editing(&harness), Some(Cell::CostGroup));
    press(&mut harness, Key::Tab);
    press(&mut harness, Key::Tab);
    assert_eq!(editing(&harness), Some(Cell::Price), "B: Menge → EP");
    // Enter: C (frei) – Umschalt+Tab vom EP landet wieder in der Einheit.
    press(&mut harness, Key::Enter);
    press(&mut harness, Key::F2);
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(editing(&harness), Some(Cell::Unit), "C: Einheit frei");
    press(&mut harness, Key::Escape);
    // Pfeil hoch auf B, aktive Zelle bleibt die Einheit: F2 öffnet die
    // nächste freie Zelle.
    press(&mut harness, Key::ArrowUp);
    press(&mut harness, Key::F2);
    assert_eq!(
        editing(&harness),
        Some(Cell::Price),
        "F2 auf der Einheit → EP"
    );
}

#[test]
fn tab_passes_the_locked_unit_in_the_property_panel_and_the_position_window() {
    // Eigenschaftenfenster.
    let folder = TempDir::new("tab-stlb-panel");
    let mut harness = open(&folder.0, &with_stlb());
    click_row(&mut harness, "01.0020 Schrank");
    focus_field(&mut harness, "2");
    press(&mut harness, Key::Tab);
    assert_eq!(
        focused_value(&harness).as_deref(),
        Some("10,00"),
        "Panel: Menge → EP"
    );
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        focused_value(&harness).as_deref(),
        Some("2"),
        "Panel: EP → Menge"
    );

    // Positionsfenster.
    let folder = TempDir::new("tab-stlb-fenster");
    let mut harness = open_wide(&folder.0, &with_stlb());
    click_row(&mut harness, "01.0020 Schrank");
    press_with(&mut harness, Modifiers::ALT, Key::Enter);
    assert!(shows(&harness, "Allgemein"), "Positionsfenster offen");
    focus_field(&mut harness, "2");
    press(&mut harness, Key::Tab);
    assert_eq!(
        focused_value(&harness).as_deref(),
        Some("10,00"),
        "Fenster: Menge → EP"
    );
    press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
    assert_eq!(
        focused_value(&harness).as_deref(),
        Some("2"),
        "Fenster: EP → Menge"
    );
}

/// One click with the left button at `pos`.
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

/// Middle of the Menge cell of the row named `name`.
fn quantity_cell(harness: &UiHarness, name: &str) -> egui::Pos2 {
    let row = harness
        .query_all_by(|node| node.role() == Role::Row && node.label().as_deref() == Some(name))
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“"))
        .rect();
    let columns = harness.state().workspace().unwrap().table_layout().unwrap();
    egui::pos2((columns.qty_left + columns.qty_right) / 2.0, row.center().y)
}

/// Anwender 19.11. 00:08 („Tab zum Editieren in STLB-Bau“): the way without
/// F2 – click into the Menge of an STLB-Bau position, type, Tab – lands in
/// the EP; also with the position window open and following the table.
#[test]
fn clicking_into_the_quantity_and_typing_then_tab_reaches_the_price() {
    for wide in [false, true] {
        let folder = TempDir::new("tab-stlb-lostippen");
        let mut harness = if wide {
            let mut harness = open_wide(&folder.0, &with_stlb());
            click_row(&mut harness, "01.0010 Anfang");
            press_with(&mut harness, Modifiers::ALT, Key::Enter);
            assert!(shows(&harness, "Allgemein"), "Positionsfenster offen");
            harness
        } else {
            open(&folder.0, &with_stlb())
        };
        let cell = quantity_cell(&harness, "01.0020 Schrank");
        click_at(&mut harness, cell);
        type_text(&mut harness, "5");
        assert_eq!(
            editing(&harness),
            Some(Cell::Quantity),
            "lostippen öffnet die Menge"
        );
        press(&mut harness, Key::Tab);
        assert_eq!(
            editing(&harness),
            Some(Cell::Price),
            "Tab → EP (Fenster: {wide})"
        );
        assert_eq!(quantity(&harness, "Schrank"), Decimal::from(5));
        press_with(&mut harness, Modifiers::SHIFT, Key::Tab);
        assert_eq!(
            editing(&harness),
            Some(Cell::Quantity),
            "Umschalt+Tab → Menge"
        );
    }
}
