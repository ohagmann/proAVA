//! Kostenverfolgung with a synthetic project, figures by hand in the
//! comments: Heizung (LV 1, KG 421/422, two bids, no award) and
//! Gebäudeautomation (LV 2, KG 481/484, awarded with 2 % discount, change
//! orders, invoices), a GA-Kostenberechnung and -schätzung.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::*;
use crate::bid::{Address, Bid, Discount};
use crate::billing::{Invoice, InvoiceItem, InvoiceKind, Mode};
use crate::change_order::{ChangeOrder, ChangeOrderId, ChangeOrderItem, ItemChange};
use crate::cost_demo::demo_cost_estimate;
use crate::cost_estimate::CostStage;
use crate::lv::{Lv, LvGroup};
use crate::lv_purpose::LvPurpose;
use crate::oz_mask::OzMask;
use crate::position::Position;
use crate::project_name::ProjectName;

fn eur(value: i64) -> Money {
    Money::from_cents(value * 100)
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

struct Fixture {
    project: Project,
    /// Bids on Heizung: 53.000 € and 57.000 €.
    cheap: BidId,
    dear: BidId,
    /// GA rows: DDC (481), Kabel (484), Nachtragsposition (481).
    ddc: RowId,
    cable: RowId,
    extra: RowId,
}

fn group(lv: &mut Lv, code: &str, text: &str) {
    let mut group = LvGroup::new(0, "", text);
    group.set_cost_group(Some(code.into()));
    lv.push_group(group);
}

fn position(lv: &mut Lv, text: &str, quantity: &str, unit: &str, price: Option<&str>) -> RowId {
    lv.push_position(Position::new("", text, dec(quantity), unit, price.map(dec)))
}

/// Heizung: Wärmepumpe 1 St × 50.000 (421), Rohr 200 m × 50 (422) =
/// 60.000 € Kostenvoranschlag.
fn heating() -> (Lv, RowId, RowId) {
    let mut lv = Lv::new("Heizung", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
    lv.set_number("1");
    group(&mut lv, "421", "Wärmeerzeugung");
    let pump = position(&mut lv, "Wärmepumpe", "1", "St", Some("50000"));
    group(&mut lv, "422", "Wärmeverteilnetze");
    let pipe = position(&mut lv, "Rohrleitung", "200", "m", Some("50"));
    lv.renumber();
    (lv, pump, pipe)
}

/// GA: DDC 10 St × 2.000 (481), Kabel 1000 m × 10 (484) = 30.000 €; the
/// Nachtragsposition under 481 has no planner price.
fn automation() -> (Lv, RowId, RowId, RowId) {
    let mut lv = Lv::new(
        "Gebäudeautomation",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    lv.set_number("2");
    group(&mut lv, "481", "Automationseinrichtungen");
    let ddc = position(&mut lv, "DDC-Station", "10", "St", Some("2000"));
    group(&mut lv, "484", "Kabel");
    let cable = position(&mut lv, "Kabel", "1000", "m", Some("10"));
    group(&mut lv, "481", "Nachträge");
    let extra = position(&mut lv, "DDC-Station zusätzlich", "2", "St", None);
    lv.renumber();
    (lv, ddc, cable, extra)
}

fn bid(name: &str, prices: &[(RowId, &str)]) -> Bid {
    let mut bid = Bid::new(Address::named(name), format!("{name}.X84"));
    for (row, price) in prices {
        bid.set_unit_price(*row, Some(dec(price)));
    }
    bid
}

/// The project: Heizung with two bids, GA awarded (bid 28.000 €, 2 %
/// Nachlass → 27.440 €), N01 approved (2 St × 1.500 = 3.000 → 2.940 after
/// discount), N02 open in the forecast (Kabel +100 m × 9 = 900 → 882).
fn fixture() -> Fixture {
    let (heat, pump, pipe) = heating();
    let mut project = Project::new(
        ProjectName::parse("Rathaus 3").unwrap(),
        "2026-11-18T12:00:00",
        heat,
    );
    // B1: 45.000 + 200 × 40 = 53.000; B2: 48.000 + 200 × 45 = 57.000.
    let cheap = project.add_bid_to(0, bid("Wärme Nord", &[(pump, "45000"), (pipe, "40")]));
    let dear = project.add_bid_to(0, bid("Wärme Süd", &[(pump, "48000"), (pipe, "45")]));

    let (ga, ddc, cable, extra) = automation();
    let index = project.add_lv(ga);
    let mut offer = bid("Regeltechnik West", &[(ddc, "1900"), (cable, "9")]);
    offer.set_discount(Discount::Percent(dec("2")));
    let offer = project.add_bid_to(index, offer);
    project.set_active_lv(index);
    project.award_bid(offer, "01.12.2026").unwrap();
    project.take_over_contract().unwrap();

    let mut approved = ChangeOrder::new(ChangeOrderId(0), 1, "Zusätzliche DDC");
    approved.status = ChangeOrderStatus::Approved;
    approved.items = vec![ChangeOrderItem {
        position: extra,
        offered_quantity: dec("2"),
        offered_unit_price: Some(dec("1500")),
        ..ChangeOrderItem::default()
    }];
    project.add_change_order_to(index, approved);
    let mut open = ChangeOrder::new(ChangeOrderId(0), 2, "Mehr Kabel");
    open.status = ChangeOrderStatus::Offered;
    open.items = vec![ChangeOrderItem {
        position: cable,
        change: ItemChange::Quantity,
        offered_quantity: dec("100"),
        ..ChangeOrderItem::default()
    }];
    project.add_change_order_to(index, open);
    project.set_active_lv(0);

    project.add_cost_estimate(demo_cost_estimate(CostStage::Estimate));
    project.add_cost_estimate(demo_cost_estimate(CostStage::Calculation));
    Fixture {
        project,
        cheap,
        dear,
        ddc,
        cable,
        extra,
    }
}

fn level(level: u8) -> TrackOptions {
    TrackOptions {
        level,
        ..TrackOptions::default()
    }
}

fn row<'a>(report: &'a TrackReport, code: &str) -> &'a TrackRow {
    report
        .rows
        .iter()
        .find(|row| row.code.as_deref() == Some(code) && row.kind != TrackRowKind::Lv)
        .unwrap_or_else(|| panic!("Zeile {code} fehlt: {:?}", codes(report)))
}

fn codes(report: &TrackReport) -> Vec<String> {
    report.rows.iter().map(TrackRow::title).collect()
}

fn value(report: &TrackReport, code: &str, stage: TrackStage) -> Money {
    row(report, code).value(stage).unwrap_or_default()
}

/// Kostenberechnung of the demo per KG.
fn calculation(project: &Project) -> BTreeMap<String, Money> {
    project
        .cost_estimates()
        .iter()
        .find(|estimate| estimate.stage == CostStage::Calculation)
        .unwrap()
        .result()
        .groups
        .into_iter()
        .map(|group| (group.code, group.amount))
        .collect()
}

fn add_invoice(project: &mut Project, kind: InvoiceKind, number: &str, items: &[(RowId, &str)]) {
    let mut invoice = Invoice::new(kind, number, "28.02.2027");
    invoice.quantities = Mode::Cumulative;
    invoice.release = Mode::Cumulative;
    for (row, quantity) in items {
        invoice.items.insert(
            *row,
            InvoiceItem {
                claimed_quantity: Some(dec(quantity)),
                ..InvoiceItem::default()
            },
        );
    }
    project.billing_of_mut(1).unwrap().add_invoice(invoice);
}

#[test]
fn the_chain_per_cost_group_from_estimate_to_forecast() {
    let fixture = fixture();
    let report = cost_tracking(&fixture.project, &level(3));
    let kb = calculation(&fixture.project);

    // Outline: 400 › 420 › 421, 422 › 480 › 481 …
    let titles = codes(&report);
    let at = |code: &str| {
        titles
            .iter()
            .position(|title| title.starts_with(code))
            .unwrap()
    };
    assert!(at("400") < at("420") && at("420") < at("421") && at("421") < at("422"));
    assert!(at("422") < at("480") && at("480") < at("481") && at("481") < at("484"));
    assert_eq!(row(&report, "400").kind, TrackRowKind::Group);
    assert_eq!(row(&report, "481").depth, 2);

    // Kostenvoranschlag = planner's prices.
    assert_eq!(value(&report, "421", TrackStage::PreTender), eur(50_000));
    assert_eq!(value(&report, "422", TrackStage::PreTender), eur(10_000));
    assert_eq!(value(&report, "420", TrackStage::PreTender), eur(60_000));
    // Angebot Heizung = lowest bid; GA = awarded bid after discount, shared out:
    // 27.440 × 19.000 / 28.000 = 18.620 and 8.820.
    assert_eq!(value(&report, "421", TrackStage::Offer), eur(45_000));
    assert_eq!(value(&report, "420", TrackStage::Offer), eur(53_000));
    assert_eq!(value(&report, "481", TrackStage::Offer), eur(18_620));
    assert_eq!(value(&report, "484", TrackStage::Offer), eur(8_820));
    // Auftrag with N01 (3.000 − 2 % = 2.940).
    assert_eq!(value(&report, "481", TrackStage::Contract), eur(21_560));
    assert_eq!(value(&report, "484", TrackStage::Contract), eur(8_820));
    assert_eq!(row(&report, "420").value(TrackStage::Contract), None);
    // Prognose: contract + open N02 (882); Heizung from its bid; KG of no LV
    // from the Kostenberechnung (483, 485 …), not those the LVs hold.
    assert_eq!(value(&report, "484", TrackStage::Forecast), eur(9_702));
    assert_eq!(value(&report, "420", TrackStage::Forecast), eur(53_000));
    assert_eq!(value(&report, "483", TrackStage::Forecast), kb["483"]);
    assert_eq!(value(&report, "483", TrackStage::Calculation), kb["483"]);
    assert_eq!(value(&report, "481", TrackStage::Calculation), kb["481"]);
    let uncovered: Money = kb
        .iter()
        .filter(|(code, _)| !["481", "484"].contains(&code.as_str()))
        .map(|(_, amount)| *amount)
        .sum();
    assert_eq!(
        report.total.value(TrackStage::Forecast),
        Some(eur(53_000) + eur(21_560) + eur(9_702) + uncovered)
    );
    // The GA forecast equals the change-order overview's.
    let overview = crate::change_order::change_order_overview(
        fixture.project.contract_of(1),
        fixture.project.change_orders_of(1),
    );
    assert_eq!(overview.forecast, eur(21_560) + eur(9_702));
    // Kostenschätzung 480 is shown but not used for the forecast (LV and
    // Kostenberechnung are more precise).
    assert!(row(&report, "480").value(TrackStage::Estimate).is_some());
    assert!(report
        .sources
        .iter()
        .any(|source| source.starts_with("Kostenberechnung")));
    assert!(report
        .assumptions
        .iter()
        .any(|text| text.contains("offene Nachträge in der Prognose: N02")));
    assert!(report
        .assumptions
        .iter()
        .any(|text| text.contains("aus der Kostenberechnung")));
}

#[test]
fn the_chosen_bid_counts_with_its_reason_until_the_award() {
    let mut fixture = fixture();
    assert!(fixture.project.set_offer_choice_of(
        0,
        Some(OfferChoice {
            bid: fixture.dear,
            reason: "wirtschaftlichstes Angebot nach Wertung".into(),
        })
    ));
    let report = cost_tracking(&fixture.project, &level(2));
    assert_eq!(value(&report, "420", TrackStage::Offer), eur(57_000));
    assert_eq!(value(&report, "420", TrackStage::Forecast), eur(57_000));
    assert_eq!(
        report.lvs[0].offer.as_deref(),
        Some("Wärme Süd (gewählt: wirtschaftlichstes Angebot nach Wertung)")
    );
    // Excluded: back to the lowest bid.
    fixture.project.set_active_lv(0);
    fixture
        .project
        .bid_mut(fixture.dear)
        .unwrap()
        .set_excluded(true);
    let report = cost_tracking(&fixture.project, &level(2));
    assert_eq!(value(&report, "420", TrackStage::Offer), eur(53_000));
    assert_eq!(
        report.lvs[0].offer.as_deref(),
        Some("Wärme Nord (Mindestbieter)")
    );
    // A bid of another LV cannot be chosen.
    assert!(!fixture.project.set_offer_choice_of(
        0,
        Some(OfferChoice {
            bid: BidId(99),
            reason: String::new(),
        })
    ));
    let _ = fixture.cheap;
}

#[test]
fn the_discount_falls_on_every_cost_group_exactly() {
    let mut fixture = fixture();
    // An odd discount: 3,333 %.
    fixture.project.set_active_lv(1);
    let contract = fixture.project.contract_mut().unwrap();
    contract.discount_percent = dec("3.333");
    contract.total = Money::from_cents(2_706_676);
    let report = cost_tracking(
        &fixture.project,
        &TrackOptions {
            view: TrackView::ByLv,
            ..level(3)
        },
    );
    let lv = report.rows.iter().find(|row| row.lv == Some(1)).unwrap();
    // 27.066,76 + N01 3.000 − 3,333 % (99,99) = 29.966,77.
    assert_eq!(
        lv.value(TrackStage::Contract),
        Some(Money::from_cents(2_996_677))
    );
    let members: Money = report
        .rows
        .iter()
        .filter(|row| row.lv == Some(1) && row.kind != TrackRowKind::Lv)
        .filter_map(|row| row.value(TrackStage::Contract))
        .sum();
    assert_eq!(Some(members), lv.value(TrackStage::Contract));
}

#[test]
fn billing_counts_per_cost_group_and_more_than_ordered_goes_into_the_forecast() {
    let mut fixture = fixture();
    // 1. Abschlag: DDC 5 St (9.500), Kabel 1.050 m (9.450) → after 2 %:
    // 9.310 + 9.261 = 18.571. Kabel lies within contract + open N02.
    add_invoice(
        &mut fixture.project,
        InvoiceKind::Partial,
        "AR-1",
        &[(fixture.ddc, "5"), (fixture.cable, "1050")],
    );
    let report = cost_tracking(&fixture.project, &level(3));
    assert_eq!(value(&report, "481", TrackStage::Billed), eur(9_310));
    assert_eq!(value(&report, "484", TrackStage::Billed), eur(9_261));
    assert_eq!(value(&report, "484", TrackStage::Forecast), eur(9_702));
    assert!(report.released_net.cents() > 0);
    // 2. Abschlag: Kabel 1.150 m → 10.350 − 2 % = 10.143, 441 above
    // contract (8.820) + open N02 (882).
    add_invoice(
        &mut fixture.project,
        InvoiceKind::Partial,
        "AR-2",
        &[(fixture.ddc, "5"), (fixture.cable, "1150")],
    );
    let report = cost_tracking(&fixture.project, &level(3));
    assert_eq!(value(&report, "484", TrackStage::Billed), eur(10_143));
    assert_eq!(
        value(&report, "484", TrackStage::Forecast),
        eur(9_702) + eur(441)
    );
    assert!(report.lvs[1].forecast_basis.contains("Mehrmengen"));
    assert_eq!(row(&report, "484").value(TrackStage::Final), None);
    assert_eq!(report.final_lvs, 0);
}

#[test]
fn the_final_invoice_is_the_cost_statement() {
    let mut fixture = fixture();
    // Schlussrechnung: DDC 10, N01 2, Kabel 980 m → 19.000 + 3.000 + 8.820 =
    // 30.820, after 2 % 30.203,60.
    add_invoice(
        &mut fixture.project,
        InvoiceKind::Final,
        "SR",
        &[
            (fixture.ddc, "10"),
            (fixture.extra, "2"),
            (fixture.cable, "980"),
        ],
    );
    let report = cost_tracking(&fixture.project, &level(2));
    let ga = row(&report, "480");
    assert_eq!(
        ga.value(TrackStage::Final),
        Some(Money::from_cents(3_020_360))
    );
    assert_eq!(ga.value(TrackStage::Billed), ga.value(TrackStage::Final));
    // The forecast of the GA is the final invoice (plus the KG of no LV).
    let kb = calculation(&fixture.project);
    let uncovered: Money = kb
        .iter()
        .filter(|(code, _)| !["481", "484"].contains(&code.as_str()))
        .map(|(_, amount)| *amount)
        .sum();
    assert_eq!(
        ga.value(TrackStage::Forecast),
        Some(Money::from_cents(3_020_360) + uncovered)
    );
    assert_eq!(report.final_lvs, 1);
    assert_eq!(
        report.final_text(),
        "1 von 1 beauftragten LV schlussgerechnet"
    );
    assert_eq!(report.lvs[1].forecast_basis, "Kostenfeststellung");
}

#[test]
fn deviations_to_budget_and_previous_stage_mark_the_cells() {
    let mut fixture = fixture();
    let tracking = fixture.project.cost_tracking_mut();
    tracking.set_budget("481", eur(10_000));
    tracking.set_budget("484", eur(9_000));
    let report = cost_tracking(&fixture.project, &level(3));
    let ddc = row(&report, "481");
    // Auftrag 21.560 against 10.000: +11.560 € = +115,6 %.
    let deviation = ddc.to_budget(TrackStage::Contract).unwrap();
    assert_eq!(deviation.amount, eur(11_560));
    assert_eq!(deviation.percent, Some(dec("115.6")));
    assert_eq!(report.mark(ddc, TrackStage::Contract), Mark::OverBudget);
    // Kabel: Prognose 9.702 above the budget 9.000; Auftrag 8.820 below it
    // and equal to the bid (no mark).
    let cable = row(&report, "484");
    assert_eq!(report.mark(cable, TrackStage::Forecast), Mark::OverBudget);
    assert_eq!(report.mark(cable, TrackStage::Contract), Mark::None);
    let previous = cable.to_previous(TrackStage::Forecast).unwrap();
    assert_eq!(previous.stage, Some(TrackStage::Contract));
    assert_eq!(previous.deviation.percent, Some(dec("10.0")));
    // Above the threshold to the previous stage (budget not exceeded).
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("484", eur(20_000));
    fixture.project.cost_tracking_mut().threshold_percent = dec("5");
    let report = cost_tracking(&fixture.project, &level(3));
    assert_eq!(
        report.mark(row(&report, "484"), TrackStage::Forecast),
        Mark::AbovePrevious
    );
}

#[test]
fn a_group_budget_sums_its_members_unless_it_has_its_own() {
    let mut tracking = CostTracking::default();
    tracking.set_budget("481", eur(10_000));
    tracking.set_budget("484", eur(5_000));
    tracking.set_budget("421", eur(40_000));
    assert_eq!(tracking.budget_of("480"), Some(eur(15_000)));
    assert_eq!(tracking.budget_of("400"), Some(eur(55_000)));
    assert_eq!(tracking.budget_total(), Some(eur(55_000)));
    tracking.set_budget("480", eur(12_000));
    assert_eq!(tracking.budget_of("480"), Some(eur(12_000)));
    assert_eq!(tracking.budget_of("400"), Some(eur(52_000)));
    assert_eq!(tracking.budget_conflicts().len(), 1);
    assert!(tracking.budget_conflicts()[0].contains("KG 480"));
    // Zero removes.
    tracking.set_budget("480", Money::ZERO);
    assert_eq!(tracking.budget_of("480"), Some(eur(15_000)));
    // Taking over a Kostenberechnung replaces everything.
    let sums = BTreeMap::from([("481".to_string(), eur(1)), ("482".to_string(), eur(2))]);
    tracking.take_budget(&sums, "Kostenberechnung LPH 3", "18.11.2026");
    assert_eq!(tracking.budget.len(), 2);
    assert_eq!(tracking.budget_total(), Some(eur(3)));
    assert_eq!(tracking.budget_note, "aus Kostenberechnung LPH 3");
    assert_eq!(tracking.budget_date, "18.11.2026");
}

#[test]
fn the_budget_shows_on_its_level_and_in_the_total() {
    let mut fixture = fixture();
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("480", eur(100_000));
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("421", eur(48_000));
    let report = cost_tracking(&fixture.project, &level(3));
    assert_eq!(
        row(&report, "480").value(TrackStage::Budget),
        Some(eur(100_000))
    );
    assert_eq!(row(&report, "481").value(TrackStage::Budget), None);
    assert_eq!(
        row(&report, "421").value(TrackStage::Budget),
        Some(eur(48_000))
    );
    assert_eq!(
        row(&report, "420").value(TrackStage::Budget),
        Some(eur(48_000))
    );
    assert_eq!(report.total.value(TrackStage::Budget), Some(eur(148_000)));
    // A budget alone makes its row appear.
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("540", eur(5_000));
    let report = cost_tracking(&fixture.project, &level(2));
    assert_eq!(
        row(&report, "540").value(TrackStage::Budget),
        Some(eur(5_000))
    );
    assert_eq!(
        row(&report, "500").value(TrackStage::Budget),
        Some(eur(5_000))
    );
}

#[test]
fn levels_and_the_view_by_vergabeeinheit() {
    let fixture = fixture();
    let one = cost_tracking(&fixture.project, &level(1));
    assert_eq!(codes(&one), ["400 Bauwerk – Technische Anlagen"]);
    assert_eq!(one.rows[0].kind, TrackRowKind::Leaf);
    let two = cost_tracking(&fixture.project, &level(2));
    let titles = codes(&two);
    assert_eq!(titles[0], "400 Bauwerk – Technische Anlagen");
    assert!(titles.contains(&"420 Wärmeversorgungsanlagen".to_string()));
    assert!(titles.contains(&"480 Gebäude- und Anlagenautomation".to_string()));
    // All levels give the same total.
    let three = cost_tracking(&fixture.project, &level(3));
    assert_eq!(one.total, two.total);
    assert_eq!(two.total.values, three.total.values);

    let by_lv = cost_tracking(
        &fixture.project,
        &TrackOptions {
            view: TrackView::ByLv,
            ..level(3)
        },
    );
    let kinds: Vec<(TrackRowKind, String)> = by_lv
        .rows
        .iter()
        .map(|row| (row.kind, row.title()))
        .collect();
    assert_eq!(kinds[0], (TrackRowKind::Lv, "LV 1 Heizung".to_string()));
    assert_eq!(kinds[1].1, "421 Wärmeerzeugungsanlagen");
    let ga = kinds
        .iter()
        .position(|(_, title)| title == "LV 2 Gebäudeautomation")
        .unwrap();
    assert!(kinds[ga + 1].1.starts_with("481"));
    let rest = by_lv
        .rows
        .iter()
        .find(|row| row.kind == TrackRowKind::NoLv)
        .unwrap();
    assert!(rest.value(TrackStage::Calculation).is_some());
    assert_eq!(rest.value(TrackStage::PreTender), None);
    assert_eq!(by_lv.total.values, three.total.values);
}

#[test]
fn gross_uses_the_rate_of_each_lv() {
    let mut fixture = fixture();
    fixture
        .project
        .lv_at_mut(0)
        .unwrap()
        .set_vat_percent(Decimal::from(7));
    let gross = cost_tracking(
        &fixture.project,
        &TrackOptions {
            gross: true,
            ..level(2)
        },
    );
    // 53.000 × 1,07; the GA with its 19 %.
    assert_eq!(value(&gross, "420", TrackStage::Offer), eur(56_710));
    assert_eq!(
        value(&gross, "480", TrackStage::Offer),
        Money::from_cents(2_744_000 * 119 / 100)
    );
    // Mixed rates are named.
    assert_eq!(
        gross.amounts_text(),
        "brutto (USt je LV und Kostenermittlung: 7 % und 19 %)"
    );
    assert!(gross
        .vat_details()
        .contains(&"LV 1 Heizung: 7 %".to_string()));
}

#[test]
fn gross_budget_and_estimates_take_the_rate_of_their_cost_group() {
    let mut fixture = fixture();
    fixture
        .project
        .lv_at_mut(0)
        .unwrap()
        .set_vat_percent(Decimal::from(7));
    let tracking = fixture.project.cost_tracking_mut();
    tracking.set_budget("420", eur(50_000));
    tracking.set_budget("484", eur(9_000));
    let report = cost_tracking(
        &fixture.project,
        &TrackOptions {
            gross: true,
            ..level(3)
        },
    );
    // Heizung: the KG of a 7 % LV; the cables of the GA with 19 %.
    assert_eq!(value(&report, "420", TrackStage::Budget), eur(53_500));
    assert_eq!(value(&report, "484", TrackStage::Budget), eur(10_710));
    assert_eq!(
        report.total.value(TrackStage::Budget),
        Some(eur(53_500) + eur(10_710))
    );
    // The Datenpunktrechnungen (no rate of their own) with the usual rate
    // of the project: one LV each at 7 % and 19 % – 19 % among equals.
    let net = cost_tracking(&fixture.project, &level(3));
    assert_eq!(value(&report, "481", TrackStage::Calculation), {
        let net = value(&net, "481", TrackStage::Calculation);
        net + net.percent(Decimal::from(19))
    });
    // One rate: just that.
    fixture
        .project
        .lv_at_mut(0)
        .unwrap()
        .set_vat_percent(Decimal::from(19));
    let report = cost_tracking(
        &fixture.project,
        &TrackOptions {
            gross: true,
            ..level(3)
        },
    );
    assert_eq!(report.amounts_text(), "brutto (USt 19 %)");
    assert!(report.vat_details().is_empty());
}

#[test]
fn cost_lvs_add_up_and_a_doubled_group_is_reported() {
    let mut fixture = fixture();
    // Kosten-LV Gebäudeautomation (Kostenberechnung) with KG 481 – the
    // Datenpunktrechnung has 481 too.
    let mut cost = Lv::new(
        "Kostenberechnung GA",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    cost.set_purpose(LvPurpose::Cost(CostStage::Calculation));
    group(&mut cost, "481", "Automationseinrichtungen");
    position(&mut cost, "DDC je Datenpunkt", "100", "St", Some("150"));
    // And Heizung as a Kosten-LV: adds up with the GA.
    let mut heat = Lv::new(
        "Kostenberechnung Heizung",
        OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
    );
    heat.set_purpose(LvPurpose::Cost(CostStage::Calculation));
    group(&mut heat, "421", "Wärmeerzeugung");
    position(&mut heat, "Wärmepumpe", "40", "kW", Some("1250"));
    fixture.project.add_lv(cost);
    fixture.project.add_lv(heat);
    let report = cost_tracking(&fixture.project, &level(3));
    let kb = calculation(&fixture.project);
    assert_eq!(
        value(&report, "481", TrackStage::Calculation),
        kb["481"] + eur(15_000)
    );
    assert_eq!(value(&report, "421", TrackStage::Calculation), eur(50_000));
    let doubled: Vec<&String> = report
        .warnings
        .iter()
        .filter(|warning| warning.contains("doppelt"))
        .collect();
    assert_eq!(doubled.len(), 1, "{:?}", report.warnings);
    assert!(doubled[0].contains("KG 481"), "{}", doubled[0]);
    // Kosten-LVs are no Vergabeeinheit: not in the Kostenvoranschlag.
    assert_eq!(value(&report, "421", TrackStage::PreTender), eur(50_000));
    assert_eq!(report.lvs.len(), 2);

    // Deselect the Datenpunktrechnung: no warning, 481 from the Kosten-LV.
    let parts = stage_parts(&fixture.project);
    let datapoints = parts
        .iter()
        .find(|part| part.method == "nach Datenpunkten" && part.stage == CostStage::Calculation)
        .unwrap();
    assert!(datapoints.by_default && datapoints.counts);
    let key = datapoints.key.clone();
    fixture
        .project
        .cost_tracking_mut()
        .source_choice
        .insert(key, false);
    let report = cost_tracking(&fixture.project, &level(3));
    assert!(!report
        .warnings
        .iter()
        .any(|warning| warning.contains("doppelt")));
    assert_eq!(value(&report, "481", TrackStage::Calculation), eur(15_000));
    assert!(!report
        .rows
        .iter()
        .any(|row| row.code.as_deref() == Some("483")));
}

#[test]
fn the_newest_data_point_estimate_counts_unless_another_is_chosen() {
    let mut fixture = fixture();
    let mut newer = demo_cost_estimate(CostStage::Calculation);
    newer.date = "01.01.2030".into();
    newer.rate = Decimal::from(1);
    for kind in &mut newer.kinds {
        kind.unit_price = Decimal::from(1);
    }
    let newer_total = newer.result().total;
    let newer = fixture.project.add_cost_estimate(newer);
    let report = cost_tracking(&fixture.project, &level(1));
    assert_eq!(
        report.total.value(TrackStage::Calculation),
        Some(newer_total)
    );
    let parts = stage_parts(&fixture.project);
    let older = parts
        .iter()
        .find(|part| part.stage == CostStage::Calculation && !part.counts)
        .unwrap();
    assert!(!older.by_default);
    // Choose the older one and drop the newer.
    let older_key = older.key.clone();
    let tracking = fixture.project.cost_tracking_mut();
    tracking.source_choice.insert(older_key, true);
    tracking
        .source_choice
        .insert(format!("datenpunkte:{}", newer.0), false);
    let report = cost_tracking(&fixture.project, &level(1));
    let kb: Money = calculation(&fixture.project).values().copied().sum();
    assert_eq!(report.total.value(TrackStage::Calculation), Some(kb));
}

#[test]
fn a_recorded_state_is_compared_in_every_stage() {
    let mut fixture = fixture();
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("484", eur(9_000));
    let snapshot = record_snapshot(&fixture.project, "18.11.2026", "vor Nachtrag N02");
    fixture.project.cost_tracking_mut().snapshots.push(snapshot);
    // Later N02 is rejected (882 less in the forecast) and the budget of the
    // cables raised.
    fixture.project.set_active_lv(1);
    let id = fixture.project.change_orders()[1].id;
    fixture.project.change_order_mut(id).unwrap().status = ChangeOrderStatus::Rejected;
    fixture
        .project
        .cost_tracking_mut()
        .set_budget("484", eur(10_000));
    let report = cost_tracking(
        &fixture.project,
        &TrackOptions {
            snapshot: Some(0),
            deviation: TrackDeviation::Snapshot,
            ..level(3)
        },
    );
    assert_eq!(
        report.snapshot.as_deref(),
        Some("18.11.2026 – vor Nachtrag N02")
    );
    let cable = row(&report, "484");
    assert_eq!(cable.compared(TrackStage::Forecast), Some(eur(9_702)));
    assert_eq!(
        cable.to_snapshot(TrackStage::Forecast).unwrap().amount,
        -eur(882)
    );
    assert_eq!(
        TrackDeviation::Snapshot
            .of(cable, TrackStage::Forecast)
            .unwrap()
            .amount,
        -eur(882)
    );
    // The budget as it was, the other stages unchanged.
    assert_eq!(cable.compared(TrackStage::Budget), Some(eur(9_000)));
    assert_eq!(
        cable.to_snapshot(TrackStage::Budget).unwrap().amount,
        eur(1_000)
    );
    assert_eq!(
        cable.compared(TrackStage::Contract),
        cable.value(TrackStage::Contract)
    );
    assert_eq!(
        cable.to_snapshot(TrackStage::Contract).unwrap().amount,
        Money::ZERO
    );
    assert_eq!(
        cable.compared(TrackStage::Calculation),
        cable.value(TrackStage::Calculation)
    );
    assert_eq!(cable.to_snapshot(TrackStage::Final), None);
    let total = &report.total;
    assert_eq!(
        total.to_snapshot(TrackStage::Forecast).unwrap().amount,
        -eur(882)
    );
    assert_eq!(
        total.to_snapshot(TrackStage::Budget).unwrap().amount,
        eur(1_000)
    );
    // By LV too.
    let by_lv = cost_tracking(
        &fixture.project,
        &TrackOptions {
            view: TrackView::ByLv,
            snapshot: Some(0),
            ..level(3)
        },
    );
    let ga = by_lv
        .rows
        .iter()
        .find(|row| row.lv == Some(1) && row.kind == TrackRowKind::Lv)
        .unwrap();
    assert_eq!(
        ga.to_snapshot(TrackStage::Forecast).unwrap().amount,
        -eur(882)
    );
    // Budgets are not per LV.
    assert_eq!(ga.compared(TrackStage::Budget), None);
}

#[test]
fn positions_without_cost_group_are_listed() {
    let mut fixture = fixture();
    let lv = fixture.project.lv_at_mut(0).unwrap();
    lv.push_group(LvGroup::new(0, "", "Sonstiges"));
    lv.push_position(Position::new(
        "",
        "Inbetriebnahme",
        dec("1"),
        "psch",
        Some(dec("2500")),
    ));
    lv.renumber();
    let report = cost_tracking(&fixture.project, &level(2));
    let without = report
        .rows
        .iter()
        .find(|row| row.kind == TrackRowKind::Without)
        .unwrap();
    assert_eq!(without.value(TrackStage::PreTender), Some(eur(2_500)));
    assert_eq!(report.without_group.len(), 1);
    assert_eq!(report.without_group[0].short_text, "Inbetriebnahme");
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning == "LV 1 Heizung: 1 Position ohne Kostengruppe."));
}

