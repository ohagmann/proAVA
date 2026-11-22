use std::str::FromStr;

use rust_decimal::Decimal;

use super::*;
use crate::{datan, xml};

fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn level(kind: BreakdownKind, length: u8) -> BreakdownLevel {
    BreakdownLevel {
        kind,
        label: if kind == BreakdownKind::Level {
            "Titel".into()
        } else {
            String::new()
        },
        length,
        numeric: kind != BreakdownKind::Index,
    }
}

/// Titel (2) . Position (4) . Index (1)
fn mask() -> Vec<BreakdownLevel> {
    vec![
        level(BreakdownKind::Level, 2),
        level(BreakdownKind::Item, 4),
        level(BreakdownKind::Index, 1),
    ]
}

fn item(part: &str) -> Item {
    Item {
        rno_part: part.into(),
        qty: Some(dec("2")),
        unit: "St".into(),
        short_text: "Kabel".into(),
        long_text: RichText::plain("Kabel liefern und verlegen."),
        ..Item::default()
    }
}

fn priced(part: &str, qty: &str, price: &str) -> Item {
    let mut item = item(part);
    item.qty = Some(dec(qty));
    item.unit_price = Some(dec(price));
    item.total = xml::item_total(&item);
    item
}

fn title(part: &str, body: Vec<BoqNode>) -> BoqNode {
    BoqNode::Category(Category {
        rno_part: part.into(),
        label: format!("Titel {part}"),
        body,
        ..Category::default()
    })
}

fn doc(phase: Phase, body: Vec<BoqNode>) -> GaebDocument {
    GaebDocument {
        phase,
        program: "Test".into(),
        date: Some("2026-11-17".into()),
        project: ProjectInfo {
            name: "Projekt".into(),
            currency: "EUR".into(),
            currency_label: "Euro".into(),
            ..ProjectInfo::default()
        },
        award: Award {
            boq: Boq {
                name: "LV".into(),
                breakdown: mask(),
                body,
                ..Boq::default()
            },
            ..Award::default()
        },
    }
}

fn good_lv(phase: Phase) -> GaebDocument {
    let items = if phase.has_prices() {
        vec![
            BoqNode::Item(priced("0010", "2", "10.50")),
            BoqNode::Item(priced("0020", "3.5", "7.33")),
        ]
    } else {
        vec![BoqNode::Item(item("0010")), BoqNode::Item(item("0020"))]
    };
    let mut doc = doc(
        phase,
        vec![
            title("01", items),
            title("02", vec![BoqNode::Item(item_for(phase, "0010"))]),
        ],
    );
    if phase.has_prices() {
        xml::fill_totals(&mut doc.award.boq);
    }
    doc
}

fn item_for(phase: Phase, part: &str) -> Item {
    if phase.has_prices() {
        priced(part, "2", "100")
    } else {
        item(part)
    }
}

fn ids(findings: &[Finding]) -> Vec<&'static str> {
    findings.iter().map(|finding| finding.id).collect()
}

fn has(findings: &[Finding], id: &str) -> bool {
    findings.iter().any(|finding| finding.id == id)
}

fn severity_of(findings: &[Finding], id: &str) -> Severity {
    findings
        .iter()
        .find(|finding| finding.id == id)
        .unwrap_or_else(|| panic!("{id} fehlt in {:?}", ids(findings)))
        .severity
}

#[test]
fn a_clean_lv_has_no_findings_in_any_phase() {
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let findings = check(&good_lv(phase), phase, None);
        assert!(findings.is_empty(), "{phase:?}: {findings:#?}");
    }
}

#[test]
fn finding_names_rule_severity_and_reference() {
    let mut lv = good_lv(Phase::Dp83);
    lv.award.boq.breakdown.clear();
    let finding = &check(&lv, Phase::Dp83, None)[0];
    assert_eq!(finding.code(), "R1");
    assert_eq!(finding.rule(), 1);
    assert_eq!(
        finding.to_string(),
        "R1 Fehler: Die OZ-Maske (BoQBkdn) fehlt. [FD 3.3 11.2.3 BoQInfo 5]"
    );
    assert_eq!(counts(&check(&lv, Phase::Dp83, None)), [1, 0, 0]);
}

// ------------------------------------------------------------------ R1 ----

