use std::path::PathBuf;

use proava_application::ProjectRepository;
use proava_domain::{
    Address, Bid, BidPrice, ComplementKind, Decimal, Discount, Lv, LvGroup, LvRow, Money, Note,
    OzMask, PerformanceDescription, Position, PositionKind, Project, ProjectName, RichText, RowId,
    Run, TextBlock, TextComplement,
};
use proava_domain::{Align, Cell, Table};
use proava_infrastructure::project_xml::write_project_xml;
use proava_infrastructure::{project_from_bytes, project_to_bytes, XmlProjectFile};

const TRICKY: &str =
    "Fenster & Türen <außen> \"Typ A\" 'B' ä ö ü ß € 100 %\nzweite Zeile\n\teingerückt";

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempDir(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-test-{label}-"))
            .tempdir()
            .unwrap();
        Self(dir.path().to_path_buf(), dir)
    }
}

fn run(text: &str, bold: bool, italic: bool, underline: bool) -> Run {
    Run {
        text: text.to_string(),
        bold,
        italic,
        underline,
        ..Run::default()
    }
}

fn full_project() -> Project {
    let mask = OzMask::parse("11.22.PPPPI").unwrap();
    let mut lv = Lv::new(format!("LV {TRICKY}"), mask);
    lv.set_position_step(100);
    lv.set_vat_percent(dec("7.5"));
    lv.set_currency("CHF");

    lv.push_group(LvGroup::new(0, "", "Automationsstationen & Schaltschränke"));
    lv.set_number(format!("480-V08 {TRICKY}"));
    lv.set_time_label("Stunden");
    lv.set_level_labels(vec![format!("Los {TRICKY}"), String::new()]);
    lv.set_request_exported(Some("2026-11-17T20:15:00".into()));
    lv.set_sent_fingerprint(Some("0123456789abcdef".into()));
    lv.set_sent_file(Some(format!("/Projekte/{TRICKY}/LV.X83")));
    lv.set_alphanumeric_positions(true);
    lv.set_price_components(vec![
        proava_domain::PriceComponent {
            label: format!("Lohn {TRICKY}"),
            kind: "Wages".into(),
        },
        proava_domain::PriceComponent {
            label: "Stoffe".into(),
            kind: String::new(),
        },
    ]);
    let mut note = Note::new(TRICKY);
    note.set_heading(format!("Überschrift {TRICKY}"));
    note.set_preliminary(true);
    lv.push_note(note);
    lv.push_group(LvGroup::new(1, "", "ASP01"));

    let mut normal = Position::new("", "DDC-Station", dec("1.500"), "St", Some(dec("2499.995")));
    normal.set_long_text(RichText::new(vec![
        TextBlock::paragraph(vec![
            run("fett", true, false, false),
            run(" normal ", false, false, false),
            run("alles", true, true, true),
        ]),
        TextBlock::paragraph(Vec::new()),
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![run("Punkt & Komma", false, true, false)],
            align: Default::default(),
        },
        TextBlock::ListItem {
            ordered: true,
            level: 2,
            runs: vec![run("Erstens\nmit Umbruch", false, false, true)],
            align: Default::default(),
        },
        TextBlock::Table(Table::from_texts(vec![
            vec!["Typ".into(), "Maß <mm>".into()],
            vec!["A".into(), String::new()],
        ])),
        TextBlock::Image {
            file: "schrank.png".into(),
            width_mm: Some(62.5),
        },
        TextBlock::Image {
            file: "detail_2.jpg".into(),
            width_mm: None,
        },
        TextBlock::paragraph(vec![Run::plain(TRICKY)]),
        TextBlock::Paragraph {
            runs: vec![
                Run {
                    font: Some("Times New Roman".into()),
                    size_pt: Some(14.0),
                    ..run("Titel mittig", true, false, false)
                },
                Run {
                    font: Some("Courier New".into()),
                    size_pt: Some(8.5),
                    ..run(" klein", false, false, false)
                },
            ],
            align: Align::Center,
        },
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![run("rechts", false, false, false)],
            align: Align::Right,
        },
        TextBlock::Table(Table {
            rows: vec![
                vec![Cell::plain("Kopf"), Cell::plain("Wert")],
                vec![
                    Cell {
                        runs: vec![run("fett", true, false, false)],
                        align: Align::Justify,
                    },
                    Cell {
                        runs: vec![run(TRICKY, false, true, false)],
                        align: Align::Right,
                    },
                ],
            ],
            header_rows: 1,
            column_widths_mm: vec![None, Some(42.5)],
        }),
    ]));
    normal.set_quantity_formula(Some("=1+0,5 & <x>".into()));
    let normal_id = lv.push_position(normal);
    let mut performance = PerformanceDescription::new(
        "12",
        format!("Trassen {TRICKY}"),
        RichText::new(vec![TextBlock::paragraph(vec![run(
            TRICKY, true, false, false,
        )])]),
    );
    performance.set_short_text("Kabeltrassen");
    performance.set_catalog(proava_domain::CatalogKeys {
        stlb: Some(proava_domain::StlbReference {
            catalog: "STLB-Bau".into(),
            group: 7,
            ..Default::default()
        }),
        standard_number: None,
    });
    let performance_id = lv.push_row(LvRow::Performance(performance));
    lv.link_performance(normal_id, Some(performance_id))
        .unwrap();

    let mut repeated = Position::new("", "wie vor, jedoch", dec("2"), "St", None);
    repeated.set_quantity_open(true);
    repeated.set_price_breakdown(true);
    repeated.set_not_applicable(true);
    repeated.set_key_item(true);
    repeated.set_status(proava_domain::EditStatus::Done);
    repeated.set_price_components(vec![Some(dec("12.5")), None, Some(dec("0"))]);
    repeated.set_time_quantity(Some(dec("0.75")));
    repeated.set_text_origin(Some(proava_domain::TextOrigin {
        source: "AUSSCHREIBEN.DE".into(),
        supplier: format!("Muster {TRICKY}"),
        retrieved: "2026-11-17".into(),
        file: "Merkliste <1>.X83".into(),
        reference: String::new(),
        note: String::new(),
    }));
    repeated.set_sub_descriptions(vec![proava_domain::SubDescription {
        number: "01".into(),
        short_text: format!("Variante {TRICKY}"),
        long_text: RichText::plain(TRICKY),
        quantity: Some(dec("2.5")),
        unit: "St".into(),
        catalog: proava_domain::CatalogKeys {
            stlb: None,
            standard_number: Some(proava_domain::StandardNumber {
                number: "9300201123000202".into(),
                catalog: "StLB".into(),
            }),
        },
    }]);
    repeated.set_complements(vec![
        TextComplement {
            marker: "01".into(),
            kind: ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            body: String::new(),
            tail: TRICKY.into(),
            anchor: Some(proava_domain::TextAnchor {
                block: 2,
                offset: 7,
                own_line: false,
            }),
        },
        TextComplement {
            marker: "02".into(),
            kind: ComplementKind::Owner,
            caption: String::new(),
            body: TRICKY.into(),
            tail: String::new(),
            anchor: Some(proava_domain::TextAnchor {
                block: 1,
                offset: 0,
                own_line: true,
            }),
        },
    ]);
    let repeated_id = lv.push_position(repeated);
    lv.link_repetition(repeated_id, Some(normal_id)).unwrap();

    let mut provisional = Position::new("", "Bedarf mit GB", dec("3"), "m", Some(dec("10")));
    provisional.set_kind(PositionKind::Provisional { with_total: true });
    let provisional_id = lv.push_position(provisional);

    let mut provisional_without = Position::new("", "Bedarf ohne GB", dec("0"), "m", None);
    provisional_without.set_kind(PositionKind::Provisional { with_total: false });
    let removed_id = lv.push_position(provisional_without);

    lv.push_group(LvGroup::new(1, "", "Wahl"));
    let mut base = Position::new("", "Grundposition", dec("2"), "St", Some(dec("100")));
    base.set_kind(PositionKind::Base { group: 3 });
    let base_id = lv.push_position(base);
    let mut alternative = Position::new("", "Wahlposition", dec("2"), "St", Some(dec("120")));
    alternative.set_kind(PositionKind::Alternative {
        group: 3,
        serial: 2,
    });
    lv.push_position(alternative);
    let mut supplement = Position::new("", "Zulage", dec("4"), "St", Some(dec("-5.25")));
    supplement.set_kind(PositionKind::Supplement);
    lv.push_position(supplement);

    lv.push_group(LvGroup::new(0, "", "Pauschal und Stundenlohn"));
    let mut lump = Position::new("", "Inbetriebnahme", dec("1"), "psch", Some(dec("5000")));
    lump.set_lump_sum(true);
    lv.push_position(lump);
    let mut hourly = Position::new("", "Monteur", dec("40"), "h", Some(dec("68.50")));
    hourly.set_hourly(true);
    lv.push_position(hourly);
    lv.push_position(Position::stlb("Text folgt aus STLB-Bau"));
    lv.push_note(Note::rich(RichText::new(vec![TextBlock::ListItem {
        ordered: true,
        level: 0,
        runs: vec![run("Hinweis formatiert", true, false, false)],
        align: Default::default(),
    }])));
    // Schlussbemerkung (Text nach dem LV) at the end.
    let mut closing = Note::new("Der Bieter gibt die Zahl der Arbeitskräfte an.");
    closing.set_heading("Schlussblatt");
    closing.set_closing(true);
    lv.push_note(closing);
    lv.renumber();

    // A deleted row: its id must not come back after loading.
    let removed_index = lv.index_of(removed_id).unwrap();
    lv.remove_row(removed_index);

    let mut project = Project::new(
        ProjectName::parse("Schulzentrum & Co").unwrap(),
        "2026-11-17T18:00:00",
        lv,
    );
    let info = project.info_mut();
    info.label = format!("Neubau {TRICKY}");
    info.short_name = format!("40651 {TRICKY}");
    info.award_number = "V-2026/17".into();
    info.submission_deadline = "15.12.2026, 10:00 Uhr".into();
    info.submission_place = "Rathaus, Zimmer 3".into();
    info.owner = Address {
        name1: "Stadt Musterstadt".into(),
        name2: "Gebäudewirtschaft".into(),
        name3: format!("Abteilung {TRICKY}"),
        name4: "Sachgebiet Technik".into(),
        street: "Schwanenstraße 5–7".into(),
        postal_code: "12345".into(),
        city: "Musterstadt".into(),
        country: "Deutschland".into(),
        iln: "4012345000009".into(),
        contact: "Frau Muster".into(),
        phone: "+49 203 0".into(),
        fax: "+49 203 1".into(),
        email: "a@b.de".into(),
        vat_id: "DE123456789".into(),
    };
    info.exchange_number = "430-V03".into();
    info.site_number = format!("B-1 {TRICKY}");
    info.site_name = "Haus A".into();
    info.notification_site = Some(proava_domain::Place {
        address: Address::named("Bauleitung"),
        number: "N-1".into(),
        name: "Container".into(),
    });
    info.further_sites = vec![proava_domain::ExecutionSite {
        site: proava_domain::Place {
            address: Address::default(),
            number: "B-2".into(),
            name: "Haus B".into(),
        },
        notification: None,
    }];
    info.requesters = vec![proava_domain::Place {
        address: Address::named("Schulamt"),
        number: "R-1".into(),
        name: String::new(),
    }];
    info.client_account = "D-4711".into();
    info.client_country_kind = "EEA".into();
    info.client_country_code = "D".into();
    info.planner = Address::named("Planungsbüro Muster");
    info.page_frame = Some(proava_domain::PageFrame {
        header: [
            format!("{{projekt}} {}", TRICKY.lines().next().unwrap()),
            "{dokumentart}\nVorabzug".into(),
            String::new(),
        ],
        footer: ["{lv}".into(), String::new(), "S. {seite}/{seiten}".into()],
        header_rule: false,
        footer_rule: true,
        logo: proava_domain::FrameLogoPlace::Left,
        font_size_pt: 9,
    });
    info.own_client = Some(Address {
        name1: "Gebäudemanagement Musterstadt".into(),
        city: "Musterstadt".into(),
        ..Address::default()
    });

    assert_eq!(
        project.add_image("schrank.png", vec![0x89, b'P', b'N', b'G', 0, 1, 2, 255]),
        "schrank.png"
    );
    assert_eq!(
        project.add_image("detail_2.jpg", (0..=255).collect()),
        "detail_2.jpg"
    );

    let mut first = Bid::new(
        Address {
            name1: "Müller & Söhne GmbH".into(),
            city: "Köln".into(),
            ..Address::default()
        },
        "mueller.x84",
    );
    first.set_bid_date(Some("2026-12-14".into()));
    first.set_exchange_number("AN-17");
    first.set_discount(Discount::Percent(dec("3.5")));
    first.set_price(
        normal_id,
        BidPrice {
            unit_price: Some(dec("2400.10")),
            stated_total: Some(dec("3600.15")),
            not_offered: false,
        },
    );
    first.set_price(
        provisional_id,
        BidPrice {
            unit_price: None,
            stated_total: None,
            not_offered: true,
        },
    );
    first.set_unknown_ordinals(vec!["99.99.0010".into(), "x & y".into()]);
    // Abweichungsprotokoll with every part (Anwender 20.11.).
    let item = |ordinal: &str| proava_domain::BidItem {
        ordinal: ordinal.into(),
        short_text: format!("Text {TRICKY}"),
        long_text: "Lang\nzweite Zeile".into(),
        unit: "St".into(),
        quantity: Some(dec("2")),
        unit_price: Some(dec("10.5")),
        total: Some(dec("21")),
        not_offered: false,
        kind: "Wahlposition".into(),
    };
    first.set_structure(proava_domain::StructureReport {
        foreign: vec![item("99.99.0010")],
        assigned: vec![proava_domain::Assignment {
            item: item("98.0010"),
            row: provisional_id,
            by: proava_domain::AssignedBy::Hand,
            similarity: 87,
        }],
        conflicts: vec![(normal_id, item("97.0010"))],
        suggestions: vec![proava_domain::StoredSuggestion {
            ordinal: "97.0010".into(),
            row: base_id,
            similarity: 95,
            sure: true,
        }],
        quantities: vec![(normal_id, dec("1.5"))],
        units: vec![(normal_id, "m".into())],
        short_texts: vec![(normal_id, "Anderer Text".into())],
        long_texts: vec![(normal_id, 42)],
        kinds: vec![(normal_id, "Bedarfsposition ohne GB".into())],
        titles: vec!["Titel 02 „Neu“ nur im Angebot".into()],
        file_total: Some(Money::from_cents(1_234_567)),
    });
    first.set_complements(
        normal_id,
        vec![proava_domain::BidComplement {
            marker: "01".into(),
            text: format!("Fabrikat {TRICKY}"),
        }],
    );
    first.set_award(Some(proava_domain::Award {
        date: "01.12.2026".into(),
        total: Money::from_cents(123456),
    }));
    let first_id = project.add_bid(first);

    let mut second = Bid::new(Address::named("Schmidt"), "schmidt.d84");
    second.set_discount(Discount::Amount(Money::from_cents(-12345)));
    second.set_excluded(true);
    second.set_price(
        base_id,
        BidPrice {
            unit_price: Some(dec("99")),
            stated_total: None,
            not_offered: false,
        },
    );
    project.add_bid(second);
    let mut third = Bid::new(Address::named("Weg"), "weg.x84");
    third.set_discount(Discount::None);
    project.add_bid(third);
    let third_id = project.bids()[2].id();
    project.remove_bid(third_id);
    assert_ne!(first_id, third_id);

    // Bieterverzeichnis
    let mut invited = proava_domain::InvitedBidder::new(Address {
        name1: "Müller GmbH".into(),
        city: "Musterstadt".into(),
        email: "info@mueller.example".into(),
        ..Address::default()
    });
    invited.contact_person = "Frau Müller".into();
    invited.requested = "01.11.2026".into();
    invited.side_offers = true;
    invited.remark = "Nebenangebot „Fabrikat B“".into();
    invited.rejection_reason = "Preis nicht wirtschaftlich\nzweite Zeile".into();
    let invited = project.add_invitee(invited);
    project.link_bid(first_id, Some(invited)).unwrap();
    let mut refused = proava_domain::InvitedBidder::new(Address::named("Absage AG"));
    refused.status = proava_domain::InviteStatus::NoBid;
    project.add_invitee(refused);

    // Schriftverkehr: geänderte Vorlage
    let mut letter = proava_domain::LetterTemplate::standard(proava_domain::LetterKind::Award);
    letter.subject = "Auftrag – {lv} (Los „A“ & B)".into();
    project.set_letter_template(letter);
    project
}

