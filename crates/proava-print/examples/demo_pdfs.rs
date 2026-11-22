//! Writes sample PDFs (Angebotsaufforderung, LV mit Preisen, Preisspiegel,
//! Niederschrift, LV mit Kopf-/Fußtext) to `PROAVA_OUT` (default
//! /tmp/proava/proava-print/), for a visual check of the layout.

use std::path::Path;
use std::str::FromStr;

use proava_print::*;
use rust_decimal::Decimal;

/// Optional sample picture (JPEG/PNG) for the long text and the logo.
const SAMPLE_IMAGE_VAR: &str = "PROAVA_DEMO_IMAGE";

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn header(kind: Option<&str>) -> PrintHeader {
    PrintHeader {
        project_name: "Schulzentrum Musterstadt".into(),
        project_title: "Neubau Schulzentrum Parkstraße – Gebäudeautomation".into(),
        client: "Stadt Musterstadt, Gebäudewirtschaft".into(),
        lv_name: "LV 480 Gebäudeautomation".into(),
        date: "17.11.2026".into(),
        author: "Max Muster".into(),
        document_kind: kind.map(Into::into),
        logo: None,
        frame: Default::default(),
        look: Default::default(),
        currency: String::new(),
    }
}

struct Pos {
    oz: &'static str,
    short: &'static str,
    long: Vec<TextBlock>,
    qty: &'static str,
    unit: &'static str,
    ep: &'static str,
    kinds: Vec<PositionKind>,
}

fn pos(
    oz: &'static str,
    short: &'static str,
    qty: &'static str,
    unit: &'static str,
    ep: &'static str,
) -> Pos {
    Pos {
        oz,
        short,
        long: Vec::new(),
        qty,
        unit,
        ep,
        kinds: Vec::new(),
    }
}

