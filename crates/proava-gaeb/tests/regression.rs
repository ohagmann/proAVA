//! Regression tests with the small synthetic GAEB files in `tests/data`
//! (self-written, see `tests/data/README.md`). They mirror what the official
//! GAEB/BVBS test files contain: Hinweistext, titles, index, Bedarfs- and
//! Wahlpositionen, lump sum and hourly items, text complements, list, table
//! and image in the long text, surcharge items, lots, DA XML 3.1/3.2/3.3 and
//! the same LV as GAEB 90.
//!
//! Tests marked `#[ignore = "Befund …"]` describe known codec gaps found with
//! the official files (docs/BUILD.md); remove the mark once fixed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use proava_gaeb::{
    datan, xml, BoqNode, BreakdownKind, GaebDocument, Item, Phase, Provisional, StandardNumber,
    TextBlock,
};
use rust_decimal::Decimal;

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn dec(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn read_xml(name: &str) -> GaebDocument {
    let bytes = std::fs::read(data(name)).unwrap();
    let (doc, report) = xml::read_with_report(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
    doc
}

fn read_datan(name: &str) -> GaebDocument {
    let report = datan::read_file(data(name)).unwrap();
    assert!(report.warnings.is_empty(), "{name}: {:?}", report.warnings);
    report.document
}

/// Items by OZ with numeric pieces without leading zeros ("1.1.10.A").
fn items(doc: &GaebDocument) -> BTreeMap<String, Item> {
    doc.award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (key(&pieces), item.clone()))
        .collect()
}

fn key(pieces: &[String]) -> String {
    pieces
        .iter()
        .map(|piece| {
            let piece = piece.trim();
            if piece.chars().all(|c| c.is_ascii_digit()) {
                let stripped = piece.trim_start_matches('0');
                if stripped.is_empty() { "0" } else { stripped }.to_string()
            } else {
                piece.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Wahlbereiche (ALNB) of the LV's titles by OZ.
fn title_alternatives(
    nodes: &[BoqNode],
    path: &mut Vec<String>,
    out: &mut BTreeMap<String, proava_gaeb::Alternative>,
) {
    for node in nodes {
        if let BoqNode::Category(category) = node {
            path.push(category.rno_part.clone());
            if let Some(alternative) = &category.alternative {
                out.insert(key(path), alternative.clone());
            }
            title_alternatives(&category.body, path, out);
            path.pop();
        }
    }
}

/// The bid with item kinds, missing quantities, surcharge data and Wahl-
/// bereiche from the LV – an X84/D84 alone does not tell Wahl- or
/// Bedarfspositionen apart.
fn with_lv_kinds(bid: &GaebDocument, lv: &GaebDocument) -> GaebDocument {
    let mut titles = BTreeMap::new();
    title_alternatives(&lv.award.boq.body, &mut Vec::new(), &mut titles);
    fn walk(
        nodes: &mut [BoqNode],
        path: &mut Vec<String>,
        lv: &BTreeMap<String, Item>,
        titles: &BTreeMap<String, proava_gaeb::Alternative>,
    ) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    if category.alternative.is_none() {
                        category.alternative = titles.get(&key(path)).cloned();
                    }
                    walk(&mut category.body, path, lv, titles);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    let mut pieces = path.clone();
                    pieces.push(item.rno_part.clone());
                    if !item.rno_index.is_empty() {
                        pieces.push(item.rno_index.clone());
                    }
                    let Some(source) = lv.get(&key(&pieces)) else {
                        continue;
                    };
                    item.kind = source.kind.clone();
                    item.not_applicable |= source.not_applicable;
                    if item.qty.is_none() {
                        item.qty = source.qty;
                    }
                    if item
                        .surcharge
                        .as_ref()
                        .is_none_or(|s| s.markup_type.is_empty())
                    {
                        item.surcharge = source.surcharge.clone();
                    }
                }
                _ => {}
            }
        }
    }
    let mut doc = bid.clone();
    walk(
        &mut doc.award.boq.body,
        &mut Vec::new(),
        &items(lv),
        &titles,
    );
    doc
}

fn lv_total(doc: &GaebDocument) -> Option<Decimal> {
    doc.award.boq.totals.as_ref().and_then(|t| t.total)
}

#[test]
fn syn1_request_kinds_and_texts() {
    let doc = read_xml("syn1_33.X83");
    assert_eq!(doc.phase, Phase::Dp83);
    assert_eq!(doc.award.award_no, "V-2026-001");
    let boq = &doc.award.boq;
    assert_eq!(boq.breakdown.len(), 4);
    assert_eq!(boq.breakdown[3].kind, BreakdownKind::Index);
    assert!(matches!(boq.body[0], BoqNode::Remark(_)));

    let items = items(&doc);
    let keys: Vec<&str> = items.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "1.1.10", "1.1.20", "1.1.30", "1.1.40", "1.1.50", "1.1.60.A", "1.1.60.B", "1.2.10",
            "2.1.10", "2.1.20", "2.1.30"
        ]
    );
    assert_eq!(items["1.1.10"].qty, Some(dec("125.5")));
    assert_eq!(items["1.1.10"].unit, "m3");
    assert_eq!(
        items["1.1.20"].kind.provisional,
        Some(Provisional::WithTotal)
    );
    assert_eq!(
        items["1.1.30"].kind.provisional,
        Some(Provisional::WithoutTotal)
    );
    let base = items["1.1.40"].kind.alternative.as_ref().unwrap();
    let alternative = items["1.1.50"].kind.alternative.as_ref().unwrap();
    assert_eq!(base.serial.trim_start_matches('0'), "");
    assert_eq!(alternative.serial, "1");
    assert_eq!(base.group, alternative.group);
    assert!(items["1.1.60.A"].kind.lump_sum && items["1.1.60.B"].kind.lump_sum);
    assert!(items["1.2.10"].kind.hourly);

    let long = &items["1.1.10"].long_text.blocks;
    assert!(long
        .iter()
        .any(|b| matches!(b, TextBlock::ListItem { ordered: false, .. })));
    let table = long.iter().find_map(|b| match b {
        TextBlock::Table(rows) => Some(rows),
        _ => None,
    });
    assert_eq!(
        table.unwrap().texts()[1],
        vec!["Oberboden".to_string(), "0,30 m".to_string()]
    );
    assert!(long.iter().any(|b| match b {
        TextBlock::Paragraph { runs, .. } =>
            runs.iter().any(|r| r.bold && r.text == "Bodenklasse 3-5"),
        _ => false,
    }));

    let complements = &items["2.1.10"].complements;
    assert_eq!(complements.len(), 2);
    assert_eq!(
        (complements[0].kind.as_str(), complements[0].body.as_str()),
        ("Owner", "grau")
    );
    assert_eq!(
        (complements[1].kind.as_str(), complements[1].body.as_str()),
        ("Bidder", "")
    );

    let surcharge = items["2.1.30"].surcharge.as_ref().unwrap();
    assert_eq!(surcharge.markup_type, "ListInSubQty");
    assert_eq!(surcharge.items[0].ordinal, "02.01.0010");
}