#[test]
fn full_project_survives_save_and_load() {
    let folder = TempDir::new("roundtrip");
    let file = folder.0.join("Projekt.proava");
    let original = full_project();
    let store = XmlProjectFile::new();

    store.save_project(&file, &original).unwrap();
    assert!(!folder.0.join("Projekt.proava.tmp").exists());
    let content = std::fs::read(&file).unwrap();
    assert!(content.starts_with(b"PK"));

    let loaded = store.load_project(&file).unwrap();
    assert_eq!(loaded, original);
    assert_eq!(loaded.lv().rows().len(), original.lv().rows().len());
    for (left, right) in loaded.lv().rows().iter().zip(original.lv().rows()) {
        assert_eq!(left, right);
        assert_eq!(left.id(), right.id());
    }
    assert_eq!(loaded.info(), original.info());
    assert_eq!(
        loaded.info().client().name1,
        "Gebäudemanagement Musterstadt",
        "eigener Auftraggeber neben dem Bauherrn"
    );
    assert_eq!(loaded.images(), original.images());
    assert_eq!(loaded.bids(), original.bids());
    assert_eq!(loaded.invitees(), original.invitees());
    assert_eq!(loaded.invitees().len(), 2);
    assert_eq!(loaded.letter_templates(), original.letter_templates());
    assert_eq!(loaded.letter_templates().len(), 1);
    assert_eq!(loaded.lv().next_row_id(), original.lv().next_row_id());
    assert_eq!(loaded.next_bid_id(), original.next_bid_id());
    assert_eq!(loaded.lv().net_total(), original.lv().net_total());

    // Saving the loaded project again gives the same content.
    assert_eq!(
        write_project_xml(&loaded).unwrap(),
        write_project_xml(&original).unwrap()
    );
}

#[test]
fn texts_for_the_search_come_without_images_and_folders_are_listed() {
    let folder = TempDir::new("texts");
    let file = folder.0.join("Projekt.proava");
    let original = full_project();
    let store = XmlProjectFile::new();
    store.save_project(&file, &original).unwrap();
    assert!(!original.images().is_empty());

    let texts = store.load_project_texts(&file).unwrap();
    assert!(texts.images().is_empty(), "only projekt.xml is read");
    assert_eq!(texts.lv().rows().len(), original.lv().rows().len());
    for (left, right) in texts.lv().rows().iter().zip(original.lv().rows()) {
        assert_eq!(left.id(), right.id(), "ids as in the full project");
        assert_eq!(left.short_text(), right.short_text());
    }
    assert_eq!(texts.lv().net_total(), original.lv().net_total());

    // Schema 1 (plain XML) and broken files.
    std::fs::write(
        folder.0.join("Alt.proava"),
        r#"<?xml version="1.0"?><proAVA schema="1"><project name="Alt" created="2026-11-10T09:00:00"/><lv title="GA" oz-mask="11.22.PPPPI" position-step="10"><position oz="01.01.0010" short="DDC" lang="" quantity="1" unit="St" unit-price-cents="0"/></lv></proAVA>"#,
    )
    .unwrap();
    let old = store
        .load_project_texts(&folder.0.join("Alt.proava"))
        .unwrap();
    assert_eq!(old.lv().position_count(), 1);
    std::fs::write(folder.0.join("Kaputt.proava"), b"PK\x03\x04kaputt").unwrap();
    let error = store
        .load_project_texts(&folder.0.join("Kaputt.proava"))
        .unwrap_err();
    assert!(error.contains("Kaputt.proava"), "{error}");

    std::fs::create_dir_all(folder.0.join("b Unterordner")).unwrap();
    std::fs::create_dir_all(folder.0.join("A Ordner")).unwrap();
    let (folders, files) = store.folder_entries(&folder.0);
    let names = |paths: &[PathBuf]| {
        paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&folders), ["A Ordner", "b Unterordner"]);
    assert_eq!(
        names(&files),
        ["Alt.proava", "Kaputt.proava", "Projekt.proava"]
    );
    assert_eq!(
        store.folder_entries(&folder.0.join("gibt es nicht")),
        (Vec::new(), Vec::new())
    );
}

