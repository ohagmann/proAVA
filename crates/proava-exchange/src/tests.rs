use std::path::Path;

use proava_application::ExchangeFiles;

mod bid_structure;
use proava_domain::{
    Address, Decimal, Discount, Lv, LvGroup, LvRow, Money, Note, OzMask, Position, PositionKind,
    Project, ProjectName, RichText, Run, Table, TextBlock,
};
use proava_gaeb::{self as gaeb, BoqNode, BreakdownKind, GaebDocument, Item, Phase};

use crate::oz::{detect_step, MaskLayout};
use crate::text::{format_deadline, parse_deadline};
use crate::{
    breakdown_from_mask, gaeb_to_project, lv_to_gaeb, mask_from_breakdown, match_bid, GaebFormat,
    GaebPdfExchange,
};

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn position(ordinal: &str, text: &str, qty: &str, unit: &str, price: Option<&str>) -> Position {
    Position::new(ordinal, text, dec(qty), unit, price.map(dec))
}

fn with_kind(mut position: Position, kind: PositionKind) -> Position {
    position.set_kind(kind);
    position
}

fn formatted_text() -> RichText {
    RichText::new(vec![
        TextBlock::paragraph(vec![
            Run::plain("Liefern und montieren, "),
            Run {
                text: "fett".into(),
                bold: true,
                ..Run::default()
            },
            Run {
                text: " kursiv unterstrichen".into(),
                italic: true,
                underline: true,
                ..Run::default()
            },
        ]),
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![Run::plain("BACnet/IP")],
            align: Default::default(),
        },
        TextBlock::ListItem {
            ordered: true,
            level: 0,
            runs: vec![Run::plain("Schaltschrank")],
            align: Default::default(),
        },
        TextBlock::Table(Table::from_texts(vec![
            vec!["Typ".into(), "Wert".into()],
            vec!["E/A".into(), "32".into()],
        ])),
        TextBlock::Image {
            file: "schema.png".into(),
            width_mm: Some(80.0),
        },
    ])
}

/// Two levels, every position kind, hints on three places, a jump in the
/// subtitle numbers (01 → 10) and an index.
fn demo_project() -> Project {
    let mask = OzMask::parse("11.22.PPPPI").unwrap();
    let mut lv = Lv::new("GA Gebäudeautomation", mask);
    lv.push_note(Note::rich(RichText::new(vec![
        TextBlock::paragraph(vec![Run {
            text: "Vorbemerkung".into(),
            bold: true,
            ..Run::default()
        }]),
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![Run::plain("VOB/B gilt")],
            align: Default::default(),
        },
    ])));
    lv.push_group(LvGroup::new(0, "01", "Automationsstationen"));
    lv.push_note(Note::new("Hinweis zum Titel"));
    lv.push_group(LvGroup::new(1, "01.01", "ASP01"));

    let mut first = position("01.01.0010", "DDC-Station", "2", "St", Some("1250.50"));
    first.set_long_text(formatted_text());
    lv.push_position(first);
    lv.push_position(with_kind(
        position("01.01.0020", "Reserve-Modul", "3", "St", Some("100")),
        PositionKind::Provisional { with_total: false },
    ));
    lv.push_position(with_kind(
        position("01.01.0030", "Zusatzmodul", "1", "St", Some("80")),
        PositionKind::Provisional { with_total: true },
    ));
    let mut base = with_kind(
        position(
            "01.01.0040",
            "Schrank Stahlblech",
            "1",
            "psch",
            Some("3000"),
        ),
        PositionKind::Base { group: 1 },
    );
    base.set_lump_sum(true);
    lv.push_position(base);
    let mut alternative = with_kind(
        position("01.01.0050", "Schrank Edelstahl", "1", "psch", Some("4500")),
        PositionKind::Alternative {
            group: 1,
            serial: 1,
        },
    );
    alternative.set_lump_sum(true);
    lv.push_position(alternative);
    lv.push_position(position(
        "01.01.0060",
        "Zulage Sockel",
        "1",
        "St",
        Some("150"),
    ));
    let mut hourly = position(
        "01.01.0070.1",
        "Stundenlohn Techniker",
        "8",
        "h",
        Some("85.25"),
    );
    hourly.set_hourly(true);
    lv.push_position(hourly);
    lv.push_note(Note::new("Hinweis am Ende von ASP01"));

    lv.push_group(LvGroup::new(1, "01.10", "ASP10"));
    lv.push_position(position(
        "01.10.0010",
        "DDC-Station klein",
        "1",
        "St",
        Some("900"),
    ));
    lv.push_group(LvGroup::new(0, "02", "Feldgeräte"));
    lv.push_group(LvGroup::new(1, "02.01", "Fühler"));
    lv.push_position(position(
        "02.01.0010",
        "Temperaturfühler",
        "12.5",
        "St",
        None,
    ));

    let mut project = Project::new(
        ProjectName::parse("RH3").unwrap(),
        "2026-11-17T18:00:00",
        lv,
    );
    let info = project.info_mut();
    info.label = "Rathaus 3 Musterstadt".into();
    info.owner = Address {
        name1: "Stadt Düsseldorf".into(),
        street: "Marktplatz 1".into(),
        postal_code: "40213".into(),
        city: "Düsseldorf".into(),
        ..Address::default()
    };
    info.submission_place = "Vergabestelle, Raum 12".into();
    info.submission_deadline = "15.12.2026, 10:00 Uhr".into();
    project
}

/// Everything that must survive a round trip, without ids.
fn snapshot(lv: &Lv) -> Vec<String> {
    lv.rows()
        .iter()
        .map(|row| match row {
            LvRow::Group(group) => format!(
                "G{} {} {}",
                group.level(),
                group.ordinal(),
                group.short_text()
            ),
            LvRow::Note(note) => format!("N {:?}", note.text().without_images()),
            LvRow::Spacer(_) => "S".to_string(),
            LvRow::Performance(performance) => format!(
                "A {} {} {} {:?}",
                performance.number(),
                performance.label(),
                performance.short_text(),
                performance.long_text().without_images()
            ),
            LvRow::Position(position) => format!(
                "P {} {} {} {} {:?} {:?} psch={} h={} AB={:?} {:?}",
                position.ordinal(),
                position.short_text(),
                position.quantity().normalize(),
                position.unit(),
                position.unit_price().map(|price| price.normalize()),
                position.kind(),
                position.is_lump_sum(),
                position.is_hourly(),
                lv.performance_of(position)
                    .map(|performance| performance.number()),
                position.long_text().without_images()
            ),
        })
        .collect()
}

fn categories(nodes: &[BoqNode]) -> Vec<&gaeb::Category> {
    nodes
        .iter()
        .filter_map(|node| match node {
            BoqNode::Category(category) => Some(category),
            _ => None,
        })
        .collect()
}

fn for_items(nodes: &mut [BoqNode], action: &mut dyn FnMut(&mut Item)) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => for_items(&mut category.body, action),
            BoqNode::Item(item) => action(item),
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
        }
    }
}

fn has(warnings: &[String], text: &str) -> bool {
    warnings.iter().any(|warning| warning.contains(text))
}

#[test]
fn round_trip_keeps_outline_kinds_texts_and_prices() {
    let project = demo_project();
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp82);
    assert!(
        !has(&warnings, "Bild"),
        "Bilder meldet erst das Schreiben: {warnings:?}"
    );
    assert!(
        has(&warnings, "ohne Einheitspreis: 02.01.0010"),
        "{warnings:?}"
    );

    let (back, import_warnings) = gaeb_to_project(
        &doc,
        ProjectName::parse("RH3").unwrap(),
        "2026-11-17T19:00:00",
    );
    assert!(import_warnings.is_empty(), "{import_warnings:?}");
    assert_eq!(snapshot(back.lv()), snapshot(project.lv()));
    assert_eq!(back.lv().oz_mask().pattern(), "11.22.PPPPI");
    assert_eq!(back.lv().position_step(), 10);
    assert_eq!(back.lv().title(), "GA Gebäudeautomation");
    assert_eq!(back.lv().vat_percent(), dec("19"));
    assert_eq!(back.lv().currency(), "EUR");
    assert_eq!(back.info(), project.info());
    assert_eq!(back.lv().net_total(), project.lv().net_total());
}

#[test]
fn priced_export_writes_totals_of_counted_positions_only() {
    let project = demo_project();
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    let net = project.lv().net_total();
    // 2 × 1250,50 + 80 + 3000 + 150 + 8 × 85,25 + 900; Bedarf ohne GB and Wahl stay out.
    assert_eq!(
        net,
        Money::from_cents(250100 + 8000 + 300000 + 15000 + 68200 + 90000)
    );
    let totals = doc.award.boq.totals.clone().unwrap();
    assert_eq!(totals.total, Some(net.to_decimal()));
    assert_eq!(totals.vat_percent, Some(dec("19")));
    assert_eq!(
        totals.total_gross,
        Some((net + project.lv().vat(net)).to_decimal())
    );

    let titles = categories(&doc.award.boq.body);
    assert_eq!(titles.len(), 2);
    assert_eq!(
        titles[0].totals.as_ref().unwrap().total,
        Some(net.to_decimal())
    );
    let subtitles = categories(&titles[0].body);
    assert_eq!(
        subtitles
            .iter()
            .map(|c| c.rno_part.as_str())
            .collect::<Vec<_>>(),
        ["01", "10"]
    );
    assert_eq!(
        subtitles[1].totals.as_ref().unwrap().total,
        Some(dec("900.00"))
    );

    let items = doc.award.boq.items();
    let provisional = items
        .iter()
        .find(|(_, item)| item.rno_part == "0020")
        .unwrap()
        .1;
    assert_eq!(
        provisional.kind.provisional,
        Some(gaeb::Provisional::WithoutTotal)
    );
    assert_eq!(provisional.total, None);
    let base = items
        .iter()
        .find(|(_, item)| item.rno_part == "0040")
        .unwrap()
        .1;
    assert_eq!(base.kind.alternative.as_ref().unwrap().serial, "0");
    assert!(base.kind.lump_sum);
    let markup = items
        .iter()
        .find(|(_, item)| item.rno_part == "0060")
        .unwrap()
        .1;
    assert!(!markup.kind.markup);
    let hourly = items
        .iter()
        .find(|(_, item)| item.rno_part == "0070")
        .unwrap();
    assert_eq!(hourly.0, ["01", "01", "0070", "1"]);
    assert!(hourly.1.kind.hourly);
    assert_eq!(hourly.1.total, Some(dec("682.00")));

    assert!(matches!(doc.award.boq.body[0], BoqNode::Remark(_)));
    assert!(matches!(titles[0].body[0], BoqNode::Remark(_)));
    assert_eq!(doc.project.name, "RH3");
    assert_eq!(doc.project.label, "Rathaus 3 Musterstadt");
    assert_eq!(doc.award.owner.as_ref().unwrap().city, "Düsseldorf");
    assert_eq!(
        doc.award.submission.open_date.as_deref(),
        Some("2026-12-15")
    );
    assert_eq!(doc.award.submission.open_time.as_deref(), Some("10:00:00"));
}

#[test]
fn request_for_quotation_has_no_prices() {
    let project = demo_project();
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(!has(&warnings, "Einheitspreis"));
    assert!(doc.award.boq.totals.is_none());
    for (_, item) in doc.award.boq.items() {
        assert!(item.unit_price.is_none() && item.total.is_none());
        assert!(item.qty.is_some());
    }
    assert!(categories(&doc.award.boq.body)
        .iter()
        .all(|c| c.totals.is_none()));
    let (back, _) = gaeb_to_project(&doc, ProjectName::parse("X").unwrap(), "");
    assert!(back
        .lv()
        .positions()
        .all(|position| position.unit_price().is_none()));
}

#[test]
fn three_levels_round_trip() {
    let mask = OzMask::parse("1.22.33.PPPP").unwrap();
    let mut lv = Lv::new("Drei Ebenen", mask);
    lv.push_group(LvGroup::new(0, "1", "Los-artiger Titel"));
    lv.push_group(LvGroup::new(1, "1.02", "Untertitel"));
    lv.push_group(LvGroup::new(2, "1.02.10", "Abschnitt"));
    lv.push_position(position("1.02.10.0100", "A", "1", "St", Some("1")));
    lv.push_position(position("1.02.10.0200", "B", "1", "St", Some("2")));
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp82);
    assert!(warnings.is_empty(), "{warnings:?}");
    let labels: Vec<_> = doc
        .award
        .boq
        .breakdown
        .iter()
        .map(|level| level.label.as_str())
        .collect();
    assert_eq!(labels, ["Titel", "Untertitel", "Abschnitt", ""]);
    let (back, _) = gaeb_to_project(&doc, ProjectName::parse("P").unwrap(), "");
    assert_eq!(snapshot(back.lv()), snapshot(project.lv()));
    assert_eq!(back.lv().position_step(), 100);
}

/// GAEB DA XML 3.3, Fachdokumentation 4.3.4: lower levels may stay unused.
/// A position under a title without subtitles goes straight into that
/// LV-Bereich – no category „00“ (a Teil-OZ of zeros alone is not allowed).
#[test]
fn position_above_the_deepest_level_goes_straight_into_its_title() {
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Titel"));
    lv.push_position(position("", "Direkt unter dem Titel", "1", "St", None));
    lv.renumber();
    assert_eq!(lv.rows()[1].ordinal(), "01.00.0010");
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(!has(&warnings, "Zwischenstufe"), "{warnings:?}");
    let title = categories(&doc.award.boq.body)[0];
    assert!(categories(&title.body).is_empty(), "keine Zwischenstufe");
    assert_eq!(doc.award.boq.items()[0].0, ["01", "0010"]);
}

/// A title with positions and subtitles: GAEB lets a Bereich hold only one
/// of both – its positions get an empty intermediate category, with a note.
#[test]
fn positions_beside_subtitles_get_an_empty_intermediate_category() {
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Titel"));
    lv.push_position(position("", "Direkt unter dem Titel", "1", "St", None));
    lv.push_group(LvGroup::new(1, "", "Untertitel"));
    lv.push_position(position("", "Darunter", "1", "St", None));
    lv.renumber();
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(
        has(&warnings, "Zwischenstufe ohne Text eingefügt): 01.00.0010"),
        "{warnings:?}"
    );
    let title = categories(&doc.award.boq.body)[0];
    let inner: Vec<(&str, &str)> = categories(&title.body)
        .iter()
        .map(|category| (category.rno_part.as_str(), category.label.as_str()))
        .collect();
    assert_eq!(inner, [("00", ""), ("01", "Untertitel")]);
}

/// Bids for an LV with positions under a higher title match both ways the
/// request may have carried them: straight under the title („01.0010“, now)
/// and in an empty intermediate category „00“ (earlier export).
#[test]
fn bids_match_positions_above_the_deepest_level_with_and_without_intermediate_category() {
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Titel"));
    lv.push_position(position("", "A", "2", "St", Some("10")));
    lv.push_position(position("", "B", "1", "St", Some("20")));
    lv.renumber();
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let ids: Vec<_> = project
        .lv()
        .positions()
        .map(|position| position.id())
        .collect();

    let (mut now, _) = lv_to_gaeb(&project, Phase::Dp82);
    now.phase = Phase::Dp84;
    let mut earlier = now.clone();
    let BoqNode::Category(title) = &mut earlier.award.boq.body[0] else {
        panic!("Titel");
    };
    let items = std::mem::take(&mut title.body);
    title.body.push(BoqNode::Category(gaeb::Category {
        rno_part: "00".into(),
        body: items,
        ..gaeb::Category::default()
    }));
    for (doc, what) in [
        (now, "ohne Zwischenstufe"),
        (earlier, "mit Zwischenstufe 00"),
    ] {
        let import = match_bid(&project, &doc, "a.x84");
        assert!(
            import.bid.unknown_ordinals().is_empty(),
            "{what}: {:?}",
            import.warnings
        );
        for id in &ids {
            assert!(import.bid.price(*id).is_some(), "{what}: Preis für {id:?}");
        }
    }
}

/// Like a Positionsbibliothek: mask with four title levels, positions under
/// titles of level 2 – X81 and X83 without intermediate categories, back in
/// with the same outline.
#[test]
fn positions_under_level_two_of_four_round_trip_without_intermediate_levels() {
    let mut lv = Lv::new("Bibliothek", OzMask::parse("11.222.33.44.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Automation"));
    lv.push_group(LvGroup::new(1, "", "Hardware"));
    lv.push_position(position("", "Bedienstation", "2", "St", Some("2500")));
    lv.push_position(position("", "Router", "1", "St", Some("300")));
    lv.push_group(LvGroup::new(1, "", "Software"));
    lv.push_position(position("", "Grundsoftware", "1", "psch", Some("4000")));
    lv.renumber();
    assert_eq!(lv.rows()[2].ordinal(), "01.001.00.00.0010");
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(!has(&warnings, "Zwischenstufe"), "{warnings:?}");
    let items: Vec<String> = doc
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, _)| pieces.join("."))
        .collect();
    assert_eq!(items, ["01.001.0010", "01.001.0020", "01.002.0010"]);
    // The rules of the Fachdokumentation: no error, no Teil-OZ of zeros.
    let findings = proava_gaeb::rules::check(&doc, Phase::Dp83, None);
    assert!(
        !findings.iter().any(|finding| {
            finding.severity == proava_gaeb::rules::Severity::Error || finding.id == "R2.zero"
        }),
        "{findings:?}"
    );

    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-level-two-")
        .tempdir()
        .unwrap();
    for (file, kind) in [
        ("lv.X81", proava_application::GaebExport::Description),
        (
            "lv.X83",
            proava_application::GaebExport::RequestForQuotation,
        ),
    ] {
        let path = scratch.path().join(file);
        exchange.export_gaeb(&project, kind, &path).unwrap();
        let (back, _) = exchange
            .import_lv(&path, &ProjectName::parse("P").unwrap(), "")
            .unwrap();
        let outline: Vec<(String, String)> = back
            .lv()
            .rows()
            .iter()
            .map(|row| (row.ordinal().to_string(), row.short_text().to_string()))
            .collect();
        let expected: Vec<(String, String)> = [
            ("01", "Automation"),
            ("01.001", "Hardware"),
            ("01.001.0010", "Bedienstation"),
            ("01.001.0020", "Router"),
            ("01.002", "Software"),
            ("01.002.0010", "Grundsoftware"),
        ]
        .iter()
        .map(|(ordinal, text)| (ordinal.to_string(), text.to_string()))
        .collect();
        assert_eq!(outline, expected, "{file}");
        assert_eq!(back.lv().oz_mask().pattern(), "11.222.33.44.PPPP", "{file}");
        // Schemaprüfung, wenn die GAEB-XSDs auf dem Rechner liegen.
        if let Some(xsd) = proava_gaeb::schemas::schema_folder() {
            let script = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../proava-gaeb/scripts/validate_xsd.py"
            );
            let output = std::process::Command::new("python3")
                .args([script, xsd.to_str().unwrap(), path.to_str().unwrap()])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{file}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn imported_position_above_the_deepest_level_keeps_its_ordinal() {
    // As AVA programs write it (GAEB 90 fits 9 digits): mask with three levels, positions of 01.01 directly
    // below the second level, those of 01.02 on the third.
    let mut lv = Lv::new("LV", OzMask::parse("11.22.33.PPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Los 1"));
    lv.push_group(LvGroup::new(1, "01.01", "Automation"));
    lv.push_position(position("01.01.001", "Flach", "1", "St", None));
    lv.push_position(position("01.01.002", "Flach 2", "2", "St", None));
    lv.push_group(LvGroup::new(1, "01.02", "Schränke"));
    lv.push_group(LvGroup::new(2, "01.02.01", "ASP01"));
    lv.push_position(position("01.02.01.001", "Tief", "1", "St", None));
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(!has(&warnings, "Zwischenstufe"), "{warnings:?}");
    assert!(!has(&warnings, "passt nicht zur OZ-Maske"), "{warnings:?}");
    let items: Vec<String> = doc
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, _)| pieces.join("."))
        .collect();
    assert_eq!(items, ["01.01.001", "01.01.002", "01.02.01.001"]);

    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-shallow-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for file in ["lv.X83", "lv.D83"] {
        let path = folder.join(file);
        exchange
            .export_gaeb(
                &project,
                proava_application::GaebExport::RequestForQuotation,
                &path,
            )
            .unwrap();
        let (back, _) = exchange
            .import_lv(&path, &ProjectName::parse("P").unwrap(), "")
            .unwrap();
        let ordinals: Vec<&str> = back.lv().rows().iter().map(LvRow::ordinal).collect();
        assert_eq!(
            ordinals,
            [
                "01",
                "01.01",
                "01.01.001",
                "01.01.002",
                "01.02",
                "01.02.01",
                "01.02.01.001"
            ],
            "{file}"
        );
    }
    std::fs::remove_dir_all(&folder).unwrap();

    // With an index in the mask: "02.002.1" below title 02 is position 002,
    // index 1 – not title 02.002.
    let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPI").unwrap());
    lv.push_group(LvGroup::new(0, "02", "Titel"));
    lv.push_position(position("02.002.1", "Flach mit Index", "1", "St", None));
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let items: Vec<String> = doc
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, _)| pieces.join("."))
        .collect();
    assert_eq!(items[0], "02.002.1");
}

#[test]
fn export_warns_about_texts_stlb_and_missing_values() {
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Titel"));
    lv.push_position(position("01.0010", &"x".repeat(71), "1", "St", None));
    let mut stlb = Position::stlb("Mauerwerk");
    stlb.set_ordinal("01.0020");
    lv.push_position(stlb);
    lv.push_position(position("01.0030", "", "0", "", None));
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(has(&warnings, "länger als 70 Zeichen"), "{warnings:?}");
    assert!(has(&warnings, "01.0010 (71)"));
    assert!(has(
        &warnings,
        "STLB-Platzhalter ohne fertigen Text: 01.0020"
    ));
    assert!(has(&warnings, "ohne Kurztext: 01.0030"));
    assert!(has(&warnings, "ohne Menge: 01.0020, 01.0030"));
    assert!(has(&warnings, "ohne Einheit: 01.0020, 01.0030"));
    assert_eq!(doc.award.boq.items()[0].1.short_text.chars().count(), 71);
}

#[test]
fn mask_and_breakdown_convert_both_ways() {
    for (pattern, expected) in [
        (
            "11.22.PPPPI",
            vec![
                (BreakdownKind::Level, 2),
                (BreakdownKind::Level, 2),
                (BreakdownKind::Item, 4),
                (BreakdownKind::Index, 1),
            ],
        ),
        (
            "1.22.33.PPPP",
            vec![
                (BreakdownKind::Level, 1),
                (BreakdownKind::Level, 2),
                (BreakdownKind::Level, 2),
                (BreakdownKind::Item, 4),
            ],
        ),
        (
            "11.PPPP",
            vec![(BreakdownKind::Level, 2), (BreakdownKind::Item, 4)],
        ),
    ] {
        let mask = OzMask::parse(pattern).unwrap();
        let breakdown = breakdown_from_mask(&mask);
        let kinds: Vec<_> = breakdown
            .iter()
            .map(|level| (level.kind, level.length))
            .collect();
        assert_eq!(kinds, expected, "{pattern}");
        assert_eq!(mask_from_breakdown(&breakdown).unwrap().pattern(), pattern);
    }
    let lot = vec![
        gaeb::BreakdownLevel {
            kind: BreakdownKind::Lot,
            label: "Los".into(),
            length: 1,
            numeric: true,
        },
        gaeb::BreakdownLevel {
            kind: BreakdownKind::Level,
            label: "Titel".into(),
            length: 2,
            numeric: false,
        },
        gaeb::BreakdownLevel {
            kind: BreakdownKind::Item,
            label: String::new(),
            length: 3,
            numeric: true,
        },
    ];
    assert_eq!(mask_from_breakdown(&lot).unwrap().pattern(), "1.LL.PPP");
    assert!(mask_from_breakdown(&lot[..2]).is_err());
}

#[test]
fn level_labels_travel_as_breakdown_labels() {
    let mask = OzMask::parse("1.222.33.PPPP").unwrap();
    let labels = vec!["Los".to_string(), "Kostengruppe".to_string()];
    let breakdown = crate::breakdown_with_labels(&mask, &labels);
    assert_eq!(breakdown[0].kind, BreakdownKind::Lot);
    assert_eq!(breakdown[1].label, "Kostengruppe");
    assert_eq!(breakdown[2].label, "Abschnitt");
    assert_eq!(breakdown[2].kind, BreakdownKind::Level);
    assert_eq!(
        crate::labels_from_breakdown(&breakdown),
        vec!["Los", "Kostengruppe", "Abschnitt"]
    );
    assert_eq!(
        mask_from_breakdown(&breakdown).unwrap().pattern(),
        "1.222.33.PPPP"
    );
    let plain = breakdown_from_mask(&mask);
    assert_eq!(plain[0].kind, BreakdownKind::Level);
    assert_eq!(plain[0].label, "Titel");
}

/// Eine oberste Stufe mit der Bezeichnung „LOS“, als BoQLevel geschrieben,
/// bleibt beim Einlesen und Zurückschreiben so (nur genau „Los“ ist ein Lot).
#[test]
fn a_level_called_los_in_capitals_stays_a_level() {
    let mask = OzMask::parse("111.22.PPPP").unwrap();
    let level = crate::breakdown_with_labels(&mask, &["LOS".to_string(), "Titel".to_string()]);
    assert_eq!(level[0].kind, BreakdownKind::Level);
    assert_eq!(level[0].label, "LOS");
    let labels = crate::labels_from_breakdown(&level);
    assert_eq!(labels, ["LOS", "Titel"]);
    let again = crate::breakdown_with_labels(&mask, &labels);
    assert_eq!(again, level);
    let lot = crate::breakdown_with_labels(&mask, &["Los".to_string()]);
    assert_eq!(lot[0].kind, BreakdownKind::Lot);
}

#[test]
fn ordinals_split_along_the_mask() {
    let layout = MaskLayout::of(&OzMask::parse("11.22.PPPPI").unwrap());
    let parts = layout.split_position("01.02.0015.1").unwrap();
    assert_eq!(
        (parts.levels, parts.position, parts.index),
        (
            vec!["01".to_string(), "02".to_string()],
            "0015".to_string(),
            "1".to_string()
        )
    );
    assert_eq!(layout.split_position("01.02.0015").unwrap().index, "");
    assert!(layout.split_position("01.0015").is_none());
}

#[test]
fn steps_and_deadlines() {
    assert_eq!(detect_step(&[10, 20, 30]), 10);
    assert_eq!(detect_step(&[100, 300]), 100);
    assert_eq!(detect_step(&[1000]), 1000);
    assert_eq!(detect_step(&[1, 2, 3]), 1);
    assert_eq!(detect_step(&[10, 25]), 1);
    assert_eq!(detect_step(&[]), 10);
    assert_eq!(
        parse_deadline("15.12.2026, 10:00 Uhr"),
        (Some("2026-12-15".into()), Some("10:00:00".into()))
    );
    assert_eq!(
        parse_deadline("2026-12-15"),
        (Some("2026-12-15".into()), None)
    );
    assert_eq!(parse_deadline("nach Absprache"), (None, None));
    assert_eq!(
        format_deadline(Some("2026-12-15"), Some("10:00:00")),
        "15.12.2026, 10:00 Uhr"
    );
}

/// A bidder's X84 built from our own X82: prices changed, one position
/// left out, one unknown added, one arithmetic error, one changed quantity,
/// numbers written without leading zeros, index left away, 3 % discount.
fn bidder_file(project: &Project) -> GaebDocument {
    let (mut doc, _) = lv_to_gaeb(project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    doc.project.name = "FW 3 Bieterkopie".into();
    doc.award.bidder = Some(gaeb::Address {
        name1: "Muster Automation GmbH".into(),
        city: "Essen".into(),
        ..gaeb::Address::default()
    });
    doc.award.bid_date = Some("2026-12-14".into());

    let BoqNode::Category(title) = &mut doc.award.boq.body[1] else {
        panic!()
    };
    title.rno_part = "1".into();
    let BoqNode::Category(asp01) = &mut title.body[1] else {
        panic!()
    };
    asp01
        .body
        .retain(|node| !matches!(node, BoqNode::Item(item) if item.rno_part == "0060"));
    asp01.body.push(BoqNode::Item(Item {
        rno_part: "0099".into(),
        unit_price: Some(dec("5")),
        qty: Some(dec("1")),
        ..Item::default()
    }));

    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.rno_part == "0070" {
            item.rno_index.clear();
        }
        if item.rno_part == "0010" {
            item.rno_part = "10".into();
        }
    });
    for_items(&mut doc.award.boq.body, &mut |item| {
        let price = dec("10");
        item.unit_price = Some(price);
        item.total = item.qty.map(|qty| (qty * price).round_dp(2));
    });
    let BoqNode::Category(title) = &mut doc.award.boq.body[1] else {
        panic!()
    };
    let BoqNode::Category(asp01) = &mut title.body[1] else {
        panic!()
    };
    let BoqNode::Item(item) = &mut asp01.body[1] else {
        panic!()
    };
    item.total = Some(dec("999"));
    let BoqNode::Category(asp10) = &mut title.body[2] else {
        panic!()
    };
    let BoqNode::Item(item) = &mut asp10.body[0] else {
        panic!()
    };
    item.qty = Some(dec("2"));
    item.total = Some(dec("20"));

    doc.award.boq.totals = Some(gaeb::Totals {
        total: Some(dec("1")),
        discount_percent: Some(dec("3")),
        ..gaeb::Totals::default()
    });
    doc
}