fn sample_positions(image: Option<Vec<u8>>) -> Vec<(&'static str, &'static str, Vec<Pos>)> {
    let mut as_long = vec![
        TextBlock::Paragraph {
            runs: vec![TextRun {
                text: "Automationsstation – Leistungsumfang".into(),
                bold: true,
                font: Some(FONT_SERIF.into()),
                size_pt: Some(13.0),
                ..TextRun::default()
            }],
            align: TextAlign::Center,
        },
        TextBlock::Paragraph {
            runs: vec![
                TextRun::plain("Adresse im Netz: "),
                TextRun {
                    text: "BACnet/IP 192.168.10.21".into(),
                    font: Some(FONT_MONO.into()),
                    size_pt: Some(8.0),
                    ..TextRun::default()
                },
            ],
            align: TextAlign::Right,
        },
        TextBlock::text(
            "Liefern, montieren und betriebsfertig anschließen einer frei programmierbaren \
             Automationsstation (BACnet B-BC) für den Einbau im Informationsschwerpunkt, \
             bestehend aus:",
        ),
        TextBlock::bullet("Prozessor- und Kommunikationsmodul mit BACnet/IP, 2 × Ethernet"),
        TextBlock::bullet("Ein- und Ausgabemodule nach Datenpunktliste, steckbar"),
        TextBlock::ListItem {
            level: 1,
            marker: None,
            runs: vec![TextRun::plain("Reserve je Signalart mindestens 15 %")],
            align: Default::default(),
        },
        TextBlock::bullet("Netzteil 24 V AC/DC, Hutschienenmontage"),
        TextBlock::paragraph(vec![
            TextRun::plain("Die Station ist mit der "),
            TextRun::bold("vollständigen Anwendersoftware"),
            TextRun::plain(" nach Funktionsbeschreibung zu liefern. "),
            TextRun {
                text: "Fabrikat und Typ sind vom Bieter anzugeben.".into(),
                italic: true,
                ..TextRun::default()
            },
        ]),
        TextBlock::table(
            true,
            vec![
                vec!["Signalart".into(), "Anzahl".into(), "Bemerkung".into()],
                vec![
                    "Digitaleingänge (DI)".into(),
                    "48".into(),
                    "potenzialfrei".into(),
                ],
                vec![
                    "Digitalausgänge (DO)".into(),
                    "24".into(),
                    "Relais 230 V".into(),
                ],
                vec![
                    "Analogeingänge (AI)".into(),
                    "32".into(),
                    "Pt1000 / 0–10 V".into(),
                ],
                vec!["Analogausgänge (AO)".into(), "16".into(), "0–10 V".into()],
            ],
        ),
        TextBlock::paragraph(vec![
            TextRun::plain("Fabrikat: "),
            TextRun {
                text: "………………………………".into(),
                ..TextRun::default()
            },
            TextRun::plain("   Typ: ………………………………"),
        ]),
    ];
    let mut window_long = vec![TextBlock::text(
        "Fensterkontakt als Magnetkontakt, verdeckt eingebaut, für die Überwachung der \
         Kellerfenster (Nachtlüftung und Einbruchmeldung an die GA). Einbausituation siehe Abbildung.",
    )];
    if let Some(data) = image {
        window_long.push(TextBlock::Image(PrintImage {
            data,
            file_name: "Kellerfenster.jpg".into(),
            width_mm: Some(70.0),
            caption: Some("Abb. 1: Einbausituation Kellerfenster".into()),
            clip: None,
        }));
        as_long.insert(
            1,
            TextBlock::text("Hinweis zu Sonderzeichen: Kennzeichen #AS01, *nicht fett*, $Variable$, <BMK>, @Adresse."),
        );
    }

    vec![
        (
            "01",
            "Automationsstationen",
            vec![
                Pos {
                    long: as_long,
                    kinds: vec![],
                    ..pos("01.0010", "Automationsstation BACnet B-BC, modular", "4", "St", "6850")
                },
                Pos {
                    long: vec![TextBlock::text(
                        "Bedien- und Anzeigeeinheit (Touchpanel 7\") in der Schaltschranktür, \
                         Anbindung an die Automationsstation über BACnet/IP.",
                    )],
                    ..pos("01.0020", "Lokale Vorrangbedieneinrichtung, Touchpanel", "4", "St", "1240.5")
                },
                Pos {
                    long: vec![TextBlock::text(
                        "Erweiterung der Automationsstation um ein weiteres E/A-Modul \
                         mit 16 Eingängen, nur bei Bedarf und auf Anordnung der Bauleitung.",
                    )],
                    kinds: vec![PositionKind::Optional],
                    ..pos("01.0030", "E/A-Erweiterungsmodul 16 DI", "2", "St", "385")
                },
                Pos {
                    kinds: vec![PositionKind::LumpSum],
                    long: vec![TextBlock::text(
                        "Programmierung, Parametrierung und Inbetriebnahme aller Automationsstationen \
                         einschließlich Funktionsprüfung und Protokoll. Einbindung der \
                         DDC-Automationsstationen über Feldbus-Kommunikationsschnittstellen \
                         (Modbus-RTU-Gateway-Konfiguration, BACnet-MSTP-Router-Parametrierung) in die \
                         Gebäudeleittechnik-Managementebene-Datenpunktliste-Raumautomations-Erweiterungsmodule.",
                    )],
                    ..pos("01.0040", "Software und Inbetriebnahme Automationsstationen", "1", "psch", "18500")
                },
            ],
        ),
        (
            "02",
            "Feldgeräte",
            vec![
                Pos {
                    long: vec![TextBlock::text(
                        "Kanaltemperaturfühler Pt1000, Tauchrohr 200 mm, Messbereich -50 … +80 °C, \
                         Schutzart IP65, einschließlich Montageflansch.",
                    )],
                    ..pos("02.0010", "Kanaltemperaturfühler Pt1000", "24", "St", "86.4")
                },
                Pos {
                    long: window_long,
                    ..pos("02.0020", "Fensterkontakt Kellerfenster", "12", "St", "54.9")
                },
                Pos {
                    long: vec![TextBlock::text(
                        "Wie Position 02.0020, jedoch als Funkkontakt (EnOcean) mit Empfänger.",
                    )],
                    kinds: vec![PositionKind::Alternative],
                    ..pos("02.0030", "Fensterkontakt Funk (EnOcean)", "12", "St", "79")
                },
                Pos {
                    kinds: vec![PositionKind::Hourly],
                    long: vec![TextBlock::text(
                        "Stundenlohnarbeiten Monteur für Arbeiten, die nicht im LV beschrieben sind, \
                         nur nach vorheriger Anordnung.",
                    )],
                    ..pos("02.0040", "Stundenlohn Monteur", "20", "h", "68")
                },
            ],
        ),
    ]
}

