use rust_decimal::Decimal;

use super::record::OzMask;
use super::*;
use crate::model::*;

/// Builds a DATAN file: every entry is the content of columns 1–74.
fn datan(records: &[String]) -> Vec<u8> {
    let text: String = records
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{r:<74}{:06}\r\n", i + 1))
        .collect();
    encode(&text, Encoding::Cp850)
}

/// One record: kind in columns 1–2, then texts starting at 1-based columns.
fn r(kind: &str, fields: &[(usize, &str)]) -> String {
    let mut line = vec![' '; 74];
    for (i, c) in kind.chars().enumerate() {
        line[i] = c;
    }
    for (col, text) in fields {
        for (i, c) in text.chars().enumerate() {
            line[col - 1 + i] = c;
        }
    }
    line.into_iter().collect::<String>().trim_end().to_string()
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn d84() -> Vec<u8> {
    datan(&[
        r("00", &[(11, "84"), (29, "Demo-01"), (63, "1122PPPPI90")]),
        r(
            "01",
            &[(3, "Schulgebäude Neubau"), (43, "08.05.25"), (72, "X")],
        ),
        r("02", &[(3, "Neubau Grundschule Süd")]),
        r("03", &[(3, "Stadt Musterstadt")]),
        r("04", &[(3, "Muster GmbH & Co. KG")]),
        r("06", &[(3, "2Stunden"), (18, "Material"), (32, "Lohn")]),
        r("08", &[(3, "EUR"), (9, "Euro")]),
        r(
            "23",
            &[
                (3, "01010010"),
                (13, "00018888840"),
                (25, "000001888884"),
                (49, "0990000"),
            ],
        ),
        r("26", &[(3, "61B'Hersteller XY")]),
        r("26", &[(3, "61BTyp 4711'")]),
        r(
            "23",
            &[(3, "01010020"), (13, "0000001250"), (25, "000000012500")],
        ),
        r(
            "30",
            &[(3, "0000015000"), (13, "0000001000"), (23, "0000000250")],
        ),
        r(
            "23",
            &[(3, "01010030"), (57, "01000"), (63, "000000013750")],
        ),
        r("32", &[(3, "0101"), (13, "000001901384"), (25, "1000000")]),
        r("32", &[(3, "01"), (13, "000001901384"), (25, "1000000")]),
        r(
            "23",
            &[(3, "02  0010"), (12, "-0000000500"), (24, "-000000005000")],
        ),
        r("32", &[(3, "02"), (12, "-000000005000"), (25, "1000000")]),
        r(
            "99",
            &[
                (3, "000001896384"),
                (15, "0990000"),
                (22, "1900"),
                (26, "000002234130"),
                (70, "00090"),
            ],
        ),
    ])
}

/// Gefunden mit den Eigenschaftstests: eine eigene D83 mit der Einheit „m³“
/// (CP850 0xFC) und sonst ohne Umlaute kam als „mü“ zurück – 0xFC ist in
/// Windows-1252 „ü“.
#[test]
fn units_with_exponents_do_not_flip_the_detection() {
    let cp850 = encode("Kies 2 m³, Fläche m²", Encoding::Cp850);
    assert_eq!(detect(&encode("1 m³", Encoding::Cp850)), Encoding::Cp850);
    assert_eq!(
        detect(&encode("5 m² und 2 cm³", Encoding::Cp850)),
        Encoding::Cp850
    );
    assert_eq!(decode(&cp850, detect(&cp850)), "Kies 2 m³, Fläche m²");
    // Windows-1252 stays recognisable, with exponents and with umlauts.
    assert_eq!(detect(&[b'1', b' ', b'm', 0xb3]), Encoding::Windows1252);
    assert_eq!(detect(&[b'T', 0xfc, b'r']), Encoding::Windows1252);
    assert_eq!(
        detect(&[b'M', 0xfc, b'l', b'l', b' ', b'm', 0xb2]),
        Encoding::Windows1252
    );
}

/// Gefunden mit den Eigenschaftstests: der CP850-Text „±äÜä“ ist zufällig
/// gültiges UTF-8 (F1 84 9A 84) und kam als „?000“ zurück.
#[test]
fn valid_utf8_must_also_read_plausibly() {
    let bytes = encode("±äÜä", Encoding::Cp850);
    assert!(std::str::from_utf8(&bytes).is_ok());
    assert_eq!(detect(&bytes), Encoding::Cp850);
    // Real UTF-8 stays UTF-8, also with only symbols or emoji.
    assert_eq!(detect("Größe 5 m² – Tür".as_bytes()), Encoding::Utf8);
    assert_eq!(detect("Preis 5 €".as_bytes()), Encoding::Utf8);
    assert_eq!(detect("Hinweis 😀".as_bytes()), Encoding::Utf8);
    assert_eq!(detect("\u{feff}x".as_bytes()), Encoding::Utf8);
}

#[test]
fn cp850_umlauts_and_detection() {
    let bytes = [0x81, 0x84, 0x94, 0xe1, 0x8e, 0x99, 0x9a, 0xfd, 0xf8];
    assert_eq!(decode(&bytes, Encoding::Cp850), "üäößÄÖÜ²°");
    assert_eq!(detect(&bytes), Encoding::Cp850);
    assert_eq!(detect("Straße".as_bytes()), Encoding::Utf8);
    assert_eq!(
        detect(&[b'S', b't', b'r', b'a', 0xdf, b'e', 0xe4]),
        Encoding::Windows1252
    );
    assert_eq!(decode(b"Stra~e {u}", Encoding::Din66003), "Straße äuü");
    assert_eq!(
        encode("Größe €", Encoding::Cp850),
        vec![b'G', b'r', 0x94, 0xe1, b'e', b' ', b'?']
    );
    for b in 0x80..=0xffu8 {
        let c = decode(&[b], Encoding::Cp850);
        assert_eq!(encode(&c, Encoding::Cp850), vec![b]);
    }
}

#[test]
fn oz_mask() {
    let mask = OzMask::parse("1122PPPPI").unwrap();
    assert_eq!(mask.to_mask_string(), "1122PPPPI");
    let split = mask.split("01020030a");
    assert_eq!(split.levels, vec!["01", "02"]);
    assert_eq!((split.item.as_str(), split.index.as_str()), ("0030", "A"));
    let split = mask.split(" 1 2  30 ");
    assert_eq!(split.dotted(), "01.02.0030");
    assert_eq!(
        mask.join(&["1".into(), "2".into()], Some("30"), ""),
        "01020030 "
    );
    assert_eq!(mask.join(&["1".into()], None, ""), "01       ");
    let split = mask.split("04  0010 ");
    assert_eq!(split.path(), vec!["04"]);
    assert_eq!(OzMask::parse("12PPP0000").unwrap().level_count(), 2);
    assert!(OzMask::parse("11X2PPPPI").is_err());
    assert!(OzMask::parse("21PPPP000").is_err());
    assert!(OzMask::parse("112200000").is_err());
}

#[test]
fn reads_bid_short_form() {
    let report = read(&d84()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let doc = &report.document;
    assert_eq!(doc.phase, Phase::Dp84);
    assert_eq!(doc.date.as_deref(), Some("2025-05-08"));
    assert_eq!(doc.project.label, "Neubau Grundschule Süd");
    assert_eq!(doc.project.name, "Demo-01");
    assert_eq!(doc.project.currency, "EUR");
    assert_eq!(
        doc.award.bidder.as_ref().unwrap().name1,
        "Muster GmbH & Co. KG"
    );
    assert_eq!(doc.award.owner.as_ref().unwrap().name1, "Stadt Musterstadt");
    let boq = &doc.award.boq;
    assert_eq!(boq.label, "Schulgebäude Neubau");
    assert_eq!(boq.up_component_labels, vec!["Material", "Lohn"]);
    assert_eq!(boq.time_label, "Stunden");

    let items = boq.items();
    assert_eq!(items.len(), 4);
    let (oz, first) = &items[0];
    assert_eq!(oz, &vec!["01", "01", "0010"]);
    assert_eq!(first.unit_price, Some(dec("18888.840")));
    assert_eq!(first.total, Some(dec("18888.84")));
    assert_eq!(first.discount_percent, Some(dec("1")));
    assert_eq!(first.complements.len(), 1);
    let complement = &first.complements[0];
    assert_eq!(complement.marker, "61");
    assert_eq!(complement.kind, "Bidder");
    // Two records are one text (fixed record length, Anwender 18.11.).
    assert_eq!(complement.body, "Hersteller XY Typ 4711");
    assert!(first.long_text.is_empty());

    let second = items[1].1;
    assert_eq!(second.unit_price, Some(dec("12.50")));
    assert_eq!(second.time_quantity, Some(dec("1.5")));
    assert_eq!(
        second.up_components,
        vec![Some(dec("10.00")), Some(dec("2.50"))]
    );

    let markup = items[2].1;
    assert!(markup.kind.markup);
    assert_eq!(markup.markup_percent, Some(dec("10.00")));
    assert_eq!(markup.markup_base, Some(dec("137.50")));
    assert_eq!(markup.total, None);

    let (oz, credit) = &items[3];
    assert_eq!(oz, &vec!["02", "0010"]);
    assert_eq!(credit.unit_price, Some(dec("-5.00")));
    assert_eq!(credit.total, Some(dec("-50.00")));

    let BoqNode::Category(title) = &boq.body[0] else {
        panic!("category expected");
    };
    let BoqNode::Category(sub) = &title.body[0] else {
        panic!("sub category expected");
    };
    assert_eq!(sub.totals.as_ref().unwrap().total, Some(dec("19013.84")));
    assert_eq!(sub.totals.as_ref().unwrap().discount_percent, None);

    let totals = boq.totals.as_ref().unwrap();
    assert_eq!(totals.total, Some(dec("18963.84")));
    assert_eq!(totals.discount_percent, Some(dec("1")));
    assert_eq!(totals.total_after_discount, Some(dec("18774.20")));
    assert_eq!(totals.vat_percent, Some(dec("19")));
    assert_eq!(totals.total_gross, Some(dec("22341.30")));
    assert_eq!(report.declared_item_count, Some(90));
}

fn d83() -> Vec<u8> {
    datan(&[
        r("T0", &[]),
        r("T1", &[(3, "Es gelten die Vertragsbedingungen.")]),
        r("T9", &[]),
        r(
            "00",
            &[(11, "83L"), (14, "VG-2025-17"), (63, "1122PPPPI90")],
        ),
        r(
            "01",
            &[
                (3, "Gebäudeautomation"),
                (43, "01.12.25"),
                (51, "15.01.26"),
                (59, "10:30"),
                (64, "28.02.26"),
            ],
        ),
        r("02", &[(3, "Neubau Grundschule Süd")]),
        r("03", &[(3, "Stadt Musterstadt")]),
        r("08", &[(3, "EUR"), (9, "Euro")]),
        r("20", &[]),
        r("26", &[(6, "Vorbemerkung zum LV.")]),
        r("11", &[(3, "01"), (12, "N"), (17, "Titel")]),
        r("12", &[(3, "Automationseinrichtungen")]),
        r("11", &[(3, "0101"), (12, "N"), (17, "Untertitel")]),
        r("12", &[(3, "Hardware")]),
        r("12", &[(3, "zweite Zeile")]),
        r("41", &[(6, "1Ausführungsbeschreibung ASP01")]),
        r("42", &[]),
        r("25", &[(3, "Ausführungsbeschreibung ASP01")]),
        r("26", &[(6, "IO-Konfiguration nach GA-Funktionsliste.")]),
        r(
            "21",
            &[
                (3, "01010010"),
                (12, "NNN"),
                (21, "A"),
                (24, "00000001000St"),
                (43, "X"),
                (44, "1"),
                (45, "12345678901234567"),
            ],
        ),
        r(
            "28",
            &[(3, "AS ASP01 gem. Ausführungsbeschreibung"), (70, "1")],
        ),
        r("25", &[(3, "Automationsstation ASP01")]),
        r("26", &[(6, "Automationsstation liefern und montieren.")]),
        r("26", &[]),
        r("26", &[(6, "  eingerückte Zeile")]),
        r("26", &[(3, "61BFabrikat: '....................'")]),
        r(
            "21",
            &[
                (3, "01010020"),
                (12, "GNN"),
                (15, "001"),
                (24, "00000002500m"),
                (44, "2"),
                (45, "0710141211"),
            ],
        ),
        r("25", &[(3, "Kabel Grundposition")]),
        r(
            "21",
            &[
                (3, "01010030"),
                (12, "AEN"),
                (15, "0011"),
                (24, "00000002500m"),
            ],
        ),
        r("25", &[(3, "Kabel Wahlposition")]),
        r(
            "21",
            &[
                (3, "01010040b"),
                (12, "NNN"),
                (22, "X"),
                (24, "00000000000psch"),
            ],
        ),
        r("25", &[(3, "Pauschale mit Index")]),
        r(
            "21",
            &[
                (3, "01010050"),
                (12, "NNN"),
                (21, "W"),
                (24, "00000001000St"),
            ],
        ),
        r("28", &[(3, "Wie 01.01.0010"), (58, "01010010")]),
        r("25", &[(3, "Wiederholung")]),
        r("31", &[(3, "0101")]),
        r("31", &[(3, "01")]),
        r("99", &[(70, "00005")]),
    ])
}

#[test]
fn reads_request_long_form() {
    let report = read(&d83()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let doc = &report.document;
    assert_eq!(doc.phase, Phase::Dp83);
    assert_eq!(doc.award.award_no, "VG-2025-17");
    assert_eq!(
        doc.award.contract_text,
        vec!["Es gelten die Vertragsbedingungen."]
    );
    let sub = &doc.award.submission;
    assert_eq!(sub.open_date.as_deref(), Some("2026-01-15"));
    assert_eq!(sub.open_time.as_deref(), Some("10:30"));
    assert_eq!(sub.award_deadline.as_deref(), Some("2026-02-28"));
    let boq = &doc.award.boq;
    assert_eq!(boq.breakdown[0].label, "Titel");
    assert_eq!(boq.breakdown[1].label, "Untertitel");
    assert_eq!(boq.breakdown[3].kind, BreakdownKind::Index);

    let BoqNode::Remark(remark) = &boq.body[0] else {
        panic!("remark expected, got {:?}", boq.body[0]);
    };
    assert_eq!(remark.text.to_plain(), "Vorbemerkung zum LV.");
    let BoqNode::Category(title) = &boq.body[1] else {
        panic!("category expected");
    };
    assert_eq!(title.label, "Automationseinrichtungen");
    let BoqNode::Category(hardware) = &title.body[0] else {
        panic!("sub category expected");
    };
    assert_eq!(hardware.label, "Hardware\nzweite Zeile");
    let BoqNode::PerfDescr(perf) = &hardware.body[0] else {
        panic!("perf descr expected");
    };
    assert_eq!(perf.number, "1");
    assert_eq!(perf.short_text, "Ausführungsbeschreibung ASP01");

    let items = boq.items();
    assert_eq!(items.len(), 5);
    let station = items[0].1;
    assert_eq!(station.qty, Some(dec("1.000")));
    assert_eq!(station.unit, "St");
    assert_eq!(station.short_text, "Automationsstation ASP01");
    assert_eq!(
        station.long_text.to_plain(),
        "Automationsstation liefern und montieren.\n\n  eingerückte Zeile"
    );
    assert_eq!(station.kind.reference_description.as_deref(), Some("1"));
    assert_eq!(station.complements.len(), 1);
    assert_eq!(station.complements[0].caption, "Fabrikat:");
    assert_eq!(station.complements[0].body, "");

    let base = items[1].1;
    assert_eq!(
        base.kind.alternative,
        Some(Alternative {
            group: "001".into(),
            serial: "0".into()
        })
    );
    let alternative = items[2].1;
    assert_eq!(alternative.kind.alternative.as_ref().unwrap().serial, "1");
    assert_eq!(
        alternative.kind.provisional,
        Some(Provisional::WithoutTotal)
    );

    let (oz, lump) = &items[3];
    assert_eq!(oz, &vec!["01", "01", "0040", "B"]);
    assert!(lump.kind.lump_sum);
    assert!(lump.qty_to_be_determined);
    assert_eq!(lump.qty, None);

    let repeat = items[4].1;
    assert_eq!(repeat.kind.repetition_of.as_deref(), Some("01.01.0010"));
}

#[test]
fn rejects_what_is_not_datan() {
    assert!(matches!(read(b""), Err(DatanError::NotDatan)));
    assert!(matches!(
        read(b"<?xml version=\"1.0\"?><GAEB/>"),
        Err(DatanError::NotDatan)
    ));
    let bad_phase = datan(&[r("00", &[(11, "89"), (63, "1122PPPPI90")])]);
    assert!(matches!(read(&bad_phase), Err(DatanError::UnknownPhase(p)) if p == "89"));
    let bad_mask = datan(&[r("00", &[(11, "84"), (63, "XXXXXXXXX90")])]);
    let err = read(&bad_mask).unwrap_err();
    assert!(matches!(err, DatanError::InvalidMask(_)));
    assert!(err.to_string().contains("OZ-Maske"));
    // No mask at all (seen in a bidder's D84): say so instead of naming an empty mask.
    let no_mask = datan(&[r("00", &[(11, "84")]), r("99", &[])]);
    let err = read(&no_mask).unwrap_err().to_string();
    assert!(
        err.contains("keine OZ-Maske (Satz 00, Stellen 63–71)"),
        "{err}"
    );
    assert!(!err.contains("„"), "{err}");
}

#[test]
fn tolerates_odd_records() {
    let header = r("00", &[(11, "84"), (63, "1122PPPPI90")]);
    let text = header
        + "\r\n\
                77unbekannt\r\n\
                2301010010  00000X1000  000000001000\r\n\
                23\r\n\
                26   Langtext ohne Position\n\
                32\r\n\
                99000000001000\r\n";
    let report = read(text.as_bytes()).unwrap();
    let items = report.document.award.boq.items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].1.unit_price, None);
    assert_eq!(items[0].1.total, Some(dec("10.00")));
    let messages: Vec<String> = report.warnings.iter().map(|w| w.to_string()).collect();
    assert!(
        messages.iter().any(|m| m.contains("Satzart „77“")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("keine Zahl")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("ohne Positionsnummer")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("ohne Ordnungszahl")),
        "{messages:?}"
    );
}

#[test]
fn never_panics_on_garbage() {
    let mut seed: u32 = 0x2545_f491;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let kinds = [
        "00", "01", "02", "06", "10", "11", "12", "20", "21", "22", "23", "24", "25", "26", "28",
        "30", "31", "32", "33", "41", "99", "T1",
    ];
    for _ in 0..300 {
        let mut text = r("00", &[(11, "82"), (63, "1122PPPPI90")]) + "\r\n";
        for _ in 0..40 {
            let kind = kinds[next() as usize % kinds.len()];
            let len = next() as usize % 90;
            let body: String = (0..len)
                .map(|_| match next() % 6 {
                    0 => ' ',
                    1 => '-',
                    2 => '\'',
                    3 => char::from(b'A' + (next() % 26) as u8),
                    4 => 'ü',
                    _ => char::from(b'0' + (next() % 10) as u8),
                })
                .collect();
            text.push_str(&format!("{kind}{body}\r\n"));
        }
        let _ = read(text.as_bytes());
        let _ = read(&encode(&text, Encoding::Cp850));
    }
}

#[test]
fn lots() {
    let bytes = datan(&[
        r("00", &[(11, "81L"), (63, "1PPP0000090X")]),
        r("01", &[(3, "Lose")]),
        r("02", &[(3, "Projekt")]),
        r("08", &[(3, "EUR"), (9, "Euro")]),
        r("10", &[(3, " 1Los Rohbau")]),
        r("11", &[(3, "1"), (12, "N")]),
        r("12", &[(3, "Erdarbeiten")]),
        r("21", &[(3, "1010"), (12, "NNN"), (24, "00000010000m3")]),
        r("25", &[(3, "Aushub")]),
        r("31", &[(3, "1")]),
        r("33", &[(3, " 1")]),
        r("10", &[(3, " 2Los Ausbau")]),
        r("11", &[(3, "1"), (12, "N")]),
        r("12", &[(3, "Putz")]),
        r("21", &[(3, "1010"), (12, "NNN"), (24, "00000020000m2")]),
        r("25", &[(3, "Putz")]),
        r("31", &[(3, "1")]),
        r("33", &[(3, " 2")]),
        r("99", &[(70, "00002")]),
    ]);
    let report = read(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let boq = &report.document.award.boq;
    assert_eq!(boq.breakdown[0].kind, BreakdownKind::Lot);
    let items = boq.items();
    assert_eq!(items[0].0, vec!["01", "1", "010"]);
    assert_eq!(items[1].0, vec!["02", "1", "010"]);
    let BoqNode::Category(lot) = &boq.body[1] else {
        panic!("lot expected");
    };
    assert_eq!(lot.label, "Los Ausbau");

    let written = write(&report.document).unwrap();
    let again = read(&written).unwrap();
    assert_eq!(again.document, report.document);
}

fn assert_valid_lines(bytes: &[u8]) {
    let text = decode(bytes, Encoding::Cp850);
    assert!(text.ends_with("\r\n"));
    for (i, line) in text.trim_end_matches("\r\n").split("\r\n").enumerate() {
        assert_eq!(line.chars().count(), 80, "line {}: {line:?}", i + 1);
        assert_eq!(&line[line.len() - 6..], format!("{:06}", i + 1));
    }
}

#[test]
fn request_roundtrip() {
    let first = read(&d83()).unwrap().document;
    let bytes = write(&first).unwrap();
    assert_valid_lines(&bytes);
    let report = read(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.declared_item_count, Some(5));
    assert_eq!(report.document, first);
}

#[test]
fn standard_numbers_in_record_21() {
    let doc = read(&d83()).unwrap().document;
    let items = doc.award.boq.items();
    let number = |index: usize| items[index].1.standard_number.clone();
    assert_eq!(
        number(0),
        Some(StandardNumber {
            number: "12345678901234567".into(),
            catalog: "StLB".into(),
        })
    );
    assert_eq!(
        number(1),
        Some(StandardNumber {
            number: "0710141211".into(),
            catalog: "StLK".into(),
        })
    );
    assert_eq!(number(2), None);
    let text = decode(&write(&doc).unwrap(), Encoding::Cp850);
    let line = text
        .lines()
        .find(|line| line.starts_with("2101010010"))
        .unwrap();
    assert_eq!(&line[43..61], "112345678901234567");
    // STLB-Bau keys have no place in GAEB 90 and are left out.
    fn add_stlb(nodes: &mut [BoqNode]) -> usize {
        let mut count = 0;
        for node in nodes {
            match node {
                BoqNode::Category(category) => count += add_stlb(&mut category.body),
                BoqNode::Item(item) => {
                    item.stlb = Some(StlbReference {
                        catalog: "STLB-Bau".into(),
                        group: 1,
                        ..StlbReference::default()
                    });
                    count += 1;
                }
                _ => {}
            }
        }
        count
    }
    let mut with_stlb = doc.clone();
    assert_eq!(add_stlb(&mut with_stlb.award.boq.body), 5);
    assert_eq!(write(&with_stlb).unwrap(), write(&doc).unwrap());
}

#[test]
fn bid_roundtrip() {
    let first = read(&d84()).unwrap().document;
    let bytes = write(&first).unwrap();
    assert_valid_lines(&bytes);
    let text = decode(&bytes, Encoding::Cp850);
    assert!(text.contains("2301010010  0001888884  000001888884            0990000"));
    assert!(text.contains("2661B'Hersteller XY"));
    let report = read(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.document, first);
}

#[test]
fn writes_bid_from_model() {
    let item = |rno: &str, up: &str, it: &str| {
        BoqNode::Item(Item {
            rno_part: rno.into(),
            qty: Some(dec("2")),
            unit: "St".into(),
            unit_price: Some(dec(up)),
            total: Some(dec(it)),
            short_text: "Text".into(),
            long_text: RichText::plain("Langtext, der in D84 nicht vorkommt"),
            ..Item::default()
        })
    };
    let doc = GaebDocument {
        phase: Phase::Dp84,
        program: "proAVA".into(),
        date: Some("2026-11-17".into()),
        project: ProjectInfo {
            name: "P1".into(),
            label: "Projekt".into(),
            currency: "EUR".into(),
            ..ProjectInfo::default()
        },
        award: Award {
            bidder: Some(Address {
                name1: "Bieter AG".into(),
                ..Address::default()
            }),
            boq: Boq {
                breakdown: OzMask::parse("1PPPP0000").unwrap().breakdown(),
                body: vec![BoqNode::Category(Category {
                    rno_part: "1".into(),
                    label: "Titel".into(),
                    body: vec![
                        item("10", "1.5", "3"),
                        item("20", "1234567.891", "2469135.78"),
                    ],
                    totals: None,
                    alternative: None,
                    catalog_assigns: Vec::new(),
                })],
                ..Boq::default()
            },
            ..Award::default()
        },
    };
    let bytes = write(&doc).unwrap();
    assert_valid_lines(&bytes);
    let text = decode(&bytes, Encoding::Cp850);
    assert!(!text.contains("Langtext"));
    assert!(text.contains("04Bieter AG"));
    assert!(text.contains("2310020     01234567891 000246913578"));
    assert!(text.contains("321         000246913878"), "{text}");
    let back = read(&bytes).unwrap().document;
    let items = back.award.boq.items();
    assert_eq!(items[1].1.unit_price, Some(dec("1234567.891")));
    assert_eq!(
        back.award.boq.totals.unwrap().total,
        Some(dec("2469138.78"))
    );
}

#[test]
fn writer_rejects_what_does_not_fit() {
    let mut doc = read(&d84()).unwrap().document;
    doc.award.boq.breakdown = vec![
        BreakdownLevel {
            kind: BreakdownKind::Level,
            label: String::new(),
            length: 4,
            numeric: true,
        },
        BreakdownLevel {
            kind: BreakdownKind::Item,
            label: String::new(),
            length: 6,
            numeric: true,
        },
    ];
    assert!(matches!(write(&doc), Err(DatanError::InvalidMask(_))));

    let mut doc = read(&d84()).unwrap().document;
    if let Some(totals) = doc.award.boq.totals.as_mut() {
        totals.total = Some(dec("99999999999.99"));
    }
    let err = write(&doc).unwrap_err();
    assert!(err.to_string().contains("passt nicht"), "{err}");
}

fn plain_blocks(blocks: &[TextBlock]) -> Vec<String> {
    blocks
        .iter()
        .map(|block| match block {
            TextBlock::Paragraph { runs, .. } => runs.iter().map(|r| r.text.as_str()).collect(),
            TextBlock::ListItem { ordered, runs, .. } => format!(
                "{}{}",
                if *ordered { "# " } else { "• " },
                runs.iter().map(|r| r.text.as_str()).collect::<String>()
            ),
            _ => "?".into(),
        })
        .collect()
}

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|l| l.to_string()).collect()
}

