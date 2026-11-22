use rust_decimal::Decimal;

use super::*;
use crate::{Address, Bid, LvGroup, OzMask, Position, Project, ProjectName};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// Title 01 with A (1 psch), B (120 St), C (140 St); title 02 with D, E (1 psch).
fn lv() -> Lv {
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Bauliches"));
    lv.push_position(Position::new(
        "01.0010",
        "Kernbohrungen einmessen",
        dec("1"),
        "psch",
        None,
    ));
    lv.push_position(Position::new(
        "01.0020",
        "Bohrlöcher Beton",
        dec("120"),
        "St",
        None,
    ));
    lv.push_position(Position::new(
        "01.0030",
        "Bohrungen Mauerwerk",
        dec("140"),
        "St",
        None,
    ));
    lv.push_group(LvGroup::new(0, "02", "Dienste"));
    lv.push_position(Position::new(
        "02.0010",
        "Plattform",
        dec("1"),
        "psch",
        None,
    ));
    lv.push_position(Position::new(
        "02.0020",
        "Assistent",
        dec("1"),
        "psch",
        None,
    ));
    lv
}

fn item(ordinal: &str, quantity: Option<&str>, price: &str, total: Option<&str>) -> BidItem {
    BidItem {
        ordinal: ordinal.into(),
        quantity: quantity.map(dec),
        unit_price: Some(dec(price)),
        total: total.map(dec),
        ..BidItem::default()
    }
}

fn row(lv: &Lv, ordinal: &str) -> RowId {
    lv.position_by_ordinal(ordinal).unwrap().id()
}

#[test]
fn quantity_is_written_or_follows_from_gp_and_ep() {
    let written = item("1", Some("120"), "98.44", None);
    assert_eq!(written.quantity_fits(dec("120")), Some(true));
    assert_eq!(written.quantity_fits(dec("140")), Some(false));
    let derived = item("1", None, "2.49", Some("30895.92"));
    assert_eq!(derived.priced_quantity(), Some(dec("12408")));
    assert_eq!(derived.quantity_fits(dec("12408")), Some(true));
    assert_eq!(derived.quantity_fits(dec("1")), Some(false));
    assert_eq!(
        item("1", None, "0", Some("0")).quantity_fits(dec("1")),
        None
    );
}

#[test]
fn texts_and_units_are_compared_normalized() {
    assert_eq!(
        text_similarity("Bohrlöcher  Beton – 110 mm", "bohrlöcher beton - 110 mm"),
        1.0
    );
    assert!(
        text_similarity(
            "Automationseinrichtung ASP13",
            "Automationseinrichtung ASP14"
        ) < 0.6
    );
    assert!(same_unit("Stk.", "St") && same_unit("pschl", "psch") && !same_unit("m", "St"));
}

/// The numbering shifted by one in title 01 (the file has no 01.0030 but
/// an extra first position), title 02 has one level more in the file.
#[test]
fn shifted_numbers_and_an_extra_title_level_are_aligned_by_order_and_quantity() {
    let lv = lv();
    let items = vec![
        item("01.0005", Some("1"), "2608.66", None),
        item("01.0010", Some("120"), "98.44", None),
        item("01.0020", Some("140"), "63", None),
        item("02.10.0010", Some("1"), "136923.90", None),
        item("02.10.0020", Some("1"), "91458.34", None),
    ];
    let by_ordinal = vec![
        None,
        Some(row(&lv, "01.0010")),
        Some(row(&lv, "01.0020")),
        None,
        None,
    ];
    let proposed = align(&lv, &items, &by_ordinal, &[], false);
    // 01.0010 and 01.0020 contradict (quantity): conflicts, kept by OZ.
    assert_eq!(proposed.conflicts, vec![1, 2]);
    let pairs: Vec<(usize, String, bool)> = proposed
        .suggestions
        .iter()
        .map(|s| {
            (
                s.item,
                lv.position_by_id(s.row).unwrap().ordinal().to_string(),
                s.sure,
            )
        })
        .collect();
    assert_eq!(
        pairs,
        vec![
            (0, "01.0010".into(), true),
            (1, "01.0020".into(), true),
            (2, "01.0030".into(), true),
            (3, "02.0010".into(), true),
            (4, "02.0020".into(), true),
        ]
    );
    // Taken over: every item at its position, no conflict left.
    let taken = align(&lv, &items, &by_ordinal, &[], true);
    assert!(taken.conflicts.is_empty() && taken.suggestions.is_empty());
    let ordinals: Vec<String> = taken
        .rows
        .iter()
        .map(|row| {
            lv.position_by_id(row.unwrap())
                .unwrap()
                .ordinal()
                .to_string()
        })
        .collect();
    assert_eq!(
        ordinals,
        ["01.0010", "01.0020", "01.0030", "02.0010", "02.0020"]
    );
    assert_eq!(taken.by_structure.len(), 5);
}