#[test]
fn the_short_name_from_our_own_x83_is_no_other_project() {
    let mut project = demo_project();
    project.info_mut().short_name = "QA-2026".into();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    assert_eq!(doc.project.name, "QA-2026", "X83 trägt den Kurznamen");
    let import = match_bid(&project, &doc, "a.x84");
    assert!(
        !import
            .warnings
            .iter()
            .any(|w| w.contains("Projektname in der Datei")),
        "{:?}",
        import.warnings
    );
}

#[test]
fn bid_is_matched_with_findings() {
    let project = demo_project();
    let doc = bidder_file(&project);
    let import = match_bid(&project, &doc, "muster.x84");
    let bid = &import.bid;
    let warnings = &import.warnings;

    assert_eq!(bid.name(), "Muster Automation GmbH");
    assert_eq!(bid.bid_date(), Some("2026-12-14"));
    assert_eq!(bid.source(), "muster.x84");
    assert_eq!(bid.prices().len(), project.lv().position_count() - 1);
    assert_eq!(bid.unknown_ordinals(), ["1.01.0099"]);
    assert_eq!(bid.discount(), Discount::Percent(dec("3")));

    let hourly = project.lv().position_by_ordinal("01.01.0070.1").unwrap();
    assert_eq!(bid.unit_price(hourly.id()), Some(dec("10")));
    let first = project.lv().position_by_ordinal("01.01.0010").unwrap();
    assert_eq!(bid.unit_price(first.id()), Some(dec("10")));

    assert!(
        has(warnings, "Positionen ohne Preis im Angebot (1): 01.01.0060"),
        "{warnings:?}"
    );
    assert!(
        has(
            warnings,
            "nicht im LV: Angebot 1.01.0099 – ohne Text, EP 10"
        ),
        "{warnings:?}"
    );
    assert!(has(
        warnings,
        "Strukturabweichungen: 1 Position nicht im LV, 1 LV-Position fehlt, 1 Menge"
    ));
    assert!(has(warnings, "Ohne Index zugeordnet (1): 1.01.0070"));
    assert!(has(
        warnings,
        "Rechenfehler GP ≠ Menge × EP (1): 01.01.0020"
    ));
    assert!(has(
        warnings,
        "Menge: 01.10.0010 – LV 1, Angebot 2 – gewertet mit der LV-Menge"
    ));
    assert_eq!(bid.structure().quantities.len(), 1);
    assert!(
        bid.structure().conflicts.is_empty(),
        "abweichende Menge ist kein OZ-Konflikt"
    );
    assert!(has(warnings, "Projektname in der Datei"));
    assert!(has(warnings, "Angebotssumme der Datei 1,00 €"));
    assert!(!has(warnings, "kein Angebot"));
    assert!(!has(
        warnings,
        "Rechenfehler GP ≠ Menge × EP (1): 01.10.0010"
    ));
}

#[test]
fn priced_lv_read_as_bid_is_flagged_but_used() {
    let project = demo_project();
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    let import = match_bid(&project, &doc, "planer.x82");
    assert!(has(&import.warnings, "Phase 82, kein Angebot"));
    assert!(has(&import.warnings, "keinen Bieter"));
    assert_eq!(import.bid.name(), "Bieter aus planer.x82");
    assert!(!has(&import.warnings, "Rechenfehler"));
    assert!(
        !has(&import.warnings, "Angebotssumme"),
        "{:?}",
        import.warnings
    );
    assert_eq!(import.bid.net_total(project.lv()), project.lv().net_total());
    assert!(has(
        &import.warnings,
        "Positionen ohne Preis im Angebot (1): 02.01.0010"
    ));
}

#[test]
fn discount_as_amount_from_totals() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp84);
    let net = project.lv().net_total().to_decimal();
    doc.award.boq.totals = Some(gaeb::Totals {
        total: Some(net),
        total_after_discount: Some(net - dec("500")),
        ..gaeb::Totals::default()
    });
    let import = match_bid(&project, &doc, "a.x84");
    assert_eq!(
        import.bid.discount(),
        Discount::Amount(Money::from_cents(50000))
    );
    assert!(
        !has(&import.warnings, "Summe nach Nachlass"),
        "{:?}",
        import.warnings
    );
}

#[test]
fn file_formats_and_xml_through_the_port() {
    assert_eq!(GaebFormat::of(Path::new("a.X83")), Ok(GaebFormat::Xml));
    assert_eq!(GaebFormat::of(Path::new("a.xml")), Ok(GaebFormat::Xml));
    assert_eq!(GaebFormat::of(Path::new("b.d84")), Ok(GaebFormat::Datan));
    assert!(GaebFormat::of(Path::new("c.P83"))
        .unwrap_err()
        .contains("GAEB 2000"));
    assert!(GaebFormat::of(Path::new("d.pdf")).is_err());

    let exchange = GaebPdfExchange;
    let project = demo_project();
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-xml-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let target = folder.join("Angebotsaufforderung.X83");
    let warnings = exchange
        .export_gaeb(
            &project,
            proava_application::GaebExport::RequestForQuotation,
            &target,
        )
        .unwrap();
    assert!(has(&warnings, "Bild"), "{warnings:?}");
    let written = std::fs::read_to_string(&target).unwrap();
    assert!(written.contains("DA83/3.3"));
    assert!(!written.contains("<UP>"));

    let priced = folder.join("Preisanschlag.X82");
    exchange
        .export_gaeb(&project, proava_application::GaebExport::PricedLv, &priced)
        .unwrap();
    let (back, _) = exchange
        .import_lv(&priced, &ProjectName::parse("Zurück").unwrap(), "")
        .unwrap();
    assert_eq!(back.lv().net_total(), project.lv().net_total());
    assert_eq!(back.lv().position_count(), project.lv().position_count());

    let import = exchange.import_bid(&project, &priced).unwrap();
    assert_eq!(import.bid.net_total(project.lv()), project.lv().net_total());
    std::fs::remove_dir_all(&folder).ok();
    let error = exchange
        .import_bid(&project, Path::new("/nicht/da.x84"))
        .unwrap_err();
    assert!(error.contains("nicht lesen"), "{error}");
}

fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[200, 30, 30, 30, 200, 30, 30, 30, 200, 90, 90, 90])
            .unwrap();
    }
    out
}

fn project_with_image_and_bids() -> Project {
    let mut project = demo_project();
    let file = project.add_image("schema.png", tiny_png());
    assert_eq!(file, "schema.png");
    let doc = bidder_file(&project);
    let first = match_bid(&project, &doc, "muster.x84").bid;
    project.add_bid(first);
    let mut second = proava_domain::Bid::new(Address::named("Zweite GmbH"), "zweite.x84");
    let ids: Vec<_> = project
        .lv()
        .positions()
        .map(|p| (p.id(), p.unit_price()))
        .collect();
    for (id, price) in ids {
        second.set_price(
            id,
            proava_domain::BidPrice {
                unit_price: Some(price.unwrap_or(dec("7")) + dec("1.111")),
                ..proava_domain::BidPrice::default()
            },
        );
    }
    second.set_discount(Discount::Amount(Money::from_cents(12345)));
    project.add_bid(second);
    let mut excluded = proava_domain::Bid::new(Address::named("Ausgeschlossen"), "x.x84");
    excluded.set_excluded(true);
    project.add_bid(excluded);
    project
}

fn print_titles(
    items: &[proava_print::PrintItem],
    out: &mut Vec<(String, Option<Decimal>)>,
) -> Decimal {
    let mut sum = Decimal::ZERO;
    for item in items {
        match item {
            proava_print::PrintItem::Title(title) => {
                out.push((title.oz.clone(), title.total));
                let computed = print_titles(&title.items, out);
                assert_eq!(
                    title.total.unwrap_or(computed),
                    computed,
                    "Titel {}",
                    title.oz
                );
                sum += computed;
            }
            proava_print::PrintItem::Position(position) if position.in_total => {
                sum += position.effective_total().unwrap_or_default();
            }
            _ => {}
        }
    }
    sum
}

fn has_image(items: &[proava_print::PrintItem]) -> bool {
    items.iter().any(|item| match item {
        proava_print::PrintItem::Title(title) => has_image(&title.items),
        proava_print::PrintItem::Position(position) => position.long_text.iter().any(
            |block| matches!(block, proava_print::TextBlock::Image(image) if image.data == tiny_png() && image.width_mm == Some(80.0)),
        ),
        proava_print::PrintItem::Note(_)
        | proava_print::PrintItem::Preliminary(_)
        | proava_print::PrintItem::Closing(_)
        | proava_print::PrintItem::Performance(..) => false,
    })
}

#[test]
fn print_model_of_priced_lv_has_domain_sums_and_the_image() {
    use proava_print::LvPrintMode;
    let project = project_with_image_and_bids();
    let print = crate::lv_to_print(&project, LvPrintMode::Priced, "17.11.2026");
    assert_eq!(print.header.project_name, "RH3");
    assert_eq!(print.header.project_title, "Rathaus 3 Musterstadt");
    assert_eq!(
        print.header.client,
        "Stadt Düsseldorf, Marktplatz 1, 40213 Düsseldorf"
    );
    assert_eq!(print.header.lv_name, "GA Gebäudeautomation");
    assert_eq!(print.net_total, Some(project.lv().net_total().to_decimal()));
    assert!(has_image(&print.items));

    let mut titles = Vec::new();
    let computed = print_titles(&print.items, &mut titles);
    assert_eq!(computed, project.lv().net_total().to_decimal());
    let expected: Vec<(String, Option<Decimal>)> = project
        .lv()
        .group_totals()
        .iter()
        .map(|total| {
            (
                project.lv().rows()[total.group_row].ordinal().to_string(),
                Some(total.total.to_decimal()),
            )
        })
        .collect();
    assert_eq!(titles, expected);
    assert!(matches!(print.items[0], proava_print::PrintItem::Note(_)));

    let blank = crate::lv_to_print(&project, LvPrintMode::Blank, "17.11.2026");
    assert_eq!(blank.net_total, None);
    let mut blank_titles = Vec::new();
    print_titles(&blank.items, &mut blank_titles);
    assert!(blank_titles.iter().all(|(_, total)| total.is_none()));
}

#[test]
fn print_model_of_price_comparison_matches_the_domain() {
    let project = project_with_image_and_bids();
    let print = crate::price_comparison_to_print(&project, "17.11.2026");
    let comparison = proava_domain::PriceComparison::build(
        project.lv(),
        project.bids(),
        proava_domain::ComparisonOptions::default(),
    );
    let names: Vec<_> = print.bidders.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["Muster Automation GmbH", "Zweite GmbH"]);
    assert_eq!(print.bidders[0].date, "14.12.2026");
    assert_eq!(print.bidders[0].note, "");
    assert_eq!(print.bidders[0].discount_percent, Some(dec("3")));
    assert_eq!(print.bidders[1].discount_amount, Some(dec("123.45")));
    for (bidder, summary) in print.bidders.iter().zip(&comparison.bids) {
        assert_eq!(bidder.net_total, Some(summary.net.to_decimal()));
    }

    fn walk(
        items: &[proava_print::PcItem],
        titles: &mut Vec<Vec<Option<Decimal>>>,
        positions: &mut usize,
    ) {
        for item in items {
            match item {
                proava_print::PcItem::Title(title) => {
                    titles.push(title.totals.clone());
                    walk(&title.items, titles, positions);
                }
                proava_print::PcItem::Position(position) => {
                    assert_eq!(position.prices.len(), 2);
                    *positions += 1;
                }
            }
        }
    }
    let mut titles = Vec::new();
    let mut positions = 0;
    walk(&print.items, &mut titles, &mut positions);
    assert_eq!(positions, project.lv().position_count());
    let expected: Vec<Vec<Option<Decimal>>> = comparison
        .lines
        .iter()
        .filter_map(|line| match line {
            proava_domain::ComparisonLine::Group(group) => Some(
                group
                    .totals
                    .iter()
                    .map(|total| Some(total.to_decimal()))
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    assert_eq!(titles, expected);
}

/// Preisspiegel ohne Planerpreise: keine Spalte „Planerpreis“; was ein
/// Angebot außerhalb des LV enthält und Rechenfehler gehen mit in den Druck
/// (Hinweise unter den Angebotssummen), die Summen bleiben.
#[test]
fn price_comparison_print_leaves_out_missing_planner_prices_and_carries_bid_remarks() {
    let mut project = project_with_image_and_bids();
    assert!(project.lv().has_planner_prices());
    assert!(crate::price_comparison_to_print(&project, "20.11.2026").estimate);

    // Rechenfehler und unbekannte OZ beim zweiten Bieter.
    let (first, quantity, price) = project
        .lv()
        .positions()
        .find(|p| !p.quantity().is_zero() && p.kind().counts_in_total())
        .map(|p| {
            (
                p.id(),
                p.quantity(),
                p.unit_price().unwrap_or(dec("7")) + dec("1.111"),
            )
        })
        .unwrap();
    let second = project.bids()[1].id();
    let bid = project.bid_mut(second).unwrap();
    bid.set_price(
        first,
        proava_domain::BidPrice {
            unit_price: Some(price),
            stated_total: Some(quantity * price + dec("100")),
            ..proava_domain::BidPrice::default()
        },
    );
    bid.set_unknown_ordinals(vec!["99.99.0010".into()]);
    let before = crate::price_comparison_to_print(&project, "20.11.2026");
    let remarks = &before.bidders[1];
    assert_eq!(remarks.unknown_ordinals, vec!["99.99.0010".to_string()]);
    assert_eq!(remarks.arithmetic_errors.len(), 1);
    let ordinal = project
        .lv()
        .position_by_id(first)
        .unwrap()
        .ordinal()
        .to_string();
    assert_eq!(remarks.arithmetic_errors[0].oz, ordinal);
    assert_eq!(
        remarks.arithmetic_errors[0].stated - remarks.arithmetic_errors[0].computed,
        dec("100")
    );
    // The first bid came from a file: its unknown OZ stand in the
    // Abweichungsprotokoll, not twice.
    assert!(before.bidders[0].unknown_ordinals.is_empty());
    let structure = before.bidders[0]
        .structure
        .as_ref()
        .expect("Abweichungsprotokoll");
    assert!(
        structure.summary.contains("Position nicht im LV"),
        "{}",
        structure.summary
    );
    assert!(structure
        .lines
        .iter()
        .any(|line| line.contains(&project.bids()[0].unknown_ordinals()[0])));

    // Planerpreise weg: keine Spalte, Summen der Bieter unverändert.
    let rows = project.lv().rows().len();
    for index in 0..rows {
        if let Some(proava_domain::LvRow::Position(position)) = project.lv_mut().row_mut(index) {
            position.set_unit_price(None);
        }
    }
    assert!(!project.lv().has_planner_prices());
    let after = crate::price_comparison_to_print(&project, "20.11.2026");
    assert!(!after.estimate, "keine Spalte Planerpreis");
    assert_eq!(after.estimate_net, None);
    let nets = |print: &proava_print::PrintPriceComparison| -> Vec<_> {
        print
            .bidders
            .iter()
            .map(|bidder| bidder.net_total)
            .collect()
    };
    assert_eq!(nets(&after), nets(&before));
}

#[test]
fn pdfs_are_written_from_a_domain_project() {
    let project = project_with_image_and_bids();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-pdf-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for (document, name) in [
        (
            proava_application::PdfDocument::RequestForQuotation,
            "aufforderung.pdf",
        ),
        (proava_application::PdfDocument::PricedLv, "bepreist.pdf"),
        (
            proava_application::PdfDocument::PriceComparison,
            "preisspiegel.pdf",
        ),
    ] {
        let file = folder.join(name);
        exchange.export_pdf(&project, document, &file).unwrap();
        let bytes = std::fs::read(&file).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "{name}");
    }
    let without_bids = demo_project();
    let error = exchange
        .export_pdf(
            &without_bids,
            proava_application::PdfDocument::PriceComparison,
            &folder.join("leer.pdf"),
        )
        .unwrap_err();
    assert!(error.contains("Angebot"), "{error}");
    std::fs::remove_dir_all(&folder).unwrap();
}

/// What a D84 looks like: no quantities, one item with GP only, one
/// Zuschlagsposition with percent, a discount amount in the totals.
#[test]
fn d84_style_bid_without_quantities() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp84);
    for_items(&mut doc.award.boq.body, &mut |item| {
        item.qty = None;
        item.unit = String::new();
        match item.rno_part.as_str() {
            "0040" => {
                item.unit_price = None;
                item.total = Some(dec("2900"));
            }
            "0060" => {
                item.unit_price = None;
                item.total = None;
                item.markup_percent = Some(dec("4.5"));
            }
            "0030" => item.discount_percent = Some(dec("2")),
            _ => {}
        }
    });
    doc.award.boq.totals = Some(gaeb::Totals {
        discount_amount: Some(dec("250")),
        ..gaeb::Totals::default()
    });
    let import = match_bid(&project, &doc, "a.d84");
    let warnings = &import.warnings;
    assert!(!has(warnings, "Menge abweichend"), "{warnings:?}");
    assert!(!has(warnings, "Einheit abweichend"));
    assert!(!has(warnings, "Rechenfehler"));
    assert!(has(warnings, "EP = GP ÷ Menge übernommen (1): 01.01.0040"));
    assert!(has(
        warnings,
        "keine Zuschlagsposition ist (nicht gewertet, bitte prüfen) (1): 01.01.0060 (4,5 %)"
    ));
    assert!(has(
        warnings,
        "Nachlass auf einzelne Positionen (nicht übernommen, bitte prüfen) (1): 01.01.0030 (2 %)"
    ));
    assert!(has(warnings, "ohne Preis im Angebot (1): 02.01.0010"));
    let base = project.lv().position_by_ordinal("01.01.0040").unwrap();
    assert_eq!(import.bid.unit_price(base.id()), Some(dec("2900")));
    assert_eq!(
        import.bid.discount(),
        Discount::Amount(Money::from_cents(25000))
    );
}

#[test]
fn lump_sum_offer_becomes_a_discount() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp84);
    let net = project.lv().net_total();
    doc.award.boq.totals = Some(gaeb::Totals {
        total: Some(net.to_decimal()),
        lump_sum: Some(dec("7000")),
        ..gaeb::Totals::default()
    });
    let import = match_bid(&project, &doc, "a.d84");
    assert_eq!(
        import.bid.discount(),
        Discount::Amount(net - Money::from_cents(700000))
    );
    assert!(has(&import.warnings, "Pauschalsumme 7.000,00 €"));
}

#[test]
fn sub_descriptions_contract_text_and_performance_descriptions_are_kept() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award.award_no = "VE 0420".into();
    doc.award.contract_text = vec!["Es gilt die VOB/B.".into()];
    let BoqNode::Category(title) = &mut doc.award.boq.body[1] else {
        panic!()
    };
    title.body.insert(
        0,
        BoqNode::PerfDescr(gaeb::PerfDescr {
            number: "3".into(),
            label: "Kabel".into(),
            short_text: String::new(),
            long_text: gaeb::RichText::plain("Alle Kabel halogenfrei."),
            ..gaeb::PerfDescr::default()
        }),
    );
    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.rno_part == "0020" {
            item.sub_descriptions.push(gaeb::SubDescription {
                number: "1".into(),
                qty: Some(dec("2")),
                unit: "St".into(),
                short_text: "Variante A".into(),
                long_text: gaeb::RichText::plain("Text A"),
                ..gaeb::SubDescription::default()
            });
        }
    });
    let (back, warnings) = gaeb_to_project(&doc, ProjectName::parse("X").unwrap(), "");
    assert_eq!(back.info().award_number, "VE 0420");
    assert!(has(&warnings, "Vertragliche Regelungen"), "{warnings:?}");
    let notes: Vec<String> = back
        .lv()
        .rows()
        .iter()
        .filter_map(|row| match row {
            LvRow::Note(note) => Some(format!("{}|{}", note.heading(), note.text().to_plain())),
            _ => None,
        })
        .collect();
    assert_eq!(notes[0], "Vertragliche Regelungen|Es gilt die VOB/B.");
    assert!(!notes
        .iter()
        .any(|note| note.contains("Ausführungsbeschreibung")));
    let performance = back.lv().performance_by_number("3").unwrap();
    assert_eq!(performance.label(), "Kabel");
    let position = back.lv().position_by_ordinal("01.01.0020").unwrap();
    let sub = &position.sub_descriptions()[0];
    assert_eq!(
        (
            sub.number.as_str(),
            sub.short_text.as_str(),
            sub.unit.as_str()
        ),
        ("1", "Variante A", "St")
    );
    assert_eq!(sub.quantity, Some(dec("2")));
    assert_eq!(sub.long_text.to_plain(), "Text A");
    assert!(!position
        .long_text()
        .to_plain()
        .contains("Unterbeschreibung"));
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    let item = again
        .award
        .boq
        .items()
        .into_iter()
        .find(|(pieces, _)| pieces.join(".") == "01.01.0020")
        .unwrap()
        .1
        .clone();
    assert_eq!(item.sub_descriptions.len(), 1);
    assert_eq!(item.sub_descriptions[0].short_text, "Variante A");
}

#[test]
fn datan_files_go_out_and_come_back() {
    let project = demo_project();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-datan-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();

    let file = folder.join("lv.D83");
    exchange
        .export_gaeb(
            &project,
            proava_application::GaebExport::RequestForQuotation,
            &file,
        )
        .unwrap();
    let (back, _) = exchange
        .import_lv(&file, &ProjectName::parse("RH3").unwrap(), "")
        .unwrap();
    let outline = |lv: &Lv| -> Vec<String> {
        lv.rows()
            .iter()
            .filter(|row| !row.is_note())
            .map(|row| format!("{} {}", row.ordinal(), row.short_text()))
            .collect()
    };
    assert_eq!(outline(back.lv()), outline(project.lv()));
    let kinds: Vec<_> = back.lv().positions().map(|p| p.kind()).collect();
    let expected: Vec<_> = project.lv().positions().map(|p| p.kind()).collect();
    assert_eq!(kinds, expected);
    assert_eq!(back.lv().oz_mask().pattern(), "11.22.PPPPI");

    let bid_file = folder.join("bieter.D84");
    let doc = bidder_file(&project);
    std::fs::write(&bid_file, proava_gaeb::datan::write(&doc).unwrap()).unwrap();
    let import = exchange.import_bid(&project, &bid_file).unwrap();
    assert!(
        import.bid.prices().len() >= project.lv().position_count() - 2,
        "{:?}",
        import.warnings
    );
    assert!(
        has(&import.warnings, "nicht im LV: Angebot 01.01.0099"),
        "{:?}",
        import.warnings
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn zulage_is_an_ordinary_item_and_a_percentage_makes_a_zuschlag() {
    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0060")
        .unwrap();
    project
        .lv_mut()
        .row_mut(index)
        .and_then(|row| row.as_position_mut())
        .unwrap()
        .set_kind(PositionKind::Supplement);
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    let mut found = false;
    fn mark(nodes: &mut [proava_gaeb::BoqNode], found: &mut bool) {
        for node in nodes {
            match node {
                proava_gaeb::BoqNode::Category(category) => mark(&mut category.body, found),
                proava_gaeb::BoqNode::Item(item) if item.rno_part == "0060" => {
                    // A Zulage is no Zuschlag: no MarkupIt, no percentage.
                    assert!(!item.kind.markup);
                    assert!(item.surcharge.is_none() && item.markup_percent.is_none());
                    // A percentage (X84 short form) makes it a Zuschlagsposition.
                    item.markup_percent = Some("5".parse().unwrap());
                    *found = true;
                }
                _ => {}
            }
        }
    }
    mark(&mut doc.award.boq.body, &mut found);
    assert!(found);
    let (back, warnings) = gaeb_to_project(&doc, ProjectName::parse("RH3").unwrap(), "");
    assert!(!has(&warnings, "Zuschlagspositionen"), "{warnings:?}");
    let position = back.lv().position_by_ordinal("01.01.0060").unwrap();
    assert!(position.is_surcharge());
    assert_eq!(position.surcharge_percent(), Some(dec("5")));
}

/// Title 01 with positions, one marked „zu bezuschlagen“, and the three kinds
/// of Zuschlag: on all positions of the list, on the marked ones, on listed
/// positions (one with a part quantity).
fn project_with_surcharges() -> Project {
    use proava_domain::{Surcharge, SurchargeKind, SurchargeReference};
    let zuschlag = |ordinal: &str, text: &str, surcharge: Surcharge, percent: &str| {
        let mut position = position(ordinal, text, "0", "", Some(percent));
        position.set_surcharge(Some(surcharge));
        position
    };
    let mask = OzMask::parse("11.22.PPPPI").unwrap();
    let mut lv = Lv::new("Zuschläge", mask);
    lv.push_group(LvGroup::new(0, "01", "Rohbau"));
    lv.push_group(LvGroup::new(1, "01.01", "Erdarbeiten"));
    lv.push_position(position("01.01.0010", "Aushub", "100", "m3", Some("12.50")));
    let mut marked = position("01.01.0020", "Verfüllen", "40", "m3", Some("8"));
    marked.set_marked_for_surcharge(true);
    lv.push_position(marked);
    let mut all = zuschlag(
        "01.01.0030",
        "Zuschlag Erschwernis",
        Surcharge::default(),
        "5",
    );
    all.set_long_text(RichText::plain("Zuschlag auf alle Positionen des Titels"));
    lv.push_position(all);
    lv.push_group(LvGroup::new(1, "01.02", "Mauerarbeiten"));
    lv.push_position(position("01.02.0010", "Mauerwerk", "20", "m2", Some("60")));
    let mut wall = position("01.02.0020", "Sturz", "4", "St", Some("35"));
    wall.set_marked_for_surcharge(true);
    lv.push_position(wall);
    lv.push_position(zuschlag(
        "01.02.0030",
        "Zuschlag Winterbau",
        Surcharge {
            split: false,
            kind: SurchargeKind::Marked,
            references: Vec::new(),
        },
        "10",
    ));
    let first = lv.position_by_ordinal("01.01.0010").unwrap().id();
    let wall = lv.position_by_ordinal("01.02.0010").unwrap().id();
    lv.push_position(zuschlag(
        "01.02.0040",
        "Zuschlag Kleinmengen",
        Surcharge {
            split: false,
            kind: SurchargeKind::Listed,
            references: vec![
                SurchargeReference {
                    position: first,
                    quantity: None,
                },
                SurchargeReference {
                    position: wall,
                    quantity: Some(dec("5")),
                },
            ],
        },
        "2.5",
    ));
    Project::new(
        ProjectName::parse("ZS1").unwrap(),
        "2026-11-18T12:00:00",
        lv,
    )
}

/// Bedarfsträger, Ausführungs- und Benachrichtigungsorte, Debitorennummer
/// und Staat des Auftraggebers, DPNo des Bieters: wie geliefert erhalten.
#[test]
fn places_and_party_numbers_survive_import_and_export() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let address = |name: &str| gaeb::Address {
        name1: name.into(),
        street: "Weg 1".into(),
        postal_code: "40210".into(),
        city: "Düsseldorf".into(),
        ..gaeb::Address::default()
    };
    doc.award.requesters = vec![gaeb::Place {
        address: Some(address("Schulamt")),
        id_no: "R-1".into(),
        name: "Schule".into(),
    }];
    doc.award.sites = vec![
        gaeb::Site {
            site: gaeb::Place {
                address: Some(address("Schulzentrum")),
                id_no: "B-1".into(),
                name: "Haus A".into(),
            },
            notification: Some(gaeb::Place {
                address: None,
                id_no: "N-1".into(),
                name: "Bauleitung".into(),
            }),
        },
        gaeb::Site {
            site: gaeb::Place {
                address: None,
                id_no: "B-2".into(),
                name: "Haus B".into(),
            },
            notification: None,
        },
    ];
    doc.award.owner_account = "D-4711".into();
    doc.award.owner_country = gaeb::Country {
        kind: "EEA".into(),
        name: "D".into(),
    };
    let text = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let read = gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, warnings) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert!(
        !warnings.iter().any(|w| w.contains("übergangen")),
        "{warnings:?}"
    );
    let info = back.info();
    assert_eq!(info.site.name1, "Schulzentrum");
    assert_eq!(
        (info.site_number.as_str(), info.site_name.as_str()),
        ("B-1", "Haus A")
    );
    assert_eq!(info.notification_site.as_ref().unwrap().name, "Bauleitung");
    assert_eq!(info.further_sites.len(), 1);
    assert_eq!(info.further_sites[0].site.number, "B-2");
    assert_eq!(info.requesters[0].address.name1, "Schulamt");
    assert_eq!(info.client_account, "D-4711");
    assert_eq!(
        (
            info.client_country_kind.as_str(),
            info.client_country_code.as_str()
        ),
        ("EEA", "D")
    );
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    assert_eq!(again.award.requesters, read.award.requesters);
    assert_eq!(again.award.sites, read.award.sites);
    assert_eq!(again.award.owner_account, "D-4711");
    assert_eq!(again.award.owner_country, read.award.owner_country);
    let written = gaeb::xml::write(&again, Phase::Dp83).unwrap();
    assert!(gaeb::rules::check_xml(written.as_bytes()).is_empty());

    // Bid: DPNo of the bidder (CTR).
    let mut bid_doc = bidder_file(&project);
    bid_doc.award.bidder_dp_no = "AN-17".into();
    let import = match_bid(&project, &bid_doc, "a.x84");
    assert_eq!(import.bid.exchange_number(), "AN-17");
}