#[test]
fn reflow_joins_wrapped_lines() {
    let blocks = crate::text_reflow::reflow(
        &lines(&[
            "Automationseinrichtung, gemäß GA-Funktionsliste,",
            "Beiblatt 070-5, kondensierend), für Einbau in Peer-",
            "to-Peer Kommunikation, Gebäude- und Raumautomation,",
            "die Steuerung der Gebäudeautomations-",
            "anlage mit DIN EN 61000-",
            "6-3 und Kommunikations-",
            "Teilnehmern, Ein-",
            "/Ausgabefunktionen.",
            "",
            "Ausstattungsmerkmale:",
            "l",
            "Temperaturfühler mit einer sehr langen Beschreibung,",
            "die weitergeht",
            "l",
            "kein Display",
            "Anzahl   Ein- / Ausgangsart",
            "4   Digitale Ausgänge",
            "Hinweis Automationseinrichtungen",
            "Sämtliche Bestandteile sind aufzuführen.",
            "- Strich-Punkt",
            "1. erster Schritt",
            "2. zweiter Schritt",
            "    eingerückt",
        ]),
        55,
    );
    assert_eq!(
        plain_blocks(&blocks),
        vec![
            "Automationseinrichtung, gemäß GA-Funktionsliste, Beiblatt 070-5, kondensierend), \
             für Einbau in Peer-to-Peer Kommunikation, Gebäude- und Raumautomation, die \
             Steuerung der Gebäudeautomationsanlage mit DIN EN 61000-6-3 und \
             Kommunikations-Teilnehmern, Ein-/Ausgabefunktionen.",
            "",
            "Ausstattungsmerkmale:",
            "• Temperaturfühler mit einer sehr langen Beschreibung, die weitergeht",
            "• kein Display",
            "Anzahl   Ein- / Ausgangsart",
            "4   Digitale Ausgänge",
            "Hinweis Automationseinrichtungen",
            "Sämtliche Bestandteile sind aufzuführen.",
            "• Strich-Punkt",
            "# erster Schritt",
            "# zweiter Schritt",
            "    eingerückt",
        ]
    );
}