#[test]
fn r1_mask_rules() {
    let check_mask = |breakdown: Vec<BreakdownLevel>| {
        let mut lv = good_lv(Phase::Dp83);
        lv.award.boq.breakdown = breakdown;
        check(&lv, Phase::Dp83, None)
    };
    use BreakdownKind::*;
    assert!(has(&check_mask(vec![]), "R1.mask_missing"));
    assert!(has(&check_mask(vec![level(Level, 2)]), "R1.item"));
    assert!(has(
        &check_mask(vec![level(Level, 2), level(Item, 4), level(Item, 2)]),
        "R1.item"
    ));
    assert!(has(
        &check_mask(vec![level(Level, 2), level(Lot, 1), level(Item, 4)]),
        "R1.lot"
    ));
    assert!(has(
        &check_mask(vec![level(Lot, 1), level(Lot, 1), level(Item, 4)]),
        "R1.lot"
    ));
    let six = vec![
        level(Lot, 1),
        level(Level, 1),
        level(Level, 1),
        level(Level, 1),
        level(Level, 1),
        level(Level, 1),
        level(Item, 4),
    ];
    assert!(has(&check_mask(six), "R1.levels"));
    assert!(has(
        &check_mask(vec![level(Item, 4), level(Level, 2)]),
        "R1.levels"
    ));
    let mut wide_index = level(Index, 2);
    wide_index.length = 2;
    assert!(has(
        &check_mask(vec![level(Level, 2), level(Item, 4), wide_index]),
        "R1.index"
    ));
    assert!(has(
        &check_mask(vec![level(Level, 2), level(Index, 1), level(Item, 4)]),
        "R1.index"
    ));
    assert!(has(
        &check_mask(vec![level(Level, 6), level(Level, 4), level(Item, 5)]),
        "R1.length"
    ));
    let mut labelled = level(Item, 4);
    labelled.label = "Position".into();
    assert!(has(
        &check_mask(vec![level(Level, 2), labelled]),
        "R1.label"
    ));
    // Five levels including the lot, 14 places: allowed.
    let five = vec![
        level(Lot, 1),
        level(Level, 2),
        level(Level, 2),
        level(Level, 2),
        level(Level, 2),
        level(Item, 4),
        level(Index, 1),
    ];
    assert!(!check_mask(five).iter().any(|f| f.id.starts_with("R1.")));
}

#[test]
fn r1_unit_price_components_need_a_declaration() {
    let mut lv = good_lv(Phase::Dp82);
    if let BoqNode::Category(category) = &mut lv.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.up_components = vec![Some(dec("10.50"))];
        }
    }
    assert!(has(&check(&lv, Phase::Dp82, None), "R1.up_components"));
    lv.award.boq.up_component_labels = vec!["Lohn".into()];
    assert!(!has(&check(&lv, Phase::Dp82, None), "R1.up_components"));
}

#[test]
fn r1_bid_without_mask_is_fine() {
    let mut bid = good_lv(Phase::Dp84);
    bid.award.boq.breakdown.clear();
    assert!(!has(&check(&bid, Phase::Dp84, None), "R1.mask_missing"));
}

// ------------------------------------------------------------------ R2 ----

#[test]
fn r2_pieces_against_the_mask() {
    let lv = doc(
        Phase::Dp83,
        vec![
            title("001", vec![BoqNode::Item(item("0010"))]),
            title("1A", vec![BoqNode::Item(item("0010"))]),
            title("Ä1", vec![BoqNode::Item(item("0010"))]),
            title("05", vec![BoqNode::Item(item("0010"))]),
        ],
    );
    let findings: Vec<Finding> = check(&lv, Phase::Dp83, None)
        .into_iter()
        .filter(|f| f.id.starts_with("R2"))
        .collect();
    assert!(has(&findings, "R2.length"), "{:?}", ids(&findings));
    assert!(has(&findings, "R2.numeric"));
    assert!(has(&findings, "R2.characters"));
    assert!(!findings.iter().any(|f| f.ordinal.as_deref() == Some("05")));
}

#[test]
fn r2_zero_level_is_only_a_hint() {
    let lv = doc(
        Phase::Dp83,
        vec![title("00", vec![BoqNode::Item(item("0000"))])],
    );
    let findings = check(&lv, Phase::Dp83, None);
    assert_eq!(ids(&findings), ["R2.zero", "R2.zero"], "{findings:#?}");
    assert!(findings.iter().all(|f| f.severity == Severity::Hint));
}

#[test]
fn r2_index() {
    let mut with_index = item("0010");
    with_index.rno_index = "AB".into();
    let mut lv = doc(
        Phase::Dp83,
        vec![title("01", vec![BoqNode::Item(with_index)])],
    );
    assert!(has(&check(&lv, Phase::Dp83, None), "R2.index"));
    if let BoqNode::Category(category) = &mut lv.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.rno_index = "A".into();
        }
    }
    assert!(check(&lv, Phase::Dp83, None).is_empty());
    lv.award.boq.breakdown.pop();
    assert!(has(&check(&lv, Phase::Dp83, None), "R2.index"));
}

// ------------------------------------------------------------------ R3 ----

