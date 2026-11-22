use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::*;
use crate::contract::ContractItem;
use crate::{Address, Bid, ChangeOrderStatus, LvGroup, OzMask, PositionKind, ProjectName};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// Project with a 2-level LV (11.22.PPPP), two contract positions and one
/// open bid; the contract is set by hand.
fn project() -> (Project, [RowId; 2]) {
    let mut lv = Lv::new("GA", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "Schaltschrank"));
    let a = lv.push_position(Position::new(
        "01.01.0010",
        "Kabel NYM 3x1,5",
        dec("100"),
        "m",
        Some(dec("2.40")),
    ));
    let b = lv.push_position(Position::new(
        "01.01.0020",
        "Raumfühler Temperatur",
        dec("10"),
        "St",
        None,
    ));
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "2026-11-17", lv);
    let item = |ordinal: &str, short: &str, quantity: &str, unit: &str, price: &str| ContractItem {
        ordinal: ordinal.into(),
        short_text: short.into(),
        unit: unit.into(),
        quantity: dec(quantity),
        unit_price: dec(price),
        kind: PositionKind::Normal,
        lump_sum: false,
        hourly: false,
    };
    project.set_contract(Some(Contract {
        contractor: "Firma A".into(),
        items: BTreeMap::from([
            (a, item("01.01.0010", "Kabel NYM 3x1,5", "100", "m", "2.50")),
            (
                b,
                item("01.01.0020", "Raumfühler Temperatur", "10", "St", "80"),
            ),
        ]),
        total: Money::from_cents(105_000),
        ..Contract::default()
    }));
    let mut bid = Bid::new(Address::named("Firma B"), "b.x84");
    bid.set_unit_price(a, Some(dec("3.00")));
    bid.set_unit_price(b, Some(dec("90")));
    project.add_bid(bid);
    let mut bid = Bid::new(Address::named("Firma A"), "a.x84");
    bid.set_unit_price(a, Some(dec("2.50")));
    bid.set_unit_price(b, Some(dec("80")));
    project.add_bid(bid);
    (project, [a, b])
}

fn ordinals(project: &Project) -> Vec<String> {
    project
        .lv()
        .rows()
        .iter()
        .map(|row| row.ordinal().to_string())
        .collect()
}

#[test]
fn area_at_the_end_keeps_contract_numbers() {
    let (mut project, [a, _]) = project();
    let first = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Wetterstation"));
    let proposal = project.change_order_proposal(first).unwrap();
    assert_eq!(proposal.title, "02.01 N01 Wetterstation");
    assert_eq!(proposal.ordinal, "02.01.0010");
    assert_eq!(project.lv().rows().len(), 4, "Vorschlag ändert nichts");

    let row = project
        .add_change_order_position(
            first,
            Position::new("", "Wetterstation", dec("1"), "St", None),
            Some(dec("1947")),
        )
        .unwrap();
    assert_eq!(
        project.lv().position_by_id(row).unwrap().ordinal(),
        "02.01.0010"
    );
    let second = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Mehr Kabel"));
    project
        .add_change_order_position(
            second,
            Position::new("", "Kabel J-Y(St)Y", dec("50"), "m", None),
            None,
        )
        .unwrap();
    project
        .add_change_order_position(
            first,
            Position::new("", "Mast", dec("1"), "St", None),
            Some(dec("300")),
        )
        .unwrap();
    assert_eq!(
        ordinals(&project),
        [
            "01",
            "01.01",
            "01.01.0010",
            "01.01.0020",
            "02",
            "02.01",
            "02.01.0010",
            "02.01.0020",
            "02.02",
            "02.02.0010"
        ]
    );
    assert_eq!(project.lv().rows()[4].short_text(), COLLECTOR_TITLE);
    assert_eq!(project.lv().rows()[8].short_text(), "N02 Mehr Kabel");
    assert_eq!(
        project.lv().position_by_id(a).unwrap().ordinal(),
        "01.01.0010"
    );
    let order = project.change_order(first).unwrap();
    assert_eq!(order.items.len(), 2);
    assert_eq!(order.items[0].offered_quantity, dec("1"));
    assert_eq!(
        order.offered_total(project.contract()),
        Money::from_cents(224_700)
    );

    // Löschen mit Positionen räumt den Bereich auf, der Sammeltitel bleibt.
    project.delete_change_order(first, true).unwrap();
    assert_eq!(
        ordinals(&project),
        [
            "01",
            "01.01",
            "01.01.0010",
            "01.01.0020",
            "02",
            "02.02",
            "02.02.0010"
        ]
    );
}