/// Projektangaben, die die Eingabe nicht annimmt (Ausführungsende vor dem
/// Beginn), übernimmt der Import wie geliefert und sagt es.
#[test]
fn implausible_dates_are_taken_over_as_delivered_and_named() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award.submission.construction_start = Some("2020-12-18".into());
    doc.award.submission.construction_end = Some("2020-01-31".into());
    let (back, warnings) = gaeb_to_project(&doc, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(back.info().execution_start, "18.12.2020");
    assert_eq!(back.info().execution_end, "31.01.2020");
    assert!(
        warnings.iter().any(|warning| warning.contains(
            "Das Ausführungsende (31.01.2020) liegt vor dem Ausführungsbeginn (18.12.2020) – wie aus der Datei übernommen"
        )),
        "{warnings:?}"
    );
}

/// Pauschalsumme eines Titels (TotalLSUM in BoQCtgy): Titelsumme, die
/// Summen darüber rechnen mit ihr; Export und Import, Druck.
#[test]
fn lump_sum_of_a_title_goes_out_counts_above_and_comes_back() {
    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01")
        .unwrap();
    let positions = project.lv().block_positions_total(index);
    let before = project.lv().net_total();
    if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
        group.set_lump_sum_total(Some(Money::from_cents(100_000)));
    }
    let lv_total = project.lv().net_total();
    assert_eq!(lv_total, before - positions + Money::from_cents(100_000));

    let (doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    fn find_category<'a>(nodes: &'a [BoqNode], label: &str) -> Option<&'a gaeb::Category> {
        nodes.iter().find_map(|node| match node {
            BoqNode::Category(category) if category.label.contains(label) => Some(category),
            BoqNode::Category(category) => find_category(&category.body, label),
            _ => None,
        })
    }
    let asp01 = find_category(&doc.award.boq.body, "ASP01").unwrap();
    let totals = asp01.totals.clone().unwrap();
    assert_eq!(totals.total, Some(positions.to_decimal()));
    assert_eq!(totals.lump_sum, Some(dec("1000")));
    assert_eq!(
        doc.award.boq.totals.as_ref().unwrap().total,
        Some(lv_total.to_decimal())
    );
    let written = gaeb::xml::write(&doc, Phase::Dp82).unwrap();
    assert!(written.contains("<TotalLSUM>1000.00</TotalLSUM>"));
    let read = gaeb::xml::read(written.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    let group = back
        .lv()
        .rows()
        .iter()
        .find(|row| row.ordinal() == "01.01")
        .and_then(LvRow::as_group)
        .unwrap();
    assert_eq!(group.lump_sum_total(), Some(Money::from_cents(100_000)));
    assert_eq!(back.lv().net_total(), lv_total);

    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Priced, "18.11.2026");
    fn find(items: &[proava_print::PrintItem]) -> Option<&proava_print::PrintTitle> {
        items.iter().find_map(|item| match item {
            proava_print::PrintItem::Title(title) if title.oz == "01.01" => Some(title),
            proava_print::PrintItem::Title(title) => find(&title.items),
            _ => None,
        })
    }
    let title = find(&print.items).unwrap();
    assert!(title.lump);
    assert_eq!(title.total, Some(dec("1000")));
    assert_eq!(print.net_total, Some(lv_total.to_decimal()));
}

#[test]
fn lump_sum_of_the_lv_goes_out_and_comes_back() {
    let mut project = demo_project();
    project
        .lv_mut()
        .set_lump_sum_total(Some(Money::from_cents(100_000)));
    let positions = project.lv().positions_total();
    assert_ne!(positions, Money::from_cents(100_000));
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    let totals = doc.award.boq.totals.clone().unwrap();
    assert_eq!(totals.lump_sum, Some(dec("1000")));
    assert_eq!(totals.total, Some(positions.to_decimal()));
    assert_eq!(totals.total_gross, Some(dec("1190")));
    let written = gaeb::xml::write(&doc, Phase::Dp82).unwrap();
    assert!(
        written.contains("<TotalLSUM>1000.00</TotalLSUM>"),
        "{written}"
    );
    let findings = gaeb::rules::check_xml(written.as_bytes());
    assert!(
        !findings
            .iter()
            .any(|finding| finding.text.contains("Summe") || finding.text.contains("TotalLSUM")),
        "{findings:?}"
    );
    let read = gaeb::xml::read(written.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(back.lv().lump_sum_total(), Some(Money::from_cents(100_000)));
    assert_eq!(back.lv().net_total(), Money::from_cents(100_000));
    assert_eq!(back.lv().positions_total(), positions);
    // Without prices there is no Pauschalsumme.
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(doc
        .award
        .boq
        .totals
        .as_ref()
        .is_none_or(|totals| totals.lump_sum.is_none()));
}

#[test]
fn zuschlaege_count_in_the_lv_total() {
    let project = project_with_surcharges();
    let lv = project.lv();
    let values = lv.surcharge_values();
    let value = |ordinal: &str| values[&lv.position_by_ordinal(ordinal).unwrap().id()];
    // 100 × 12,50 + 40 × 8 = 1.570,00; 5 % = 78,50
    assert_eq!(value("01.01.0030").base, Money::from_cents(157_000));
    assert_eq!(value("01.01.0030").amount, Money::from_cents(7_850));
    // marked positions of its own list (title 01.02): 4 × 35 = 140,00;
    // 10 % = 14,00 (01.01.0020 is marked too, but in another list)
    assert_eq!(value("01.02.0030").base, Money::from_cents(14_000));
    assert_eq!(value("01.02.0030").amount, Money::from_cents(1_400));
    // listed: 1.250,00 + 5 × 60 = 1.550,00; 2,5 % = 38,75
    assert_eq!(value("01.02.0040").base, Money::from_cents(155_000));
    assert_eq!(value("01.02.0040").amount, Money::from_cents(3_875));
    // positions 1.570 + 1.340 = 2.910,00 plus 131,25 Zuschläge
    assert_eq!(lv.net_total(), Money::from_cents(304_125));
}

#[test]
fn zuschlaege_go_out_as_markup_items_and_come_back() {
    let project = project_with_surcharges();
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp82);
    assert!(!has(&warnings, "ohne Menge"), "{warnings:?}");
    assert!(!has(&warnings, "ohne Einheit"), "{warnings:?}");
    let mut items = Vec::new();
    fn collect(nodes: &[BoqNode], items: &mut Vec<Item>) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => collect(&category.body, items),
                BoqNode::Item(item) => items.push(item.clone()),
                _ => {}
            }
        }
    }
    collect(&doc.award.boq.body, &mut items);
    let item = |part: &str, nth: usize| {
        items
            .iter()
            .filter(|item| item.rno_part == part)
            .nth(nth)
            .unwrap()
            .clone()
    };
    let all = item("0030", 0);
    assert_eq!(all.surcharge.as_ref().unwrap().markup_type, "AllInCat");
    assert_eq!(all.markup_percent, Some(dec("5")));
    assert_eq!(all.markup_base, Some(dec("1570")));
    assert_eq!(all.total, Some(dec("78.5")));
    assert!(all.qty.is_none() && all.unit.is_empty() && all.unit_price.is_none());
    assert!(
        item("0020", 0).kind.markup,
        "MarkupIt an der gekennzeichneten Position"
    );
    assert_eq!(
        item("0030", 1).surcharge.as_ref().unwrap().markup_type,
        "IdentAsMark"
    );
    let listed = item("0040", 0);
    let listed_items = &listed.surcharge.as_ref().unwrap().items;
    assert_eq!(
        listed.surcharge.as_ref().unwrap().markup_type,
        "ListInSubQty"
    );
    assert_eq!(listed_items.len(), 2);
    assert_eq!(listed_items[1].sub_qty, Some(dec("5")));

    // The written file passes the GAEB rules (no R9 for the Zuschläge) and
    // keeps the Zuschläge through the reader.
    let written = gaeb::xml::write(&doc, Phase::Dp82).unwrap();
    assert!(
        gaeb::rules::check_xml(written.as_bytes()).is_empty(),
        "{:?}",
        gaeb::rules::check_xml(written.as_bytes())
    );
    let read = gaeb::xml::read(written.as_bytes()).unwrap();
    let (back, warnings) = gaeb_to_project(&read, ProjectName::parse("ZS1").unwrap(), "");
    assert!(!has(&warnings, "Zuschlag"), "{warnings:?}");
    assert_eq!(back.lv().net_total(), project.lv().net_total());
    let back_listed = back.lv().position_by_ordinal("01.02.0040").unwrap();
    let surcharge = back_listed.surcharge().unwrap();
    assert_eq!(surcharge.references.len(), 2);
    assert_eq!(
        back.lv()
            .position_by_id(surcharge.references[1].position)
            .unwrap()
            .ordinal(),
        "01.02.0010"
    );
    assert!(back
        .lv()
        .position_by_ordinal("01.02.0020")
        .unwrap()
        .is_marked_for_surcharge());
    assert_eq!(snapshot(back.lv()), snapshot(project.lv()));

    // Angebotsaufforderung: kind and references, no percentage.
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let written = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    assert!(gaeb::rules::check_xml(written.as_bytes()).is_empty());
    assert!(written.contains("<MarkupType>ListInSubQty</MarkupType>"));
    assert!(!written.contains("<Markup>"));
}

#[test]
fn bidders_price_zuschlaege_in_percent() {
    let project = project_with_surcharges();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.surcharge.is_some() {
            // the bidder's own percentage, X84 short form without MarkupType
            item.surcharge = None;
            item.markup_percent = Some(dec("4"));
            item.markup_base = None;
            item.total = None;
        } else {
            item.unit_price = Some(dec("10"));
            item.total = item.qty.map(|qty| qty * dec("10"));
        }
    });
    let import = match_bid(&project, &doc, "Bieter");
    assert!(
        !import.warnings.iter().any(|w| w.contains("Prozentsatz")),
        "{:?}",
        import.warnings
    );
    let lv = project.lv();
    let id = lv.position_by_ordinal("01.01.0030").unwrap().id();
    assert_eq!(
        import.bid.price(id).and_then(|price| price.unit_price),
        Some(dec("4"))
    );
    // 140 × 10 = 1.400,00, 4 % = 56,00
    let values = import.bid.surcharge_values(lv);
    assert_eq!(values[&id].amount, Money::from_cents(5_600));
    // positions 164 × 10 = 1.640,00 plus 4 % of 1.400 + 40 + (1.000 + 5 × 10)
    let positions = Money::from_cents(164_000);
    let zuschlaege = values.values().map(|value| value.amount).sum::<Money>();
    assert_eq!(zuschlaege, Money::from_cents(5_600 + 160 + 4_200));
    assert_eq!(import.bid.net_total(lv), positions + zuschlaege);

    // A file total without the Zuschläge (as GAEB 90 bids carry it) is named.
    doc.award.boq.totals = Some(gaeb::Totals {
        total: Some(positions.to_decimal()),
        ..gaeb::Totals::default()
    });
    let import = match_bid(&project, &doc, "Bieter");
    assert!(
        import.warnings.iter().any(|warning| warning
            .contains("die Datei zählt die Zuschläge (99,60 €) nicht mit, proAVA wertet sie")),
        "{:?}",
        import.warnings
    );
}

#[test]
fn print_shows_percentage_and_base_of_a_zuschlag() {
    let project = project_with_surcharges();
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Priced, "18.11.2026");
    fn find(items: &[proava_print::PrintItem], oz: &str) -> Option<proava_print::PrintPosition> {
        items.iter().find_map(|item| match item {
            proava_print::PrintItem::Title(title) => find(&title.items, oz),
            proava_print::PrintItem::Position(position) if position.oz == oz => {
                Some(position.clone())
            }
            _ => None,
        })
    }
    let zuschlag = find(&print.items, "01.01.0030").unwrap();
    let surcharge = zuschlag.surcharge.unwrap();
    assert_eq!(surcharge.base, Some(dec("1570")));
    assert_eq!(surcharge.percent, Some(dec("5")));
    assert_eq!(zuschlag.total_price, Some(dec("78.5")));
    assert_eq!(zuschlag.quantity, None);
    assert!(zuschlag
        .remarks
        .iter()
        .any(|remark| remark.starts_with("Zuschlag")));
    assert_eq!(print.net_total, Some(project.lv().net_total().to_decimal()));
}

#[test]
fn price_comparison_print_follows_screen_settings() {
    use proava_application::ComparisonPrint;
    let mut project = demo_project();
    let ids: Vec<_> = project
        .lv()
        .positions()
        .map(|position| position.id())
        .collect();
    for (name, factor) in [("Alpha", 1), ("Beta", 2), ("Gamma", 3)] {
        let mut bid = proava_domain::Bid::new(proava_domain::Address::named(name), "x.x84");
        for id in &ids {
            bid.set_unit_price(*id, Some(Decimal::from(10 * factor)));
        }
        project.add_bid(bid);
    }
    let bid_ids: Vec<_> = project.bids().iter().map(|bid| bid.id()).collect();
    let print = ComparisonPrint {
        bids: vec![bid_ids[2], bid_ids[0]],
        names: vec!["Bieter 1".to_string(), "Bieter 2".to_string()],
        totals_only: true,
        ..ComparisonPrint::default()
    };
    let model = crate::price_comparison_to_print_with(&project, "17.11.2026", &print);
    let names: Vec<_> = model
        .bidders
        .iter()
        .map(|bidder| bidder.name.as_str())
        .collect();
    assert_eq!(names, vec!["Bieter 1", "Bieter 2"]);
    assert!(model.bidders.iter().all(|bidder| bidder.note.is_empty()));
    let first = &model.items[0];
    let proava_print::PcItem::Title(title) = first else {
        panic!("expected a title");
    };
    assert_eq!(title.totals.len(), 2);
    assert!(title.totals[0] > title.totals[1]);
    fn has_position(items: &[proava_print::PcItem]) -> bool {
        items.iter().any(|item| match item {
            proava_print::PcItem::Position(_) => true,
            proava_print::PcItem::Title(title) => has_position(&title.items),
        })
    }
    assert!(!has_position(&model.items));
}

#[test]
fn images_go_into_the_project_on_import() {
    let mut project = demo_project();
    assert_eq!(project.add_image("schema.png", tiny_png()), "schema.png");
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let options = gaeb::xml::WriteOptions {
        include_images: true,
    };
    let plain = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    assert!(!plain.contains("<image"), "Standard-Export ohne Bilder");
    let text = gaeb::xml::write_with(&doc, Phase::Dp83, &options).unwrap();
    let read = gaeb::xml::read(text.as_bytes()).unwrap();

    let (back, warnings) = gaeb_to_project(&read, ProjectName::parse("B").unwrap(), "");
    assert!(
        has(&warnings, "1 Bild aus den Langtexten übernommen"),
        "{warnings:?}"
    );
    assert_eq!(back.images().len(), 1);
    let image = &back.images()[0];
    assert_eq!(image.bytes, tiny_png());
    let used: Vec<String> = back
        .lv()
        .positions()
        .flat_map(|p| {
            p.long_text()
                .images()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(used, vec![image.file.clone()]);
    let width = back
        .lv()
        .positions()
        .flat_map(|p| p.long_text().blocks().to_vec())
        .find_map(|block| match block {
            TextBlock::Image { width_mm, .. } => Some(width_mm),
            _ => None,
        })
        .unwrap();
    let original = project
        .lv()
        .positions()
        .flat_map(|p| p.long_text().blocks().to_vec())
        .find_map(|block| match block {
            TextBlock::Image { width_mm, .. } => Some(width_mm),
            _ => None,
        })
        .unwrap();
    match (width, original) {
        (Some(back), Some(original)) => assert!((back - original).abs() < 0.3),
        (back, original) => assert_eq!(back, original),
    }
}

#[test]
fn preliminary_texts_become_notes_at_the_start() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award.add_texts.push(proava_gaeb::AddText {
        outline: "Vorbemerkungen GA".to_string(),
        detail: proava_gaeb::RichText::plain("Alle Leistungen nach VOB/C."),
    });
    let (back, _) = gaeb_to_project(&doc, ProjectName::parse("RH3").unwrap(), "");
    let Some(LvRow::Note(first)) = back.lv().rows().first() else {
        panic!("first row is no note");
    };
    assert!(first.is_preliminary());
    assert_eq!(first.heading(), "Vorbemerkungen GA");
    assert_eq!(first.text().to_plain(), "Alle Leistungen nach VOB/C.");

    // Back to GAEB: XML carries it as Vorbemerkung, GAEB 90 as first hint.
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    assert_eq!(again.award.add_texts.len(), 1);
    assert_eq!(again.award.add_texts[0].outline, "Vorbemerkungen GA");
    assert!(!again.award.boq.body.iter().any(|node| matches!(
        node,
        BoqNode::Remark(remark) if remark.short_text == "Vorbemerkungen GA"
    )));
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-addtext-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let exchange = GaebPdfExchange;
    for file in ["lv.X83", "lv.D83"] {
        let path = folder.join(file);
        exchange
            .export_gaeb(
                &back,
                proava_application::GaebExport::RequestForQuotation,
                &path,
            )
            .unwrap();
        let (read, _) = exchange
            .import_lv(&path, &ProjectName::parse("RH3").unwrap(), "")
            .unwrap();
        let text = read
            .lv()
            .rows()
            .iter()
            .find_map(|row| match row {
                LvRow::Note(note) if note.text().to_plain().contains("VOB/C") => Some(note),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{file}: Vorbemerkung fehlt"));
        assert_eq!(text.is_preliminary(), file == "lv.X83", "{file}");
    }
    std::fs::remove_dir_all(&folder).unwrap();
}

/// Demo project with two Ausführungsbeschreibungen: AB 2 at the start of
/// ASP10 (used by a position after it and one before it in ASP01), AB 1
/// after the last position (used by a position before it).
fn project_with_performance() -> Project {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let group = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.10")
        .unwrap();
    let mut text = formatted_text();
    text.blocks_mut()
        .retain(|block| !matches!(block, TextBlock::Image { .. }));
    let mut second = proava_domain::PerformanceDescription::new("2", "DDC-Stationen", text);
    second.set_short_text("Automationsstationen");
    let second = lv.insert_row(group + 1, LvRow::Performance(second));
    let first = lv.push_row(LvRow::Performance(
        proava_domain::PerformanceDescription::new(
            "1",
            "Fühler",
            RichText::plain("Alle Fühler mit Tauchhülse."),
        ),
    ));
    let id_of = |lv: &Lv, ordinal: &str| lv.position_by_ordinal(ordinal).unwrap().id();
    for (ordinal, performance) in [
        ("01.10.0010", second),
        ("01.01.0010", second),
        ("02.01.0010", first),
    ] {
        let position = id_of(lv, ordinal);
        lv.link_performance(position, Some(performance)).unwrap();
    }
    project
}

#[test]
fn performance_descriptions_and_references_go_through_xml_and_datan() {
    let project = project_with_performance();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-perf-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let expected = snapshot(project.lv());
    assert!(expected.iter().any(|row| row.contains("AB=Some(\"2\")")));
    for (file, kind) in [
        ("lv.X81", proava_application::GaebExport::Description),
        ("lv.X82", proava_application::GaebExport::PricedLv),
        (
            "lv.X83",
            proava_application::GaebExport::RequestForQuotation,
        ),
        (
            "lv.D83",
            proava_application::GaebExport::RequestForQuotation,
        ),
        ("lv.D82", proava_application::GaebExport::PricedLv),
    ] {
        let path = folder.join(file);
        let warnings = exchange.export_gaeb(&project, kind, &path).unwrap();
        assert!(
            !warnings
                .iter()
                .any(|w| w.contains("Ausführungsbeschreibung") && !w.starts_with("Fachregel")),
            "{file}: {warnings:?}"
        );
        // 01.01.0010 and 02.01.0010 refer to descriptions that stand after
        // them; GAEB wants them before (FD 3.3 11.2.3 PerfDescr 2).
        assert!(
            has(&warnings, "Fachregel R10 (Fehler): OZ 01.01.0010 bezieht sich auf Ausführungsbeschreibung A0002, die erst danach steht."),
            "{file}: {warnings:?}"
        );
        let (back, warnings) = exchange
            .import_lv(&path, &ProjectName::parse("RH3").unwrap(), "")
            .unwrap();
        assert!(
            !warnings
                .iter()
                .any(|w| w.contains("Ausführungsbeschreibung")),
            "{file}: {warnings:?}"
        );
        if file.contains(".X") {
            // PerfNo without leading zeros (proAVA shows A0002).
            let xml = std::fs::read_to_string(&path).unwrap();
            assert!(
                xml.contains("<PerfNo>2</PerfNo>") && !xml.contains("<PerfNo>0002</PerfNo>"),
                "{file}"
            );
        }
        let lv = back.lv();
        assert_eq!(lv.performance_count(), 2, "{file}");
        let second = lv.performance_by_number("2").unwrap();
        assert_eq!(
            (second.number(), second.mark().as_str()),
            ("2", "A0002"),
            "{file}"
        );
        assert_eq!(second.label(), "DDC-Stationen", "{file}");
        assert_eq!(lv.positions_using(second.id()).len(), 2, "{file}");
        let first = lv.performance_by_number("1").unwrap();
        assert_eq!(lv.positions_using(first.id()).len(), 1, "{file}");
        if file.starts_with("lv.X") {
            // GAEB 90 keeps neither formatting nor the short text of an AB.
            let mut got = snapshot(lv);
            let mut want = expected.clone();
            if file == "lv.X81" || file == "lv.X83" {
                got.retain(|row| !row.starts_with('P'));
                want.retain(|row| !row.starts_with('P'));
            }
            assert_eq!(got, want, "{file}");
        }
    }
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn performance_descriptions_are_printed_and_referenced() {
    let project = project_with_performance();
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "17.11.2026");
    fn walk(items: &[proava_print::PrintItem], notes: &mut Vec<String>, refs: &mut Vec<String>) {
        for item in items {
            match item {
                proava_print::PrintItem::Title(title) => walk(&title.items, notes, refs),
                proava_print::PrintItem::Note(blocks)
                | proava_print::PrintItem::Preliminary(blocks)
                | proava_print::PrintItem::Closing(blocks)
                | proava_print::PrintItem::Performance(_, blocks) => {
                    if let Some(proava_print::TextBlock::Paragraph { runs, .. }) = blocks.first() {
                        notes.push(runs.iter().map(|run| run.text.as_str()).collect());
                    }
                }
                proava_print::PrintItem::Position(position) => {
                    for remark in &position.remarks {
                        refs.push(format!("{} {remark}", position.oz));
                    }
                }
            }
        }
    }
    let (mut notes, mut refs) = (Vec::new(), Vec::new());
    walk(&print.items, &mut notes, &mut refs);
    assert!(notes.contains(&"DDC-Stationen".to_string()), "{notes:?}");
    assert!(notes.contains(&"Fühler".to_string()), "{notes:?}");
    assert_eq!(
        refs,
        vec![
            "01.01.0010 Ausführung nach Ausführungsbeschreibung A0002 – DDC-Stationen",
            "01.10.0010 Ausführung nach Ausführungsbeschreibung A0002 – DDC-Stationen",
            "02.01.0010 Ausführung nach Ausführungsbeschreibung A0001 – Fühler",
        ]
    );
}

/// Kurztext-LV: Ausführungsbeschreibungen keep mark and heading, the
/// positions their reference (a bidder must see that one applies).
#[test]
fn short_text_lv_keeps_performance_marks_and_references() {
    let project = project_with_performance();
    let job = proava_application::PrintJob::new(proava_application::PrintKind::ShortTextLv);
    assert!(job.settings.performance && !job.settings.long_texts);
    let selection = crate::print_job::print_selection(&project, &job).unwrap();
    let mut print = crate::lv_to_print(&selection, proava_print::LvPrintMode::Priced, "17.11.2026");
    crate::print_job::strip_long_texts(&mut print.items);
    fn walk(
        items: &[proava_print::PrintItem],
        marks: &mut Vec<String>,
        refs: &mut usize,
        texts: &mut usize,
    ) {
        for item in items {
            match item {
                proava_print::PrintItem::Title(title) => walk(&title.items, marks, refs, texts),
                proava_print::PrintItem::Performance(mark, blocks) => {
                    assert_eq!(blocks.len(), 1, "nur die Überschrift");
                    marks.push(mark.clone());
                }
                proava_print::PrintItem::Position(position) => {
                    *refs += position
                        .remarks
                        .iter()
                        .filter(|remark| remark.starts_with("Ausführung nach "))
                        .count();
                    *texts += position.long_text.len();
                }
                proava_print::PrintItem::Note(_)
                | proava_print::PrintItem::Preliminary(_)
                | proava_print::PrintItem::Closing(_) => {}
            }
        }
    }
    let (mut marks, mut refs, mut texts) = (Vec::new(), 0, 0);
    walk(&print.items, &mut marks, &mut refs, &mut texts);
    assert_eq!(marks, ["A0002", "A0001"]);
    assert_eq!((refs, texts), (3, 0));
}

#[test]
fn export_warns_about_performance_numbers_gaeb_cannot_carry() {
    let mut project = project_with_performance();
    let id = project.lv().performance_by_number("1").unwrap().id();
    project.lv_mut().set_performance_number(id, "X1").unwrap();
    let (_, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(
        has(&warnings, "Ausführungsbeschreibungen ohne Nummer 1–9999"),
        "{warnings:?}"
    );
}

/// Demo project with what GAEB carries beyond text, quantity and price:
/// complements, flags, a Wiederholungsbeschreibung, a hint heading, the
/// LV number and the project short name.
fn project_with_details() -> Project {
    let mut project = demo_project();
    project.info_mut().short_name = "40651".into();
    let lv = project.lv_mut();
    lv.set_number("480-V08");
    let base = lv.position_by_ordinal("01.01.0010").unwrap().id();
    let index = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0020")
        .unwrap();
    let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
    position.set_quantity_open(true);
    position.set_price_breakdown(true);
    position.set_key_item(true);
    position.set_complements(vec![
        proava_domain::TextComplement {
            marker: "01".into(),
            kind: proava_domain::ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            ..Default::default()
        },
        proava_domain::TextComplement {
            marker: "02".into(),
            kind: proava_domain::ComplementKind::Owner,
            caption: "Farbe".into(),
            body: "RAL 7035".into(),
            ..Default::default()
        },
    ]);
    let repeated = position.id();
    lv.link_repetition(repeated, Some(base)).unwrap();
    let index = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.10.0010")
        .unwrap();
    lv.row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_not_applicable(true);
    let note = lv
        .rows()
        .iter()
        .position(|row| row.short_text() == "Hinweis zum Titel")
        .unwrap();
    if let Some(LvRow::Note(note)) = lv.row_mut(note) {
        note.set_heading("Wichtig");
    }
    project
}

#[test]
fn complements_flags_repetitions_headings_and_names_go_through_gaeb() {
    let project = project_with_details();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-details-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for (file, kind) in [
        ("lv.X82", proava_application::GaebExport::PricedLv),
        (
            "lv.X83",
            proava_application::GaebExport::RequestForQuotation,
        ),
        (
            "lv.D83",
            proava_application::GaebExport::RequestForQuotation,
        ),
    ] {
        let path = folder.join(file);
        exchange.export_gaeb(&project, kind, &path).unwrap();
        let (back, warnings) = exchange
            .import_lv(&path, &ProjectName::parse("Neu").unwrap(), "")
            .unwrap();
        let lv = back.lv();
        let position = lv.position_by_ordinal("01.01.0020").unwrap();
        assert!(position.is_quantity_open(), "{file}");
        assert_eq!(position.complements().len(), 2, "{file}");
        assert_eq!(position.complements()[0].caption, "Fabrikat:", "{file}");
        assert_eq!(
            position.complements()[1].kind,
            proava_domain::ComplementKind::Owner,
            "{file}"
        );
        assert_eq!(position.complements()[1].body, "RAL 7035", "{file}");
        let base = lv.position_by_id(position.repeats().expect(file)).unwrap();
        assert_eq!(base.ordinal(), "01.01.0010", "{file}");
        assert!(
            !warnings.iter().any(|w| w.contains("Wiederholung")),
            "{file}: {warnings:?}"
        );
        if file.starts_with("lv.X") {
            assert!(
                position.has_price_breakdown() && position.is_key_item(),
                "{file}"
            );
            assert_eq!(lv.number(), "480-V08", "{file}");
            assert_eq!(back.info().short_name, "40651", "{file}");
            let heading = lv.rows().iter().find_map(|row| match row {
                LvRow::Note(note) if !note.heading().is_empty() => Some(note.heading()),
                _ => None,
            });
            assert_eq!(heading, Some("Wichtig"), "{file}");
        }
        if file == "lv.X82" {
            let gone = lv.position_by_ordinal("01.10.0010").unwrap();
            assert!(gone.is_not_applicable());
            assert_eq!(lv.net_total(), project.lv().net_total());
        }
    }
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn print_shows_complements_repetitions_not_applicable_and_headings() {
    let project = project_with_details();
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Priced, "17.11.2026");
    fn texts(items: &[proava_print::PrintItem], out: &mut Vec<String>) {
        for item in items {
            let blocks = match item {
                proava_print::PrintItem::Title(title) => {
                    texts(&title.items, out);
                    continue;
                }
                proava_print::PrintItem::Note(blocks)
                | proava_print::PrintItem::Preliminary(blocks)
                | proava_print::PrintItem::Closing(blocks)
                | proava_print::PrintItem::Performance(_, blocks) => blocks,
                proava_print::PrintItem::Position(position) => {
                    // Remarks under the short text („Position entfällt“ …).
                    out.extend(position.remarks.iter().cloned());
                    &position.long_text
                }
            };
            for block in blocks {
                if let proava_print::TextBlock::Paragraph { runs, .. } = block {
                    out.push(runs.iter().map(|run| run.text.as_str()).collect());
                }
            }
        }
    }
    let mut all = Vec::new();
    texts(&print.items, &mut all);
    for wanted in [
        "Fabrikat: ....................................................",
        "Farbe RAL 7035",
        "Wiederholung von Position 01.01.0010",
        "Position entfällt",
        "Wichtig",
    ] {
        assert!(all.iter().any(|text| text == wanted), "{wanted}: {all:?}");
    }
    let mut titles = Vec::new();
    let sum = print_titles(&print.items, &mut titles);
    assert_eq!(sum, project.lv().net_total().to_decimal());
}

#[test]
fn price_components_time_and_alphanumeric_positions_go_through_gaeb() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    lv.set_price_components(vec![
        proava_domain::PriceComponent {
            label: "Lohn".into(),
            kind: "Wages".into(),
        },
        proava_domain::PriceComponent {
            label: "Stoffe".into(),
            kind: "Materials".into(),
        },
    ]);
    lv.set_time_label("Stunden");
    lv.set_alphanumeric_positions(true);
    let index = lv
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0020")
        .unwrap();
    let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
    position.set_price_components(vec![Some(dec("60")), Some(dec("40"))]);
    position.set_time_quantity(Some(dec("0.5")));

    let (priced, _) = lv_to_gaeb(&project, Phase::Dp82);
    assert_eq!(priced.award.boq.up_component_labels, ["Lohn", "Stoffe"]);
    let item_level = priced
        .award
        .boq
        .breakdown
        .iter()
        .find(|level| level.kind == BreakdownKind::Item)
        .unwrap();
    assert!(!item_level.numeric);
    let text = proava_gaeb::xml::write(&priced, Phase::Dp82).unwrap();
    let read = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(
        back.lv().price_components(),
        project.lv().price_components()
    );
    assert_eq!(back.lv().time_label(), "Stunden");
    assert!(back.lv().has_alphanumeric_positions());
    let position = back.lv().position_by_ordinal("01.01.0020").unwrap();
    assert_eq!(
        position.price_components(),
        [Some(dec("60")), Some(dec("40"))]
    );
    assert_eq!(position.time_quantity(), Some(dec("0.5")));

    // The X83 asks for the EP shares (labels as in the X82), without values;
    // the X81 names them as well (as other AVA programs do), also without values.
    let (tender, _) = lv_to_gaeb(&project, Phase::Dp83);
    assert_eq!(tender.award.boq.up_component_labels, ["Lohn", "Stoffe"]);
    assert_eq!(tender.award.boq.up_component_kinds, ["Wages", "Materials"]);
    assert!(tender
        .award
        .boq
        .items()
        .iter()
        .all(|(_, item)| item.up_components.is_empty() && item.unit_price.is_none()));
    let text = proava_gaeb::xml::write(&tender, Phase::Dp83).unwrap();
    assert!(text.contains("<NoUPComps>2</NoUPComps>"), "{text}");
    // Eine eingelesene X83 behält die Bezeichnungen (sonst fragte die wieder
    // geschriebene X83 die Aufgliederung ohne Anteile ab).
    let read = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(
        back.lv().price_components(),
        project.lv().price_components()
    );
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    assert_eq!(again.award.boq.up_component_labels, ["Lohn", "Stoffe"]);
    let (description, _) = lv_to_gaeb(&project, Phase::Dp81);
    assert_eq!(
        description.award.boq.up_component_labels,
        ["Lohn", "Stoffe"]
    );
    assert!(description
        .award
        .boq
        .items()
        .iter()
        .all(|(_, item)| item.up_components.is_empty() && item.unit_price.is_none()));
    let text = proava_gaeb::xml::write(&description, Phase::Dp81).unwrap();
    assert!(text.contains("<NoUPComps>2</NoUPComps>"), "{text}");
    assert!(
        text.contains("<LblUPComp1 Type=\"Wages\">Lohn</LblUPComp1>"),
        "{text}"
    );
    assert!(!text.contains("<UPComp1>"), "{text}");
    // … and comes back when the X81 is read (labels from every phase).
    let read = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(
        back.lv().price_components(),
        project.lv().price_components()
    );
}

/// Abnahme mit einem echten LV: Währungsname und Bruttosumme der obersten Titel wie in
/// AVA-Programmen; Untertitel nur netto.
#[test]
fn currency_label_and_gross_sums_of_top_level_titles() {
    let project = demo_project();
    let (priced, _) = lv_to_gaeb(&project, Phase::Dp82);
    assert_eq!(priced.project.currency_label, "Euro");
    let vat = project.lv().vat_percent();
    let mut checked = 0;
    for node in &priced.award.boq.body {
        let BoqNode::Category(category) = node else {
            continue;
        };
        let totals = category.totals.as_ref().unwrap();
        let net = Money::from_decimal(totals.total.unwrap());
        assert_eq!(
            totals.total_gross,
            Some((net + net.percent(vat)).to_decimal())
        );
        for inner in &category.body {
            if let BoqNode::Category(sub) = inner {
                assert_eq!(sub.totals.as_ref().unwrap().total_gross, None);
            }
        }
        checked += 1;
    }
    assert!(checked > 0);
    let text = proava_gaeb::xml::write(&priced, Phase::Dp82).unwrap();
    assert!(text.contains("<CurLbl>Euro</CurLbl>"));
    // Ein oberster Titel nur mit Hinweisen (Vorbemerkungen) hat keine Bruttosumme.
    let mut with_notes = demo_project();
    let lv = with_notes.lv_mut();
    lv.insert_row(0, LvRow::Note(Note::new("Vorbemerkung")));
    lv.insert_row(0, LvRow::Group(LvGroup::new(0, "00", "Vorbemerkungen")));
    let (priced, _) = lv_to_gaeb(&with_notes, Phase::Dp82);
    let BoqNode::Category(first) = &priced.award.boq.body[0] else {
        panic!("Titel erwartet")
    };
    assert_eq!(first.rno_part, "00");
    assert_eq!(first.totals.as_ref().unwrap().total_gross, None);
    // Ohne Preise keine Summen.
    let (tender, _) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(tender.award.boq.body.iter().all(|node| match node {
        BoqNode::Category(category) => category.totals.is_none(),
        _ => true,
    }));
}

#[test]
fn fonts_sizes_alignment_and_tables_survive_gaeb_xml() {
    use proava_domain::{Align, Cell};
    let text = RichText::new(vec![
        TextBlock::Paragraph {
            runs: vec![Run {
                font: Some("Times New Roman".into()),
                size_pt: Some(14.0),
                bold: true,
                ..Run::plain("Überschrift")
            }],
            align: Align::Center,
        },
        TextBlock::Paragraph {
            runs: vec![
                Run::plain("Standard "),
                Run {
                    font: Some("Courier New".into()),
                    size_pt: Some(8.0),
                    ..Run::plain("Code")
                },
            ],
            align: Align::Justify,
        },
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![Run::plain("rechts")],
            align: Align::Right,
        },
        TextBlock::Table(Table {
            rows: vec![
                vec![Cell::plain("Typ"), Cell::plain("Wert")],
                vec![
                    Cell::plain("E/A"),
                    Cell {
                        runs: vec![Run {
                            italic: true,
                            ..Run::plain("32")
                        }],
                        align: Align::Right,
                    },
                ],
            ],
            header_rows: 1,
            column_widths_mm: vec![Some(40.0), None],
        }),
    ]);
    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.as_position().is_some())
        .unwrap();
    project
        .lv_mut()
        .row_mut(index)
        .and_then(LvRow::as_position_mut)
        .unwrap()
        .set_long_text(text.clone());
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let xml = proava_gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    assert!(xml.contains("font-family:Times New Roman;font-size:14pt;font-weight:bold;"));
    assert!(xml.contains("<p style=\"text-align:center;\">"));
    let read = proava_gaeb::xml::read(xml.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("Zurück").unwrap(), "");
    let position = back.lv().rows()[index].as_position().unwrap();
    let mut expected = text;
    // Pixel widths: 40 mm → 151 px → 40.0 mm; head cells come back bold.
    if let TextBlock::Table(table) = &mut expected.blocks_mut()[3] {
        table.column_widths_mm = vec![Some(40.0), None];
    }
    assert_eq!(position.long_text(), &expected);
}

#[test]
fn items_with_index_but_no_index_level_keep_their_ordinals() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award
        .boq
        .breakdown
        .retain(|level| level.kind != BreakdownKind::Index);
    let (back, warnings) = gaeb_to_project(&doc, ProjectName::parse("RH3").unwrap(), "");
    assert!(has(&warnings, "Indexstelle"), "{warnings:?}");
    assert_eq!(back.lv().oz_mask().pattern(), "11.22.PPPPI");
    assert!(back.lv().position_by_ordinal("01.01.0070.1").is_some());
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    let ordinals: Vec<String> = again
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, _)| pieces.join("."))
        .collect();
    assert!(
        ordinals.contains(&"01.01.0070.1".to_string()),
        "{ordinals:?}"
    );
}