#[test]
fn reflow_removes_rtf_remnants() {
    let blocks = crate::text_reflow::reflow(
        &lines(&[
            r"\par",
            r"Angebotshinweis:\par",
            r"Bodenkanalheizungen Katherm HK 290,  Höhe 160 mm, 4-Leiter",
            r"System, mit\par",
            r"Roll-Rost \b Aluminium\b0  natur\tab eloxiert\par Ende",
            r"C:\Temp\datei",
        ]),
        60,
    );
    assert_eq!(
        plain_blocks(&blocks),
        vec![
            "",
            "Angebotshinweis:",
            "Bodenkanalheizungen Katherm HK 290,  Höhe 160 mm, 4-Leiter System, mit",
            "Roll-Rost Aluminium natur eloxiert",
            "Ende",
            r"C:\Temp\datei",
        ]
    );
}

#[test]
fn long_text_roundtrip_rewraps() {
    let mut doc = read(&d83()).unwrap().document;
    let long = "Einpflegen des AKS / BAS, je BACnet-Objekt, in die BACnet MS/TP Schnittstelle \
                der Raumbediengeräte. Wenn die Schnittstelle das nicht zulässt, gilt: die \
                Gebäude- und Raumautomation übernimmt; Peer-to-Peer-Kommunikation bleibt.";
    let text = RichText {
        blocks: vec![
            TextBlock::paragraph(vec![Run {
                text: long.into(),
                ..Run::default()
            }]),
            TextBlock::paragraph(vec![Run::default()]),
            TextBlock::paragraph(vec![Run {
                text: "Folgende Leistungen:".into(),
                ..Run::default()
            }]),
            TextBlock::ListItem {
                ordered: false,
                level: 0,
                runs: vec![Run {
                    text: "erster Punkt mit so viel Text, dass er auf jeden Fall umbrochen \
                           werden muss"
                        .into(),
                    ..Run::default()
                }],
                align: Default::default(),
            },
            TextBlock::ListItem {
                ordered: true,
                level: 0,
                runs: vec![Run {
                    text: "nummeriert".into(),
                    ..Run::default()
                }],
                align: Default::default(),
            },
            TextBlock::ListItem {
                ordered: true,
                level: 0,
                runs: vec![Run {
                    text: "auch nummeriert".into(),
                    ..Run::default()
                }],
                align: Default::default(),
            },
        ],
    };
    let mut first = true;
    for node in &mut doc.award.boq.body {
        if let BoqNode::Category(c) = node {
            if let BoqNode::Category(sub) = &mut c.body[0] {
                for node in &mut sub.body {
                    if let BoqNode::Item(item) = node {
                        if first {
                            item.long_text = text.clone();
                            first = false;
                        }
                    }
                }
            }
        }
    }
    let bytes = write(&doc).unwrap();
    let written = decode(&bytes, Encoding::Cp850);
    for line in written.lines().filter(|l| l.starts_with("26")) {
        assert!(line.chars().count() == 80, "{line}");
        assert!(line[5..60].chars().count() <= 55);
    }
    assert!(written.contains("26   Einpflegen des AKS / BAS, je BACnet-Objekt, in die"));
    assert!(written.contains("26   - erster Punkt"));
    assert!(written.contains("26   1. nummeriert"));
    let again = read(&bytes).unwrap().document;
    assert_eq!(again, doc);
}