#[test]
fn r3_unique_and_ascending() {
    let duplicate = doc(
        Phase::Dp83,
        vec![title(
            "01",
            vec![BoqNode::Item(item("0010")), BoqNode::Item(item("0010"))],
        )],
    );
    let findings = check(&duplicate, Phase::Dp83, None);
    assert_eq!(severity_of(&findings, "R3.duplicate"), Severity::Error);
    assert_eq!(findings[0].ordinal.as_deref(), Some("01.0010"));

    let descending = doc(
        Phase::Dp83,
        vec![
            title("02", vec![BoqNode::Item(item("0010"))]),
            title(
                "01",
                vec![BoqNode::Item(item("0020")), BoqNode::Item(item("0010"))],
            ),
        ],
    );
    let findings = check(&descending, Phase::Dp83, None);
    let order: Vec<_> = findings
        .iter()
        .filter(|f| f.id == "R3.order")
        .map(|f| f.ordinal.clone().unwrap())
        .collect();
    assert_eq!(order, ["01", "01.0010"], "{findings:#?}");
    assert_eq!(severity_of(&findings, "R3.order"), Severity::Warning);
}

#[test]
fn r3_sort_order_of_the_characters() {
    // FD 3.3 4.2.1: space < digits < upper case < underscore < lower case.
    let sorted = [" 1", "11", "1A", "1_", "1a", "A1", "a1"];
    for pair in sorted.windows(2) {
        assert_eq!(compare_piece(pair[0], pair[1]), Ordering::Less, "{pair:?}");
    }
    assert_eq!(compare_piece("9", "10"), Ordering::Less);
    assert_eq!(compare_piece("0010", "10"), Ordering::Equal);
    let index = |pieces: &[&str]| pieces.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    assert_eq!(
        compare_pieces(&index(&["01", "0010"]), &index(&["01", "0010", "A"])),
        Ordering::Less
    );
}

// ------------------------------------------------------------------ R4 ----

#[test]
fn r4_outline() {
    let mut deep = doc(
        Phase::Dp83,
        vec![title(
            "01",
            vec![title("02", vec![BoqNode::Item(item("0010"))])],
        )],
    );
    assert!(has(&check(&deep, Phase::Dp83, None), "R4.too_deep"));
    // With two levels the same tree is fine, and a title of the upper level
    // may hold items directly (FD 3.3 4.3.4: lower levels may stay empty).
    deep.award
        .boq
        .breakdown
        .insert(0, level(BreakdownKind::Level, 2));
    deep.award
        .boq
        .body
        .push(title("02", vec![BoqNode::Item(item("0010"))]));
    let findings = check(&deep, Phase::Dp83, None);
    assert!(
        !findings.iter().any(|f| f.id.starts_with("R4")),
        "{findings:#?}"
    );

    let mixed = doc(
        Phase::Dp83,
        vec![
            title("01", vec![BoqNode::Item(item("0010"))]),
            BoqNode::Item(item("0020")),
        ],
    );
    let findings = check(&mixed, Phase::Dp83, None);
    assert!(has(&findings, "R4.mixed"));
    assert!(has(&findings, "R4.outside"));
}

// ------------------------------------------------------------------ R5 ----

fn alternative(group: &str, serial: &str) -> Option<Alternative> {
    Some(Alternative {
        group: group.into(),
        serial: serial.into(),
    })
}

#[test]
fn r5_base_and_alternative_belong_together() {
    let mut base = item("0010");
    base.kind.alternative = alternative("1", "0");
    let mut wahl = item("0020");
    wahl.kind.alternative = alternative("01", "1");
    let both = doc(
        Phase::Dp83,
        vec![title(
            "01",
            vec![BoqNode::Item(base.clone()), BoqNode::Item(wahl.clone())],
        )],
    );
    assert!(check(&both, Phase::Dp83, None).is_empty());

    let only_alternative = doc(Phase::Dp83, vec![title("01", vec![BoqNode::Item(wahl)])]);
    assert_eq!(
        severity_of(
            &check(&only_alternative, Phase::Dp83, None),
            "R5.without_base"
        ),
        Severity::Error
    );
    let only_base = doc(Phase::Dp83, vec![title("01", vec![BoqNode::Item(base)])]);
    assert!(has(
        &check(&only_base, Phase::Dp83, None),
        "R5.without_alternative"
    ));

    // A Grundbereich with a Wahlposition of the same group.
    let mut area = title("01", vec![BoqNode::Item(item("0010"))]);
    if let BoqNode::Category(category) = &mut area {
        category.alternative = alternative("7", "0");
    }
    let mut wahl = item("0010");
    wahl.kind.alternative = alternative("7", "1");
    let lv = doc(
        Phase::Dp83,
        vec![area, title("02", vec![BoqNode::Item(wahl)])],
    );
    assert!(check(&lv, Phase::Dp83, None).is_empty());
}

