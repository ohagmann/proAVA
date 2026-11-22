//! Reiter Kostenverfolgung mit dem synthetischen Projekt „Demo
//! Kostenverfolgung“ (GA, Heizung, Lüftung, Sanitär, Kosten-LVs): Kette je
//! KG, Ebene, Ansicht nach Vergabeeinheiten, netto/brutto, Erläuterung,
//! Budget, Angebotswahl, Kostenstand und Sprung zu Positionen ohne KG –
//! alles mit Strg+Z.

mod common;

use common::*;
use proava_domain::{
    cost_tracking, LvGroup, Money, Position, Project, TrackOptions, TrackStage, TrackView,
};
use proava_ui::StartFile;

fn open_project(folder: &TempDir, project: &Project) -> UiHarness {
    let file = save(&folder.0, project);
    let mut harness = harness(&folder.0, Some(StartFile::Project(file)));
    assert!(
        harness.state().project().is_some(),
        "Projekt nicht geöffnet"
    );
    press_with(&mut harness, Modifiers::COMMAND, Key::Num7);
    harness
}

fn open(folder: &TempDir) -> UiHarness {
    open_project(folder, &proava_ui::demo::tracking_demo_project())
}

#[test]
fn the_chain_over_all_trades_by_level_and_by_vergabeeinheit() {
    let folder = TempDir::new("kostenverfolgung");
    let mut harness = open(&folder);
    assert!(
        shows(&harness, "Kostenverfolgung"),
        "Reiter in der Kopfzeile"
    );
    // Ebene 2 by default.
    for row in [
        "400 Bauwerk – Technische Anlagen",
        "410 Abwasser-, Wasser-, Gasanlagen",
        "420 Wärmeversorgungsanlagen",
        "430 Raumlufttechnische Anlagen",
        "480 Gebäude- und Anlagenautomation",
        "Gesamt",
    ] {
        assert!(shows(&harness, row), "Zeile {row}");
    }
    assert!(!shows(&harness, "421 Wärmeerzeugungsanlagen"));
    click(&mut harness, "3");
    assert!(shows(&harness, "421 Wärmeerzeugungsanlagen"));
    assert!(shows(&harness, "431 Lüftungsanlagen"));
    // The view computes what the domain computes.
    let report = cost_tracking(
        project(&harness),
        &TrackOptions {
            level: 3,
            ..TrackOptions::default()
        },
    );
    for stage in [
        TrackStage::Budget,
        TrackStage::Estimate,
        TrackStage::Calculation,
        TrackStage::PreTender,
        TrackStage::Offer,
        TrackStage::Contract,
        TrackStage::Forecast,
        TrackStage::Billed,
        TrackStage::Final,
    ] {
        assert!(
            report
                .total
                .value(stage)
                .is_some_and(|value| value.cents() > 0),
            "Spalte {} im Demo-Projekt",
            stage.label()
        );
    }
    assert_eq!(report.final_lvs, 1);
    assert_eq!(report.contract_lvs, 2);
    assert!(shows_part(
        &harness,
        "1 von 2 beauftragten LVs schlussgerechnet"
    ));

    click(&mut harness, "nach Vergabeeinheiten");
    for row in [
        "LV 480 Gebäudeautomation",
        "LV 420 Heizung",
        "LV 430 Lüftung",
        "LV 410 Sanitär",
    ] {
        assert!(shows(&harness, row), "Zeile {row}");
    }
    assert!(
        !shows(&harness, "LV KB Kostenberechnung Technik"),
        "Kosten-LVs sind keine Vergabeeinheit"
    );
    assert!(shows_part(&harness, "ohne Vergabeeinheit"));
}

#[test]
fn gross_and_the_note_of_a_cost_group_with_undo() {
    let folder = TempDir::new("kostenverfolgung-erlaeuterung");
    let mut harness = open(&folder);
    click(&mut harness, "brutto");
    assert!(shows_part(&harness, "brutto (USt 19 %)"));
    click(&mut harness, "netto");

    click(&mut harness, "430 Raumlufttechnische Anlagen");
    assert!(shows(&harness, "Erläuterung / Maßnahme KG 430"));
    let before = project(&harness).cost_tracking().clone();
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .click();
    settle(&mut harness);
    type_text(&mut harness, "Nachtrag Brandschutzklappen prüfen");
    assert_eq!(
        project(&harness)
            .cost_tracking()
            .notes
            .get("430")
            .map(String::as_str),
        Some("Nachtrag Brandschutzklappen prüfen")
    );
    press(&mut harness, Key::Escape);
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(*project(&harness).cost_tracking(), before);
}