#[test]
fn execution_period_binding_period_and_procedure_go_through_gaeb() {
    let mut project = demo_project();
    let info = project.info_mut();
    info.execution_start = "01.03.2027".into();
    info.execution_end = "30.09.2027".into();
    info.binding_period_end = "31.01.2027".into();
    info.award_procedure = "Öffentliche Ausschreibung".into();
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(!has(&warnings, "kein Datum"), "{warnings:?}");
    assert_eq!(doc.award.category, "OpenCall");
    assert_eq!(
        doc.award.submission.construction_start.as_deref(),
        Some("2027-03-01")
    );
    let text = proava_gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let read = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(back.info().execution_start, "01.03.2027");
    assert_eq!(back.info().execution_end, "30.09.2027");
    assert_eq!(back.info().binding_period_end, "31.01.2027");
    assert_eq!(back.info().award_procedure, "Öffentliche Ausschreibung");

    project.info_mut().award_procedure = "Losverfahren".into();
    project.info_mut().execution_start = "Frühjahr".into();
    let (doc, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(doc.award.category.is_empty());
    assert!(has(&warnings, "Vergabeart „Losverfahren“"), "{warnings:?}");
    assert!(
        has(&warnings, "Ausführungsbeginn „Frühjahr“"),
        "{warnings:?}"
    );
}

#[test]
fn contract_text_goes_back_to_gaeb_90_as_t1() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award.contract_text = vec!["Es gilt die VOB/B.".into(), "Zweite Zeile.".into()];
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-t1-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("quelle.D83");
    std::fs::write(&source, proava_gaeb::datan::write(&doc).unwrap()).unwrap();
    let exchange = GaebPdfExchange;
    let (imported, _) = exchange
        .import_lv(&source, &ProjectName::parse("RH3").unwrap(), "")
        .unwrap();
    let target = folder.join("ziel.D83");
    exchange
        .export_gaeb(
            &imported,
            proava_application::GaebExport::RequestForQuotation,
            &target,
        )
        .unwrap();
    let read = proava_gaeb::datan::read_file(&target).unwrap().document;
    assert_eq!(
        read.award.contract_text.join(" "),
        "Es gilt die VOB/B. Zweite Zeile."
    );
    assert!(!read.award.boq.body.iter().any(|node| matches!(
        node,
        BoqNode::Remark(remark) if remark.text.to_plain().contains("Es gilt die VOB/B")
    )));
    std::fs::remove_dir_all(&folder).unwrap();
}

// ----- STLB-Bau -----

fn stlb_file() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../proava-gaeb/tests/data/syn4_stlb_33.X83")
}

#[test]
fn catalog_positions_carry_the_stlb_key() {
    let read = GaebPdfExchange
        .read_catalog_positions(&stlb_file())
        .unwrap();
    assert_eq!(read.positions.len(), 4);
    assert_eq!(read.with_stlb_key, 2);
    let first = &read.positions[0];
    let stlb = first.stlb_reference().unwrap();
    assert_eq!((stlb.group, stlb.work_category.as_str()), (9001, "070"));
    assert_eq!(stlb.characteristics.len(), 3);
    assert_eq!(stlb.complement_links, vec![("1".to_string(), 90013)]);
    assert!(!first.stlb_text_changed());
    assert_eq!(
        first.catalog_line().as_deref(),
        Some("STLB-Bau 4/2026 070 TA")
    );
    assert_eq!(
        read.positions[1].catalog_line().as_deref(),
        Some("STLB-Bau 10/2025 070 TB")
    );
    assert_eq!(
        read.positions[2]
            .standard_number()
            .map(|n| n.number.as_str()),
        Some("12345678901234567")
    );
    assert_eq!(read.positions[3].catalog_line(), None);
}

#[test]
fn stlb_keys_go_through_the_project_to_gaeb() {
    let file = std::fs::read(stlb_file()).unwrap();
    let doc = gaeb::xml::read(&file).unwrap();
    let (project, _) = gaeb_to_project(&doc, ProjectName::parse("S").unwrap(), "");
    for phase in [Phase::Dp81, Phase::Dp83, Phase::Dp86] {
        let (out, _) = lv_to_gaeb(&project, phase);
        let original: Vec<_> = doc
            .award
            .boq
            .items()
            .into_iter()
            .map(|(_, i)| i.clone())
            .collect();
        let written: Vec<_> = out
            .award
            .boq
            .items()
            .into_iter()
            .map(|(_, i)| i.clone())
            .collect();
        assert_eq!(original.len(), written.len());
        for (a, b) in original.iter().zip(&written) {
            assert_eq!(a.stlb, b.stlb, "{phase:?} {}", a.rno_part);
            assert_eq!(a.standard_number, b.standard_number);
            let links = |item: &Item| -> Vec<Option<i64>> {
                item.complements
                    .iter()
                    .map(|c| c.stlb_characteristic)
                    .collect()
            };
            assert_eq!(links(a), links(b));
        }
        // And through the XML writer and reader once more.
        let text = gaeb::xml::write(&out, phase).unwrap();
        assert_eq!(text.matches("<STLBBau>").count(), 2);
    }
}

#[test]
fn changed_stlb_long_text_leaves_without_the_key() {
    let file = std::fs::read(stlb_file()).unwrap();
    let doc = gaeb::xml::read(&file).unwrap();
    let (mut project, _) = gaeb_to_project(&doc, ProjectName::parse("S").unwrap(), "");
    let stlb: Vec<usize> = project
        .lv()
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.as_position()
                .is_some_and(|p| p.stlb_reference().is_some())
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(stlb.len(), 2);
    // Imported STLB-Bau texts are write-protected at once.
    for &index in &stlb {
        let position = project.lv().rows()[index].as_position().unwrap();
        assert!(position.is_text_locked(), "{}", position.ordinal());
        assert!(!position.stlb_text_changed());
    }
    // First: the Langtext changed (an old project, before the lock).
    // Second: only the Kurztext – that stays an STLB-Bau text.
    let first = project.lv().rows()[stlb[0]]
        .as_position()
        .unwrap()
        .ordinal()
        .to_string();
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(stlb[0]) {
        position.set_long_text(RichText::plain("Eigener Text"));
        assert!(position.stlb_text_changed());
    }
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(stlb[1]) {
        position.set_short_text("Kurztext geändert");
        assert!(!position.stlb_text_changed(), "Kurztext ist frei");
    }
    let (out, messages) = lv_to_gaeb(&project, Phase::Dp83);
    let items: Vec<_> = out
        .award
        .boq
        .items()
        .into_iter()
        .map(|(_, i)| i.clone())
        .collect();
    let changed = items
        .iter()
        .find(|item| item.short_text.contains("Kurztext geändert"));
    assert!(
        changed.is_some_and(|item| item.stlb.is_some()),
        "Kurztext allein behält die Kennung"
    );
    let text = gaeb::xml::write(&out, Phase::Dp83).unwrap();
    assert_eq!(
        text.matches("<STLBBau>").count(),
        1,
        "geänderter Langtext ohne STLBBau"
    );
    assert!(
        messages
            .iter()
            .any(|line| line.contains("weicht von STLB-Bau ab") && line.contains(&first)),
        "{messages:?}"
    );
}

#[test]
fn print_shows_the_catalogue_line_first() {
    let file = std::fs::read(stlb_file()).unwrap();
    let doc = gaeb::xml::read(&file).unwrap();
    let (project, _) = gaeb_to_project(&doc, ProjectName::parse("S").unwrap(), "");
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "17.11.2026");
    let text = format!("{:?}", print.items);
    let line = text.find("STLB-Bau 4/2026 070 TA").expect("Katalogzeile");
    let body = text.find("Testgerät Raumbediengerät für").unwrap();
    assert!(line < body);
}

#[test]
fn catalog_positions_from_a_zip() {
    use std::io::Write;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-stlb-zip-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("STLB-Export.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("liesmich.txt", options).unwrap();
    zip.write_all(b"kein GAEB").unwrap();
    zip.start_file("export/LV.X83", options).unwrap();
    zip.write_all(&std::fs::read(stlb_file()).unwrap()).unwrap();
    zip.finish().unwrap();
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    assert_eq!((read.positions.len(), read.with_stlb_key), (4, 2));
    assert_eq!(read.source_in_archive, "export/LV.X83");

    let empty = folder.join("leer.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&empty).unwrap());
    zip.start_file("liesmich.txt", options).unwrap();
    zip.finish().unwrap();
    let error = GaebPdfExchange.read_catalog_positions(&empty).unwrap_err();
    assert!(error.contains("keine GAEB-Datei"), "{error}");
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn keys_of_sub_and_performance_descriptions_go_through() {
    use proava_domain::{
        CatalogKeys, PerformanceDescription, StandardNumber, StlbReference, SubDescription,
    };
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    let mut performance =
        PerformanceDescription::new("1", "Trassen", RichText::plain("Kabeltrassen"));
    performance.set_catalog(CatalogKeys {
        stlb: Some(StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some("2026-04".into()),
            work_category: "053".into(),
            group: 77,
            ..StlbReference::default()
        }),
        standard_number: None,
    });
    lv.push_row(LvRow::Performance(performance));
    let mut leader = position("", "Leitbeschreibung", "1", "St", None);
    leader.set_sub_descriptions(vec![SubDescription {
        number: "1".into(),
        short_text: "Variante".into(),
        long_text: RichText::plain("Text"),
        quantity: Some(dec("2")),
        unit: "St".into(),
        catalog: CatalogKeys {
            stlb: None,
            standard_number: Some(StandardNumber {
                number: "9300201123000202".into(),
                catalog: "StLB".into(),
            }),
        },
    }]);
    lv.push_position(leader);
    lv.renumber();
    let project = Project::new(ProjectName::parse("K").unwrap(), "", lv);
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let text = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let read = gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("K").unwrap(), "");
    let performance = back
        .lv()
        .rows()
        .iter()
        .find_map(|row| match row {
            LvRow::Performance(performance) => Some(performance),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        performance.catalog().stlb.as_ref().map(|s| s.group),
        Some(77)
    );
    let leader = back.lv().positions().next().unwrap();
    assert_eq!(
        leader.sub_descriptions()[0]
            .catalog
            .standard_number
            .as_ref()
            .map(|n| n.number.as_str()),
        Some("9300201123000202")
    );
    let print = crate::lv_to_print(&back, proava_print::LvPrintMode::Blank, "17.11.2026");
    let printed = format!("{:?}", print.items);
    assert!(printed.contains("STLB-Bau 4/2026 053"), "{printed}");
    assert!(printed.contains("StLB 9300201123000202"));
}

/// Demo project with the bid of `bidder_file` (prices 10 €, 3 % Nachlass),
/// one Textergänzung of the bidder and the award.
fn awarded_project() -> (Project, proava_domain::BidId) {
    let mut project = project_with_details();
    let mut doc = bidder_file(&project);
    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.rno_part == "0020" {
            for complement in &mut item.complements {
                if complement.kind == "Bidder" {
                    complement.body = "Siemens PXC5".into();
                }
            }
        }
    });
    let import = match_bid(&project, &doc, "muster.x84");
    assert!(
        has(&import.warnings, "Bieterangaben"),
        "{:?}",
        import.warnings
    );
    let id = project.add_bid(import.bid);
    project.info_mut().award_number = "VE 0420".into();
    project.info_mut().execution_start = "01.03.2027".into();
    project.info_mut().execution_end = "30.09.2027".into();
    project.award_bid(id, "01.12.2026").unwrap();
    (project, id)
}

#[test]
fn bidder_complements_are_kept_with_the_bid() {
    let (project, id) = awarded_project();
    let bid = project.bids().iter().find(|bid| bid.id() == id).unwrap();
    let position = project.lv().position_by_ordinal("01.01.0020").unwrap();
    assert_eq!(bid.complements_of(position.id())[0].marker, "01");
    assert_eq!(bid.complement_lines(position), ["Fabrikat: Siemens PXC5"]);
}