#[test]
fn syn1_same_model_in_xml_31_32_33() {
    let v33 = read_xml("syn1_33.X83");
    for name in ["syn1_32.X83", "syn1_31.X83"] {
        let other = read_xml(name);
        assert_eq!(other.award.boq.body, v33.award.boq.body, "{name}");
        assert_eq!(other.award.boq.breakdown, v33.award.boq.breakdown, "{name}");
    }
}

#[test]
fn syn1_gaeb90_matches_xml() {
    let x = items(&read_xml("syn1_33.X83"));
    let d = items(&read_datan("syn1_33.D83"));
    // The GAEB 90 file has no surcharge item (2.1.30), the rest is the same LV.
    let x_keys: Vec<&String> = x.keys().filter(|k| *k != "2.1.30").collect();
    assert_eq!(x_keys, d.keys().collect::<Vec<_>>());
    for (oz, dx) in &d {
        let ix = &x[oz];
        assert_eq!(ix.qty, dx.qty, "{oz}");
        assert_eq!(ix.unit, dx.unit, "{oz}");
        assert_eq!(ix.short_text, dx.short_text, "{oz}");
        assert_eq!(ix.kind.provisional, dx.kind.provisional, "{oz}");
        assert_eq!(ix.kind.lump_sum, dx.kind.lump_sum, "{oz}");
        assert_eq!(ix.kind.hourly, dx.kind.hourly, "{oz}");
        assert_eq!(
            ix.kind
                .alternative
                .as_ref()
                .map(|a| a.serial.trim_start_matches('0')),
            dx.kind
                .alternative
                .as_ref()
                .map(|a| a.serial.trim_start_matches('0')),
            "{oz}"
        );
        assert_eq!(ix.complements.len(), dx.complements.len(), "{oz}");
    }
}