#[test]
fn empty_lv_label_stays_empty() {
    let mut doc = read(&d83()).unwrap().document;
    doc.award.boq.label.clear();
    let again = read(&write(&doc).unwrap()).unwrap().document;
    assert_eq!(again.award.boq.label, "");
    assert_eq!(again, doc);
}

#[test]
fn standard_number_of_a_sub_description_in_record_24() {
    let bytes = datan(&[
        r("00", &[(11, "83"), (63, "1122PPPPI90")]),
        r("08", &[(3, "EUR")]),
        r("11", &[(3, "01"), (12, "N")]),
        r("12", &[(3, "Titel")]),
        r("11", &[(3, "0101"), (12, "N")]),
        r("12", &[(3, "Untertitel")]),
        r("21", &[(3, "01010010"), (12, "NNL"), (24, "00000001000St")]),
        r("25", &[(3, "Leitbeschreibung")]),
        r(
            "24",
            &[
                (3, " 1"),
                (6, "00000002000"),
                (17, "St"),
                (23, "1"),
                (24, "9300201123000202"),
            ],
        ),
        r("25", &[(3, "Unterbeschreibung")]),
        r("31", &[(3, "0101")]),
        r("31", &[(3, "01")]),
        r("99", &[(70, "00001")]),
    ]);
    let doc = read(&bytes).unwrap().document;
    let sub = &doc.award.boq.items()[0].1.sub_descriptions[0];
    assert_eq!(
        sub.standard_number,
        Some(StandardNumber {
            number: "9300201123000202".into(),
            catalog: "StLB".into(),
        })
    );
    let again = read(&write(&doc).unwrap()).unwrap().document;
    assert_eq!(again, doc);
}