#[test]
fn contract_lv_carries_bid_prices_bidder_discount_and_complements() {
    let (project, id) = awarded_project();
    let bid = project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .unwrap()
        .clone();
    let (doc, warnings) = crate::contract_to_gaeb(&project, id).unwrap();
    assert!(!has(&warnings, "nicht beauftragt"), "{warnings:?}");
    assert_eq!(doc.phase, Phase::Dp86);
    assert_eq!(
        doc.award.bidder.as_ref().unwrap().name1,
        "Muster Automation GmbH"
    );
    assert_eq!(doc.award.owner.as_ref().unwrap().name1, "Stadt Düsseldorf");
    assert_eq!(doc.award.award_no, "VE 0420");
    assert_eq!(
        doc.award.submission.construction_start.as_deref(),
        Some("2027-03-01")
    );
    let totals = doc.award.boq.totals.clone().unwrap();
    let net = bid.net_total(project.lv());
    assert_eq!(totals.total, Some(net.to_decimal()));
    assert_eq!(totals.discount_percent, Some(dec("3")));
    assert_eq!(
        totals.total_after_discount,
        Some(bid.total_after_discount(project.lv()).to_decimal())
    );
    assert_eq!(
        bid.award().unwrap().total,
        bid.total_after_discount(project.lv())
    );
    let items = doc.award.boq.items();
    let (_, item) = items
        .iter()
        .find(|(pieces, _)| pieces.join(".") == "01.01.0020")
        .unwrap();
    assert_eq!(item.unit_price, Some(dec("10")));
    let complement = item
        .complements
        .iter()
        .find(|c| c.kind == "Bidder")
        .unwrap();
    assert_eq!(complement.body, "Siemens PXC5");

    // Through the files: X86 and D86 come back with the bid's prices.
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-contract-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for (file, kind) in [
        ("auftrag.X86", proava_application::ContractExport::Xml),
        ("auftrag.D86", proava_application::ContractExport::Datan),
    ] {
        let path = folder.join(file);
        let warnings = exchange.export_contract(&project, id, kind, &path).unwrap();
        // GAEB 90 has no „entfällt“: 01.10.0010 counts again there.
        assert_eq!(
            has(&warnings, "entfällt"),
            file.ends_with("D86"),
            "{warnings:?}"
        );
        let (back, _) = exchange
            .import_lv(&path, &ProjectName::parse("RH3").unwrap(), "")
            .unwrap();
        let position = back.lv().position_by_ordinal("01.01.0020").unwrap();
        assert_eq!(position.unit_price(), Some(dec("10")), "{file}");
        if file.ends_with("X86") {
            assert_eq!(back.lv().net_total(), net, "{file}");
        }
    }
    let pdf = folder.join("auftrag.pdf");
    exchange
        .export_contract(&project, id, proava_application::ContractExport::Pdf, &pdf)
        .unwrap();
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    // Schemaprüfung, wenn die GAEB-XSDs auf dem Rechner liegen
    // (scripts/gaeb_muster_laden.sh, siehe docs/BUILD.md).
    if let Some(xsd) = proava_gaeb::schemas::schema_folder() {
        let script = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../proava-gaeb/scripts/validate_xsd.py"
        );
        let output = std::process::Command::new("python3")
            .args([
                script,
                xsd.to_str().unwrap(),
                folder.join("auftrag.X86").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn contract_print_has_parties_discount_and_bidder_texts() {
    let (project, id) = awarded_project();
    let (print, _) = crate::contract_to_print(&project, id, "01.12.2026").unwrap();
    assert_eq!(print.header.document_kind.as_deref(), Some("Auftrags-LV"));
    let labels: Vec<&str> = print
        .parties
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "Auftraggeber",
            "Auftragnehmer",
            "Vergabenummer",
            "Angebot vom",
            "Auftrag erteilt",
            "Ausführungszeit"
        ]
    );
    let (label, amount) = print.discount.clone().unwrap();
    assert_eq!(label, "Nachlass 3 %");
    let bid = project.bids().iter().find(|bid| bid.id() == id).unwrap();
    let net = bid.net_total(project.lv());
    assert_eq!(amount, bid.discount_amount(net).to_decimal());
    let mut titles = Vec::new();
    assert_eq!(print_titles(&print.items, &mut titles), net.to_decimal());
    let mut found = false;
    fn walk(items: &[proava_print::PrintItem], found: &mut bool) {
        for item in items {
            match item {
                proava_print::PrintItem::Title(title) => walk(&title.items, found),
                proava_print::PrintItem::Position(position) => {
                    for block in &position.long_text {
                        if let proava_print::TextBlock::Paragraph { runs, .. } = block {
                            let text: String = runs.iter().map(|run| run.text.as_str()).collect();
                            *found |= text.contains("Siemens PXC5");
                        }
                    }
                }
                _ => {}
            }
        }
    }
    walk(&print.items, &mut found);
    assert!(found);
}

/// Awarded demo project with long texts on every position (many pages),
/// an image and an Ausführungsbeschreibung.
fn print_project() -> (Project, proava_domain::BidId) {
    let (mut project, bid) = awarded_project();
    let image = project.add_image("plan.png", tiny_png());
    let lv = project.lv_mut();
    let ids: Vec<_> = lv.positions().map(|position| position.id()).collect();
    for id in ids {
        let index = lv.index_of(id).unwrap();
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        let mut text = RichText::plain(&"Liefern, montieren und anschließen. ".repeat(40));
        for _ in 0..12 {
            text.push(TextBlock::paragraph(vec![Run::plain(
                "Weitere Ausführung nach Herstellervorgabe und Planung des Auftraggebers.",
            )]));
        }
        text.push(TextBlock::Image {
            file: image.clone(),
            width_mm: Some(40.0),
        });
        position.set_long_text(text);
    }
    lv.push_row(LvRow::Performance(
        proava_domain::PerformanceDescription::new(
            "1",
            "Allgemein",
            RichText::plain("Alles nach VOB/C."),
        ),
    ));
    (project, bid)
}

#[test]
fn every_print_kind_writes_a_pdf_and_options_work() {
    use proava_application::{PrintJob, PrintKind};
    let (mut project, bid) = print_project();
    // Kostenermittlung: the newest of the project is printed.
    project.add_cost_estimate(proava_domain::cost_demo::demo_cost_estimate(
        proava_domain::CostStage::Calculation,
    ));
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-print3-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let mut pages = std::collections::HashMap::new();
    // The Abrechnung has its own test (needs a contract and invoices); the
    // Kostenermittlung (Kosten-LV) prints the same LV as a Kosten-LV.
    let mut cost_lv = project.clone();
    cost_lv.lv_mut().set_purpose(proava_domain::LvPurpose::Cost(
        proava_domain::CostStage::Calculation,
    ));
    for kind in PrintKind::ALL
        .into_iter()
        .filter(|kind| !kind.needs_invoice())
    {
        let mut job = PrintJob::new(kind);
        job.bid = kind.needs_bid().then_some(bid);
        let file = folder.join(format!("{}.pdf", kind.key()));
        let project = if kind == PrintKind::CostLv {
            &cost_lv
        } else {
            &project
        };
        let outcome = exchange.export_print(project, &job, &file).unwrap();
        assert!(
            std::fs::read(&file).unwrap().starts_with(b"%PDF"),
            "{kind:?}"
        );
        assert!(outcome.pages >= 1, "{kind:?}");
        assert_eq!(
            exchange.print_page_count(project, &job).unwrap(),
            outcome.pages
        );
        pages.insert(kind, outcome.pages);
    }
    // Without long texts much shorter; the Summenblatt shortest.
    assert!(
        pages[&PrintKind::ShortTextLv] * 2 <= pages[&PrintKind::PricedLv],
        "{pages:?}"
    );
    assert!(
        pages[&PrintKind::Summary] <= pages[&PrintKind::ShortTextLv],
        "{pages:?}"
    );

    // Range: only title 02 (with its heading), no notes, no images.
    let title = project
        .lv()
        .rows()
        .iter()
        .find(|row| row.ordinal() == "02")
        .unwrap()
        .id();
    let mut job = PrintJob::new(PrintKind::PricedLv);
    job.titles = vec![title];
    job.settings.notes = false;
    job.settings.images = false;
    job.settings.performance = false;
    let selection = crate::print_selection(&project, &job).unwrap();
    let ordinals: Vec<&str> = selection.lv().rows().iter().map(LvRow::ordinal).collect();
    assert_eq!(ordinals, ["02", "02.01", "02.01.0010"]);
    assert!(selection.images().len() == 1);
    assert!(!selection
        .lv()
        .positions()
        .any(|p| p.long_text().has_images()));
    let outcome = exchange
        .export_print(&project, &job, &folder.join("titel.pdf"))
        .unwrap();
    assert!(outcome.pages < pages[&PrintKind::PricedLv]);

    // A sub title keeps its parent heading.
    let sub = project
        .lv()
        .rows()
        .iter()
        .find(|row| row.ordinal() == "01.10")
        .unwrap()
        .id();
    job.titles = vec![sub];
    let selection = crate::print_selection(&project, &job).unwrap();
    assert_eq!(selection.lv().rows()[0].ordinal(), "01");
    assert_eq!(selection.lv().rows()[1].ordinal(), "01.10");

    // Layout options reach the PDF: landscape, bigger font = more pages.
    let mut job = PrintJob::new(PrintKind::PricedLv);
    job.settings.font_size_pt = 11;
    job.settings.title_page_break = true;
    let bigger = exchange.print_page_count(&project, &job).unwrap();
    assert!(bigger >= pages[&PrintKind::PricedLv]);

    // Deckblatt and logo come with the job (one way for all kinds).
    assert!(PrintJob::new(PrintKind::RequestForQuotation).settings.cover);
    let mut job = PrintJob::new(PrintKind::PricedLv);
    assert!(!job.settings.cover);
    job.settings.cover = true;
    job.logo = Some(proava_application::PdfLogo {
        file_name: "logo.png".into(),
        bytes: tiny_png(),
    });
    let with_cover = exchange.print_page_count(&project, &job).unwrap();
    // The cover takes page 1, the LV starts on page 2.
    assert!(with_cover >= pages[&PrintKind::PricedLv], "{with_cover}");
    let (pdf, _) = crate::render_print(&project, &job, "17.11.2026").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let mut job = PrintJob::new(PrintKind::Contract);
    job.settings.cover = true;
    job.settings.header_text = "Vorabzug".into();
    assert!(exchange.print_page_count(&project, &job).unwrap() >= pages[&PrintKind::Contract]);

    let mut job = PrintJob::new(PrintKind::BidPrices);
    assert!(
        exchange.print_page_count(&project, &job).is_err(),
        "Angebot fehlt"
    );
    job.titles = vec![proava_domain::RowId(9999)];
    job.bid = Some(bid);
    assert!(
        exchange.print_page_count(&project, &job).is_err(),
        "Titel fehlt"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn complements_are_printed_at_their_place() {
    let mut lv = Lv::new("LV", proava_domain::OzMask::parse("11.PPPP").unwrap());
    lv.push_group(proava_domain::LvGroup::new(0, "", "Titel"));
    let mut position = position("", "Gerät", "1", "St", None);
    position.set_long_text(RichText::plain("Farbe ."));
    position.set_complements(vec![
        proava_domain::TextComplement {
            marker: "1".into(),
            kind: proava_domain::ComplementKind::Owner,
            caption: "Farbe".into(),
            body: "reinweiß".into(),
            tail: String::new(),
            anchor: Some(proava_domain::TextAnchor {
                block: 0,
                offset: 6,
                own_line: false,
            }),
        },
        proava_domain::TextComplement {
            marker: "2".into(),
            kind: proava_domain::ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            ..Default::default()
        },
    ]);
    lv.push_position(position);
    lv.renumber();
    let project = Project::new(
        proava_domain::ProjectName::parse("P").unwrap(),
        "2026-11-17",
        lv,
    );
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "17.11.2026");
    let proava_print::PrintItem::Title(title) = &print.items[0] else {
        panic!("{:?}", print.items)
    };
    let proava_print::PrintItem::Position(position) = &title.items[0] else {
        panic!()
    };
    let texts: Vec<String> = position
        .long_text
        .iter()
        .filter_map(|block| match block {
            proava_print::TextBlock::Paragraph { runs, .. } => {
                Some(runs.iter().map(|run| run.text.as_str()).collect())
            }
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"Farbe Farbe reinweiß.".to_string()),
        "{texts:?}"
    );
    assert_eq!(
        texts.last().unwrap(),
        "Fabrikat: ....................................................",
        "Lücke mit 52 Punkten"
    );
}

// ---------- GAEB-Import (Dialog „GAEB ▸ Importieren …“) ----------

/// X81, X82 and X83 in DA XML 3.1, 3.2 and 3.3, D81, D82 and D83: the import
/// reads phase and format, X82/D82 bring their unit prices (optional),
/// X81/X83/D81/D83 stay without prices; order numbers as in the file.
#[test]
fn import_reads_x81_x82_x83_in_every_xml_version_and_d81_d82_d83() {
    use proava_application::gaeb_import::{self, ImportPreview};
    use proava_application::GaebExport;

    let project = demo_project();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-import-formats-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let ordinals = |lv: &Lv| -> Vec<String> {
        lv.positions()
            .map(|position| position.ordinal().to_string())
            .collect()
    };
    let mut checked = Vec::new();
    for (kind, phase) in [
        (GaebExport::Description, 81u8),
        (GaebExport::PricedLv, 82),
        (GaebExport::RequestForQuotation, 83),
    ] {
        let mut files = Vec::new();
        for version in ["3.1", "3.2", "3.3"] {
            let file = folder.join(format!("lv-{version}.X{phase}"));
            exchange.export_gaeb(&project, kind, &file).unwrap();
            if version != "3.3" {
                let text = std::fs::read_to_string(&file).unwrap();
                let older = text
                    .replace(&format!("DA{phase}/3.3"), &format!("DA{phase}/{version}"))
                    .replace(
                        "<Version>3.3</Version>",
                        &format!("<Version>{version}</Version>"),
                    );
                assert_ne!(older, text);
                std::fs::write(&file, older).unwrap();
            }
            files.push((file, format!("GAEB DA XML {version}"), 'X'));
        }
        let datan = folder.join(format!("lv.D{phase}"));
        exchange.export_gaeb(&project, kind, &datan).unwrap();
        files.push((datan, "GAEB 90".to_string(), 'D'));

        for (file, format, letter) in files {
            let import = gaeb_import::read(&exchange, &file, "2026-11-17T22:00:00").unwrap();
            let preview = ImportPreview::of(&import);
            let name = file.display().to_string();
            assert_eq!(import.read.phase, Some(phase), "{name}");
            assert_eq!(preview.format, format, "{name}");
            assert!(
                preview.kind.starts_with(&format!("{letter}{phase} – ")),
                "{name}: {}",
                preview.kind
            );
            assert_eq!(preview.positions, project.lv().position_count(), "{name}");
            assert!(!preview.is_bid(), "{name}");
            assert_eq!(
                ordinals(import.read.project.lv()),
                ordinals(project.lv()),
                "{name}"
            );
            if phase == 82 {
                assert!(preview.has_prices(), "{name}");
                assert_eq!(preview.net_total, project.lv().net_total(), "{name}");
            } else {
                assert!(!preview.has_prices(), "{name}: X81/X83 without prices");
                assert_eq!(preview.net_total, Money::ZERO, "{name}");
            }

            // All three ways: new LV (with and without prices), new project.
            let mut target = Project::new(
                ProjectName::parse("Ziel").unwrap(),
                "2026-11-17",
                Lv::new("Vorhanden", OzMask::parse("11.PPPP").unwrap()),
            );
            target.lv_mut().set_number("1");
            let number = preview.suggested_number(&target);
            let (index, protocol) = gaeb_import::add_as_lv(
                &mut target,
                &import,
                &number,
                &preview.suggested_title(),
                preview.has_prices(),
            )
            .unwrap();
            let lv = target.lv_at(index).unwrap();
            assert_eq!(ordinals(lv), ordinals(project.lv()), "{name}");
            assert_eq!(lv.net_total(), preview.net_total, "{name}");
            let text = protocol.text();
            assert!(
                text.contains(&format!("{letter}{phase} – ")),
                "{name}: {text}"
            );
            if phase == 82 {
                assert!(text.contains("Einheitspreise als Planerpreise\n"), "{text}");
                let (index, protocol) =
                    gaeb_import::add_as_lv(&mut target, &import, "ohne", "Ohne Preise", false)
                        .unwrap();
                assert_eq!(target.lv_at(index).unwrap().net_total(), Money::ZERO);
                assert!(
                    protocol.text().contains("nicht übernommen"),
                    "{}",
                    protocol.text()
                );
            } else {
                assert!(text.contains("Preise: keine in der Datei\n"), "{text}");
            }
            let copy = gaeb_import::as_project(&import, "Kopie", preview.has_prices()).unwrap();
            assert_eq!(copy.name().as_str(), "Kopie");
            assert_eq!(ordinals(copy.lv()), ordinals(project.lv()), "{name}");
            checked.push(format!("{letter}{phase} {format}"));
        }
    }
    assert_eq!(checked.len(), 12, "{checked:?}");
    std::fs::remove_dir_all(&folder).ok();
}

/// A supplied description as manufacturers send it: DA XML 3.1, phase 81,
/// no quantities, no prices, formatted long text, a Vorbemerkung – also
/// inside a zip. Nothing of the text is lost, every position is „offen“.
#[test]
fn a_supplied_x81_without_quantities_keeps_its_texts_also_from_a_zip() {
    use std::io::Write as _;

    use proava_application::gaeb_import::{self, ImportPreview};

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA81/3.1">
  <GAEBInfo><Version>3.1</Version><VersDate>2009-05</VersDate><Date>2026-11-01</Date><ProgSystem>Herstellerportal</ProgSystem><ProgName>Ausschreibungstexte</ProgName></GAEBInfo>
  <PrjInfo><NamePrj>Vortexte</NamePrj><LblPrj>Ausschreibungstexte Raumautomation</LblPrj><Cur>EUR</Cur><CurLbl>Euro</CurLbl></PrjInfo>
  <Award>
    <DP>81</DP>
    <AddText><OutlineAddText><OutlTxt><TextOutlTxt><p><span>Allgemeines</span></p></TextOutlTxt></OutlTxt></OutlineAddText>
      <DetailAddText><Text><p><span>Die Texte gelten für alle Geräte der Serie.</span></p></Text></DetailAddText></AddText>
    <BoQ>
      <BoQInfo><Name>RA</Name><LblBoQ>Raumautomation</LblBoQ>
        <BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn>
        <BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn>
      </BoQInfo>
      <BoQBody>
        <BoQCtgy RNoPart="01">
          <LblTx><p><span>Raumbediengeräte</span></p></LblTx>
          <BoQBody><Itemlist>
            <Item RNoPart="0010">
              <QU>St</QU>
              <Description><CompleteText>
                <DetailTxt><Text>
                  <p><span>Raumbediengerät mit Display, </span><span style="font-weight:bold;">KNX</span><span>, für Unterputzdose.</span></p>
                  <p><span>Farbe: reinweiß</span></p>
                </Text></DetailTxt>
                <OutlineText><OutlTxt><TextOutlTxt><p><span>Raumbediengerät KNX</span></p></TextOutlTxt></OutlTxt></OutlineText>
              </CompleteText></Description>
            </Item>
            <Item RNoPart="0020">
              <QU>St</QU>
              <Description><CompleteText>
                <DetailTxt><Text><p><span>Präsenzmelder Decke, 360 Grad.</span></p></Text></DetailTxt>
                <OutlineText><OutlTxt><TextOutlTxt><p><span>Präsenzmelder</span></p></TextOutlTxt></OutlTxt></OutlineText>
              </CompleteText></Description>
            </Item>
          </Itemlist></BoQBody>
        </BoQCtgy>
      </BoQBody>
    </BoQ>
  </Award>
</GAEB>
"#;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-import-supplied-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("Hersteller.X81");
    std::fs::write(&file, xml).unwrap();
    let archive = folder.join("Lieferung.zip");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        zip.start_file(
            "Texte/Hersteller.X81",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    for source in [&file, &archive] {
        let import = gaeb_import::read(&GaebPdfExchange, source, "2026-11-17T22:00:00").unwrap();
        let preview = ImportPreview::of(&import);
        assert_eq!(preview.kind, "X81 – Leistungsbeschreibung");
        assert_eq!(preview.format, "GAEB DA XML 3.1");
        assert_eq!(preview.program, "Ausschreibungstexte");
        assert_eq!(
            (preview.titles, preview.positions, preview.preliminary),
            (1, 2, 1)
        );
        assert_eq!(preview.without_quantity, 2);
        assert!(preview.is_description() && !preview.has_prices());
        assert_eq!(preview.suggested_title(), "Raumautomation");
        let mut target = Project::new(
            ProjectName::parse("Ziel").unwrap(),
            "2026-11-17",
            Lv::new("GA", OzMask::parse("11.PPPP").unwrap()),
        );
        let (index, protocol) =
            gaeb_import::add_as_lv(&mut target, &import, "RA", "Raumautomation", false).unwrap();
        let lv = target.lv_at(index).unwrap();
        let device = lv.positions().next().unwrap();
        assert_eq!(device.ordinal(), "01.0010");
        assert_eq!(device.short_text(), "Raumbediengerät KNX");
        assert_eq!(device.status(), proava_domain::EditStatus::Open);
        let bold: Vec<&str> = device
            .long_text()
            .blocks()
            .iter()
            .flat_map(|block| match block {
                TextBlock::Paragraph { runs, .. } => runs.as_slice(),
                _ => &[],
            })
            .filter(|run| run.bold)
            .map(|run| run.text.as_str())
            .collect();
        assert_eq!(bold, ["KNX"], "formatting kept");
        assert!(device.long_text().to_plain().contains("Farbe: reinweiß"));
        let text = protocol.text();
        assert!(text.contains("\nDatei: "), "{text}");
        assert!(
            text.contains("Bearbeitungsstatus: alle Positionen „offen“"),
            "{text}"
        );
        if source == &archive {
            assert!(
                text.contains("\nIm Archiv: Texte/Hersteller.X81\n"),
                "{text}"
            );
        }
    }
    std::fs::remove_dir_all(&folder).ok();
}

/// Gefunden mit den Eigenschaftstests: was das Format nicht tragen kann,
/// ging still verloren – der Bezug auf eine Ausführungsbeschreibung bei
/// einer Wiederholung (GAEB kennt je Position nur einen Bezug) und
/// „pauschal“ bei einer anderen Einheit als „psch“ in GAEB 90.
#[test]
fn export_warns_about_references_and_lump_sums_the_format_drops() {
    let mut project = project_with_performance();
    let lv = project.lv_mut();
    let base = lv.position_by_ordinal("01.10.0010").unwrap().id();
    let repeated = lv.position_by_ordinal("01.01.0010").unwrap().id();
    lv.link_repetition(repeated, Some(base)).unwrap();
    let lump = lv.position_by_ordinal("02.01.0010").unwrap().id();
    let index = lv.index_of(lump).unwrap();
    let position = lv.row_mut(index).and_then(LvRow::as_position_mut).unwrap();
    position.set_lump_sum(true);
    position.set_unit("St");

    let (_, warnings) = lv_to_gaeb(&project, Phase::Dp83);
    assert!(has(&warnings, "nur einen Bezug"), "{warnings:?}");
    assert!(has(&warnings, "01.01.0010"), "{warnings:?}");

    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-verlust-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let exchange = GaebPdfExchange;
    let datan = exchange
        .export_gaeb(
            &project,
            proava_application::GaebExport::RequestForQuotation,
            &folder.join("lv.D83"),
        )
        .unwrap();
    assert!(has(&datan, "„psch“"), "{datan:?}");
    assert!(has(&datan, "02.01.0010"), "{datan:?}");
    let xml = exchange
        .export_gaeb(
            &project,
            proava_application::GaebExport::RequestForQuotation,
            &folder.join("lv.X83"),
        )
        .unwrap();
    assert!(!has(&xml, "„psch“"), "XML trägt „pauschal“: {xml:?}");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn cost_groups_travel_through_gaeb() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let title = lv.rows().iter().position(|row| row.is_group()).unwrap();
    if let Some(proava_domain::LvRow::Group(group)) = lv.row_mut(title) {
        group.set_cost_group(Some("481".into()));
    }
    let position = lv.rows().iter().rposition(|row| row.is_position()).unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(position) {
        position.set_cost_group(Some("484".into()));
    }
    for phase in [Phase::Dp82, Phase::Dp83] {
        let (doc, _) = lv_to_gaeb(&project, phase);
        assert_eq!(doc.award.boq.catalogs.len(), 1);
        let text = gaeb::xml::write(&doc, phase).unwrap();
        assert!(text.contains(
            "<CtlgAssign><CtlgID>idDIN276-18</CtlgID><CtlgCode>481</CtlgCode></CtlgAssign>"
        ));
        let read = gaeb::xml::read(text.as_bytes()).unwrap();
        let (back, _) = gaeb_to_project(
            &read,
            ProjectName::parse("KG").unwrap(),
            "2026-11-17T19:00:00",
        );
        let lv = back.lv();
        assert_eq!(
            lv.rows()[title].as_group().unwrap().cost_group(),
            Some("481")
        );
        assert_eq!(
            lv.positions().last().unwrap().cost_group(),
            Some("484"),
            "{phase:?}"
        );
        assert_eq!(lv.cost_catalog().name, "DIN 276-18");
    }
    // Ohne Kostengruppen kein Katalog in der Datei.
    let (doc, _) = lv_to_gaeb(&demo_project(), Phase::Dp83);
    assert!(doc.award.boq.catalogs.is_empty());
}

/// Bilder im GAEB-Export: mit „Bilder mitgeben“ (Vorgabe) in GAEB DA XML,
/// ausgeschaltet nicht; GAEB 90 kann keine und warnt.
#[test]
fn images_go_into_gaeb_xml_when_wanted() {
    use proava_application::{GaebExport, GaebExportOptions};
    let mut project = demo_project();
    assert_eq!(project.add_image("schema.png", tiny_png()), "schema.png");
    let (count, bytes) = project.lv_images();
    assert_eq!(count, 1);
    assert!(bytes > 0);
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-bilder-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let exchange = GaebPdfExchange;
    let kind = GaebExport::RequestForQuotation;

    let with = folder.join("mit.X83");
    let warnings = exchange.export_gaeb(&project, kind, &with).unwrap();
    assert!(
        has(&warnings, "1 Bild aus den Langtexten mitgegeben"),
        "{warnings:?}"
    );
    let text = std::fs::read_to_string(&with).unwrap();
    assert_eq!(text.matches("<image ").count(), 1, "Vorgabe: Bilder mit");
    let read = gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("B").unwrap(), "");
    assert_eq!(back.images().len(), 1);
    assert_eq!(
        back.images()[0].bytes,
        project.images()[0].bytes,
        "Bild unverändert"
    );

    let without = folder.join("ohne.X83");
    let warnings = exchange
        .export_gaeb_with(
            &project,
            kind,
            &without,
            GaebExportOptions { images: false },
        )
        .unwrap();
    assert!(has(&warnings, "nicht mitgegeben"), "{warnings:?}");
    assert!(!std::fs::read_to_string(&without)
        .unwrap()
        .contains("<image"));

    let datan = exchange
        .export_gaeb(&project, kind, &folder.join("lv.D83"))
        .unwrap();
    assert!(has(&datan, "GAEB 90 kann keine Bilder"), "{datan:?}");
    std::fs::remove_dir_all(&folder).unwrap();
}

// ------------------------------------------------- Fachregeln (R1–R13) ----

/// Errors and warnings except R13: the demo positions have no long texts,
/// which e-Vergabe platforms reject (R13.long_text).
fn serious(findings: &[crate::RuleFinding]) -> Vec<String> {
    findings
        .iter()
        .filter(|finding| finding.severity != crate::RuleSeverity::Hint && finding.code() != "R13")
        .map(|finding| finding.to_string())
        .collect()
}

fn position_index(project: &Project, ordinal: &str) -> usize {
    project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == ordinal)
        .unwrap()
}

#[test]
fn export_check_applies_the_fachregeln_to_the_written_file() {
    use proava_application::GaebExport;
    let project = demo_project();
    for kind in [
        GaebExport::Description,
        GaebExport::RequestForQuotation,
        GaebExport::PricedLv,
    ] {
        let findings = crate::export_rule_findings(&project, kind);
        assert!(
            serious(&findings).is_empty(),
            "{kind:?}: {:#?}",
            serious(&findings)
        );
    }

    // FD 3.3 11.2.3 Item 7: no Mengeneinheit outside X84.
    let mut broken = demo_project();
    let index = position_index(&broken, "01.01.0010");
    if let Some(LvRow::Position(position)) = broken.lv_mut().row_mut(index) {
        position.set_unit("");
    }
    let findings = crate::export_rule_findings(&broken, GaebExport::RequestForQuotation);
    let unit: Vec<_> = findings.iter().filter(|f| f.id == "R9.unit").collect();
    assert_eq!(unit.len(), 1, "{findings:#?}");
    assert_eq!(unit[0].ordinal.as_deref(), Some("01.01.0010"));
    assert_eq!(unit[0].severity, crate::RuleSeverity::Error);
}

#[test]
fn export_names_rule_violations_of_the_file_it_wrote() {
    use proava_application::GaebExport;
    let mut project = demo_project();
    let index = position_index(&project, "01.01.0010");
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
        position.set_unit("");
    }
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-rules-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["lv.X83", "lv.D83"] {
        let file = folder.join(name);
        let warnings = GaebPdfExchange
            .export_gaeb(&project, GaebExport::RequestForQuotation, &file)
            .unwrap();
        assert!(
            has(
                &warnings,
                "Fachregel R9 (Fehler): OZ 01.01.0010 ohne Mengeneinheit"
            ),
            "{name}: {warnings:?}"
        );
    }
    let clean = folder.join("sauber.X83");
    let warnings = GaebPdfExchange
        .export_gaeb(&demo_project(), GaebExport::RequestForQuotation, &clean)
        .unwrap();
    assert!(
        !warnings
            .iter()
            .any(|w| w.starts_with("Fachregel") && !w.starts_with("Fachregel R13")),
        "{warnings:?}"
    );
    assert!(has(
        &warnings,
        "Fachregel R13 (Warnung): OZ 01.01.0020 ohne Langtext"
    ));
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn bid_import_shows_r11_against_the_request() {
    let project = demo_project();
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-r11-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    doc.award.bidder = Some(gaeb::Address {
        name1: "Muster Automation GmbH".into(),
        ..gaeb::Address::default()
    });
    let clean = folder.join("sauber.X84");
    std::fs::write(&clean, gaeb::xml::write(&doc, Phase::Dp84).unwrap()).unwrap();
    let import = GaebPdfExchange.import_bid(&project, &clean).unwrap();
    assert!(!has(&import.warnings, "Fachregel"), "{:?}", import.warnings);

    // The bidder changed the OZ mask and filled in a complement the
    // request does not have.
    doc.award.boq.breakdown[1].length = 3;
    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.rno_part == "0010" {
            item.complements.push(gaeb::TextComplement {
                marker: "7".into(),
                kind: "Bidder".into(),
                body: "Fabrikat X".into(),
                ..gaeb::TextComplement::default()
            });
        }
    });
    let changed = folder.join("geaendert.X84");
    std::fs::write(&changed, gaeb::xml::write(&doc, Phase::Dp84).unwrap()).unwrap();
    let import = GaebPdfExchange.import_bid(&project, &changed).unwrap();
    assert!(
        has(
            &import.warnings,
            "Fachregel R11 (Fehler): Die OZ-Maske des Angebots"
        ),
        "{:?}",
        import.warnings
    );
    assert!(
        has(
            &import.warnings,
            "Textergänzung 7 ist in der Angebotsaufforderung nicht vorgesehen"
        ),
        "{:?}",
        import.warnings
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn work_categories_travel_through_gaeb_next_to_cost_groups() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let position = lv.rows().iter().rposition(|row| row.is_position()).unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(position) {
        position.set_cost_group(Some("484".into()));
        position.set_work_category(Some("070".into()));
    }
    let first = lv.rows().iter().position(|row| row.is_position()).unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(first) {
        position.set_work_category(Some("MSR Technik".into()));
    }
    let title = lv.rows().iter().position(|row| row.is_group()).unwrap();
    if let Some(proava_domain::LvRow::Group(group)) = lv.row_mut(title) {
        group.set_work_category(Some("070".into()));
    }
    for phase in [Phase::Dp81, Phase::Dp83, Phase::Dp86] {
        let (doc, _) = lv_to_gaeb(&project, phase);
        assert_eq!(
            doc.award.boq.catalogs.len(),
            2,
            "DIN 276 und Leistungsbereiche"
        );
        let text = gaeb::xml::write(&doc, phase).unwrap();
        assert!(
            text.contains("<Ctlg><CtlgID>idLB</CtlgID><CtlgType>work category</CtlgType><CtlgName>Leistungsbereiche</CtlgName></Ctlg>"),
            "{phase:?}"
        );
        // Schema check by hand (no process from tests): write the file and run
        // crates/proava-gaeb/scripts/validate_xsd.py – checked 17./18.11.2026.
        assert!(
            text.contains("<CtlgAssign><CtlgID>idLB</CtlgID><CtlgCode>070</CtlgCode></CtlgAssign>")
        );
        let read = gaeb::xml::read(text.as_bytes()).unwrap();
        let (back, _) = gaeb_to_project(
            &read,
            ProjectName::parse("LB").unwrap(),
            "2026-11-17T23:00:00",
        );
        let lv = back.lv();
        let last = lv.positions().last().unwrap();
        assert_eq!(last.work_category(), Some("070"), "{phase:?}");
        assert_eq!(last.cost_group(), Some("484"), "KG bleibt daneben");
        assert_eq!(
            lv.positions().next().unwrap().work_category(),
            Some("MSR Technik")
        );
        assert_eq!(
            lv.rows()[title].as_group().unwrap().work_category(),
            Some("070"),
            "Titel"
        );
        assert_eq!(lv.work_catalog().name, "Leistungsbereiche");
    }
    // Ohne Leistungsbereiche kein Katalog dafür.
    let (doc, _) = lv_to_gaeb(&demo_project(), Phase::Dp83);
    assert!(!doc
        .award
        .boq
        .catalogs
        .iter()
        .any(|catalog| catalog.is_work_categories()));
}

