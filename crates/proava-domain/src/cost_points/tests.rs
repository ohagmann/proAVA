use rust_decimal::Decimal;

use super::*;
use crate::cost_estimate::PointRow;
use crate::cost_group::cost_group_report;
use crate::money::Money;
use crate::oz_mask::OzMask;
use crate::position::Position;
use crate::project_name::ProjectName;

fn euro(value: i64) -> Money {
    Money::from_decimal(Decimal::from(value))
}

/// 100 AI in the only Bauteil of the section.
fn hundred_points(points: &mut CostEstimate) {
    points.sections[0].rows[0] = PointRow {
        counts: vec![100, 0, 0, 0, 0, 0],
        ..PointRow::default()
    };
}

/// Kostenberechnung KG 420 + 480 (GU): a Wärmepumpe in 420.421 (60.000 €) and
/// the Datenpunkt-Abschnitt 480 with 100 AI × 400 € = 40.000 €.
fn gu_calculation() -> Lv {
    let mut lv = cost_lv_template(
        CostStage::Calculation,
        "Kostenberechnung KG 400",
        &["420", "480"],
        10,
    );
    let index = lv.insert_point_section().unwrap();
    assert_eq!(
        lv.rows()[index].ordinal(),
        "480",
        "der leere Titel 480 der Vorlage"
    );
    let points = lv.rows()[index]
        .as_group()
        .unwrap()
        .points()
        .unwrap()
        .clone();
    let mut points = points;
    hundred_points(&mut points);
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        group.set_points(Some(points));
    }
    let pump_title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "420.421")
        .unwrap();
    lv.insert_row(
        pump_title + 1,
        LvRow::Position(Position::new(
            "420.421.0010",
            "Wärmepumpe",
            Decimal::from(40),
            "kW",
            Some(Decimal::from(1500)),
        )),
    );
    lv
}

fn project_with(lv: Lv) -> Project {
    let tender = Lv::new(
        "Ausschreibung",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    let mut project = Project::new(
        ProjectName::parse("Rathaus 3").unwrap(),
        "2026-11-19",
        tender,
    );
    project.add_lv(lv);
    project
}

#[test]
fn a_new_section_turns_the_empty_title_480_into_data_points() {
    let lv = gu_calculation();
    let index = lv.point_sections()[0];
    let range = lv.block_range(index);
    assert_eq!(range.len(), 1, "die leeren Untertitel 480.481 … sind weg");
    let group = lv.rows()[index].as_group().unwrap();
    assert_eq!(group.cost_group(), Some("480"));
    assert_eq!(group.points().unwrap().stage, CostStage::Calculation);
}

#[test]
fn mixed_cost_lv_sums_kurztext_and_points_once() {
    let lv = gu_calculation();
    assert_eq!(lv.net_total(), euro(100_000));
    let totals = lv.group_totals();
    let total_of = |ordinal: &str| {
        let index = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == ordinal)
            .unwrap();
        totals
            .iter()
            .find(|total| total.group_row == index)
            .unwrap()
            .total
    };
    assert_eq!(total_of("420"), euro(60_000));
    assert_eq!(total_of("480"), euro(40_000));
    let index = lv.point_sections()[0];
    assert_eq!(lv.block_positions_total(index), euro(40_000));

    let report = cost_group_report(&lv, &[]);
    let planner = |code: &str, subtotal: bool| {
        report
            .lines
            .iter()
            .find(|line| line.code.as_deref() == Some(code) && line.subtotal == subtotal)
            .map(|line| line.planner)
    };
    assert_eq!(planner("421", false), Some(euro(60_000)));
    assert_eq!(planner("481", false), Some(euro(16_000)), "40 %");
    assert_eq!(planner("489", false), Some(euro(1_200)), "3 %");
    assert_eq!(planner("480", true), Some(euro(40_000)), "Summe 481–489");
    assert_eq!(report.planner_total, euro(100_000));

    let project = project_with(lv);
    for (level, code, amount) in [
        (1, "400", 100_000),
        (2, "420", 60_000),
        (2, "480", 40_000),
        (3, "484", 12_000),
    ] {
        assert_eq!(
            project.cost_sums(CostStage::Calculation, level)[code],
            euro(amount),
            "{level} {code}"
        );
    }
    let total: Money = project
        .cost_sums(CostStage::Calculation, 3)
        .values()
        .copied()
        .sum();
    assert_eq!(total, euro(100_000), "nichts doppelt");
}

#[test]
fn an_estimate_section_is_kg_480() {
    let mut lv = cost_lv_template(CostStage::Estimate, "Kostenschätzung KG 400", &["420"], 10);
    let index = lv.insert_point_section().unwrap();
    assert_eq!(lv.rows()[index].ordinal(), "480", "neuer Titel nach 420");
    assert_eq!(index, lv.rows().len() - 1);
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        hundred_points(group.points_mut().unwrap());
    }
    assert_eq!(lv.net_total(), euro(45_000), "100 DP × 450 €");
    let project = project_with(lv);
    assert_eq!(
        project.cost_sums(CostStage::Estimate, 2)["480"],
        euro(45_000)
    );
    assert_eq!(
        project.cost_sums(CostStage::Estimate, 3)["480"],
        euro(45_000)
    );
}

#[test]
fn a_new_title_480_comes_before_490_and_tender_lvs_have_none() {
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["420", "490"], 10);
    // 480 is not in the template: a new title between 420 and 490.
    let index = lv.insert_point_section().unwrap();
    assert_eq!(lv.rows()[index].ordinal(), "480");
    assert_eq!(lv.rows()[index + 1].ordinal(), "490");
    // A second one: 480 is taken – it still comes, and LV prüfen says so.
    let second = lv.insert_point_section().unwrap();
    assert_ne!(lv.rows()[second].ordinal(), "480");
    let texts: Vec<String> = point_section_findings(&lv)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.contains("schon im Datenpunkt-Abschnitt 480")),
        "{texts:?}"
    );

    let mut tender = Lv::new(
        "Ausschreibung",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    assert!(tender
        .insert_point_section()
        .unwrap_err()
        .contains("Kosten-LV"));
}