#[test]
fn one_title_level_and_none() {
    let mut lv = Lv::new("GA", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_position(Position::new("01.0010", "A", dec("1"), "St", None));
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Zusatz"));
    project
        .add_change_order_position(id, Position::new("", "B", dec("1"), "St", None), None)
        .unwrap();
    assert_eq!(ordinals(&project), ["01", "01.0010", "02", "02.0010"]);
    assert_eq!(project.lv().rows()[2].short_text(), "N01 Zusatz");

    let mut lv = Lv::new("GA", OzMask::parse("PPPP").unwrap());
    lv.push_position(Position::new("0010", "A", dec("1"), "St", None));
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Zusatz"));
    assert_eq!(
        project.change_order_proposal(id).unwrap().title,
        "Ende des LV"
    );
    project
        .add_change_order_position(id, Position::new("", "B", dec("1"), "St", None), None)
        .unwrap();
    assert_eq!(ordinals(&project), ["0010", "0020"]);
}

#[test]
fn assign_positions_and_titles() {
    let (mut project, [a, _]) = project();
    // Positionen im LV-Editor geschrieben, dann dem Nachtrag zugeordnet.
    let lv = project.lv_mut();
    let title = lv.push_group(LvGroup::new(1, "01.02", "Nachrüstung"));
    let x = lv.push_position(Position::new("01.02.0010", "Taster", dec("4"), "St", None));
    lv.push_position(Position::new("01.02.0020", "Leuchte", dec("2"), "St", None));
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Nachrüstung"));
    assert_eq!(project.assign_to_change_order(id, title).unwrap(), 2);
    assert_eq!(
        project.change_order(id).unwrap().items[0].offered_quantity,
        dec("4")
    );
    assert!(
        project.assign_to_change_order(id, title).is_err(),
        "alles schon zugeordnet"
    );
    assert!(project
        .assign_to_change_order(id, x)
        .unwrap_err()
        .contains("gehört schon zu N01"));
    // Auftragsposition: Mengenänderung.
    assert_eq!(project.assign_to_change_order(id, a).unwrap(), 1);
    let item = project.change_order(id).unwrap().item(a).unwrap().clone();
    assert_eq!(item.change, ItemChange::Quantity);
    assert!(project
        .assign_to_change_order(id, a)
        .unwrap_err()
        .contains("schon als Mengenänderung"));
    let other = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Andere"));
    assert_eq!(
        project.assign_to_change_order(other, a).unwrap(),
        1,
        "Mehrmenge in einem zweiten Nachtrag"
    );
    assert_eq!(
        project.change_order_of_position(x).unwrap().0.label(),
        "N01"
    );
    project.remove_change_order_item(id, x, true).unwrap();
    assert!(project.lv().index_of(x).is_none(), "Position gelöscht");
    project.remove_change_order_item(id, a, true).unwrap();
    assert!(
        project.lv().index_of(a).is_some(),
        "Auftragsposition bleibt"
    );
}

#[test]
fn price_hints_from_contract_planner_and_bids() {
    let (mut project, [a, _]) = project();
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Kabel"));
    let mut position = Position::new("", "Kabel NYM 5x1,5", dec("80"), "m", Some(dec("3.20")));
    position.set_unit_price(Some(dec("3.20")));
    let row = project
        .add_change_order_position(id, position, Some(dec("4.00")))
        .unwrap();
    let item = project.change_order(id).unwrap().item(row).unwrap().clone();
    let hints = price_hints(&project, &item);
    let labels: Vec<&str> = hints.iter().map(|hint| hint.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "ähnlich: 01.01.0010 Kabel NYM 3x1,5",
            "Preisspiegel Mittel zu 01.01.0010",
            "Planerpreis"
        ]
    );
    // 4,00 gegen 2,50 = +60 %; Mittel 2,75 → +45,5 %; Planer 3,20 → +25 %.
    assert_eq!(hints[0].deviation, Some(dec("60.0")));
    assert_eq!(hints[1].price, dec("2.75"));
    assert_eq!(hints[1].deviation, Some(dec("45.5")));
    assert_eq!(hints[2].deviation, Some(dec("25.0")));

    let quantity = ChangeOrderItem {
        position: a,
        change: ItemChange::Quantity,
        offered_quantity: dec("20"),
        ..ChangeOrderItem::default()
    };
    let hints = price_hints(&project, &quantity);
    assert_eq!(hints[0].label, "EP Auftrag 01.01.0010");
    assert_eq!(hints[0].deviation, Some(dec("0.0")));
    assert!(hints
        .iter()
        .any(|hint| hint.label == "Preisspiegel günstigster" && hint.price == dec("2.50")));
    assert_eq!(deviation_percent(dec("1"), Decimal::ZERO), None);
}