fn sample_lv(image: Option<Vec<u8>>) -> PrintLv {
    let mut items = vec![PrintItem::Note(vec![
        TextBlock::paragraph(vec![TextRun::bold("Vorbemerkungen")]),
        TextBlock::text(
            "Die Preise verstehen sich für die fertige, betriebsbereite Leistung einschließlich \
             aller Nebenleistungen nach VOB/C DIN 18386. Fabrikat- und Typangaben sind in den \
             dafür vorgesehenen Feldern einzutragen.",
        ),
    ])];
    for (oz, text, positions) in sample_positions(image) {
        items.push(PrintItem::Title(PrintTitle {
            lump: false,
            oz: oz.into(),
            label: String::new(),
            text: text.into(),
            total: None,
            items: positions
                .into_iter()
                .map(|p| {
                    let in_total = !p
                        .kinds
                        .iter()
                        .any(|k| matches!(k, PositionKind::Optional | PositionKind::Alternative));
                    PrintItem::Position(PrintPosition {
                        oz: p.oz.into(),
                        short_text: p.short.into(),
                        long_text: p.long,
                        takeoff: Vec::new(),
                        quantity: Some(d(p.qty)),
                        unit: p.unit.into(),
                        unit_price: Some(d(p.ep)),
                        total_price: None,
                        kinds: p.kinds,
                        in_total,
                        price_parts: Vec::new(),
                        remarks: Vec::new(),
                        cost_group: None,
                        work_category: None,
                        surcharge: None,
                    })
                })
                .collect(),
        }));
    }
    PrintLv {
        header: header(None),
        items,
        net_total: None,
        vat_percent: d("19"),
        ..PrintLv::default()
    }
}

