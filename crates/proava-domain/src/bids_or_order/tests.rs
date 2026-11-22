use rust_decimal::Decimal;

use super::{position_ids, BidsOrOrder};
use crate::{
    Address, Bid, ChangeOrder, ChangeOrderId, ChangeOrderItem, ItemChange, Lv, LvGroup, LvRow,
    OzMask, Position, Project, ProjectName, RowId,
};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// Two LVs: LV 1 with A and B, both priced by two bids; LV 2 without bids.
fn project() -> (Project, RowId, RowId) {
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Titel"));
    let a = lv.push_position(Position::new("01.0010", "A", dec("2"), "St", None));
    let b = lv.push_position(Position::new("01.0020", "B", dec("1"), "St", None));
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    for name in ["X", "Y"] {
        let mut bid = Bid::new(Address::named(name), format!("{name}.x84"));
        bid.set_unit_price(a, Some(dec("10")));
        bid.set_unit_price(b, Some(dec("5")));
        project.add_bid(bid);
    }
    let mut second = Lv::new("LV 2", OzMask::parse("11.PPPP").unwrap());
    second.push_position(Position::new("01.0010", "C", dec("1"), "St", None));
    project.add_lv(second);
    (project, a, b)
}

fn new_position(project: &mut Project, ordinal: &str) -> RowId {
    let lv = project.lv_mut();
    let at = lv.rows().len();
    lv.insert_row(
        at,
        LvRow::Position(Position::new(ordinal, "Neu", dec("1"), "St", None)),
    )
}

#[test]
fn bids_and_order_are_counted_per_lv() {
    let (mut project, _, _) = project();
    assert_eq!(
        project.bids_or_order(0),
        BidsOrOrder {
            bids: 2,
            order: false
        }
    );
    assert!(project.lv_has_bids_or_order(0));
    assert!(!project.lv_has_bids_or_order(1));
    assert_eq!(project.bids_or_order(0).label(), "2 Angebote");
    let first = project.bids()[0].id();
    project.award_bid(first, "19.11.2026").unwrap();
    assert_eq!(
        project.bids_or_order(0).label(),
        "2 Angebote und einen Auftrag"
    );
    assert_eq!(
        BidsOrOrder {
            bids: 1,
            order: false
        }
        .label(),
        "ein Angebot"
    );
    assert_eq!(
        BidsOrOrder {
            bids: 0,
            order: true
        }
        .label(),
        "einen Auftrag"
    );
}

#[test]
fn new_positions_are_uncovered_unless_they_belong_to_a_change_order() {
    let (mut project, _, _) = project();
    let before = position_ids(project.lv());
    let new = new_position(&mut project, "01.0030");
    let uncovered = project.uncovered_positions(0, &before);
    assert_eq!(uncovered.added, vec![new]);
    assert!(uncovered.removed.is_empty());
    assert_eq!(uncovered.verb(), "einfügen");
    assert_eq!(
        project.uncovered_note(0, &uncovered).unwrap(),
        "Das LV hat bereits 2 Angebote – die neue Position fehlt dort."
    );
    // As a Nachtragsposition it is no question.
    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "Nachtrag");
    order.items.push(ChangeOrderItem {
        position: new,
        change: ItemChange::NewPosition,
        ..ChangeOrderItem::default()
    });
    project.add_change_order(order);
    assert!(project.uncovered_positions(0, &before).is_empty());
}

#[test]
fn removed_positions_count_when_a_bid_or_the_order_contains_them() {
    let (mut project, a, _) = project();
    let extra = new_position(&mut project, "01.0030");
    let before = position_ids(project.lv());
    // The extra position came after the bids: removing it is no question.
    let index = project.lv().index_of(extra).unwrap();
    project.lv_mut().remove_row(index);
    assert!(project.uncovered_positions(0, &before).is_empty());
    let index = project.lv().index_of(a).unwrap();
    project.lv_mut().remove_row(index);
    let uncovered = project.uncovered_positions(0, &before);
    assert_eq!(uncovered.removed, vec![a]);
    assert_eq!(uncovered.verb(), "löschen");
    assert_eq!(
        project.uncovered_note(0, &uncovered).unwrap(),
        "Das LV hat bereits 2 Angebote – die gelöschte Position ist dort enthalten."
    );
}

#[test]
fn with_an_order_the_note_points_to_a_change_order() {
    let (mut project, _, b) = project();
    let first = project.bids()[0].id();
    project.award_bid(first, "19.11.2026").unwrap();
    project.take_over_contract().unwrap();
    let before = position_ids(project.lv());
    new_position(&mut project, "01.0030");
    new_position(&mut project, "01.0040");
    let uncovered = project.uncovered_positions(0, &before);
    assert_eq!(
        project.uncovered_note(0, &uncovered).unwrap(),
        "Das LV hat bereits 2 Angebote und einen Auftrag – 2 neue Positionen fehlen dort. \
         Zusätzliche Leistungen gehören in einen Nachtrag."
    );
    let index = project.lv().index_of(b).unwrap();
    project.lv_mut().remove_row(index);
    let uncovered = project.uncovered_positions(0, &before);
    assert_eq!(uncovered.verb(), "ändern");
    assert!(project
        .uncovered_note(0, &uncovered)
        .unwrap()
        .ends_with("die gelöschte Position ist dort enthalten. Geänderte Leistungen gehören in einen Nachtrag."));
}

#[test]
fn an_lv_without_bids_and_order_never_asks() {
    let (mut project, _, _) = project();
    project.set_active_lv(1);
    let before = position_ids(project.lv());
    new_position(&mut project, "01.0020");
    let uncovered = project.uncovered_positions(1, &before);
    assert!(uncovered.is_empty());
    assert_eq!(project.uncovered_note(1, &uncovered), None);
}
