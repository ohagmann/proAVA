use proava_application::ChangeOrderExport;
use proava_domain::{
    Address, Bid, ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderStatus, Decimal, Lv, LvGroup,
    OzMask, Position, Project, ProjectName, RichText,
};
use proava_gaeb::{xml, Phase};

use super::*;

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// LV 11.22.PPPP with two contract positions, awarded; one change order
/// with two new positions (one rejected) and a Mehrmenge.
fn project() -> (Project, ChangeOrderId) {
    let mut lv = Lv::new("LV 480 GA", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "ASP01"));
    let a = lv.push_position(Position::new(
        "01.01.0010",
        "E/A-Modul 8 AI",
        dec("2"),
        "St",
        None,
    ));
    let b = lv.push_position(Position::new("01.01.0020", "Kabel", dec("100"), "m", None));
    let mut project = Project::new(
        ProjectName::parse("Nachtrag Test").unwrap(),
        "2026-11-17",
        lv,
    );
    let mut bid = Bid::new(Address::named("Firma A GmbH"), "a.x84");
    bid.set_unit_price(a, Some(dec("465")));
    bid.set_unit_price(b, Some(dec("2.45")));
    let bid = project.add_bid(bid);
    project.award_bid(bid, "15.09.2026").unwrap();
    project.take_over_contract().unwrap();
    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "Wetterstation");
    order.basis = ChangeBasis::AdditionalPerformance;
    order.reason = "Bauherrenwunsch".into();
    order.offer_number = "NA-1".into();
    let id = project.add_change_order(order);
    let mut station = Position::new("", "Wetterstation", dec("1"), "St", None);
    station.set_long_text(RichText::plain("Kompakte Wetterstation, BACnet/IP."));
    let station = project
        .add_change_order_position(id, station, Some(dec("1947")))
        .unwrap();
    let mut weekend = Position::new("", "Zuschlag Wochenende", dec("1"), "psch", None);
    weekend.set_long_text(RichText::plain("Zuschlag für Arbeiten am Wochenende."));
    weekend.set_lump_sum(true);
    let extra = project
        .add_change_order_position(id, weekend, Some(dec("450")))
        .unwrap();
    project.assign_to_change_order(id, b).unwrap();
    let order = project.change_order_mut(id).unwrap();
    order.item_mut(station).unwrap().checked_unit_price = Some(dec("1697"));
    order.item_mut(station).unwrap().note = "EK geprüft".into();
    order.item_mut(extra).unwrap().status = Some(ChangeOrderStatus::Rejected);
    order.item_mut(b).unwrap().offered_quantity = dec("40");
    order.set_status(ChangeOrderStatus::Approved, "28.10.2026", "");
    (project, id)
}

#[test]
fn x83_and_x86_carry_number_status_and_only_new_positions() {
    let (project, id) = project();
    let (doc, warnings) = change_order_to_gaeb(&project, id, ChangeOrderExport::Request).unwrap();
    assert!(
        warnings.iter().any(|w| w.contains("Mindermenge")),
        "{warnings:?}"
    );
    let tag = doc.award.boq.change_order.clone().unwrap();
    assert_eq!((tag.number, tag.status.as_str()), (1, "Filed"));
    assert_eq!(doc.award.boq.label, "N01 Wetterstation");
    let items = doc.award.boq.items();
    let ordinals: Vec<String> = items.iter().map(|(path, _)| path.join(".")).collect();
    assert_eq!(
        ordinals,
        ["02.01.0010", "02.01.0020"],
        "nur die neuen Positionen mit ihren Titeln"
    );
    assert!(items.iter().all(|(_, item)| item.unit_price.is_none()));
    let text = xml::write(&doc, Phase::Dp83).unwrap();
    assert!(text.contains("<CONo>1</CONo>") && text.contains("Kompakte Wetterstation"));

    let (doc, _) = change_order_to_gaeb(&project, id, ChangeOrderExport::Order).unwrap();
    assert_eq!(
        doc.award.boq.change_order.as_ref().unwrap().status,
        "Approved"
    );
    let items = doc.award.boq.items();
    assert_eq!(items[0].1.unit_price, Some(dec("1697")), "geprüfter EP");
    assert_eq!(items[1].1.change_order.as_ref().unwrap().status, "Rejected");
    assert!(
        items[1].1.not_applicable,
        "abgelehnt = entfällt im Nachtragsauftrag"
    );
    let written = xml::write(&doc, Phase::Dp86).unwrap();
    // Summe = geprüfte Wetterstation 1.697 ohne den abgelehnten Zuschlag.
    let read = xml::read(written.as_bytes()).unwrap();
    assert_eq!(
        read.award.boq.totals.as_ref().and_then(|t| t.total),
        Some(dec("1697"))
    );
    assert_eq!(doc.award.bidder.as_ref().unwrap().name1, "Firma A GmbH");
    let written = xml::write(&doc, Phase::Dp86).unwrap();
    let again = xml::read(written.as_bytes()).unwrap();
    assert_eq!(again.award.boq.change_order.as_ref().unwrap().number, 1);
}