#[test]
fn performance_descriptions_and_references_survive() {
    let original = full_project();
    let loaded = project_from_bytes(&project_to_bytes(&original).unwrap()).unwrap();
    let performance = loaded.lv().performance_by_number("12").unwrap();
    assert_eq!(performance.short_text(), "Kabeltrassen");
    assert_eq!(performance.label(), format!("Trassen {TRICKY}"));
    assert_eq!(performance.long_text().to_plain(), TRICKY);
    let users = loaded.lv().positions_using(performance.id());
    assert_eq!(users.len(), 1);
    assert_eq!(
        loaded.lv().position_by_id(users[0]).unwrap().short_text(),
        "DDC-Station"
    );

    // A reference written before its description is kept, one to a missing
    // description is dropped.
    let xml = write_project_xml(&original).unwrap();
    let id = performance.id().0;
    let start = xml.find("<performance").unwrap();
    let end = xml[start..].find("</performance>").unwrap() + start + "</performance>".len();
    let block = xml[start..end].to_string();
    let moved = xml
        .replacen(&block, "", 1)
        .replacen("</lv>", &format!("{block}</lv>"), 1);
    let project = proava_infrastructure::project_xml::read_project_xml(&moved, |_| None).unwrap();
    assert_eq!(project.lv().positions_using(RowId(id)).len(), 1);
    let dangling = xml.replacen(&block, "", 1);
    let project =
        proava_infrastructure::project_xml::read_project_xml(&dangling, |_| None).unwrap();
    assert!(project
        .lv()
        .positions()
        .all(|position| position.performance().is_none()));
}

/// Unterpunkte keep their level; the attribute is only written for them,
/// so files of earlier versions (without it) read as level 0.
#[test]
fn list_levels_are_kept_and_old_files_read_as_level_0() {
    let original = full_project();
    let xml = write_project_xml(&original).unwrap();
    assert!(xml.contains("level=\"2\""), "Unterpunkt mit Ebene");
    assert!(
        xml.split("<li ")
            .skip(1)
            .all(|tag| !tag[..tag.find('>').unwrap()].contains("level=\"0\"")),
        "Ebene 0 ohne Attribut"
    );
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    let levels = |project: &proava_domain::Project| -> Vec<(bool, u8)> {
        project
            .lv()
            .positions()
            .flat_map(|position| position.long_text().blocks().to_vec())
            .filter_map(|block| block.list_kind())
            .collect()
    };
    assert_eq!(levels(&loaded), levels(&original));
    assert!(levels(&loaded).contains(&(true, 2)));
    let old = xml.replace(" level=\"2\"", "");
    let project = proava_infrastructure::project_xml::read_project_xml(&old, |_| None).unwrap();
    assert!(levels(&project).iter().all(|(_, level)| *level == 0));
}

#[test]
fn position_flags_complements_repetitions_and_headings_survive() {
    let original = full_project();
    let loaded = project_from_bytes(&project_to_bytes(&original).unwrap()).unwrap();
    let lv = loaded.lv();
    assert_eq!(lv.number(), format!("480-V08 {TRICKY}"));
    assert_eq!(loaded.info().short_name, format!("40651 {TRICKY}"));
    let repeated = lv
        .positions()
        .find(|position| position.short_text() == "wie vor, jedoch")
        .unwrap();
    assert!(repeated.is_quantity_open() && repeated.has_price_breakdown());
    assert!(repeated.is_not_applicable() && repeated.is_key_item());
    assert_eq!(repeated.status(), proava_domain::EditStatus::Done);
    assert!(lv
        .positions()
        .filter(|position| position.short_text() != "wie vor, jedoch")
        .all(|position| position.status() == proava_domain::EditStatus::Open));
    assert_eq!(repeated.complements().len(), 2);
    assert_eq!(repeated.complements()[0].tail, TRICKY);
    assert_eq!(repeated.complements()[1].kind, ComplementKind::Owner);
    assert_eq!(
        repeated.complements()[0].anchor,
        Some(proava_domain::TextAnchor {
            block: 2,
            offset: 7,
            own_line: false
        })
    );
    assert!(repeated.complements()[1].anchor.unwrap().own_line);
    let base = lv.position_by_id(repeated.repeats().unwrap()).unwrap();
    assert_eq!(base.short_text(), "DDC-Station");
    let note = lv.rows().iter().find_map(|row| match row {
        LvRow::Note(note) if !note.heading().is_empty() => Some(note),
        _ => None,
    });
    assert_eq!(note.unwrap().heading(), format!("Überschrift {TRICKY}"));
    assert!(note.unwrap().is_preliminary());
    let LvRow::Note(last) = lv.rows().last().unwrap() else {
        panic!("Schlussbemerkung am Ende")
    };
    assert!(last.is_closing() && !last.is_preliminary());
    assert_eq!(last.heading(), "Schlussblatt");
}

#[test]
fn surcharges_and_marks_survive() {
    use proava_domain::{Surcharge, SurchargeKind, SurchargeReference};
    let mut lv = Lv::new("Zuschläge", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Rohbau"));
    lv.push_position(Position::new(
        "01.0010",
        "Aushub",
        dec("100"),
        "m3",
        Some(dec("12.5")),
    ));
    let mut marked = Position::new("01.0020", "Verfüllen", dec("40"), "m3", Some(dec("8")));
    marked.set_marked_for_surcharge(true);
    lv.push_position(marked);
    let first = lv.position_by_ordinal("01.0010").unwrap().id();
    for (ordinal, kind, references) in [
        ("01.0030", SurchargeKind::AllInList, Vec::new()),
        ("01.0040", SurchargeKind::Marked, Vec::new()),
        (
            "01.0050",
            SurchargeKind::Listed,
            vec![SurchargeReference {
                position: first,
                quantity: Some(dec("2.5")),
            }],
        ),
    ] {
        let mut position = Position::new(ordinal, "Zuschlag", Decimal::ZERO, "", Some(dec("3.5")));
        // The last one an Aufteilungszeile (Kosten-LV): the flag survives too.
        let split = ordinal.ends_with("0050");
        position.set_surcharge(Some(Surcharge {
            kind,
            references,
            split,
        }));
        lv.push_position(position);
    }
    let original = Project::new(ProjectName::parse("ZS").unwrap(), "2026-11-18T12:00:00", lv);
    let loaded = project_from_bytes(&project_to_bytes(&original).unwrap()).unwrap();
    let lv = loaded.lv();
    assert_eq!(lv.positions().count(), 5);
    for (before, after) in original.lv().positions().zip(lv.positions()) {
        assert_eq!(
            after.surcharge(),
            before.surcharge(),
            "{}",
            before.ordinal()
        );
        assert_eq!(after.unit_price(), before.unit_price());
        assert_eq!(
            after.is_marked_for_surcharge(),
            before.is_marked_for_surcharge()
        );
    }
    assert_eq!(lv.net_total(), original.lv().net_total());
    assert_eq!(lv.lump_sum_total(), None);
    // The Pauschalsumme of the LV and of a title survive.
    let mut lump = original.clone();
    lump.lv_mut()
        .set_lump_sum_total(Some(Money::from_cents(123_456)));
    if let Some(LvRow::Group(group)) = lump.lv_mut().row_mut(0) {
        group.set_lump_sum_total(Some(Money::from_cents(7_890)));
    }
    let loaded = project_from_bytes(&project_to_bytes(&lump).unwrap()).unwrap();
    assert_eq!(
        loaded.lv().lump_sum_total(),
        Some(Money::from_cents(123_456))
    );
    assert_eq!(
        loaded.lv().rows()[0].as_group().unwrap().lump_sum_total(),
        Some(Money::from_cents(7_890))
    );
    // A file without Zuschläge has none (older files stay readable).
    let plain = full_project();
    let loaded = project_from_bytes(&project_to_bytes(&plain).unwrap()).unwrap();
    assert!(loaded
        .lv()
        .positions()
        .all(|position| !position.is_surcharge() && !position.is_marked_for_surcharge()));
}

#[test]
fn special_characters_are_not_escaped_twice() {
    let project = full_project();
    let bytes = project_to_bytes(&project).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("projekt.xml").unwrap(), &mut xml).unwrap();
    assert!(
        xml.contains("Fenster &amp; Türen &lt;außen&gt; &quot;Typ A&quot;"),
        "{xml}"
    );
    assert!(!xml.contains("&amp;amp;"));
    assert!(xml.contains("ä ö ü ß €"));
    assert!(archive.by_name("bilder/schrank.png").is_ok());

    let loaded = project_from_bytes(&bytes).unwrap();
    let LvRow::Note(note) = &loaded.lv().rows()[1] else {
        panic!("note expected");
    };
    assert_eq!(note.text().to_plain(), TRICKY);
    assert_eq!(loaded.name().as_str(), "Schulzentrum & Co");
}

