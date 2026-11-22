//! Example project for trying the bid views without files: a small GA-LV with
//! four bids that differ the way real bids do.

use proava_domain::{
    Address, Bid, BidPrice, Decimal, Discount, Lv, LvGroup, LvRow, OzMask, Position, PositionKind,
    Project, ProjectName,
};

struct Item {
    text: &'static str,
    quantity: i64,
    unit: &'static str,
    cents: i64,
    kind: PositionKind,
}

const fn item(text: &'static str, quantity: i64, unit: &'static str, cents: i64) -> Item {
    Item {
        text,
        quantity,
        unit,
        cents,
        kind: PositionKind::Normal,
    }
}

const fn special(
    text: &'static str,
    quantity: i64,
    unit: &'static str,
    cents: i64,
    kind: PositionKind,
) -> Item {
    Item {
        text,
        quantity,
        unit,
        cents,
        kind,
    }
}

type Section = (&'static str, &'static [(&'static str, &'static [Item])]);

const SECTIONS: &[Section] = &[
    (
        "Automationsstationen",
        &[
            (
                "ASP01 Heizzentrale",
                &[
                    item(
                        "DDC-Automationsstation, frei programmierbar",
                        1,
                        "St",
                        285000,
                    ),
                    item("E/A-Modul 16 DI", 3, "St", 41000),
                    item("E/A-Modul 8 AI", 2, "St", 46500),
                    item("E/A-Modul 8 AO", 2, "St", 44000),
                    item("Bedien- und Anzeigeeinheit 7\"", 1, "St", 98000),
                    item(
                        "Schaltschrank 1200x2000x400, komplett verdrahtet",
                        1,
                        "St",
                        1240000,
                    ),
                    item("Netzwerkswitch 8 Port, Hutschiene", 1, "St", 39000),
                ],
            ),
            (
                "ASP02 Lüftung RLT01–RLT03",
                &[
                    item(
                        "DDC-Automationsstation, frei programmierbar",
                        1,
                        "St",
                        285000,
                    ),
                    item("E/A-Modul 16 DI", 4, "St", 41000),
                    item("E/A-Modul 8 AI", 3, "St", 46500),
                    item("E/A-Modul 8 AO", 3, "St", 44000),
                    item("Frequenzumrichter-Anbindung Modbus RTU", 6, "St", 18500),
                    item(
                        "Schaltschrank 1600x2000x500, komplett verdrahtet",
                        1,
                        "St",
                        1680000,
                    ),
                ],
            ),
        ],
    ),
    (
        "Feldgeräte",
        &[
            (
                "Fühler",
                &[
                    item(
                        "Tauchtemperaturfühler Pt1000 mit Schutzrohr",
                        24,
                        "St",
                        9800,
                    ),
                    item("Kanaltemperaturfühler Pt1000", 18, "St", 8600),
                    item("Außentemperaturfühler mit Strahlungsschutz", 2, "St", 14500),
                    item("Kanalfeuchte- und Temperaturfühler", 6, "St", 32000),
                    item("CO2-Fühler Raum, 0–10 V", 12, "St", 26500),
                    item("Differenzdruckschalter Filter", 9, "St", 7400),
                    item("Frostschutzthermostat 6 m Kapillare", 3, "St", 16800),
                ],
            ),
            (
                "Stellgeräte",
                &[
                    item(
                        "Durchgangsventil DN25 mit Stellantrieb 0–10 V",
                        8,
                        "St",
                        64000,
                    ),
                    item(
                        "Durchgangsventil DN40 mit Stellantrieb 0–10 V",
                        4,
                        "St",
                        92000,
                    ),
                    item(
                        "Dreiwegeventil DN50 mit Stellantrieb 0–10 V",
                        2,
                        "St",
                        138000,
                    ),
                    item("Klappenantrieb 10 Nm, stetig", 10, "St", 23500),
                    item("Klappenantrieb 20 Nm mit Federrücklauf", 4, "St", 38500),
                    special(
                        "Raumbediengerät Standard",
                        12,
                        "St",
                        14500,
                        PositionKind::Base { group: 1 },
                    ),
                    special(
                        "Raumbediengerät mit Display",
                        12,
                        "St",
                        24500,
                        PositionKind::Alternative {
                            group: 1,
                            serial: 1,
                        },
                    ),
                ],
            ),
            (
                "Verkabelung",
                &[
                    item("Installationskabel J-Y(St)Y 2x2x0,8", 1800, "m", 165),
                    item("Leitung NYM-J 5x1,5", 650, "m", 245),
                    item("Kabelrinne 100x60 mit Deckel", 120, "m", 2850),
                    item("Brandschottung für Kabeldurchführung", 14, "St", 8900),
                ],
            ),
        ],
    ),
    (
        "Software und Dienstleistungen",
        &[
            (
                "Software",
                &[
                    item("Anwendersoftware je Datenpunkt", 640, "St", 1150),
                    item(
                        "Managementebene, Lizenz 1000 Datenpunkte",
                        1,
                        "psch",
                        980000,
                    ),
                    item("Anlagenbilder je Anlage", 9, "St", 48000),
                    item("BACnet-Gateway zur Fremdanlage", 2, "St", 145000),
                ],
            ),
            (
                "Inbetriebnahme und Dokumentation",
                &[
                    item("Inbetriebnahme und Datenpunkttest", 1, "psch", 640000),
                    item("Revisionsunterlagen", 1, "psch", 220000),
                    item("Einweisung des Betreibers", 8, "h", 9500),
                    special(
                        "Programmierer (Stundenlohn, nur auf Anordnung)",
                        40,
                        "h",
                        9800,
                        PositionKind::Provisional { with_total: false },
                    ),
                    special(
                        "Zusätzliche Wartung im 1. Jahr",
                        1,
                        "psch",
                        180000,
                        PositionKind::Provisional { with_total: true },
                    ),
                ],
            ),
        ],
    ),
];

/// One bidder: general price level (per mille of the planner's price),
/// seed for the scatter and what is special about the bid.
struct Bidder {
    name: &'static str,
    street: &'static str,
    postal_code: &'static str,
    city: &'static str,
    level: i64,
    seed: u64,
}