#[test]
fn offer_read_back_updates_the_change_order() {
    let (mut project, id) = project();
    // The firm answers the X83 with prices (as X84 would carry them).
    let (mut doc, _) = change_order_to_gaeb(&project, id, ChangeOrderExport::Request).unwrap();
    doc.phase = Phase::Dp84;
    let mut prices = [dec("1900"), dec("400")].into_iter();
    fn set(nodes: &mut [proava_gaeb::BoqNode], prices: &mut impl Iterator<Item = Decimal>) {
        for node in nodes {
            match node {
                proava_gaeb::BoqNode::Category(category) => set(&mut category.body, prices),
                proava_gaeb::BoqNode::Item(item) => item.unit_price = prices.next(),
                _ => {}
            }
        }
    }
    set(&mut doc.award.boq.body, &mut prices);
    doc.award.bidder = Some(proava_gaeb::Address {
        name1: "Firma A GmbH".into(),
        ..Default::default()
    });
    doc.award.bid_date = Some("2026-10-12".into());
    let written = xml::write(&doc, Phase::Dp84).unwrap();
    let read = xml::read(written.as_bytes()).unwrap();
    let (offer, warnings) = change_order_offer_from_gaeb(&read, "N01.X84");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(offer.date, "12.10.2026");
    assert_eq!(offer.number, None, "X84 hat keine CONo");
    let result = project
        .take_change_order_offer(id, &offer, "17.11.2026")
        .unwrap();
    assert_eq!((result.updated, result.created), (2, 0));
    let order = project.change_order(id).unwrap();
    assert_eq!(order.items[0].offered_unit_price, Some(dec("1900")));
    assert_eq!(
        order.items[0].checked_unit_price,
        Some(dec("1697")),
        "Prüfung bleibt"
    );
    assert_eq!(order.contractor, "", "Firma = Auftragnehmer");
}