#[test]
fn schema_1_files_are_read() {
    // As written by proAVA 0.2–0.4, including its double escaping and raw line breaks.
    let old = r#"<?xml version="1.0" encoding="utf-8"?>
<proAVA schema="1">
  <project name="Altes &amp;amp; Projekt" created="2026-11-10T09:00:00"/>
  <lv title="GA &amp;amp; MSR" oz-mask="11.22.PPPPI" position-step="100">
    <group oz="01" short="Stationen"/>
    <note text="Hinweis &amp;lt;wichtig&amp;gt;"/>
    <group oz="01.01" short="ASP01"/>
    <position oz="01.01.0100" short="DDC" lang="Zeile 1
Zeile 2" quantity="2.5" unit="St" unit-price-cents="123456"/>
    <position oz="01.01.0200" short="Platzhalter" lang="" quantity="0" unit="" unit-price-cents="0" stlb="1"/>
  </lv>
</proAVA>
"#;
    let project = project_from_bytes(old.as_bytes()).unwrap();
    assert_eq!(project.name().as_str(), "Altes & Projekt");
    assert_eq!(
        project.info().own_client,
        None,
        "alte Dateien: Auftraggeber wie Bauherr"
    );
    assert_eq!(project.created_iso(), "2026-11-10T09:00:00");
    let lv = project.lv();
    assert_eq!(lv.title(), "GA & MSR");
    assert_eq!(lv.position_step(), 100);
    assert_eq!(lv.rows().len(), 5);
    assert_eq!(lv.rows()[0].as_group().unwrap().level(), 0);
    assert_eq!(lv.rows()[2].as_group().unwrap().level(), 1);
    let LvRow::Note(note) = &lv.rows()[1] else {
        panic!("note expected");
    };
    assert_eq!(note.text().to_plain(), "Hinweis <wichtig>");
    let ddc = lv.rows()[3].as_position().unwrap();
    assert_eq!(ddc.quantity(), dec("2.5"));
    assert_eq!(ddc.unit_price(), Some(dec("1234.56")));
    assert_eq!(ddc.long_text(), &RichText::plain("Zeile 1\nZeile 2"));
    let placeholder = lv.rows()[4].as_position().unwrap();
    assert!(placeholder.is_stlb_placeholder());
    assert_eq!(placeholder.unit_price(), None);

    // Saved again it becomes schema 2 and stays the same.
    let again = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(again, project);
}

#[test]
fn backup_and_restore_with_container_and_old_files() {
    let folder = TempDir::new("backup");
    let store = XmlProjectFile::new();
    let file = folder.0.join("Projekt.proava");
    let project = full_project();
    store.save_project(&file, &project).unwrap();

    let backup = store.backup(&file, &folder.0.join("Sicherungen")).unwrap();
    let name = backup.file_name().unwrap().to_string_lossy().to_string();
    assert!(
        name.starts_with("Projekt_Sicherung_") && name.ends_with(".zip"),
        "{name}"
    );

    let restored = store.restore(&backup, &folder.0.join("zurück")).unwrap();
    let restored_name = restored.file_name().unwrap().to_string_lossy().to_string();
    assert!(
        restored_name.starts_with("Projekt_wiederhergestellt_"),
        "{restored_name}"
    );
    assert!(restored_name.ends_with(".proava"));
    assert_eq!(store.load_project(&restored).unwrap(), project);

    // An old backup: zip with a schema-1 XML inside.
    let old_backup = folder.0.join("Alt_Sicherung_2026-11-10_0900.zip");
    {
        let handle = std::fs::File::create(&old_backup).unwrap();
        let mut zip = zip::ZipWriter::new(handle);
        zip.start_file("Alt.proava", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(
            &mut zip,
            br#"<proAVA schema="1"><project name="Alt" created=""/><lv title="LV" oz-mask="11.22.PPPPI" position-step="10"><position oz="01.01.0010" short="x" lang="" quantity="1" unit="St" unit-price-cents="100"/></lv></proAVA>"#,
        )
        .unwrap();
        zip.finish().unwrap();
    }
    let restored = store.restore(&old_backup, &folder.0).unwrap();
    assert!(restored
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("Alt_wiederhergestellt_"));
    let old = store.load_project(&restored).unwrap();
    assert_eq!(old.name().as_str(), "Alt");
    assert_eq!(
        old.lv().positions().next().unwrap().unit_price(),
        Some(dec("1"))
    );

    // A backup that holds no readable project is refused.
    let broken = folder.0.join("Kaputt_Sicherung_x.zip");
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&broken).unwrap());
        zip.start_file("Kaputt.proava", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut zip, b"kein Projekt").unwrap();
        zip.finish().unwrap();
    }
    assert!(store.restore(&broken, &folder.0).is_err());
}

#[test]
fn saving_replaces_the_file_and_leaves_no_temporary_file() {
    let folder = TempDir::new("atomic");
    let store = XmlProjectFile::new();
    let file = store
        .create_project(&folder.0, &ProjectName::parse("Neu").unwrap())
        .unwrap();
    let mut project = store.load_project(&file).unwrap();
    project.lv_mut().set_title("Geändert");
    store.save_project(&file, &project).unwrap();
    assert_eq!(store.load_project(&file).unwrap().lv().title(), "Geändert");
    let names: Vec<String> = std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["Neu.proava".to_string()]);
}