/// Textergänzung mit Nachsatz nach dem schließenden Hochkomma („ mm,“) kommt
/// beim Lesen mit; Hinweistexte tragen ihre Textergänzungen in GAEB 90 als
/// Textzeilen (der Satz 26 eines Hinweises hat keine TE-Kennung).
#[test]
fn complement_tails_and_hint_complements_in_gaeb_90() {
    let bytes = datan(&[
        r("00", &[(11, "83L"), (63, "1122PPPPI90")]),
        r("11", &[(3, "01"), (12, "N"), (17, "Titel")]),
        r("12", &[(3, "GA")]),
        r("21", &[(3, "01010010"), (12, "NNN"), (24, "00000001000St")]),
        r("25", &[(3, "Kabel")]),
        r("26", &[(3, "11ALänge Kabel '1,5' m,")]),
        r("99", &[]),
    ]);
    let doc = read(&bytes).unwrap().document;
    let item = doc.award.boq.items()[0].1.clone();
    assert_eq!(item.complements[0].caption, "Länge Kabel");
    assert_eq!(item.complements[0].body, "1,5");
    assert_eq!(item.complements[0].tail, "m,");
    let written = write_text(&doc).unwrap();
    assert!(written.contains("11ALänge Kabel '1,5' m,"), "{written}");

    let mut with_hint = doc.clone();
    let remark = Remark {
        text: RichText {
            blocks: vec![TextBlock::paragraph(vec![Run {
                text: "Managementfunktionen,".into(),
                ..Run::default()
            }])],
        },
        complements: vec![TextComplement {
            marker: "11".into(),
            kind: "Owner".into(),
            caption: "Anzahl Bedienplätze ".into(),
            body: "2".into(),
            tail: ", ".into(),
            ..TextComplement::default()
        }],
        ..Remark::default()
    };
    if let BoqNode::Category(title) = &mut with_hint.award.boq.body[0] {
        title.body.insert(0, BoqNode::Remark(remark));
    }
    let written = write_text(&with_hint).unwrap();
    assert!(
        written
            .lines()
            .any(|line| line.starts_with("26   Anzahl Bedienplätze '2',")),
        "{written}"
    );
}

