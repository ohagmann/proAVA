use rust_decimal::Decimal;

use super::*;
use crate::cost_estimate::CostStage;
use crate::cost_group::cost_group_report;
use crate::cost_lv::{cost_lv_findings, cost_lv_template};
use crate::oz_mask::OzMask;
use crate::project::Project;
use crate::project_name::ProjectName;
use crate::surcharge::{SurchargeKind, SurchargeReference};

fn euro(value: i64) -> Money {
    Money::from_decimal(Decimal::from(value))
}

fn set_quantity(lv: &mut Lv, id: RowId, quantity: i64) {
    let index = lv.index_of(id).unwrap();
    lv.row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_quantity(Decimal::from(quantity));
}

/// A Kostenberechnung KG 480 with the GA template under title 480 and the
/// quantities of one Bauteil (made up, with the sum of a real
/// Kostenberechnung): 100 AI, 150 BI, 20 AO, 15 BO, 11 AV, 10 BV = 118.200 €.
fn ga_calculation() -> (Lv, Vec<RowId>, Vec<RowId>) {
    let mut lv = cost_lv_template(
        CostStage::Calculation,
        "Kostenberechnung KG 480",
        &["480"],
        10,
    );
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    let range = lv.insert_points_template(Some(title)).unwrap();
    let rows: Vec<RowId> = lv.rows()[range].iter().map(LvRow::id).collect();
    assert_eq!(rows.len(), 13, "Titel, 6 Datenpunkte, 6 Aufteilungszeilen");
    let points = rows[1..7].to_vec();
    let splits = rows[7..].to_vec();
    for (id, quantity) in points.iter().zip([100, 150, 20, 15, 11, 10]) {
        set_quantity(&mut lv, *id, quantity);
    }
    (lv, points, splits)
}

#[test]
fn the_ga_template_is_kurztext_with_the_split_of_the_datenpunktrechnung() {
    let (lv, points, splits) = ga_calculation();
    let short: Vec<String> = points
        .iter()
        .map(|id| lv.position_by_id(*id).unwrap().short_text().to_string())
        .collect();
    assert_eq!(short[0], "1.1.1 Analoge Eingabe (AI)");
    assert_eq!(short[4], "1.2.1 Analogwert (AV)");
    let prices: Vec<Decimal> = points
        .iter()
        .map(|id| lv.position_by_id(*id).unwrap().unit_price().unwrap())
        .collect();
    assert_eq!(prices[..4], [Decimal::from(400); 4]);
    assert_eq!(prices[4..], [Decimal::from(200); 2]);
    let codes: Vec<&str> = splits
        .iter()
        .map(|id| lv.position_by_id(*id).unwrap().cost_group().unwrap())
        .collect();
    assert_eq!(codes, ["481", "482", "483", "484", "485", "489"]);
    let title = lv.rows()[lv.index_of(points[0]).unwrap() - 1]
        .as_group()
        .unwrap();
    assert_eq!(title.short_text(), POINTS_TEMPLATE_TITLE);
    assert_eq!(title.level(), 1, "Untertitel von 480");
}