#[test]
fn without_lvs_the_forecast_comes_from_the_estimate() {
    let mut project = Project::new(
        ProjectName::parse("Neubau Schule").unwrap(),
        "2026-11-18T12:00:00",
        Lv::new("leer", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
    );
    let estimate = demo_cost_estimate(CostStage::Estimate);
    let total = estimate.result().total;
    project.add_cost_estimate(estimate);
    let report = cost_tracking(&project, &level(3));
    assert_eq!(value(&report, "480", TrackStage::Forecast), total);
    assert_eq!(value(&report, "480", TrackStage::Estimate), total);
    assert!(report
        .assumptions
        .iter()
        .any(|text| text == "KG 480: noch nicht vergeben – aus der Kostenschätzung"));
    // Nothing tendered, nothing awarded.
    assert!(!report.used[TrackStage::Contract.index()]);
    assert_eq!(
        report.final_text(),
        "0 von 0 beauftragten LVs schlussgerechnet"
    );
}

#[test]
fn allocation_is_exact_and_keeps_the_order() {
    let parts = vec![
        (Some("481".to_string()), Money::from_cents(100)),
        (Some("484".to_string()), Money::from_cents(200)),
        (None, Money::from_cents(0)),
    ];
    let shared = allocate(&parts, Money::from_cents(1000));
    assert_eq!(
        shared.iter().map(|(_, amount)| *amount).sum::<Money>(),
        Money::from_cents(1000)
    );
    assert_eq!(shared[0].1, Money::from_cents(333));
    assert_eq!(shared[1].1, Money::from_cents(667));
    assert_eq!(
        allocate(&[], Money::from_cents(5)),
        vec![(None, Money::from_cents(5))]
    );
    assert_eq!(code_level("400"), 1);
    assert_eq!(code_level("480"), 2);
    assert_eq!(code_level("481"), 3);
    assert_eq!(at_level("481", 2).as_deref(), Some("480"));
    assert_eq!(at_level("480", 3).as_deref(), Some("480"));
    assert_eq!(at_level("4x", 1), None);
    assert_eq!(group_label("700"), "Baunebenkosten");
}

#[test]
fn the_previous_stage_is_compared_per_vergabeeinheit() {
    let fixture = fixture();
    let by_lv = cost_tracking(
        &fixture.project,
        &TrackOptions {
            view: TrackView::ByLv,
            ..level(3)
        },
    );
    let lv_row = |title: &str| {
        by_lv
            .rows
            .iter()
            .find(|row| row.kind == TrackRowKind::Lv && row.title() == title)
            .unwrap()
    };
    let ga = lv_row("LV 2 Gebäudeautomation");
    let heating = lv_row("LV 1 Heizung");
    let report = cost_tracking(&fixture.project, &level(3));
    let total = &report.total;
    // Auftrag only for the GA: against the Angebot of the GA alone, not
    // against the Angebote of both LVs.
    let contract = total.to_previous(TrackStage::Contract).unwrap();
    assert_eq!(contract.stage, Some(TrackStage::Offer));
    assert_eq!(
        contract.deviation.amount,
        ga.value(TrackStage::Contract).unwrap() - ga.value(TrackStage::Offer).unwrap()
    );
    // Prognose: GA against its Auftrag, Heizung against its Angebot, the
    // KG without LV against the Kostenberechnung – mixed stages.
    let forecast = total.to_previous(TrackStage::Forecast).unwrap();
    assert_eq!(forecast.stage, None);
    assert_eq!(forecast.label(), "zur Vorstufe je LV");
    let expected = (ga.value(TrackStage::Forecast).unwrap()
        - ga.value(TrackStage::Contract).unwrap())
        + (heating.value(TrackStage::Forecast).unwrap()
            - heating.value(TrackStage::Offer).unwrap());
    assert_eq!(forecast.deviation.amount, expected);
    // The Kostenvoranschlag has no LV-wise base: against the
    // Kostenberechnung of the whole row.
    let pre = total.to_previous(TrackStage::PreTender).unwrap();
    assert_eq!(pre.stage, Some(TrackStage::Calculation));
    assert_eq!(pre.label(), "zur Kostenberechnung");
    assert_eq!(
        pre.deviation.amount,
        total.value(TrackStage::PreTender).unwrap() - total.value(TrackStage::Calculation).unwrap()
    );
}

/// DIN 276, 4.6: the first column is the Kostenvorgabe (Budget of the client);
/// the key „budget“ stays, „kostenvorgabe“ reads too.
#[test]
fn the_budget_is_called_kostenvorgabe() {
    assert_eq!(TrackStage::Budget.label(), "Kostenvorgabe");
    assert!(TrackStage::Budget
        .explanation()
        .starts_with("Budget bzw. Kostenvorgabe des Bauherrn (DIN 276, 4.6)"));
    assert_eq!(TrackDeviation::Budget.label(), "zur Kostenvorgabe");
    for key in ["budget", "Kostenvorgabe"] {
        assert_eq!(
            TrackDeviation::from_key(key),
            Some(TrackDeviation::Budget),
            "{key}"
        );
    }
}