#[test]
fn page_frame_comes_from_the_project_or_the_settings_with_project_values() {
    use proava_application::{PrintJob, PrintKind};
    let mut lv = Lv::new(
        "Gebäudeautomation",
        proava_domain::OzMask::parse("11.PPPP").unwrap(),
    );
    lv.set_number("480");
    lv.push_group(proava_domain::LvGroup::new(0, "", "Titel"));
    lv.push_position(position("", "Gerät", "1", "St", Some("10")));
    lv.renumber();
    let mut project = Project::new(
        ProjectName::parse("Rahmen").unwrap(),
        "2026-11-17T20:00:00",
        lv,
    );
    project.info_mut().award_number = "V-15".into();
    project.info_mut().short_name = "VG-14".into();
    project.info_mut().owner.name1 = "Stadt Musterstadt".into();

    let mut settings_frame = proava_domain::PageFrame::default();
    settings_frame.header[0] = "{vergabenummer}".into();
    let frame = crate::page_frame_to_print(
        &project,
        &settings_frame,
        "17.11.2026",
        &[("datei", "a.pdf".into())],
    );
    assert_eq!(
        frame.header[0], "{vergabenummer}",
        "ohne eigene: die aus den Einstellungen"
    );
    let value = |key: &str| {
        frame
            .values
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, text)| text.clone())
    };
    assert_eq!(value("vergabenummer").as_deref(), Some("V-15"));
    assert_eq!(value("projektnummer").as_deref(), Some("VG-14"));
    assert_eq!(value("lv_nr").as_deref(), Some("480"));
    assert_eq!(value("auftraggeber").as_deref(), Some("Stadt Musterstadt"));
    assert_eq!(
        value("datei").as_deref(),
        Some("a.pdf"),
        "Wert des Ausdrucks gewinnt"
    );

    let mut own = proava_domain::PageFrame::default();
    own.footer[2] = "S. {seite}".into();
    own.logo = proava_domain::FrameLogoPlace::Right;
    project.info_mut().page_frame = Some(own);
    let frame = crate::page_frame_to_print(&project, &settings_frame, "17.11.2026", &[]);
    assert_eq!(frame.header[0], "{projekt}", "eigene des Projekts gewinnt");
    assert_eq!(frame.footer[2], "S. {seite}");
    assert_eq!(frame.logo, proava_print::FrameLogo::Right);

    for kind in [
        PrintKind::RequestForQuotation,
        PrintKind::PricedLv,
        PrintKind::Summary,
        PrintKind::CostGroups,
    ] {
        let mut job = PrintJob::new(kind);
        job.frame = settings_frame.clone();
        job.file_name = "Rahmen.pdf".into();
        let (pdf, outcome) = crate::render_print(&project, &job, "17.11.2026").unwrap();
        assert!(pdf.starts_with(b"%PDF") && outcome.pages >= 1, "{kind:?}");
    }
}

#[test]
fn placeholder_names_agree_between_domain_and_print() {
    let domain: Vec<&str> = proava_domain::PAGE_PLACEHOLDERS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(domain, proava_print::PLACEHOLDER_NAMES);
    assert_eq!(
        proava_domain::DEFAULT_PAGE_HEADER,
        proava_print::DEFAULT_HEADER
    );
    assert_eq!(
        proava_domain::DEFAULT_PAGE_FOOTER,
        proava_print::DEFAULT_FOOTER
    );
}

/// Positionsübergabe wie aus STLB-Bau (syn5: Zahlenwerte, Ansprache mit
/// Leerzeichen, Nachsatz mit Zeilenumbruch, TA und TB): Übernahme ins LV und
/// Export X83 geben Kennung und Textergänzungen zeichengleich weiter – das
/// prüft STLB-Bau Check bei öffentlichen Auftraggebern.
#[test]
fn stlb_text_complements_survive_take_over_and_export() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../proava-gaeb/tests/data/syn5_stlb_te_33.X83");
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    assert_eq!((read.positions.len(), read.with_stlb_key), (2, 2));
    let first = read.positions[0].stlb_reference().unwrap();
    assert_eq!(
        first.complement_numbers,
        vec![("11".to_string(), proava_domain::ComplementNumber::Decimal)]
    );
    assert_eq!(first.cost_group, "482");
    assert_eq!(
        read.positions[0].catalog_line().as_deref(),
        Some("STLB-Bau 4/2026 070 TA TB")
    );
    assert!(proava_domain::stlb::stlb_problems(&read.positions[0]).is_empty());

    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    for position in &read.positions {
        let mut fresh = Position::new("", "", Decimal::ZERO, "", None);
        proava_domain::stlb::fill_from_catalog(&mut fresh, position);
        lv.push_position(fresh);
    }
    lv.renumber();
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let written = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let original = std::fs::read_to_string(&file).unwrap();
    let complements = |text: &str| -> Vec<String> {
        text.split("<TextComplement ")
            .skip(1)
            .map(|part| {
                let end = part.find("</TextComplement>").unwrap();
                // Hochkomma als Entität oder Zeichen – beides ist dasselbe.
                part[..end].replace("&#x27;", "'")
            })
            .collect()
    };
    assert_eq!(complements(&written), complements(&original));
    let keys = |text: &str| -> Vec<String> {
        text.split("<STLBBau>")
            .skip(1)
            .map(|part| part[..part.find("</STLBBau>").unwrap()].replace('\n', ""))
            .collect()
    };
    assert_eq!(keys(&written), keys(&original));
}

/// Herstellertexte wie aus einem Textportal (syn6, Hersteller und Typen
/// ausgedacht): nur Positionen, Text unverändert („oder gleichwertig“,
/// Bieterangabe), Herkunft aus „Hersteller:“/„Fabrikat:“ oder dem LV-Namen,
/// Artikelnummer, Datum und Dateiname; die Quelle bleibt der Programmname der
/// Datei (der Dialog setzt seine Quelle ein, wenn die Datei keine bekannte nennt).
#[test]
fn manufacturer_texts_carry_their_origin() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn6_herstellertext_32.X81");
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    assert_eq!((read.positions.len(), read.with_stlb_key), (3, 0));
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    let first = &read.positions[0];
    assert_eq!(first.short_text(), "Drehantrieb 20 Nm, stetig");
    assert!(first.long_text().to_plain().contains("oder gleichwertig"));
    assert_eq!(first.complements().len(), 1, "Bieterangabe bleibt");
    let origin = first.text_origin().expect("Herkunft");
    assert_eq!(origin.source, "proAVA Testdaten");
    assert_eq!(origin.supplier, "Muster Antriebe GmbH");
    assert_eq!(origin.reference, "MA-20-SR");
    assert_eq!(origin.file, "syn6_herstellertext_32.X81");
    assert_eq!(
        origin.retrieved.len(),
        10,
        "Tag der Datei: {}",
        origin.retrieved
    );
    assert_eq!(
        read.positions[1].text_origin().unwrap().supplier,
        "Muster Antriebe GmbH, Typ MA-10-F"
    );
    let third = read.positions[2].text_origin().unwrap();
    assert_eq!(third.supplier, "Muster Antriebe GmbH", "aus dem LV-Namen");
    assert_eq!(third.reference, "");
    assert_eq!(read.positions[2].quantity(), Decimal::ZERO);
}

/// Befund 3: GAEB OWN carries the Auftraggeber; without an own one the
/// Bauherr, as before.
#[test]
fn gaeb_owner_is_the_client_and_falls_back_to_the_building_owner() {
    let mut project = demo_project();
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    assert_eq!(
        doc.award.owner.as_ref().unwrap().name1,
        "Stadt Düsseldorf",
        "wie Bauherr"
    );

    project.info_mut().own_client = Some(Address {
        name1: "IDR Vergabestelle".into(),
        city: "Düsseldorf".into(),
        ..Address::default()
    });
    let (doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    assert_eq!(
        doc.award.owner.as_ref().unwrap().name1,
        "IDR Vergabestelle",
        "eigener Auftraggeber"
    );
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "17.11.2026");
    assert_eq!(
        print.header.client, "IDR Vergabestelle, Düsseldorf",
        "Kopfzeile der Ausdrucke"
    );
}

/// Positions of the print model, depth first.
fn print_positions(items: &[proava_print::PrintItem]) -> Vec<&proava_print::PrintPosition> {
    let mut out = Vec::new();
    for item in items {
        match item {
            proava_print::PrintItem::Title(title) => out.extend(print_positions(&title.items)),
            proava_print::PrintItem::Position(position) => out.push(position),
            _ => {}
        }
    }
    out
}

#[test]
fn print_model_carries_the_takeoff_of_a_position() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let index = lv.rows().iter().position(|row| row.is_position()).unwrap();
    let ordinal = lv.rows()[index].ordinal().to_string();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(index) {
        position.set_takeoff(vec![
            proava_domain::TakeoffLine {
                label: " EG Strang 1 ".into(),
                formula: "2*12,5".into(),
                invoice: None,
            },
            proava_domain::TakeoffLine {
                label: "OG".into(),
                formula: "3+".into(),
                invoice: None,
            },
        ]);
    }
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Priced, "18.11.2026");
    let positions = print_positions(&print.items);
    let position = positions
        .iter()
        .find(|position| position.oz == ordinal)
        .unwrap();
    assert_eq!(position.takeoff.len(), 2);
    assert_eq!(position.takeoff[0].label, "EG Strang 1");
    assert_eq!(position.takeoff[0].result, Some("25".parse().unwrap()));
    assert_eq!(position.takeoff[1].result, None, "falscher Rechenansatz");
    assert_eq!(position.takeoff_sum(), Some("25".parse().unwrap()));
    assert!(positions
        .iter()
        .filter(|other| other.oz != ordinal)
        .all(|other| other.takeoff.is_empty() && other.takeoff_sum().is_none()));
}

#[test]
fn author_from_the_settings_or_the_planner_fills_ersteller_and_bearbeiter() {
    let mut project = demo_project();
    project.info_mut().planner.name1 = "Planungsbüro Beispiel".into();
    let value = |frame: &proava_print::PrintFrame, key: &str| {
        frame
            .values
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
            .unwrap()
    };
    let default = proava_domain::PageFrame::default();
    let frame = crate::page_frame_to_print(
        &project,
        &default,
        "17.11.2026",
        &[("ersteller", String::new()), ("bearbeiter", String::new())],
    );
    assert_eq!(value(&frame, "ersteller"), "Planungsbüro Beispiel");
    assert_eq!(value(&frame, "bearbeiter"), "Planungsbüro Beispiel");
    let frame = crate::page_frame_to_print(
        &project,
        &default,
        "17.11.2026",
        &[
            ("ersteller", "M. Muster".into()),
            ("bearbeiter", "M. Muster".into()),
        ],
    );
    assert_eq!(value(&frame, "ersteller"), "M. Muster");
    assert_eq!(value(&frame, "planer"), "Planungsbüro Beispiel");
    let mut job = proava_application::PrintJob::new(proava_application::PrintKind::PricedLv);
    job.author = "M. Muster".into();
    let (pdf, _) = crate::render_print(&project, &job, "17.11.2026").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn takeoff_is_printed_only_with_the_option_and_for_the_lv_kinds() {
    use proava_application::{PrintJob, PrintKind};
    let mut project = demo_project();
    let lv = project.lv_mut();
    let index = lv.rows().iter().position(|row| row.is_position()).unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(index) {
        position.set_takeoff(
            (0..150)
                .map(|number| proava_domain::TakeoffLine {
                    label: format!("Raum {number}"),
                    formula: "1".into(),
                    invoice: None,
                })
                .collect(),
        );
    }
    let pages = |kind: PrintKind, takeoff: bool| {
        let mut job = PrintJob::new(kind);
        job.settings.takeoff = takeoff;
        crate::render_print(&project, &job, "18.11.2026")
            .unwrap()
            .1
            .pages
    };
    // 150 Zeilen brauchen mehr als eine Seite: mehr Seiten nur mit der Option (die
    // Mengenermittlung bricht um, statt als Block auf eine neue Seite zu springen).
    assert!(pages(PrintKind::PricedLv, true) > pages(PrintKind::PricedLv, false));
    assert!(pages(PrintKind::ShortTextLv, true) > pages(PrintKind::ShortTextLv, false));
    assert_eq!(
        pages(PrintKind::RequestForQuotation, true),
        pages(PrintKind::RequestForQuotation, false),
        "Angebotsaufforderung druckt keine Mengenermittlung"
    );
}

/// Hinweistext mit STLB-Bau-Kennung und Textergänzung (Standardbeschreibung):
/// GAEB → Projekt → GAEB zeichengleich, im Druck Katalogzeile und Ergänzung.
#[test]
fn stlb_remarks_keep_key_and_complements_through_the_project() {
    let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3">
<GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-11-17</Date><ProgName>Test</ProgName></GAEBInfo>
<PrjInfo><NamePrj>H</NamePrj><LblPrj>Hinweise</LblPrj><Cur>EUR</Cur><CurLbl>Euro</CurLbl></PrjInfo>
<Award><DP>83</DP><AwardInfo><Cur>EUR</Cur><CurLbl>Euro</CurLbl></AwardInfo>
<BoQ ID="B1"><BoQInfo><Name>H</Name><LblBoQ>H</LblBoQ><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo>
<BoQBody><BoQCtgy ID="T1" RNoPart="01"><LblTx><p><span>GA</span></p></LblTx><BoQBody><Itemlist>
<Remark ID="R1"><Description><STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName><VersDate>2026-04</VersDate><WCtg>070</WCtg><Group>9201</Group></STLBBauCtlg><STLBBauID><ArtChrIdent>-92012</ArtChrIdent></STLBBauID></STLBBau><CompleteText><ComplTSA>Yes</ComplTSA><DetailTxt><Text><span>Managementfunktionen,</span><br/><span/></Text><TextComplement MarkLbl="11" Kind="Owner" ArtChrIdent="-92012"><ComplCaption><span>Anzahl Bedienplätze </span></ComplCaption><ComplBodyInt Value="2"/><ComplBody><span>'2'</span></ComplBody><ComplTail><span>, </span></ComplTail></TextComplement><Text><span>Einbau in Schaltschrank.</span></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Managementebene</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Remark>
<Item ID="I1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description><OutlineText><OutlTxt><TextOutlTxt><span>Danach</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>
</Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>"#;
    let doc = gaeb::xml::read(source.as_bytes()).unwrap();
    let (project, _) = gaeb_to_project(&doc, ProjectName::parse("H").unwrap(), "");
    let note = project
        .lv()
        .rows()
        .iter()
        .find_map(|row| match row {
            LvRow::Note(note) => Some(note),
            _ => None,
        })
        .expect("Hinweis");
    assert_eq!(note.complements().len(), 1);
    assert_eq!(note.complements()[0].body, "2");
    assert_eq!(
        note.catalog_line().as_deref(),
        Some("STLB-Bau 4/2026 070 TA")
    );
    let stlb = note.catalog().stlb.as_ref().unwrap();
    assert_eq!(stlb.complement_links, vec![("11".to_string(), -92012)]);

    let (again, _) = lv_to_gaeb(&project, Phase::Dp83);
    let written = gaeb::xml::write(&again, Phase::Dp83).unwrap();
    let remark_of = |text: &str| -> String {
        let start = text.find("<Remark").unwrap();
        let start = start + text[start..].find("<Description>").unwrap();
        let end = text.find("</Remark>").unwrap();
        // Einrückung zwischen den Elementen und leere spans zählen nicht.
        let mut compact = String::new();
        for piece in text[start..end]
            .replace("&#x27;", "'")
            .replace("<span/>", "")
            .split('<')
        {
            if !compact.is_empty() || !piece.is_empty() {
                compact.push('<');
                let close = piece.find('>').map(|i| i + 1).unwrap_or(piece.len());
                let rest = &piece[close..];
                compact.push_str(&piece[..close]);
                if !rest.trim().is_empty() {
                    compact.push_str(rest);
                }
            }
        }
        compact
    };
    assert_eq!(remark_of(&written), remark_of(source));

    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "17.11.2026");
    let note_text = format!("{:?}", print.items);
    assert!(note_text.contains("STLB-Bau 4/2026 070 TA"), "{note_text}");
    assert!(note_text.contains("Anzahl Bedienplätze"), "{note_text}");
}

/// Awarded project with the contract taken over and two cumulative
/// Abschlagsrechnungen (the second with a reduced quantity).
fn billing_project() -> (Project, proava_domain::InvoiceId, proava_domain::RowId) {
    use proava_domain::{Invoice, InvoiceItem, InvoiceKind, VatRule};
    let (mut project, _) = awarded_project();
    project.take_over_contract().unwrap();
    let contract = project.contract().unwrap();
    let (row, item) = contract
        .items
        .iter()
        .find(|(_, item)| item.quantity > Decimal::from(2) && !item.lump_sum)
        .map(|(row, item)| (*row, item.clone()))
        .expect("Position mit Menge");
    let mut first = Invoice::new(InvoiceKind::Partial, "AR-1", "28.02.2027");
    first.vat = VatRule::Rate(Decimal::from(19));
    first.items.insert(
        row,
        InvoiceItem {
            claimed_quantity: Some(Decimal::ONE),
            ..InvoiceItem::default()
        },
    );
    project.billing_mut().add_invoice(first);
    let mut second = Invoice::new(InvoiceKind::Partial, "AR-2", "30.04.2027");
    second.vat = VatRule::Rate(Decimal::from(19));
    second.items.insert(
        row,
        InvoiceItem {
            claimed_quantity: Some(item.quantity),
            checked_quantity: Some(Decimal::from(2)),
            note: "Rest nicht montiert".into(),
            ..InvoiceItem::default()
        },
    );
    let id = project.billing_mut().add_invoice(second);
    (project, id, row)
}

#[test]
fn billing_check_and_release_follow_the_check() {
    let (project, id, row) = billing_project();
    let context = project.billing_context().unwrap();
    let check = project.billing().check(&context, id).unwrap();

    let report = crate::billing_check_to_print(&project, Some(id), "05.05.2027").unwrap();
    assert!(
        report
            .invoice
            .starts_with("2. Abschlagsrechnung AR-2 vom 30.04.2027"),
        "{}",
        report.invoice
    );
    let ordinal = project.lv().row_by_id(row).unwrap().ordinal().to_string();
    let position = report
        .rows
        .iter()
        .find_map(|r| match r {
            proava_print::PrintBillingRow::Position(p) if p.ordinal == ordinal => Some(p),
            _ => None,
        })
        .expect("Position in der Aufmaßprüfung");
    assert_eq!(position.checked, Decimal::from(2));
    assert_eq!(position.previous, Decimal::ONE);
    assert!(
        position
            .notes
            .iter()
            .any(|note| note == "Rest nicht montiert"),
        "{:?}",
        position.notes
    );
    assert!(
        position.notes.iter().any(|note| note.contains("gekürzt")),
        "{:?}",
        position.notes
    );
    assert!(matches!(
        report.rows.first(),
        Some(proava_print::PrintBillingRow::Title { .. })
    ));

    let release = crate::release_to_print(&project, Some(id), "05.05.2027").unwrap();
    assert_eq!(release.earlier.len(), 1, "AR-1 als bisherige Freigabe");
    let gross = release
        .calculation
        .iter()
        .find(|line| line.label == "= Freigabe, brutto")
        .and_then(|line| line.amount)
        .unwrap();
    assert_eq!(gross, check.release.release_gross.to_decimal());
    assert!(release
        .notes
        .iter()
        .any(|note| note.contains("Bauabzugsteuer")));
    assert_eq!(release.signatures.len(), 2);

    // Without an invoice: the last one; without contract: a clear message.
    let last = crate::release_to_print(&project, None, "05.05.2027").unwrap();
    assert_eq!(last.invoice, release.invoice);
    let (plain, _) = awarded_project();
    let error = crate::release_to_print(&plain, None, "05.05.2027").unwrap_err();
    assert!(error.contains("Auftrag"), "{error}");
}

#[test]
fn billing_pdfs_are_written_through_the_print_job() {
    use proava_application::{PrintJob, PrintKind};
    let (project, id, _) = billing_project();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-billing-print-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    for kind in [PrintKind::BillingCheck, PrintKind::Release] {
        let mut job = PrintJob::new(kind);
        job.invoice = Some(id);
        job.settings.header_text = "Vorabzug".into();
        let file = folder.join(format!("{}.pdf", kind.key()));
        let outcome = exchange.export_print(&project, &job, &file).unwrap();
        assert!(
            std::fs::read(&file).unwrap().starts_with(b"%PDF"),
            "{kind:?}"
        );
        assert!(outcome.pages >= 1);
        assert_eq!(
            exchange.print_page_count(&project, &job).unwrap(),
            outcome.pages
        );
    }
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn release_letter_goes_to_the_client_with_the_release_amount() {
    let (mut project, id, _) = billing_project();
    // Without Bauherr/Auftraggeber: a clear message.
    project.info_mut().owner = Address::default();
    project.info_mut().own_client = None;
    let error = crate::release_letter_to_print(&project, Some(id), "05.05.2027").unwrap_err();
    assert!(error.contains("Bauherr"), "{error}");

    project.info_mut().owner = Address {
        name1: "Stadt Musterstadt".into(),
        street: "Rathausplatz 1".into(),
        postal_code: "12345".into(),
        city: "Musterstadt".into(),
        ..Address::default()
    };
    let context = project.billing_context().unwrap();
    let check = project.billing().check(&context, id).unwrap();
    let letters = crate::release_letter_to_print(&project, Some(id), "05.05.2027").unwrap();
    assert_eq!(letters.letters.len(), 1);
    let letter = &letters.letters[0];
    assert_eq!(letter.recipient[0], "Stadt Musterstadt");
    assert!(
        letter.subject.contains("2. Abschlagsrechnung AR-2"),
        "{}",
        letter.subject
    );
    assert!(letter
        .references
        .iter()
        .any(|(label, text)| label == "Rechnung" && text == "AR-2"));
    let body: String = letter
        .body
        .iter()
        .map(|block| match block {
            proava_print::TextBlock::Paragraph { runs, .. } => {
                runs.iter().map(|run| run.text.as_str()).collect::<String>()
            }
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains(&check.release.release_gross.to_string()),
        "{body}"
    );
    assert!(!body.contains('{'), "alle Platzhalter gefüllt: {body}");

    // The other letters show „–“ for the invoice placeholders.
    let mut template = project.letter_template(proava_domain::LetterKind::Award);
    template.subject.push_str(" {rechnung.nummer}");
    project.set_letter_template(template);
    let values = proava_domain::letter_values(
        &project,
        &proava_domain::InvitedBidder::new(Address::named("X")),
        "05.05.2027",
    );
    assert!(values
        .iter()
        .any(|(key, value)| key == "rechnung.nummer" && value == "–"));
}

/// Abrechnung, Freigabe-Anschreiben and Niederschrift in the Währung of the LV.
#[test]
fn billing_letters_and_submission_follow_the_currency_of_the_lv() {
    let (mut project, id, _) = billing_project();
    project.lv_mut().set_currency("USD");
    let check = crate::billing_check_to_print(&project, Some(id), "05.05.2027").unwrap();
    assert_eq!(check.header.currency, "USD");
    assert!(
        check.facts.iter().any(
            |(label, value)| label == "Auftrag mit Nachträgen" && value.ends_with(" USD netto")
        ),
        "{:?}",
        check.facts
    );
    let release = crate::release_to_print(&project, Some(id), "05.05.2027").unwrap();
    assert!(
        release
            .facts
            .iter()
            .any(|(label, value)| label == "Forderung netto" && value.ends_with(" USD")),
        "{:?}",
        release.facts
    );
    let context = project.billing_context().unwrap();
    let gross = project
        .billing()
        .check(&context, id)
        .unwrap()
        .release
        .release_gross;
    let values =
        proava_domain::letters::release_letter_values(&project, Some(id), "05.05.2027").unwrap();
    let expected = format!("{} USD", gross.plain());
    assert!(
        values
            .iter()
            .any(|(key, value)| key == "freigabe.brutto" && *value == expected),
        "{values:?}"
    );
    assert_eq!(
        crate::submission_to_print(&project, "05.05.2027")
            .header
            .currency,
        "USD"
    );
    // Euro stays „€“.
    project.lv_mut().set_currency("EUR");
    assert_eq!(
        crate::submission_to_print(&project, "05.05.2027")
            .header
            .currency,
        "€"
    );
}

/// A synthetic X89 (or X31 without header) for one position: nested
/// BoQCtgy for the title levels, the item with BillQty/QtyDeterm.
fn billing_file(ordinal: &str, phase: &str, header: &str, item: &str) -> String {
    let pieces: Vec<&str> = ordinal.split('.').collect();
    let (levels, last) = pieces.split_at(pieces.len() - 1);
    let mut body = format!(
        "<Itemlist><Item RNoPart=\"{}\">{item}</Item></Itemlist>",
        last[0]
    );
    for level in levels.iter().rev() {
        body = format!("<BoQCtgy RNoPart=\"{level}\"><BoQBody>{body}</BoQBody></BoQCtgy>");
    }
    let container = if phase == "31" {
        "QtyDeterm"
    } else {
        "Invoice"
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><GAEB><GAEBInfo><Version>3.3</Version></GAEBInfo>\
         <{container}><DP>{phase}</DP><CTR><Address><Name1>Fremdfirma</Name1></Address></CTR>\
         <BoQ><BoQBody>{body}</BoQBody></BoQ>{header}</{container}></GAEB>"
    )
}

#[test]
fn contract_is_read_from_an_x86() {
    use proava_application::ContractExport;
    let (project, bid) = awarded_project();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-x86-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("Auftrag.X86");
    exchange
        .export_contract(&project, bid, ContractExport::Xml, &file)
        .unwrap();

    // The LV has another quantity than the Auftrags-LV now.
    let mut target = project.clone();
    let row = target
        .lv()
        .positions()
        .find(|p| !p.is_lump_sum() && p.quantity() > Decimal::ONE)
        .unwrap()
        .id();
    let file_quantity = target.lv().position_by_id(row).unwrap().quantity();
    let index = target.lv().index_of(row).unwrap();
    target
        .lv_mut()
        .row_mut(index)
        .unwrap()
        .as_position_mut()
        .unwrap()
        .set_quantity(file_quantity + Decimal::from(5));

    let import = exchange.import_contract(&target, &file).unwrap();
    let contract = import.contract;
    let awarded = project.bids().iter().find(|b| b.id() == bid).unwrap();
    assert_eq!(contract.contractor, awarded.name());
    assert_eq!(contract.bid, None);
    assert_eq!(
        contract.items[&row].quantity, file_quantity,
        "Menge des Auftrags-LV"
    );
    assert_eq!(
        contract.items[&row].unit_price,
        awarded.unit_price(row).unwrap()
    );
    assert!(
        import.warnings.iter().any(|w| w.contains("Auftragsmengen")),
        "{:?}",
        import.warnings
    );
    let mut original = project.clone();
    let original_contract = original.take_over_contract().unwrap().clone();
    let expected = original_contract.total;
    assert_eq!(contract.items.len(), original_contract.items.len());
    assert_eq!(
        contract.net_before_discount(),
        original_contract.net_before_discount()
    );
    assert!(
        (contract.total - expected).cents().abs() <= 1,
        "{} / {}",
        contract.total,
        expected
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn invoice_and_takeoff_are_read_from_x89_and_x31() {
    let (mut project, ar2, row) = billing_project();
    let ordinal = project
        .lv()
        .position_by_id(row)
        .unwrap()
        .ordinal()
        .to_string();
    let exchange = GaebPdfExchange;
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-x89-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();

    // X89, cumulative: 3 so far (AR-2 checked 2) → takeoff counts 3-2.
    let x89 = billing_file(
        &ordinal,
        "89",
        "<InvoiceHeader><InvoiceNo>AR-3</InvoiceNo><InvoiceDate>2027-06-30</InvoiceDate>\
         <InvoiceType>deduction</InvoiceType><SettlementType>accumulated</SettlementType></InvoiceHeader>",
        "<BillQty>3</BillQty><QU>St</QU><UP>999.00</UP>\
         <QtyDeterm><Qty>3</Qty><QDetermItem><QTakeoff Row=\"           EG        0          3\"/></QDetermItem></QtyDeterm>",
    );
    let file = folder.join("AR-3.X89");
    std::fs::write(&file, x89).unwrap();
    let import = exchange.import_invoice(&project, &file).unwrap();
    assert_eq!(import.format, "X89");
    let invoice = import.invoice.clone().unwrap();
    assert_eq!(invoice.number, "AR-3");
    assert_eq!(invoice.date, "30.06.2027");
    assert_eq!(invoice.kind, proava_domain::InvoiceKind::Partial);
    assert_eq!(invoice.quantities, proava_domain::Mode::Cumulative);
    assert_eq!(invoice.items[&row].claimed_quantity, Some(Decimal::from(3)));
    let lines = &import.takeoff.iter().find(|(r, _)| *r == row).unwrap().1;
    assert_eq!(lines.last().unwrap().formula, "3-2", "{lines:?}");
    assert!(lines[0].label.starts_with("REB: EG"), "{lines:?}");
    assert!(
        import.warnings.iter().any(|w| w.contains("Fremdfirma")),
        "{:?}",
        import.warnings
    );

    let id = project.billing_mut().add_invoice(invoice);
    project.billing_mut().add_takeoff(import.takeoff, Some(id));
    let context = project.billing_context().unwrap();
    let check = project.billing().check(&context, id).unwrap();
    let line = check
        .positions
        .iter()
        .find(|line| line.position == row)
        .unwrap();
    assert_eq!(line.cumulative, Decimal::from(3), "bisher 2 + Aufmaß 1");
    assert!(line
        .findings
        .iter()
        .any(|f| matches!(f, proava_domain::Finding::UnitPriceDiffers { .. })));

    // X31 for AR-2: the quantity line as it is.
    let x31 = billing_file(&ordinal, "31", "", "<QtyDeterm><Qty>1,5</Qty></QtyDeterm>");
    let file = folder.join("Aufmass.X31");
    std::fs::write(&file, x31).unwrap();
    let import = exchange.import_invoice(&project, &file).unwrap();
    assert!(import.invoice.is_none());
    assert_eq!(import.takeoff[0].1.last().unwrap().formula, "1,5");
    let _ = ar2;

    // An LV file is no invoice.
    let error = exchange
        .import_invoice(&project, &folder.join("fehlt.X89"))
        .unwrap_err();
    assert!(error.contains("lesen"), "{error}");
    let _ = std::fs::remove_dir_all(&folder);
}

/// An X31 with REB rows but without Qty (as some programs write it): the
/// rows come as text, the answer names the positions whose takeoff counts 0.
#[test]
fn takeoff_without_quantity_is_named() {
    let (project, _, row) = billing_project();
    let ordinal = project
        .lv()
        .position_by_id(row)
        .unwrap()
        .ordinal()
        .to_string();
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-x31-reb-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let x31 = billing_file(
        &ordinal,
        "31",
        "",
        "<QtyDeterm><QDetermItem><QTakeoff Row=\"           EG     100004     8000   4000\"/></QDetermItem></QtyDeterm>",
    );
    let file = folder.join("Aufmass.X31");
    std::fs::write(&file, x31).unwrap();
    let import = GaebPdfExchange.import_invoice(&project, &file).unwrap();
    let lines = &import.takeoff.iter().find(|(r, _)| *r == row).unwrap().1;
    assert!(
        lines.iter().all(|line| line.formula.is_empty()),
        "{lines:?}"
    );
    assert!(
        import
            .warnings
            .iter()
            .any(|w| w.contains("ohne Aufmaßmenge") && w.contains(&ordinal)),
        "{:?}",
        import.warnings
    );

    // With a quantity: no such warning.
    let x31 = billing_file(&ordinal, "31", "", "<QtyDeterm><Qty>1,5</Qty></QtyDeterm>");
    std::fs::write(&file, x31).unwrap();
    let import = GaebPdfExchange.import_invoice(&project, &file).unwrap();
    assert!(
        !import
            .warnings
            .iter()
            .any(|w| w.contains("ohne Aufmaßmenge")),
        "{:?}",
        import.warnings
    );
    let _ = std::fs::remove_dir_all(&folder);
}

/// Übernahme aus einer Datei in der Form von STLB-Bau Online (syn7, nach dem
/// Vorbild einer echten Datei vom Anwender): Kostengruppe kommt über CtlgAssign
/// (2018 vor 2008), Kennung und Textergänzungen gehen zeichengleich bis zum
/// Export, die Fachregeln melden nichts.
#[test]
fn stlb_online_file_survives_take_over_and_export() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn7_stlb_online_33.X81");
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    assert_eq!((read.positions.len(), read.with_stlb_key), (3, 3));
    assert_eq!(
        read.positions[0].cost_group(),
        Some("482"),
        "DIN 276 2018, nicht 2008"
    );
    assert_eq!(
        proava_domain::stlb::catalog_cost_group(&read.positions[1]).as_deref(),
        Some("481"),
        "KG aus CtlgAssign, die Kennung hat keine"
    );
    assert_eq!(
        read.positions[0].catalog_line().as_deref(),
        Some("STLB-Bau 10/2022 070 TB")
    );

    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    let mut title = LvGroup::new(0, "", "GA");
    title.set_cost_group(Some("481".into()));
    lv.push_group(title);
    for _ in &read.positions {
        let mut placeholder = Position::stlb("folgt");
        placeholder.set_quantity(Decimal::from(3));
        lv.push_position(placeholder);
    }
    lv.renumber();
    for (index, source) in (1..).zip(&read.positions) {
        if let Some(target) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
            proava_domain::stlb::fill_from_catalog(target, source);
        }
        proava_domain::stlb::adopt_catalog_cost_group(&mut lv, index);
    }
    let own: Vec<Option<&str>> = lv.positions().map(|p| p.cost_group()).collect();
    assert_eq!(own, vec![Some("482"), None, None], "481 erbt vom Titel");

    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let original = std::fs::read_to_string(&file).unwrap();
    // Leerraum zwischen Elementen, BOM und Entität des Hochkommas zählen nicht.
    let compact = |text: &str| -> String {
        let mut out = String::new();
        for (index, piece) in text.trim_start_matches('\u{feff}').split('<').enumerate() {
            if index == 0 {
                continue;
            }
            let close = piece.find('>').map(|i| i + 1).unwrap_or(piece.len());
            out.push('<');
            out.push_str(&piece[..close].replace(" />", "/>"));
            let rest = &piece[close..];
            if !rest.trim().is_empty() {
                out.push_str(rest);
            }
        }
        out.replace("&#x27;", "'")
    };
    let between = |text: &str, open: &str, close: &str| -> Vec<String> {
        text.split(open)
            .skip(1)
            .map(|part| format!("{open}{}", &part[..part.find(close).unwrap() + close.len()]))
            .collect()
    };
    let original = compact(&original);
    for phase in [Phase::Dp81, Phase::Dp83] {
        let (doc, _) = lv_to_gaeb(&project, phase);
        let written = gaeb::xml::write(&doc, phase).unwrap();
        assert!(
            gaeb::rules::check_xml(written.as_bytes()).is_empty(),
            "{phase:?}: {:?}",
            gaeb::rules::check_xml(written.as_bytes())
        );
        let written = compact(&written);
        assert_eq!(
            between(&written, "<STLBBau>", "</STLBBau>"),
            between(&original, "<STLBBau>", "</STLBBau>"),
            "{phase:?}"
        );
        assert_eq!(
            between(&written, "<TextComplement ", "</TextComplement>"),
            between(&original, "<TextComplement ", "</TextComplement>"),
            "{phase:?}"
        );
        assert!(written.contains("<CtlgCode>482</CtlgCode>"), "{phase:?}");
    }
}

/// Druck einer Übernahme aus STLB-Bau Online (syn7): der Nachsatz „vom
/// Bieter einzutragen“ steht wie in STLB-Bau auf eigener Zeile und ohne
/// doppeltes „(vom Bieter)“; die leere Unterbeschreibung (Menge 0.000) ohne
/// „– 0“.
#[test]
fn stlb_online_take_over_prints_complements_as_delivered() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn7_stlb_online_33.X81");
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    for source in &read.positions {
        let mut placeholder = Position::stlb("folgt");
        placeholder.set_quantity(Decimal::from(2));
        proava_domain::stlb::fill_from_catalog(&mut placeholder, source);
        lv.push_position(placeholder);
    }
    lv.renumber();
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "18.11.2026");
    let proava_print::PrintItem::Title(title) = &print.items[0] else {
        panic!("{:?}", print.items)
    };
    let paragraphs = |index: usize| -> Vec<String> {
        let proava_print::PrintItem::Position(position) = &title.items[index] else {
            panic!("{:?}", title.items[index])
        };
        position
            .long_text
            .iter()
            .filter_map(|block| match block {
                proava_print::TextBlock::Paragraph { runs, .. } => {
                    Some(runs.iter().map(|run| run.text.as_str()).collect())
                }
                _ => None,
            })
            .collect()
    };
    let first = paragraphs(0).join("\n");
    assert!(
        first.contains("Gerätetyp gemäß Geräteliste ....................................................\nvom Bieter einzutragen."),
        "{first:?}"
    );
    assert!(!first.contains("(vom Bieter)"), "{first:?}");
    let sub = paragraphs(0)
        .into_iter()
        .find(|text| text.starts_with("Unterbeschreibung"))
        .unwrap();
    assert!(!sub.contains("– 0"), "{sub:?}");
    let last = paragraphs(title.items.len() - 1).join("\n");
    assert!(
        last.contains(
            "Gerätetyp gemäß Anlage ....................................................."
        ),
        "{last:?}"
    );
}