#[test]
fn hit_example_split_carries_the_money_once() {
    let (lv, points, splits) = ga_calculation();
    let values = lv.surcharge_values();
    let amounts: Vec<Money> = splits.iter().map(|id| values[id].amount).collect();
    assert_eq!(
        amounts,
        [47_280, 14_184, 11_820, 35_460, 5_910, 3_546].map(euro),
        "40/12/10/30/5/3 % von 118.200 €"
    );
    assert!(splits.iter().all(|id| values[id].base == euro(118_200)));
    // The Datenpunkte are only the Rechengrundlage: no double count.
    assert_eq!(lv.net_total(), euro(118_200));
    let bases = lv.split_bases();
    assert!(points.iter().all(|id| bases.contains(id)));
    assert!(splits.iter().all(|id| !bases.contains(id)));
    let report = cost_group_report(&lv, &[]);
    let line = |code: &str, subtotal: bool| {
        report
            .lines
            .iter()
            .find(|line| line.code.as_deref() == Some(code) && line.subtotal == subtotal)
            .map(|line| (line.positions, line.planner))
    };
    assert_eq!(line("481", false), Some((1, euro(47_280))));
    assert_eq!(line("480", true), Some((6, euro(118_200))));
    assert_eq!(
        line("480", false),
        None,
        "keine KG 480 aus der Rechengrundlage"
    );
    assert_eq!(report.planner_total, euro(118_200));
    assert!(
        cost_lv_findings(&lv).is_empty(),
        "{:?}",
        cost_lv_findings(&lv)
    );

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
    let sums = project.cost_sums(CostStage::Calculation, 3);
    assert_eq!(sums["484"], euro(35_460));
    assert_eq!(
        project.cost_sums(CostStage::Calculation, 2)["480"],
        euro(118_200)
    );
}

#[test]
fn two_blocks_count_each_on_their_own() {
    // Anlagenautomation and Raumautomation, both under 480.
    let (mut lv, _, _) = ga_calculation();
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    let range = lv.insert_points_template(Some(title)).unwrap();
    let second: Vec<RowId> = lv.rows()[range].iter().map(LvRow::id).collect();
    // 300 AI, 200 BI, 150 AO, 170 BO, 1 AV, 1 BV = 328.400 € (made up).
    for (id, quantity) in second[1..7].iter().zip([300, 200, 150, 170, 1, 1]) {
        set_quantity(&mut lv, *id, quantity);
    }
    assert_eq!(lv.net_total(), euro(118_200 + 328_400));
    let values = lv.surcharge_values();
    assert_eq!(
        values[&second[7]].amount,
        euro(131_360),
        "40 % von 328.400 €"
    );
    assert_eq!(lv.split_blocks().len(), 2);
}

#[test]
fn the_rounding_rest_goes_to_the_largest_share() {
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    // 10,01 € on 40/12/10/30/5/3 %: rounded per line 4,00 + 1,20 + 1,00 + 3,00
    // + 0,50 + 0,30 = 10,00 – the cent goes to 481.
    lv.insert_numbered(
        title + 1,
        LvRow::Position(Position::new(
            "",
            "Datenpunkte",
            Decimal::ONE,
            "psch",
            Some("10.01".parse().unwrap()),
        )),
    )
    .unwrap();
    let range = lv.insert_split_block(Some(title + 1)).unwrap();
    let splits: Vec<RowId> = lv.rows()[range].iter().map(LvRow::id).collect();
    let values = lv.surcharge_values();
    assert_eq!(values[&splits[0]].amount, Money::from_cents(401));
    let total: Money = splits.iter().map(|id| values[id].amount).sum();
    assert_eq!(total, Money::from_cents(1001));
    assert_eq!(lv.net_total(), Money::from_cents(1001));
}

#[test]
fn a_block_without_100_percent_is_found() {
    let (mut lv, _, splits) = ga_calculation();
    let index = lv.index_of(splits[0]).unwrap();
    lv.row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_unit_price(Some(Decimal::from(30)));
    let texts: Vec<String> = cost_lv_findings(&lv)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(
        texts[0].contains("90 % statt 100 %")
            && texts[0].contains("11.820,00 €")
            && texts[0].contains("nicht verteilt"),
        "{texts:?}"
    );
    // No rounding rest then: the block counts what its lines give.
    assert_eq!(lv.net_total(), euro(118_200 - 11_820));
    let block = &lv.split_blocks()[0];
    assert!(!block.is_complete());
}