// ------------------------------------------------------------------ R6 ----

#[test]
fn r6_no_total_for_items_without_total() {
    let mut base = priced("0010", "2", "10");
    base.kind.alternative = alternative("1", "0");
    let mut wahl = priced("0020", "2", "12");
    wahl.kind.alternative = alternative("1", "1");
    let mut provisional = priced("0030", "1", "5");
    provisional.kind.provisional = Some(Provisional::WithoutTotal);
    let mut lv = doc(
        Phase::Dp82,
        vec![title(
            "01",
            vec![
                BoqNode::Item(base),
                BoqNode::Item(wahl),
                BoqNode::Item(provisional),
            ],
        )],
    );
    xml::fill_totals(&mut lv.award.boq);
    let findings = check(&lv, Phase::Dp82, None);
    let flagged: Vec<_> = findings
        .iter()
        .filter(|f| f.id == "R6.total")
        .map(|f| f.ordinal.clone().unwrap())
        .collect();
    assert_eq!(flagged, ["01.0020", "01.0030"], "{findings:#?}");
    assert!(!has(&check(&lv, Phase::Dp83, None), "R6.total"));

    let mut area = title("02", vec![BoqNode::Item(priced("0010", "1", "1"))]);
    if let BoqNode::Category(category) = &mut area {
        category.alternative = alternative("1", "2");
        category.totals = Some(Totals {
            total: Some(dec("1")),
            ..Totals::default()
        });
    }
    lv.award.boq.body.push(area);
    let findings = check(&lv, Phase::Dp82, None);
    assert_eq!(severity_of(&findings, "R6.area_total"), Severity::Hint);
}

// ------------------------------------------------------------------ R7 ----

fn surcharge(part: &str, kind: &str, percent: &str) -> Item {
    Item {
        rno_part: part.into(),
        short_text: "Zuschlag".into(),
        long_text: RichText::plain("Zuschlag auf die Positionen."),
        markup_percent: Some(dec(percent)),
        surcharge: Some(Surcharge {
            markup_type: kind.into(),
            items: Vec::new(),
        }),
        kind: ItemKind {
            markup: true,
            ..ItemKind::default()
        },
        ..Item::default()
    }
}

#[test]
fn r7_surcharge_structure() {
    let one = |markup: Item| {
        let lv = doc(
            Phase::Dp83,
            vec![title(
                "01",
                vec![BoqNode::Item(item("0010")), BoqNode::Item(markup)],
            )],
        );
        check(&lv, Phase::Dp83, None)
    };
    assert!(has(
        &one(surcharge("0020", "ListInSubQty", "10")),
        "R7.sub_qty"
    ));
    let mut listed = surcharge("0020", "AllInCat", "10");
    listed
        .surcharge
        .as_mut()
        .unwrap()
        .items
        .push(SurchargeItem {
            ordinal: "01.0010".into(),
            sub_qty: None,
        });
    assert!(has(&one(listed), "R7.sub_qty"));
    assert!(has(&one(surcharge("0020", "Sonstwie", "10")), "R7.type"));
    assert_eq!(
        severity_of(&one(surcharge("0020", "IdentAsMark", "10")), "R7.marked"),
        Severity::Warning
    );
    assert!(one(surcharge("0020", "AllInCat", "10")).is_empty());
}

#[test]
fn r7_surcharge_amounts() {
    let lv = |base: Option<&str>, total: &str| {
        let mut markup = surcharge("0030", "AllInCat", "10");
        markup.markup_base = base.map(dec);
        markup.total = Some(dec(total));
        doc(
            Phase::Dp82,
            vec![title(
                "01",
                vec![
                    BoqNode::Item(priced("0010", "1", "100")),
                    BoqNode::Item(priced("0020", "2", "50")),
                    BoqNode::Item(markup),
                ],
            )],
        )
    };
    let only_r7 = |doc: &GaebDocument| -> Vec<&'static str> {
        check(doc, Phase::Dp82, None)
            .iter()
            .filter(|f| f.id.starts_with("R7"))
            .map(|f| f.id)
            .collect()
    };
    assert!(only_r7(&lv(Some("200"), "20")).is_empty());
    assert!(only_r7(&lv(None, "20")).is_empty());
    assert_eq!(only_r7(&lv(Some("180"), "18")), ["R7.base"]);
    assert_eq!(only_r7(&lv(Some("200"), "21")), ["R7.total"]);
}

// ------------------------------------------------------------------ R8 ----