/// Steuerzeichen in einem Satz (Seitenvorschub, Escape aus alten
/// Programmen) werden zu Leerzeichen: die Spalten bleiben, nichts davon
/// kommt ins Projekt.
#[test]
fn control_characters_in_records_become_blanks() {
    let bytes = datan(&[
        r("00", &[(11, "83L"), (14, "VG"), (63, "1122PPPPI90")]),
        r("11", &[(3, "01"), (12, "N"), (17, "Titel")]),
        r("12", &[(3, "Auto\u{c}mation\u{1b}")]),
        r("99", &[]),
    ]);
    let report = read(&bytes).unwrap();
    let mut labels = Vec::new();
    fn walk(nodes: &[BoqNode], labels: &mut Vec<String>) {
        for node in nodes {
            if let BoqNode::Category(category) = node {
                labels.push(category.label.clone());
                walk(&category.body, labels);
            }
        }
    }
    walk(&report.document.award.boq.body, &mut labels);
    assert_eq!(labels, ["Auto mation"]);
    assert!(labels
        .iter()
        .all(|label| !label.chars().any(char::is_control)));
}

/// Nummer der Ausführungsbeschreibung in Satz 41 und Bezug in Satz 28: geschrieben
/// vierstellig mit führenden Nullen wie die übrigen Zahlenfelder („0001“, so auch
/// andere AVA-Programme), gelesen in jeder Schreibweise („0001“, „   1“, „1   “).
#[test]
fn performance_numbers_are_written_with_zeros_and_read_in_any_form() {
    let doc = read(&d83()).unwrap().document;
    let text = decode(&write(&doc).unwrap(), Encoding::Cp850);
    let perf = text.lines().find(|line| line.starts_with("41")).unwrap();
    assert_eq!(&perf[2..6], "0001", "{perf}");
    let reference = text
        .lines()
        .find(|line| line.starts_with("28") && line[66..70].trim() != "")
        .unwrap();
    assert_eq!(&reference[66..70], "0001", "{reference}");
    let back = read(&encode(&text, Encoding::Cp850)).unwrap().document;
    let items = back.award.boq.items();
    assert_eq!(items[0].1.kind.reference_description.as_deref(), Some("1"));

    for (number, referenced) in [("0001", "   1"), ("1   ", "0001"), ("   1", "1   ")] {
        let changed: Vec<String> = text
            .lines()
            .map(|line| {
                let mut line = line.to_string();
                if line.starts_with("41") {
                    line.replace_range(2..6, number);
                } else if line.starts_with("28") && line[66..70].trim() == "1" {
                    line.replace_range(66..70, referenced);
                }
                line
            })
            .collect();
        let doc = read(&encode(&changed.join("\r\n"), Encoding::Cp850))
            .unwrap()
            .document;
        let BoqNode::Category(title) = &doc.award.boq.body[1] else {
            panic!()
        };
        let BoqNode::Category(hardware) = &title.body[0] else {
            panic!()
        };
        let BoqNode::PerfDescr(perf) = &hardware.body[0] else {
            panic!()
        };
        assert_eq!(perf.number, "1", "{number:?}");
        let items = doc.award.boq.items();
        assert_eq!(
            items[0].1.kind.reference_description.as_deref(),
            Some("1"),
            "{referenced:?}"
        );
    }
}