#[test]
fn syn1_bids_fit_the_lv_and_add_up() {
    let lv = read_xml("syn1_33.X83");
    let lv_items = items(&lv);
    for (name, bid) in [
        ("syn1_33.X84", read_xml("syn1_33.X84")),
        ("syn1_33.D84", read_datan("syn1_33.D84")),
    ] {
        let bid_items = items(&bid);
        for oz in bid_items.keys() {
            assert!(lv_items.contains_key(oz), "{name}: {oz} fehlt im LV");
        }
        let merged = with_lv_kinds(&bid, &lv);
        assert_eq!(
            Some(xml::computed_total(&merged.award.boq.body, &[])),
            lv_total(&bid),
            "{name}"
        );
    }
    assert_eq!(lv_total(&read_xml("syn1_33.X84")), Some(dec("7292.67")));
    let x = items(&read_xml("syn1_33.X84"));
    let d = items(&read_datan("syn1_33.D84"));
    for (oz, dx) in &d {
        assert_eq!(x[oz].unit_price, dx.unit_price, "{oz}");
        assert_eq!(x[oz].total, dx.total, "{oz}");
    }
    assert_eq!(x["2.1.10"].complements[0].body, "Musterbelag 2000");
    assert_eq!(d["2.1.10"].complements[0].body, "Musterbelag 2000");
}

#[test]
fn synthetic_files_roundtrip() {
    for name in [
        "syn1_31.X83",
        "syn1_32.X83",
        "syn1_33.X83",
        "syn1_33.X84",
        "syn2_lose_33.X83",
        "syn3_zuschlag_wahltitel_33.X83",
        "syn3_zuschlag_wahltitel_33.X84",
        "syn4_stlb_33.X83",
        "syn5_stlb_te_33.X83",
        "syn6_herstellertext_32.X81",
        "syn7_stlb_online_33.X81",
        "syn8_stlb_standardbeschreibung_33.X81",
    ] {
        let doc = read_xml(name);
        let written = xml::write(&doc, doc.phase).unwrap();
        let again = xml::read(written.as_bytes()).unwrap();
        assert_eq!(again, xml::as_written(&doc, doc.phase), "{name}");
    }
    let d83 = read_datan("syn1_33.D83");
    let again = datan::read(&datan::write(&d83).unwrap()).unwrap().document;
    assert_eq!(again, d83);
    let d84 = read_datan("syn1_33.D84");
    let again = datan::read(&datan::write(&d84).unwrap()).unwrap().document;
    assert_eq!(items(&again).len(), items(&d84).len());
    for ((oz, a), (_, b)) in items(&again).iter().zip(items(&d84).iter()) {
        assert_eq!((a.unit_price, a.total), (b.unit_price, b.total), "{oz}");
    }
    assert_eq!(lv_total(&again), lv_total(&d84));
}