#[test]
fn r8_item_and_title_sums() {
    let mut lv = good_lv(Phase::Dp82);
    assert!(check(&lv, Phase::Dp82, None).is_empty());
    if let BoqNode::Category(category) = &mut lv.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[1] {
            item.total = Some(dec("25.67"));
        }
        category.totals = None;
    }
    // Titles summing the stated (wrong) item total: only the item is wrong.
    lv.award.boq.totals = None;
    xml::fill_totals(&mut lv.award.boq);
    let findings = check(&lv, Phase::Dp82, None);
    assert_eq!(ids(&findings), ["R8.item_total"], "{findings:#?}");

    let mut lv = good_lv(Phase::Dp82);
    if let BoqNode::Category(category) = &mut lv.award.boq.body[0] {
        category.totals.as_mut().unwrap().total = Some(dec("1"));
    }
    let findings = check(&lv, Phase::Dp82, None);
    assert!(has(&findings, "R8.category_total"));
    assert!(has(&findings, "R8.boq_total"));
}

#[test]
fn r8_discount_rounding_as_in_the_fachdokumentation() {
    // FD 3.3 4.6.17: 73,789 m3 × 9,77 € = 720,92; 1,5 % Nachlass → 710,11.
    let mut example = item("0010");
    example.qty = Some(dec("73.789"));
    example.unit_price = Some(dec("9.77"));
    example.discount_percent = Some(dec("1.5"));
    assert_eq!(xml::item_total(&example), Some(dec("710.11")));

    let mut lv = good_lv(Phase::Dp82);
    let totals = lv.award.boq.totals.as_mut().unwrap();
    totals.total = Some(dec("720.92"));
    totals.discount_percent = Some(dec("1.5"));
    totals.total_after_discount = Some(dec("710.11"));
    let lv_total = |lv: &GaebDocument| {
        check(lv, Phase::Dp82, None)
            .into_iter()
            .filter(|f| f.id == "R8.after_discount")
            .count()
    };
    assert_eq!(lv_total(&lv), 0);
    lv.award.boq.totals.as_mut().unwrap().total_after_discount = Some(dec("710.10"));
    assert_eq!(lv_total(&lv), 1);
    lv.award.boq.totals.as_mut().unwrap().total_after_discount = None;
    assert_eq!(lv_total(&lv), 1);
}

#[test]
fn r8_gross_and_vat() {
    let totals = Totals {
        total_net: Some(dec("1600")),
        vat_percent: Some(dec("19")),
        vat_amount: Some(dec("304")),
        total_gross: Some(dec("1904")),
        ..Totals::default()
    };
    let mut out = Vec::new();
    totals_problems(&totals, "LV", None, &mut out);
    assert!(out.is_empty());
    let wrong = Totals {
        vat_amount: Some(dec("300")),
        ..totals
    };
    totals_problems(&wrong, "LV", None, &mut out);
    let found: Vec<_> = out
        .iter()
        .map(|(id, severity, ..)| (*id, *severity))
        .collect();
    assert_eq!(
        found,
        [("R8.gross", Severity::Error), ("R8.vat", Severity::Warning)]
    );
}

// ------------------------------------------------------------------ R9 ----

#[test]
fn r9_item_rules() {
    let single = |item: Item, phase: Phase| {
        check(
            &doc(phase, vec![title("01", vec![BoqNode::Item(item)])]),
            phase,
            None,
        )
    };
    let mut no_qty = item("0010");
    no_qty.qty = None;
    assert!(has(&single(no_qty.clone(), Phase::Dp83), "R9.qty"));
    let mut lump = no_qty.clone();
    lump.kind.lump_sum = true;
    assert!(!has(&single(lump, Phase::Dp83), "R9.qty"));
    let mut free = no_qty.clone();
    free.qty_to_be_determined = true;
    assert!(single(free.clone(), Phase::Dp83).is_empty());
    free.qty = Some(dec("3"));
    assert!(has(&single(free.clone(), Phase::Dp83), "R9.qty_tbd"));
    assert!(!has(&single(free, Phase::Dp86), "R9.qty_tbd"));

    let mut no_unit = item("0010");
    no_unit.unit.clear();
    assert!(has(&single(no_unit.clone(), Phase::Dp83), "R9.unit"));
    assert!(!has(&single(no_unit, Phase::Dp84), "R9.unit"));

    let mut no_text = item("0010");
    no_text.short_text.clear();
    no_text.long_text = RichText::default();
    assert!(has(&single(no_text, Phase::Dp83), "R9.description"));

    let mut both = item("0010");
    both.kind.repetition_of = Some("01.0005".into());
    both.kind.reference_description = Some("1".into());
    assert!(has(&single(both, Phase::Dp83), "R9.references"));

    let mut parts = priced("0010", "2", "10");
    parts.up_components = vec![Some(dec("4")), Some(dec("5"))];
    let mut lv = doc(
        Phase::Dp82,
        vec![title("01", vec![BoqNode::Item(parts.clone())])],
    );
    lv.award.boq.up_component_labels = vec!["Lohn".into(), "Stoffe".into()];
    xml::fill_totals(&mut lv.award.boq);
    assert!(has(&check(&lv, Phase::Dp82, None), "R9.components"));
    parts.up_components = vec![Some(dec("4")), Some(dec("6"))];
    lv.award.boq.body = vec![title("01", vec![BoqNode::Item(parts.clone())])];
    xml::fill_totals(&mut lv.award.boq);
    assert!(check(&lv, Phase::Dp82, None).is_empty());
    lv.award.boq.up_component_labels.pop();
    assert!(has(&check(&lv, Phase::Dp82, None), "R9.components"));

    let mut not_offered = priced("0010", "2", "10");
    not_offered.not_offered = true;
    assert!(has(&single(not_offered, Phase::Dp84), "R9.not_offered"));
}