const BIDDERS: &[Bidder] = &[
    Bidder {
        name: "Regeltechnik Rhein-Ruhr GmbH",
        street: "Am Hafen 12",
        postal_code: "12345",
        city: "Musterstadt",
        level: 985,
        seed: 11,
    },
    Bidder {
        name: "Müller Gebäudeautomation GmbH",
        street: "Industriestraße 4",
        postal_code: "45127",
        city: "Essen",
        level: 1040,
        seed: 23,
    },
    Bidder {
        name: "Schmidt & Partner MSR KG",
        street: "Kölner Landstraße 210",
        postal_code: "40591",
        city: "Düsseldorf",
        level: 1095,
        seed: 37,
    },
    Bidder {
        name: "Automation Nord GmbH",
        street: "Speicherweg 3",
        postal_code: "20457",
        city: "Hamburg",
        level: 1180,
        seed: 51,
    },
];

/// Small deterministic scatter generator; the demo must look the same every time.
struct Scatter(u64);

impl Scatter {
    fn next_permille(&mut self, spread: i64) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = ((self.0 >> 33) % (2 * spread as u64 + 1)) as i64;
        value - spread
    }
}

/// The demo project: three titles with sub titles, 40 positions (with a
/// Bedarfs- and a Wahlposition) and four bids – one with 3 % discount, one
/// with missing prices, an arithmetic error and an unknown order number.
pub fn demo_project() -> Project {
    build(SECTIONS, BIDDERS.len(), 1)
}

/// The demo project with a Kostenschätzung (LPH 2) and a Kostenberechnung
/// (LPH 3) of a synthetic school (Reiter Kosten, Handbuch).
pub fn cost_demo_project() -> Project {
    use proava_domain::cost_demo::demo_cost_estimate;
    use proava_domain::CostStage;
    let mut project = build_named("Demo Kosten GA", SECTIONS, BIDDERS.len(), 1);
    // Kostengruppen at the titles, for the Kostenkontrolle.
    let lv = project.lv_mut();
    for index in 0..lv.rows().len() {
        if let Some(proava_domain::LvRow::Group(group)) = lv.row_mut(index) {
            let code = match group.short_text() {
                "Automationsstationen" => Some("481"),
                "Feldgeräte" => Some("484"),
                "Software und Dienstleistungen" => Some("483"),
                _ => None,
            };
            if let Some(code) = code {
                group.set_cost_group(Some(code.into()));
            }
        }
    }
    project.add_cost_estimate(demo_cost_estimate(CostStage::Estimate));
    project.add_cost_estimate(demo_cost_estimate(CostStage::Calculation));
    project
}

/// [`cost_demo_project`] with a Kostenberechnung KG 400 as Kosten-LV (Kurztext-LV,
/// KG 410–430 by positions; the KG 480 stays the Datenpunktrechnung) – all
/// quantities and Kennwerte made up. The Kosten-LV is the active LV.
pub fn cost_lv_demo_project() -> Project {
    let mut project = cost_demo_project();
    let index = project.add_lv(cost_lv());
    project.set_active_lv(index);
    project
}

/// GU-Fall: the Kostenberechnung KG 400 in one Kostenermittlung – the
/// Kosten-LV of [`cost_lv_demo_project`] (KG 410–430 as Kurztext positions)
/// with the Datenpunkt-Abschnitt 480 (the plant list of the demo
/// Datenpunktrechnung); no standalone Datenpunktrechnung. Next to it the tender
/// LV of [`demo_project`]. All quantities and Kennwerte made up. The Kosten-LV
/// is the active LV.
pub fn gu_demo_project() -> Project {
    use proava_domain::cost_demo::demo_cost_estimate;
    use proava_domain::{CostStage, LvRow};
    let mut project = demo_project();
    let mut lv = cost_lv();
    lv.set_title("Kostenberechnung KG 400 (GU)");
    let index = lv
        .insert_point_section()
        .expect("Kosten-LV nimmt einen Datenpunkt-Abschnitt");
    if let Some(LvRow::Group(group)) = lv.row_mut(index) {
        let mut points = demo_cost_estimate(CostStage::Calculation);
        points.title.clear();
        points.remark.clear();
        group.set_points(Some(points));
    }
    let index = project.add_lv(lv);
    project.set_active_lv(index);
    project
}

/// The GA as Kurztext (Anwender 19.11.: „die Darstellung ist einheitlich“):
/// the Kosten-LV of [`cost_lv_demo_project`] (KG 410–430) with a title 480 and
/// below it Anlagenautomation and Raumautomation – the Datenpunkte AI … BV as
/// Rechengrundlage, under each the Aufteilung on KG 481–489. Next to it the
/// tender LV of [`demo_project`]. All quantities made up. The Kosten-LV is the
/// active LV.
pub fn ga_kurztext_demo_project() -> Project {
    let mut project = demo_project();
    let mut lv = cost_lv();
    lv.set_title("Kostenberechnung KG 400 (GA als Kurztext)");
    let mut title = LvGroup::new(0, "480", "Gebäude- und Anlagenautomation");
    title.set_cost_group(Some("480".into()));
    lv.push_group(title);
    // (Untertitel, Mengen AI, BI, AO, BO, AV, BV)
    let blocks: [(&str, [i64; 6]); 2] = [
        ("Anlagenautomation", [100, 150, 20, 15, 11, 10]),
        ("Raumautomation", [300, 200, 150, 170, 1, 1]),
    ];
    for (name, quantities) in blocks {
        let title = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == "480")
            .expect("Titel 480");
        let range = lv
            .insert_points_template(Some(title))
            .expect("Kosten-LV nimmt die GA-Vorlage");
        let ids: Vec<_> = lv.rows()[range].iter().map(LvRow::id).collect();
        if let Some(LvRow::Group(group)) = lv.index_of(ids[0]).and_then(|index| lv.row_mut(index)) {
            group.set_short_text(name);
        }
        for (id, quantity) in ids[1..7].iter().zip(quantities) {
            if let Some(position) = lv
                .index_of(*id)
                .and_then(|index| lv.row_mut(index))
                .and_then(LvRow::as_position_mut)
            {
                position.set_quantity(Decimal::from(quantity));
            }
        }
    }
    let index = project.add_lv(lv);
    project.set_active_lv(index);
    project
}