#[test]
fn newer_schema_and_foreign_files_are_refused() {
    assert!(
        project_from_bytes(br#"<proAVA schema="3"><project name="x"/></proAVA>"#)
            .unwrap_err()
            .contains("Schema 3")
    );
    assert!(project_from_bytes(b"<html/>").is_err());
    assert!(project_from_bytes(b"irgendwas").is_err());
}

#[test]
fn several_lvs_and_master_data_survive() {
    use proava_domain::{Contact, ContractTerms, Deduction};
    let mut project = full_project();
    {
        let info = project.info_mut();
        info.description = TRICKY.to_string();
        info.site = Address {
            name1: "Baustelle Parkstraße".into(),
            city: "Musterstadt".into(),
            ..Address::default()
        };
        info.contacts = vec![Contact {
            role: "Architekt".into(),
            address: Address::named("Architekten & Partner"),
        }];
        info.award_procedure = "Öffentliche Ausschreibung".into();
        info.binding_period_end = "31.01.2027".into();
        info.execution_start = "01.03.2027".into();
        info.execution_end = "30.11.2027".into();
        info.execution_note = "ASP01 bis KW 20".into();
        info.terms = ContractTerms {
            performance_security_percent: Some(dec("5")),
            warranty_security_percent: Some(dec("3")),
            cash_discount_percent: Some(dec("2")),
            cash_discount_days: Some(14),
            deductions: vec![Deduction {
                label: "Bauwesenversicherung".into(),
                percent: dec("0.3"),
            }],
            payment_terms: "Abschläge nach Baufortschritt".into(),
            notes: TRICKY.into(),
        };
    }
    let mut second = Lv::new("Los 2 – MSR", OzMask::parse("11.PPPP").unwrap());
    second.set_number("482");
    second.push_group(LvGroup::new(0, "01", "Schaltschränke"));
    second.push_position(Position::new(
        "01.0010",
        "Schrank",
        dec("1"),
        "St",
        Some(dec("100")),
    ));
    let index = project.add_lv(second);
    project.add_bid_to(index, Bid::new(Address::named("Los-2-Bieter"), "los2.x84"));
    project.set_active_lv(index);

    let bytes = project_to_bytes(&project).unwrap();
    let back = project_from_bytes(&bytes).unwrap();
    assert_eq!(back, project);
    assert_eq!(back.lv_count(), 2);
    assert_eq!(back.active_lv(), 1);
    assert_eq!(back.bids().len(), 1);
    assert_eq!(back.bids()[0].name(), "Los-2-Bieter");
    assert_eq!(back.bids_of(0).len(), project.bids_of(0).len());
    assert_eq!(back.info().terms.cash_discount_days, Some(14));
}

#[test]
fn schema_2_texts_before_formatting_still_load() {
    // Langtext as written up to 17.11.2026: cells with text in `t`, no align/font/size.
    let old = r#"<?xml version="1.0" encoding="utf-8"?>
<proAVA schema="2">
  <project name="Alt" created="2026-11-16T09:00:00"/>
  <lv title="LV" oz-mask="11.22.PPPPI" position-step="10">
    <position id="1" oz="01.01.0010" short="DDC" quantity="1" unit="St">
      <text>
        <p><r b="1" t="fett"/><r t=" normal"/></p>
        <li ordered="1"><r t="eins"/></li>
        <table><tr><td t="A"/><td t="B"/></tr><tr><td t="1"/><td t=""/></tr></table>
      </text>
    </position>
  </lv>
</proAVA>
"#;
    let project = proava_infrastructure::project_xml::read_project_xml(old, |_| None).unwrap();
    let position = project
        .lv()
        .rows()
        .iter()
        .find_map(LvRow::as_position)
        .unwrap();
    let blocks = position.long_text().blocks();
    assert_eq!(
        blocks[0],
        TextBlock::paragraph(vec![run("fett", true, false, false), Run::plain(" normal")])
    );
    assert_eq!(
        blocks[1],
        TextBlock::list_item(true, vec![Run::plain("eins")])
    );
    assert_eq!(
        blocks[2],
        TextBlock::Table(Table::from_texts(vec![
            vec!["A".into(), "B".into()],
            vec!["1".into(), String::new()],
        ]))
    );
}

#[test]
fn stlb_keys_survive_save_and_load() {
    use proava_domain::{StandardNumber, StlbCharacteristic, StlbReference};
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    let mut from_stlb = Position::new("", "Raumbediengerät", dec("4"), "St", None);
    from_stlb.set_long_text(RichText::plain("Unterputz, Farbe 'reinweiß'"));
    from_stlb.attach_stlb(StlbReference {
        catalog: "STLB-Bau".into(),
        version: Some("2026-04".into()),
        work_category: "070".into(),
        group: 9001,
        cost_group: "481".into(),
        characteristics: vec![
            StlbCharacteristic {
                characteristic: 90011,
                index: Some(0),
                value: Some(2),
                short_text_part: true,
            },
            StlbCharacteristic {
                characteristic: 90013,
                index: None,
                value: None,
                short_text_part: false,
            },
        ],
        owner_complements: true,
        bidder_complements: false,
        text_fingerprint: None,
        legacy_fingerprint: None,
        complement_links: vec![("1".into(), 90013), ("2".into(), 90014)],
        complement_numbers: vec![
            ("2".into(), proava_domain::ComplementNumber::Decimal),
            ("3".into(), proava_domain::ComplementNumber::Integer),
        ],
    });
    lv.push_position(from_stlb);
    let mut old = Position::new("", "Alte StLB-Nummer", dec("10"), "m", None);
    old.set_standard_number(Some(StandardNumber {
        number: "12345678901234567".into(),
        catalog: "StLB".into(),
    }));
    lv.push_position(old);
    lv.push_position(Position::new("", "Frei", dec("1"), "St", None));
    lv.renumber();
    let project = Project::new(
        ProjectName::parse("STLB").unwrap(),
        "2026-11-17T18:00:00",
        lv,
    );
    let again = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(again, project);
    let first = again.lv().positions().next().unwrap();
    assert!(first.stlb_reference().unwrap().text_fingerprint.is_some());
    assert!(!first.stlb_text_changed());
    assert_eq!(
        first.catalog_line().as_deref(),
        Some("STLB-Bau 4/2026 070 TA")
    );
}

#[test]
fn edit_status_is_written_only_when_set() {
    let mut original = full_project();
    let id = original.lv().positions().next().unwrap().id();
    original
        .lv_mut()
        .set_status(&[id], proava_domain::EditStatus::InProgress);
    let xml = proava_infrastructure::project_xml::write_project_xml(&original).unwrap();
    assert_eq!(xml.matches("status=\"in-progress\"").count(), 1, "{xml}");
    assert_eq!(xml.matches("status=\"done\"").count(), 1);
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    assert_eq!(
        loaded.lv().position_by_id(id).unwrap().status(),
        proava_domain::EditStatus::InProgress
    );
    // Unbekannte Werte werden nicht still zu „offen“.
    let broken = xml.replacen("status=\"done\"", "status=\"fertich\"", 1);
    assert!(proava_infrastructure::project_xml::read_project_xml(&broken, |_| None).is_err());
}

#[test]
fn cost_groups_and_catalog_survive() {
    let mut original = full_project();
    let lv = original.lv_mut();
    let title = lv.rows().iter().position(|row| row.is_group()).unwrap();
    if let Some(LvRow::Group(group)) = lv.row_mut(title) {
        group.set_cost_group(Some("481".into()));
    }
    let position = lv.rows().iter().position(|row| row.is_position()).unwrap();
    if let Some(LvRow::Position(position)) = lv.row_mut(position) {
        position.set_cost_group(Some("482".into()));
    }
    let xml = proava_infrastructure::project_xml::write_project_xml(&original).unwrap();
    assert!(
        !xml.contains("cost-catalog-id"),
        "Standardkatalog wird nicht geschrieben"
    );
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    let lv = loaded.lv();
    assert_eq!(
        lv.rows()[title].as_group().unwrap().cost_group(),
        Some("481")
    );
    assert_eq!(lv.positions().next().unwrap().cost_group(), Some("482"));

    let mut other = loaded.clone();
    other.lv_mut().set_cost_catalog(proava_domain::CostCatalog {
        id: "KG".into(),
        kind: "cost group DIN 276-06".into(),
        name: "DIN 276-06".into(),
    });
    let xml = proava_infrastructure::project_xml::write_project_xml(&other).unwrap();
    let again = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    assert_eq!(again.lv().cost_catalog().id, "KG");
    assert_eq!(again.lv().cost_catalog().kind, "cost group DIN 276-06");
}

#[test]
fn work_categories_and_catalog_survive() {
    let mut original = full_project();
    let lv = original.lv_mut();
    let index = lv.rows().iter().position(|row| row.is_position()).unwrap();
    if let Some(LvRow::Position(position)) = lv.row_mut(index) {
        position.set_work_category(Some("070".into()));
    }
    let title = lv.rows().iter().position(|row| row.is_group()).unwrap();
    if let Some(LvRow::Group(group)) = lv.row_mut(title) {
        group.set_work_category(Some("075".into()));
    }
    let xml = proava_infrastructure::project_xml::write_project_xml(&original).unwrap();
    assert!(xml.contains("work-category=\"070\""));
    assert!(xml.contains("work-category=\"075\""), "am Titel");
    assert!(
        !xml.contains("work-catalog-id"),
        "Standardkatalog wird nicht geschrieben"
    );
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    assert_eq!(
        loaded.lv().rows()[index]
            .as_position()
            .unwrap()
            .work_category(),
        Some("070")
    );
    assert_eq!(
        loaded.lv().rows()[title]
            .as_group()
            .unwrap()
            .work_category(),
        Some("075")
    );

    let mut other = loaded.clone();
    other.lv_mut().set_work_catalog(proava_domain::WorkCatalog {
        id: "LB".into(),
        kind: "work category".into(),
        name: "Gewerke".into(),
    });
    let xml = proava_infrastructure::project_xml::write_project_xml(&other).unwrap();
    let again = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    assert_eq!(again.lv().work_catalog().id, "LB");
    assert_eq!(again.lv().work_catalog().name, "Gewerke");
}

/// Strg+C in one proAVA, Strg+V in another: rows and whole LVs go through
/// the clipboard format completely (texts, images, kinds, references …).
#[test]
fn clipboard_format_carries_rows_and_lvs_completely() {
    use proava_application::clipboard::{decode, encode, Clip, ClipHeader, ClipKind};
    use proava_domain::transfer::{collect, images_of, insert, TransferOptions};

    let repository = XmlProjectFile::new();
    let source = full_project();
    let lv = source.lv();
    let roots: Vec<_> = lv
        .rows()
        .iter()
        .filter(|row| row.as_group().is_none_or(|group| group.level() == 0))
        .map(|row| row.id())
        .collect();
    let options = TransferOptions {
        quantities: true,
        prices: true,
    };
    let transfer = collect(&source, lv, &roots, options);
    assert!(!transfer.images.is_empty(), "das Testprojekt hat Bilder");
    let (header, without_images) =
        encode(&repository, &Clip::Rows(transfer.clone()), lv.oz_mask()).unwrap();
    assert!(!without_images);
    let html = format!("<table></table><!--{}-->", header.line());
    let found = ClipHeader::find(&html).unwrap();
    assert_eq!(found.kind, ClipKind::Rows);
    let Clip::Rows(back) = decode(&repository, &found).unwrap() else {
        panic!("Zeilen erwartet");
    };
    assert_eq!(back.rows, transfer.rows, "Zeilen vollständig");
    assert_eq!(back.images, transfer.images, "Bilder vollständig");

    // Pasted into another project: new ids, images along.
    let mut target = Project::new(
        ProjectName::parse("Ziel").unwrap(),
        "2026-11-17T22:00:00",
        Lv::new("Ziel", lv.oz_mask().clone()),
    );
    let range = insert(&mut target, None, &back).unwrap();
    assert_eq!(range.len(), transfer.rows.len());
    assert_eq!(target.images().len(), transfer.images.len());
    assert!(target.lv().duplicate_ordinals().is_empty());

    // A whole LV.
    let images = images_of(&source, lv.rows());
    let clip = Clip::Lv {
        lv: Box::new(lv.clone()),
        images: images.clone(),
    };
    let (header, _) = encode(&repository, &clip, lv.oz_mask()).unwrap();
    assert!(header.summary.starts_with("LV "), "{}", header.summary);
    let found = ClipHeader::find(&header.line()).unwrap();
    let Clip::Lv {
        lv: copy,
        images: copied,
    } = decode(&repository, &found).unwrap()
    else {
        panic!("LV erwartet");
    };
    assert_eq!(copy.rows(), lv.rows());
    assert_eq!(copy.oz_mask(), lv.oz_mask());
    assert_eq!(copy.vat_percent(), lv.vat_percent());
    assert_eq!(copied, images);
}

/// Auftrag, Nachträge and Abrechnung survive save and load in the second LV,
/// and the release computed afterwards is the same.
#[test]
fn contract_change_orders_and_billing_survive() {
    use proava_domain::{
        ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus, CheckBasis,
        Correction, Deduction, Invoice, InvoiceItem, InvoiceKind, ItemChange, Mode, Progress,
        TakeoffLine, VatRule,
    };

    let mut original = full_project();
    original.info_mut().terms.performance_security_percent = Some(dec("10"));
    original.info_mut().terms.deductions = vec![Deduction {
        label: TRICKY.into(),
        percent: dec("0.9"),
    }];
    let mut lv = Lv::new("Los 2", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Titel"));
    let a = lv.push_position(Position::new("01.0010", "Kabel", dec("100"), "m", None));
    let mut lump = Position::new("01.0020", "Pauschale", dec("1"), "psch", None);
    lump.set_lump_sum(true);
    let l = lv.push_position(lump);
    let n = lv.push_position(Position::new("01.0030", "Nachtrag", dec("1"), "St", None));
    let index = original.add_lv(lv);
    original.set_active_lv(index);
    let mut bid = Bid::new(Address::named("Firma & Söhne"), "f.x84");
    bid.set_price(
        a,
        BidPrice {
            unit_price: Some(dec("2.5")),
            ..BidPrice::default()
        },
    );
    bid.set_price(
        l,
        BidPrice {
            unit_price: Some(dec("250000")),
            ..BidPrice::default()
        },
    );
    bid.set_discount(Discount::Percent(dec("1.5")));
    let bid = original.add_bid(bid);
    original.award_bid(bid, "01.03.2026").unwrap();
    original.take_over_contract().unwrap();
    original.contract_mut().unwrap().number = "A-100".into();

    let mut order = ChangeOrder::new(ChangeOrderId(0), 0, "Mast <höher>");
    order.status = ChangeOrderStatus::Approved;
    order.basis = ChangeBasis::AdditionalPerformance;
    order.reason = TRICKY.into();
    order.offer_number = "N02".into();
    order.discount_applies = false;
    order.in_forecast = false;
    order.contractor = "Nachunternehmer & Co".into();
    order.announced_date = "01.02.2026".into();
    order.offer_due = "15.02.2026".into();
    order.decision_due = "01.03.2026".into();
    order.recommendation = TRICKY.into();
    order.group = Some(n);
    order.history = vec![
        proava_domain::StatusChange {
            status: ChangeOrderStatus::Offered,
            date: "10.02.2026".into(),
            note: TRICKY.into(),
        },
        proava_domain::StatusChange {
            status: ChangeOrderStatus::Approved,
            date: String::new(),
            note: String::new(),
        },
    ];
    order.items = vec![
        ChangeOrderItem {
            position: n,
            offered_quantity: dec("1"),
            offered_unit_price: Some(dec("1947")),
            checked_unit_price: Some(dec("1697.5")),
            note: "EK geprüft".into(),
            ..ChangeOrderItem::default()
        },
        ChangeOrderItem {
            position: a,
            change: ItemChange::Quantity,
            offered_quantity: dec("-10"),
            status: Some(ChangeOrderStatus::Rejected),
            ..ChangeOrderItem::default()
        },
    ];
    original.add_change_order(order);
    original.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "offen"));

    let billing = original.billing_mut();
    let mut first = Invoice::new(InvoiceKind::Partial, "R-1", "31.03.2026");
    first.quantities = Mode::Increment;
    first.vat = VatRule::ReverseCharge;
    first.claimed_net = Some(Money::from_cents(1_234_56));
    first.items.insert(
        a,
        InvoiceItem {
            claimed_quantity: Some(dec("40")),
            claimed_unit_price: Some(dec("2.6")),
            claimed_total: Some(dec("104")),
            checked_quantity: Some(dec("38.5")),
            basis: Some(CheckBasis::Quantity),
            note: TRICKY.into(),
            evidence: true,
            ..InvoiceItem::default()
        },
    );
    first.items.insert(
        l,
        InvoiceItem {
            progress: Some(Progress {
                factor: dec("0.0001"),
                note: "Absprache".into(),
            }),
            ..InvoiceItem::default()
        },
    );
    first.deductions = Some(Vec::new());
    first.correction = Some(Correction {
        amount: Money::from_cents(-1),
        text: "Rundung".into(),
    });
    let first = billing.add_invoice(first);
    let mut second = Invoice::new(InvoiceKind::Final, "S", "30.06.2026");
    second.vat = VatRule::Rate(dec("19"));
    second.security_percent = Some(dec("5"));
    second.credited = Some(Money::from_cents(100));
    second.claimed_lump = Some(Money::from_cents(50_00));
    second.checked_by = "O. H.".into();
    billing.add_invoice(second);
    billing.takeoff_mut(a).extend([
        TakeoffLine {
            label: "EG „Nord“".into(),
            formula: "2*12,5".into(),
            invoice: Some(first),
        },
        TakeoffLine {
            label: "später".into(),
            formula: "13,5".into(),
            invoice: None,
        },
    ]);

    let xml = write_project_xml(&original).unwrap();
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |file| {
        original.image(file).map(|image| image.bytes.clone())
    })
    .unwrap();
    assert_eq!(loaded.contract(), original.contract());
    assert_eq!(loaded.change_orders(), original.change_orders());
    assert_eq!(loaded.billing(), original.billing());
    assert_eq!(loaded, original);
    assert_eq!(write_project_xml(&loaded).unwrap(), xml);
    // The first LV has none of it.
    assert!(loaded.contract_of(0).is_none());
    assert!(loaded.change_orders_of(0).is_empty());
    // Same release after loading.
    let before: Vec<_> = original
        .billing()
        .check_all(&original.billing_context().unwrap())
        .into_iter()
        .map(|check| check.release)
        .collect();
    let after: Vec<_> = loaded
        .billing()
        .check_all(&loaded.billing_context().unwrap())
        .into_iter()
        .map(|check| check.release)
        .collect();
    assert_eq!(before, after);
    assert_eq!(after.len(), 2);
    // New ids continue after the stored ones.
    let mut again = loaded.clone();
    let next = again.billing_mut().add_invoice(Invoice::default());
    assert_eq!(next.0, 3);
    let order = again.add_change_order(ChangeOrder::new(ChangeOrderId(0), 0, "drei"));
    assert_eq!(
        again
            .change_orders()
            .iter()
            .find(|o| o.id == order)
            .unwrap()
            .number,
        3
    );
}