#[test]
fn the_budget_is_taken_over_from_a_cost_calculation_and_undone() {
    let folder = TempDir::new("kostenverfolgung-budget");
    let mut harness = open(&folder);
    let before = project(&harness).cost_tracking().budget.clone();
    click(&mut harness, "Kostenvorgabe (Budget) …");
    assert!(shows(&harness, "Kostenvorgabe (Budget)"));
    // First choice: Kostenberechnung (alle geltenden) – Kosten-LV and GA.
    click(&mut harness, "übernehmen");
    click(&mut harness, "Übernehmen");
    let budget = project(&harness).cost_tracking().budget.clone();
    assert_ne!(budget, before);
    let calculation = project(&harness).cost_sums(proava_domain::CostStage::Calculation, 3);
    let expected: Money = calculation
        .iter()
        .filter(|(code, _)| !code.is_empty())
        .map(|(_, amount)| *amount)
        .sum();
    assert_eq!(budget.values().copied().sum::<Money>(), expected);
    assert!(budget.contains_key("421") && budget.contains_key("481"));
    assert!(project(&harness)
        .cost_tracking()
        .budget_note
        .starts_with("aus Kostenberechnung"));
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).cost_tracking().budget, before);
}

#[test]
fn the_bid_of_an_lv_without_award_is_chosen_with_undo() {
    let folder = TempDir::new("kostenverfolgung-angebot");
    let mut harness = open(&folder);
    click(&mut harness, "nach Vergabeeinheiten");
    click(&mut harness, "LV 420 Heizung");
    assert!(shows(&harness, "Angebot für die Kostenverfolgung"));
    harness
        .query_all_by(|node| {
            node.role() == egui::accesskit::Role::ComboBox
                && node.label().as_deref() == Some("Angebot für die Kostenverfolgung")
        })
        .next()
        .expect("Auswahl des Angebots")
        .click();
    settle(&mut harness);
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Heizungsbau Süd KG – "))
        })
        .next()
        .expect("Angebot in der Liste")
        .click();
    settle(&mut harness);
    let heating = project(&harness)
        .lvs()
        .position(|lv| lv.title() == "Heizung")
        .unwrap();
    let chosen = project(&harness)
        .offer_choice_of(heating)
        .cloned()
        .expect("gewählt");
    let name = project(&harness)
        .bids_of(heating)
        .iter()
        .find(|bid| bid.id() == chosen.bid)
        .map(|bid| bid.name().to_string());
    assert_eq!(name.as_deref(), Some("Heizungsbau Süd KG"));
    let report = cost_tracking(
        project(&harness),
        &TrackOptions {
            view: TrackView::ByLv,
            ..TrackOptions::default()
        },
    );
    let lv = report.lvs.iter().find(|lv| lv.index == heating).unwrap();
    assert!(lv
        .offer
        .as_deref()
        .unwrap()
        .starts_with("Heizungsbau Süd KG (gewählt"));
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).offer_choice_of(heating), None);
}

#[test]
fn a_cost_state_is_recorded_and_undone() {
    let folder = TempDir::new("kostenverfolgung-stand");
    let mut harness = open(&folder);
    assert_eq!(project(&harness).cost_tracking().snapshots.len(), 1);
    click(&mut harness, "Kostenstand festhalten …");
    assert!(shows(&harness, "Kostenstand festhalten"));
    click(&mut harness, "Festhalten");
    let tracking = project(&harness).cost_tracking();
    assert_eq!(tracking.snapshots.len(), 2);
    assert!(!tracking.snapshots[1].cells.is_empty());
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    assert_eq!(project(&harness).cost_tracking().snapshots.len(), 1);
}

#[test]
fn a_position_without_cost_group_is_shown_in_the_lv() {
    let folder = TempDir::new("kostenverfolgung-ohne-kg");
    let mut demo = proava_ui::demo::tracking_demo_project();
    let index = demo.lvs().position(|lv| lv.title() == "Sanitär").unwrap();
    let lv = demo.lv_at_mut(index).unwrap();
    lv.push_group(LvGroup::new(0, "", "Sonstiges"));
    let row = lv.push_position(Position::new(
        "",
        "Druckprobe",
        proava_domain::Decimal::ONE,
        "psch",
        Some(proava_domain::Decimal::from(900)),
    ));
    lv.renumber();
    let mut harness = open_project(&folder, &demo);
    click(&mut harness, "ohne Kostengruppe");
    assert!(shows(&harness, "Ohne Kostengruppe (1)"));
    click(&mut harness, "zeigen");
    assert_eq!(project(&harness).active_lv(), index);
    let selected = selected(&harness).expect("Zeile gewählt");
    assert_eq!(project(&harness).lv().rows()[selected].id(), row);
}

#[test]
fn a_plain_tender_project_shows_no_tracking_tab() {
    let folder = TempDir::new("kostenverfolgung-kein-reiter");
    let harness = open_demo(&folder.0);
    assert!(!shows(&harness, "Kostenverfolgung"));
}