/// The Kostenberechnung KG 400 as Kosten-LV of [`cost_lv_demo_project`].
fn cost_lv() -> Lv {
    use proava_domain::{cost_lv_mask, CostStage, LvGroup, LvPurpose, Note, RichText};
    let stage = CostStage::Calculation;
    let mask = cost_lv_mask(stage);
    let mut lv = Lv::new("Kostenberechnung KG 400", mask);
    lv.set_number("KB");
    lv.set_purpose(LvPurpose::Cost(stage));
    let mut note = Note::rich(RichText::plain(
        "Kostenberechnung nach DIN 276:2018-12, Gliederung bis zur 3. Ebene der Kostengliederung.\n\
         Kostenstand: 4. Quartal 2026\n\
         Grundlagen: Entwurfsplanung LPH 3, Stand 10/2026\n\
         Verfahren und Quellen der Kostenkennwerte: Mengen aus den Entwurfsplänen, Kennwerte aus eigenen \
         Projekten 2024–2026 (Bibliothek)\n\
         Alle Beträge netto ohne Umsatzsteuer.",
    ));
    note.set_heading("Angaben zur Kostenberechnung");
    note.set_preliminary(true);
    lv.push_note(note);
    // (Kurztext, Menge, Einheit, Kennwert) under (KG 3. Ebene, Name) under (KG 2. Ebene, Name).
    type Items = &'static [(&'static str, i64, &'static str, i64)];
    type Subgroups = &'static [(&'static str, &'static str, Items)];
    let groups: [(&str, &str, Subgroups); 3] = [
        (
            "410",
            "Abwasser-, Wasser-, Gasanlagen",
            &[
                (
                    "411",
                    "Abwasseranlagen",
                    &[("Abwasserleitungen", 900, "m", 85)],
                ),
                (
                    "412",
                    "Wasseranlagen",
                    &[
                        ("Wasserleitungen", 1100, "m", 62),
                        ("Sanitärobjekte", 85, "St", 950),
                    ],
                ),
            ],
        ),
        (
            "420",
            "Wärmeversorgungsanlagen",
            &[
                (
                    "421",
                    "Wärmeerzeugungsanlagen",
                    &[("Wärmepumpenanlage Sole/Wasser", 120, "kW", 1450)],
                ),
                (
                    "422",
                    "Wärmeverteilnetze",
                    &[("Rohrleitungen Wärmeverteilnetz", 1250, "m", 68)],
                ),
                ("423", "Raumheizflächen", &[("Heizkörper", 140, "St", 520)]),
            ],
        ),
        (
            "430",
            "Raumlufttechnische Anlagen",
            &[(
                "431",
                "Lüftungsanlagen",
                &[
                    ("Zuluftanlage", 12_000, "m3/h", 4),
                    ("Zuluftleitungen", 1850, "m2", 72),
                ],
            )],
        ),
    ];
    for (code, name, subgroups) in groups {
        let number: u32 = code.parse().unwrap_or(0);
        let mut title = LvGroup::new(0, lv.oz_mask().format_group(&[number]), name);
        title.set_cost_group(Some(code.into()));
        lv.push_group(title);
        for (sub, sub_name, positions) in subgroups {
            let sub_number: u32 = sub.parse().unwrap_or(0);
            let mut subtitle = LvGroup::new(
                1,
                lv.oz_mask().format_group(&[number, sub_number]),
                *sub_name,
            );
            subtitle.set_cost_group(Some((*sub).into()));
            lv.push_group(subtitle);
            for (index, (text, quantity, unit, price)) in positions.iter().enumerate() {
                lv.push_position(Position::new(
                    format!("{code}.{sub}.{:04}", (index + 1) * 10),
                    *text,
                    Decimal::from(*quantity),
                    *unit,
                    Some(Decimal::from(*price)),
                ));
            }
        }
    }
    lv
}

/// The demo with everything the program can hold (Affentest, trying out by
/// hand): [`cost_demo_project`] and in LV 480 an STLB-Bau position (Langtext
/// and unit write-protected, a Textergänzung of the Auftraggeber) and a
/// Langtext with lists and Unterpunkten – rows, sums and bids of LV 480 stay
/// as in [`demo_project`]; a second Ausschreibungs-LV with a title with a
/// Pauschalsumme, an STLB-Bau position, a Zuschlagsposition and a
/// Schlussbemerkung; the Kosten-LV of [`cost_lv_demo_project`]. LV 480 is the
/// active LV. All data made up.
pub fn full_demo_project() -> Project {
    use proava_domain::{Money, Note, RichText, Run, Surcharge, TextBlock};
    let mut project = cost_demo_project();
    let lv = project.lv_mut();
    let positions: Vec<usize> = (0..lv.rows().len())
        .filter(|&index| lv.rows()[index].is_position())
        .collect();
    // The second position comes from STLB-Bau (the first keeps a free text).
    if let Some(position) = lv
        .row_mut(positions[1])
        .and_then(proava_domain::LvRow::as_position_mut)
    {
        as_stlb(position, 4711);
    }
    // A Langtext with lists and Unterpunkten.
    if let Some(position) = lv
        .row_mut(positions[2])
        .and_then(proava_domain::LvRow::as_position_mut)
    {
        let item = |ordered: bool, level: u8, text: &str| {
            TextBlock::list_item_at(ordered, level, vec![Run::plain(text)])
        };
        position.set_long_text(RichText::new(vec![
            TextBlock::text("Analoge Eingänge, je Kanal einstellbar:"),
            item(false, 0, "Spannung"),
            item(false, 1, "0–10 V"),
            item(false, 1, "2–10 V"),
            item(false, 0, "Strom 4–20 mA"),
            item(false, 0, "Widerstand"),
            item(false, 1, "Pt1000"),
            item(false, 2, "Zweileiter"),
            item(false, 2, "Dreileiter"),
            TextBlock::text("Inbetriebnahme:"),
            item(true, 0, "Verdrahtung prüfen"),
            item(true, 1, "Schirmung auflegen"),
            item(true, 0, "Kanäle parametrieren"),
        ]));
    }

    // Second Ausschreibungs-LV: Schaltschrankbau.
    let mask = OzMask::parse("11.22.PPPP").expect("valid mask");
    let mut second = Lv::new("LV 482 Schaltschrankbau", mask);
    second.set_number("482");
    let mut title = LvGroup::new(0, "", "Schaltschränke");
    title.set_cost_group(Some("482".into()));
    second.push_group(title);
    let mut subtitle = LvGroup::new(1, "", "Schaltschrank ASP01");
    // Pauschalsumme of the title (GAEB TotalLSUM).
    subtitle.set_lump_sum_total(Some(Money::from_cents(1_850_000)));
    second.push_group(subtitle);
    let mut cabinet = Position::new(
        "",
        "Schaltschrank Standgehäuse 1200x2000x400",
        Decimal::ONE,
        "St",
        Some(Decimal::new(980000, 2)),
    );
    as_stlb(&mut cabinet, 4712);
    second.push_position(cabinet);
    let mut wiring = Position::new(
        "",
        "Verdrahtung und Prüfung",
        Decimal::ONE,
        "psch",
        Some(Decimal::new(640000, 2)),
    );
    wiring.set_lump_sum(true);
    wiring.set_marked_for_surcharge(true);
    second.push_position(wiring);
    // Zuschlagsposition: 5 % on the positions of the list before it.
    let mut surcharge = Position::new(
        "",
        "Zuschlag Kleinmaterial",
        Decimal::ZERO,
        "",
        Some(Decimal::from(5)),
    );
    surcharge.set_surcharge(Some(Surcharge::default()));
    second.push_position(surcharge);
    let mut closing = Note::rich(RichText::plain(
        "Alle Schaltschränke werden vor der Auslieferung im Werk geprüft; das Prüfprotokoll liegt bei.",
    ));
    closing.set_heading("Schlussbemerkung");
    closing.set_closing(true);
    second.push_note(closing);
    second.renumber();
    project.add_lv(second);
    project.add_lv(cost_lv());
    project.set_active_lv(0);
    project
}

