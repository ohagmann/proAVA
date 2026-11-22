//! GAEB DA XML codec: synthetic snippets and roundtrips (no customer files).

use std::str::FromStr;

use proava_gaeb::xml::{self, GaebXmlError};
use proava_gaeb::*;
use rust_decimal::Decimal;

fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        ..Run::default()
    }
}

fn paragraph(text: &str) -> TextBlock {
    TextBlock::paragraph(vec![run(text)])
}

fn item(rno: &str, qty: &str, unit: &str, price: &str, short: &str) -> Item {
    Item {
        rno_part: rno.into(),
        qty: Some(dec(qty)),
        unit: unit.into(),
        unit_price: Some(dec(price)),
        short_text: short.into(),
        long_text: RichText {
            blocks: vec![paragraph(&format!("{short}, Langtext"))],
        },
        ..Item::default()
    }
}

/// An LV with every item kind the codec knows.
fn sample() -> GaebDocument {
    let mut normal = item("0010", "12.5", "m", "10.255", "Kabel verlegen");
    normal.up_components = vec![Some(dec("4.10")), Some(dec("6.155"))];
    normal.time_quantity = Some(dec("0.25"));
    normal.kind.markup = true;
    normal.catalog_assigns = vec![
        CatalogAssign {
            catalog: "idDIN276-18".into(),
            code: "482".into(),
        },
        CatalogAssign {
            catalog: "KG2".into(),
            code: "070".into(),
        },
    ];
    normal.long_text = RichText {
        blocks: vec![
            TextBlock::paragraph(vec![
                run("Normal "),
                Run {
                    text: "fett".into(),
                    bold: true,
                    ..Run::default()
                },
                run(" und\nzweite Zeile"),
            ]),
            TextBlock::paragraph(vec![]),
            TextBlock::ListItem {
                ordered: false,
                level: 0,
                runs: vec![run("Punkt eins")],
                align: Default::default(),
            },
            TextBlock::ListItem {
                ordered: false,
                level: 0,
                runs: vec![Run {
                    text: "Punkt zwei".into(),
                    italic: true,
                    underline: true,
                    ..Run::default()
                }],
                align: Default::default(),
            },
            TextBlock::ListItem {
                ordered: true,
                level: 0,
                runs: vec![run("Erstens")],
                align: Default::default(),
            },
            TextBlock::Table(Table {
                header_rows: 1,
                column_widths_mm: vec![Some(25.4), None],
                ..Table::from_texts(vec![
                    vec!["Typ".into(), "Anzahl".into()],
                    vec!["AI".into(), "4".into()],
                ])
            }),
            TextBlock::Paragraph {
                runs: vec![Run {
                    text: "Mittig in Times 14 pt".into(),
                    font: Some("Times New Roman".into()),
                    size_pt: Some(14.0),
                    ..Run::default()
                }],
                align: Align::Center,
            },
            TextBlock::Paragraph {
                runs: vec![Run {
                    text: "Blocksatz Courier".into(),
                    font: Some("Courier New".into()),
                    italic: true,
                    underline: true,
                    ..Run::default()
                }],
                align: Align::Justify,
            },
        ],
    };
    normal.complements = vec![
        TextComplement {
            marker: "1".into(),
            kind: "Bidder".into(),
            caption: "Fabrikat:".into(),
            ..TextComplement::default()
        },
        TextComplement {
            marker: "2".into(),
            kind: "Owner".into(),
            caption: "Farbe".into(),
            body: "RAL 9016".into(),
            tail: "matt".into(),
            stlb_characteristic: Some(910),
            number: None,
            // „Normal |fett“: mitten im ersten Absatz (Schema- und Rundlauftests).
            anchor: Some(TextAnchor {
                block: 0,
                offset: 7,
                own_line: false,
            }),
        },
    ];

    let mut lump = item("0020", "1", "psch", "1500", "Inbetriebnahme");
    lump.kind.lump_sum = true;
    lump.kind.markup = true;
    let mut without = item("0030", "10", "h", "65", "Regiestunden");
    without.kind.provisional = Some(Provisional::WithoutTotal);
    without.kind.hourly = true;
    let mut with = item("0040", "2", "St", "100", "Bedarf mit GB");
    with.kind.provisional = Some(Provisional::WithTotal);
    with.stlb = Some(stlb_reference());
    let mut base = item("0050", "3", "St", "200", "Grundposition");
    base.standard_number = Some(StandardNumber {
        number: "12345678901234567".into(),
        catalog: "StLB".into(),
    });
    base.kind.alternative = Some(Alternative {
        group: "1".into(),
        serial: "0".into(),
    });
    let mut alternative = item("0060", "3", "St", "250", "Wahlposition");
    alternative.kind.alternative = Some(Alternative {
        group: "1".into(),
        serial: "1".into(),
    });
    let mut not_applicable = item("0070", "1", "St", "99", "Entfällt");
    not_applicable.not_applicable = true;
    let mut tbd = item("0080", "0", "m", "5", "Menge vom Bieter");
    tbd.qty = None;
    tbd.qty_to_be_determined = true;
    let mut repeat = item("0090", "4", "m", "10.255", "Wie 0010");
    repeat.kind.repetition_of = Some("01.01.0010".into());
    repeat.key_item = true;
    let mut indexed = item("0100", "1", "St", "50", "Mit Index");
    indexed.rno_index = "A".into();
    indexed.kind.reference_description = Some("1".into());
    indexed.up_breakdown = true;
    let mut leader = item("0110", "1", "St", "80", "Leitbeschreibung");
    leader.sub_descriptions = vec![SubDescription {
        number: "1".into(),
        qty: Some(dec("2")),
        unit: "St".into(),
        short_text: "Unterbeschreibung".into(),
        long_text: RichText {
            blocks: vec![paragraph("Text der Unterbeschreibung")],
        },
        stlb: None,
        standard_number: Some(StandardNumber {
            number: "9300201123000202".into(),
            catalog: "StLB".into(),
        }),
    }];
    let surcharge = Item {
        rno_part: "0120".into(),
        short_text: "Zuschlag Kleinmaterial".into(),
        markup_percent: Some(dec("5")),
        markup_base: Some(dec("1628.19")),
        kind: ItemKind {
            markup: true,
            ..ItemKind::default()
        },
        surcharge: Some(Surcharge {
            markup_type: "IdentAsMark".into(),
            items: Vec::new(),
        }),
        ..Item::default()
    };
    let mut discounted = item("0010", "8", "St", "12.50", "Mit Nachlass");
    discounted.discount_percent = Some(dec("10"));

    let title1 = Category {
        rno_part: "01".into(),
        label: "Rohbau".into(),
        body: vec![BoqNode::Category(Category {
            rno_part: "01".into(),
            label: "Kabel".into(),
            body: vec![
                BoqNode::Remark(Remark {
                    short_text: "Hinweis".into(),
                    text: RichText {
                        blocks: vec![paragraph("Alle Kabel halogenfrei.")],
                    },
                    ..Remark::default()
                }),
                BoqNode::PerfDescr(PerfDescr {
                    number: "1".into(),
                    label: "Ausführung AS".into(),
                    short_text: "Automationsstation".into(),
                    long_text: RichText {
                        blocks: vec![paragraph("Beschreibung der Station")],
                    },
                    stlb: Some(stlb_reference()),
                    standard_number: None,
                    complements: Vec::new(),
                }),
                BoqNode::Item(normal),
                BoqNode::Item(lump),
                BoqNode::Item(without),
                BoqNode::Item(with),
                BoqNode::Item(base),
                BoqNode::Item(alternative),
                BoqNode::Item(not_applicable),
                BoqNode::Item(tbd),
                BoqNode::Item(repeat),
                BoqNode::Item(indexed),
                BoqNode::Item(leader),
                BoqNode::Item(surcharge),
            ],
            totals: None,
            alternative: None,
            catalog_assigns: Vec::new(),
        })],
        totals: None,
        alternative: None,
        catalog_assigns: vec![CatalogAssign {
            catalog: "idDIN276-18".into(),
            code: "481".into(),
        }],
    };
    let title2 = Category {
        rno_part: "02".into(),
        label: "Nachlass-Titel".into(),
        body: vec![BoqNode::Category(Category {
            rno_part: "01".into(),
            label: "Mit Nachlass".into(),
            body: vec![BoqNode::Item(discounted)],
            totals: Some(Totals {
                discount_percent: Some(dec("2")),
                total_after_discount: Some(dec("88.20")),
                ..Totals::default()
            }),
            alternative: None,
            catalog_assigns: Vec::new(),
        })],
        totals: None,
        alternative: None,
        catalog_assigns: Vec::new(),
    };
    GaebDocument {
        phase: Phase::Dp83,
        program: "Test".into(),
        date: Some("2026-11-17".into()),
        project: ProjectInfo {
            name: "TEST1".into(),
            label: "Testprojekt".into(),
            currency: "EUR".into(),
            currency_label: "Euro".into(),
            up_fraction_digits: None,
        },
        award: Award {
            owner: Some(Address {
                name1: "Bauherr GmbH".into(),
                name2: "Hochbauamt".into(),
                name3: "Abteilung 2".into(),
                name4: "Sachgebiet Technik".into(),
                street: "Hauptstraße 1".into(),
                postal_code: "40210".into(),
                city: "Düsseldorf".into(),
                country: "Deutschland".into(),
                iln: "4012345000009".into(),
                contact: "Frau Muster".into(),
                phone: "0211 123".into(),
                fax: "0211 124".into(),
                email: "bau@example.org".into(),
                vat_id: "DE123456789".into(),
            }),
            bidder: None,
            bid_date: None,
            submission: Submission {
                location: "Vergabestelle".into(),
                open_date: Some("2026-12-01".into()),
                open_time: Some("10:00:00".into()),
                construction_start: Some("2027-01-04".into()),
                construction_end: Some("2027-06-30".into()),
                award_deadline: Some("2027-01-15".into()),
            },
            boq: Boq {
                change_order: None,
                catalogs: vec![
                    Catalog {
                        id: "idDIN276-18".into(),
                        kind: "cost group DIN 276 2018-12".into(),
                        name: "DIN 276-18".into(),
                    },
                    Catalog {
                        id: "KG2".into(),
                        kind: "work category".into(),
                        name: "Leistungsbereiche".into(),
                    },
                ],
                name: "480".into(),
                label: "Gebäudeautomation".into(),
                breakdown: vec![
                    BreakdownLevel {
                        kind: BreakdownKind::Level,
                        label: "Titel".into(),
                        length: 2,
                        numeric: true,
                    },
                    BreakdownLevel {
                        kind: BreakdownKind::Level,
                        label: "Untertitel".into(),
                        length: 2,
                        numeric: true,
                    },
                    BreakdownLevel {
                        kind: BreakdownKind::Item,
                        label: String::new(),
                        length: 4,
                        numeric: true,
                    },
                    BreakdownLevel {
                        kind: BreakdownKind::Index,
                        label: String::new(),
                        length: 1,
                        numeric: false,
                    },
                ],
                body: vec![BoqNode::Category(title1), BoqNode::Category(title2)],
                totals: None,
                up_component_labels: vec!["Lohn".into(), "Material".into()],
                time_label: "Stunden".into(),
                up_component_kinds: vec!["Wages".into(), "Materials".into()],
                date: Some("2026-11-17".into()),
            },
            award_no: "VG-2026-1".into(),
            owner_dp_no: "430-V03".into(),
            owner_account: "D-4711".into(),
            owner_country: Country {
                kind: "EEA".into(),
                name: "D".into(),
            },
            bidder_dp_no: String::new(),
            requesters: vec![Place {
                address: Some(Address {
                    name1: "Schulamt".into(),
                    street: "Weg 1".into(),
                    postal_code: "40210".into(),
                    city: "Düsseldorf".into(),
                    ..Address::default()
                }),
                id_no: "R-1".into(),
                name: "Bedarfsträger Schule".into(),
            }],
            sites: vec![
                Site {
                    site: Place {
                        address: Some(Address {
                            name1: "Schulzentrum".into(),
                            street: "Allee 3".into(),
                            postal_code: "40211".into(),
                            city: "Düsseldorf".into(),
                            ..Address::default()
                        }),
                        id_no: "B-1".into(),
                        name: "Neubau Haus A".into(),
                    },
                    notification: Some(Place {
                        address: None,
                        id_no: "N-1".into(),
                        name: "Bauleitung Container".into(),
                    }),
                },
                Site {
                    site: Place {
                        address: None,
                        id_no: "B-2".into(),
                        name: "Haus B".into(),
                    },
                    notification: None,
                },
            ],
            contract_text: Vec::new(),
            category: "OpenCall".into(),
            add_texts: vec![AddText {
                outline: "Vorbemerkungen".into(),
                detail: RichText {
                    blocks: vec![paragraph("Es gilt die VOB.")],
                },
            }],
            // Schlussbemerkung: AddText at the root after the Award.
            closing_texts: vec![AddText {
                outline: "Schlussblatt".into(),
                detail: RichText {
                    blocks: vec![paragraph("Der Bieter gibt die Zahl der Arbeitskräfte an.")],
                },
            }],
        },
    }
}