/// STLB-Bau Online liefert eine Standardbeschreibung als Position ohne
/// Einheit und Menge (Anwender 18.11., syn8 nach dem Vorbild): die Übernahme
/// schlägt „Hinweis“ vor; Hinweistext und Ausführungsbeschreibung mit Kennung
/// kommen als solche mit. Umgewandelt behalten alle Text und Kennung, und das
/// LV geht schemagerecht als X81/X83 hinaus.
#[test]
fn stlb_standard_descriptions_are_offered_as_texts() {
    use proava_domain::RowKind;
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../proava-gaeb/tests/data/syn8_stlb_standardbeschreibung_33.X81");
    let read = GaebPdfExchange.read_catalog_positions(&file).unwrap();
    assert_eq!(read.positions.len(), 4, "{:?}", read.positions);
    let kinds: Vec<(RowKind, RowKind)> = (0..4)
        .map(|index| (read.kind(index).delivered, read.kind(index).suggested))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (RowKind::Position, RowKind::Note),
            (RowKind::Position, RowKind::Position),
            (RowKind::Note, RowKind::Note),
            (RowKind::Performance, RowKind::Performance),
        ]
    );
    assert!(read
        .kind(0)
        .reason()
        .unwrap()
        .contains("Standardbeschreibung"));
    assert_eq!(read.with_stlb_key, 4);
    assert_eq!(
        read.delivered_positions().len(),
        2,
        "Aktualisieren nur mit Positionen"
    );
    assert_eq!(read.positions[0].short_text(), "Standardbesch Testfunktion");
    assert_eq!(read.positions[2].short_text(), "Testhinweis");
    assert_eq!(read.positions[3].short_text(), "Testausführung");

    // In ein LV: jede Zeile in der vorgeschlagenen Art, Text und Kennung bleiben.
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    for (index, source) in read.positions.iter().enumerate() {
        let mut row = Position::new("", "", Decimal::ZERO, "", None);
        proava_domain::stlb::fill_from_catalog(&mut row, source);
        let at = lv.rows().len();
        lv.push_position(row);
        lv.convert_row(at, read.kind(index).suggested).ok();
    }
    lv.renumber();
    let note = lv.rows()[1]
        .as_note()
        .expect("Standardbeschreibung als Hinweis");
    assert_eq!(note.heading(), "Standardbesch Testfunktion");
    assert_eq!(note.catalog().stlb.as_ref().unwrap().group, 9301);
    assert!(note
        .text()
        .to_plain()
        .starts_with("Gefordert sind die Testeinrichtungen"));
    assert_eq!(lv.rows()[2].ordinal(), "01.0010");
    assert_eq!(
        lv.rows()[3]
            .as_note()
            .unwrap()
            .catalog()
            .stlb
            .as_ref()
            .unwrap()
            .group,
        9303
    );
    let performance = lv.rows()[4]
        .as_performance()
        .expect("Ausführungsbeschreibung");
    assert_eq!(performance.catalog().stlb.as_ref().unwrap().group, 9304);
    let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
    for phase in [Phase::Dp81, Phase::Dp83] {
        let (doc, _) = lv_to_gaeb(&project, phase);
        let written = gaeb::xml::write(&doc, phase).unwrap();
        assert!(
            gaeb::rules::check_xml(written.as_bytes()).is_empty(),
            "{phase:?}: {:?}",
            gaeb::rules::check_xml(written.as_bytes())
        );
        assert_eq!(written.matches("<Remark").count(), 2, "{phase:?}");
        assert!(
            written.contains("<Group>9301</Group>"),
            "Kennung im Hinweistext: {written}"
        );
        assert!(written.contains("<PerfDescr"), "{phase:?}");
    }
}

/// The Kostengruppe at every position (own or from the title),
/// the Leistungsbereich only when asked for.
#[test]
fn print_shows_cost_group_and_work_category_at_the_positions() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let title = lv.rows().iter().position(|row| row.is_group()).unwrap();
    if let Some(proava_domain::LvRow::Group(group)) = lv.row_mut(title) {
        group.set_cost_group(Some("481".into()));
        group.set_work_category(Some("070".into()));
    }
    let last = lv.rows().iter().rposition(|row| row.is_position()).unwrap();
    if let Some(proava_domain::LvRow::Position(position)) = lv.row_mut(last) {
        position.set_cost_group(Some("484".into()));
    }
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "18.11.2026");
    fn codes(items: &[proava_print::PrintItem], out: &mut Vec<(Option<String>, Option<String>)>) {
        for item in items {
            match item {
                proava_print::PrintItem::Title(title) => codes(&title.items, out),
                proava_print::PrintItem::Position(position) => {
                    out.push((position.cost_group.clone(), position.work_category.clone()))
                }
                _ => {}
            }
        }
    }
    let mut all = Vec::new();
    codes(&print.items, &mut all);
    assert_eq!(
        all.first().unwrap(),
        &(Some("481".to_string()), Some("070".to_string())),
        "vom Titel geerbt"
    );
    assert_eq!(all.last().unwrap().0.as_deref(), Some("484"), "eigene KG");

    let job = proava_application::PrintJob::new(proava_application::PrintKind::RequestForQuotation);
    assert!(job.settings.cost_groups && !job.settings.work_categories);
    let mut model = print.clone();
    crate::print_job::keep_codes(
        &mut model.items,
        job.settings.cost_groups,
        job.settings.work_categories,
    );
    let mut kept = Vec::new();
    codes(&model.items, &mut kept);
    assert!(kept.iter().all(|(_, lb)| lb.is_none()), "LB nur auf Wunsch");
    assert_eq!(kept.first().unwrap().0.as_deref(), Some("481"));
    let (pdf, _) = crate::render_print(&project, &job, "18.11.2026").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

// ------------------------------------------------- PDF-Ausschnitte ----

/// A project with one position whose long text holds the left half of page
/// 1 of the synthetic vector PDF (150 mm wide in the text).
fn project_with_pdf_clip() -> Project {
    use proava_application::{prepare_clip, PdfSource};
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "Titel"));
    lv.push_position(position("", "Systemtopologie", "1", "psch", Some("100")));
    lv.renumber();
    let mut project = Project::new(ProjectName::parse("Ausschnitt").unwrap(), "2026-11-18", lv);
    let exchange = GaebPdfExchange;
    let source = PdfSource::read(
        &exchange,
        "topologie.pdf",
        proava_print::sample_vector_pdf(),
    )
    .unwrap();
    assert_eq!(source.pages.len(), 2);
    let (width, height) = source.pages[0];
    let prepared = prepare_clip(
        &exchange,
        &source,
        1,
        proava_domain::ClipRect::new(0.0, 0.0, width / 2.0, height),
    )
    .unwrap();
    // The page alone is kept when it is smaller than the file (this tiny
    // sample may not gain anything – then the file stays whole).
    let single = exchange.pdf_single_page(&source.bytes, 1).unwrap();
    assert_eq!(exchange.pdf_pages(&single).unwrap().len(), 1);
    assert_eq!(prepared.page_only, single.len() < source.bytes.len());
    assert_eq!(prepared.bytes.len(), single.len().min(source.bytes.len()));
    assert_eq!(prepared.clip.page, 1);
    assert_eq!(
        prepared.summary().contains("nur diese Seite"),
        prepared.page_only,
        "{}",
        prepared.summary()
    );
    let file = project.add_pdf_clip(&prepared.name, prepared.bytes, prepared.clip);
    assert_eq!(file, "topologie_S1.pdf");
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.is_position())
        .unwrap();
    let mut text = RichText::plain("Systemtopologie siehe Ausschnitt:");
    text.push(TextBlock::Image {
        file,
        width_mm: Some(150.0),
    });
    if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
        position.set_long_text(text);
    }
    project
}

/// Im proAVA-PDF bleibt der Ausschnitt Vektorgrafik: der Text der PDF ist
/// als Schrift im Druck, kein Rasterbild.
#[test]
fn pdf_clip_prints_as_vector() {
    let project = project_with_pdf_clip();
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-ausschnitt-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("lv.pdf");
    GaebPdfExchange
        .export_pdf(
            &project,
            proava_application::PdfDocument::RequestForQuotation,
            &file,
        )
        .unwrap();
    let bytes = std::fs::read(&file).unwrap();
    let (text, images) = proava_print::pdf_content(&bytes).unwrap();
    assert!(
        text.contains("TOPOLOGIEKERN"),
        "Text des Ausschnitts als Schrift: {text}"
    );
    assert!(!text.contains("ZWEITESEITE"), "nur Seite 1");
    assert_eq!(images, 0, "kein Rasterbild im Druck");
    std::fs::remove_dir_all(&folder).unwrap();
}

/// GAEB nimmt nur Pixel: der Ausschnitt geht als PNG mit 300 dpi mit, das
/// Bild wird für Prüfung und Export nur einmal gerechnet.
#[test]
fn pdf_clip_goes_into_gaeb_as_png() {
    use proava_application::GaebExport;
    let project = project_with_pdf_clip();
    let exchange = GaebPdfExchange;
    let pictures = exchange.gaeb_pictures(&project);
    assert_eq!(pictures.len(), 1);
    let (width, height) = pictures[0].clip_pixels.unwrap();
    assert_eq!(width, 1772, "150 mm bei 300 dpi");
    assert_eq!(height, 1772, "halbe Seite 100 × 100 mm");
    let renders = crate::GAEB_RENDERS.load(std::sync::atomic::Ordering::Relaxed);

    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-exchange-ausschnitt-gaeb-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("lv.X83");
    let warnings = exchange
        .export_gaeb(&project, GaebExport::RequestForQuotation, &file)
        .unwrap();
    assert!(
        has(&warnings, "1 Bild aus den Langtexten mitgegeben"),
        "{warnings:?}"
    );
    assert_eq!(
        crate::GAEB_RENDERS.load(std::sync::atomic::Ordering::Relaxed),
        renders,
        "Export nimmt das schon gerechnete Bild"
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("<image ").count(), 1);
    assert!(text.contains("image/png"));
    let read = gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(&read, ProjectName::parse("B").unwrap(), "");
    let png = &back.images()[0].bytes;
    assert!(png.starts_with(b"\x89PNG"));
    let png_width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(png_width, 1772);
    assert!(
        back.images()[0].clip.is_none(),
        "aus GAEB kommt ein Bild zurück"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

/// Schriftverkehr-Archiv: the collective PDF and each bidder's letter on
/// its own come from the same port; the filed letters together have the
/// pages of the collective PDF (pages are counted per letter).
#[test]
fn letters_in_memory_for_the_archive() {
    use proava_application::{write_letters, LetterJob, LetterRequest, LetterTarget, PrintLook};
    use proava_domain::{Address, InvitedBidder, LetterKind, PageFrame};
    let mut project = demo_project();
    let ids: Vec<_> = ["Müller GmbH", "Schmidt & Partner KG"]
        .iter()
        .map(|name| project.add_invitee(InvitedBidder::new(Address::named(*name))))
        .collect();
    let exchange = GaebPdfExchange;
    let request = LetterRequest::Bidders {
        kind: LetterKind::Invitation,
        recipients: ids.clone(),
    };
    let look = PrintLook::default();
    let (bytes, pages) = exchange
        .letters_pdf(
            &project,
            &request,
            None,
            &PageFrame::default(),
            &look,
            "Alle.pdf",
        )
        .unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-letters-archive-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let job = LetterJob {
        request,
        target: LetterTarget::One(folder.join("Alle.pdf")),
        file_in_project: true,
        logo: None,
        frame: PageFrame::default(),
        look,
        date: "18.11.2026".into(),
    };
    let output = write_letters(&exchange, &project, &job).unwrap();
    assert_eq!(output.pages, pages);
    assert!(std::fs::read(folder.join("Alle.pdf"))
        .unwrap()
        .starts_with(b"%PDF"));
    assert_eq!(output.filed.len(), 2);
    assert!(output
        .filed
        .iter()
        .all(|entry| entry.pdf.starts_with(b"%PDF")));
    assert!(
        output.filed[0].pdf.len() < bytes.len(),
        "Einzelbrief kleiner als das Sammel-PDF"
    );
    for entry in &output.filed {
        project.add_correspondence(entry.clone());
    }
    assert_eq!(
        project.correspondence()[1].recipient_name,
        "Schmidt & Partner KG"
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

/// The demo project with Abstandshalter at the start, after a title,
/// between positions, before a hint and at the end.
fn spaced(project: &Project) -> Project {
    let mut spaced = project.clone();
    let lv = spaced.lv_mut();
    let rows = lv.rows().len();
    for index in (0..=rows).rev().filter(|index| index % 3 == 0) {
        lv.insert_numbered(index, LvRow::Spacer(proava_domain::Spacer::new()))
            .unwrap();
    }
    lv.push_spacer();
    spaced
}

#[test]
fn spacers_stay_out_of_gaeb_csv_and_the_rules() {
    use proava_application::{CsvEncoding, GaebExport};
    let plain = demo_project();
    let spaced = spaced(&plain);
    assert!(spaced.lv().spacer_count() >= 4);
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83] {
        let (with, with_warnings) = lv_to_gaeb(&spaced, phase);
        let (without, without_warnings) = lv_to_gaeb(&plain, phase);
        assert_eq!(with, without, "{phase:?}: GAEB ohne Abstandshalter");
        assert_eq!(
            with_warnings, without_warnings,
            "{phase:?}: still, keine Meldung"
        );
        assert_eq!(
            gaeb::xml::write(&with, phase).unwrap(),
            gaeb::xml::write(&without, phase).unwrap()
        );
    }
    for kind in [
        GaebExport::Description,
        GaebExport::RequestForQuotation,
        GaebExport::PricedLv,
    ] {
        let lines = |project: &Project| -> Vec<String> {
            crate::export_rule_findings(project, kind)
                .iter()
                .map(crate::rule_line)
                .collect()
        };
        assert_eq!(
            lines(&spaced),
            lines(&plain),
            "{kind:?}: keine neuen Fachregel-Befunde"
        );
    }
    // The import makes none.
    let (doc, _) = lv_to_gaeb(&spaced, Phase::Dp83);
    let (back, _) = gaeb_to_project(&doc, ProjectName::parse("X").unwrap(), "");
    assert_eq!(back.lv().spacer_count(), 0);
    // CSV: the empty lines between the rows space them anyway.
    for encoding in [CsvEncoding::Utf8Bom, CsvEncoding::Windows1252] {
        let with = crate::csv_export::lv_to_csv(spaced.lv(), encoding);
        let without = crate::csv_export::lv_to_csv(plain.lv(), encoding);
        assert_eq!(with.bytes, without.bytes);
        assert_eq!(with.notes, without.notes);
    }
}

#[test]
fn spacers_are_in_no_printout() {
    // Abstandshalter are only for editing (Anwender 18.11.): every printout is
    // the same with and without them.
    use proava_application::{PrintJob, PrintKind};
    let (mut plain, bid) = awarded_project();
    // Kostenermittlung (its Kostenkontrolle reads the LV with the spacers too).
    plain.add_cost_estimate(proava_domain::cost_demo::demo_cost_estimate(
        proava_domain::CostStage::Calculation,
    ));
    let spaced = spaced(&plain);
    assert!(spaced.lv().spacer_count() >= 4);
    let render = |project: &Project, job: &PrintJob| {
        crate::render_print(project, job, "18.11.2026").unwrap().0
    };
    let mut checked = 0;
    for kind in PrintKind::ALL {
        if matches!(kind, PrintKind::BillingCheck | PrintKind::Release) {
            continue; // Abrechnung: positions of an invoice only
        }
        let mut job = PrintJob::new(kind);
        job.bid = Some(bid);
        // The Kostenermittlung (Kosten-LV) needs the LV as a Kosten-LV.
        let as_kind = |project: &Project| {
            let mut project = project.clone();
            if kind == PrintKind::CostLv {
                project.lv_mut().set_purpose(proava_domain::LvPurpose::Cost(
                    proava_domain::CostStage::Calculation,
                ));
            }
            project
        };
        let (plain, spaced) = (as_kind(&plain), as_kind(&spaced));
        let without = render(&plain, &job);
        if without != render(&plain, &job) {
            // Not byte-stable (should not happen): compare the text instead.
            panic!("{kind:?}: PDF nicht reproduzierbar");
        }
        assert!(
            render(&spaced, &job) == without,
            "{kind:?}: Druck mit Abstandshaltern weicht ab"
        );
        checked += 1;
    }
    assert!(checked >= 8);
    // The print model never sees them, with or without long texts.
    for mode in [
        proava_print::LvPrintMode::Blank,
        proava_print::LvPrintMode::Priced,
    ] {
        let with = crate::lv_to_print(&spaced, mode, "18.11.2026");
        let without = crate::lv_to_print(&plain, mode, "18.11.2026");
        assert_eq!(format!("{:?}", with.items), format!("{:?}", without.items));
    }
}

/// Bieterangaben aus dem Langtext-Editor (Anwender, 18.11.2026): an der Stelle
/// im Text als X83 (TextComplement Kind="Bidder"), zurückgelesen an derselben
/// Stelle; der Bieter füllt sie in seiner X84 aus, Preisspiegel und
/// Auftrags-LV zeigen seine Angabe.
#[test]
fn bidder_field_from_the_editor_goes_through_x83_x84_and_the_contract_lv() {
    use proava_domain::complement_text::{insert, FieldPlace};
    use proava_domain::{ComplementKind, TextComplement};

    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0010")
        .unwrap();
    let position = project.lv().rows()[index].as_position().unwrap().clone();
    let first_words: String = position.long_text().to_plain().chars().take(12).collect();
    let field = |caption: &str| {
        TextComplement {
            kind: ComplementKind::Bidder,
            caption: caption.into(),
            ..TextComplement::default()
        }
        .with_spaces()
    };
    let (text, complements) = insert(
        position.long_text(),
        position.complements(),
        field("Fabrikat:"),
        &FieldPlace::After(first_words.clone()),
    )
    .unwrap();
    let (text, complements) = insert(&text, &complements, field("Typ:"), &FieldPlace::End).unwrap();
    {
        let position = project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap();
        position.set_long_text(text.clone());
        position.set_complements(complements.clone());
    }
    let fabrikat = complements
        .iter()
        .find(|c| c.caption == "Fabrikat: ")
        .unwrap()
        .clone();
    assert!(fabrikat.anchor.is_some_and(|anchor| !anchor.own_line));

    // X83: the field stands in the text, the reader puts it back there.
    let (tender, _) = lv_to_gaeb(&project, Phase::Dp83);
    let xml = gaeb::xml::write(&tender, Phase::Dp83).unwrap();
    assert!(xml.contains("Kind=\"Bidder\""), "{xml}");
    let read = gaeb::xml::read(xml.as_bytes()).unwrap();
    let (back, _) = gaeb_to_project(
        &read,
        ProjectName::parse("Rundlauf").unwrap(),
        "2026-11-18T12:00:00",
    );
    let back_position = back.lv().position_by_ordinal("01.01.0010").unwrap();
    assert_eq!(back_position.long_text().to_plain(), text.to_plain());
    let back_fabrikat = back_position
        .complements()
        .iter()
        .find(|c| c.caption.trim() == "Fabrikat:")
        .unwrap();
    assert_eq!(
        back_fabrikat.anchor, fabrikat.anchor,
        "dieselbe Stelle im Text"
    );
    assert_eq!(back_fabrikat.marker, fabrikat.marker);

    // X84 of a bidder: Fabrikat filled in.
    let mut doc = bidder_file(&project);
    for_items(&mut doc.award.boq.body, &mut |item| {
        if item.rno_part.trim_start_matches('0') == "10" {
            for complement in &mut item.complements {
                if complement.kind == "Bidder" && complement.marker.trim() == fabrikat.marker {
                    complement.body = "Siemens QMX3.P74".into();
                }
            }
        }
    });
    let import = match_bid(&project, &doc, "muster.x84");
    let id = project.add_bid(import.bid);
    let bid = project.bids().iter().find(|bid| bid.id() == id).unwrap();
    let position = project.lv().position_by_ordinal("01.01.0010").unwrap();
    assert_eq!(
        bid.complement_lines(position),
        ["Fabrikat: Siemens QMX3.P74"],
        "Preisspiegel-Tooltip"
    );

    // Auftrags-LV: the bidder's entry at the place of the gap.
    project.award_bid(id, "01.12.2026").unwrap();
    let (print, _) = crate::contract_to_print(&project, id, "01.12.2026").unwrap();
    let printed = format!("{:?}", print.items);
    assert!(printed.contains("Fabrikat: Siemens QMX3.P74"), "{printed}");
    assert!(
        printed.contains(&format!("text: \"{}\"", ".".repeat(52))),
        "Lücke Typ: {printed}"
    );
}

/// Gedruckte Lücke einer Bieterangabe wie bei STLB-Bau: 52 Punkte in einem
/// Stück (nie umbrochen), ohne „(vom Bieter)“; im Kurztext-LV bleiben
/// nur die Bieterangaben unter dem Kurztext.
#[test]
fn printed_gap_is_52_dots_in_one_piece_and_stays_in_the_short_text_lv() {
    use proava_domain::complement_text::{insert, FieldPlace};
    use proava_domain::{ComplementKind, TextComplement};
    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0010")
        .unwrap();
    let position = project.lv().rows()[index].as_position().unwrap().clone();
    let words: String = position.long_text().to_plain().chars().take(12).collect();
    let field = |caption: &str, kind| {
        TextComplement {
            kind,
            caption: caption.into(),
            ..TextComplement::default()
        }
        .with_spaces()
    };
    let (text, list) = insert(
        position.long_text(),
        position.complements(),
        field("Fabrikat:", ComplementKind::Bidder),
        &FieldPlace::After(words),
    )
    .unwrap();
    let mut owner = field("Nennweite", ComplementKind::Owner);
    owner.body = "DN 20".into();
    let (text, list) = insert(&text, &list, owner, &FieldPlace::End).unwrap();
    let (text, list) = insert(
        &text,
        &list,
        field("Typ:", ComplementKind::Bidder),
        &FieldPlace::End,
    )
    .unwrap();
    {
        let position = project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap();
        position.set_long_text(text);
        position.set_complements(list);
    }
    fn position_of<'a>(
        items: &'a [proava_print::PrintItem],
        short: &str,
    ) -> Option<&'a proava_print::PrintPosition> {
        items.iter().find_map(|item| match item {
            proava_print::PrintItem::Title(title) => position_of(&title.items, short),
            proava_print::PrintItem::Position(position) if position.short_text == short => {
                Some(position)
            }
            _ => None,
        })
    }
    let runs = |blocks: &[proava_print::TextBlock]| -> Vec<proava_print::TextRun> {
        blocks
            .iter()
            .flat_map(|block| match block {
                proava_print::TextBlock::Paragraph { runs, .. } => runs.clone(),
                _ => Vec::new(),
            })
            .collect()
    };
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "18.11.2026");
    let printed = position_of(&print.items, position.short_text()).unwrap();
    let all = runs(&printed.long_text);
    let gaps: Vec<_> = all.iter().filter(|run| run.gap).collect();
    assert_eq!(gaps.len(), 2, "Fabrikat und Typ");
    assert!(gaps
        .iter()
        .all(|gap| gap.text == ".".repeat(52) && !gap.underline));
    let plain: String = all.iter().map(|run| run.text.as_str()).collect();
    // No „(vom Bieter)“ in print; a space between gap and the text after it.
    assert!(
        plain.contains(&format!(
            "Liefern und Fabrikat: {} montieren",
            ".".repeat(52)
        )),
        "{plain:?}"
    );
    assert!(!plain.contains("(vom"), "{plain:?}");
    assert!(plain.contains("Nennweite DN 20"), "{plain:?}");
    assert!(!plain.contains('\u{E000}'), "kein Platzhalter im Druck");

    // Kurztext-LV: only the Bieterangaben, as lines, with their gaps.
    let job = proava_application::PrintJob::new(proava_application::PrintKind::ShortTextLv);
    let selection = crate::print_job::print_selection(&project, &job).unwrap();
    let mut short = crate::lv_to_print(&selection, proava_print::LvPrintMode::Blank, "18.11.2026");
    crate::print_job::strip_long_texts(&mut short.items);
    let printed = position_of(&short.items, position.short_text()).unwrap();
    assert_eq!(
        printed.long_text.len(),
        2,
        "Fabrikat und Typ, nicht der Langtext: {:?}",
        printed.long_text
    );
    let all = runs(&printed.long_text);
    let plain: String = all.iter().map(|run| run.text.as_str()).collect();
    assert!(plain.starts_with("Fabrikat: ...."), "{plain:?}");
    assert!(
        !plain.contains("Nennweite"),
        "Angabe des Auftraggebers gehört zum Langtext"
    );
    assert_eq!(all.iter().filter(|run| run.gap).count(), 2);
    // Without Bieterangaben the Kurztext-LV stays as before: no long text.
    let other =
        position_of(&short.items, "Raumbediengerät").map(|position| position.long_text.len());
    assert!(other.is_none_or(|count| count == 0));
}