/// Makes `position` a text from STLB-Bau (made-up key, group `group`): a
/// Langtext with a Textergänzung of the Auftraggeber, unit „St“.
fn as_stlb(position: &mut Position, group: i64) {
    use proava_domain::stlb::{StlbCharacteristic, StlbReference};
    use proava_domain::{ComplementKind, RichText, TextComplement};
    position.set_long_text(RichText::plain(
        "Gehäuse aus Stahlblech, pulverbeschichtet, Schutzart IP 54, Farbe",
    ));
    position.set_unit("St");
    position.set_complements(vec![TextComplement {
        marker: "1".into(),
        kind: ComplementKind::Owner,
        body: "RAL 7035".into(),
        ..TextComplement::default()
    }]);
    position.attach_stlb(StlbReference {
        catalog: "STLB-Bau".into(),
        version: Some("2026-10".into()),
        work_category: "070".into(),
        group,
        cost_group: "482".into(),
        characteristics: vec![StlbCharacteristic {
            characteristic: 1,
            index: Some(0),
            value: Some(2),
            short_text_part: true,
        }],
        owner_complements: true,
        ..StlbReference::default()
    });
}

/// A big version for performance checks: the demo LV repeated `copies` times
/// and `bidders` bids (up to eight).
pub fn large_demo_project(copies: usize, bidders: usize) -> Project {
    build(SECTIONS, bidders.clamp(1, 8), copies.max(1))
}