/// A made-up STLB-Bau key (numbers are not taken from STLB-Bau).
fn stlb_reference() -> StlbReference {
    StlbReference {
        catalog: "STLB-Bau".into(),
        version: Some("2026-04".into()),
        work_category: "070".into(),
        group: 901,
        cost_group: "481".into(),
        characteristics: vec![
            StlbCharacteristic {
                characteristic: 900,
                index: Some(0),
                value: Some(3),
                short_text_part: true,
            },
            StlbCharacteristic {
                characteristic: 910,
                index: None,
                value: None,
                short_text_part: false,
            },
            StlbCharacteristic {
                characteristic: 905,
                index: Some(1),
                value: Some(12),
                short_text_part: false,
            },
        ],
    }
}

fn roundtrip(doc: &GaebDocument, phase: Phase) -> GaebDocument {
    let text = xml::write(doc, phase).unwrap();
    xml::read(text.as_bytes()).unwrap()
}

#[test]
fn x83_roundtrip_keeps_the_tender() {
    let doc = sample();
    let again = roundtrip(&doc, Phase::Dp83);
    assert_eq!(again, xml::as_written(&doc, Phase::Dp83));
    // What X83 drops: prices, NotAppl.
    let items = again.award.boq.items();
    assert!(items.iter().all(|(_, item)| item.unit_price.is_none()));
    assert!(items.iter().all(|(_, item)| !item.not_applicable));
    // Everything else stays.
    let (_, first) = &items[0];
    assert_eq!(first.long_text, {
        let doc = sample();
        let items = doc.award.boq.items();
        items[0].1.long_text.clone()
    });
    // Reihenfolge wie im Text: die Ergänzung mitten im ersten Absatz zuerst.
    assert_eq!(first.complements[0].body, "RAL 9016");
    assert_eq!(first.complements[1].body, "");
    assert_eq!(again.award.boq.time_label, "Stunden");
    // Bedarfsträger, Ausführungs- und Benachrichtigungsorte, OWN-Felder.
    assert_eq!(again.award.requesters, sample().award.requesters);
    assert_eq!(again.award.sites, sample().award.sites);
    assert_eq!(again.award.owner_account, "D-4711");
    assert_eq!(again.award.owner_country.name, "D");
}

#[test]
fn x86_keeps_requester_sites_and_both_dp_numbers() {
    let mut doc = sample();
    doc.award.bidder = Some(Address {
        name1: "Firma".into(),
        street: "Weg 2".into(),
        postal_code: "45127".into(),
        city: "Essen".into(),
        ..Address::default()
    });
    doc.award.bidder_dp_no = "AN-17".into();
    let text = xml::write(&doc, Phase::Dp86).unwrap();
    // XSD order: OWN, Requester, CTR, CnstSite, NotifSite
    let at = |tag: &str| text.find(tag).unwrap_or_else(|| panic!("{tag}: {text}"));
    assert!(at("<OWN>") < at("<Requester>"));
    assert!(at("<Requester>") < at("<CTR>"));
    assert!(at("<CTR>") < at("<CnstSite>"));
    assert!(at("<CnstSite>") < at("<NotifSite>"));
    let again = xml::read(text.as_bytes()).unwrap();
    assert_eq!(again.award.bidder_dp_no, "AN-17");
    assert_eq!(again.award.owner_dp_no, "430-V03");
    assert_eq!(again.award.sites.len(), 2);
    assert_eq!(
        again.award.sites[0].notification.as_ref().unwrap().name,
        "Bauleitung Container"
    );
    // A bid has none of them.
    let bid = xml::write(&doc, Phase::Dp84).unwrap();
    assert!(!bid.contains("<Requester>") && !bid.contains("<CnstSite>"));
    assert!(bid.contains("<DPNo>AN-17</DPNo>"));
}

#[test]
fn x82_roundtrip_with_prices_and_totals() {
    let doc = sample();
    let again = roundtrip(&doc, Phase::Dp82);
    assert_eq!(again, xml::as_written(&doc, Phase::Dp82));
    let items: Vec<(String, Item)> = again
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (pieces.join("."), item.clone()))
        .collect();
    let total = |oz: &str| items.iter().find(|(key, _)| key == oz).unwrap().1.total;
    // 12.5 × 10.255 = 128.1875 → 128.19 (kaufmännisch)
    assert_eq!(total("01.01.0010"), Some(dec("128.19")));
    assert_eq!(total("01.01.0030"), None, "Bedarf ohne GB");
    assert_eq!(total("01.01.0060"), None, "Wahlposition");
    assert_eq!(total("01.01.0070"), None, "entfällt");
    assert_eq!(
        total("02.01.0010"),
        Some(dec("90.00")),
        "100 − 10 % Nachlass"
    );
    // 128.19 + 1500 + 200 + 600 + 41.02 + 50 + 80 + Zuschlag 81.41
    let title = match &again.award.boq.body[0] {
        BoqNode::Category(category) => category.totals.clone().unwrap(),
        _ => unreachable!(),
    };
    assert_eq!(title.total, Some(dec("2680.62")));
    assert_eq!(
        again.award.boq.totals.unwrap().total,
        Some(dec("2768.82")),
        "Titel 02 zählt mit 88,20 nach Nachlass"
    );
}

#[test]
fn x81_and_x84_roundtrip() {
    let doc = sample();
    assert_eq!(
        roundtrip(&doc, Phase::Dp81),
        xml::as_written(&doc, Phase::Dp81)
    );

    let mut bid = sample();
    bid.award.bidder = Some(Address {
        name1: "Bieter AG".into(),
        street: "Weg 2".into(),
        postal_code: "45127".into(),
        city: "Essen".into(),
        ..Address::default()
    });
    bid.award.bid_date = Some("2026-11-30".into());
    let again = roundtrip(&bid, Phase::Dp84);
    assert_eq!(again, xml::as_written(&bid, Phase::Dp84));
    assert_eq!(again.award.bidder.as_ref().unwrap().name1, "Bieter AG");
    let items = again.award.boq.items();
    assert!(items.iter().all(|(_, item)| item.short_text.is_empty()));
    assert_eq!(items[0].1.unit_price, Some(dec("10.255")));
    assert_eq!(items[0].1.complements.len(), 1, "nur Bieterangaben");
    assert!(again
        .award
        .boq
        .items()
        .iter()
        .any(|(_, item)| item.markup_percent.is_some()));
}

const SNIPPET_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.2">
 <GAEBInfo><Version>3.2</Version><VersDate>2013-10</VersDate><Date>2024-01-02</Date><ProgName>Fremd</ProgName></GAEBInfo>
 <PrjInfo><NamePrj>P</NamePrj><LblPrj>Projekt</LblPrj><Cur>EUR</Cur><CurLbl>Euro</CurLbl></PrjInfo>
 <Award>
  <DP>83</DP>
  <BoQ ID="b">
   <BoQInfo><Name>1</Name><LblBoQ>LV</LblBoQ>
    <BoQBkdn><Type>BoQLevel</Type><LblBoQBkdn>Titel</LblBoQBkdn><Length>2</Length><Num>Yes</Num></BoQBkdn>
    <BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn>
    <BoQBkdn><Type>Index</Type><Length>1</Length><Num>No</Num></BoQBkdn>
   </BoQInfo>
   <BoQBody>
    <BoQCtgy ID="c1" RNoPart="1">
     <LblTx><p><span>Titel eins</span></p></LblTx>
     <BoQBody><Itemlist>
"#;
const SNIPPET_TAIL: &str = "
     </Itemlist></BoQBody>
    </BoQCtgy>
   </BoQBody>
  </BoQ>
 </Award>
</GAEB>";