// ----------------------------------------------------------------- R10 ----

fn perf(number: &str) -> BoqNode {
    BoqNode::PerfDescr(PerfDescr {
        number: number.into(),
        short_text: "Ausführung".into(),
        long_text: RichText::plain("Wie folgt."),
        ..PerfDescr::default()
    })
}

fn complement(marker: &str, kind: &str) -> TextComplement {
    TextComplement {
        marker: marker.into(),
        kind: kind.into(),
        ..TextComplement::default()
    }
}

#[test]
fn r10_performance_descriptions_and_markers() {
    let mut referring = item("0010");
    referring.kind.reference_description = Some("1".into());
    let before = doc(
        Phase::Dp83,
        vec![title(
            "01",
            vec![perf("1"), BoqNode::Item(referring.clone())],
        )],
    );
    assert!(check(&before, Phase::Dp83, None).is_empty());
    let after = doc(
        Phase::Dp83,
        vec![title("01", vec![BoqNode::Item(referring), perf("1")])],
    );
    assert!(has(&check(&after, Phase::Dp83, None), "R10.perf_order"));
    let twice = doc(
        Phase::Dp83,
        vec![title(
            "01",
            vec![perf("1"), perf("1"), BoqNode::Item(item("0010"))],
        )],
    );
    assert!(has(&check(&twice, Phase::Dp83, None), "R10.perf_no"));

    let mut marked = item("0010");
    marked.complements = vec![complement("1", "Bidder"), complement("2", "Owner")];
    let lv = doc(
        Phase::Dp83,
        vec![title("01", vec![BoqNode::Item(marked.clone())])],
    );
    assert!(check(&lv, Phase::Dp83, None).is_empty());
    marked.complements.push(complement("2", "Bidder"));
    marked.complements.push(complement("a", "Bidder"));
    let lv = doc(Phase::Dp83, vec![title("01", vec![BoqNode::Item(marked)])]);
    let markers = check(&lv, Phase::Dp83, None)
        .into_iter()
        .filter(|f| f.id == "R10.marker")
        .count();
    assert_eq!(markers, 2);
}

const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-11-17</Date><ProgSystem>Test 1.0</ProgSystem></GAEBInfo>
<Award><DP>83</DP><BoQ ID="b"><BoQBody><BoQCtgy ID="c" RNoPart="01"><BoQBody>"#;
const TAIL: &str = "</BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>";

fn xml_item(flags: &str, detail: &str, outline: &str) -> String {
    format!(
        r#"{HEAD}<Itemlist><Item ID="i" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description><CompleteText>{flags}<DetailTxt><Text><p>Text {detail}</p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Kurz {outline}</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item></Itemlist>{TAIL}"#
    )
}

#[test]
fn r10_complement_flags_in_the_file() {
    let bidder = r#"<TextComplement Kind="Bidder" MarkLbl="1"><ComplBody><span>...</span></ComplBody></TextComplement>"#;
    let owner = r#"<TextComplement Kind="Owner" MarkLbl="2"><ComplBody><span>x</span></ComplBody></TextComplement>"#;
    assert!(check_xml(xml_item("", "", "").as_bytes()).is_empty());
    let findings = check_xml(xml_item("", bidder, "").as_bytes());
    assert_eq!(ids(&findings), ["R10.compl_ts"]);
    assert_eq!(findings[0].ordinal.as_deref(), Some("01.0010"));
    assert!(check_xml(xml_item("<ComplTSB>Yes</ComplTSB>", bidder, "").as_bytes()).is_empty());
    assert_eq!(
        ids(&check_xml(
            xml_item("<ComplTSB>Yes</ComplTSB>", "", owner).as_bytes()
        )),
        ["R10.compl_ts"]
    );
    assert!(check_xml(xml_item("<ComplTSA>Yes</ComplTSA>", "", owner).as_bytes()).is_empty());
    assert_eq!(
        ids(&check_xml(
            xml_item("<ComplTSA>Yes</ComplTSA>", "", bidder).as_bytes()
        )),
        ["R10.short_text_bidder"]
    );
}