#[test]
fn a_listed_split_refers_to_positions_elsewhere() {
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    let a = lv.insert_row(
        title + 1,
        LvRow::Position(Position::new(
            "",
            "AI",
            Decimal::from(10),
            "St",
            Some(Decimal::from(400)),
        )),
    );
    let b = lv.insert_row(
        title + 2,
        LvRow::Position(Position::new(
            "",
            "BI",
            Decimal::from(10),
            "St",
            Some(Decimal::from(400)),
        )),
    );
    // A line on AI only (8.000 € × 100 %); BI stays an ordinary position.
    let mut split = Position::surcharge_position(
        "KG 481",
        Surcharge {
            kind: SurchargeKind::Listed,
            references: vec![SurchargeReference {
                position: a,
                quantity: None,
            }],
            split: true,
        },
        Some(Decimal::ONE_HUNDRED),
    );
    split.set_cost_group(Some("481".into()));
    let at = lv.rows().len();
    let id = lv.insert_row(at, LvRow::Position(split));
    assert_eq!(lv.surcharge_values()[&id].amount, euro(4_000));
    let bases = lv.split_bases();
    assert!(bases.contains(&a) && !bases.contains(&b));
    assert_eq!(
        lv.net_total(),
        euro(8_000),
        "AI über die Aufteilung, BI selbst"
    );
}

#[test]
fn only_a_cost_lv_takes_aufteilungszeilen() {
    let mut tender = Lv::new(
        "Ausschreibung",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    assert!(tender
        .insert_split_block(None)
        .unwrap_err()
        .contains("Kosten-LV"));
    assert!(tender
        .insert_points_template(None)
        .unwrap_err()
        .contains("Kosten-LV"));
    // A Kosten-LV that becomes a tender LV keeps them – LV prüfen says so.
    let (mut lv, _, _) = ga_calculation();
    lv.set_purpose(LvPurpose::Tender);
    let texts: Vec<String> = cost_lv_findings(&lv)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(texts.len(), 6, "{texts:?}");
    assert!(texts[0].contains("Aufteilungszeile im Ausschreibungs-LV"));
}

#[test]
fn a_split_without_positions_before_is_found() {
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    lv.insert_split_block(Some(title)).unwrap();
    let texts: Vec<String> = cost_lv_findings(&lv)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert!(
        texts
            .iter()
            .any(|text| text.contains("keine Rechengrundlage")),
        "{texts:?}"
    );
}

#[test]
fn a_block_on_chosen_positions_refers_to_them_as_a_list() {
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
    let title = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "480")
        .unwrap();
    let range = lv.insert_points_template(Some(title)).unwrap();
    let ids: Vec<RowId> = lv.rows()[range.clone()].iter().map(LvRow::id).collect();
    // Remove the template's block: the chosen AI and BI get one of their own.
    for index in (range.start + 7..range.end).rev() {
        lv.remove_row(index);
    }
    set_quantity(&mut lv, ids[1], 10); // AI 4.000 €
    set_quantity(&mut lv, ids[2], 5); // BI 2.000 €
    set_quantity(&mut lv, ids[3], 1); // AO 400 € – stays ordinary
    let block = lv.insert_split_block_on(&[ids[1], ids[2]]).unwrap();
    assert_eq!(
        block.start,
        lv.index_of(ids[2]).unwrap() + 1,
        "hinter der letzten"
    );
    // No free number between 0020 and 0030: the positions of the title got new ones.
    let ordinals: Vec<&str> = lv.positions().map(Position::ordinal).collect();
    assert!(
        ordinals.iter().all(|ordinal| !ordinal.is_empty()),
        "{ordinals:?}"
    );
    let mut unique = ordinals.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ordinals.len(), "{ordinals:?}");
    assert!(
        lv.rows().iter().any(|row| row.ordinal() == "480"),
        "Titel-OZ bleibt"
    );
    let first = lv.rows()[block.start].as_position().unwrap();
    let surcharge = first.surcharge().unwrap();
    assert!(surcharge.split && surcharge.kind == SurchargeKind::Listed);
    assert_eq!(surcharge.references.len(), 2);
    assert_eq!(first.marks(), ["Aufteilung"]);
    assert_eq!(lv.net_total(), euro(6_400));
    assert_eq!(lv.surcharge_values()[&first.id()].amount, euro(2_400));
}