fn snippet(items: &str) -> GaebDocument {
    xml::read(format!("{SNIPPET_HEAD}{items}{SNIPPET_TAIL}").as_bytes()).unwrap()
}

fn only_item(doc: &GaebDocument) -> Item {
    doc.award.boq.items()[0].1.clone()
}

#[test]
fn reads_item_kinds_from_snippets() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="10" RNoIndex="a">
             <ALNGroupNo>12</ALNGroupNo><ALNSerNo>1</ALNSerNo>
             <Provis>WithTotal</Provis><LumpSumItem>Yes</LumpSumItem><HourIt>Yes</HourIt>
             <KeyIt>Yes</KeyIt><MarkupIt>Yes</MarkupIt>
             <Qty>1.000</Qty><QU>psch</QU>
             <Description><OutlineText><OutlTxt><TextOutlTxt><span>Nur Kurztext</span></TextOutlTxt></OutlTxt></OutlineText></Description>
           </Item>"#,
    );
    assert_eq!(doc.phase, Phase::Dp83);
    assert_eq!(doc.program, "Fremd");
    let item = only_item(&doc);
    let pieces = &doc.award.boq.items()[0].0;
    assert_eq!(
        pieces,
        &vec!["01".to_string(), "0010".into(), "A".into()],
        "aufgefüllt, Index groß"
    );
    assert_eq!(
        item.kind.alternative,
        Some(Alternative {
            group: "12".into(),
            serial: "1".into()
        })
    );
    assert_eq!(item.kind.provisional, Some(Provisional::WithTotal));
    assert!(item.kind.lump_sum && item.kind.hourly && item.kind.markup && item.key_item);
    assert_eq!(item.short_text, "Nur Kurztext");
    assert!(item.long_text.blocks.is_empty());

    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><QtyTBD>Yes</QtyTBD><QU>m</QU></Item>
           <Item ID="i2" RNoPart="0020"><NotAppl>Yes</NotAppl><Qty>2</Qty><QU>St</QU></Item>
           <Item ID="i3" RNoPart="0030"><RefDescr>Rep</RefDescr><RefRNo IDRef="i1"/><Qty>2</Qty><QU>St</QU></Item>"#,
    );
    let items = doc.award.boq.items();
    assert!(items[0].1.qty_to_be_determined && items[0].1.qty.is_none());
    assert!(items[1].1.not_applicable);
    assert_eq!(items[2].1.kind.repetition_of.as_deref(), Some("01.0010"));
}

#[test]
fn reads_markup_item_and_perf_descr() {
    let doc = snippet(
        r#"<PerfDescr ID="p1"><PerfNo>7</PerfNo><PerfLbl>AS</PerfLbl><Description><CompleteText><DetailTxt><Text><p><span>Station</span></p></Text></DetailTxt></CompleteText></Description></PerfDescr>
           <Item ID="i1" RNoPart="0010"><MarkupIt>Yes</MarkupIt><RefPerfNo IDRef="p1"/><Qty>2</Qty><QU>St</QU><UP>50.00</UP><IT>100.00</IT></Item>
           <MarkupItem ID="m1" RNoPart="0020"><MarkupType>ListInSubQty</MarkupType>
             <MarkupSubQty><RefItem IDRef="i1"/><SubQty>1</SubQty></MarkupSubQty>
             <ITMarkup>50.00</ITMarkup><Markup>10.000000</Markup><IT>5.00</IT>
             <Description><OutlineText><OutlTxt><TextOutlTxt><span>Zuschlag</span></TextOutlTxt></OutlTxt></OutlineText></Description>
           </MarkupItem>"#,
    );
    let BoqNode::Category(title) = &doc.award.boq.body[0] else {
        panic!()
    };
    let BoqNode::PerfDescr(perf) = &title.body[0] else {
        panic!()
    };
    assert_eq!((perf.number.as_str(), perf.label.as_str()), ("7", "AS"));
    let items = doc.award.boq.items();
    assert_eq!(items[0].1.kind.reference_description.as_deref(), Some("7"));
    let markup = &items[1].1;
    assert!(xml::is_surcharge(markup));
    assert_eq!(markup.markup_percent, Some(dec("10")));
    assert_eq!(markup.markup_base, Some(dec("50")));
    assert_eq!(markup.total, Some(dec("5.00")));
    let surcharge = markup.surcharge.as_ref().unwrap();
    assert_eq!(surcharge.items[0].ordinal, "01.0010");
    assert_eq!(xml::computed_total(&doc.award.boq.body, &[]), dec("105.00"));
}

#[test]
fn reads_formatted_text_and_complements() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>1</Qty><QU>St</QU>
             <Description><CompleteText><ComplTSB>Yes</ComplTSB><DetailTxt>
               <Text>
                 <p style="font-weight:bold;">
                   <span>Fett aus p</span>
                   <span style="font-weight:normal">normal</span>
                 </p>
                 <p><span>a</span><br/><span style="font-style:italic;text-decoration:underline">b</span></p>
                 <ul><li><p><span>eins</span></p></li><li><span>zwei</span></li></ul>
                 <ol><li>drei</li></ol>
                 <table><tbody><tr><td><p><span>x</span></p></td><td>y</td></tr></tbody></table>
                 <p/>
               </Text>
               <TextComplement MarkLbl="3" Kind="Bidder"><ComplCaption><span>Fabrikat: </span></ComplCaption><ComplBody><span>'</span><br/><span>.........'</span></ComplBody></TextComplement>
               <TextComplement MarkLbl="4" Kind="Bidder"><ComplBody><span>'Siemens'</span></ComplBody></TextComplement>
               <Text><span>vom Bieter einzutragen</span></Text>
             </DetailTxt>
             <OutlineText><OutlTxt><TextOutlTxt><p><span>Kurz</span></p></TextOutlTxt></OutlTxt></OutlineText>
             </CompleteText></Description></Item>"#,
    );
    let item = only_item(&doc);
    let blocks = &item.long_text.blocks;
    assert_eq!(
        blocks[0],
        TextBlock::paragraph(vec![
            Run {
                text: "Fett aus p".into(),
                bold: true,
                ..Run::default()
            },
            run("normal"),
        ]),
        "Einrückung zwischen spans fällt weg, Stil vom p wird geerbt"
    );
    assert_eq!(
        blocks[1],
        TextBlock::paragraph(vec![
            run("a\n"),
            Run {
                text: "b".into(),
                italic: true,
                underline: true,
                ..Run::default()
            },
        ])
    );
    assert_eq!(
        blocks[2],
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![run("eins")],
            align: Default::default()
        }
    );
    assert_eq!(
        blocks[3],
        TextBlock::ListItem {
            ordered: false,
            level: 0,
            runs: vec![run("zwei")],
            align: Default::default()
        }
    );
    assert_eq!(
        blocks[4],
        TextBlock::ListItem {
            ordered: true,
            level: 0,
            runs: vec![run("drei")],
            align: Default::default()
        }
    );
    assert_eq!(
        blocks[5],
        TextBlock::Table(Table::from_texts(vec![vec!["x".into(), "y".into()]]))
    );
    assert_eq!(
        blocks[6],
        TextBlock::paragraph(vec![]),
        "leeres <p/> = Leerzeile"
    );
    assert_eq!(
        blocks[7],
        TextBlock::paragraph(vec![run("vom Bieter einzutragen")])
    );
    assert_eq!(blocks.len(), 8);
    assert_eq!(item.complements.len(), 2);
    assert_eq!(
        item.complements[0].caption, "Fabrikat: ",
        "Ansprache wie geschrieben (STLB-Bau Check vergleicht sie zeichengenau)"
    );
    assert_eq!(item.complements[0].body, "", "Punkte = leer");
    assert_eq!(item.complements[1].body, "Siemens", "ohne Hochkommas");
    assert_eq!(item.short_text, "Kurz");
}

#[test]
fn reads_header_namespace_free_and_windows_1252() {
    let text = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<g:GAEB xmlns:g=\"http://www.gaeb.de/GAEB_DA_XML/200407\"><g:GAEBInfo><g:Version>3.1</g:Version></g:GAEBInfo>\
<g:PrjInfo><g:NamePrj>Gr\u{fc}n</g:NamePrj></g:PrjInfo><g:Award><g:DP>82</g:DP>\
<g:AwardInfo><g:Cat>OpenCall</g:Cat><g:OpenDate>2026-01-02</g:OpenDate><g:OpenTime>11:00:00</g:OpenTime><g:EvalEnd>2026-02-01</g:EvalEnd><g:SubmLoc>Rathaus</g:SubmLoc></g:AwardInfo>\
<g:OWN><g:Address><g:Name1>Stadt</g:Name1><g:Street>S</g:Street><g:PCode>1</g:PCode><g:City>C</g:City></g:Address><g:AwardNo>V-1</g:AwardNo></g:OWN>\
<g:AddText><g:OutlineAddText><g:span>Vorbemerkung</g:span></g:OutlineAddText><g:DetailAddText><g:p><g:span>Text</g:span></g:p></g:DetailAddText></g:AddText>\
<g:BoQ ID=\"b\"><g:BoQInfo><g:Name>1</g:Name><g:BoQBkdn><g:Type>Item</g:Type><g:Length>4</g:Length><g:Num>Yes</g:Num></g:BoQBkdn>\
<g:Totals><g:Total>10.00</g:Total></g:Totals></g:BoQInfo>\
<g:BoQBody><g:Itemlist><g:Item ID=\"i\" RNoPart=\"1\"><g:Qty>2</g:Qty><g:QU>m</g:QU><g:UP>5</g:UP><g:IT>10.00</g:IT></g:Item></g:Itemlist></g:BoQBody></g:BoQ></g:Award></g:GAEB>";
    let bytes: Vec<u8> = text.chars().map(|ch| ch as u32 as u8).collect();
    let (doc, report) = xml::read_with_report(&bytes).unwrap();
    assert_eq!(report.version, "3.1");
    assert_eq!(doc.phase, Phase::Dp82);
    assert_eq!(doc.project.name, "Grün");
    assert_eq!(doc.award.category, "OpenCall");
    assert_eq!(doc.award.award_no, "V-1");
    assert_eq!(
        doc.award.submission.award_deadline.as_deref(),
        Some("2026-02-01")
    );
    assert_eq!(doc.award.owner.as_ref().unwrap().name1, "Stadt");
    assert_eq!(doc.award.add_texts[0].outline, "Vorbemerkung");
    assert_eq!(doc.award.boq.items()[0].0, vec!["0001".to_string()]);
    assert_eq!(xml::computed_total(&doc.award.boq.body, &[]), dec("10.00"));
}

