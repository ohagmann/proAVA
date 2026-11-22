//! Strukturvergleich beim Einlesen eines Angebots (Anwender 20.11.):
//! umnummerierte OZ, fehlende und zusätzliche Positionen, geänderte Menge,
//! Einheit, Kurztext und Positionsart, Titel umbenannt und umgestellt –
//! synthetische Dateien, keine Kundendaten.

use proava_application::BidMatchOptions;
use proava_domain::{
    AssignedBy, Decimal, Lv, LvGroup, Money, OzMask, Position, Project, ProjectName,
};
use proava_gaeb::{self as gaeb, BoqNode, Item, Phase, Provisional};

use crate::{lv_to_gaeb, match_bid_with};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 01 Automation (01.01 ASP01 with four, 01.02 ASP02 with three
/// positions), 02 Feldgeräte (02.01 Fühler, 02.02 Ventile).
fn project() -> Project {
    let mut lv = Lv::new("Gebäudeautomation", OzMask::parse("11.22.PPPP").unwrap());
    let position = |lv: &mut Lv, oz: &str, text: &str, qty: &str, unit: &str, ep: &str| {
        lv.push_position(Position::new(oz, text, dec(qty), unit, Some(dec(ep))));
    };
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    lv.push_group(LvGroup::new(1, "01.01", "ASP01"));
    position(&mut lv, "01.01.0010", "DDC-Station", "1", "St", "1000");
    position(&mut lv, "01.01.0020", "E/A-Modul", "4", "St", "300");
    position(&mut lv, "01.01.0030", "Netzteil", "2", "St", "80");
    position(&mut lv, "01.01.0040", "Schaltschrank", "1", "St", "5000");
    lv.push_group(LvGroup::new(1, "01.02", "ASP02"));
    position(&mut lv, "01.02.0010", "DDC-Station", "1", "St", "1000");
    position(&mut lv, "01.02.0020", "E/A-Modul", "3", "St", "300");
    position(&mut lv, "01.02.0030", "Bedienteil", "1", "St", "450");
    lv.push_group(LvGroup::new(0, "02", "Feldgeräte"));
    lv.push_group(LvGroup::new(1, "02.01", "Fühler"));
    position(&mut lv, "02.01.0010", "Raumfühler", "10", "St", "60");
    position(&mut lv, "02.01.0020", "Kanalfühler", "4", "St", "90");
    position(&mut lv, "02.01.0030", "Außenfühler", "1", "St", "120");
    lv.push_group(LvGroup::new(1, "02.02", "Ventile"));
    position(&mut lv, "02.02.0010", "Ventil DN15", "6", "St", "200");
    position(&mut lv, "02.02.0020", "Ventil DN25", "2", "St", "260");
    Project::new(ProjectName::parse("Musterhalle").unwrap(), "2026-11-20", lv)
}

fn category<'a>(nodes: &'a mut [BoqNode], rno: &str) -> &'a mut gaeb::Category {
    nodes
        .iter_mut()
        .find_map(|node| match node {
            BoqNode::Category(category) if category.rno_part.trim() == rno => Some(category),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Titel {rno}"))
}

fn item<'a>(nodes: &'a mut [BoqNode], rno: &str) -> &'a mut Item {
    nodes
        .iter_mut()
        .find_map(|node| match node {
            BoqNode::Item(item) if item.rno_part.trim() == rno => Some(item),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Position {rno}"))
}

/// The bidder's file: the LV with its prices, changed as bidders do.
fn bid_file(project: &Project) -> gaeb::GaebDocument {
    let (mut doc, _) = lv_to_gaeb(project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    doc.award.bidder = Some(gaeb::Address {
        name1: "Muster Regeltechnik GmbH".into(),
        ..gaeb::Address::default()
    });
    let body = &mut doc.award.boq.body;
    {
        let automation = category(body, "01");
        // ASP01: the planner of the bidder numbered 0020–0040 anew.
        let asp01 = category(&mut automation.body, "01");
        for (old, new) in [("0020", "0021"), ("0030", "0022"), ("0040", "0023")] {
            item(&mut asp01.body, old).rno_part = new.into();
        }
        // ASP02: quantity 5 instead of 3 (GP for 5), other Kurztext.
        let asp02 = category(&mut automation.body, "02");
        let changed = item(&mut asp02.body, "0020");
        changed.qty = Some(dec("5"));
        changed.total = Some(dec("1500"));
        item(&mut asp02.body, "0030").short_text = "Bediengerät mit Display".into();
        // ASP02 before ASP01.
        automation.body.swap(0, 1);
    }
    {
        let field = category(body, "02");
        let sensors = category(&mut field.body, "01");
        sensors.label = "Sensoren".into();
        item(&mut sensors.body, "0010").unit = "m".into();
        // One position more: not in the LV.
        sensors.body.push(BoqNode::Item(Item {
            rno_part: "0040".into(),
            short_text: "Feuchtefühler".into(),
            qty: Some(dec("2")),
            unit: "St".into(),
            unit_price: Some(dec("150")),
            total: Some(dec("300")),
            ..Item::default()
        }));
        let valves = category(&mut field.body, "02");
        // DN15 offered as Bedarfsposition, DN25 missing.
        item(&mut valves.body, "0010").kind.provisional = Some(Provisional::WithTotal);
        valves
            .body
            .retain(|node| !matches!(node, BoqNode::Item(item) if item.rno_part.trim() == "0020"));
    }
    // The file sum as the bidder's program adds it up.
    let mut sum = Decimal::ZERO;
    for (_, item) in doc.award.boq.items() {
        sum += item.total.unwrap_or_default();
    }
    doc.award.boq.totals = Some(gaeb::Totals {
        total: Some(sum),
        ..gaeb::Totals::default()
    });
    doc
}

fn ordinal(project: &Project, row: proava_domain::RowId) -> String {
    project
        .lv()
        .position_by_id(row)
        .unwrap()
        .ordinal()
        .to_string()
}

#[test]
fn every_deviation_is_in_the_protocol_and_shifted_numbers_are_proposed() {
    let project = project();
    let doc = bid_file(&project);
    let import = match_bid_with(&project, &doc, "muster.x84", &BidMatchOptions::default());
    let bid = &import.bid;
    let report = bid.structure();
    // By OZ: the renumbered three and the extra one are not in the LV.
    let foreign: Vec<&str> = report
        .foreign
        .iter()
        .map(|item| item.ordinal.as_str())
        .collect();
    assert_eq!(
        foreign,
        ["01.01.0021", "01.01.0022", "01.01.0023", "02.01.0040"]
    );
    assert_eq!(
        report.summary(bid.absent_positions(project.lv()).len()).unwrap(),
        "Strukturabweichungen: 4 Positionen nicht im LV, 4 LV-Positionen fehlen, 1 Menge, 1 Einheit, 1 Kurztext, 1 Positionsart, 2 Titel"
    );
    // Proposals: the three in order (quantities confirm), sure.
    let proposals: Vec<(String, String, bool)> = report
        .suggestions
        .iter()
        .map(|s| (s.ordinal.clone(), ordinal(&project, s.row), s.sure))
        .collect();
    assert_eq!(
        proposals,
        [
            ("01.01.0021".to_string(), "01.01.0020".to_string(), true),
            ("01.01.0022".into(), "01.01.0030".into(), true),
            ("01.01.0023".into(), "01.01.0040".into(), true),
        ]
    );
    let lines: Vec<String> = report
        .lines(project.lv(), &bid.absent_positions(project.lv()))
        .iter()
        .map(proava_domain::DeviationLine::text)
        .collect();
    for expected in [
        "nicht im LV: Angebot 02.01.0040 – Feuchtefühler, EP 150, GP 300,00 € – nicht gewertet",
        "fehlt im Angebot: LV 02.02.0020 – Ventil DN25",
        "Menge: 01.02.0020 – LV 3, Angebot 5 – gewertet mit der LV-Menge",
        "Einheit: 02.01.0010 – LV „St“, Angebot „m“",
        "Kurztext: 01.02.0030 – LV „Bedienteil“, Angebot „Bediengerät mit Display“",
        "Positionsart: 02.02.0010 – LV Normalposition, Angebot Bedarfsposition mit GB",
        "Titel: Titel 02.01 heißt im Angebot „Sensoren“ statt „Fühler“",
        "Titel: Titelreihenfolge im Angebot umgestellt (01.02 steht vor 01.01)",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "{expected}\n{lines:#?}"
        );
    }
    // The import messages name it, and the sure proposals not taken.
    assert!(import
        .warnings
        .iter()
        .any(|w| w.starts_with("Strukturabweichungen: 4 Positionen")));
    assert!(import
        .warnings
        .iter()
        .any(|w| w.contains("3 sichere Zuordnungsvorschläge nicht übernommen")));
}

#[test]
fn by_structure_the_sure_ones_are_taken_and_the_lv_quantity_is_ranked() {
    let project = project();
    let doc = bid_file(&project);
    let options = BidMatchOptions {
        by_structure: true,
        manual: Vec::new(),
    };
    let import = match_bid_with(&project, &doc, "muster.x84", &options);
    let mut project = project;
    let id = project.add_bid(import.bid);
    let lv = project.lv().clone();
    let bid = project.bids().iter().find(|bid| bid.id() == id).unwrap();
    let report = bid.structure();
    let assigned: Vec<(String, String, AssignedBy)> = report
        .assigned
        .iter()
        .map(|a| (a.item.ordinal.clone(), ordinal(&project, a.row), a.by))
        .collect();
    assert_eq!(
        assigned,
        [
            (
                "01.01.0021".to_string(),
                "01.01.0020".to_string(),
                AssignedBy::Structure
            ),
            (
                "01.01.0022".into(),
                "01.01.0030".into(),
                AssignedBy::Structure
            ),
            (
                "01.01.0023".into(),
                "01.01.0040".into(),
                AssignedBy::Structure
            ),
        ]
    );
    assert_eq!(report.foreign.len(), 1, "nur der Feuchtefühler");
    // Ranked with the LV quantity: E/A-Modul ASP02 3 × 300, not 5 × 300.
    let changed = lv.position_by_ordinal("01.02.0020").unwrap();
    assert_eq!(bid.total_for(changed), Money::from_decimal(dec("900")));
    // ASP01 7.360 + ASP02 2.350 + Fühler 1.080 + DN15 1.200 (DN25 fehlt).
    let netto = Money::from_decimal(dec("11990"));
    assert_eq!(bid.net_total(&lv), netto);
    // File: + 600 (Menge 5 statt 3) + 300 (Feuchtefühler).
    let breakdown = bid.total_breakdown(&lv).unwrap();
    assert_eq!(breakdown.file, Money::from_decimal(dec("12890")));
    assert_eq!(breakdown.foreign, Money::from_decimal(dec("300")));
    assert_eq!(breakdown.quantities, Money::from_decimal(dec("600")));
    assert_eq!(breakdown.arithmetic, Money::ZERO);
    assert_eq!(breakdown.rest, Money::ZERO);
    assert!(
        bid.arithmetic_errors(&lv).is_empty(),
        "Menge 5 ist kein Rechenfehler"
    );
}

#[test]
fn by_hand_a_file_position_goes_where_the_user_says() {
    let project = project();
    let doc = bid_file(&project);
    let options = BidMatchOptions {
        by_structure: false,
        manual: vec![("02.01.0040".into(), "02.02.0020".into())],
    };
    let import = match_bid_with(&project, &doc, "muster.x84", &options);
    let report = import.bid.structure();
    let hand: Vec<_> = report
        .assigned
        .iter()
        .filter(|a| a.by == AssignedBy::Hand)
        .collect();
    assert_eq!(hand.len(), 1);
    assert_eq!(ordinal(&project, hand[0].row), "02.02.0020");
    let dn25 = project.lv().position_by_ordinal("02.02.0020").unwrap();
    assert_eq!(import.bid.unit_price(dn25.id()), Some(dec("150")));
    assert!(!report
        .foreign
        .iter()
        .any(|item| item.ordinal == "02.01.0040"));
}