fn sample_price_comparison() -> PrintPriceComparison {
    let factors = [d("1.00"), d("0.92"), d("1.11")];
    let bidders = vec![
        PrintBidder {
            name: "Elektro Nord GmbH".into(),
            date: "10.11.2026".into(),
            discount_percent: Some(d("3")),
            ..PrintBidder::default()
        },
        PrintBidder {
            name: "GA-Systeme West AG".into(),
            date: "12.11.2026".into(),
            ..PrintBidder::default()
        },
        PrintBidder {
            name: "Regeltechnik Süd KG".into(),
            date: "13.11.2026".into(),
            discount_amount: Some(d("2500")),
            ..PrintBidder::default()
        },
    ];
    let items = sample_positions(None)
        .into_iter()
        .map(|(oz, text, positions)| {
            PcItem::Title(PcTitle {
                oz: oz.into(),
                text: text.into(),
                totals: Vec::new(),
                estimate: None,
                items: positions
                    .into_iter()
                    .enumerate()
                    .map(|(n, p)| {
                        let in_total = !p.kinds.iter().any(|k| {
                            matches!(k, PositionKind::Optional | PositionKind::Alternative)
                        });
                        PcItem::Position(PcPosition {
                            oz: p.oz.into(),
                            short_text: p.short.into(),
                            quantity: Some(d(p.qty)),
                            unit: p.unit.into(),
                            kinds: p.kinds,
                            in_total,
                            prices: factors
                                .iter()
                                .enumerate()
                                .map(|(b, f)| {
                                    // bidder 2 leaves one position open
                                    if b == 2 && n == 3 {
                                        return BidPrice::default();
                                    }
                                    let skew = Decimal::from(((n + b) % 3) as i64) * d("0.04");
                                    BidPrice {
                                        unit_price: Some(
                                            (d(p.ep) * (f + skew - d("0.04"))).round_dp(2),
                                        ),
                                        ..BidPrice::default()
                                    }
                                })
                                .collect(),
                            estimate: BidPrice::default(),
                            quantity_note: String::new(),
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    PrintPriceComparison {
        header: header(None),
        bidders,
        items,
        vat_percent: d("19"),
        paper: PaperSize::Auto,
        ..PrintPriceComparison::default()
    }
}

fn sample_submission() -> PrintSubmission {
    let bidder = |name: &str, net: &str| SubmissionBidder {
        name: name.into(),
        place: "Musterstadt".into(),
        received: "12.11.2026 09:40".into(),
        status: "rechtzeitig".into(),
        net: Some(d(net)),
        discount: None,
        side_offers: false,
        remark: String::new(),
    };
    PrintSubmission {
        header: header(Some("Niederschrift über die Angebotsöffnung")),
        facts: vec![
            (
                "Ablauf der Angebotsfrist".into(),
                "12.11.2026, 10:00 Uhr".into(),
            ),
            ("Ort".into(), "Vergabestelle, Raum 101".into()),
        ],
        bidders: vec![
            bidder("Regeltechnik Rhein-Ruhr GmbH", "113900.22"),
            bidder("Müller Gebäudeautomation GmbH", "132701.37"),
        ],
        vat_percent: d("19"),
        notes: vec!["Die Angebote waren verschlossen und unversehrt.".into()],
        signatures: vec!["Verhandlungsleiter".into(), "Schriftführer".into()],
        header_text: String::new(),
        footer_text: String::new(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::var("PROAVA_OUT").unwrap_or_else(|_| "/tmp/proava/proava-print".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;
    let image_path = std::env::var(SAMPLE_IMAGE_VAR).unwrap_or_default();
    let image = std::fs::read(&image_path).ok();
    if image.is_none() {
        eprintln!("Kein Beispielbild ({SAMPLE_IMAGE_VAR}), PDF ohne Bild");
    }
    let lv = sample_lv(image.clone());
    // Variant with logo (the sample picture) and free header/footer text.
    let mut with_logo = lv.clone();
    with_logo.header.logo = image.map(|data| PrintImage::new(data, "logo.jpg"));

    let files = [
        (
            "Angebotsaufforderung.pdf",
            render_lv_pdf(&lv, LvPrintMode::Blank)?,
        ),
        (
            "LV_mit_Preisen.pdf",
            render_lv_pdf(&lv, LvPrintMode::Priced)?,
        ),
        (
            "Preisspiegel.pdf",
            render_price_comparison_pdf(&sample_price_comparison())?,
        ),
        (
            "Niederschrift.pdf",
            render_submission_pdf(&sample_submission())?,
        ),
        ("LV_Kopf_Fuss_eigen.pdf", {
            let mut own = with_logo.clone();
            own.header.frame = PrintFrame {
                header: [
                    "{titel}".into(),
                    "{dokumentart}\nVorabzug".into(),
                    "Nr. {projektnummer}".into(),
                ],
                footer: [
                    "{datei}".into(),
                    String::new(),
                    "Seite {seite} von {seiten}".into(),
                ],
                logo: FrameLogo::Left,
                values: vec![
                    ("projektnummer".into(), "VG-2026-014".into()),
                    ("datei".into(), "LV_480_bepreist.pdf".into()),
                ],
                ..PrintFrame::default()
            };
            render_lv_pdf(&own, LvPrintMode::Priced)?
        }),
        (
            "LV_mit_Kopftext.pdf",
            render_lv_pdf_with(
                &with_logo,
                LvPrintMode::Priced,
                &PrintOptions {
                    header_text: "Vorabzug – nicht zur Ausführung".into(),
                    footer_text: "Aufgestellt: Planungsbüro Muster".into(),
                    ..PrintOptions::default()
                },
            )?,
        ),
    ];
    for (name, pdf) in files {
        let path = out.join(name);
        std::fs::write(&path, pdf)?;
        println!("{}", path.display());
    }
    Ok(())
}
