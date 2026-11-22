//! GU-Fall: Datenpunkt-Abschnitt im Kosten-LV – einfügen (Menü Einfügen, nur
//! im Kosten-LV), Anlagenliste im Fenster über dem LV ausfüllen, Summen am
//! Titel, Strg+Z, Doppelklick öffnet die Anlagenliste, „In Kosten-LV
//! übernehmen“ aus dem Reiter Kosten.

mod common;

use common::*;
use egui_kittest::kittest::NodeT;
use proava_domain::{CostStage, LvPurpose, Money, Project, RowId};
use proava_ui::StartFile;

fn open(folder: &TempDir, project: &Project) -> UiHarness {
    let file = save(&folder.0, project);
    harness(&folder.0, Some(StartFile::Project(file)))
}

fn window_row(harness: &UiHarness) -> Option<RowId> {
    harness.state().workspace().unwrap().point_window_row()
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

const INSERT: &str = "Datenpunkt-Abschnitt einfügen (KG 480)";

#[test]
fn insert_a_point_section_fill_its_plant_list_and_undo() {
    let folder = TempDir::new("gu-einfuegen");
    let mut harness = open(&folder, &proava_ui::demo::cost_lv_demo_project());
    let before = project(&harness).lv().net_total();
    assert_eq!(
        project(&harness).lv().purpose(),
        LvPurpose::Cost(CostStage::Calculation)
    );

    open_bar_menu(&mut harness, "Einfügen");
    click_menu(&mut harness, INSERT);
    let lv = project(&harness).lv();
    let sections = lv.point_sections();
    assert_eq!(sections.len(), 1, "ein Datenpunkt-Abschnitt");
    let index = sections[0];
    assert_eq!(lv.rows()[index].ordinal(), "480");
    let id = lv.rows()[index].id();
    assert_eq!(window_row(&harness), Some(id), "Anlagenliste offen");
    assert!(shows_part(&harness, "Datenpunkte – 480"), "Fenstertitel");
    assert!(shows(&harness, "Gesamtkosten KG 481–489 (netto)"));
    assert!(
        !shows(&harness, "Bezeichnung"),
        "Kopf der eigenständigen Kostenermittlung fehlt"
    );

    // A line as in the Reiter Kosten: Ort, Gewerk, Anlage, AI.
    click(&mut harness, "Zeile");
    type_text(&mut harness, "ASP01");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "RLT");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "RLT-01");
    press(&mut harness, Key::Tab);
    type_text(&mut harness, "12");
    press(&mut harness, Key::Enter);
    let lv = project(&harness).lv();
    let points = lv.point_section(id).unwrap();
    assert_eq!(points.sections[0].rows[1].counts[0], 12);
    let amount = Money::from_cents(12 * 40_000);
    assert_eq!(lv.net_total(), before + amount, "zählt zum Kosten-LV");

    // Strg+Z as in the Reiter Kosten: each cell, then the line, then the section.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let points = project(&harness).lv().point_section(id).unwrap().clone();
    assert_eq!(points.sections[0].rows.len(), 2);
    assert_eq!(points.result().points, 0, "die Anzahl zuerst");
    assert_eq!(points.sections[0].rows[1].plant, "RLT-01");
    for _ in 0..3 {
        press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    }
    let points = project(&harness).lv().point_section(id).unwrap().clone();
    assert_eq!(points.sections[0].rows[1].location, "", "Ort zurück");
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        project(&harness).lv().point_section(id).unwrap().sections[0]
            .rows
            .len(),
        1,
        "die Zeile"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert!(project(&harness).lv().point_sections().is_empty());
    assert_eq!(project(&harness).lv().net_total(), before);
    assert_eq!(window_row(&harness), None, "Fenster zu mit dem Titel");
    press_with(&mut harness, Modifiers::COMMAND, Key::Y);
    assert_eq!(project(&harness).lv().point_sections().len(), 1);
}

#[test]
fn only_a_cost_lv_takes_a_point_section() {
    let folder = TempDir::new("gu-nur-kosten-lv");
    let mut harness = open_demo(&folder.0);
    open_bar_menu(&mut harness, "Einfügen");
    assert!(disabled(&harness, INSERT), "im Ausschreibungs-LV gesperrt");
    press(&mut harness, Key::Escape);
}

#[test]
fn double_click_opens_the_plant_list_and_the_panel_sums_per_kg() {
    let folder = TempDir::new("gu-doppelklick");
    let mut harness = open(&folder, &proava_ui::demo::gu_demo_project());
    let lv = project(&harness).lv();
    let index = lv.point_sections()[0];
    let id = lv.rows()[index].id();
    let label = format!(
        "{} {}",
        lv.rows()[index].ordinal(),
        lv.rows()[index].short_text()
    );
    double_click(&mut harness, &label);
    assert_eq!(window_row(&harness), Some(id));
    assert!(
        shows_part(&harness, "Berechnet über Datenpunkte"),
        "Eigenschaften des Titels"
    );
    assert!(shows_part(&harness, "KG 481"), "Summe je KG");
    assert!(shows(&harness, "Anlagenliste öffnen …"));
    press(&mut harness, Key::Escape);
    assert_eq!(window_row(&harness), None, "Esc schließt");
    click(&mut harness, "Anlagenliste öffnen …");
    assert_eq!(window_row(&harness), Some(id));
}

#[test]
fn a_standalone_estimate_moves_into_a_cost_lv() {
    let folder = TempDir::new("gu-uebernehmen");
    let mut harness = open(&folder, &proava_ui::demo::cost_demo_project());
    press_with(&mut harness, Modifiers::COMMAND, Key::Num6);
    let estimates = project(&harness).cost_estimates().len();
    let lvs = project(&harness).lv_count();
    click(&mut harness, "In Kosten-LV übernehmen");
    let now = project(&harness);
    assert_eq!(now.cost_estimates().len(), estimates - 1);
    assert_eq!(now.lv_count(), lvs + 1);
    assert!(
        now.lv().purpose().cost_stage().is_some(),
        "das neue Kosten-LV ist aktiv"
    );
    assert_eq!(now.lv().point_sections().len(), 1);
    assert!(shows_part(&harness, "mit dem Datenpunkt-Abschnitt 480"));
    // One step back: both again as before.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let now = project(&harness);
    assert_eq!(now.cost_estimates().len(), estimates);
    assert_eq!(now.lv_count(), lvs);
}