/// The demo project with a contract and its billing (erfunden): the
/// first bid awarded and taken over, change order N01 (two new positions,
/// approved), three cumulative Abschlagsrechnungen, one Teilrechnung as
/// increment and the Schlussrechnung – with takeoff lines, progress on a
/// lump sum, a reduced quantity, a price different from the contract and a
/// position billed without contract.
pub fn billing_demo_project() -> Project {
    use proava_domain::{
        ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus, Deduction,
        Invoice, InvoiceItem, InvoiceKind, Mode, Progress, TakeoffLine, VatRule,
    };

    let mut project = build_named("Demo Abrechnung GA", SECTIONS, BIDDERS.len(), 1);
    {
        let terms = &mut project.info_mut().terms;
        terms.performance_security_percent = Some(Decimal::from(10));
        terms.warranty_security_percent = Some(Decimal::from(5));
        terms.cash_discount_percent = Some(Decimal::from(2));
        terms.cash_discount_days = Some(14);
        terms.deductions = vec![Deduction {
            label: "Bauwesenversicherung".into(),
            percent: Decimal::new(3, 1),
        }];
    }
    let bid = project.bids()[0].id();
    project.award_bid(bid, "01.12.2026").expect("bid exists");
    project.take_over_contract().expect("awarded");
    if let Some(contract) = project.contract_mut() {
        contract.number = "A-2026-480".into();
    }

    // Change order N01: two new positions under a title „Nachträge“.
    let lv = project.lv_mut();
    lv.push_group(LvGroup::new(0, "", "Nachträge"));
    lv.push_group(LvGroup::new(1, "", "N01 Zusätzliche Raumbediengeräte"));
    let device = lv.push_position(Position::new(
        "",
        "Raumbediengerät mit CO2-Sensor, zusätzlich",
        Decimal::from(6),
        "St",
        None,
    ));
    let mut commissioning = Position::new(
        "",
        "Inbetriebnahme Raumbediengeräte",
        Decimal::ONE,
        "psch",
        None,
    );
    commissioning.set_lump_sum(true);
    let commissioning = lv.push_position(commissioning);
    let without_contract = lv.push_position(Position::new(
        "",
        "Zusätzliche Kabelverlegung ohne Auftrag",
        Decimal::from(40),
        "m",
        None,
    ));
    lv.renumber();
    let mut order = ChangeOrder::new(ChangeOrderId(0), 1, "Zusätzliche Raumbediengeräte");
    order.basis = ChangeBasis::AdditionalPerformance;
    order.reason = "Bauherrenwunsch: zusätzliche Besprechungsräume im 1. OG.".into();
    order.status = ChangeOrderStatus::Approved;
    order.offer_number = "NA-01".into();
    order.decision_date = "15.01.2027".into();
    order.items = vec![
        ChangeOrderItem {
            position: device,
            offered_quantity: Decimal::from(6),
            offered_unit_price: Some(Decimal::new(41850, 2)),
            checked_unit_price: Some(Decimal::new(39500, 2)),
            note: "Preis auf Niveau Hauptauftrag".into(),
            ..ChangeOrderItem::default()
        },
        ChangeOrderItem {
            position: commissioning,
            offered_quantity: Decimal::ONE,
            offered_unit_price: Some(Decimal::from(480)),
            ..ChangeOrderItem::default()
        },
    ];
    project.add_change_order(order);

    let positions: Vec<(proava_domain::RowId, Decimal, bool)> = project
        .contract()
        .expect("contract")
        .items
        .iter()
        .filter(|(_, item)| item.counts())
        .map(|(id, item)| (*id, item.quantity, item.lump_sum))
        .collect();
    // Share of the ordered quantity per Abschlag, by position index.
    let share = |index: usize, step: usize| -> Decimal {
        let base = [30, 60, 85][step] as i64;
        let wobble = ((index * 7 + step * 3) % 9) as i64 - 4;
        Decimal::from((base + wobble).clamp(0, 100)) / Decimal::ONE_HUNDRED
    };
    let round = |value: Decimal| value.round_dp(0);
    let mut ids = Vec::new();
    for step in 0..3 {
        let mut invoice = Invoice::new(
            InvoiceKind::Partial,
            format!("AR-{}", step + 1),
            ["28.02.2027", "30.04.2027", "30.06.2027"][step],
        );
        invoice.received = ["03.03.2027", "05.05.2027", "02.07.2027"][step].into();
        invoice.vat = VatRule::Rate(Decimal::from(19));
        for (index, (id, quantity, lump)) in positions.iter().enumerate() {
            if *lump {
                continue;
            }
            let claimed = round(*quantity * share(index, step));
            if claimed.is_zero() {
                continue;
            }
            invoice.items.insert(
                *id,
                InvoiceItem {
                    claimed_quantity: Some(claimed),
                    ..InvoiceItem::default()
                },
            );
        }
        ids.push(project.billing_mut().add_invoice(invoice));
    }
    let billing = project.billing_mut();
    // AR-1: one quantity reduced with a note.
    if let Some((id, quantity, _)) = positions.get(1) {
        let item = billing.invoice_mut(ids[0]).expect("invoice").item_mut(*id);
        let claimed = item.claimed_quantity.unwrap_or_default();
        item.checked_quantity = Some((claimed - Decimal::ONE).max(Decimal::ZERO).min(*quantity));
        item.note = "1 Modul noch nicht montiert".into();
    }
    // AR-2: progress 25 % on the commissioning lump sum, takeoff for the cable.
    if let Some((id, _, _)) = positions.iter().find(|(_, _, lump)| *lump) {
        billing
            .invoice_mut(ids[1])
            .expect("invoice")
            .item_mut(*id)
            .progress = Some(Progress {
            factor: Decimal::new(25, 2),
            note: "Absprache mit AN vom 28.04.2027: 1/4 der Anlagen in Betrieb".into(),
        });
    }
    let cable = positions
        .iter()
        .map(|(id, quantity, _)| (*id, *quantity))
        .max_by_key(|(_, quantity)| *quantity)
        .map(|(id, _)| id);
    if let Some(cable) = cable {
        billing.takeoff_mut(cable).extend([
            TakeoffLine {
                label: "UG Technikzentrale, Strang 1".into(),
                formula: "4*85,5".into(),
                invoice: Some(ids[1]),
            },
            TakeoffLine {
                label: "EG Flur Nord".into(),
                formula: "2*(64+38,5)".into(),
                invoice: Some(ids[1]),
            },
            TakeoffLine {
                label: "1. OG, Klassen 1.01–1.08".into(),
                formula: "8*42".into(),
                invoice: Some(ids[1]),
            },
        ]);
    }
    // AR-3: the change order positions.
    let third = billing.invoice_mut(ids[2]).expect("invoice");
    third.items.insert(
        device,
        InvoiceItem {
            claimed_quantity: Some(Decimal::from(4)),
            ..InvoiceItem::default()
        },
    );

    // TR-4: Teilrechnung as increment, released on its own.
    let mut partial = Invoice::new(InvoiceKind::Partial, "TR-4", "31.08.2027");
    partial.received = "02.09.2027".into();
    partial.vat = VatRule::Rate(Decimal::from(19));
    partial.quantities = Mode::Increment;
    partial.release = Mode::Increment;
    partial.items.insert(
        device,
        InvoiceItem {
            claimed_quantity: Some(Decimal::from(2)),
            ..InvoiceItem::default()
        },
    );
    partial.items.insert(
        commissioning,
        InvoiceItem {
            progress: Some(Progress {
                factor: Decimal::new(5, 1),
                note: "Raumbediengeräte zur Hälfte in Betrieb".into(),
            }),
            ..InvoiceItem::default()
        },
    );
    billing.add_invoice(partial);

    // Schlussrechnung: everything, a price above the contract, a position
    // without contract.
    let mut last = Invoice::new(InvoiceKind::Final, "SR-1", "30.11.2027");
    last.received = "03.12.2027".into();
    last.vat = VatRule::Rate(Decimal::from(19));
    for (index, (id, quantity, lump)) in positions.iter().enumerate() {
        let mut item = InvoiceItem {
            claimed_quantity: Some(*quantity),
            ..InvoiceItem::default()
        };
        if *lump {
            item.claimed_quantity = Some(Decimal::ONE);
        }
        if index == 3 {
            item.claimed_unit_price = Some(Decimal::new(47900, 2));
        }
        last.items.insert(*id, item);
    }
    last.items.insert(
        device,
        InvoiceItem {
            claimed_quantity: Some(Decimal::from(6)),
            ..InvoiceItem::default()
        },
    );
    last.items.insert(
        commissioning,
        InvoiceItem {
            claimed_quantity: Some(Decimal::ONE),
            ..InvoiceItem::default()
        },
    );
    last.items.insert(
        without_contract,
        InvoiceItem {
            claimed_quantity: Some(Decimal::from(40)),
            claimed_unit_price: Some(Decimal::new(1250, 2)),
            ..InvoiceItem::default()
        },
    );
    billing.add_invoice(last);
    project
}

fn build(sections: &[Section], bidders: usize, copies: usize) -> Project {
    build_named("Demo Preisspiegel GA", sections, bidders, copies)
}

