//! Kosten-LV (Kostenermittlung als Kurztext-LV): aus dem Reiter Kosten
//! anlegen, im Reiter LV bearbeiten, Vergabe gesperrt, Druckart
//! „Kostenermittlung (Kosten-LV)“, Kostenberechnung aus Kostenschätzung.

mod common;

use common::*;
use egui_kittest::kittest::NodeT;
use proava_domain::{CostStage, LvPurpose, LvRow};

fn open_costs(folder: &TempDir) -> UiHarness {
    let mut harness = open_demo(&folder.0);
    press_with(&mut harness, Modifiers::COMMAND, Key::Num6);
    harness
}

/// Whether the widget with this label (a menu entry also with its shortcut
/// behind) is greyed out.
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

#[test]
fn cost_lv_from_the_costs_tab_is_a_kurztext_lv_without_tender() {
    let folder = TempDir::new("kosten-lv");
    let mut harness = open_costs(&folder);
    let tender_lvs = project(&harness).lv_count();
    assert!(shows(&harness, "Nach Positionen (Kurztext-LV)"));
    assert!(shows(
        &harness,
        "Nach Datenpunkten (Gebäudeautomation, KG 480)"
    ));
    click(&mut harness, "Kostenschätzung als Kurztext-LV …");
    // Dialog „Neues LV“ with the Art set and the titles after DIN 276.
    assert!(shows(&harness, "Neues LV"));
    assert!(shows(
        &harness,
        "Titel nach Kostengruppen (DIN 276, KG 400)"
    ));
    assert!(shows(&harness, "111.PPPP"), "Maske der Kostenschätzung");
    // Only 420 and 430.
    for code in ["410", "440", "450", "460", "470", "480", "490"] {
        click(&mut harness, code);
    }
    click(&mut harness, "Anlegen");

    let project_now = project(&harness);
    assert_eq!(project_now.lv_count(), tender_lvs + 1);
    let lv = project_now.lv();
    assert_eq!(lv.purpose(), LvPurpose::Cost(CostStage::Estimate));
    let titles: Vec<&str> = lv
        .rows()
        .iter()
        .filter_map(|row| match row {
            LvRow::Group(group) => Some(group.ordinal()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["420", "430"]);
    assert!(
        shows(
            &harness,
            "Kostenschätzung nach DIN 276 (2. Ebene) – Kosten-LV, nicht zur Ausschreibung"
        ),
        "Hinweiszeile über der Tabelle"
    );
    assert!(shows_part(&harness, "Kostenschätzung KG 400"), "LV-Auswahl");

    // Angebote: a note instead of the tab, back to the LV.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num2);
    assert!(
        shows_part(&harness, "ist eine Kostenschätzung"),
        "Hinweis im Reiter Angebote"
    );
    click(&mut harness, "Zum Kosten-LV");
    assert!(shows_part(&harness, "Kosten-LV, nicht zur Ausschreibung"));

    // The menu Angebote: reading bids is greyed out.
    open_bar_menu(&mut harness, "Angebote");
    assert!(
        disabled(&harness, proava_ui::strings::MENU_IMPORT_BIDS),
        "Angebote einlesen gesperrt"
    );
    press(&mut harness, Key::Escape);

    // Strg+P: the Kostenermittlung of the Kosten-LV, tender printouts greyed out.
    press_with(&mut harness, Modifiers::COMMAND, Key::P);
    assert!(
        shows(&harness, "Kurztext-Liste der Positionen"),
        "Optionen des Kosten-LV"
    );
    assert!(disabled(&harness, "LV ohne Preise (Angebotsaufforderung)"));
    assert!(disabled(&harness, "Niederschrift über die Angebotsöffnung"));
    click(&mut harness, "Abbrechen");

    // Kosten ▸ Kostenberechnung aus Kostenschätzung (Kurztext-LV): a copy.
    press_with(&mut harness, Modifiers::COMMAND, Key::Num6);
    assert!(
        shows_part(&harness, "Kostenschätzung KG 400 · 0,00 €"),
        "Kosten-LV im Reiter Kosten"
    );
    open_bar_menu(&mut harness, "Kosten");
    click_menu(
        &mut harness,
        "Kostenberechnung aus Kostenschätzung (Kurztext-LV)",
    );
    let project_now = project(&harness);
    assert_eq!(project_now.lv_count(), tender_lvs + 2);
    assert_eq!(
        project_now.lv().purpose(),
        LvPurpose::Cost(CostStage::Calculation)
    );
    assert_eq!(project_now.lv().title(), "Kostenberechnung KG 400");
    assert_eq!(project_now.cost_lv_indices().len(), 2);
}

#[test]
fn a_tender_lv_with_bids_stays_a_tender_lv() {
    let folder = TempDir::new("kosten-lv-art");
    let mut harness = open_demo(&folder.0);
    assert!(!project(&harness).bids().is_empty(), "Demo mit Angeboten");
    open_bar_menu(&mut harness, "LV");
    click_menu(&mut harness, "LV-Eigenschaften …");
    assert!(shows(&harness, "Eigenschaften: LV"));
    click(&mut harness, "Kostenberechnung");
    assert!(shows_part(
        &harness,
        "Das LV hat schon 4 Angebote – es bleibt ein Ausschreibungs-LV."
    ));
    click(&mut harness, "Übernehmen");
    assert!(project(&harness).lv().purpose().is_tender());
    assert!(
        shows_part(&harness, "Für die Kostenermittlung ein neues LV anlegen"),
        "Fehlerzeile im Dialog, der offen bleibt"
    );
}