#[test]
fn stlb_keys_from_a_tender() {
    let doc = read_xml("syn4_stlb_33.X83");
    let items = items(&doc);
    let first = &items["1.10"];
    let stlb = first.stlb.as_ref().expect("STLB-Bau-Kennung");
    assert_eq!(
        (
            stlb.catalog.as_str(),
            stlb.version.as_deref(),
            stlb.work_category.as_str(),
            stlb.group,
            stlb.cost_group.as_str(),
        ),
        ("STLB-Bau", Some("2026-04"), "070", 9001, "481")
    );
    let ids: Vec<(i64, Option<i64>, Option<i64>, bool)> = stlb
        .characteristics
        .iter()
        .map(|id| (id.characteristic, id.index, id.value, id.short_text_part))
        .collect();
    assert_eq!(
        ids,
        vec![
            (90011, Some(0), Some(2), true),
            (90012, Some(0), Some(7), false),
            (90013, None, None, false),
        ]
    );
    assert_eq!(first.complements[0].stlb_characteristic, Some(90013));
    assert_eq!(first.complements[0].body, "reinweiß");
    let second = &items["1.20"];
    assert_eq!(second.stlb.as_ref().map(|stlb| stlb.group), Some(9002));
    assert_eq!(second.stlb.as_ref().unwrap().cost_group, "");
    assert_eq!(second.complements[0].kind, "Bidder");
    assert_eq!(second.complements[0].stlb_characteristic, Some(90022));
    let third = &items["1.30"];
    assert_eq!(third.stlb, None);
    assert_eq!(
        third.standard_number,
        Some(StandardNumber {
            number: "12345678901234567".into(),
            catalog: "StLB".into(),
        })
    );
    let fourth = &items["1.40"];
    assert_eq!(
        (fourth.stlb.is_none(), fourth.standard_number.is_none()),
        (true, true)
    );
}

#[test]
fn lots_in_xml() {
    let doc = read_xml("syn2_lose_33.X83");
    let boq = &doc.award.boq;
    assert_eq!(boq.breakdown[0].kind, BreakdownKind::Lot);
    let keys: Vec<String> = items(&doc).into_keys().collect();
    assert_eq!(keys, ["1.1.10", "2.1.10"]);
    let BoqNode::Category(lot) = &boq.body[1] else {
        panic!("Los erwartet");
    };
    assert_eq!(lot.label, "Los 2 Ausbau");
}

/// BVBS-Prüfdatei 3.3 AVA, Titel 1.30.20: AllInCat applies to the items
/// since the previous surcharge group (0070 = 10 % of 0060 only).
#[test]
fn surcharge_all_in_cat_as_bvbs() {
    let lv = read_xml("syn3_zuschlag_wahltitel_33.X83");
    let bid = with_lv_kinds(&read_xml("syn3_zuschlag_wahltitel_33.X84"), &lv);
    let BoqNode::Category(title) = &bid.award.boq.body[0] else {
        panic!("Titel erwartet");
    };
    assert_eq!(
        xml::computed_total(&title.body, &["01".to_string()]),
        dec("1790.00")
    );
}

/// BVBS-Prüfdatei 3.3 AVA, Titel 1.10.40: Wahl-Titel (ALNBGroupNo/ALNBSerNo
/// at BoQCtgy) are not part of the LV total.
#[test]
fn alternative_title_not_in_total() {
    let lv = read_xml("syn3_zuschlag_wahltitel_33.X83");
    let bid = with_lv_kinds(&read_xml("syn3_zuschlag_wahltitel_33.X84"), &lv);
    assert_eq!(
        xml::computed_total(&bid.award.boq.body, &[]),
        dec("2290.00")
    );
}

/// DA86 has no AwardInfo/Cat, OpenDate … and needs CTR (GAEB_DA_XML_86_3.3).
#[test]
fn x86_follows_the_schema_order() {
    let doc = read_xml("syn1_33.X83");
    let x86 = xml::write(&doc, Phase::Dp86).unwrap();
    assert!(
        !x86.contains("<Cat>"),
        "AwardInfo/Cat gibt es in DA86 nicht"
    );
    assert!(
        !x86.contains("<OpenDate>"),
        "OpenDate gibt es in DA86 nicht"
    );
    assert!(x86.contains("<CTR>"), "CTR ist in DA86 Pflicht");
}