fn build_named(name: &str, sections: &[Section], bidders: usize, copies: usize) -> Project {
    let mask = OzMask::parse("11.22.PPPP").expect("valid mask");
    let mut lv = Lv::new("LV 480 Gebäudeautomation", mask);
    for copy in 0..copies {
        for (title, subs) in sections {
            let title = if copies > 1 {
                format!("{title} – Bauteil {}", copy + 1)
            } else {
                title.to_string()
            };
            lv.push_group(LvGroup::new(0, "", title));
            for (sub, items) in subs.iter() {
                lv.push_group(LvGroup::new(1, "", *sub));
                for entry in items.iter() {
                    let mut position = Position::new(
                        "",
                        entry.text,
                        Decimal::from(entry.quantity),
                        entry.unit,
                        Some(Decimal::new(entry.cents, 2)),
                    );
                    position.set_kind(entry.kind);
                    if entry.unit == "psch" {
                        position.set_lump_sum(true);
                    }
                    if entry.unit == "h" {
                        position.set_hourly(true);
                    }
                    lv.push_position(position);
                }
            }
        }
    }
    lv.renumber();

    let mut project = Project::new(
        ProjectName::parse(name).expect("valid name"),
        "2026-11-17T18:00:00",
        lv,
    );
    project.info_mut().label = "Neubau Schulzentrum – Gebäudeautomation".to_string();

    for number in 0..bidders {
        let template = &BIDDERS[number % BIDDERS.len()];
        let name = if number < BIDDERS.len() {
            template.name.to_string()
        } else {
            format!("{} (Los {})", template.name, number + 1)
        };
        let bid = bid_for(&project, number, template, name);
        project.add_bid(bid);
    }
    project
}

fn bid_for(project: &Project, number: usize, template: &Bidder, name: String) -> Bid {
    let mut address = Address::named(name);
    address.street = template.street.to_string();
    address.postal_code = template.postal_code.to_string();
    address.city = template.city.to_string();
    address.phone = format!("0123 {}", 4400 + number * 17);
    address.email = format!("angebote@bieter{}.example", number + 1);

    let source = format!("Angebot_Bieter{}.X84", number + 1);
    let mut bid = Bid::new(address, source);
    bid.set_bid_date(Some(format!("{:02}.11.2026", 9 + number)));

    let level = template.level + 7 * (number / BIDDERS.len()) as i64;
    let mut scatter = Scatter(template.seed + number as u64 * 101);
    let positions: Vec<Position> = project.lv().positions().cloned().collect();
    for (index, position) in positions.iter().enumerate() {
        let planner = position.unit_price().unwrap_or_default();
        let planner_cents = (planner * Decimal::ONE_HUNDRED)
            .round()
            .to_string()
            .parse::<i64>()
            .unwrap_or(0);
        let mut factor = level + scatter.next_permille(110);
        if number == 1 && index % 17 == 5 {
            factor = factor * 16 / 10;
        }
        if number == 3 && index % 19 == 8 {
            factor = factor * 55 / 100;
        }
        let unit_cents = (planner_cents * factor / 1000).max(1);
        let unit_price = Decimal::new(unit_cents, 2);
        let mut stated = position.total_for(Some(unit_price)).to_decimal();

        if number == 2 && (index == 3 || index == 26 || index == 33) {
            continue;
        }
        if number == 2 && index == 12 {
            stated += Decimal::from(100);
        }
        bid.set_price(
            position.id(),
            BidPrice {
                unit_price: Some(unit_price),
                stated_total: Some(stated),
                not_offered: false,
            },
        );
    }

    match number % BIDDERS.len() {
        1 => bid.set_discount(Discount::Percent(Decimal::from(3))),
        2 => bid.set_unknown_ordinals(vec!["03.02.0090".to_string(), "04.01.0010".to_string()]),
        _ => {}
    }
    bid
}

/// (Kostengruppe, Titel, [(Kurztext, Menge, Einheit, EP in Euro)])
type TradeGroup = (
    &'static str,
    &'static str,
    &'static [(&'static str, i64, &'static str, i64)],
);

fn trade_lv(number: &str, title: &str, groups: &[TradeGroup]) -> Lv {
    let mut lv = Lv::new(title, OzMask::parse("11.22.PPPP").expect("valid mask"));
    lv.set_number(number);
    for (code, group_title, items) in groups {
        let mut group = LvGroup::new(0, "", *group_title);
        group.set_cost_group(Some((*code).to_string()));
        lv.push_group(group);
        for (text, quantity, unit, euro) in items.iter() {
            lv.push_position(Position::new(
                "",
                *text,
                Decimal::from(*quantity),
                *unit,
                Some(Decimal::from(*euro)),
            ));
        }
    }
    lv.renumber();
    lv
}

/// A bid with every planner's price times `permille` / 1000.
fn trade_bid(lv: &Lv, name: &str, permille: i64, discount: Option<i64>) -> Bid {
    let mut bid = Bid::new(Address::named(name), format!("{name}.X84"));
    for position in lv.positions() {
        let price = position.unit_price().unwrap_or_default() * Decimal::from(permille)
            / Decimal::from(1000);
        bid.set_unit_price(position.id(), Some(price.round_dp(2)));
    }
    if let Some(percent) = discount {
        bid.set_discount(Discount::Percent(Decimal::from(percent)));
    }
    bid
}

const HEATING: &[TradeGroup] = &[
    (
        "421",
        "Wärmeerzeugungsanlagen",
        &[
            ("Wärmepumpe Luft/Wasser 60 kW", 1, "St", 68_500),
            ("Pufferspeicher 1.500 l", 1, "St", 4_200),
        ],
    ),
    (
        "422",
        "Wärmeverteilnetze",
        &[
            ("Rohrleitung Stahl DN 25–50, gedämmt", 420, "m", 58),
            ("Umwälzpumpe Heizkreis", 6, "St", 1_150),
        ],
    ),
    (
        "423",
        "Raumheizflächen",
        &[
            ("Heizkörper mit Thermostatventil", 85, "St", 640),
            ("Fußbodenheizung", 650, "m2", 62),
        ],
    ),
];

const VENTILATION: &[TradeGroup] = &[(
    "431",
    "Lüftungsanlagen",
    &[
        ("RLT-Gerät 8.000 m³/h mit WRG", 1, "St", 78_000),
        ("Luftkanäle verzinkt", 1_250, "m2", 64),
        ("Brandschutzklappe", 24, "St", 780),
        ("Luftauslass", 96, "St", 145),
    ],
)];

const SANITARY: &[TradeGroup] = &[
    (
        "411",
        "Abwasseranlagen",
        &[("Abwasserleitung PP DN 50–150", 260, "m", 55)],
    ),
    (
        "412",
        "Wasseranlagen",
        &[
            ("Trinkwasserleitung Edelstahl", 380, "m", 48),
            ("Sanitärobjekt komplett", 42, "St", 690),
        ],
    ),
];