#[test]
fn status_history_and_sums() {
    let (mut project, [a, _]) = project();
    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "Mehrmenge");
    order.announced_date = "02.03.2026".into();
    let id = project.add_change_order(order);
    project.assign_to_change_order(id, a).unwrap();
    let order = project.change_order_mut(id).unwrap();
    order.items[0].offered_quantity = dec("40");
    assert!(order.set_status(ChangeOrderStatus::Offered, "10.03.2026", "per Mail"));
    assert!(order.set_status(ChangeOrderStatus::Reviewed, "12.03.2026", ""));
    assert!(order.set_status(
        ChangeOrderStatus::Approved,
        "20.03.2026",
        "Schreiben Bauherr"
    ));
    assert!(!order.set_status(ChangeOrderStatus::Approved, "", ""));
    assert_eq!(order.history.len(), 4, "Anzeige + drei Wechsel");
    assert_eq!(order.history[0].status, ChangeOrderStatus::Announced);
    assert_eq!(order.offer_date, "10.03.2026");
    assert_eq!(order.checked_date, "12.03.2026");
    assert_eq!(order.decision_date, "20.03.2026");
    assert_eq!(order.status_date(), "20.03.2026");
    let mut rejected = ChangeOrder::new(ChangeOrderId(0), 0, "Abgelehnt");
    rejected.status = ChangeOrderStatus::Rejected;
    project.add_change_order(rejected);
    let sums = sums_by_status(project.contract(), project.change_orders());
    assert_eq!(sums.len(), 7);
    let approved = sums
        .iter()
        .find(|sum| sum.status == ChangeOrderStatus::Approved)
        .unwrap();
    assert_eq!(
        (approved.count, approved.approved),
        (1, Money::from_cents(10_000))
    );
    assert_eq!(
        sums.iter()
            .find(|sum| sum.status == ChangeOrderStatus::Rejected)
            .unwrap()
            .count,
        1
    );
    // Abrechenbar: 100 + 40 m.
    let billable = crate::billable_items(
        project.contract().unwrap(),
        project.change_orders(),
        project.lv(),
    );
    assert_eq!(billable[&a].quantity, dec("140"));
}