/// Images in the long text are part of DA XML (`<image>` in `<p>`): the
/// reader keeps them as image blocks, the writer leaves them out unless asked.
#[test]
fn image_in_long_text_is_not_lost_silently() {
    let doc = read_xml("syn1_33.X83");
    let images: Vec<&TextBlock> = doc
        .award
        .boq
        .items()
        .into_iter()
        .flat_map(|(_, item)| item.long_text.blocks.iter())
        .filter(|block| matches!(block, TextBlock::Image { .. }))
        .collect();
    assert_eq!(images.len(), 1);
    let TextBlock::Image {
        file_name,
        data,
        width_mm,
    } = images[0]
    else {
        unreachable!()
    };
    assert!(data.starts_with(b"\x89PNG"), "{file_name}");
    assert!(file_name.ends_with(".png"));
    assert!(width_mm.is_some());

    let plain = xml::write(&doc, Phase::Dp83).unwrap();
    assert!(!plain.contains("<image"), "Standard: ohne Bilder");
    let options = xml::WriteOptions {
        include_images: true,
    };
    let with = xml::write_with(&doc, Phase::Dp83, &options).unwrap();
    assert!(with.contains("<image Encoding=\"base64\" Type=\"image/png\""));
    let again = xml::read(with.as_bytes()).unwrap();
    assert_eq!(again, xml::as_written_with(&doc, Phase::Dp83, &options));
    let bid = xml::write_with(&doc, Phase::Dp84, &options).unwrap();
    assert!(!bid.contains("<image"), "X84 erlaubt keine Bilder");
}

/// All synthetic XML files written again in their phase match the GAEB 3.3
/// XSDs. Needs the XSDs (`proava_gaeb::schemas`, folder with GAEB_DA_XML_8x_3.3_*.xsd)
/// and xmllint or python3-lxml.
#[test]
#[ignore]
fn synthetic_files_written_match_the_schema() {
    let Some(xsd) = proava_gaeb::schemas::schema_folder() else {
        panic!(
            "Keine GAEB-XSDs gefunden in {:?} – laden mit scripts/gaeb_muster_laden.sh",
            proava_gaeb::schemas::schema_folders()
        );
    };
    let xsd = xsd.to_string_lossy().to_string();
    // A folder only this test uses (unique, created exclusively), removed
    // with `scratch` at the end of the test – nothing else.
    let scratch = tempfile::Builder::new()
        .prefix("proava-gaeb-regr-")
        .tempdir()
        .unwrap();
    let dir = scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(data("")).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        if !name.to_ascii_uppercase().contains(".X8") {
            continue;
        }
        let doc = xml::read_file(&path).unwrap();
        let out = dir.join(format!("{name}.{}", xml::file_extension(doc.phase)));
        xml::write_file(&doc, doc.phase, &out).unwrap();
    }
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/validate_xsd.py");
    let output = std::process::Command::new("python3")
        .args([script, &xsd, dir.to_str().unwrap()])
        .output()
        .unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "{report}");
}

#[test]
fn text_complements_keep_their_place_in_the_long_text() {
    let x = items(&read_xml("syn4_stlb_33.X83"));
    let first = x.values().next().unwrap();
    let text = match &first.long_text.blocks[0] {
        proava_gaeb::TextBlock::Paragraph { runs, .. } => {
            runs.iter().map(|run| run.text.as_str()).collect::<String>()
        }
        other => panic!("{other:?}"),
    };
    // „… Farbe [Ergänzung].“ – die Marke ist aus dem Text wieder heraus.
    assert!(text.ends_with("Farbe ."), "{text}");
    assert!(!text.contains('\u{FFFC}'));
    let anchor = first.complements[0].anchor.unwrap();
    assert!(!anchor.own_line);
    assert_eq!(anchor.block, 0);
    assert_eq!(anchor.offset, text.chars().count() - 1);
}