/// Kostenschätzung der Gewerke (2. Ebene) als Kosten-LV, Menge × Kennwert.
const ESTIMATE_TRADES: &[TradeGroup] = &[
    (
        "410",
        "Abwasser-, Wasser-, Gasanlagen",
        &[("Sanitär je m² BGF", 2_400, "m2", 30)],
    ),
    (
        "420",
        "Wärmeversorgungsanlagen",
        &[("Heizung je kW Heizlast", 180, "kW", 1_150)],
    ),
    (
        "430",
        "Raumlufttechnische Anlagen",
        &[("Lüftung je m³/h Zuluft", 8_000, "m3/h", 21)],
    ),
];

/// Kostenberechnung der Gewerke (3. Ebene) als Kosten-LV.
const CALCULATION_TRADES: &[TradeGroup] = &[
    (
        "411",
        "Abwasseranlagen",
        &[("Abwasserleitungen", 260, "m", 52)],
    ),
    (
        "412",
        "Wasseranlagen",
        &[
            ("Trinkwasserleitungen", 380, "m", 45),
            ("Sanitärobjekte", 42, "St", 650),
        ],
    ),
    (
        "421",
        "Wärmeerzeugungsanlagen",
        &[("Wärmepumpe", 60, "kW", 1_050)],
    ),
    (
        "422",
        "Wärmeverteilnetze",
        &[("Rohrleitungen", 420, "m", 55), ("Pumpen", 6, "St", 1_100)],
    ),
    (
        "423",
        "Raumheizflächen",
        &[("Heizflächen", 1_450, "m2 NGF", 52)],
    ),
    (
        "431",
        "Lüftungsanlagen",
        &[("Lüftung je m³/h", 8_000, "m3/h", 19)],
    ),
];

/// Kostenverfolgung (Handbuch, Tests, Bilder; erfunden): the GA of the
/// billing demo (Auftrag, Nachtrag N01, Abrechnung bis zur Schlussrechnung,
/// KG 481/483/484) plus Heizung (drei Angebote, noch nicht vergeben),
/// Lüftung (beauftragt, offener Nachtrag in der Prognose, 1. Abschlag) and
/// Sanitär (nur Planerpreise); Kostenschätzung und -berechnung der GA nach
/// Datenpunkten, der übrigen Gewerke als Kosten-LV; Budget je KG,
/// Erläuterungen und ein festgehaltener Kostenstand vor dem Nachtrag der
/// Lüftung.
pub fn tracking_demo_project() -> Project {
    use proava_domain::cost_demo::demo_cost_estimate;
    use proava_domain::{
        BudgetKind, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus, CostStage,
        Invoice, InvoiceItem, InvoiceKind, ItemChange, LvPurpose, Mode, Money, VatRule,
    };

    let mut project = billing_demo_project()
        .with_name(ProjectName::parse("Demo Kostenverfolgung").expect("valid name"));
    project.info_mut().label = "Neubau Schulzentrum Musterstadt – Technische Anlagen".to_string();
    {
        let lv = project.lv_mut();
        lv.set_number("480");
        lv.set_title("Gebäudeautomation");
        for index in 0..lv.rows().len() {
            if let Some(LvRow::Group(group)) = lv.row_mut(index) {
                let code = match group.short_text() {
                    "Automationsstationen" | "Nachträge" => Some("481"),
                    "Feldgeräte" => Some("484"),
                    "Software und Dienstleistungen" => Some("483"),
                    _ => None,
                };
                if let Some(code) = code {
                    group.set_cost_group(Some(code.into()));
                }
            }
        }
    }

    // Heizung: three bids, not awarded yet.
    let heating = trade_lv("420", "Heizung", HEATING);
    let bids = [
        trade_bid(&heating, "Wärme & Klima Nord GmbH", 1_064, None),
        trade_bid(&heating, "Heizungsbau Süd KG", 1_120, Some(2)),
        trade_bid(&heating, "Haustechnik West", 1_185, None),
    ];
    let heating = project.add_lv(heating);
    for bid in bids {
        project.add_bid_to(heating, bid);
    }

    // Lüftung: awarded and taken over.
    let ventilation_lv = trade_lv("430", "Lüftung", VENTILATION);
    let ventilation_bid = trade_bid(&ventilation_lv, "Luftbau Musterstadt GmbH", 968, Some(3));
    let ventilation = project.add_lv(ventilation_lv);
    let bid = project.add_bid_to(ventilation, ventilation_bid);
    project.set_active_lv(ventilation);
    project.award_bid(bid, "15.09.2026").expect("bid exists");
    project.take_over_contract().expect("awarded");

    // Sanitär: only the planner's prices (Kostenvoranschlag).
    project.add_lv(trade_lv("410", "Sanitär", SANITARY));

    // Kostenermittlungen: GA after data points, the other trades as Kosten-LVs.
    let mut estimate = demo_cost_estimate(CostStage::Estimate);
    estimate.date = "15.06.2026".into();
    project.add_cost_estimate(estimate);
    let mut calculation = demo_cost_estimate(CostStage::Calculation);
    calculation.date = "12.10.2026".into();
    let calculation_total = calculation.result().total;
    project.add_cost_estimate(calculation);
    let mut trades_estimate = trade_lv("KS", "Kostenschätzung Technik", ESTIMATE_TRADES);
    trades_estimate.set_purpose(LvPurpose::Cost(CostStage::Estimate));
    project.add_lv(trades_estimate);
    let mut trades_calculation = trade_lv("KB", "Kostenberechnung Technik", CALCULATION_TRADES);
    trades_calculation.set_purpose(LvPurpose::Cost(CostStage::Calculation));
    project.add_lv(trades_calculation);

    // Budget per KG (Kostenrahmen des Bauherrn), Erläuterungen.
    {
        let tracking = project.cost_tracking_mut();
        tracking.budget_kind = BudgetKind::Ceiling;
        tracking.budget_date = "01.06.2026".into();
        tracking.budget_note = "Kostenrahmen des Bauherrn".into();
        let thousand = |euro: i64| Money::from_cents(euro * 100_000);
        tracking.set_budget("410", thousand(75));
        tracking.set_budget("420", thousand(185));
        tracking.set_budget("430", thousand(175));
        let ga = calculation_total.cents() * 95 / 100 / 100_000 * 100_000;
        tracking.set_budget("480", Money::from_cents(ga));
        tracking.set_note(
            "420",
            "Angebote über der Kostenberechnung (Wärmepumpe). Maßnahme: Variante Sole/Wasser mit Förderung prüfen, Vergabe erst nach Abstimmung mit dem Bauherrn.",
        );
        tracking.set_note(
            "480",
            "Nachtrag N01 Raumbediengeräte auf Wunsch des Bauherrn; Mehrkosten aus der Reserve KG 400.",
        );
    }

    // The state before the change order of the Lüftung.
    let snapshot =
        proava_domain::record_snapshot(&project, "01.10.2026", "Kostenberechnung abgestimmt");
    project.cost_tracking_mut().snapshots.push(snapshot);

    // Lüftung: change order open in the forecast, first invoice.
    project.set_active_lv(ventilation);
    let flaps = project
        .lv()
        .positions()
        .find(|position| position.short_text() == "Brandschutzklappe")
        .map(|position| position.id());
    let channels = project
        .lv()
        .positions()
        .find(|position| position.short_text() == "Luftkanäle verzinkt")
        .map(|position| position.id());
    let unit = project
        .lv()
        .positions()
        .find(|position| position.short_text().starts_with("RLT-Gerät"))
        .map(|position| position.id());
    if let (Some(flaps), Some(channels), Some(unit)) = (flaps, channels, unit) {
        let mut order = ChangeOrder::new(ChangeOrderId(0), 1, "Zusätzliche Brandschutzklappen");
        order.status = ChangeOrderStatus::Offered;
        order.offer_number = "NA-430-01".into();
        order.items = vec![ChangeOrderItem {
            position: flaps,
            change: ItemChange::Quantity,
            offered_quantity: Decimal::from(8),
            ..ChangeOrderItem::default()
        }];
        project.add_change_order(order);
        let mut invoice = Invoice::new(InvoiceKind::Partial, "AR-1", "30.11.2026");
        invoice.received = "02.12.2026".into();
        invoice.vat = VatRule::Rate(Decimal::from(19));
        invoice.quantities = Mode::Cumulative;
        invoice.release = Mode::Cumulative;
        for (row, quantity) in [(unit, 1), (channels, 620), (flaps, 12)] {
            invoice.items.insert(
                row,
                InvoiceItem {
                    claimed_quantity: Some(Decimal::from(quantity)),
                    ..InvoiceItem::default()
                },
            );
        }
        project.billing_mut().add_invoice(invoice);
    }
    project.set_active_lv(0);
    project
}