/// Eine Textergänzung über mehrere Sätze 26 ist nur wegen der festen Satzlänge
/// umbrochen: beim Lesen entsteht wie im Fließtext ein Leerzeichen (ein
/// getrenntes Wort verliert den Trennstrich, ein Kurzwort „Zu-“ vor „und“ behält
/// ihn), kein Zeilenumbruch; Schreiben und wieder Lesen ändern den Inhalt nicht.
#[test]
fn complement_lines_are_joined_like_flowing_text() {
    let bytes = datan(&[
        r("00", &[(11, "83L"), (63, "1122PPPPI90")]),
        r("11", &[(3, "01"), (12, "N"), (17, "Titel")]),
        r("12", &[(3, "GA")]),
        r("21", &[(3, "01010010"), (12, "NNN"), (24, "00000001000St")]),
        r("25", &[(3, "Fühler")]),
        r("26", &[(6, "gemäß Einzelbeschreibung,")]),
        r("26", &[(3, "01AEinzelbeschreibungs-Nr '")]),
        r(
            "26",
            &[(3, "01AVorbemerkung Differenzdruck-Kontaktgeber Luft")],
        ),
        r("26", &[(3, "01AFilterwächter / Zu-")]),
        r("26", &[(3, "01Aund Abluft, Gebäude-")]),
        r("26", &[(3, "01Aautomation'")]),
        r("26", &[(6, ".")]),
        r("21", &[(3, "01010020"), (12, "NNN"), (24, "00000001000St")]),
        r("25", &[(3, "Automationsstation")]),
        r("26", &[(3, "61B'Hersteller\tEY6AS80F021\t2\tSt")]),
        r("26", &[(3, "61BHersteller\tEY6IO30F001\t11\tSt'")]),
        r("99", &[]),
    ]);
    let doc = read(&bytes).unwrap().document;
    let item = doc.award.boq.items()[0].1.clone();
    assert_eq!(item.complements.len(), 1);
    assert_eq!(item.complements[0].caption, "Einzelbeschreibungs-Nr");
    assert_eq!(
        item.complements[0].body,
        "Vorbemerkung Differenzdruck-Kontaktgeber Luft Filterwächter / Zu- und Abluft, Gebäudeautomation"
    );
    let again = read(&write(&doc).unwrap()).unwrap().document;
    assert_eq!(again.award.boq.items()[0].1.complements, item.complements);
    // A bidder's product list, one tab-separated line per record, keeps its lines.
    let station = doc.award.boq.items()[1].1.clone();
    assert_eq!(
        station.complements[0].body,
        "Hersteller\tEY6AS80F021\t2\tSt\nHersteller\tEY6IO30F001\t11\tSt"
    );
    assert_eq!(
        again.award.boq.items()[1].1.complements,
        station.complements
    );
}