/// Files without contract, change orders and billing still read the same.
#[test]
fn files_without_billing_have_none() {
    let original = full_project();
    let xml = write_project_xml(&original).unwrap();
    assert!(
        !xml.contains("<contract") && !xml.contains("<billing") && !xml.contains("<change-orders")
    );
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    assert!(loaded.contract().is_none());
    assert!(loaded.billing().is_empty());
}

#[test]
fn takeoff_of_a_position_survives() {
    let mut original = full_project();
    let lv = original.lv_mut();
    let index = lv.rows().iter().position(|row| row.is_position()).unwrap();
    let lines = vec![
        proava_domain::TakeoffLine {
            label: "EG Strang 1".into(),
            formula: "2*12,5".into(),
            invoice: None,
        },
        proava_domain::TakeoffLine {
            label: "OG „Flur“ & Technik".into(),
            formula: "4,5+3".into(),
            invoice: None,
        },
    ];
    if let Some(LvRow::Position(position)) = lv.row_mut(index) {
        position.set_takeoff(lines.clone());
    }
    let xml = proava_infrastructure::project_xml::write_project_xml(&original).unwrap();
    assert!(
        xml.contains("<takeoff>"),
        "Mengenermittlung in der Position"
    );
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |_| None).unwrap();
    let position = loaded.lv().rows()[index].as_position().unwrap();
    assert_eq!(position.takeoff(), lines.as_slice());
    assert_eq!(position.takeoff_total().0, "32.5".parse().unwrap());
}

#[test]
fn library_entries_and_origins_round_trip() {
    use proava_domain::library::{EntryInfo, LibraryOrigin};

    let mut lv = Lv::new("Bibliothek", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Raumautomation"));
    let mut entry = Position::new(
        "01.0010",
        "Raumbediengerät",
        Decimal::ONE,
        "St",
        Some(dec("180")),
    );
    entry.set_library_info(EntryInfo {
        keywords: vec![
            "KNX".into(),
            "Fenster & Türen <außen> \"Typ A\" ä ß €".into(),
        ],
        as_of: "01.11.2026".into(),
        price_date: "10/2026".into(),
    });
    lv.push_position(entry);
    let mut taken = Position::new("01.0020", "Fühler", Decimal::ONE, "St", None);
    taken.set_origin(Some(LibraryOrigin {
        library: "Büro & Co".into(),
        entry: RowId(42),
        as_of: String::new(),
    }));
    lv.push_position(taken);
    lv.push_position(Position::new("01.0030", "ohne", Decimal::ONE, "St", None));
    let project = Project::new(ProjectName::parse("Bibliothek").unwrap(), "2026-11-17", lv);
    let back = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    let positions: Vec<&Position> = back.lv().positions().collect();
    assert_eq!(
        positions[0].library_info(),
        project.lv().positions().next().unwrap().library_info()
    );
    assert_eq!(positions[0].library_info().keywords.len(), 2);
    assert_eq!(positions[1].origin().unwrap().library, "Büro & Co");
    assert_eq!(positions[1].origin().unwrap().entry, RowId(42));
    assert!(positions[2].origin().is_none() && positions[2].library_info().is_empty());
}

#[test]
fn stlb_hint_texts_keep_complements_and_key() {
    use proava_domain::{CatalogKeys, StlbReference, TextAnchor};
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "", "GA"));
    let mut note = Note::rich(RichText::plain("Managementfunktionen,\nEinbau."));
    note.set_heading("Managementebene");
    note.set_complements(vec![TextComplement {
        marker: "11".into(),
        kind: ComplementKind::Owner,
        caption: "Anzahl Bedienplätze ".into(),
        body: "2".into(),
        tail: ", ".into(),
        anchor: Some(TextAnchor {
            block: 1,
            offset: 0,
            own_line: true,
        }),
    }]);
    note.set_catalog(CatalogKeys {
        stlb: Some(StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some("2026-04".into()),
            work_category: "070".into(),
            group: 9201,
            owner_complements: true,
            complement_links: vec![("11".into(), -92012)],
            complement_numbers: vec![("11".into(), proava_domain::ComplementNumber::Integer)],
            ..StlbReference::default()
        }),
        standard_number: None,
    });
    lv.push_note(note);
    lv.push_position(Position::new("", "Danach", dec("1"), "St", None));
    lv.renumber();
    let project = Project::new(ProjectName::parse("H").unwrap(), "2026-11-17T23:00:00", lv);
    let again = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(again, project);
}

/// Bibliothek und Textquelle nebeneinander: `<origin library=… entry=…>` und
/// `<text-origin source=…>`; die ältere Form `<origin source=…>` (erster
/// AUSSCHREIBEN.DE-Stand, ohne `entry`) wird als Textquelle gelesen und ist
/// keine Bibliotheksverknüpfung.
#[test]
fn library_origin_and_text_source_live_side_by_side() {
    use proava_domain::library::LibraryOrigin;

    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "Feldgeräte"));
    let mut both = Position::new("01.0010", "Drehantrieb", Decimal::ONE, "St", None);
    both.set_origin(Some(LibraryOrigin {
        library: "Büro".into(),
        entry: RowId(7),
        as_of: "01.11.2026".into(),
    }));
    both.set_text_origin(Some(proava_domain::TextOrigin {
        source: "AUSSCHREIBEN.DE".into(),
        supplier: "Muster Antriebe GmbH".into(),
        retrieved: "2026-11-17".into(),
        file: "Merkliste.X81".into(),
        reference: "MA-20-SR".into(),
        note: String::new(),
    }));
    lv.push_position(both);
    let project = Project::new(ProjectName::parse("Beides").unwrap(), "2026-11-18", lv);
    let bytes = project_to_bytes(&project).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("projekt.xml").unwrap(), &mut xml).unwrap();
    assert!(xml.contains("<text-origin "), "{xml}");
    assert!(xml.contains("<origin library="), "{xml}");
    let back = project_from_bytes(&bytes).unwrap();
    assert_eq!(back, project);

    // Älterer Stand: Textquelle als <origin source=…>, Bibliothek daneben.
    let old = xml.replace("<text-origin ", "<origin ");
    assert_eq!(old.matches("<origin ").count(), 2);
    let read = project_from_bytes(old.as_bytes()).unwrap();
    let position = read.lv().positions().next().unwrap();
    assert_eq!(position.origin().unwrap().entry, RowId(7));
    assert_eq!(position.text_origin().unwrap().reference, "MA-20-SR");
    // Nur die ältere Textquelle: keine Bibliotheksverknüpfung, kein Fehler.
    let only_text = old.replace(
        &old[old.find("<origin library=").unwrap()..][..old
            [old.find("<origin library=").unwrap()..]
            .find("/>")
            .unwrap()
            + 2],
        "",
    );
    let read = project_from_bytes(only_text.as_bytes()).unwrap();
    let position = read.lv().positions().next().unwrap();
    assert!(position.origin().is_none());
    assert_eq!(position.text_origin().unwrap().source, "AUSSCHREIBEN.DE");
}