#[test]
fn unequal_gaps_give_proposals_to_check_and_texts_decide() {
    let lv = lv();
    // Only two items for three free positions: no sure proposal by order.
    let mut first = item("09.1", Some("1"), "10", None);
    first.short_text = "Kernbohrungen einmessen".into();
    let mut second = item("09.2", Some("140"), "5", None);
    second.short_text = "Bohrungen in Mauerwerk".into();
    let items = vec![first, second];
    let proposed = align(&lv, &items, &[None, None], &[], false);
    let found: Vec<(String, bool)> = proposed
        .suggestions
        .iter()
        .map(|s| {
            (
                lv.position_by_id(s.row).unwrap().ordinal().to_string(),
                s.sure,
            )
        })
        .collect();
    assert_eq!(
        found[0],
        ("01.0010".into(), true),
        "gleicher Text, gleiche Menge"
    );
    assert_eq!(found[1].0, "01.0030");
    assert!(!found[1].1, "ähnlicher Text: prüfen");
}

#[test]
fn a_hand_assignment_wins() {
    let lv = lv();
    let items = vec![item("7.1", None, "5", None)];
    let assigned = align(&lv, &items, &[None], &[(0, row(&lv, "02.0020"))], true);
    assert_eq!(assigned.rows[0], Some(row(&lv, "02.0020")));
}

#[test]
fn the_report_assigns_items_later_and_explains_the_file_sum() {
    let lv = lv();
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let mut bid = Bid::new(Address::named("Nord"), "nord.x84");
    let a = row(project.lv(), "01.0010");
    bid.set_price(
        a,
        crate::BidPrice {
            unit_price: Some(dec("10")),
            stated_total: Some(dec("12")),
            not_offered: false,
        },
    );
    let b = row(project.lv(), "01.0020");
    bid.set_price(
        b,
        crate::BidPrice {
            unit_price: Some(dec("1")),
            stated_total: Some(dec("130")),
            not_offered: false,
        },
    );
    let c = row(project.lv(), "01.0030");
    let mut report = StructureReport {
        foreign: vec![
            item("09.1", Some("140"), "2", Some("280")),
            item("09.2", None, "50", Some("50")),
        ],
        quantities: vec![(b, dec("130"))],
        file_total: Some(Money::from_decimal(dec("472"))),
        ..StructureReport::default()
    };
    report.suggestions.push(StoredSuggestion {
        ordinal: "09.1".into(),
        row: c,
        similarity: 95,
        sure: true,
    });
    bid.set_structure(report);
    let id = project.add_bid(bid);
    let lv = project.lv().clone();
    let bid = project.bid_mut(id).unwrap();
    // Ranked: 1 × 10 + 120 × 1 = 130; file 472 = 130 + foreign 330 + 2 (A) + 10 (B: 130 − 120).
    let breakdown = bid.total_breakdown(&lv).unwrap();
    assert_eq!(breakdown.ranked, Money::from_decimal(dec("130")));
    assert_eq!(breakdown.foreign, Money::from_decimal(dec("330")));
    assert_eq!(breakdown.arithmetic, Money::from_decimal(dec("2")));
    assert_eq!(breakdown.quantities, Money::from_decimal(dec("10")));
    assert_eq!(breakdown.rest, Money::ZERO);
    // B's GP fits the bidder's quantity: no Rechenfehler there.
    let errors: Vec<String> = bid
        .arithmetic_errors(&lv)
        .iter()
        .map(|(p, _, _)| p.ordinal().to_string())
        .collect();
    assert_eq!(errors, ["01.0010"]);
    assert_eq!(
        bid.structure().summary(1).unwrap(),
        "Strukturabweichungen: 2 Positionen nicht im LV, 1 LV-Position fehlt, 1 Menge"
    );

    assert_eq!(bid.take_sure_suggestions(&lv), 1);
    assert_eq!(bid.unit_price(c), Some(dec("2")));
    assert_eq!(bid.structure().assigned.len(), 1);
    assert_eq!(bid.structure().foreign.len(), 1);
    assert!(
        bid.assign_item(&lv, "09.2", a, AssignedBy::Hand, 0)
            .is_err(),
        "A hat einen Preis ohne Widerspruch"
    );
    // Title 02 is not in the file.
    let absent: Vec<String> = bid
        .absent_positions(&lv)
        .iter()
        .map(|row| lv.position_by_id(*row).unwrap().ordinal().to_string())
        .collect();
    assert_eq!(absent, ["02.0010", "02.0020"]);
    let lines: Vec<String> = bid
        .structure()
        .lines(&lv, &[])
        .iter()
        .map(DeviationLine::text)
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("umnummeriert: Angebot 09.1 → LV 01.0030")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("nicht im LV: Angebot 09.2")),
        "{lines:?}"
    );
}