/// D84 eines Bieterprogramms, das die vierstelligen Positionsnummern auf drei
/// Stellen kürzt: mehrere Preissätze (23) mit derselben OZ. Der letzte gilt wie
/// bisher, aber der Leser sagt es deutlich (statt still zu überschreiben).
#[test]
fn repeated_price_records_are_named() {
    let bytes = datan(&[
        r("00", &[(11, "84"), (63, "112233PPP90")]),
        r(
            "23",
            &[(3, "010101  1"), (13, "00000058570"), (25, "000000005857")],
        ),
        r(
            "23",
            &[(3, "010101  1"), (13, "00000073220"), (25, "000000007322")],
        ),
        r(
            "23",
            &[(3, "010101  1"), (13, "00000043930"), (25, "000000004393")],
        ),
        r(
            "23",
            &[(3, "010101  2"), (13, "00000010000"), (25, "000000001000")],
        ),
        r("99", &[]),
    ]);
    let report = read(&bytes).unwrap();
    let warning = report
        .warnings
        .iter()
        .find(|warning| warning.message.starts_with("ACHTUNG"))
        .expect("Hinweis auf wiederholte Ordnungszahlen");
    assert!(
        warning.message.contains("2 Preissätze (23) wiederholen")
            && warning
                .message
                .contains("(1 Ordnungszahl, z. B. 01.01.01.001 3×)")
            && warning.message.contains("vermutlich gekürzt"),
        "{}",
        warning.message
    );
    assert_eq!(warning.line, 3);
    let items = report.document.award.boq.items();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].1.unit_price,
        Some(dec("43.930")),
        "der letzte Satz gilt"
    );
}

/// Die Überschrift eines Hinweises (z. B. „Schlussblatt“, „Vorbemerkung
/// Kabel“) hat in GAEB 90 keinen eigenen Satz: sie steht als erste Textzeile
/// davor, statt zu fehlen.
#[test]
fn the_heading_of_a_hint_opens_its_text_in_gaeb_90() {
    let mut doc = read(&d83()).unwrap().document;
    doc.award.boq.body.push(BoqNode::Remark(Remark {
        short_text: "Schlussblatt".into(),
        text: RichText {
            blocks: vec![TextBlock::paragraph(vec![Run {
                text: "Der Bieter gibt die Zahl der Arbeitskräfte an.".into(),
                ..Run::default()
            }])],
        },
        ..Remark::default()
    }));
    let text = decode(&write(&doc).unwrap(), Encoding::Cp850);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("Schlussblatt"))
        .unwrap();
    assert!(
        lines[at].starts_with("26") && lines[at - 1].starts_with("20"),
        "{}",
        lines[at]
    );
    let back = read(&encode(&text, Encoding::Cp850)).unwrap().document;
    let Some(BoqNode::Remark(remark)) = back.award.boq.body.last() else {
        panic!()
    };
    assert_eq!(
        remark.text.to_plain(),
        "Schlussblatt\n\nDer Bieter gibt die Zahl der Arbeitskräfte an.",
        "Überschrift, Leerzeile, Text"
    );
}

/// Leere Kurztextsätze (25) am Ende – manche Programme füllen den Kurztext so
/// auf – tragen nichts: kein Zeilenumbruch am Ende des Kurztexts. Eine leere
/// Zeile in der Mitte bleibt; Schreiben und Lesen ändern nichts mehr.
#[test]
fn empty_short_text_records_at_the_end_are_left_out() {
    let bytes = datan(&[
        r("00", &[(11, "81"), (63, "1122PPPPI90")]),
        r("11", &[(3, "01"), (12, "N"), (17, "Titel")]),
        r("12", &[(3, "Heizung")]),
        r("21", &[(3, "01010010"), (12, "NNN"), (24, "00000002000St")]),
        r("25", &[(3, "Unterflurkonvektor HK 4-Leiter")]),
        r("25", &[(3, "160/290/0950 Rollrost Aluminium")]),
        r("25", &[]),
        r("26", &[(6, "Konvektor liefern und einbauen.")]),
        r("21", &[(3, "01010020"), (12, "NNN"), (24, "00000001000St")]),
        r("25", &[(3, "Erste Zeile.")]),
        r("25", &[]),
        r("25", &[(3, "Dritte Zeile")]),
        r("25", &[]),
        r("25", &[]),
        r("99", &[]),
    ]);
    let doc = read(&bytes).unwrap().document;
    let items = doc.award.boq.items();
    assert_eq!(
        items[0].1.short_text,
        "Unterflurkonvektor HK 4-Leiter\n160/290/0950 Rollrost Aluminium"
    );
    assert!(
        !items[1].1.short_text.ends_with('\n'),
        "{:?}",
        items[1].1.short_text
    );
    assert!(
        items[1].1.short_text.starts_with("Erste Zeile.\n")
            && items[1].1.short_text.ends_with("Dritte Zeile"),
        "Mitte bleibt: {:?}",
        items[1].1.short_text
    );
    let again = read(&write(&doc).unwrap()).unwrap().document;
    assert_eq!(
        again.award.boq.items()[0].1.short_text,
        items[0].1.short_text
    );
    assert_eq!(
        again.award.boq.items()[1].1.short_text,
        items[1].1.short_text
    );
}
