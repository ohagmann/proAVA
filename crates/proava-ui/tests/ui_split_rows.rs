//! Aufteilungszeilen im Kosten-LV: „GA nach Datenpunkten (Kurztext-Vorlage)
//! einfügen“ und „Aufteilung nach KG einfügen“ (nur im Kosten-LV), Prozent in
//! der Tabelle ändern, Strg+Z.

mod common;

use common::*;
use egui_kittest::kittest::NodeT;
use proava_domain::{Decimal, LvRow, Money, Project, RowId};
use proava_ui::cell_edit::Cell;
use proava_ui::StartFile;

fn open(folder: &TempDir, project: &Project) -> UiHarness {
    let file = save(&folder.0, project);
    harness(&folder.0, Some(StartFile::Project(file)))
}

/// Whether the menu entry (label, also with its shortcut behind) is greyed out.
fn disabled(harness: &UiHarness, label: &str) -> bool {
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|text| text == label || text.starts_with(&format!("{label} ")))
        })
        .next()
        .unwrap_or_else(|| panic!("„{label}“ fehlt"))
        .accesskit_node()
        .is_disabled()
}

const TEMPLATE: &str = "GA nach Datenpunkten (Kurztext-Vorlage) einfügen";
const SPLIT: &str = "Aufteilung nach KG einfügen";

/// Selects the row with this id (arrow keys from the top).
fn select(harness: &mut UiHarness, id: RowId) {
    press(harness, Key::Home);
    for _ in 0..80 {
        if let Some(index) = selected(harness) {
            if project(harness).lv().rows()[index].id() == id {
                return;
            }
        }
        press(harness, Key::ArrowDown);
    }
    panic!("Zeile nicht erreicht");
}

fn editing_cell(harness: &UiHarness) -> Option<Cell> {
    harness.state().workspace().unwrap().editing_cell()
}

/// Types `text` into the cell `cell` of the selected row (F2, then Tab).
fn type_into(harness: &mut UiHarness, cell: Cell, text: &str) {
    press(harness, Key::F2);
    for _ in 0..8 {
        if editing_cell(harness) == Some(cell) {
            break;
        }
        press(harness, Key::Tab);
    }
    assert_eq!(editing_cell(harness), Some(cell), "Zelle {cell:?}");
    press_with(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, text);
    press(harness, Key::Enter);
}

#[test]
fn ga_template_split_percent_and_undo() {
    let folder = TempDir::new("aufteilung");
    let mut harness = open(&folder, &proava_ui::demo::cost_lv_demo_project());
    let before = project(&harness).lv().net_total();
    let rows = project(&harness).lv().rows().len();

    open_bar_menu(&mut harness, "Einfügen");
    click_menu(&mut harness, TEMPLATE);
    let lv = project(&harness).lv();
    assert_eq!(
        lv.rows().len(),
        rows + 13,
        "Titel, 6 Datenpunkte, 6 Aufteilungszeilen"
    );
    let index = selected(&harness).unwrap();
    let first = lv.rows()[index].as_position().unwrap();
    assert!(
        first.short_text().ends_with("(AI)"),
        "erster Datenpunkt gewählt"
    );
    let ai = first.id();
    let splits: Vec<RowId> = lv
        .positions()
        .filter(|position| {
            position
                .surcharge()
                .is_some_and(|surcharge| surcharge.split)
        })
        .map(|position| position.id())
        .collect();
    assert_eq!(splits.len(), 6);
    assert!(
        shows_part(&harness, "Aufteilung darunter trägt die Kosten"),
        "Meldung"
    );

    // 100 AI × 400 € = 40.000 €: counted once, through the Aufteilung.
    type_into(&mut harness, Cell::Quantity, "100");
    let lv = project(&harness).lv();
    assert_eq!(
        lv.position_by_id(ai).unwrap().quantity(),
        Decimal::from(100)
    );
    assert_eq!(lv.net_total(), before + Money::from_cents(4_000_000));
    assert_eq!(
        lv.surcharge_values()[&splits[0]].amount,
        Money::from_cents(1_600_000)
    );

    // 481 from 40 % to 45 %: the block makes 105 %, LV prüfen and the table say so.
    select(&mut harness, splits[0]);
    type_into(&mut harness, Cell::Price, "45");
    let lv = project(&harness).lv();
    assert_eq!(
        lv.position_by_id(splits[0]).unwrap().unit_price(),
        Some(Decimal::from(45))
    );
    assert_eq!(
        lv.surcharge_values()[&splits[0]].amount,
        Money::from_cents(1_800_000)
    );
    let findings = proava_domain::cost_lv_findings(lv);
    assert!(
        findings
            .iter()
            .any(|(_, text)| text.contains("105 % statt 100 %")),
        "{findings:?}"
    );

    // Strg+Z: the percentage, then the quantity, then the whole template.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness)
            .lv()
            .position_by_id(splits[0])
            .unwrap()
            .unit_price(),
        Some(Decimal::from(40))
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).lv().net_total(), before);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).lv().rows().len(), rows);
}