#[test]
fn sections_follow_the_stage_and_leave_tender_lvs() {
    let mut estimate = cost_lv_template(CostStage::Estimate, "Kostenschätzung GU", &["480"], 10);
    let index = estimate.insert_point_section().unwrap();
    if let Some(LvRow::Group(group)) = estimate.row_mut(index) {
        hundred_points(group.points_mut().unwrap());
    }
    // Kostenberechnung daraus: the section becomes one (EP je DP-Art, split).
    let calculation = crate::cost_lv::calculation_from(&estimate);
    let points = calculation.rows()[index]
        .as_group()
        .unwrap()
        .points()
        .unwrap();
    assert_eq!(points.stage, CostStage::Calculation);
    assert_eq!(
        points.sections,
        estimate.rows()[index]
            .as_group()
            .unwrap()
            .points()
            .unwrap()
            .sections
    );
    assert_eq!(calculation.net_total(), euro(40_000), "100 AI × 400 €");
    // Pasted into a tender LV: the title stays, without data points.
    let mut tender = calculation.clone();
    tender.set_purpose(LvPurpose::Tender);
    assert_eq!(tender.fit_point_sections(), 0, "set_purpose passt schon an");
    assert!(tender.point_sections().is_empty());
    assert_eq!(tender.rows()[index].ordinal(), "480");
    // Back to a Kostenschätzung: Kennwert je DP-Art.
    let mut back = calculation.clone();
    back.set_purpose(LvPurpose::Cost(CostStage::Estimate));
    let points = back.rows()[index].as_group().unwrap().points().unwrap();
    assert!(points.stage == CostStage::Estimate && points.rate_per_kind);
    assert_eq!(back.net_total(), euro(40_000));
}

#[test]
fn kg_counted_twice_is_found() {
    let mut lv = gu_calculation();
    assert!(
        point_section_findings(&lv).is_empty(),
        "{:?}",
        point_section_findings(&lv)
    );
    // A Kurztext position with KG 481 next to the section.
    let mut ddc = Position::new(
        "420.421.0020",
        "DDC",
        Decimal::ONE,
        "St",
        Some(Decimal::from(5000)),
    );
    ddc.set_cost_group(Some("481".into()));
    let at = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "420.421.0010")
        .unwrap()
        + 1;
    lv.insert_row(at, LvRow::Position(ddc));
    let texts: Vec<String> = point_section_findings(&lv)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(
        texts[0].contains("KG 481 ist auch im Datenpunkt-Abschnitt 480"),
        "{texts:?}"
    );
    // In a Kostenschätzung any KG 48x is the section's.
    let mut estimate = cost_lv_template(CostStage::Estimate, "KS", &["480"], 10);
    estimate.insert_point_section().unwrap();
    let mut cable = Position::new("0010", "Kabel", Decimal::ONE, "psch", Some(Decimal::ONE));
    cable.set_cost_group(Some("484".into()));
    estimate.push_position(cable);
    assert!(point_section_findings(&estimate)
        .iter()
        .any(|(_, text)| text.contains("KG 484 ist auch")));
    // Notes of the section itself (split ≠ 100 %).
    let index = lv.point_sections()[0];
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        group.points_mut().unwrap().shares[0].percent = Decimal::from(30);
    }
    assert!(crate::cost_lv::cost_lv_findings(&lv)
        .iter()
        .any(|(_, text)| text.starts_with("Datenpunkte 480:")));
}

#[test]
fn a_standalone_estimate_moves_into_a_cost_lv_and_counts_once() {
    let mut project = project_with(gu_calculation());
    let mut standalone = CostEstimate::template(CostStage::Calculation, "12.11.2026", "Planer");
    standalone.title = "Kostenberechnung LPH 3".into();
    hundred_points(&mut standalone);
    let id = project.add_cost_estimate(standalone.clone());
    // Before: two sources of KG 481 (the Kosten-LV and the standalone one).
    assert_eq!(project.cost_sources(3).len(), 2);
    assert_eq!(
        project.cost_sums(CostStage::Calculation, 3)["481"],
        euro(32_000)
    );

    let index = project.estimate_to_cost_lv(id, "KB2").unwrap();
    assert!(
        project.cost_estimates().is_empty(),
        "verschoben, nicht kopiert"
    );
    let lv = project.lv_at(index).unwrap();
    assert_eq!(lv.purpose(), LvPurpose::Cost(CostStage::Calculation));
    assert_eq!(lv.title(), "Kostenberechnung LPH 3");
    assert_eq!(lv.number(), "KB2");
    let sections = lv.point_sections();
    assert_eq!(sections.len(), 1);
    let points = lv.rows()[sections[0]].as_group().unwrap().points().unwrap();
    assert_eq!(points.sections, standalone.sections);
    assert_eq!(lv.net_total(), euro(40_000));
    let note = lv.rows()[0].as_note().unwrap().text().to_plain();
    assert!(note.contains("Kostenstand: 12.11.2026"), "{note}");
    // Two Kosten-LVs now, each counted once.
    assert_eq!(project.cost_sources(3).len(), 2);
    assert_eq!(
        project.cost_sums(CostStage::Calculation, 3)["481"],
        euro(32_000)
    );
    assert!(
        project.estimate_to_cost_lv(id, "X").is_err(),
        "schon übernommen"
    );
}