#[test]
fn the_table_is_exported_as_csv_as_shown() {
    let folder = TempDir::new("kostenverfolgung-csv");
    let mut harness = open(&folder);
    click(&mut harness, "3");
    click(&mut harness, "CSV …");
    assert!(shows(&harness, "Kostenverfolgung als CSV"));
    harness
        .get_all_by_label("Speichern")
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    for _ in 0..200 {
        harness.step();
    }
    let file = std::fs::read_dir(&folder.0)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "csv"))
        .expect("CSV geschrieben");
    assert!(file
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("Kostenverfolgung"));
    let text = String::from_utf8(std::fs::read(&file).unwrap()[3..].to_vec()).unwrap();
    assert!(text.starts_with("Zeile;KG;Bezeichnung;Vergabeeinheit;Kostenvorgabe netto;"));
    assert!(text.contains("\r\nKostengruppe;421;Wärmeerzeugungsanlagen;;"));
    assert!(text.contains("\r\nGesamt;;Gesamt;;638000,00;"));
}

#[test]
fn print_takes_the_view_of_the_tab_and_writes_the_pdf() {
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::NodeT;
    let folder = TempDir::new("kostenverfolgung-druck");
    let mut harness = open(&folder);
    click(&mut harness, "3");
    click(&mut harness, "zur Vorstufe");
    click(&mut harness, "Drucken …");
    let chosen = harness
        .query_all_by(|node| node.label().as_deref() == Some("Kostenverfolgung (DIN 276)"))
        .find_map(|node| node.accesskit_node().toggled());
    assert_eq!(chosen, Some(Toggled::True), "Druckart gewählt");
    assert!(shows_part(
        &harness,
        "Ebene 3, nach Kostengruppen, netto, Abweichung zur Vorstufe"
    ));
    assert!(shows(&harness, "Diagramm der Stufen je Kostengruppe"));
    press(&mut harness, Key::Enter);
    assert!(
        shows_part(&harness, "Kostenverfolgung"),
        "Dateiname-Vorschlag"
    );
    harness
        .get_all_by_label(proava_ui::strings::FD_SAVE)
        .last()
        .expect("Speichern")
        .click();
    settle(&mut harness);
    let mut written = None;
    for _ in 0..1500 {
        harness.step();
        written = std::fs::read_dir(&folder.0)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.extension().is_some_and(|extension| extension == "pdf")
                    && path.to_string_lossy().contains("Kostenverfolgung")
            });
        if written.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pdf = std::fs::read(written.expect("PDF geschrieben")).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn a_recorded_state_is_compared_in_every_stage() {
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::NodeT;
    let folder = TempDir::new("kostenverfolgung-vergleich");
    let mut harness = open(&folder);
    // Without a recorded state there is no „zum Stand“.
    assert!(!shows(&harness, "zum Stand"));
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::ComboBox)
        .find(|node| node.value().as_deref() == Some("kein Kostenstand"))
        .expect("Auswahl Vergleich")
        .click();
    settle(&mut harness);
    click(
        &mut harness,
        "Stand 01.10.2026 – Kostenberechnung abgestimmt",
    );
    // Choosing it shows the change of every stage since then.
    let toggled = harness
        .query_all_by(|node| node.label().as_deref() == Some("zum Stand"))
        .find_map(|node| node.accesskit_node().toggled());
    assert_eq!(toggled, Some(Toggled::True));
    // The side panel (Gesamt) names the state for each stage.
    assert!(shows_part(&harness, "Stand 01.10.2026: "));
    let report = proava_domain::cost_tracking(
        project(&harness),
        &proava_domain::TrackOptions {
            snapshot: Some(0),
            deviation: proava_domain::TrackDeviation::Snapshot,
            ..TrackOptions::default()
        },
    );
    // Since then the Lüftung got a change order in the forecast and its
    // first invoice: Prognose and Abrechnung moved, the Kostenberechnung not.
    let total = &report.total;
    assert!(total
        .to_snapshot(TrackStage::Forecast)
        .is_some_and(|deviation| deviation.amount.cents() > 0));
    assert_eq!(
        total
            .to_snapshot(TrackStage::Calculation)
            .map(|deviation| deviation.amount),
        Some(Money::ZERO)
    );
    assert!(total.compared(TrackStage::Budget).is_some());
    // Back to no state: back to the deviation „zur Kostenvorgabe“.
    harness
        .query_all_by(|node| node.role() == egui::accesskit::Role::ComboBox)
        .find(|node| {
            node.value()
                .as_deref()
                .is_some_and(|value| value.starts_with("Stand 01.10.2026"))
        })
        .expect("Auswahl Vergleich")
        .click();
    settle(&mut harness);
    click(&mut harness, "kein Kostenstand");
    let toggled = harness
        .query_all_by(|node| node.label().as_deref() == Some("zur Kostenvorgabe"))
        .find_map(|node| node.accesskit_node().toggled());
    assert_eq!(toggled, Some(Toggled::True));
    assert!(!shows(&harness, "zum Stand"));
}