#[test]
fn a_split_on_the_positions_before_from_the_right_click() {
    let folder = TempDir::new("aufteilung-rechtsklick");
    let mut harness = open(&folder, &proava_ui::demo::cost_lv_demo_project());
    // The last position of 431: the block refers to the positions of 431 before it.
    let lv = project(&harness).lv();
    let last = lv
        .rows()
        .iter()
        .filter_map(LvRow::as_position)
        .rfind(|position| position.ordinal().starts_with("430.431"))
        .unwrap()
        .id();
    let base: Money = lv
        .positions()
        .filter(|position| position.ordinal().starts_with("430.431"))
        .map(|position| position.total_price())
        .sum();
    let before = lv.net_total();
    select(&mut harness, last);
    open_bar_menu(&mut harness, "Einfügen");
    click_menu(&mut harness, SPLIT);
    let lv = project(&harness).lv();
    let splits: Vec<&proava_domain::Position> = lv
        .positions()
        .filter(|position| {
            position
                .surcharge()
                .is_some_and(|surcharge| surcharge.split)
        })
        .collect();
    assert_eq!(splits.len(), 6);
    assert_eq!(lv.surcharge_values()[&splits[0].id()].base, base);
    assert_eq!(lv.net_total(), before, "umverteilt, nicht doppelt");
    let report = proava_domain::cost_group_report(lv, &[]);
    assert!(
        report
            .lines
            .iter()
            .any(|line| line.code.as_deref() == Some("481") && !line.planner.is_zero()),
        "KG 481 aus der Aufteilung"
    );
}

#[test]
fn only_a_cost_lv_offers_the_commands() {
    let folder = TempDir::new("aufteilung-nur-kosten-lv");
    let mut harness = open_demo(&folder.0);
    open_bar_menu(&mut harness, "Einfügen");
    assert!(disabled(&harness, SPLIT), "im Ausschreibungs-LV gesperrt");
    assert!(disabled(&harness, TEMPLATE));
    press(&mut harness, Key::Escape);
}

#[test]
fn a_point_section_becomes_kurztext_positions_and_back() {
    let folder = TempDir::new("aufteilung-umwandeln");
    let mut harness = open(&folder, &proava_ui::demo::gu_demo_project());
    let lv = project(&harness).lv();
    let before = lv.net_total();
    let index = lv.point_sections()[0];
    let id = lv.rows()[index].id();
    select(&mut harness, id);
    click(&mut harness, "In Kurztext-Positionen umwandeln");
    let lv = project(&harness).lv();
    assert!(
        lv.point_sections().is_empty(),
        "kein Datenpunkt-Abschnitt mehr"
    );
    assert_eq!(lv.net_total(), before, "Summe bleibt");
    assert!(!lv.split_blocks().is_empty(), "Aufteilung je Bauteil");
    assert!(
        shows_part(&harness, "Datenpunkt-Abschnitt umgewandelt"),
        "Meldung"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).lv().point_sections(), [index], "Strg+Z");
}