/// Absätze mit Bieterangabe im Satz (Zufallstest robust3): Ausrichtung und
/// Absatzgrenzen kommen aus GAEB DA XML zurück, wie sie waren – auch zwei
/// solche Absätze hintereinander und einer, der mit dem Feld beginnt.
#[test]
fn paragraphs_with_a_field_inside_keep_alignment_and_borders_through_gaeb() {
    use proava_domain::{Align, ComplementKind, TextAnchor, TextComplement};
    let mut project = demo_project();
    let index = project
        .lv()
        .rows()
        .iter()
        .position(|row| row.ordinal() == "01.01.0010")
        .unwrap();
    let paragraph = |text: &str, align| TextBlock::Paragraph {
        runs: vec![Run::plain(text)],
        align,
    };
    let text = RichText::new(vec![
        paragraph("Erster Absatz zentriert, Fabrikat ", Align::Center),
        paragraph("Zweiter Absatz rechts, Typ ", Align::Right),
        paragraph(" steht am Anfang, Blocksatz.", Align::Justify),
        paragraph("Links mit Feld ", Align::Left),
        paragraph("Ohne Feld, zentriert.", Align::Center),
        // A long line: no hard-wrapped export (hard_wrap.rs would reflow it).
        paragraph(
            "Liefern, montieren und anschließen einschließlich aller Kleinteile, Befestigungsmaterial und Dokumentation.",
            Align::Left,
        ),
    ]);
    let field = |marker: &str, block, offset| TextComplement {
        marker: marker.into(),
        kind: ComplementKind::Bidder,
        caption: "Angabe ".into(),
        anchor: Some(TextAnchor {
            block,
            offset,
            own_line: false,
        }),
        ..TextComplement::default()
    };
    let complements = vec![
        field("01", 0, 34),
        field("02", 1, 27),
        field("03", 2, 0),
        field("04", 3, 15),
    ];
    {
        let position = project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap();
        position.set_long_text(text.clone());
        position.set_complements(complements.clone());
    }
    for phase in [Phase::Dp83, Phase::Dp82, Phase::Dp81] {
        let (doc, _) = lv_to_gaeb(&project, phase);
        let xml = gaeb::xml::write(&doc, phase).unwrap();
        assert!(
            xml.contains("<span style=\"text-align:center;\"/>"),
            "{xml}"
        );
        let read = gaeb::xml::read(xml.as_bytes()).unwrap();
        let (back, _) = gaeb_to_project(
            &read,
            ProjectName::parse("Rundlauf").unwrap(),
            "2026-11-18T12:00:00",
        );
        let position = back.lv().position_by_ordinal("01.01.0010").unwrap();
        assert_eq!(
            position.long_text(),
            &text,
            "{phase:?}: Absätze und Ausrichtung"
        );
        let anchors: Vec<_> = position
            .complements()
            .iter()
            .map(|complement| complement.anchor)
            .collect();
        let wanted: Vec<_> = complements
            .iter()
            .map(|complement| complement.anchor)
            .collect();
        assert_eq!(anchors, wanted, "{phase:?}: Stellen der Felder");
    }
}

/// So liest proAVA weiter die Dateien anderer Programme: `<br/>` mit
/// leerem `<span/>` (z. B. STLB-Bau) bleibt ein Zeilenumbruch, ein `<p>`
/// vor einer Textergänzung endet dort.
#[test]
fn foreign_bare_spans_and_closed_paragraphs_read_as_before() {
    let xml = |detail: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="1"><Qty>1</Qty><QU>St</QU><Description><CompleteText><DetailTxt>{detail}</DetailTxt></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#
        )
    };
    let complement = r#"<TextComplement MarkLbl="11" Kind="Bidder"><ComplCaption><span>Typ </span></ComplCaption><ComplBody><span>'....'</span></ComplBody><ComplTail/></TextComplement>"#;
    let read = |detail: &str| {
        let doc = gaeb::xml::read(xml(detail).as_bytes()).unwrap();
        let (_, item) = doc.award.boq.items().into_iter().next().unwrap();
        (item.long_text.clone(), item.complements.clone())
    };
    let (text, complements) = read(&format!(
        "<Text><span>Programmen,</span><br/><span/></Text>{complement}<Text><span>vom Bieter einzutragen.</span></Text>"
    ));
    assert_eq!(text.blocks.len(), 1, "ein Absatz: {text:?}");
    assert!(!complements[0].anchor.unwrap().own_line);
    let (text, complements) = read(&format!(
        "<Text><p><span>400 V AC,</span></p></Text>{complement}<Text><p><span>Hz</span></p></Text>"
    ));
    assert_eq!(text.blocks.len(), 2, "{text:?}");
    assert!(
        complements[0].anchor.unwrap().own_line,
        "nach </p> eigene Zeile"
    );
}

/// Schutz vor zu großen Drucken (Speicher, 18.11.2026): ein LV-Druck, der
/// voraussichtlich mehr Arbeitsspeicher braucht als frei ist, wird mit einer
/// verständlichen Meldung abgelehnt; passt er oder ist der freie Speicher
/// unbekannt, wird gedruckt.
#[test]
fn a_printout_beyond_the_free_memory_is_refused_with_a_message() {
    let project = demo_project();
    let job = proava_application::PrintJob::new(proava_application::PrintKind::RequestForQuotation);
    let error =
        crate::render_print_within(&project, &job, "18.11.2026", Some(1_048_576)).unwrap_err();
    assert!(error.contains("braucht voraussichtlich rund"), "{error}");
    assert!(error.contains("frei sind nur 0,0 GB"), "{error}");
    assert!(error.contains("markierte Titel"), "{error}");
    // The dialog prints with Übertrag: the message names the need without.
    assert!(
        job.settings.carry && error.contains("Ohne „Übertrag“ braucht er rund"),
        "{error}"
    );
    for available in [None, Some(64 * 1_073_741_824)] {
        let (pdf, outcome) =
            crate::render_print_within(&project, &job, "18.11.2026", available).unwrap();
        assert!(pdf.starts_with(b"%PDF") && outcome.pages > 0);
    }
    // Other printouts (Preisspiegel …) are not estimated: never refused.
    let mut comparison =
        proava_application::PrintJob::new(proava_application::PrintKind::PriceComparison);
    comparison.settings = proava_application::PrintSettings::defaults_for(
        proava_application::PrintKind::PriceComparison,
    );
    if !project.bids().is_empty() {
        assert!(crate::render_print_within(&project, &comparison, "18.11.2026", Some(1)).is_ok());
    }
}

/// Text after a Bieterangabe (synthetic, built like STLB-Bau texts): a
/// space at the start of the following span survives the import and the
/// round trip GAEB → proAVA → GAEB word for word; without one the print
/// sets one („………… vom Bieter einzutragen.“) – never „(vom Bieter)“.
#[test]
fn text_after_a_bidder_field_keeps_its_space_and_prints_set_off() {
    let xml = |after: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="1"><Qty>1</Qty><QU>St</QU><Description><CompleteText><DetailTxt><Text><span>Gerät für den Schaltschrank, betriebsfertig für die Eingabe von Programmen,</span><br/><span/></Text><TextComplement MarkLbl="11" Kind="Bidder"><ComplCaption><span>Geräteliste Blatt 4 </span></ComplCaption><ComplBody><span>'</span><br/><span>....................................................</span><span>'</span></ComplBody><ComplTail/></TextComplement><Text><span>{after}</span></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Gerät</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#
        )
    };
    let import = |xml: &str| {
        let doc = gaeb::xml::read(xml.as_bytes()).unwrap();
        let (project, _) = gaeb_to_project(
            &doc,
            ProjectName::parse("Probe").unwrap(),
            "2026-11-18T17:00:00",
        );
        project
    };
    let plain_after = |project: &Project| {
        let position = project.lv().positions().next().unwrap().clone();
        let anchor = position.complements()[0].anchor.unwrap();
        let block = position.long_text().blocks()[anchor.block].runs().unwrap();
        let text: String = block.iter().map(|run| run.text.as_str()).collect();
        text.chars().skip(anchor.offset).collect::<String>()
    };
    // With a space in the file: it stays, also through X83 and back.
    let with_space = import(&xml(" vom Bieter einzutragen."));
    assert_eq!(plain_after(&with_space), " vom Bieter einzutragen.");
    let (doc, _) = lv_to_gaeb(&with_space, Phase::Dp83);
    let written = gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    assert!(
        written.contains("> vom Bieter einzutragen.</span>"),
        "{written}"
    );
    let again = import(&written);
    assert_eq!(plain_after(&again), " vom Bieter einzutragen.");
    assert_eq!(
        again.lv().positions().next().unwrap().long_text(),
        with_space.lv().positions().next().unwrap().long_text(),
        "Rundlauf: derselbe Wortlaut"
    );
    // Without a space (as the file has it): the print sets one.
    let without = import(&xml("vom Bieter einzutragen."));
    assert_eq!(
        plain_after(&without),
        "vom Bieter einzutragen.",
        "Import erfindet nichts"
    );
    for project in [&with_space, &without] {
        let print = crate::lv_to_print(project, proava_print::LvPrintMode::Blank, "18.11.2026");
        let proava_print::PrintItem::Position(position) = &print.items[0] else {
            panic!("{:?}", print.items)
        };
        let plain: String = position
            .long_text
            .iter()
            .flat_map(|block| match block {
                proava_print::TextBlock::Paragraph { runs, .. } => runs.clone(),
                _ => Vec::new(),
            })
            .map(|run| run.text)
            .collect();
        let dots = ".".repeat(52);
        assert!(
            plain.ends_with(&format!(
                "Geräteliste Blatt 4 {dots} vom Bieter einzutragen."
            )),
            "{plain:?}"
        );
        assert!(!plain.contains("(vom"), "{plain:?}");
    }
}

/// Vorbemerkungen print as such (own item: full width, no mark
/// „Hinweis“), empty Hinweise print nothing at all.
#[test]
fn preliminary_notes_have_no_mark_and_empty_notes_print_nothing() {
    let mut project = demo_project();
    let lv = project.lv_mut();
    let mut preliminary = Note::new("Für alle Positionen gilt die VOB/C.");
    preliminary.set_heading("Allgemeine Vorbemerkungen");
    preliminary.set_preliminary(true);
    lv.insert_row(0, LvRow::Note(preliminary));
    lv.insert_row(
        1,
        LvRow::Note(Note::rich(RichText::new(vec![TextBlock::paragraph(
            vec![],
        )]))),
    );
    let print = crate::lv_to_print(&project, proava_print::LvPrintMode::Blank, "18.11.2026");
    let proava_print::PrintItem::Preliminary(blocks) = &print.items[0] else {
        panic!("{:?}", print.items[0])
    };
    let first: String = match &blocks[0] {
        proava_print::TextBlock::Paragraph { runs, .. } => {
            runs.iter().map(|run| run.text.as_str()).collect()
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(first, "Allgemeine Vorbemerkungen");
    let notes_before = demo_project()
        .lv()
        .rows()
        .iter()
        .filter(|row| row.as_note().is_some_and(|note| !note.is_blank()))
        .count();
    fn count(items: &[proava_print::PrintItem]) -> usize {
        items
            .iter()
            .map(|item| match item {
                proava_print::PrintItem::Title(title) => count(&title.items),
                proava_print::PrintItem::Note(_) => 1,
                _ => 0,
            })
            .sum()
    }
    assert_eq!(
        count(&print.items),
        notes_before,
        "der leere Hinweis fehlt im Druck"
    );
}

/// The print model gets the level of every list item and the number of a
/// numbered one („1.“, „a)“ by level); bullets get their sign in the
/// template.
#[test]
fn print_blocks_carry_list_levels_and_numbers() {
    let project = Project::new(
        ProjectName::parse("P").unwrap(),
        "",
        Lv::new("LV", OzMask::parse("11.PPPP").unwrap()),
    );
    let text = RichText::new(vec![
        TextBlock::list_item(true, vec![Run::plain("eins")]),
        TextBlock::list_item_at(true, 1, vec![Run::plain("a")]),
        TextBlock::list_item_at(false, 2, vec![Run::plain("Strich")]),
    ]);
    let blocks = crate::to_print::blocks(&project, &text);
    let kinds: Vec<(u8, Option<String>)> = blocks
        .iter()
        .filter_map(|block| match block {
            proava_print::TextBlock::ListItem { level, marker, .. } => {
                Some((*level, marker.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (0, Some("1.".to_string())),
            (1, Some("a)".to_string())),
            (2, None)
        ]
    );
}

/// Anschrift des Auftraggebers mit allen Feldern von GAEB DA XML 3.3
/// (Name3/4, Land, ILN, Ansprechpartner, Fax, USt-IdNr.) und die
/// Datenaustauschnummer (OWN/DPNo): bleiben beim Import erhalten und gehen
/// mit dem Export wieder hinaus, die DPNo auch dann, wenn sie gleich der
/// Vergabenummer ist.
#[test]
fn every_address_field_and_the_dp_number_survive_import_and_export() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    let owner = doc.award.owner.get_or_insert_with(Default::default);
    owner.name1 = "Stadt Muster".into();
    owner.name3 = "Hochbauamt".into();
    owner.name4 = "Technik".into();
    owner.country = "Deutschland".into();
    owner.iln = "4012345000009".into();
    owner.contact = "Frau Muster".into();
    owner.fax = "0211 12345".into();
    owner.vat_id = "DE123456789".into();
    doc.award.award_no = "VG-17".into();
    doc.award.owner_dp_no = "434-V02".into();
    let text = proava_gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let read = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    let (back, warnings) = gaeb_to_project(&read, ProjectName::parse("RH3").unwrap(), "");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.contains("nicht übernommen")),
        "{warnings:?}"
    );
    let info = back.info();
    assert_eq!(info.owner.name3, "Hochbauamt");
    assert_eq!(info.owner.name4, "Technik");
    assert_eq!(info.owner.country, "Deutschland");
    assert_eq!(info.owner.iln, "4012345000009");
    assert_eq!(info.owner.contact, "Frau Muster");
    assert_eq!(info.owner.fax, "0211 12345");
    assert_eq!(info.owner.vat_id, "DE123456789");
    assert_eq!(info.exchange_number, "434-V02");

    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    assert_eq!(again.award.owner, read.award.owner);
    assert_eq!(again.award.owner_dp_no, "434-V02");
    let written = proava_gaeb::xml::write(&again, Phase::Dp83).unwrap();
    let dp_no = written.find("<DPNo>434-V02</DPNo>").expect("DPNo");
    let award_no = written.find("<AwardNo>VG-17</AwardNo>").expect("AwardNo");
    assert!(dp_no < award_no, "XSD-Reihenfolge: DPNo vor AwardNo");
    assert!(proava_gaeb::rules::check_xml(written.as_bytes()).is_empty());

    // Equal to the Vergabenummer: kept as well, so the export gives it back.
    let mut same = read.clone();
    same.award.owner_dp_no = "VG-17".into();
    let (back, _) = gaeb_to_project(&same, ProjectName::parse("RH3").unwrap(), "");
    assert_eq!(back.info().exchange_number, "VG-17");
    let (again, _) = lv_to_gaeb(&back, Phase::Dp83);
    assert_eq!(again.award.owner_dp_no, "VG-17");

    // A text after the LV (AddText at the root, e.g. a „Schlussblatt“) stays
    // a closing text after the LV – no Vorbemerkung, no warning.
    let closing = text.replacen(
        "</GAEB>",
        "<AddText><OutlineAddText><p><span>Schlussblatt</span></p></OutlineAddText>\
         <DetailAddText><p><span>Angaben des Bieters</span></p></DetailAddText></AddText></GAEB>",
        1,
    );
    let (read, report) = proava_gaeb::xml::read_with_report(closing.as_bytes()).unwrap();
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("nach dem LV")),
        "{:?}",
        report.warnings
    );
    assert_eq!(read.award.closing_texts[0].outline, "Schlussblatt");
    assert!(read
        .award
        .add_texts
        .iter()
        .all(|text| text.outline != "Schlussblatt"));
}

/// D84 eines Bieterprogramms mit gekürzten Positionsnummern (GAEB 90 fasst
/// höchstens 9 Stellen, das LV braucht 10): viele Positionen tragen dieselbe OZ.
/// Das Einlesen sagt das vorne und deutlich, statt nur „andere LV-Fassung“; und
/// gleiche Lesehinweise der GAEB-90-Datei stehen einmal mit Anzahl da.
#[test]
fn a_bid_with_cut_position_numbers_says_so() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    let BoqNode::Category(title) = &mut doc.award.boq.body[1] else {
        panic!()
    };
    let BoqNode::Category(asp01) = &mut title.body[1] else {
        panic!()
    };
    let template = asp01
        .body
        .iter()
        .find_map(|node| match node {
            BoqNode::Item(item) => Some(item.clone()),
            _ => None,
        })
        .unwrap();
    for number in 0..12 {
        asp01.body.push(BoqNode::Item(Item {
            rno_part: if number < 6 { "001" } else { "002" }.into(),
            unit_price: Some(dec("10")),
            ..template.clone()
        }));
    }
    let import = match_bid(&project, &doc, "gekuerzt.d84");
    assert!(
        import.warnings[0].starts_with("ACHTUNG: In der Datei tragen 12 von")
            && import.warnings[0].contains("2 Ordnungszahlen mehrfach")
            && import.warnings[0].contains("vermutlich gekürzt"),
        "{:?}",
        import.warnings
    );
    // Ein einzelner doppelter Eintrag ist kein gekürztes Format.
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    let BoqNode::Category(title) = &mut doc.award.boq.body[1] else {
        panic!()
    };
    let BoqNode::Category(asp01) = &mut title.body[1] else {
        panic!()
    };
    asp01.body.push(BoqNode::Item(template));
    let import = match_bid(&project, &doc, "doppelt.x84");
    assert!(!import
        .warnings
        .iter()
        .any(|w| w.contains("vermutlich gekürzt")));

    let warnings = crate::files::datan_warnings(
        &(1..=145)
            .map(|line| proava_gaeb::datan::Warning {
                line,
                message: "Satzart 23 ohne Positionsnummer übersprungen".into(),
            })
            .chain(std::iter::once(proava_gaeb::datan::Warning {
                line: 200,
                message: "Langtext (26) ohne Bezug übersprungen".into(),
            }))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        warnings,
        [
            "GAEB 90: Satzart 23 ohne Positionsnummer übersprungen (145×, Zeilen 1, 2, 3, 4, 5 …)",
            "GAEB 90, Zeile 200: Langtext (26) ohne Bezug übersprungen"
        ]
    );

    // Über die Datei: das ACHTUNG des Lesers (wiederholte Preissätze) steht vorn.
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-gekuerzt-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("gekuerzt.D84");
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp82);
    doc.phase = Phase::Dp84;
    // Byte für Byte (CP850 bleibt CP850).
    let text: String = proava_gaeb::datan::write(&doc)
        .unwrap()
        .iter()
        .map(|&byte| byte as char)
        .collect();
    // Jeden Preissatz (23) zweimal: gleiche OZ, zweiter Preis.
    let doubled: Vec<String> = text
        .lines()
        .flat_map(|line| {
            if line.starts_with("23") {
                vec![line.to_string(), line.to_string()]
            } else {
                vec![line.to_string()]
            }
        })
        .collect();
    let bytes: Vec<u8> = doubled.join("\r\n").chars().map(|ch| ch as u8).collect();
    std::fs::write(&file, bytes).unwrap();
    let import = GaebPdfExchange.import_bid(&project, &file).unwrap();
    assert!(
        import.warnings[0].contains("ACHTUNG") && import.warnings[0].contains("Preissätze (23)"),
        "{:?}",
        import.warnings
    );
    std::fs::remove_dir_all(&folder).unwrap();
}

/// Schlussbemerkung (Anwender 18.11., „Zulieferungen originalgetreu“): ein Text
/// nach dem LV bleibt am Ende – Import ans Ende des LV, DA XML wieder nach dem
/// Award, GAEB 90 als Hinweis am Ende, Druck nach den Summen mit Lesezeichen,
/// Importprotokoll zählt ihn.
#[test]
fn a_closing_text_stays_at_the_end_everywhere() {
    let project = demo_project();
    let (mut doc, _) = lv_to_gaeb(&project, Phase::Dp83);
    doc.award.closing_texts.push(gaeb::AddText {
        outline: "Schlussblatt".into(),
        detail: gaeb::RichText {
            blocks: vec![gaeb::TextBlock::paragraph(vec![gaeb::Run {
                text: "Der Bieter gibt die Zahl der Arbeitskräfte an.".into(),
                ..gaeb::Run::default()
            }])],
        },
    });
    let (imported, warnings) = gaeb_to_project(&doc, ProjectName::parse("RH3").unwrap(), "");
    assert!(
        !warnings.iter().any(|w| w.contains("nach dem LV")),
        "{warnings:?}"
    );
    let LvRow::Note(last) = imported.lv().rows().last().unwrap() else {
        panic!("Schlussbemerkung am Ende erwartet")
    };
    assert!(last.is_closing() && !last.is_preliminary());
    assert_eq!(last.heading(), "Schlussblatt");

    // DA XML: wieder nach dem Award, nicht davor.
    let (again, _) = lv_to_gaeb(&imported, Phase::Dp83);
    assert_eq!(again.award.closing_texts.len(), 1);
    let text = proava_gaeb::xml::write(&again, Phase::Dp83).unwrap();
    assert!(text.find("Schlussblatt").unwrap() > text.find("</Award>").unwrap());

    // GAEB 90: ein Hinweis am Ende des LV (nach dem letzten Titel); die
    // Importvorschau einer X83 zählt die Schlussbemerkung.
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-schluss-")
        .tempdir()
        .unwrap();
    let folder = scratch.path().to_path_buf();
    std::fs::create_dir_all(&folder).unwrap();
    let d83 = folder.join("schluss.D83");
    GaebPdfExchange
        .export_gaeb(
            &imported,
            proava_application::GaebExport::RequestForQuotation,
            &d83,
        )
        .unwrap();
    let back = proava_gaeb::datan::read(&std::fs::read(&d83).unwrap())
        .unwrap()
        .document;
    let Some(gaeb::BoqNode::Remark(remark)) = back.award.boq.body.last() else {
        panic!("Hinweis am Ende: {:?}", back.award.boq.body.last())
    };
    assert!(
        remark.text.to_plain().starts_with("Schlussblatt\n"),
        "Überschrift als erste Zeile: {}",
        remark.text.to_plain()
    );
    let x83 = folder.join("schluss.X83");
    GaebPdfExchange
        .export_gaeb(
            &imported,
            proava_application::GaebExport::RequestForQuotation,
            &x83,
        )
        .unwrap();
    let import =
        proava_application::gaeb_import::read(&GaebPdfExchange, &x83, "2026-11-18T19:00:00")
            .unwrap();
    let preview = proava_application::ImportPreview::of(&import);
    assert_eq!((preview.preliminary, preview.closing), (0, 1));
    std::fs::remove_dir_all(&folder).unwrap();

    // Druck: Modell trägt sie als Schlussbemerkung, das PDF entsteht.
    let print = crate::lv_to_print(&imported, proava_print::LvPrintMode::Blank, "18.11.2026");
    let mut found = false;
    fn walk(items: &[proava_print::PrintItem], found: &mut bool) {
        for item in items {
            match item {
                proava_print::PrintItem::Title(title) => walk(&title.items, found),
                proava_print::PrintItem::Closing(_) => *found = true,
                _ => {}
            }
        }
    }
    walk(&print.items, &mut found);
    assert!(found, "Schlussbemerkung im Druckmodell");
    let (pdf, _) = crate::render_print(
        &imported,
        &proava_application::PrintJob::new(proava_application::PrintKind::RequestForQuotation),
        "18.11.2026",
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