#[test]
fn offer_from_file_updates_known_and_writes_new_positions() {
    let (mut project, [a, _]) = project();
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "Lüftung"));
    let known = project
        .add_change_order_position(
            id,
            Position::new("", "Wetterstation", dec("1"), "St", None),
            None,
        )
        .unwrap();
    // Datei der Firma: die bekannte Position (gleiche OZ), eine Auftragsposition
    // und zwei neue Positionen mit fremder OZ, eine davon mit Langtext und Bild.
    let mut source = Project::new(
        ProjectName::parse("Datei").unwrap(),
        "",
        Lv::new("N", OzMask::parse("111.22.PPPP").unwrap()),
    );
    source.add_image("bild.png", vec![1, 2, 3]);
    let lv = source.lv_mut();
    lv.push_group(LvGroup::new(0, "481", "Automation"));
    lv.push_group(LvGroup::new(1, "481.01", "Hardware"));
    let mut p = Position::new(
        "02.01.0010",
        "Wetterstation",
        dec("1"),
        "St",
        Some(dec("1947")),
    );
    p.set_long_text(crate::RichText::plain("Station"));
    lv.push_position(p);
    lv.push_position(Position::new(
        "01.01.0010",
        "Kabel",
        dec("30"),
        "m",
        Some(dec("2.50")),
    ));
    let mut p = Position::new(
        "481.01.0001",
        "IO-Erweiterung",
        dec("1"),
        "St",
        Some(dec("600")),
    );
    p.set_long_text(crate::RichText::new(vec![
        crate::TextBlock::text("Erweiterung ASP02"),
        crate::TextBlock::Image {
            file: "bild.png".into(),
            width_mm: None,
        },
    ]));
    lv.push_position(p);
    lv.push_position(Position::new(
        "481.01.0002",
        "Inbetriebnahme",
        dec("8"),
        "h",
        Some(dec("95")),
    ));
    let offer = ChangeOrderOffer {
        source,
        number: Some(3),
        date: "11.03.2026".into(),
        firm: "Firma A GmbH".into(),
        file: "N01.X82".into(),
    };
    let result = project
        .take_change_order_offer(id, &offer, "17.11.2026")
        .unwrap();
    assert_eq!((result.updated, result.created), (2, 2));
    let warnings = result.warnings.join("\n");
    assert!(
        warnings.contains("Nachtrag N03, eingelesen in N01"),
        "{warnings}"
    );
    assert!(
        warnings.contains("01.01.0010 ist Auftragsposition"),
        "{warnings}"
    );
    assert!(warnings.contains("481.01.0001 → 02.01.0020"), "{warnings}");
    let order = project.change_order(id).unwrap();
    assert_eq!(order.status, ChangeOrderStatus::Offered);
    assert_eq!(order.offer_date, "11.03.2026");
    assert_eq!(order.items.len(), 4);
    assert_eq!(
        order.item(known).unwrap().offered_unit_price,
        Some(dec("1947"))
    );
    let quantity = order.item(a).unwrap();
    assert_eq!(
        (quantity.change, quantity.offered_unit_price),
        (ItemChange::Quantity, None)
    );
    let new = order.items[2].position;
    let position = project.lv().position_by_id(new).unwrap();
    assert_eq!(position.ordinal(), "02.01.0020");
    assert_eq!(
        position.unit_price(),
        None,
        "angebotener Preis steht im Nachtrag, nicht als Planerpreis"
    );
    assert_eq!(order.items[2].offered_unit_price, Some(dec("600")));
    assert_eq!(project.images().len(), 1, "Bild übernommen");
    // Zweite Fassung: dieselben OZ, neuer Preis; geprüfte Werte bleiben.
    project.change_order_mut(id).unwrap().items[0].checked_unit_price = Some(dec("1697"));
    let mut second = offer.clone();
    second.number = None;
    second
        .source
        .lv_mut()
        .row_mut(2)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_unit_price(Some(dec("1900")));
    let result = project
        .take_change_order_offer(id, &second, "17.11.2026")
        .unwrap();
    assert_eq!(
        (result.updated, result.created),
        (4, 0),
        "fremde OZ über Kurztext und Einheit wiedererkannt"
    );
    assert_eq!(project.change_order(id).unwrap().items.len(), 4);
    let item = project.change_order(id).unwrap().item(known).unwrap();
    assert_eq!(
        (item.offered_unit_price, item.checked_unit_price),
        (Some(dec("1900")), Some(dec("1697")))
    );
}