// ----------------------------------------------------------------- R11 ----

#[test]
fn r11_bid_against_request() {
    let mut request = good_lv(Phase::Dp83);
    if let BoqNode::Category(category) = &mut request.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.complements = vec![complement("1", "Bidder"), complement("2", "Owner")];
        }
        if let BoqNode::Item(item) = &mut category.body[1] {
            item.qty = None;
            item.qty_to_be_determined = true;
        }
    }
    request.award.award_no = "V-17".into();
    let mut bid = good_lv(Phase::Dp84);
    bid.award.award_no = "V-17".into();
    if let BoqNode::Category(category) = &mut bid.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.complements = vec![complement("1", "Bidder")];
        }
    }
    assert!(
        check_bid(&bid, &request).is_empty(),
        "{:#?}",
        check_bid(&bid, &request)
    );

    let mut changed = bid.clone();
    changed.award.boq.breakdown[1].length = 3;
    changed.award.award_no = "V-18".into();
    changed.project.currency = "CHF".into();
    if let BoqNode::Category(category) = &mut changed.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.complements = vec![complement("2", "Bidder"), complement("3", "Bidder")];
            item.qty = Some(dec("5"));
        }
        if let BoqNode::Item(item) = &mut category.body[1] {
            item.qty = None;
        }
        category.body.push(BoqNode::Item(priced("0099", "1", "1")));
    }
    let findings = check_bid(&changed, &request);
    let mut found = ids(&findings);
    found.sort();
    assert_eq!(
        found,
        [
            "R11.award_no",
            "R11.breakdown",
            "R11.complement",
            "R11.complement",
            "R11.currency",
            "R11.free_qty",
            "R11.quantity",
            "R11.unknown_ordinal"
        ]
    );
    // check() runs R11 only for bids with a request.
    assert!(has(
        &check(&changed, Phase::Dp84, Some(&request)),
        "R11.breakdown"
    ));
    assert!(!has(&check(&changed, Phase::Dp84, None), "R11.breakdown"));
}

#[test]
fn r11_padding_does_not_matter() {
    let request = good_lv(Phase::Dp83);
    let mut bid = good_lv(Phase::Dp84);
    if let BoqNode::Category(category) = &mut bid.award.boq.body[0] {
        category.rno_part = "1".into();
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.rno_part = "10".into();
        }
    }
    assert!(!has(&check_bid(&bid, &request), "R11.unknown_ordinal"));
}

// ----------------------------------------------------------------- R12 ----

#[test]
fn r12_header_of_the_file() {
    let good = xml_item("", "", "");
    assert!(check_xml(good.as_bytes()).is_empty());
    let date = good.replace(
        "<VersDate>2021-05</VersDate>",
        "<VersDate>2013-10</VersDate>",
    );
    assert_eq!(ids(&check_xml(date.as_bytes())), ["R12.version_date"]);
    let version = good.replace("<Version>3.3</Version>", "<Version>2.1</Version>");
    assert_eq!(ids(&check_xml(version.as_bytes())), ["R12.version"]);
    let program = good.replace(
        "<ProgSystem>Test 1.0</ProgSystem>",
        "<ProgSystem>Test</ProgSystem>",
    );
    assert_eq!(ids(&check_xml(program.as_bytes())), ["R12.program"]);
    let namespace = good.replace("DA83/3.3", "DA84/3.3");
    assert_eq!(ids(&check_xml(namespace.as_bytes())), ["R12.namespace"]);
    let old = good
        .replace("<Version>3.3</Version>", "<Version>3.1</Version>")
        .replace("2021-05", "2009-12")
        .replace("DA83/3.3", "200407");
    assert!(check_xml(old.as_bytes()).is_empty());
}

// ----------------------------------------------------------------- R13 ----