#[cfg(test)]
mod tests {
    use proava_domain::{ComparisonOptions, PriceComparison};

    use super::{demo_project, full_demo_project, large_demo_project};

    /// The full demo holds every kind of data of the 18.11. (Affentest) and
    /// keeps LV 480 as in the demo: rows, sum, bids.
    #[test]
    fn full_demo_holds_everything_and_keeps_lv_480() {
        use proava_domain::{LvRow, Position};
        let project = full_demo_project();
        let demo = demo_project();
        assert_eq!(project.active_lv(), 0);
        assert_eq!(project.lv().rows().len(), demo.lv().rows().len());
        assert_eq!(project.lv().net_total(), demo.lv().net_total());
        assert_eq!(project.bids().len(), 4);
        let tender: Vec<_> = project
            .lvs()
            .filter(|lv| lv.purpose().is_tender())
            .collect();
        let cost = project.lvs().filter(|lv| !lv.purpose().is_tender()).count();
        assert_eq!(
            (tender.len(), cost),
            (2, 1),
            "zwei Ausschreibungs-LVs, ein Kosten-LV"
        );
        let positions = || tender.iter().flat_map(|lv| lv.positions());
        assert_eq!(
            positions()
                .filter(|position| position.is_text_locked())
                .count(),
            2,
            "STLB-Bau mit gesperrtem Langtext und Einheit"
        );
        assert!(positions().any(Position::is_surcharge), "Zuschlagsposition");
        assert!(
            positions().any(|position| position
                .long_text()
                .blocks()
                .iter()
                .any(|block| block.list_kind().is_some_and(|(_, level)| level >= 1))),
            "Liste mit Unterpunkten"
        );
        let rows = || tender.iter().flat_map(|lv| lv.rows());
        assert!(rows()
            .any(|row| matches!(row, LvRow::Group(group) if group.lump_sum_total().is_some())));
        assert!(rows().any(|row| matches!(row, LvRow::Note(note) if note.is_closing())));
        assert!(!project.cost_estimates().is_empty());
    }

    #[test]
    fn demo_has_the_promised_shape() {
        let project = demo_project();
        assert_eq!(project.bids().len(), 4);
        assert!(project.lv().position_count() >= 40);
        let lv = project.lv();
        let third = &project.bids()[2];
        assert_eq!(third.missing_positions(lv).len(), 3);
        assert_eq!(third.arithmetic_errors(lv).len(), 1);
        assert_eq!(third.unknown_ordinals().len(), 2);

        let comparison = PriceComparison::build(lv, project.bids(), ComparisonOptions::default());
        let mut ranks: Vec<usize> = comparison.bids.iter().map(|bid| bid.rank).collect();
        ranks.sort();
        assert_eq!(ranks, vec![1, 2, 3, 4]);
    }

    #[test]
    fn large_demo_scales() {
        let project = large_demo_project(13, 8);
        assert!(project.lv().position_count() > 500);
        assert_eq!(project.bids().len(), 8);
    }

    /// GA als Kurztext: KG 480 = 118.200 € + 328.400 € through the
    /// Aufteilungszeilen only; LV prüfen has nothing to say.
    #[test]
    fn ga_kurztext_demo_counts_the_points_once() {
        let project = super::ga_kurztext_demo_project();
        let lv = project.lv();
        let sums = project.cost_sums(proava_domain::CostStage::Calculation, 2);
        assert_eq!(sums["480"], proava_domain::Money::from_cents(44_660_000));
        assert_eq!(lv.split_blocks().len(), 2);
        assert!(
            proava_domain::cost_lv_findings(lv).is_empty(),
            "{:?}",
            proava_domain::cost_lv_findings(lv)
        );
    }
}
