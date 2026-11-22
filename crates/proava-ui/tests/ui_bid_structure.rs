//! Abweichungen im Angebot (Anwender 20.11.): auffälliger Hinweis in der
//! Angebotsansicht, Dialog „Angebotspositionen zuordnen“ mit „Sichere
//! übernehmen“, Strg+Z nimmt es zurück.

mod common;

use common::*;
use proava_domain::{
    Address, Bid, BidItem, BidPrice, Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName,
    StoredSuggestion, StructureReport,
};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// LV with four positions; the bid priced 01.0010 by its OZ, the file
/// numbered the other three anew (sure proposals) and has one more.
fn sample() -> Project {
    let mut lv = Lv::new("Gebäudeautomation", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Automation"));
    let mut rows = Vec::new();
    for (oz, text, qty) in [
        ("01.0010", "DDC-Station", "1"),
        ("01.0020", "E/A-Modul", "4"),
        ("01.0030", "Netzteil", "2"),
        ("01.0040", "Schaltschrank", "1"),
    ] {
        rows.push(lv.push_position(Position::new(oz, text, dec(qty), "St", Some(dec("100")))));
    }
    let mut project = Project::new(ProjectName::parse("Musterhalle").unwrap(), "2026-11-20", lv);
    let mut bid = Bid::new(Address::named("Muster Regeltechnik GmbH"), "muster.x84");
    bid.set_price(
        rows[0],
        BidPrice {
            unit_price: Some(dec("1000")),
            stated_total: Some(dec("1000")),
            not_offered: false,
        },
    );
    let item = |oz: &str, qty: &str, ep: &str| BidItem {
        ordinal: oz.into(),
        quantity: Some(dec(qty)),
        unit_price: Some(dec(ep)),
        ..BidItem::default()
    };
    let foreign = vec![
        item("01.0021", "4", "300"),
        item("01.0022", "2", "80"),
        item("01.0023", "1", "5000"),
        item("01.0099", "2", "150"),
    ];
    let suggestions = foreign[..3]
        .iter()
        .zip(&rows[1..])
        .map(|(item, row)| StoredSuggestion {
            ordinal: item.ordinal.clone(),
            row: *row,
            similarity: 95,
            sure: true,
        })
        .collect();
    bid.set_structure(StructureReport {
        foreign,
        suggestions,
        ..StructureReport::default()
    });
    bid.set_unknown_ordinals(vec![
        "01.0021".into(),
        "01.0022".into(),
        "01.0023".into(),
        "01.0099".into(),
    ]);
    project.add_bid(bid);
    project
}

#[test]
fn the_bid_view_shows_the_deviations_and_assigns_the_sure_proposals() {
    let folder = TempDir::new("angebot-struktur");
    let file = save(&folder.0, &sample());
    let mut harness = harness(&folder.0, Some(proava_ui::StartFile::Project(file)));
    harness
        .query_all_by(|node| {
            node.label()
                .is_some_and(|label| label.starts_with("Angebote ("))
        })
        .next()
        .expect("Reiter Angebote")
        .click();
    settle(&mut harness);
    click(&mut harness, "Muster Regeltechnik GmbH");
    assert!(
        shows_part(
            &harness,
            "Strukturabweichungen: 4 Positionen nicht im LV, 3 LV-Positionen fehlen"
        ),
        "Hinweis über den Prüfbefunden"
    );
    assert!(shows_part(
        &harness,
        "Vorschlag (sicher, 95 %): Angebot 01.0021 → LV 01.0020 E/A-Modul"
    ));

    click(&mut harness, "Positionen zuordnen …");
    assert!(shows(&harness, "Angebotspositionen zuordnen"), "Dialog");
    assert!(
        shows(&harness, "01.0099"),
        "auch die Position ohne Vorschlag"
    );
    assert!(shows_part(&harness, "95 % · sicher"));
    click(&mut harness, "Sichere übernehmen");
    assert!(!shows(&harness, "Angebotspositionen zuordnen"), "Dialog zu");
    let lv = project_of(&harness).lv().clone();
    let bid = &project_of(&harness).bids()[0];
    let row = |oz: &str| lv.position_by_ordinal(oz).unwrap().id();
    assert_eq!(bid.unit_price(row("01.0020")), Some(dec("300")));
    assert_eq!(bid.unit_price(row("01.0040")), Some(dec("5000")));
    assert_eq!(bid.structure().assigned.len(), 3);
    assert!(shows_part(&harness, "3 Positionen zugeordnet"));
    assert!(shows_part(
        &harness,
        "Strukturabweichungen: 3 umnummeriert (zugeordnet), 1 Position nicht im LV"
    ));

    // Strg+Z: as before.
    press_with(&mut harness, Modifiers::COMMAND, Key::Z);
    let bid = &project_of(&harness).bids()[0];
    assert_eq!(bid.unit_price(row("01.0020")), None);
    assert_eq!(bid.structure().suggestions.len(), 3);
}

fn project_of(harness: &UiHarness) -> &Project {
    common::project(harness)
}