#[test]
fn r13_tender_for_the_e_vergabe() {
    let single = |item: Item| {
        check(
            &doc(Phase::Dp83, vec![title("01", vec![BoqNode::Item(item)])]),
            Phase::Dp83,
            None,
        )
    };
    let mut no_long = item("0010");
    no_long.long_text = RichText::default();
    assert_eq!(
        severity_of(&single(no_long), "R13.long_text"),
        Severity::Warning
    );
    let mut zero = item("0010");
    zero.qty = Some(Decimal::ZERO);
    assert!(has(&single(zero), "R13.zero_qty"));
    let mut lines = item("0010");
    lines.short_text = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".into();
    assert!(has(&single(lines), "R13.short_text"));
    let mut gap = item("0010");
    gap.long_text = RichText::plain("Fabrikat: ..........");
    assert_eq!(severity_of(&single(gap), "R13.gap"), Severity::Hint);
    let mut underscores = item("0010");
    underscores.long_text = RichText::plain("Typ: ______");
    assert!(has(&single(underscores), "R13.gap"));
    let mut fine = item("0010");
    fine.long_text = RichText::plain("Ende des Satzes... Weiter. 1.2.3");
    assert!(single(fine).is_empty());
    let mut price = item("0010");
    price.unit_price = Some(dec("1"));
    assert!(has(&single(price), "R13.prices"));
    let mut image = item("0010");
    image.long_text.blocks.push(TextBlock::Image {
        file_name: "a.png".into(),
        data: vec![1],
        width_mm: None,
    });
    assert!(has(&single(image.clone()), "R13.images"));
    // Only the request for quotation.
    let lv = doc(Phase::Dp81, vec![title("01", vec![BoqNode::Item(image)])]);
    assert!(check(&lv, Phase::Dp81, None).is_empty());
}

#[test]
fn r13_bidder_complement_in_a_remark() {
    let remark = format!(
        r#"{HEAD}<Remark ID="r"><Description><CompleteText><DetailTxt><Text><p>Hinweis <TextComplement Kind="Bidder" MarkLbl="1"><ComplBody><span>...</span></ComplBody></TextComplement></p></Text></DetailTxt></CompleteText></Description></Remark><BoQCtgy ID="d" RNoPart="02"/>{TAIL}"#
    );
    assert_eq!(
        ids(&check_xml(remark.as_bytes())),
        ["R13.remark_complement"]
    );
    let x81 = remark
        .replace("<DP>83</DP>", "<DP>81</DP>")
        .replace("DA83", "DA81");
    assert!(check_xml(x81.as_bytes()).is_empty());
}

fn record(content: &str, number: usize) -> String {
    format!("{content:<74}{number:06}")
}

#[test]
fn r13_gaeb_90_lines() {
    let good = [record("00", 1), record("11", 2), record("99", 3)].join("\r\n") + "\r\n";
    assert!(check_datan(good.as_bytes()).is_empty());
    let gap = [record("00", 1), record("11", 3), record("99", 4)].join("\n");
    assert_eq!(ids(&check_datan(gap.as_bytes())), ["R13.datan_numbering"]);
    let empty = [record("00", 1), String::new(), record("99", 2)].join("\n");
    assert_eq!(
        ids(&check_datan(empty.as_bytes())),
        ["R13.datan_empty_line"]
    );
    let ends = [record("11", 1), record("12", 2)].join("\n");
    assert_eq!(
        ids(&check_datan(ends.as_bytes())),
        ["R13.datan_start", "R13.datan_end"]
    );
    let long = [record("00", 1), record("99", 2) + "X"].join("\n");
    assert_eq!(
        ids(&check_datan(long.as_bytes())),
        ["R13.datan_line_length"]
    );
}

// ------------------------------------------------- what proAVA writes ----

fn findings_of_written(doc: &GaebDocument, phase: Phase) -> Vec<Finding> {
    let text = xml::write(doc, phase).unwrap();
    let mut findings = check_xml(text.as_bytes());
    let again = xml::read(text.as_bytes()).unwrap();
    findings.extend(check(&again, phase, None));
    findings
}

#[test]
fn files_written_by_proava_follow_the_rules() {
    let mut lv = good_lv(Phase::Dp82);
    if let BoqNode::Category(category) = &mut lv.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut category.body[0] {
            item.complements = vec![complement("1", "Bidder"), complement("2", "Owner")];
        }
    }
    for phase in [
        Phase::Dp81,
        Phase::Dp82,
        Phase::Dp83,
        Phase::Dp84,
        Phase::Dp85,
        Phase::Dp86,
    ] {
        let findings = findings_of_written(&lv, phase);
        assert!(findings.is_empty(), "{phase:?}: {findings:#?}");
    }
    let request = xml::read(xml::write(&lv, Phase::Dp83).unwrap().as_bytes()).unwrap();
    let bid = xml::read(xml::write(&lv, Phase::Dp84).unwrap().as_bytes()).unwrap();
    assert!(
        check_bid(&bid, &request).is_empty(),
        "{:#?}",
        check_bid(&bid, &request)
    );
}

#[test]
fn gaeb_90_written_by_proava_follows_the_rules() {
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let mut lv = good_lv(phase);
        lv.phase = phase;
        let bytes = datan::write(&lv).unwrap();
        let mut findings = check_datan(&bytes);
        let again = datan::read(&bytes).unwrap().document;
        findings.extend(check(&again, phase, None));
        assert!(findings.is_empty(), "{phase:?}: {findings:#?}");
    }
}