#[test]
fn check_and_overview_for_print() {
    let (project, id) = project();
    let check = change_order_check_to_print(&project, id, "17.11.2026").unwrap();
    assert_eq!(check.title, "N01 Wetterstation");
    assert_eq!(check.lines.len(), 3);
    assert_eq!(check.lines[0].checked_total, "1.697,00 €");
    assert_eq!(check.lines[0].deviation, "-12,8");
    assert!(check.lines[1].dropped && check.lines[1].status == "abgelehnt");
    assert_eq!(check.lines[2].kind, "Mehrmenge");
    // 1.697 + 40 × 2,45 = 1.795,00 (Zuschlag abgelehnt).
    assert!(
        check.sums.iter().any(
            |(label, amount, _)| label == "Geprüfte Nachtragssumme netto" && amount == "1.795,00 €"
        ),
        "{:?}",
        check.sums
    );
    assert!(check
        .facts
        .iter()
        .any(|(label, value)| label == "Auftragnehmer" && value == "Firma A GmbH"));
    let (bytes, pages) = proava_print::render_change_order_check_pdf(&check).unwrap();
    assert!(bytes.starts_with(b"%PDF") && pages == 1);

    let overview = change_order_overview_to_print(&project, "17.11.2026");
    assert_eq!(overview.lines.len(), 1);
    assert_eq!(overview.lines[0].approved, "1.795,00 €");
    assert_eq!(overview.by_status[0][0], "beauftragt");
    assert!(
        overview
            .contract
            .iter()
            .any(|(label, amount, _)| label == "= Auftragssumme neu" && amount == "2.970,00 €"),
        "{:?}",
        overview.contract
    );
    let (bytes, _) = proava_print::render_change_order_overview_pdf(&overview).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

/// Prüfung Nachtragsangebot and Nachtragsübersicht in the Währung of the LV.
#[test]
fn change_order_prints_follow_the_currency_of_the_lv() {
    let (mut project, id) = project();
    project.lv_mut().set_currency("CHF");
    let check = change_order_check_to_print(&project, id, "17.11.2026").unwrap();
    assert_eq!(check.lines[0].checked_total, "1.697,00 CHF");
    assert!(
        check
            .sums
            .iter()
            .filter(|(_, amount, _)| !amount.ends_with('%'))
            .all(|(_, amount, _)| amount.ends_with(" CHF")),
        "{:?}",
        check.sums
    );
    let overview = change_order_overview_to_print(&project, "17.11.2026");
    assert_eq!(overview.lines[0].approved, "1.795,00 CHF");
    assert!(
        overview
            .contract
            .iter()
            .any(|(_, amount, _)| amount == "2.970,00 CHF"),
        "{:?}",
        overview.contract
    );
    assert_eq!(overview.header.currency, "CHF");
}

#[test]
fn change_order_files_pass_the_gaeb_rules() {
    let (project, id) = project();
    for (kind, phase) in [
        (ChangeOrderExport::Request, Phase::Dp83),
        (ChangeOrderExport::Order, Phase::Dp86),
    ] {
        let (doc, _) = change_order_to_gaeb(&project, id, kind).unwrap();
        let bytes = xml::write(&doc, phase).unwrap().into_bytes();
        let findings = crate::rule_check::file_findings(crate::GaebFormat::Xml, &bytes, phase);
        let lines: Vec<String> = findings.iter().map(crate::rule_line).collect();
        assert!(findings.is_empty(), "{phase:?}: {lines:?}");
    }
    // Eine neue Position ohne Langtext meldet R13 (Warnung) – berechtigt.
    let mut bare = project.clone();
    bare.add_change_order_position(
        id,
        Position::new("", "ohne Text", dec("1"), "St", None),
        None,
    )
    .unwrap();
    let (doc, _) = change_order_to_gaeb(&bare, id, ChangeOrderExport::Request).unwrap();
    let bytes = xml::write(&doc, Phase::Dp83).unwrap().into_bytes();
    let findings = crate::rule_check::file_findings(crate::GaebFormat::Xml, &bytes, Phase::Dp83);
    assert!(findings
        .iter()
        .any(|finding| crate::rule_line(finding).contains("R13")));
}

#[test]
fn position_marked_entfaellt_is_required_in_the_change_order_file() {
    let (mut project, id) = project();
    // Die Wetterstation stand im LV als „entfällt“ (z. B. aus einem früheren Stand).
    let row = project.change_order(id).unwrap().items[0].position;
    let index = project.lv().index_of(row).unwrap();
    project
        .lv_mut()
        .row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_not_applicable(true);
    for kind in [ChangeOrderExport::Request, ChangeOrderExport::Order] {
        let (doc, _) = change_order_to_gaeb(&project, id, kind).unwrap();
        let items = doc.award.boq.items();
        assert!(!items[0].1.not_applicable, "{kind:?}");
    }
}