#[test]
fn errors_are_german_and_specific() {
    let error = xml::read(b"<GAEB><Award>").unwrap_err();
    assert!(matches!(error, GaebXmlError::Syntax { .. }), "{error}");
    let error = xml::read(b"<Root/>").unwrap_err();
    assert!(error.to_string().contains("<Root>"), "{error}");
    let error = xml::read(b"<GAEB><GAEBInfo/></GAEB>").unwrap_err();
    assert!(error.to_string().contains("Mengenermittlung"), "{error}");
    let error = xml::read(b"<GAEB><Award><DP>89</DP></Award></GAEB>").unwrap_err();
    assert!(matches!(error, GaebXmlError::UnsupportedPhase(ref phase) if phase == "89"));

    let mut doc = sample();
    doc.award.boq.breakdown.clear();
    assert!(matches!(
        xml::write(&doc, Phase::Dp83),
        Err(GaebXmlError::Structure(_))
    ));
    let mut doc = sample();
    doc.award
        .boq
        .body
        .push(BoqNode::Item(item("0010", "1", "St", "1", "lose")));
    let error = xml::write(&doc, Phase::Dp83).unwrap_err();
    assert!(
        error.to_string().contains("nur Titel oder nur Positionen"),
        "{error}"
    );
}

#[test]
fn writer_output_follows_the_conventions() {
    let text = xml::write(&sample(), Phase::Dp82).unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(text.contains("xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA82/3.3\""));
    assert!(text.contains("<ProgName>proAVA</ProgName>"));
    assert!(text.contains("<Qty>12.500</Qty>"));
    assert!(text.contains("<UP>10.255</UP>"));
    assert!(text.contains("<UP>1500.00</UP>"));
    assert!(text.contains("<IT>128.19</IT>"));
    assert!(text.contains("<ComplTSA>Yes</ComplTSA>"));
    assert!(text.contains("<ComplTSB>Yes</ComplTSB>"));
    assert!(text.contains("<span style=\"font-weight:bold;\">fett</span>"));
    assert!(text.contains("<RefDescr>Ref</RefDescr>"));
    assert!(text.contains("<RefDescr>Rep</RefDescr>"));
    let x83 = xml::write(&sample(), Phase::Dp83).unwrap();
    assert!(!x83.contains("<UP>") && !x83.contains("<Totals>") && !x83.contains("<IT>"));
    assert!(x83.contains("<QtyTBD>Yes</QtyTBD>"));
}