/// PDF-Ausschnitt: PDF, Seite, Rechteck, Quelle und die Vorschau-Schalter
/// (eingefroren mit Bild, ausgeblendet) überstehen Speichern und Laden; ein
/// gewöhnliches Bild bleibt ein Bild.
#[test]
fn pdf_clip_round_trip() {
    use proava_domain::{ClipRect, PdfClip};
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_position(Position::new("", "Topologie", dec("1"), "psch", None));
    lv.renumber();
    let mut original = Project::new(ProjectName::parse("Ausschnitt").unwrap(), "2026-11-18", lv);
    let pdf = b"%PDF-1.7\n% synthetisch, wird nicht gerastert\n".to_vec();
    let mut clip = PdfClip::new(
        2,
        (3370.0, 2384.0),
        ClipRect::new(10.5, 20.25, 1600.0, 1900.75),
        "40849 Topologie.pdf, Seite 2",
    );
    clip.preview_hidden = true;
    let live = original.add_pdf_clip("topologie_S2.pdf", pdf.clone(), clip.clone());
    let frozen = original.add_pdf_clip("topologie_S2.pdf", pdf.clone(), clip);
    assert_eq!(frozen, "topologie_S2_2.pdf");
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    assert!(original.set_clip_preview(
        &frozen,
        true,
        false,
        Some(std::sync::Arc::new(png.clone()))
    ));
    assert!(
        !original.set_clip_preview(&frozen, true, false, None),
        "nichts geändert"
    );
    original.add_image("foto.png", vec![0x89, b'P', b'N', b'G', 9]);
    let mut text = RichText::plain("Siehe:");
    for file in [&live, &frozen, &"foto.png".to_string()] {
        text.push(TextBlock::Image {
            file: file.clone(),
            width_mm: Some(150.0),
        });
    }
    if let Some(LvRow::Position(position)) = original.lv_mut().row_mut(0) {
        position.set_long_text(text);
    }

    let bytes = project_to_bytes(&original).unwrap();
    let loaded = project_from_bytes(&bytes).unwrap();
    assert_eq!(loaded, original);
    let live_clip = loaded.image(&live).unwrap().clip.as_ref().unwrap();
    assert_eq!(live_clip.page, 2);
    assert_eq!(live_clip.rect, ClipRect::new(10.5, 20.25, 1600.0, 1900.75));
    assert_eq!(live_clip.source, "40849 Topologie.pdf, Seite 2");
    assert!(live_clip.preview_hidden && !live_clip.preview_frozen);
    let frozen_clip = loaded.image(&frozen).unwrap().clip.as_ref().unwrap();
    assert!(frozen_clip.preview_frozen && !frozen_clip.preview_hidden);
    assert_eq!(frozen_clip.frozen_png.as_deref(), Some(&png));
    assert!(loaded.image("foto.png").unwrap().clip.is_none());
    // The frozen preview is its own entry in the container.
    let archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let names: Vec<String> = archive
        .file_names()
        .filter_map(Result::ok)
        .map(|name| name.to_string())
        .collect();
    assert!(
        names
            .iter()
            .any(|name| name == "bilder/vorschau/topologie_S2_2.pdf.png"),
        "{names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|name| name.contains("vorschau/topologie_S2.pdf")),
        "{names:?}"
    );
    // Two parts of the same PDF: the PDF is stored once.
    assert!(
        names.iter().any(|name| name == "bilder/topologie_S2.pdf"),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "bilder/topologie_S2_2.pdf"),
        "{names:?}"
    );
    assert_eq!(loaded.image(&frozen).unwrap().bytes, pdf);
    // Unfrozen again: the picture goes.
    let mut again = loaded.clone();
    assert!(again.set_clip_preview(&frozen, false, false, None));
    let reloaded = project_from_bytes(&project_to_bytes(&again).unwrap()).unwrap();
    assert!(reloaded
        .image(&frozen)
        .unwrap()
        .clip
        .as_ref()
        .unwrap()
        .frozen_png
        .is_none());
}

/// Older programs read the clip as an image; files without `<pdf-clip>` stay
/// pictures, and a frozen clip whose picture is missing shows live again.
#[test]
fn pdf_clip_without_preview_entry_is_live() {
    use proava_domain::{ClipRect, PdfClip};
    let lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    let mut original = Project::new(ProjectName::parse("Ausschnitt").unwrap(), "", lv);
    let name = original.add_pdf_clip(
        "plan.pdf",
        b"%PDF-1.7".to_vec(),
        PdfClip::new(
            1,
            (595.0, 842.0),
            ClipRect::new(0.0, 0.0, 300.0, 300.0),
            "plan.pdf",
        ),
    );
    original.set_clip_preview(&name, true, false, Some(std::sync::Arc::new(vec![1, 2, 3])));
    let xml = write_project_xml(&original).unwrap();
    assert!(xml.contains("<pdf-clip"), "{xml}");
    let loaded = proava_infrastructure::project_xml::read_project_xml(&xml, |file| {
        (file == "plan.pdf").then(|| b"%PDF-1.7".to_vec())
    })
    .unwrap();
    let clip = loaded.image(&name).unwrap().clip.as_ref().unwrap();
    assert!(!clip.preview_frozen && clip.frozen_png.is_none());
}

/// Offener Punkt aus den Eigenschaftstests: Steuerzeichen aus eingefügten
/// Texten standen roh in projekt.xml – kein gültiges XML 1.0. Sie werden
/// jetzt als „Control Pictures“ geschrieben und so zurückgelesen; Dateien
/// mit rohen Steuerzeichen (ältere Fassung) bleiben lesbar.
#[test]
fn control_characters_keep_the_file_valid_xml() {
    let mut original = full_project();
    let lv = original.lv_mut();
    let index = lv.rows().iter().position(|row| row.is_position()).unwrap();
    if let Some(LvRow::Position(position)) = lv.row_mut(index) {
        position.set_short_text("Kurz\u{b}text\u{1}\u{1f}\tTab");
        position.set_long_text(RichText::plain("Lang\u{c}text\r\nzweite"));
    }
    original.info_mut().label = "Neubau\u{0}\u{8}".into();
    let xml = write_project_xml(&original).unwrap();
    let forbidden = xml
        .chars()
        .find(|character| matches!(*character as u32, 0x0..=0x8 | 0xB | 0xC | 0xE..=0x1F));
    assert_eq!(forbidden, None, "kein rohes Steuerzeichen");
    for reference in ["&#1;", "&#11;", "&#x1;", "&#xB;", "&#0;"] {
        assert!(!xml.contains(reference), "{reference}");
    }
    assert!(xml.contains("Kurz␋text␁␟"), "als Control Pictures");
    let loaded = project_from_bytes(&project_to_bytes(&original).unwrap()).unwrap();
    assert_eq!(loaded, original);

    // A file of the older version: raw control characters are read as they are.
    let old = xml.replace("Kurz␋text", "Kurz\u{b}text");
    let read = proava_infrastructure::project_xml::read_project_xml(&old, |_| None).unwrap();
    let position = read.lv().positions().next().unwrap();
    assert!(position.short_text().starts_with("Kurz\u{b}text"));
}