#[test]
fn position_marked_entfaellt_is_not_in_the_contract_and_comes_back_by_change_order() {
    // Seit master a6e46a6 lässt der Auftrag Positionen „entfällt“ weg.
    let mut lv = Lv::new("GA", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    let a = lv.push_position(Position::new("01.0010", "Fühler", dec("10"), "St", None));
    let mut gone = Position::new("01.0020", "Wetterstation", dec("1"), "St", None);
    gone.set_not_applicable(true);
    let b = lv.push_position(gone);
    let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let mut bid = Bid::new(Address::named("Firma"), "f.x84");
    bid.set_unit_price(a, Some(dec("80")));
    bid.set_unit_price(b, Some(dec("1500")));
    let bid = project.add_bid(bid);
    project.award_bid(bid, "01.10.2026").unwrap();
    project.take_over_contract().unwrap();
    assert!(project.contract().unwrap().item(b).is_none());
    // Als Nachtrag wird sie eine neue Position (keine Mengenänderung) …
    let id = project.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "doch Wetterstation"));
    project.assign_to_change_order(id, b).unwrap();
    let order = project.change_order_mut(id).unwrap();
    assert_eq!(order.items[0].change, ItemChange::NewPosition);
    order.items[0].offered_unit_price = Some(dec("1600"));
    let order = project.change_order(id).unwrap();
    assert!(order.problems(project.contract(), project.lv()).is_empty());
    // … und erst beauftragt abrechenbar, mit dem Nachtragspreis.
    let billable = crate::billable_items(
        project.contract().unwrap(),
        project.change_orders(),
        project.lv(),
    );
    assert!(!billable.contains_key(&b));
    project
        .change_order_mut(id)
        .unwrap()
        .set_status(ChangeOrderStatus::Approved, "02.11.2026", "");
    let billable = crate::billable_items(
        project.contract().unwrap(),
        project.change_orders(),
        project.lv(),
    );
    assert_eq!(billable[&b].unit_price, dec("1600"));
    // Mengenänderung an der entfallenen Position wäre ein Problem.
    let problems = ChangeOrder {
        items: vec![ChangeOrderItem {
            position: b,
            change: ItemChange::Quantity,
            offered_quantity: dec("1"),
            ..ChangeOrderItem::default()
        }],
        ..ChangeOrder::new(ChangeOrderId(9), 9, "x")
    }
    .problems(project.contract(), project.lv());
    assert!(
        problems
            .iter()
            .any(|p| p.contains("keine Auftragsposition")),
        "{problems:?}"
    );
}

/// Anwender 18.11.: Menge nach Angebotseingang geändert → Nachtrag mit
/// Mehr-/Mindermenge; ohne Auftrag wird das beauftragte Angebot übernommen.
#[test]
fn quantities_changed_after_the_bids_become_a_change_order() {
    let (mut project, [a, b]) = project();
    project.set_contract(None);
    assert!(
        project
            .quantity_changes_to_change_order("18.11.2026")
            .is_err(),
        "kein Auftrag"
    );
    let firm_a = project
        .bids()
        .iter()
        .find(|bid| bid.name() == "Firma A")
        .unwrap()
        .id();
    project.award_bid(firm_a, "18.11.2026").unwrap();
    assert_eq!(
        project
            .quantity_changes_to_change_order("18.11.2026")
            .unwrap_err(),
        "Nach dem Angebotseingang wurde keine Menge geändert."
    );
    for (row, quantity) in [(a, "120"), (b, "8")] {
        let index = project.lv().index_of(row).unwrap();
        project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_quantity(dec(quantity));
    }
    let (id, count) = project
        .quantity_changes_to_change_order("18.11.2026")
        .unwrap();
    assert_eq!(count, 2);
    let contract = project.contract().expect("Auftrag übernommen");
    assert_eq!(
        contract.item(a).unwrap().quantity,
        dec("100"),
        "Auftrag = Angebotsmenge"
    );
    let order = project.change_order(id).unwrap();
    assert_eq!(order.title, "Mengenänderungen nach Angebotseingang");
    assert_eq!(order.basis, ChangeBasis::QuantityChange);
    let quantities: Vec<_> = order
        .items
        .iter()
        .map(|item| (item.position, item.change, item.offered_quantity))
        .collect();
    assert_eq!(
        quantities,
        vec![
            (a, ItemChange::Quantity, dec("20")),
            (b, ItemChange::Quantity, dec("-2"))
        ]
    );
    // Mit dem Auftrags-EP bewertet: 20 × 2,50 − 2 × 80 = −110 €.
    assert_eq!(
        order.offered_total(project.contract()),
        Money::from_cents(-11_000)
    );
    assert!(
        order
            .reason
            .contains("01.01.0010: Massenmehrung: LV-Menge 120 statt 100 m (+20)"),
        "{}",
        order.reason
    );
    assert_eq!(
        project
            .quantity_changes_to_change_order("18.11.2026")
            .unwrap_err(),
        "Die geänderten Mengen stehen schon in einem Nachtrag."
    );
}