/// Validates the written sample in all four phases against the official
/// GAEB 3.3 XSDs. Needs the XSDs (`proava_gaeb::schemas`, see docs/BUILD.md) and
/// xmllint or python3-lxml.
#[test]
#[ignore]
fn written_files_match_the_gaeb_schema() {
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
        .prefix("proava-gaeb-xsd-")
        .tempdir()
        .unwrap();
    let dir = scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let mut bid = sample();
    bid.award.bidder = Some(Address {
        name1: "Bieter".into(),
        ..Address::default()
    });
    let options = xml::WriteOptions {
        include_images: true,
    };
    let mut lv = sample();
    lv.award.boq.body[1] = wahl_title();
    // Nachtrag: CONo/COStatus an LV und Position.
    lv.award.boq.change_order = Some(ChangeOrderTag {
        number: 3,
        status: "Offered".into(),
    });
    if let BoqNode::Category(title) = &mut lv.award.boq.body[0] {
        if let BoqNode::Category(sub) = &mut title.body[0] {
            if let BoqNode::Item(item) = &mut sub.body[0] {
                item.change_order = Some(ChangeOrderTag {
                    number: 3,
                    status: "Rejected".into(),
                });
            }
        }
    }
    if let BoqNode::Category(title) = &mut lv.award.boq.body[0] {
        if let BoqNode::Category(sub) = &mut title.body[0] {
            if let BoqNode::Item(item) = &mut sub.body[2] {
                item.long_text.blocks.push(TextBlock::Image {
                    file_name: "x.png".into(),
                    data: tiny_png(),
                    width_mm: Some(40.0),
                });
            }
        }
    }
    for phase in [
        Phase::Dp81,
        Phase::Dp82,
        Phase::Dp83,
        Phase::Dp84,
        Phase::Dp86,
    ] {
        let path = dir.join(format!("sample.{}", xml::file_extension(phase)));
        xml::write_file(&bid, phase, &path).unwrap();
        let path = dir.join(format!("lv.{}", xml::file_extension(phase)));
        xml::write_file_with(&lv, phase, &path, &options).unwrap();
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

/// All GAEB DA XML files below `PROAVA_GAEB_TESTDATA` (several folders
/// separated by ':') are read without error and survive writing them again
/// in their phase. Files are only read.
#[test]
#[ignore]
fn real_files_read_and_roundtrip() {
    let Ok(roots) = std::env::var("PROAVA_GAEB_TESTDATA") else {
        panic!("PROAVA_GAEB_TESTDATA ist nicht gesetzt");
    };
    fn collect(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap().flatten() {
                collect(&entry.path(), out);
            }
        } else if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            let extension = extension.to_ascii_lowercase();
            if extension.len() == 3
                && extension.starts_with("x8")
                && ('1'..='6').contains(&extension.chars().nth(2).unwrap())
            {
                out.push(path.to_path_buf());
            }
        }
    }
    let mut files = Vec::new();
    for root in roots.split(':').filter(|root| !root.is_empty()) {
        collect(std::path::Path::new(root), &mut files);
    }
    assert!(!files.is_empty());
    let mut failures = Vec::new();
    for file in &files {
        match xml::read_file(file) {
            Ok(doc) => {
                let again = xml::read(xml::write(&doc, doc.phase).unwrap().as_bytes()).unwrap();
                if again != xml::as_written(&doc, doc.phase) {
                    failures.push(format!("Roundtrip: {}", file.display()));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", file.display())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} von {} Dateien:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}

fn tiny_png() -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
    data.extend_from_slice(&[
        0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0,
    ]);
    data
}

/// Title 02 as Wahlbereich of group 1 (ALNBGroupNo/ALNBSerNo).
fn wahl_title() -> BoqNode {
    let mut doc = sample();
    let BoqNode::Category(mut title) = doc.award.boq.body.remove(1) else {
        unreachable!()
    };
    title.alternative = Some(Alternative {
        group: "1".into(),
        serial: "1".into(),
    });
    BoqNode::Category(title)
}

#[test]
fn wahl_title_is_kept_and_not_counted() {
    let mut doc = sample();
    let full = xml::computed_total(&doc.award.boq.body, &[]);
    doc.award.boq.body[1] = wahl_title();
    assert_eq!(
        xml::computed_total(&doc.award.boq.body, &[]),
        full - dec("88.20"),
        "Wahlbereich zählt nicht"
    );
    let text = xml::write(&doc, Phase::Dp82).unwrap();
    assert!(text.contains("<ALNBGroupNo>1</ALNBGroupNo>"));
    let again = xml::read(text.as_bytes()).unwrap();
    assert_eq!(again, xml::as_written(&doc, Phase::Dp82));
    let BoqNode::Category(title) = &again.award.boq.body[1] else {
        panic!()
    };
    assert!(xml::is_alternative_area(title));
    assert_eq!(
        again.award.boq.totals.unwrap().total,
        Some(dec("2680.62")),
        "LV-Summe ohne Wahl-Titel"
    );
    assert!(!xml::write(&doc, Phase::Dp84).unwrap().contains("ALNB"));
}

#[test]
fn sub_description_numbers_stay_as_written() {
    let mut doc = sample();
    if let BoqNode::Category(title) = &mut doc.award.boq.body[0] {
        if let BoqNode::Category(sub) = &mut title.body[0] {
            if let BoqNode::Item(item) = &mut sub.body[12] {
                item.sub_descriptions[0].number = "01".into();
            }
        }
    }
    let again = roundtrip(&doc, Phase::Dp83);
    assert_eq!(again, xml::as_written(&doc, Phase::Dp83));
    let leader = again
        .award
        .boq
        .items()
        .into_iter()
        .find(|(_, item)| !item.sub_descriptions.is_empty())
        .unwrap()
        .1
        .clone();
    assert_eq!(leader.sub_descriptions[0].number, "01");
}

#[test]
fn x86_has_owner_and_contractor_and_no_tender_data() {
    let doc = sample();
    let text = xml::write(&doc, Phase::Dp86).unwrap();
    for absent in [
        "<Cat>",
        "<OpenDate>",
        "<OpenTime>",
        "<EvalEnd>",
        "<SubmLoc>",
    ] {
        assert!(!text.contains(absent), "{absent}");
    }
    assert!(text.contains("<OWN>") && text.contains("<CTR>"));
    assert!(text.contains("<CnstStart>2027-01-04</CnstStart>"));
    assert_eq!(
        roundtrip(&doc, Phase::Dp86),
        xml::as_written(&doc, Phase::Dp86)
    );
}

#[test]
fn surcharge_without_type_counts_with_its_file_total() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>2</Qty><QU>St</QU><UP>50.00</UP><IT>100.00</IT></Item>
           <MarkupItem ID="m1" RNoPart="0020"><ITMarkup>80.00</ITMarkup><Markup>10.000000</Markup><IT>8.00</IT></MarkupItem>"#,
    );
    assert_eq!(xml::computed_total(&doc.award.boq.body, &[]), dec("108.00"));
}

#[test]
fn images_are_read_from_paragraphs_and_cells() {
    let png = xml_base64(&tiny_png());
    let doc = snippet(&format!(
        r#"<Item ID="i1" RNoPart="0010"><Qty>1</Qty><QU>St</QU><Description><CompleteText><DetailTxt><Text>
             <p><span>vor</span><image Type="image/png" width="96">{png}</image><span>nach</span></p>
             <p><image Type="image/jpeg" width="5cm" Name="C:\Bilder\Plan 1.png">{png}</image></p>
             <table><tr><td><span>Zelle</span><image Type="image/png">{png}</image></td></tr></table>
             <p><image Type="image/png">kein base64 !</image></p>
           </Text></DetailTxt></CompleteText></Description></Item>"#
    ));
    let (doc, report) = xml::read_with_report(
        xml::write_with(&doc, Phase::Dp83, &xml::WriteOptions::default())
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    assert!(report.warnings.is_empty());
    let item = only_item(&doc);
    assert!(
        !item
            .long_text
            .blocks
            .iter()
            .any(|block| matches!(block, TextBlock::Image { .. })),
        "Standard: Bilder werden nicht geschrieben"
    );

    let raw = format!(
        "{SNIPPET_HEAD}<Item ID=\"i1\" RNoPart=\"0010\"><Qty>1</Qty><QU>St</QU><Description><CompleteText><DetailTxt><Text>\
         <p><span>vor</span><image Type=\"image/png\" width=\"96\">{png}</image><span>nach</span></p>\
         <p><image Type=\"image/jpeg\" width=\"5cm\" Name=\"C:\\Bilder\\Plan 1.png\">{png}</image></p>\
         <table><tr><td><span>Zelle</span><image Type=\"image/png\">{png}</image></td></tr></table>\
         <p><image Type=\"image/png\">kein base64 !</image></p>\
         </Text></DetailTxt></CompleteText></Description></Item>{SNIPPET_TAIL}"
    );
    let (doc, report) = xml::read_with_report(raw.as_bytes()).unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("Bild"));
    let blocks = only_item(&doc).long_text.blocks;
    assert_eq!(blocks[0], TextBlock::paragraph(vec![run("vor")]));
    let TextBlock::Image {
        file_name,
        data,
        width_mm,
    } = &blocks[1]
    else {
        panic!("{blocks:?}")
    };
    assert_eq!(data, &tiny_png());
    assert!(file_name.starts_with("bild_") && file_name.ends_with(".png"));
    assert!((width_mm.unwrap() - 25.4).abs() < 0.01, "96 px = 1 Zoll");
    assert_eq!(blocks[2], TextBlock::paragraph(vec![run("nach")]));
    let TextBlock::Image {
        file_name,
        width_mm,
        ..
    } = &blocks[3]
    else {
        panic!()
    };
    assert_eq!(file_name, "Plan_1.png");
    assert_eq!(*width_mm, Some(50.0));
    assert_eq!(
        blocks[4],
        TextBlock::Table(Table::from_texts(vec![vec!["Zelle".into()]]))
    );
    assert!(
        matches!(blocks[5], TextBlock::Image { .. }),
        "Bild aus der Zelle folgt der Tabelle"
    );
    assert_eq!(blocks.len(), 6);
}

fn xml_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ALPHABET[(v >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

#[test]
fn lump_sum_replaces_the_title_total() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>2</Qty><QU>St</QU><UP>50.00</UP><IT>100.00</IT></Item>"#,
    );
    let mut boq = doc.award.boq.clone();
    let BoqNode::Category(title) = &mut boq.body[0] else {
        panic!()
    };
    title.totals = Some(Totals {
        total: Some(dec("100.00")),
        lump_sum: Some(dec("90.00")),
        ..Totals::default()
    });
    assert_eq!(xml::computed_total(&boq.body, &[]), dec("90.00"));
}

fn item_by_rno(doc: &GaebDocument, rno: &str) -> Item {
    doc.award
        .boq
        .items()
        .into_iter()
        .find(|(_, item)| item.rno_part == rno)
        .map(|(_, item)| item.clone())
        .unwrap_or_else(|| panic!("Position {rno} fehlt"))
}

#[test]
fn stlb_keys_survive_every_phase_with_texts() {
    let doc = sample();
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
        let with = item_by_rno(&again, "0040");
        assert_eq!(
            with.stlb,
            Some(stlb_reference()),
            "{phase:?}: Reihenfolge und Werte"
        );
        assert_eq!(with.standard_number, None);
        let base = item_by_rno(&again, "0050");
        assert_eq!(
            base.standard_number,
            Some(StandardNumber {
                number: "12345678901234567".into(),
                catalog: "StLB".into(),
            })
        );
        let normal = item_by_rno(&again, "0010");
        let owner = normal
            .complements
            .iter()
            .find(|complement| complement.kind == "Owner")
            .unwrap();
        assert_eq!(owner.stlb_characteristic, Some(910));
    }
}

#[test]
fn x84_carries_no_stlb_keys_but_the_complement_link() {
    let mut doc = sample();
    if let BoqNode::Category(title) = &mut doc.award.boq.body[0] {
        if let BoqNode::Category(sub) = &mut title.body[0] {
            if let BoqNode::Item(item) = &mut sub.body[2] {
                item.complements[0].stlb_characteristic = Some(920);
            }
        }
    }
    let text = xml::write(&doc, Phase::Dp84).unwrap();
    assert!(!text.contains("STLBBau") && !text.contains("StLNo"));
    assert!(text.contains(r#"<TextComplement MarkLbl="1" Kind="Bidder" ArtChrIdent="920">"#));
    let again = xml::read(text.as_bytes()).unwrap();
    assert_eq!(again, xml::as_written(&doc, Phase::Dp84));
    assert_eq!(item_by_rno(&again, "0040").stlb, None);
}

#[test]
fn stlb_key_is_written_in_schema_order() {
    let text = xml::write(&sample(), Phase::Dp83).unwrap();
    let start = text.find("<STLBBau>").expect("STLBBau fehlt");
    let block = &text[start..text.find("</STLBBau>").unwrap()];
    let order = [
        "<CtlgName>STLB-Bau</CtlgName>",
        "<VersDate>2026-04</VersDate>",
        "<WCtg>070</WCtg>",
        "<Group>901</Group>",
        "<CostGrp>481</CostGrp>",
        "</STLBBauCtlg>",
        "<STLBBauID><ArtChrIdent>900</ArtChrIdent><ArtChIdx>0</ArtChIdx><ChVIdent>3</ChVIdent><OutlTextPart>Yes</OutlTextPart></STLBBauID>",
        "<STLBBauID><ArtChrIdent>910</ArtChrIdent></STLBBauID>",
        "<STLBBauID><ArtChrIdent>905</ArtChrIdent>",
    ];
    let mut at = 0;
    for part in order {
        let found = block[at..]
            .find(part)
            .unwrap_or_else(|| panic!("{part} fehlt oder steht falsch"));
        at += found + part.len();
    }
    // STLBBau comes first in Description, before the texts.
    let description = text[..start].rfind("<Description>").unwrap();
    assert!(text[description..start].trim() == "<Description>");
    assert!(text[start..].find("<CompleteText>").is_some());
    assert!(text.contains(r#"<StLNo Type="StLB">12345678901234567</StLNo>"#));
}

#[test]
fn reads_stlb_keys_from_foreign_files() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>2.000</Qty><QU>m</QU>
             <Description>
               <STLBBau>
                 <STLBBauCtlg><CtlgName> STLB-Bau </CtlgName><VersDate>2025-10</VersDate><WCtg>053</WCtg><Group>+0042</Group></STLBBauCtlg>
                 <STLBBauID><ArtChrIdent>7</ArtChrIdent><ArtChIdx>0</ArtChIdx><ChVIdent>1</ChVIdent></STLBBauID>
                 <STLBBauID><ArtChrIdent>acht</ArtChrIdent></STLBBauID>
                 <STLBBauID><ArtChrIdent>9</ArtChrIdent><OutlTextPart>Yes</OutlTextPart></STLBBauID>
               </STLBBau>
               <CompleteText><DetailTxt><Text><p><span>Kabel </span></p>
                 <TextComplement MarkLbl="1" Kind="Owner" ArtChrIdent="9"><ComplBody><span>NYM-J</span></ComplBody></TextComplement>
               </Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Kabel</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText>
             </Description></Item>
           <Item ID="i2" RNoPart="0020"><Qty>1.000</Qty><QU>St</QU>
             <Description>
               <STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName><Group>x</Group></STLBBauCtlg></STLBBau>
               <OutlineText><OutlTxt><TextOutlTxt><span>Ohne gültige Gruppe</span></TextOutlTxt></OutlTxt></OutlineText>
             </Description></Item>
           <Item ID="i3" RNoPart="0030"><Qty>1.000</Qty><QU>St</QU>
             <Description><StLNo>071014121100</StLNo>
               <OutlineText><OutlTxt><TextOutlTxt><span>Alte StLB-Nummer</span></TextOutlTxt></OutlTxt></OutlineText>
             </Description></Item>"#,
    );
    let (_, report) =
        xml::read_with_report(xml::write(&doc, Phase::Dp83).unwrap().as_bytes()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let first = item_by_rno(&doc, "0010");
    assert_eq!(
        first.stlb,
        Some(StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some("2025-10".into()),
            work_category: "053".into(),
            group: 42,
            cost_group: String::new(),
            characteristics: vec![
                StlbCharacteristic {
                    characteristic: 7,
                    index: Some(0),
                    value: Some(1),
                    short_text_part: false,
                },
                StlbCharacteristic {
                    characteristic: 9,
                    index: None,
                    value: None,
                    short_text_part: true,
                },
            ],
        })
    );
    assert_eq!(first.complements[0].stlb_characteristic, Some(9));
    assert_eq!(item_by_rno(&doc, "0020").stlb, None);
    assert_eq!(
        item_by_rno(&doc, "0030").standard_number,
        Some(StandardNumber {
            number: "071014121100".into(),
            catalog: String::new(),
        })
    );
}

/// Hinweistext aus STLB-Bau (Standardbeschreibung, „STLB-Bau Hinweis“): Kennung
/// und Textergänzungen bleiben – vorher fielen beide beim Lesen weg, die Lücke
/// wurde zur Leerzeile (Befund aus einem echten LV, hier ausgedacht).
#[test]
fn remarks_keep_stlb_key_and_text_complements() {
    let doc = snippet(
        r#"<Remark ID="r1"><Description>
             <STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName><VersDate>2026-04</VersDate><WCtg>070</WCtg><Group>9201</Group></STLBBauCtlg>
               <STLBBauID><ArtChrIdent>92011</ArtChrIdent><ArtChIdx>0</ArtChIdx><ChVIdent>2</ChVIdent></STLBBauID>
               <STLBBauID><ArtChrIdent>-92012</ArtChrIdent></STLBBauID></STLBBau>
             <CompleteText><ComplTSA>Yes</ComplTSA><DetailTxt>
               <Text><span>Datenverarbeitungseinrichtung für Managementfunktionen,</span><br/><span/></Text>
               <TextComplement MarkLbl="11" Kind="Owner" ArtChrIdent="-92012"><ComplCaption><span>Anzahl Bedienplätze </span></ComplCaption><ComplBodyInt Value="2"/><ComplBody><span>'2'</span></ComplBody><ComplTail/></TextComplement>
               <Text><span/><br/><span>für Einbau in Schaltschrank.</span></Text>
             </DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Managementebene</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText>
           </Description></Remark>
           <Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description><OutlineText><OutlTxt><TextOutlTxt><span>Danach</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>"#,
    );
    let remark = doc
        .award
        .boq
        .body
        .iter()
        .find_map(|node| match node {
            BoqNode::Remark(remark) => Some(remark.clone()),
            BoqNode::Category(category) => category.body.iter().find_map(|node| match node {
                BoqNode::Remark(remark) => Some(remark.clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("Hinweistext");
    assert_eq!(remark.short_text, "Managementebene");
    assert_eq!(remark.stlb.as_ref().map(|stlb| stlb.group), Some(9201));
    let [complement] = &remark.complements[..] else {
        panic!("eine Textergänzung: {:?}", remark.complements);
    };
    assert_eq!(complement.caption, "Anzahl Bedienplätze ");
    assert_eq!(complement.body, "2");
    assert_eq!(complement.number, Some(ComplementNumber::Integer));
    assert_eq!(complement.stlb_characteristic, Some(-92012));
    assert!(complement.anchor.is_some(), "Platz im Text");
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let text = xml::write(&doc, phase).unwrap();
        assert!(text.contains("<Remark"), "{phase:?}");
        assert!(
            text.contains(r#"<Group>9201</Group>"#)
                && text.contains(r#"<TextComplement MarkLbl="11" Kind="Owner" ArtChrIdent="-92012"><ComplCaption><span>Anzahl Bedienplätze </span></ComplCaption><ComplBodyInt Value="2"/>"#),
            "{phase:?}\n{text}"
        );
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
    }
    assert!(!xml::write(&doc, Phase::Dp84).unwrap().contains("<Remark"));
}

/// Ausführungsbeschreibung mit Textergänzung (z. B. eine umgewandelte
/// Position oder eine Standardbeschreibung): GAEB erlaubt sie in PerfDescr,
/// sie bleibt beim Lesen und Schreiben an ihrem Platz (ausgedachte Werte).
#[test]
fn performance_descriptions_keep_text_complements() {
    let doc = snippet(
        r#"<PerfDescr ID="p1"><PerfNo>1</PerfNo><PerfLbl>Feldgeräte</PerfLbl><Description>
             <CompleteText><DetailTxt>
               <Text><span>Alle Feldgeräte montieren,</span><br/></Text>
               <TextComplement MarkLbl="1" Kind="Bidder"><ComplCaption><span>Fabrikat </span></ComplCaption><ComplBody><span>'....................................................'</span></ComplBody><ComplTail><br/><span>vom Bieter einzutragen</span></ComplTail></TextComplement>
               <Text><span>.</span></Text>
             </DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Feldgeräte</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText>
           </Description></PerfDescr>
           <Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description><OutlineText><OutlTxt><TextOutlTxt><span>Danach</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>"#,
    );
    let performance = doc
        .award
        .boq
        .body
        .iter()
        .find_map(|node| match node {
            BoqNode::PerfDescr(description) => Some(description.clone()),
            BoqNode::Category(category) => category.body.iter().find_map(|node| match node {
                BoqNode::PerfDescr(description) => Some(description.clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("Ausführungsbeschreibung");
    let [complement] = &performance.complements[..] else {
        panic!("eine Textergänzung: {:?}", performance.complements);
    };
    assert_eq!(complement.caption, "Fabrikat ");
    assert_eq!(complement.tail, "\nvom Bieter einzutragen");
    assert!(complement.anchor.is_some(), "Platz im Text");
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let text = xml::write(&doc, phase).unwrap();
        assert!(
            text.contains(r#"<TextComplement MarkLbl="1" Kind="Bidder"><ComplCaption><span>Fabrikat </span></ComplCaption>"#),
            "{phase:?}\n{text}"
        );
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
    }
}

/// Textergänzungen so, wie STLB-Bau sie liefert (Form wie in der GAEB-
/// Fachdokumentation 3.3 und den BVBS-Prüfdateien, Werte ausgedacht): Ansprache
/// mit Leerzeichen am Ende, Wert zwischen Hochkommas, Zahlenwert in
/// ComplBodyDec/ComplBodyInt, Nachsatz mit Zeilenumbruch. STLB-Bau Check
/// prüft genau das („Textergänzung – Änderung“, „Numerisch unvollständig“).
#[test]
fn stlb_text_complements_keep_their_exact_form() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>6.000</Qty><QU>St</QU>
             <Description>
               <STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName><VersDate>2026-04</VersDate><WCtg>061</WCtg><Group>9001</Group></STLBBauCtlg>
                 <STLBBauID><ArtChrIdent>9101</ArtChrIdent><ArtChIdx>0</ArtChIdx><ChVIdent>3</ChVIdent></STLBBauID>
                 <STLBBauID><ArtChrIdent>-9102</ArtChrIdent></STLBBauID>
                 <STLBBauID><ArtChrIdent>9103</ArtChrIdent></STLBBauID>
                 <STLBBauID><ArtChrIdent>9104</ArtChrIdent></STLBBauID>
               </STLBBau>
               <CompleteText><ComplTSA>Yes</ComplTSA><ComplTSB>Yes</ComplTSB><DetailTxt>
                 <Text><span style="font-family:Arial;font-size:10pt;">Datenkabel, Kategorie 7,</span><br/><span/></Text>
                 <TextComplement MarkLbl="11" Kind="Owner" ArtChrIdent="-9102"><ComplCaption><span style="font-family:Arial;font-size:10pt;">Länge Kabel </span></ComplCaption><ComplBodyDec Value="1.5"/><ComplBody><span>&#x27;1.5&#x27;</span></ComplBody><ComplTail><span> m, </span></ComplTail></TextComplement>
                 <TextComplement MarkLbl="12" Kind="Owner" ArtChrIdent="9103"><ComplCaption><span>Anzahl Adern </span></ComplCaption><ComplBodyInt Value="8"/><ComplBody><span>'8'</span></ComplBody><ComplTail/></TextComplement>
                 <TextComplement MarkLbl="51" Kind="Bidder" Empty="Yes" ArtChrIdent="9104"><ComplCaption><span>Erzeugnis </span></ComplCaption><ComplBody><span>'</span><br/><span>..........'</span></ComplBody><ComplTail><br/><span>Vom Bieter einzutragen</span><br/></ComplTail></TextComplement>
               </DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Datenkabel Kat.7</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText>
             </Description></Item>"#,
    );
    let item = item_by_rno(&doc, "0010");
    let [length, cores, product] = &item.complements[..] else {
        panic!("drei Textergänzungen: {:?}", item.complements);
    };
    assert_eq!(length.caption, "Länge Kabel ");
    assert_eq!(length.body, "1.5");
    assert_eq!(length.tail, " m, ");
    assert_eq!(length.number, Some(ComplementNumber::Decimal));
    assert_eq!(length.stlb_characteristic, Some(-9102));
    assert_eq!(cores.number, Some(ComplementNumber::Integer));
    assert_eq!(cores.tail, "", "leeres ComplTail");
    assert_eq!(product.body, "", "Punkte = leer");
    assert_eq!(
        product.tail, "\nVom Bieter einzutragen\n",
        "Zeilenumbrüche bleiben"
    );
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let text = xml::write(&doc, phase).unwrap();
        for part in [
            r#"<TextComplement MarkLbl="11" Kind="Owner" ArtChrIdent="-9102"><ComplCaption><span>Länge Kabel </span></ComplCaption><ComplBodyDec Value="1.5"/><ComplBody><span>'1.5'</span></ComplBody><ComplTail><span> m, </span></ComplTail></TextComplement>"#,
            r#"<ComplBodyInt Value="8"/><ComplBody><span>'8'</span></ComplBody>"#,
            // Ohne Empty="Yes", Lücke mit 52 Punkten – wie STLB-Bau und andere Programme.
            r#"<TextComplement MarkLbl="51" Kind="Bidder" ArtChrIdent="9104"><ComplCaption><span>Erzeugnis </span></ComplCaption><ComplBody><span>'....................................................'</span></ComplBody><ComplTail><br/><span>Vom Bieter einzutragen</span><br/></ComplTail>"#,
        ] {
            assert!(text.contains(part), "{phase:?}: {part}\n{text}");
        }
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
    }
    // Ein geänderter Wert gibt den neuen Zahlenwert; Unbrauchbares keinen.
    let text = xml::write(&doc, Phase::Dp83)
        .unwrap()
        .replace("'1.5'", "'2,25'")
        .replace("'8'", "'acht'");
    let changed = xml::read(text.as_bytes()).unwrap();
    let text = xml::write(&changed, Phase::Dp83).unwrap();
    assert!(text.contains(r#"<ComplBodyDec Value="2.25"/><ComplBody><span>'2,25'</span>"#));
    assert!(
        !text.contains("<ComplBodyInt"),
        "kein Zahlenwert aus „acht“"
    );
}

#[test]
fn complement_numbers_from_body_text() {
    let dec = ComplementNumber::Decimal;
    let int = ComplementNumber::Integer;
    assert_eq!(dec.value_of("1.5").as_deref(), Some("1.5"));
    assert_eq!(dec.value_of(" 1,5 ").as_deref(), Some("1.5"));
    assert_eq!(dec.value_of("1.000,25").as_deref(), Some("1000.25"));
    assert_eq!(dec.value_of("-3").as_deref(), Some("-3"));
    assert_eq!(dec.value_of("12.").as_deref(), None);
    assert_eq!(dec.value_of("").as_deref(), None);
    assert_eq!(dec.value_of("ca. 3").as_deref(), None);
    assert_eq!(int.value_of("220").as_deref(), Some("220"));
    assert_eq!(int.value_of("2,5").as_deref(), None);
}

#[test]
fn reading_reports_unusable_stlb_parts() {
    let text = format!(
        "{SNIPPET_HEAD}{}{SNIPPET_TAIL}",
        r#"<Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description>
             <STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName><Group>5</Group></STLBBauCtlg>
               <STLBBauID><ArtChrIdent>acht</ArtChrIdent></STLBBauID></STLBBau>
             <OutlineText><OutlTxt><TextOutlTxt><span>A</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>
           <Item ID="i2" RNoPart="0020"><Qty>1.000</Qty><QU>St</QU><Description>
             <STLBBau><STLBBauCtlg><CtlgName>STLB-Bau</CtlgName></STLBBauCtlg></STLBBau>
             <OutlineText><OutlTxt><TextOutlTxt><span>B</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>"#
    );
    let (doc, report) = xml::read_with_report(text.as_bytes()).unwrap();
    assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("01.0010") && report.warnings[0].contains("ArtChrIdent"));
    assert!(report.warnings[1].contains("01.0020") && report.warnings[1].contains("Group"));
    assert_eq!(
        item_by_rno(&doc, "0010").stlb.unwrap().characteristics,
        vec![]
    );
}

#[test]
fn stlb_key_wins_over_standard_number_and_odd_values_are_dropped() {
    let mut doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU>
             <Description><OutlineText><OutlTxt><TextOutlTxt><span>A</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>
           <Item ID="i2" RNoPart="0020"><Qty>1.000</Qty><QU>St</QU>
             <Description><OutlineText><OutlTxt><TextOutlTxt><span>B</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>"#,
    );
    if let BoqNode::Category(title) = &mut doc.award.boq.body[0] {
        if let BoqNode::Item(item) = &mut title.body[0] {
            item.stlb = Some(StlbReference {
                catalog: "STLB-Bau Dynamische BauDaten".into(),
                version: Some("04/2026".into()),
                work_category: "0700".into(),
                group: 1,
                ..StlbReference::default()
            });
            item.standard_number = Some(StandardNumber {
                number: "1".into(),
                catalog: "stlb".into(),
            });
        }
        if let BoqNode::Item(item) = &mut title.body[1] {
            item.standard_number = Some(StandardNumber {
                number: "12-34".into(),
                catalog: "StLB".into(),
            });
        }
    }
    let again = roundtrip(&doc, Phase::Dp83);
    assert_eq!(again, xml::as_written(&doc, Phase::Dp83));
    let first = item_by_rno(&again, "0010");
    let stlb = first.stlb.unwrap();
    assert_eq!(stlb.catalog, "STLB-Bau D");
    assert_eq!(stlb.version, None);
    assert_eq!(stlb.work_category, "070");
    assert_eq!(first.standard_number, None);
    assert_eq!(item_by_rno(&again, "0020").standard_number, None);
}

#[test]
fn keys_of_sub_and_performance_descriptions_survive() {
    let doc = sample();
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
        let leader = item_by_rno(&again, "0110");
        assert_eq!(
            leader.sub_descriptions[0]
                .standard_number
                .as_ref()
                .map(|n| n.number.as_str()),
            Some("9300201123000202")
        );
        let BoqNode::Category(title) = &again.award.boq.body[0] else {
            panic!()
        };
        let BoqNode::Category(sub) = &title.body[0] else {
            panic!()
        };
        let perf = sub
            .body
            .iter()
            .find_map(|node| match node {
                BoqNode::PerfDescr(perf) => Some(perf),
                _ => None,
            })
            .unwrap();
        assert_eq!(perf.stlb, Some(stlb_reference()), "{phase:?}");
    }
    assert_eq!(
        xml::write(&doc, Phase::Dp83)
            .unwrap()
            .matches("<STLBBau>")
            .count(),
        2
    );
}

fn first_item(doc: &mut GaebDocument) -> &mut Item {
    fn find(nodes: &mut [BoqNode]) -> Option<&mut Item> {
        for node in nodes {
            match node {
                BoqNode::Item(item) => return Some(item),
                BoqNode::Category(category) => {
                    if let Some(item) = find(&mut category.body) {
                        return Some(item);
                    }
                }
                _ => {}
            }
        }
        None
    }
    find(&mut doc.award.boq.body).unwrap()
}

fn anchor(block: usize, offset: usize, own_line: bool) -> Option<TextAnchor> {
    Some(TextAnchor {
        block,
        offset,
        own_line,
    })
}

/// Text complements are written at their anchors and read back there.
#[test]
fn complements_are_written_at_their_place() {
    let cases: [[Option<TextAnchor>; 2]; 6] = [
        // „Normal |fett …“ und eine eigene Zeile vor der Liste.
        [anchor(0, 7, false), anchor(2, 0, true)],
        // Zwei Stellen im selben Absatz, die zweite am Absatzende.
        [anchor(0, 0, false), anchor(0, 999, false)],
        // Absatz mit Ausrichtung, eine ohne Platz.
        [anchor(7, 5, false), None],
        // Listenpunkt und Tabelle: eigene Zeile davor; hinter dem Ende.
        [anchor(3, 2, false), anchor(42, 0, false)],
        // Zwei Absätze nacheinander mit Ergänzung (werden ein Absatz).
        [anchor(6, 3, false), anchor(7, 3, false)],
        // Vor dem leeren Absatz und vor der Tabelle.
        [anchor(1, 0, true), anchor(5, 0, true)],
    ];
    for (number, anchors) in cases.iter().enumerate() {
        let mut doc = sample();
        let item = first_item(&mut doc);
        for (complement, anchor) in item.complements.iter_mut().zip(anchors) {
            complement.anchor = *anchor;
        }
        for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
            let again = roundtrip(&doc, phase);
            assert_eq!(
                again,
                xml::as_written(&doc, phase),
                "Fall {number}, {phase:?}"
            );
        }
    }
    // Die üblichen Fälle kommen unverändert zurück.
    let mut doc = sample();
    let item = first_item(&mut doc);
    item.complements[0].anchor = anchor(0, 7, false);
    item.complements[1].anchor = anchor(2, 0, true);
    let expected = item.clone();
    let mut again = roundtrip(&doc, Phase::Dp83);
    let read = first_item(&mut again);
    assert_eq!(read.long_text, expected.long_text);
    assert_eq!(read.complements[0].anchor, anchor(0, 7, false));
    assert_eq!(read.complements[1].anchor, anchor(2, 0, true));
    let written = xml::write(&doc, Phase::Dp83).unwrap();
    assert!(
        written.contains("<Text><span>Normal </span></Text><TextComplement MarkLbl=\"1\""),
        "{written}"
    );
}

/// Regression (Prüfstand mit echten LVs, Musterdateien 3.1): a complement between
/// two runs of the same format must not leave them split – after writing
/// and reading back they are one run.
#[test]
fn runs_around_a_complement_are_joined_again() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>1</Qty><QU>St</QU>
             <Description><CompleteText><ComplTSA>Yes</ComplTSA><DetailTxt>
               <Text><span style="font-family:Arial;font-size:10pt;">Leistung </span></Text>
               <TextComplement MarkLbl="1" Kind="Owner"><ComplBody><span>'10'</span></ComplBody></TextComplement>
               <Text><span style="font-family:Arial;font-size:10pt;"> kVA, Wandschrank,</span><br/><span>mit Kontakt</span></Text>
             </DetailTxt>
             <OutlineText><OutlTxt><TextOutlTxt><p><span>USV</span></p></TextOutlTxt></OutlTxt></OutlineText>
             </CompleteText></Description></Item>"#,
    );
    let item = only_item(&doc);
    let TextBlock::Paragraph { runs, .. } = &item.long_text.blocks[0] else {
        panic!("{:?}", item.long_text)
    };
    assert_eq!(runs[0].text, "Leistung  kVA, Wandschrank,\n", "{runs:?}");
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(
        item.complements[0].anchor,
        Some(TextAnchor {
            block: 0,
            offset: 9,
            own_line: false
        })
    );
    for phase in [Phase::Dp81, Phase::Dp83, Phase::Dp86] {
        assert_eq!(
            roundtrip(&doc, phase),
            xml::as_written(&doc, phase),
            "{phase:?}"
        );
    }
}

/// Ansprache und Nachsatz einer Textergänzung stehen in der
/// Schrift des Textes davor (9 pt); die Standardschrift wird nicht
/// ausgeschrieben, der Wert bleibt ohne Schrift. Text und Leerzeichen bleiben.
#[test]
fn complement_caption_and_tail_take_a_deviating_font_of_the_text() {
    let doc = snippet(
        r#"<Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU>
             <Description><CompleteText><ComplTSA>Yes</ComplTSA><DetailTxt>
               <Text><p style="font-size:9pt;"><span style="font-family:Arial;font-size:9pt;">Sammelschiene 400 V AC,</span></p></Text>
               <TextComplement MarkLbl="11" Kind="Owner"><ComplCaption><span style="font-size:9pt;">Spannung DC </span><span> </span></ComplCaption><ComplBody><span>'220'</span></ComplBody><ComplTail><span> </span><span style="font-size:9pt;">V, Frequenz 50 </span><br/></ComplTail></TextComplement>
               <Text><p><span style="font-family:Arial;font-size:10pt;">Hz, Strom</span></p></Text>
               <TextComplement MarkLbl="12" Kind="Owner"><ComplCaption><span>Bemessungsstrom </span></ComplCaption><ComplBody><span>'16'</span></ComplBody><ComplTail><span> A</span></ComplTail></TextComplement>
             </DetailTxt><OutlineText><OutlTxt><TextOutlTxt><span>Schiene</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item>"#,
    );
    let item = item_by_rno(&doc, "0010");
    assert_eq!(item.complements[0].caption, "Spannung DC  ");
    assert_eq!(item.complements[0].tail, " V, Frequenz 50 \n");
    let text = xml::write(&doc, Phase::Dp83).unwrap();
    assert!(
        text.contains(
            r#"<ComplCaption><span style="font-size:9pt;">Spannung DC  </span></ComplCaption>"#
        ),
        "{text}"
    );
    assert!(
        text.contains(r#"<ComplBody><span>'220'</span></ComplBody>"#),
        "{text}"
    );
    assert!(
        text.contains(
            r#"<ComplTail><span style="font-size:9pt;"> V, Frequenz 50 </span><br/></ComplTail>"#
        ),
        "{text}"
    );
    // nach Text in Arial 10 pt: ohne Schriftangabe
    assert!(
        text.contains(r#"<ComplCaption><span>Bemessungsstrom </span></ComplCaption>"#),
        "{text}"
    );
    assert_eq!(
        roundtrip(&doc, Phase::Dp83),
        xml::as_written(&doc, Phase::Dp83)
    );
}

#[test]
fn change_order_number_and_status_roundtrip() {
    let mut doc = sample();
    doc.award.boq.change_order = Some(ChangeOrderTag {
        number: 2,
        status: "Approved".into(),
    });
    let mut tagged = 0;
    fn tag(nodes: &mut [BoqNode], tagged: &mut usize) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => tag(&mut category.body, tagged),
                BoqNode::Item(item) if *tagged == 0 => {
                    item.change_order = Some(ChangeOrderTag {
                        number: 2,
                        status: "Rejected".into(),
                    });
                    *tagged += 1;
                }
                _ => {}
            }
        }
    }
    tag(&mut doc.award.boq.body, &mut tagged);
    for phase in [Phase::Dp83, Phase::Dp86, Phase::Dp82] {
        let text: String = xml::write(&doc, phase)
            .unwrap()
            .split_whitespace()
            .collect();
        assert!(
            text.contains("<CONo>2</CONo><COStatus>Approved</COStatus>"),
            "{phase:?}"
        );
        assert!(
            text.contains("<CONo>2</CONo><COStatus>Rejected</COStatus>"),
            "{phase:?}"
        );
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase));
        assert_eq!(again.award.boq.change_order.as_ref().unwrap().number, 2);
    }
    // X84 kennt keinen Nachtrag.
    let text = xml::write(&doc, Phase::Dp84).unwrap();
    assert!(!text.contains("CONo"));
    // Leerer Status wird „angemeldet“.
    doc.award.boq.change_order = Some(ChangeOrderTag {
        number: 1,
        status: String::new(),
    });
    let text: String = xml::write(&doc, Phase::Dp83)
        .unwrap()
        .split_whitespace()
        .collect();
    assert!(text.contains("<CONo>1</CONo><COStatus>Filed</COStatus>"));
}

/// Prüfstand (Fachregel R11): a quantity in an X84 is the bidder's free
/// quantity (Regeln für DA84) – writing the bid again must keep it.
#[test]
fn free_quantity_of_a_bid_survives_writing() {
    let bid = format!("{SNIPPET_HEAD}<Item ID=\"i1\" RNoPart=\"10\"><Qty>12.500</Qty><UP>3.00</UP><IT>37.50</IT></Item>
     <Item ID=\"i2\" RNoPart=\"20\"><UP>4.00</UP><IT>8.00</IT></Item>{SNIPPET_TAIL}")
    .replace("<DP>83</DP>", "<DP>84</DP>")
    .replace("DA83", "DA84");
    let doc = xml::read(bid.as_bytes()).unwrap();
    let items = doc.award.boq.items();
    assert!(items[0].1.qty_to_be_determined);
    assert_eq!(items[0].1.qty, Some(dec("12.5")));
    assert!(!items[1].1.qty_to_be_determined);
    let written = xml::write(&doc, Phase::Dp84).unwrap();
    assert!(written.contains("<Qty>12.500</Qty>"), "{written}");
    let again = xml::read(written.as_bytes()).unwrap();
    assert_eq!(again.award.boq.items()[0].1.qty, Some(dec("12.5")));
    // What the writer promises for a bid read from a file.
    assert_eq!(again, xml::as_written(&doc, Phase::Dp84));
}

/// Mehrere Textergänzungen direkt hintereinander, je durch eine leere Zeile
/// getrennt (Form aus STLB-Bau/iTWO): Lesen, Schreiben, wieder Lesen ergibt
/// dasselbe wie `as_written` – für Positionen und Hinweistexte.
#[test]
fn consecutive_complements_on_own_lines_roundtrip() {
    let detail = r#"<DetailTxt><Text><span>Einrichtung für Managementfunktionen,</span><br/><span/></Text><TextComplement MarkLbl="11" Kind="Owner"><ComplCaption><span>Anzahl A </span></ComplCaption><ComplBody><span>'</span><br/><span>..........'</span></ComplBody><ComplTail/></TextComplement><Text><span/><br/><span/></Text><TextComplement MarkLbl="12" Kind="Owner"><ComplCaption><span>Anzahl B </span></ComplCaption><ComplBody><span>'</span><br/><span>..........'</span></ComplBody><ComplTail/></TextComplement><Text><span/><br/><span/></Text><TextComplement MarkLbl="13" Kind="Owner"><ComplCaption><span>Anzahl C </span></ComplCaption><ComplBody><span>'</span><br/><span>..........'</span></ComplBody><ComplTail/></TextComplement><Text><span/><br/><span>für Einbau, mit Schnittstellen,</span><br/><span>weiter im Text,</span><br/><span/></Text><TextComplement MarkLbl="16" Kind="Owner"><ComplCaption><span>Anzahl D </span></ComplCaption><ComplBody><span>'</span><br/><span>..........'</span></ComplBody><ComplTail/></TextComplement><Text><span/><br/><span>Ende.</span></Text></DetailTxt>"#;
    let doc = snippet(&format!(
        r#"<Remark ID="r1"><Description><CompleteText>{detail}</CompleteText></Description></Remark>
           <Item ID="i1" RNoPart="0010"><Qty>1.000</Qty><QU>St</QU><Description><CompleteText>{detail}<OutlineText><OutlTxt><TextOutlTxt><span>K</span></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item>"#
    ));
    for phase in [Phase::Dp81, Phase::Dp83] {
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
    }
}

/// Zuschlagsposition (MarkupItem) mit Kostengruppen-Zuordnung (CtlgAssign
/// nach der Description, Form wie in Dateien anderer Programme): bleibt in X81/X82/X83/X86.
#[test]
fn markup_items_keep_their_catalog_assignments() {
    let snippet_with_catalog = SNIPPET_HEAD.replace(
        "<BoQBkdn><Type>Index</Type><Length>1</Length><Num>No</Num></BoQBkdn>\n   </BoQInfo>",
        "<BoQBkdn><Type>Index</Type><Length>1</Length><Num>No</Num></BoQBkdn>\n    <Ctlg><CtlgID>kg</CtlgID><CtlgType>cost group DIN 276 2018-12</CtlgType><CtlgName>DIN 276</CtlgName></Ctlg>\n   </BoQInfo>",
    );
    let items = r#"<Item ID="i1" RNoPart="0010"><Qty>2.000</Qty><QU>St</QU><CtlgAssign><CtlgID>kg</CtlgID><CtlgCode>481</CtlgCode></CtlgAssign><Description><OutlineText><OutlTxt><TextOutlTxt><span>Basis</span></TextOutlTxt></OutlTxt></OutlineText></Description></Item>
           <MarkupItem ID="m1" RNoPart="0020"><MarkupType>ListInSubQty</MarkupType><MarkupSubQty><RefItem IDRef="i1"/></MarkupSubQty><Description><OutlineText><OutlTxt><TextOutlTxt><span>Zuschlag</span></TextOutlTxt></OutlTxt></OutlineText></Description><CtlgAssign><CtlgID>kg</CtlgID><CtlgCode>491</CtlgCode></CtlgAssign></MarkupItem>"#;
    let doc = xml::read(format!("{snippet_with_catalog}{items}{SNIPPET_TAIL}").as_bytes()).unwrap();
    let markup = item_by_rno(&doc, "0020");
    assert_eq!(markup.catalog_assigns.len(), 1);
    for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp86] {
        let text = xml::write(&doc, phase).unwrap();
        let markup = &text[text.find("<MarkupItem").unwrap()..text.find("</MarkupItem>").unwrap()];
        let description = markup.find("</Description>").unwrap();
        let assign = markup.find("<CtlgCode>491</CtlgCode>").expect("Zuordnung");
        assert!(
            description < assign,
            "nach der Description (XSD): {phase:?}\n{markup}"
        );
        let again = roundtrip(&doc, phase);
        assert_eq!(again, xml::as_written(&doc, phase), "{phase:?}");
    }
}

/// The type of an EP share (`LblUPComp Type`) stays as in the file, also
/// "Unknown" (GAEB 3.3 allows it); without a type it follows the label, only
/// for other labels the usual order Wages, Materials, Plant, Miscellaneous.
#[test]
fn up_component_types_stay_or_follow_the_label() {
    let mut doc = sample();
    doc.award.boq.up_component_labels = vec!["Material".into(), "Lohn".into()];
    doc.award.boq.up_component_kinds = vec!["Unknown".into(), "Unknown".into()];
    let text = xml::write(&doc, Phase::Dp83).unwrap();
    assert!(
        text.contains(r#"<LblUPComp1 Type="Unknown">Material</LblUPComp1>"#),
        "{text}"
    );
    assert!(text.contains(r#"<LblUPComp2 Type="Unknown">Lohn</LblUPComp2>"#));
    assert_eq!(
        roundtrip(&doc, Phase::Dp83),
        xml::as_written(&doc, Phase::Dp83)
    );

    doc.award.boq.up_component_labels = vec!["Material".into(), "Lohn".into(), "Montage".into()];
    doc.award.boq.up_component_kinds = vec![String::new(), "Lohn".into(), String::new()];
    let text = xml::write(&doc, Phase::Dp82).unwrap();
    assert!(text.contains(r#"<LblUPComp1 Type="Materials">Material</LblUPComp1>"#));
    assert!(text.contains(r#"<LblUPComp2 Type="Wages">Lohn</LblUPComp2>"#));
    assert!(text.contains(r#"<LblUPComp3 Type="Plant">Montage</LblUPComp3>"#));
    let again = roundtrip(&doc, Phase::Dp82);
    assert_eq!(again, xml::as_written(&doc, Phase::Dp82));
    assert_eq!(
        again.award.boq.up_component_kinds,
        ["Materials", "Wages", "Plant"]
    );
}

/// Schlussbemerkung (Anwender 18.11., „Zulieferungen originalgetreu“): AddText
/// at the root after the Award is read as a closing text, not a Vorbemerkung,
/// and written back after the Award – in DA 84 there is none.
#[test]
fn closing_texts_stay_after_the_award() {
    let doc = sample();
    let text = proava_gaeb::xml::write(&doc, Phase::Dp83).unwrap();
    let award_end = text.find("</Award>").unwrap();
    let closing = text.find("Schlussblatt").unwrap();
    assert!(
        closing > award_end,
        "nach dem Award: {}",
        &text[award_end..]
    );
    assert!(text[..award_end].contains("Vorbemerkungen"));
    let back = proava_gaeb::xml::read(text.as_bytes()).unwrap();
    assert_eq!(back.award.closing_texts, doc.award.closing_texts);
    assert_eq!(back.award.add_texts, doc.award.add_texts);
    let bid = proava_gaeb::xml::write(&doc, Phase::Dp84).unwrap();
    assert!(!bid.contains("Schlussblatt"), "84 ohne AddText");
}