#[test]
fn cost_estimates_survive_save_and_load() {
    use proava_domain::{CostEstimate, CostSection, CostShare, CostStage, PointKind, PointRow};
    let mut original = full_project();
    assert!(
        !write_project_xml(&original)
            .unwrap()
            .contains("cost-estimates"),
        "ohne Kostenermittlung kein Element"
    );
    let mut estimate = CostEstimate::template(CostStage::Estimate, "09.11.2025", TRICKY);
    estimate.title = format!("Kostenschätzung {TRICKY}");
    estimate.remark = TRICKY.into();
    estimate.rate = dec("451.5");
    estimate.rate_low = Some(dec("400"));
    estimate.rate_high = Some(dec("500"));
    estimate.sections[0].name = TRICKY.into();
    estimate.sections[0].rows[0] = PointRow {
        location: "ASP01 · 2.UG".into(),
        trade: "RLT".into(),
        plant: "TKA-2001".into(),
        remark: TRICKY.into(),
        counts: vec![12, 30, 6, 6, 0, 0],
    };
    estimate.sections[0].rows.push(PointRow {
        counts: vec![0; 6],
        ..PointRow::default()
    });
    original.add_cost_estimate(estimate);

    let mut calculation = CostEstimate::template(CostStage::Calculation, "12.11.2026", "Prüfer");
    calculation.rate_per_kind = true;
    calculation.add_kind(PointKind::new("MV", "Mehrstufenwert", dec("212.25")));
    let mut neubau = CostSection::new("Neubau");
    neubau.shares = Some(vec![
        CostShare::new("481", dec("62.5")),
        CostShare::new("484", dec("37.5")),
    ]);
    let mut row = PointRow::new("HZG", "HK – Büros");
    row.set_count(6, 3);
    neubau.rows.push(row);
    calculation.sections.push(neubau);
    // An own split without lines stays an own split.
    let mut empty = CostSection::new("Therapiezentrum");
    empty.shares = Some(Vec::new());
    calculation.sections.push(empty);
    original.add_cost_estimate(calculation);

    let bytes = project_to_bytes(&original).unwrap();
    let loaded = project_from_bytes(&bytes).unwrap();
    assert_eq!(loaded.cost_estimates(), original.cost_estimates());
    assert_eq!(loaded, original);
    let xml = write_project_xml(&loaded).unwrap();
    assert!(
        xml.contains(r#"counts="12;30;6;6""#),
        "Nullen am Ende entfallen"
    );
    assert!(xml.contains(r#"stage="kostenschaetzung""#));
}

#[test]
fn spacers_survive_save_and_load_and_old_readers_skip_them() {
    let mut original = full_project();
    let lv = original.lv_mut();
    lv.insert_numbered(2, LvRow::Spacer(proava_domain::Spacer::new()))
        .unwrap();
    let last = lv.push_spacer();
    let ids: Vec<RowId> = lv
        .rows()
        .iter()
        .filter(|row| row.is_spacer())
        .map(LvRow::id)
        .collect();
    assert_eq!(ids.len(), 2);
    let xml = write_project_xml(&original).unwrap();
    assert!(
        xml.contains(&format!("<spacer id=\"{}\"/>", last.0)),
        "{xml}"
    );
    let loaded = project_from_bytes(&project_to_bytes(&original).unwrap()).unwrap();
    assert_eq!(loaded, original);
    let loaded_ids: Vec<RowId> = loaded
        .lv()
        .rows()
        .iter()
        .filter(|row| row.is_spacer())
        .map(LvRow::id)
        .collect();
    assert_eq!(loaded_ids, ids, "Stelle und Id bleiben");
    assert_eq!(loaded.lv().net_total(), original.lv().net_total());
}

/// Bieterangaben aus dem Langtext-Editor (Stelle im Satz, eigene Zeile,
/// unter dem Langtext) – an Positionen und Hinweisen – bleiben beim
/// Speichern und Laden, wie sie sind.
#[test]
fn bidder_fields_survive_save_and_load() {
    use proava_domain::complement_text::{insert, FieldPlace};
    let mut project = full_project();
    let field = |caption: &str, kind| {
        TextComplement {
            kind,
            caption: caption.into(),
            ..TextComplement::default()
        }
        .with_spaces()
    };
    let lv = project.lv_mut();
    let index = lv.rows().iter().position(LvRow::is_position).unwrap();
    let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
    let text = RichText::plain("Raumbediengerät mit Display\nMontage auf UP-Dose");
    let (text, list) = insert(
        &text,
        &[],
        field("Fabrikat:", ComplementKind::Bidder),
        &FieldPlace::After("Display".into()),
    )
    .unwrap();
    let (text, list) = insert(
        &text,
        &list,
        field("Farbe RAL", ComplementKind::Owner),
        &FieldPlace::AfterBlock(1),
    )
    .unwrap();
    let (text, list) = insert(
        &text,
        &list,
        field("Typ:", ComplementKind::Bidder),
        &FieldPlace::End,
    )
    .unwrap();
    position.set_long_text(text.clone());
    position.set_complements(list.clone());
    let id = position.id();
    let mut note = Note::new("Gilt für alle Raumbediengeräte.");
    let (note_text, note_list) = insert(
        note.text(),
        &[],
        field("Angebotenes Produkt:", ComplementKind::Bidder),
        &FieldPlace::End,
    )
    .unwrap();
    note.set_text(note_text);
    note.set_complements(note_list.clone());
    project.lv_mut().insert_row(0, LvRow::Note(note));

    let loaded = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    let position = loaded.lv().position_by_id(id).unwrap();
    assert_eq!(position.long_text(), &text);
    assert_eq!(position.complements(), list.as_slice());
    assert_eq!(position.complements()[0].caption, "Fabrikat: ");
    assert_eq!(position.complements()[2].anchor, None, "unter dem Langtext");
    let note = loaded.lv().rows().iter().find_map(|row| match row {
        LvRow::Note(note) if !note.complements().is_empty() => Some(note),
        _ => None,
    });
    assert_eq!(note.unwrap().complements(), note_list.as_slice());
}

/// Umwandeln der Zeilenart (18.11.): Hinweis und Ausführungsbeschreibung
/// tragen die Textquelle, die Ausführungsbeschreibung auch Textergänzungen –
/// alles übersteht Speichern und Laden.
#[test]
fn hints_and_performances_keep_text_source_and_complements() {
    let origin = proava_domain::TextOrigin {
        source: "STLB-Bau".into(),
        supplier: "Positionsliste".into(),
        retrieved: "2026-11-18".into(),
        file: "Positionsliste.X81".into(),
        reference: String::new(),
        note: String::new(),
    };
    let complement = TextComplement {
        marker: "1".into(),
        kind: ComplementKind::Bidder,
        caption: "Fabrikat ".into(),
        tail: "\nvom Bieter einzutragen".into(),
        ..TextComplement::default()
    };
    let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
    lv.push_group(LvGroup::new(0, "01", "GA"));
    let mut note = Note::new("Gefordert sind die Einrichtungen.");
    note.set_heading("Standardbeschreibung");
    note.set_text_origin(Some(origin.clone()));
    lv.push_note(note);
    let mut performance =
        PerformanceDescription::new("1", "Kabel", RichText::plain("Kabel verlegen."));
    performance.set_complements(vec![complement.clone()]);
    performance.set_text_origin(Some(origin.clone()));
    lv.push_row(LvRow::Performance(performance));
    let project = Project::new(ProjectName::parse("Arten").unwrap(), "2026-11-18", lv);
    let back = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(back, project);
    let rows = back.lv().rows();
    assert_eq!(rows[1].as_note().unwrap().text_origin(), Some(&origin));
    let performance = rows[2].as_performance().unwrap();
    assert_eq!(performance.complements(), &[complement]);
    assert_eq!(performance.text_origin(), Some(&origin));
}

/// A Kosten-LV (Kostenermittlung as Kurztext-LV) keeps its purpose and Stufe;
/// a tender LV writes nothing new, so older files (no attribute) read as
/// tender LVs and an unknown Stufe never turns a cost LV into a tender LV.
#[test]
fn purpose_of_an_lv_survives_and_old_files_are_tender_lvs() {
    use proava_domain::{CostStage, LvPurpose};
    let mut project = full_project();
    let tender_xml = write_project_xml(&project).unwrap();
    assert!(
        !tender_xml.contains("purpose="),
        "Ausschreibungs-LV ohne neues Attribut"
    );
    for stage in [CostStage::Estimate, CostStage::Calculation] {
        let mut cost = Lv::new(
            format!("{} Heizung", stage.label()),
            OzMask::parse("11.PPPP").unwrap(),
        );
        cost.set_purpose(LvPurpose::Cost(stage));
        cost.push_group(LvGroup::new(0, "01", "Wärmeverteilnetze"));
        cost.push_position(Position::new(
            "01.0010",
            "Rohrleitung DN 25",
            dec("120"),
            "m",
            Some(dec("50")),
        ));
        project.add_lv(cost);
    }
    let xml = write_project_xml(&project).unwrap();
    assert!(
        xml.contains(r#"purpose="cost" cost-stage="kostenschaetzung""#),
        "{xml}"
    );
    assert!(xml.contains(r#"purpose="cost" cost-stage="kostenberechnung""#));
    let back = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(back, project);
    let purposes: Vec<LvPurpose> = back.lvs().map(|lv| lv.purpose()).collect();
    assert_eq!(
        purposes,
        [
            LvPurpose::Tender,
            LvPurpose::Cost(CostStage::Estimate),
            LvPurpose::Cost(CostStage::Calculation)
        ]
    );
    assert_eq!(back.tender_lv_indices(), [0]);
    assert_eq!(back.cost_lv_indices(), [1, 2]);

    // Unknown Stufe of a newer proAVA: still a Kostenermittlung.
    let newer = write_project_xml(&project).unwrap().replace(
        r#"cost-stage="kostenschaetzung""#,
        r#"cost-stage="kostenrahmen""#,
    );
    let read = proava_infrastructure::project_xml::read_project_xml(&newer, |_| None).unwrap();
    assert_eq!(
        read.lv_at(1).unwrap().purpose(),
        LvPurpose::Cost(CostStage::Calculation)
    );
}

/// GU-Fall: a Kosten-LV with Kurztext positions and a Datenpunkt-Abschnitt
/// keeps the section (kinds, Bauteile, plant list, split) through save and
/// load; an older file (no `points`) reads with an ordinary title.
#[test]
fn point_sections_of_a_cost_lv_survive() {
    use proava_domain::{cost_lv_template, CostStage, LvRow, PointRow};
    let mut project = full_project();
    let mut lv = cost_lv_template(
        CostStage::Calculation,
        "Kostenberechnung KG 400",
        &["420", "480"],
        10,
    );
    let index = lv.insert_point_section().unwrap();
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        let points = group.points_mut().unwrap();
        points.sections[0].name = TRICKY.into();
        points.sections[0].rows[0] = PointRow {
            location: "ASP01 · UG".into(),
            trade: "RLT".into(),
            plant: "RLT-01".into(),
            remark: TRICKY.into(),
            counts: vec![12, 30, 6, 6, 18, 2],
        };
        points.shares[0].percent = dec("41.5");
        points.shares[3].percent = dec("28.5");
    }
    project.add_lv(lv);
    let xml = write_project_xml(&project).unwrap();
    assert!(xml.contains("<points "), "{xml}");
    let back = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
    assert_eq!(back, project);
    let lv = back.lv_at(1).unwrap();
    assert_eq!(lv.point_sections(), [index]);
    assert_eq!(lv.net_total(), project.lv_at(1).unwrap().net_total());
    assert!(lv.net_total() > Money::ZERO);

    // The same file without the element (an older proAVA): an ordinary title.
    let start = xml.find("<points ").unwrap();
    let end = xml[start..].find("</points>").unwrap() + start + "</points>".len();
    let older = format!("{}{}", &xml[..start], &xml[end..]);
    let read = proava_infrastructure::project_xml::read_project_xml(&older, |_| None).unwrap();
    assert!(read.lv_at(1).unwrap().point_sections().is_empty());
    assert_eq!(read.lv_at(1).unwrap().rows()[index].ordinal(), "480");
}