/// GU-Fall → Kurztext: a Datenpunkt-Abschnitt with two Bauteile becomes two
/// subtitles with Datenpunkte and Aufteilung – the sums stay.
#[test]
fn a_point_section_becomes_kurztext_with_the_same_sums() {
    use crate::cost_estimate::PointRow;
    let mut lv = cost_lv_template(CostStage::Calculation, "KB", &["480"], 10);
    let index = lv.insert_point_section().unwrap();
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        let points = group.points_mut().unwrap();
        points.sections[0].name = "Haus A".into();
        points.sections[0].rows[0] = PointRow {
            counts: vec![10, 5, 0, 0, 3, 0],
            ..PointRow::default()
        };
        let mut second = crate::cost_estimate::CostSection::new("Haus B");
        second.rows.push(PointRow {
            counts: vec![7, 0, 2, 0, 0, 1],
            ..PointRow::default()
        });
        second.rows.push(PointRow {
            counts: vec![1, 0, 0, 0, 0, 0],
            ..PointRow::default()
        });
        second.shares = Some(vec![
            CostShare::new("481", Decimal::from(70)),
            CostShare::new("484", Decimal::from(30)),
        ]);
        points.sections.push(second);
    }
    let before = lv.net_total();
    let report_before = cost_group_report(&lv, &[]);
    let range = lv.point_section_to_kurztext(index).unwrap();
    assert!(lv.point_sections().is_empty());
    assert_eq!(lv.net_total(), before, "Summe bleibt");
    let report = cost_group_report(&lv, &[]);
    let amount = |report: &crate::cost_group::CostGroupReport, code: &str| {
        report
            .lines
            .iter()
            .find(|line| line.code.as_deref() == Some(code) && !line.subtotal)
            .map(|line| line.planner)
    };
    for code in ["481", "482", "484", "489"] {
        assert_eq!(
            amount(&report, code),
            amount(&report_before, code),
            "KG {code}"
        );
    }
    let inserted = &lv.rows()[range];
    let titles: Vec<String> = inserted
        .iter()
        .filter(|row| row.is_group())
        .map(|row| row.short_text().to_string())
        .collect();
    assert_eq!(titles, ["Haus A", "Haus B"]);
    // Haus B: 8 AI from two plant lines, its own split 481/484.
    let ai_b = inserted
        .iter()
        .filter_map(LvRow::as_position)
        .filter(|position| position.short_text().ends_with("(AI)"))
        .nth(1)
        .unwrap();
    assert_eq!(ai_b.quantity(), Decimal::from(8));
    let splits: usize = inserted
        .iter()
        .filter_map(LvRow::as_position)
        .filter(|position| {
            position
                .surcharge()
                .is_some_and(|surcharge| surcharge.split)
        })
        .count();
    assert_eq!(splits, 6 + 2);
    assert!(
        cost_lv_findings(&lv).is_empty(),
        "{:?}",
        cost_lv_findings(&lv)
    );

    // A Kostenschätzung with one Kennwert: Datenpunkte × 450 €, no split.
    let mut estimate = cost_lv_template(CostStage::Estimate, "KS", &["480"], 10);
    let index = estimate.insert_point_section().unwrap();
    if let Some(LvRow::Group(group)) = estimate.row_mut(index) {
        group.points_mut().unwrap().sections[0].rows[0].counts = vec![10, 0, 0, 0, 0, 0];
    }
    let before = estimate.net_total();
    assert_eq!(before, euro(4_500));
    estimate.point_section_to_kurztext(index).unwrap();
    assert_eq!(estimate.net_total(), before);
    assert!(estimate.split_blocks().is_empty());
}
