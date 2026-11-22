use std::str::FromStr;

use rust_decimal::Decimal;
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;

use super::*;

mod price_comparison_layout;
mod table_grid;

/// A straight line on a page (mm from the top left) with its width in pt.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RuledLine {
    pub page: usize,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub width: f64,
}

impl RuledLine {
    pub fn vertical(&self) -> bool {
        (self.x0 - self.x1).abs() < 0.01 && (self.y0 - self.y1).abs() > 0.01
    }

    /// A vertical line that runs through the height `y` (mm).
    pub fn crosses(&self, y: f64) -> bool {
        self.vertical() && self.y0.min(self.y1) < y && self.y0.max(self.y1) > y
    }
}

/// Every stroked straight line (table rules, grid lines) of the document.
pub(crate) fn ruled_lines(document: &PagedDocument) -> Vec<RuledLine> {
    fn walk(frame: &Frame, dx: f64, dy: f64, page: usize, out: &mut Vec<RuledLine>) {
        for (point, item) in frame.items() {
            let (x, y) = (dx + point.x.to_mm(), dy + point.y.to_mm());
            match item {
                FrameItem::Group(group) => walk(
                    &group.frame,
                    x + group.transform.tx.to_mm(),
                    y + group.transform.ty.to_mm(),
                    page,
                    out,
                ),
                FrameItem::Shape(shape, _) => {
                    if let (typst::visualize::Geometry::Line(to), Some(stroke)) =
                        (&shape.geometry, &shape.stroke)
                    {
                        out.push(RuledLine {
                            page,
                            x0: x,
                            y0: y,
                            x1: x + to.x.to_mm(),
                            y1: y + to.y.to_mm(),
                            width: stroke.thickness.to_pt(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for (page, p) in document.pages().iter().enumerate() {
        walk(&p.frame, 0.0, 0.0, page, &mut out);
    }
    out
}

/// Text runs with page, horizontal extent and baseline height in mm.
pub(crate) fn texts_at(document: &PagedDocument) -> Vec<(usize, String, f64, f64, f64)> {
    fn walk(
        frame: &Frame,
        dx: f64,
        dy: f64,
        page: usize,
        out: &mut Vec<(usize, String, f64, f64, f64)>,
    ) {
        for (point, item) in frame.items() {
            let (x, y) = (dx + point.x.to_mm(), dy + point.y.to_mm());
            match item {
                FrameItem::Group(group) => walk(
                    &group.frame,
                    x + group.transform.tx.to_mm(),
                    y + group.transform.ty.to_mm(),
                    page,
                    out,
                ),
                FrameItem::Text(t) => {
                    out.push((page, t.text.to_string(), x, x + t.width().to_mm(), y))
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for (page, p) in document.pages().iter().enumerate() {
        walk(&p.frame, 0.0, 0.0, page, &mut out);
    }
    out
}

/// Distinct x positions (0.1 mm) of the vertical lines through height `y`
/// on `page`, only lines of `width` pt (± 0.05).
pub(crate) fn columns_at(lines: &[RuledLine], page: usize, y: f64, width: f64) -> Vec<i64> {
    let mut xs: Vec<i64> = lines
        .iter()
        .filter(|l| l.page == page && l.crosses(y) && (l.width - width).abs() < 0.05)
        .map(|l| (l.x0 * 10.0).round() as i64)
        .collect();
    xs.sort_unstable();
    xs.dedup();
    xs
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, 4, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let pixels: Vec<u8> = (0..8).flat_map(|i| [i * 30, 120, 200 - i * 20]).collect();
        writer.write_image_data(&pixels).unwrap();
    }
    out
}

const NASTY: &str = r#"Preis #1 *fett* _unter_ @ref $x^2$ <tag> [klammer] \n // kein Kommentar = ä ö ü ß € "Zitat" #let x = 1"#;

fn sample_lv() -> PrintLv {
    let mut image = PrintImage::new(tiny_png(), "skizze.png");
    image.width_mm = Some(40.0);
    image.caption = Some("Skizze #1 <a>".into());
    PrintLv {
        header: PrintHeader {
            project_name: "Test #Projekt".into(),
            project_title: "Neubau $Halle$".into(),
            client: "Bauherr *GmbH*".into(),
            lv_name: "LV 480 Gebäudeautomation".into(),
            date: "17.11.2026".into(),
            author: "Max Muster".into(),
            document_kind: None,
            logo: Some(PrintImage::new(tiny_png(), "logo.png")),
            frame: Default::default(),
            look: Default::default(),
            currency: String::new(),
        },
        items: vec![
            PrintItem::Note(vec![TextBlock::text(NASTY)]),
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: "01".into(),
                label: String::new(),
                text: "Automationsstationen".into(),
                total: None,
                items: vec![
                    PrintItem::Position(PrintPosition {
                        oz: "01.0010".into(),
                        short_text: NASTY.into(),
                        long_text: vec![
                            TextBlock::paragraph(vec![
                                TextRun::plain("Normal "),
                                TextRun::bold("fett "),
                                TextRun {
                                    text: "kursiv unterstrichen".into(),
                                    italic: true,
                                    underline: true,
                                    ..TextRun::default()
                                },
                            ]),
                            TextBlock::bullet("Punkt *eins*"),
                            TextBlock::ListItem {
                                level: 1,
                                marker: None,
                                runs: vec![TextRun::plain("Unterpunkt _zwei_")],
                                align: Default::default(),
                            },
                            TextBlock::table(
                                true,
                                vec![vec!["Typ".into(), "Wert".into()], vec!["#a".into()]],
                            ),
                            TextBlock::Image(image),
                            TextBlock::Image(PrintImage::new(b"kein Bild".to_vec(), "kaputt.png")),
                        ],
                        takeoff: Vec::new(),
                        quantity: Some(d("2")),
                        unit: "St".into(),
                        unit_price: Some(d("1234.5")),
                        total_price: None,
                        kinds: vec![PositionKind::Base],
                        in_total: true,
                        price_parts: Vec::new(),
                        remarks: Vec::new(),
                        cost_group: None,
                        work_category: None,
                        surcharge: None,
                    }),
                    PrintItem::Title(PrintTitle {
                        lump: false,
                        oz: "01.01".into(),
                        label: String::new(),
                        text: "Untertitel".into(),
                        total: Some(d("5")),
                        items: vec![PrintItem::Note(vec![TextBlock::text("leer")])],
                    }),
                    PrintItem::Position(PrintPosition {
                        oz: "01.0020".into(),
                        short_text: "Bedarf".into(),
                        quantity: Some(d("1")),
                        unit: "psch".into(),
                        unit_price: Some(d("100")),
                        kinds: vec![PositionKind::Optional],
                        in_total: false,
                        ..PrintPosition::default()
                    }),
                ],
            }),
        ],
        ..PrintLv::default()
    }
}

fn collect(frame: &Frame, text: &mut String, images: &mut usize) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => collect(&group.frame, text, images),
            FrameItem::Text(t) => {
                text.push_str(&t.text);
                text.push(' ');
            }
            FrameItem::Image(..) => *images += 1,
            _ => {}
        }
    }
}

fn inspect(document: &PagedDocument) -> (String, usize) {
    let mut text = String::new();
    let mut images = 0;
    for page in document.pages() {
        collect(&page.frame, &mut text, &mut images);
    }
    (text, images)
}

#[test]
fn lv_pdfs_are_created() {
    let lv = sample_lv();
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        let pdf = render_lv_pdf(&lv, mode).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        assert!(!document.pages().is_empty());
    }
}

#[test]
fn special_characters_are_printed_literally() {
    let document = lv_world(&sample_lv(), LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (text, _) = inspect(&document);
    let squeezed: String = text.split_whitespace().collect();
    for token in [
        "#1",
        "*fett*",
        "_unter_",
        "@ref",
        "$x^2$",
        "<tag>",
        "[klammer]",
        "\\n",
        "//",
        "äöüß€",
        "#letx=1",
        "Test#Projekt",
        "Bauherr*GmbH*",
        "Skizze#1<a>",
    ] {
        assert!(squeezed.contains(token), "{token} fehlt in: {text}");
    }
}

#[test]
fn images_appear_and_broken_images_do_not_fail() {
    let document = lv_world(&sample_lv(), LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (text, images) = inspect(&document);
    assert_eq!(images, 2, "Logo und Bild im Langtext");
    assert!(text.contains("kaputt.png"));
}

#[test]
fn priced_lv_shows_sums() {
    let document = lv_world(&sample_lv(), LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (text, _) = inspect(&document);
    assert!(text.contains("2.469,00"), "GP: {text}");
    assert!(text.contains("2.474,00"), "Titelsumme mit Untertitel");
    assert!(text.contains("(100,00)"), "Bedarfsposition in Klammern");
    assert!(text.contains("2.944,06"), "brutto mit 19 %");
}

/// Währung of the LV in the column heads, sums and the Deckblatt (CSV import
/// in CHF or USD); empty = €.
#[test]
fn priced_lv_prints_in_the_currency_of_the_lv() {
    let mut lv = sample_lv();
    lv.header.currency = "CHF".into();
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (text, _) = inspect(&document);
    assert!(
        text.contains("EP CHF") && text.contains("GP CHF"),
        "Spaltenköpfe: {text}"
    );
    assert!(text.contains("2.944,06 CHF"), "brutto in CHF: {text}");
    assert!(
        !text.contains("GP €") && !text.contains(",06 €"),
        "kein Euro-Zeichen an Beträgen: {text}"
    );
    let (euro, _) = inspect(
        &lv_world(&sample_lv(), LvPrintMode::Priced, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap(),
    );
    assert!(
        euro.contains("EP €") && euro.contains("2.944,06 €"),
        "Vorgabe Euro: {euro}"
    );
}

#[test]
fn blank_lv_has_no_prices() {
    let document = lv_world(&sample_lv(), LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (text, _) = inspect(&document);
    assert!(!text.contains("1.234,50"));
    assert!(text.contains("Angebotsaufforderung"));
}

#[test]
fn quantities_without_needless_zeros_and_lump_sum_asks_only_for_the_gp() {
    let lv = PrintLv {
        items: vec![
            PrintItem::Position(PrintPosition {
                oz: "0010".into(),
                short_text: "Stück".into(),
                quantity: Some(d("1.000")),
                unit: "St".into(),
                unit_price: Some(d("7.25")),
                ..PrintPosition::default()
            }),
            PrintItem::Position(PrintPosition {
                oz: "0020".into(),
                short_text: "Kabel".into(),
                quantity: Some(d("2500.000")),
                unit: "m".into(),
                unit_price: Some(d("1.10")),
                ..PrintPosition::default()
            }),
            PrintItem::Position(PrintPosition {
                oz: "0030".into(),
                short_text: "Inbetriebnahme".into(),
                quantity: Some(d("1")),
                unit: "psch".into(),
                unit_price: Some(d("4321.09")),
                kinds: vec![PositionKind::LumpSum],
                ..PrintPosition::default()
            }),
        ],
        ..PrintLv::default()
    };
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let texts: Vec<String> = placed_texts(&document)
        .into_iter()
        .map(|(_, text, _, _)| text)
        .collect();
    assert!(
        texts.iter().any(|text| text == "1"),
        "Menge 1 statt 1,000: {texts:?}"
    );
    assert!(texts.iter().any(|text| text == "2.500"), "{texts:?}");
    assert!(!texts.iter().any(|text| text.contains(",000")), "{texts:?}");
    assert!(
        texts.iter().any(|text| text == "7,25"),
        "EP der Stückposition"
    );
    let lump = texts
        .iter()
        .filter(|text| text.as_str() == "4.321,09")
        .count();
    assert_eq!(lump, 1, "Pauschal: nur GP, kein EP: {texts:?}");
}

fn bid(ep: &str) -> BidPrice {
    BidPrice {
        unit_price: Some(d(ep)),
        ..BidPrice::default()
    }
}

fn sample_pc(bidders: usize) -> PrintPriceComparison {
    let names = [
        "Alpha #GmbH",
        "Beta $AG",
        "Gamma <KG>",
        "Delta",
        "Epsilon",
        "Zeta",
        "Eta",
        "Theta",
    ];
    let eps = ["100", "90", "120", "95", "110", "105", "98", "115"];
    PrintPriceComparison {
        header: PrintHeader {
            project_name: "Test".into(),
            ..PrintHeader::default()
        },
        bidders: (0..bidders)
            .map(|i| PrintBidder {
                name: names[i].into(),
                discount_percent: (i == 0).then(|| d("20")),
                ..PrintBidder::default()
            })
            .collect(),
        items: vec![PcItem::Title(PcTitle {
            oz: "01".into(),
            text: "Feldgeräte".into(),
            items: vec![PcItem::Position(PcPosition {
                oz: "01.0010".into(),
                short_text: "Fühler *x*".into(),
                quantity: Some(d("10")),
                unit: "St".into(),
                prices: (0..bidders).map(|i| bid(eps[i])).collect(),
                ..PcPosition::default()
            })],
            ..PcTitle::default()
        })],
        ..PrintPriceComparison::default()
    }
}

#[test]
fn cost_groups_pdf_is_created() {
    let report = PrintCostGroups {
        catalog: "DIN 276-18".into(),
        bidders: vec!["Bieter A".into(), "Bieter B".into(), "Bieter C".into()],
        lines: vec![
            PrintCostGroupLine {
                code: "481".into(),
                label: "Automationseinrichtungen".into(),
                positions: 3,
                planner: Decimal::new(1234567, 2),
                bidders: vec![Decimal::new(100, 0); 3],
                ..PrintCostGroupLine::default()
            },
            PrintCostGroupLine {
                code: "480".into(),
                label: "Gebäude- und Anlagenautomation".into(),
                subtotal: true,
                positions: 3,
                planner: Decimal::new(1234567, 2),
                bidders: vec![Decimal::new(100, 0); 3],
            },
        ],
        planner_total: Decimal::new(1234567, 2),
        bidder_totals: vec![Decimal::new(100, 0); 3],
        ..PrintCostGroups::default()
    };
    let pdf = render_cost_groups_pdf(&report).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn price_comparison_pdf_is_created() {
    let pc = sample_pc(3);
    let pdf = render_price_comparison_pdf(&pc).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let document = price_comparison_world(&pc).unwrap().layout().unwrap();
    let (text, _) = inspect(&document);
    assert!(text.contains("1.000,00") && text.contains("900,00") && text.contains("1.200,00"));
    // Alpha: 1000 - 20 % = 800 -> rank 1
    assert!(text.contains("800,00"));
    let page = &document.pages()[0].frame;
    assert!(page.width() > page.height(), "Querformat");
}

#[test]
fn price_comparison_switches_to_a3() {
    let document = price_comparison_world(&sample_pc(5))
        .unwrap()
        .layout()
        .unwrap();
    let width_mm = document.pages()[0].frame.width().to_mm();
    assert!((width_mm - 420.0).abs() < 1.0, "{width_mm}");
}

#[test]
fn price_comparison_marks_and_ranks() {
    let mut files = view::Files::default();
    let data = view::price_comparison(&sample_pc(3), &mut files);
    let row = &data["rows"][1];
    assert_eq!(row["ep"][1]["m"], "low");
    assert_eq!(row["ep"][2]["m"], "high");
    assert_eq!(row["mean_ep"], "103,33");
    assert_eq!(row["min_gp"], "900,00");
    assert_eq!(data["totals"]["rank"][0], 1);
    assert_eq!(data["totals"]["rank"][1], 2);
    assert_eq!(data["totals"]["rank"][2], 3);
    assert_eq!(data["totals"]["after"][0]["v"], "800,00");
}

#[test]
fn price_comparison_flags_missing_prices() {
    let mut pc = sample_pc(3);
    let PcItem::Title(title) = &mut pc.items[0] else {
        unreachable!()
    };
    let PcItem::Position(position) = &mut title.items[0] else {
        unreachable!()
    };
    position.prices[1] = BidPrice::default();
    let mut files = view::Files::default();
    let data = view::price_comparison(&pc, &mut files);
    assert_eq!(data["rows"][1]["ep"][1]["m"], "missing");
    assert_eq!(data["rows"][1]["ep"][0]["m"], "low");
    let document = price_comparison_world(&pc).unwrap().layout().unwrap();
    assert!(inspect(&document).0.contains("fehlt"));
}

#[test]
fn empty_documents_render() {
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        assert!(render_lv_pdf(&PrintLv::default(), mode)
            .unwrap()
            .starts_with(b"%PDF"));
    }
    assert!(
        render_price_comparison_pdf(&PrintPriceComparison::default())
            .unwrap()
            .starts_with(b"%PDF")
    );
}

/// Text runs with their horizontal extent in mm on each page.
fn placed_texts(document: &PagedDocument) -> Vec<(usize, String, f64, f64)> {
    fn walk(frame: &Frame, dx: f64, page: usize, out: &mut Vec<(usize, String, f64, f64)>) {
        for (point, item) in frame.items() {
            let x = dx + point.x.to_mm();
            match item {
                FrameItem::Group(group) => {
                    walk(&group.frame, x + group.transform.tx.to_mm(), page, out)
                }
                FrameItem::Text(t) => {
                    out.push((page, t.text.to_string(), x, x + t.width().to_mm()))
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for (page, p) in document.pages().iter().enumerate() {
        walk(&p.frame, 0.0, page, &mut out);
    }
    out
}

const LONG_OZ: &str = "01.481.10.0010";

fn long_oz_lv() -> PrintLv {
    PrintLv {
        items: vec![PrintItem::Title(PrintTitle {
            lump: false,
            oz: "01.481.10".into(),
            label: String::new(),
            text: "Automationseinrichtungen".into(),
            items: vec![PrintItem::Position(PrintPosition {
                oz: LONG_OZ.into(),
                short_text: "Kurztext".into(),
                quantity: Some(d("1")),
                unit: "St".into(),
                unit_price: Some(d("10")),
                ..PrintPosition::default()
            })],
            ..PrintTitle::default()
        })],
        ..PrintLv::default()
    }
}

fn assert_oz_before_text(document: &PagedDocument, text: &str) {
    let texts = placed_texts(document);
    let oz = texts
        .iter()
        .find(|t| t.1 == LONG_OZ)
        .expect("OZ in einem Stück");
    let short = texts
        .iter()
        .find(|t| t.1.starts_with(text))
        .expect("Kurztext");
    assert!(
        oz.3 < short.2,
        "OZ endet bei {} mm, Kurztext beginnt bei {} mm",
        oz.3,
        short.2
    );
}

#[test]
fn long_oz_does_not_overlap_short_text() {
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        let document = lv_world(&long_oz_lv(), mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        assert_oz_before_text(&document, "Kurztext");
    }
    let mut pc = sample_pc(3);
    let PcItem::Title(title) = &mut pc.items[0] else {
        unreachable!()
    };
    let PcItem::Position(position) = &mut title.items[0] else {
        unreachable!()
    };
    position.oz = LONG_OZ.into();
    position.short_text = "Kurztext lang".into();
    let document = price_comparison_world(&pc).unwrap().layout().unwrap();
    assert_oz_before_text(&document, "Kurztext");
}

#[test]
fn long_bidder_names_without_spaces_stay_in_their_columns() {
    let long = "Bieter aus 260101001_Angebot_GA_Markthalle_Musterstadt_V3.X84_mit_noch_viel_mehr_Text_ohne_Leerzeichen";
    let mut pc = sample_pc(3);
    for bidder in &mut pc.bidders {
        bidder.name = long.into();
    }
    let document = price_comparison_world(&pc).unwrap().layout().unwrap();
    let page_width = document.pages()[0].frame.width().to_mm();
    let texts = placed_texts(&document);
    for (_, text, _, end) in &texts {
        assert!(*end <= page_width - 14.0, "{text} läuft bis {end} mm");
    }
    let all: String = texts.iter().map(|t| t.1.as_str()).collect();
    assert!(
        all.contains("ohne_Leerzeichen"),
        "voller Name in der Übersicht"
    );
    assert!(all.contains('…'), "Spaltenkopf gekürzt");
    // Shortened head per bidder in the positions table and in the totals table.
    let cut = texts.iter().filter(|t| t.1.ends_with('…')).count();
    assert_eq!(cut, 6);
}

fn formatted_lv() -> PrintLv {
    let run = |text: &str, font: Option<&str>, size: Option<f32>| TextRun {
        text: text.into(),
        font: font.map(str::to_string),
        size_pt: size,
        ..TextRun::default()
    };
    let cell = |text: &str, align| TableCell {
        runs: vec![TextRun::plain(text)],
        align,
    };
    PrintLv {
        items: vec![PrintItem::Position(PrintPosition {
            oz: "0010".into(),
            short_text: "Formatierter Text".into(),
            long_text: vec![
                TextBlock::paragraph(vec![run("LINKSWORT", None, None)]),
                TextBlock::Paragraph {
                    runs: vec![run("MITTEWORT", Some(FONT_SERIF), Some(14.0))],
                    align: TextAlign::Center,
                },
                TextBlock::Paragraph {
                    runs: vec![run("RECHTSWORT", Some(FONT_MONO), Some(8.0))],
                    align: TextAlign::Right,
                },
                TextBlock::Paragraph {
                    runs: vec![run("Fremdschrift", Some("Wingdings"), None)],
                    align: TextAlign::Justify,
                },
                TextBlock::Table {
                    header_rows: 1,
                    rows: vec![
                        vec![
                            cell("KOPFA", TextAlign::Left),
                            cell("KOPFB", TextAlign::Left),
                        ],
                        vec![
                            cell("ZELLEA", TextAlign::Left),
                            cell("ZELLEB", TextAlign::Right),
                        ],
                    ],
                    widths_mm: vec![Some(30.0), None],
                },
            ],
            takeoff: Vec::new(),
            quantity: Some(d("1")),
            unit: "St".into(),
            unit_price: None,
            total_price: None,
            kinds: Vec::new(),
            in_total: true,
            price_parts: Vec::new(),
            remarks: Vec::new(),
            cost_group: None,
            work_category: None,
            surcharge: None,
        })],
        ..PrintLv::default()
    }
}

/// Text items with font family and size.
fn fonts_of(document: &PagedDocument) -> Vec<(String, String, f64)> {
    fn walk(frame: &Frame, out: &mut Vec<(String, String, f64)>) {
        for (_, item) in frame.items() {
            match item {
                FrameItem::Group(group) => walk(&group.frame, out),
                FrameItem::Text(t) => out.push((
                    t.text.to_string(),
                    t.font.info().family.clone(),
                    t.size.to_pt(),
                )),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for page in document.pages() {
        walk(&page.frame, &mut out);
    }
    out
}

#[test]
fn fonts_sizes_alignment_and_tables_are_printed() {
    let lv = formatted_lv();
    assert!(render_lv_pdf(&lv, LvPrintMode::Blank)
        .unwrap()
        .starts_with(b"%PDF"));
    let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let fonts = fonts_of(&document);
    let find = |word: &str| {
        fonts
            .iter()
            .find(|(text, _, _)| text.contains(word))
            .unwrap_or_else(|| panic!("{word} fehlt"))
            .clone()
    };
    let (_, family, size) = find("MITTEWORT");
    assert_eq!((family.as_str(), size), ("Liberation Serif", 14.0));
    let (_, family, size) = find("RECHTSWORT");
    assert_eq!((family.as_str(), size), ("Liberation Mono", 8.0));
    assert_eq!(find("Fremdschrift").1, "Liberation Sans");
    assert_eq!(find("KOPFA").1, "Liberation Sans");

    let placed = placed_texts(&document);
    let at = |word: &str| {
        placed
            .iter()
            .find(|(_, text, _, _)| text.contains(word))
            .map(|(_, _, left, right)| (*left, *right))
            .unwrap()
    };
    let (left, _) = at("LINKSWORT");
    let (center_left, center_right) = at("MITTEWORT");
    let (_, right_end) = at("RECHTSWORT");
    assert!(center_left > left + 20.0, "mittig: {center_left} vs {left}");
    assert!(
        right_end > center_right + 20.0,
        "rechts: {right_end} vs {center_right}"
    );
    // The fixed first column is 30 mm wide: column B starts about 30 mm after A.
    let (a, _) = at("ZELLEA");
    let (b_head, _) = at("KOPFB");
    assert!(
        (b_head - a - 30.0).abs() < 2.0,
        "Spaltenbreite: {a} {b_head}"
    );
}

#[test]
fn cover_sheet_comes_first_with_multi_line_values() {
    let mut lv = sample_lv();
    let cover = PrintCover {
        lv: "LV 480 Gebäudeautomation".into(),
        project: "Neubau #Schule".into(),
        measure: "Neubau\nzweite Zeile".into(),
        facts: vec![
            ("vergabenummer".into(), "V-2026-0815".into()),
            ("abgabetermin".into(), "15.12.2026, 10:00 Uhr".into()),
            ("bindefrist".into(), String::new()),
        ],
        addresses: vec![
            (
                "bauherr".into(),
                vec!["Stadt #Musterstadt".into(), "Rathausplatz 1".into()],
            ),
            ("planer".into(), vec!["Planungsbüro Beispiel".into()]),
        ],
        sections: vec![
            CoverSection {
                heading: "Sicherheiten".into(),
                rows: vec![("Vertragserfüllung".into(), "5 % der Auftragssumme".into())],
            },
            CoverSection {
                heading: "Leer".into(),
                rows: Vec::new(),
            },
        ],
        notes: vec![(
            "Zahlungsbedingungen".into(),
            "Abschläge nach § 16 VOB/B".into(),
        )],
        ..PrintCover::default()
    };
    for style in [CoverStyle::Standard, CoverStyle::Letterhead] {
        lv.cover = Some(PrintCover {
            style,
            ..cover.clone()
        });
        let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let (first, _) = page_text(&document, 0);
        for token in [
            "Angebotsaufforderung",
            "LV480Gebäudeautomation",
            "Neubau#Schule",
            "V-2026-0815",
            "15.12.2026,10:00Uhr",
            "Stadt#Musterstadt",
            "PlanungsbüroBeispiel",
            "Angebotssummebrutto",
            "Seite1von",
        ] {
            assert!(first.contains(token), "{style:?}: {token} fehlt: {first}");
        }
        assert!(
            first.contains("Bauherr/Auftraggeber"),
            "ohne eigenen Auftraggeber: {first}"
        );
        assert!(
            !first.contains("Bindefrist"),
            "leere Kenndaten entfallen: {first}"
        );
        assert!(
            !first.contains("Test#Projekt"),
            "kein Seitenkopf auf dem Deckblatt: {first}"
        );
        assert!(
            !first.contains("Menge"),
            "das LV beginnt erst nach dem Deckblatt: {first}"
        );
        let (second, _) = page_text(&document, 1);
        assert!(
            second.starts_with("Test#Projekt") || second.contains("Vertragsbedingungen"),
            "{second}"
        );
        assert!(
            second.contains("Vertragsbedingungen") && second.contains("Zahlungsbedingungen"),
            "{second}"
        );
        assert!(
            !second.contains("Leer"),
            "leere Abschnitte fallen weg: {second}"
        );
        let (third, _) = page_text(&document, 2);
        assert!(third.contains("Menge"), "LV ab Seite 3: {third}");
    }
    // Priced LV: no bidder field; blocks can be switched off.
    lv.cover = Some(PrintCover {
        blocks: CoverBlocks {
            addresses: false,
            facts: false,
            ..CoverBlocks::default()
        },
        ..cover
    });
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (first, _) = page_text(&document, 0);
    assert!(
        !first.contains("Angebotssumme")
            && !first.contains("Stadt#Musterstadt")
            && !first.contains("V-2026"),
        "{first}"
    );
    assert!(first.contains("Kostenvoranschlag"), "{first}");
}

#[test]
fn submission_record_lists_bidders_and_signatures() {
    let submission = PrintSubmission {
        header: PrintHeader {
            project_name: "Schule".into(),
            lv_name: "LV 1 – GA".into(),
            date: "15.12.2026".into(),
            ..PrintHeader::default()
        },
        facts: vec![("Eröffnungstermin".into(), "15.12.2026, 10:00 Uhr".into())],
        bidders: vec![
            SubmissionBidder {
                name: "Müller #GmbH".into(),
                place: "Musterstadt".into(),
                received: "14.12.2026".into(),
                status: "Angebot abgegeben".into(),
                net: Some(Decimal::from(100_000)),
                discount: Some(("3 %".into(), Decimal::from(3_000))),
                side_offers: true,
                remark: "Nebenangebot Fabrikat B".into(),
            },
            SubmissionBidder {
                name: "Absage AG".into(),
                status: "kein Angebot".into(),
                ..SubmissionBidder::default()
            },
        ],
        vat_percent: Decimal::from(19),
        notes: vec!["Die Angebote waren verschlossen.".into()],
        signatures: vec!["Verhandlungsleiter".into(), "Schriftführer".into()],
        header_text: "Vorabzug".into(),
        footer_text: String::new(),
    };
    let pdf = render_submission_pdf(&submission).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let document = crate::submission::submission_world(&submission)
        .unwrap()
        .layout()
        .unwrap();
    let (text, _) = inspect(&document);
    let squeezed: String = text.split_whitespace().collect();
    for token in [
        "NiederschriftüberdieAngebotsöffnung",
        "Müller#GmbH",
        "100.000,00€",
        "97.000,00€",
        "115.430,00€",
        "keinAngebot",
        "Verhandlungsleiter",
        "15.12.2026,10:00Uhr",
    ] {
        assert!(squeezed.contains(token), "{token} fehlt in: {text}");
    }
}

/// Text of all pages, concatenated.
fn document_text(document: &PagedDocument) -> String {
    let mut out = String::new();
    for page in document.pages() {
        let mut images = 0;
        collect(&page.frame, &mut out, &mut images);
        out.push(' ');
    }
    out
}

#[test]
fn options_steer_paper_sections_and_page_breaks() {
    let lv = sample_lv();
    let layout = |options: &PrintOptions| {
        lv_world(&lv, LvPrintMode::Priced, options)
            .unwrap()
            .layout()
            .unwrap()
    };
    let normal = layout(&PrintOptions::default());
    let text = document_text(&normal);
    assert!(text.contains("Zusammenstellung"));

    let landscape = layout(&PrintOptions {
        landscape: true,
        ..PrintOptions::default()
    });
    let size = landscape.pages()[0].frame.size();
    assert!(size.x > size.y, "quer");

    let summary_only = layout(&PrintOptions {
        summary_only: true,
        header_text: "Vorabzug".into(),
        footer_text: "Ende des LV".into(),
        ..PrintOptions::default()
    });
    let text = document_text(&summary_only);
    assert!(text.contains("Betrag") && text.contains("Vorabzug") && text.contains("Ende"));
    assert!(
        !text.contains("Menge"),
        "keine Positionstabelle im Summenblatt"
    );
    assert!(summary_only.pages().len() <= normal.pages().len());

    let without_summary = layout(&PrintOptions {
        summary: false,
        ..PrintOptions::default()
    });
    assert!(!document_text(&without_summary).contains("Zusammenstellung"));

    let broken = layout(&PrintOptions {
        title_page_break: true,
        ..PrintOptions::default()
    });
    assert!(broken.pages().len() >= normal.pages().len());

    let big = layout(&PrintOptions {
        font_size_pt: 11,
        ..PrintOptions::default()
    });
    assert!(big.pages().len() >= normal.pages().len());
    assert!(lv_page_count(&lv, LvPrintMode::Priced, &PrintOptions::default()).unwrap() >= 1);
}

#[test]
fn letters_one_per_page_with_address_and_subject() {
    let letter = |name: &str| PrintLetter {
        sender: vec![
            "Planungsbüro Muster".into(),
            "Musterweg 1".into(),
            "12345 Musterstadt".into(),
        ],
        return_line: "Planungsbüro Muster · Musterweg 1 · 12345 Musterstadt".into(),
        recipient: vec![
            name.into(),
            "Hafenstraße 2".into(),
            "54321 Beispielstadt".into(),
        ],
        references: vec![
            ("Datum".into(), "17.11.2026".into()),
            ("Vergabenummer".into(), "VG-1".into()),
        ],
        subject: "Aufforderung zur Abgabe eines Angebots – LV 1".into(),
        body: vec![
            TextBlock::text("Sehr geehrte Damen und Herren,"),
            TextBlock::bullet("Leistungsverzeichnis #1"),
        ],
        signature: vec![
            "Mit freundlichen Grüßen".into(),
            "Planungsbüro Muster".into(),
        ],
    };
    let letters = PrintLetters {
        title: "Aufforderung".into(),
        logo: Some(PrintImage::new(tiny_png(), "logo.png")),
        letters: vec![letter("Müller GmbH"), letter("Schmidt KG")],
        frame: PrintFrame::default(),
        look: PrintLook::default(),
    };
    let (pdf, pages) = render_letters_pdf(&letters).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert_eq!(pages, 2);
    let document = crate::letter::letters_world(&letters)
        .unwrap()
        .layout()
        .unwrap();
    let mut first = String::new();
    let mut images = 0;
    collect(&document.pages()[0].frame, &mut first, &mut images);
    let squeezed: String = first.split_whitespace().collect();
    for token in [
        "MüllerGmbH",
        "AufforderungzurAbgabe",
        "Leistungsverzeichnis#1",
        "VG-1",
        "Seite1",
    ] {
        assert!(squeezed.contains(token), "{token} fehlt: {first}");
    }
    assert!(!squeezed.contains("SchmidtKG"));
    assert_eq!(images, 1, "Logo");
    assert!(render_letters_pdf(&PrintLetters::default()).is_err());
}

#[test]
fn change_list_renders() {
    let mut list = PrintChangeList {
        header: PrintHeader {
            project_name: "MH1".into(),
            lv_name: "GA V2".into(),
            date: "17.11.2026".into(),
            ..PrintHeader::default()
        },
        landscape: false,
        against: "DLZ_F.X83".into(),
        summary: "1 entfallen, 1 geändert".into(),
        entries: vec![
            PrintChange {
                kind: "geändert".into(),
                row_kind: "Position".into(),
                ordinal: "01.484.10.0080".into(),
                text: "Kabelabzweigkasten & <Klemmen>".into(),
                details: vec!["Menge: 652 → 150".into()],
                ..PrintChange::default()
            },
            PrintChange {
                kind: "entfallen".into(),
                row_kind: "Position".into(),
                ordinal: "01.482.10.0090".into(),
                text: "Ventilantrieb".into(),
                ..PrintChange::default()
            },
        ],
    };
    let bytes = crate::render_change_list_pdf(&list).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    // A4 quer from the print dialog.
    list.landscape = true;
    let wide = crate::render_change_list_pdf(&list).unwrap();
    let text = String::from_utf8_lossy(&wide);
    let media = text
        .find("/MediaBox")
        .map(|at| &text[at..at + 40])
        .unwrap_or_default();
    assert!(media.contains("841.8"), "quer: {media}");
    let empty = crate::render_change_list_pdf(&PrintChangeList::default()).unwrap();
    assert!(empty.starts_with(b"%PDF"));
}

fn page_text(document: &PagedDocument, page: usize) -> (String, usize) {
    let mut text = String::new();
    let mut images = 0;
    collect(&document.pages()[page].frame, &mut text, &mut images);
    (text.split_whitespace().collect::<String>(), images)
}

fn two_title_lv() -> PrintLv {
    let positions = |prefix: &str| -> Vec<PrintItem> {
        (1..=45)
            .map(|n| {
                PrintItem::Position(PrintPosition {
                    oz: format!("{prefix}.{n:04}"),
                    short_text: format!("Leistung {n}"),
                    long_text: vec![TextBlock::text("Liefern und montieren.")],
                    quantity: Some(d("1")),
                    unit: "St".into(),
                    ..PrintPosition::default()
                })
            })
            .collect()
    };
    PrintLv {
        header: PrintHeader {
            project_name: "Projekt".into(),
            lv_name: "LV 480".into(),
            date: "17.11.2026".into(),
            logo: Some(PrintImage::new(tiny_png(), "logo.png")),
            ..PrintHeader::default()
        },
        items: vec![
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: "01".into(),
                label: String::new(),
                text: "Anlage Nord".into(),
                total: None,
                items: positions("01"),
            }),
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: "02".into(),
                label: String::new(),
                text: "Anlage Sued".into(),
                total: None,
                items: positions("02"),
            }),
        ],
        ..PrintLv::default()
    }
}

#[test]
fn page_frame_fields_placeholders_title_logo_and_rules() {
    let mut lv = two_title_lv();
    lv.header.frame = PrintFrame {
        header: [
            "Titel: {titel}".into(),
            "{dokumentart}".into(),
            "Nr. {projektnummer}\n{vergabenummer}".into(),
        ],
        footer: ["{lv}".into(), String::new(), "S. {seite}/{seiten}".into()],
        header_rule: false,
        footer_rule: false,
        logo: FrameLogo::Left,
        font_size_pt: 9.0,
        values: vec![
            ("projektnummer".into(), "VG-14".into()),
            ("vergabenummer".into(), String::new()),
        ],
    };
    let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let pages = document.pages().len();
    assert!(pages >= 4, "genug Seiten für den Titelwechsel: {pages}");
    let texts: Vec<(String, usize)> = (0..pages).map(|page| page_text(&document, page)).collect();
    assert!(texts[0].0.contains("Nr.VG-14"), "{}", texts[0].0);
    assert!(
        texts[0].0.contains(&format!("S.1/{pages}")),
        "{}",
        texts[0].0
    );
    assert!(texts[1].0.contains("Titel:01AnlageNord"), "{}", texts[1].0);
    assert!(
        texts[pages - 1].0.contains("Titel:02AnlageSued"),
        "{}",
        texts[pages - 1].0
    );
    assert!(
        !texts[pages - 1].0.contains("Titel:01"),
        "Titel der Seite, nicht der erste"
    );
    // Logo in the page header on every page (page 1 also in the title block).
    assert_eq!(
        texts[1].1,
        1,
        "Logo im Seitenkopf: {:?}",
        texts.iter().map(|t| t.1).collect::<Vec<_>>()
    );
    assert_eq!(
        texts[0].1, 1,
        "nur im Seitenkopf, nicht zusätzlich im Titelblock"
    );

    // Standard: no logo in the page header, no free text line.
    let mut plain = two_title_lv();
    plain.header.frame = PrintFrame::default();
    let document = lv_world(&plain, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let (second, images) = page_text(&document, 1);
    assert_eq!(images, 0);
    assert!(
        second.contains("Projekt")
            && second.contains("Angebotsaufforderung")
            && second.contains("17.11.2026")
    );
    assert!(
        second.contains(&format!("Seite2von{}", document.pages().len())),
        "{second}"
    );
}

#[test]
fn long_frame_texts_are_cut_instead_of_overlapping() {
    let mut lv = two_title_lv();
    let long = "Sehr lange Bezeichnung des Projekts ".repeat(8);
    lv.header.frame.header = [long.clone(), "{dokumentart}".into(), long];
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let placed = placed_texts(&document);
    let left: Vec<_> = placed
        .iter()
        .filter(|(page, text, _, _)| *page == 1 && text.starts_with("Sehr"))
        .collect();
    assert_eq!(left.len(), 2, "links und rechts je eine Zeile: {left:?}");
    let kind = placed
        .iter()
        .find(|(page, text, _, _)| *page == 1 && text.contains("Kostenvoranschlag"))
        .expect("Dokumentart in der Mitte");
    for (_, text, start, end) in left {
        assert!(text.ends_with('…'), "gekürzt: {text}");
        assert!(
            *end <= kind.2 || *start >= kind.3,
            "überlappt die Mitte: {start}–{end} gegen {}–{}",
            kind.2,
            kind.3
        );
    }
}

#[test]
fn letters_count_pages_per_letter_in_the_footer() {
    let letter = |name: &str| PrintLetter {
        recipient: vec![name.into()],
        subject: "Betreff".into(),
        body: vec![TextBlock::text("Text")],
        ..PrintLetter::default()
    };
    let letters = PrintLetters {
        title: "Aufforderung".into(),
        logo: None,
        letters: vec![letter("A GmbH"), letter("B KG")],
        frame: PrintFrame {
            values: vec![("lv".into(), "LV 480".into())],
            ..PrintFrame::default()
        },
        look: PrintLook::default(),
    };
    let document = crate::letter::letters_world(&letters)
        .unwrap()
        .layout()
        .unwrap();
    assert_eq!(document.pages().len(), 2);
    for page in 0..2 {
        let (text, _) = page_text(&document, page);
        assert!(text.contains("Seite1von1"), "je Brief gezählt: {text}");
        assert!(text.contains("LV480"), "{text}");
    }
}

#[test]
fn contents_carry_level_labels_and_price_parts() {
    let mut lv = two_title_lv();
    if let PrintItem::Title(title) = &mut lv.items[0] {
        title.label = "Los".into();
        if let PrintItem::Position(position) = &mut title.items[0] {
            position.unit_price = Some(d("12.5"));
            position.price_parts = vec![("Lohn".into(), Some(d("5"))), ("Stoffe".into(), None)];
        }
        for item in &mut title.items {
            if let PrintItem::Position(position) = item {
                position.unit_price.get_or_insert(d("100"));
            }
        }
    }
    let options = PrintOptions {
        contents: true,
        carry: true,
        ..PrintOptions::default()
    };
    let document = lv_world(&lv, LvPrintMode::Priced, &options)
        .unwrap()
        .layout()
        .unwrap();
    let pages = document.pages().len();
    let texts: Vec<String> = (0..pages)
        .map(|page| page_text(&document, page).0)
        .collect();
    // Inhaltsverzeichnis on page 1 (after the title block) with the pages of the titles.
    assert!(
        texts[0].contains("Inhaltsverzeichnis") && texts[0].contains("LosAnlageNord"),
        "{}",
        texts[0]
    );
    let south = texts
        .iter()
        .position(|text| text.contains("02AnlageSued") && text.contains("Menge"))
        .unwrap();
    assert!(
        texts[0].contains(&format!("AnlageSued{}", south + 1)),
        "Seite von 02: {}",
        texts[0]
    );
    assert!(texts[0].contains("Zusammenstellung"), "{}", texts[0]);
    // Übertrag at the bottom of a page and the top of the next inside title 01.
    let carried = texts
        .iter()
        .filter(|text| text.contains("Übertrag:"))
        .count();
    assert!(carried >= 2, "{carried}: {texts:?}");
    assert!(texts[1].contains("Übertrag:"), "{}", texts[1]);
    // EP-Aufgliederung with values (priced) …
    assert!(
        texts
            .iter()
            .any(|text| text.contains("Lohn5,00€") && text.contains("Stoffe")),
        "{texts:?}"
    );
    // … and blank in the Angebotsaufforderung.
    let blank = lv_world(&lv, LvPrintMode::Blank, &options)
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..blank.pages().len())
        .map(|page| page_text(&blank, page).0)
        .collect();
    assert!(all.contains("Lohn") && !all.contains("Lohn5,00"), "{all}");
    assert!(all.contains("Übertrag:"));
}

#[test]
fn print_style_and_accent_reach_the_templates() {
    let mut lv = two_title_lv();
    for (style, accent, expected) in [
        (PrintStyle::Classic, "#004f7c", "#004F7C"),
        (PrintStyle::ProAva, "abc123", "#ABC123"),
        (PrintStyle::ProAva, "rot", PrintLook::DEFAULT_ACCENT),
    ] {
        lv.header.look = PrintLook {
            style,
            accent: accent.into(),
            border: PageBorder::None,
            ..PrintLook::default()
        };
        let mut files = crate::view::Files::default();
        let data = crate::view::lv(
            &lv,
            LvPrintMode::Priced,
            &PrintOptions::default(),
            &mut files,
        );
        assert_eq!(data["header"]["look"]["accent"], expected);
        let name = if style == PrintStyle::Classic {
            "classic"
        } else {
            "proava"
        };
        assert_eq!(data["header"]["look"]["style"], name);
        let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        assert!(page_text(&document, 0).0.contains("AnlageNord"));
    }
}

#[test]
fn carry_is_the_sum_of_the_totals_printed_so_far_in_the_title() {
    let mut lv = two_title_lv();
    for item in &mut lv.items {
        if let PrintItem::Title(title) = item {
            for item in &mut title.items {
                if let PrintItem::Position(position) = item {
                    position.unit_price = Some(d("1"));
                }
            }
        }
    }
    let options = PrintOptions {
        carry: true,
        ..PrintOptions::default()
    };
    let document = lv_world(&lv, LvPrintMode::Priced, &options)
        .unwrap()
        .layout()
        .unwrap();
    let mut printed = 0;
    let mut checked = 0;
    for page in 0..document.pages().len() {
        let text = page_text(&document, page).0;
        if text.contains("Summe01") {
            break;
        }
        printed += text.matches("1St1,001,00").count();
        let at = text
            .rfind("Übertrag:")
            .unwrap_or_else(|| panic!("Seite {}: kein Übertrag: {text}", page + 1));
        let value: String = text[at + "Übertrag:".len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
            .collect();
        assert_eq!(value, format!("{printed},00"), "Seite {}: {text}", page + 1);
        checked += 1;
    }
    assert!(checked >= 2, "der Titel geht über mehrere Seiten");
}

#[test]
fn performance_descriptions_are_marked_and_referenced() {
    let mut lv = sample_lv();
    lv.items.insert(
        0,
        PrintItem::Performance(
            "A0003".into(),
            vec![
                TextBlock::paragraph(vec![TextRun::bold("Kabeltrassen")]),
                TextBlock::text("Kabelrinnen verzinkt."),
            ],
        ),
    );
    lv.items.insert(
        1,
        PrintItem::Note(vec![TextBlock::text("Allgemeiner Hinweis.")]),
    );
    if let Some(PrintItem::Title(title)) = lv
        .items
        .iter_mut()
        .find(|item| matches!(item, PrintItem::Title(_)))
    {
        if let Some(PrintItem::Position(position)) = title
            .items
            .iter_mut()
            .find(|item| matches!(item, PrintItem::Position(_)))
        {
            position.remarks =
                vec!["Ausführung nach Ausführungsbeschreibung A0003 – Kabeltrassen".into()];
        }
    }
    let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    assert!(all.contains("A0003Ausführungsbeschr.Kabeltrassen"), "{all}");
    assert!(
        all.contains("AusführungnachAusführungsbeschreibungA0003–Kabeltrassen"),
        "{all}"
    );
    assert!(
        all.contains("HinweisAllgemeinerHinweis."),
        "Hinweis bleibt unterscheidbar: {all}"
    );
}

/// Anwender 18.11.: „A0001“ with a small „Ausführungsbeschr.“
/// below in the OZ column. With short OZ (mask PPPP) both stay in one piece,
/// the mark above the word, both left of the text – in every LV print and
/// both styles.
#[test]
fn performance_mark_stays_in_one_piece() {
    let lv = PrintLv {
        items: vec![
            PrintItem::Performance(
                "A0001".into(),
                vec![TextBlock::paragraph(vec![TextRun::bold("Kabeltrassen")])],
            ),
            PrintItem::Position(PrintPosition {
                oz: "0010".into(),
                short_text: "Kurztext".into(),
                quantity: Some(d("1")),
                unit: "St".into(),
                unit_price: Some(d("10")),
                remarks: vec![
                    "Ausführung nach Ausführungsbeschreibung A0001 – Kabeltrassen".into(),
                ],
                ..PrintPosition::default()
            }),
        ],
        ..PrintLv::default()
    };
    for style in [PrintStyle::Classic, PrintStyle::ProAva] {
        let mut lv = lv.clone();
        lv.header.look = PrintLook {
            style,
            ..PrintLook::default()
        };
        for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
            let document = lv_world(&lv, mode, &PrintOptions::default())
                .unwrap()
                .layout()
                .unwrap();
            let texts = placed_texts(&document);
            let find = |wanted: &str| {
                texts
                    .iter()
                    .find(|t| t.1 == wanted)
                    .unwrap_or_else(|| {
                        panic!("{style:?} {mode:?}: „{wanted}“ in einem Stück: {texts:?}")
                    })
                    .clone()
            };
            let mark = find("A0001");
            let word = find("Ausführungsbeschr.");
            let label = find("Kabeltrassen");
            assert!(
                mark.3 < label.2 && word.3 < label.2,
                "{style:?} {mode:?}: OZ-Spalte vor dem Text"
            );
            assert!(
                (mark.2 - word.2).abs() < 0.5,
                "{style:?} {mode:?}: Wort unter der Marke"
            );
        }
    }
}

#[test]
fn takeoff_lines_and_sum_are_printed_under_the_position() {
    let mut lv = sample_lv();
    let mut first = true;
    fn positions(items: &mut [PrintItem], visit: &mut dyn FnMut(&mut PrintPosition)) {
        for item in items {
            match item {
                PrintItem::Title(title) => positions(&mut title.items, visit),
                PrintItem::Position(position) => visit(position),
                _ => {}
            }
        }
    }
    positions(&mut lv.items, &mut |position| {
        if std::mem::take(&mut first) {
            position.takeoff = vec![
                PrintTakeoffLine {
                    label: "EG Strang 1".into(),
                    formula: "2*12,5".into(),
                    result: Some(d("25")),
                },
                PrintTakeoffLine {
                    label: "OG <Flur> & #Technik".into(),
                    formula: "3+".into(),
                    result: None,
                },
            ];
        }
    });
    for mode in [LvPrintMode::Priced, LvPrintMode::Blank] {
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let (text, _) = inspect(&document);
        assert!(text.contains("Mengenermittlung"), "{text}");
        assert!(text.contains("EG Strang 1"), "{text}");
        assert!(text.contains("2*12,5"), "{text}");
        assert!(text.contains("Fehler"), "falscher Rechenansatz: {text}");
        assert!(text.contains("#Technik"), "wörtlich: {text}");
        assert!(text.contains("Summe"), "{text}");
    }
    // Without lines nothing is printed.
    let document = lv_world(&sample_lv(), LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    assert!(!inspect(&document).0.contains("Mengenermittlung"));
}

#[test]
fn page_border_keeps_header_and_footer_inside_and_spares_no_page() {
    let mut lv = two_title_lv();
    let plain = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    for border in [PageBorder::Thin, PageBorder::Medium] {
        lv.header.look.border = border;
        let mut files = crate::view::Files::default();
        let data = crate::view::lv(
            &lv,
            LvPrintMode::Priced,
            &PrintOptions::default(),
            &mut files,
        );
        assert_eq!(
            data["header"]["look"]["border"],
            if border == PageBorder::Thin {
                "thin"
            } else {
                "medium"
            }
        );
        for options in [
            PrintOptions::default(),
            PrintOptions {
                landscape: true,
                ..PrintOptions::default()
            },
        ] {
            let document = lv_world(&lv, LvPrintMode::Priced, &options)
                .unwrap()
                .layout()
                .unwrap();
            let pages = document.pages().len();
            // Header and footer move inside the frame: a little less room per page.
            if !options.landscape {
                assert!(pages >= plain.pages().len(), "{pages}");
            }
            for page in 0..pages {
                let text = page_text(&document, page).0;
                assert!(
                    text.contains(&format!("Seite{}von{pages}", page + 1)),
                    "{border:?} Seite {}: {text}",
                    page + 1
                );
            }
        }
    }
    // With a cover: no header on the cover (footer as without the frame), the LV follows framed.
    lv.cover = Some(PrintCover {
        lv: "LV 480".into(),
        ..PrintCover::default()
    });
    let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let cover = page_text(&document, 0).0;
    assert!(
        cover.contains("Seite1von") && !cover.starts_with("Projekt"),
        "Deckblatt ohne Kopf: {cover}"
    );
    assert!(
        page_text(&document, 1).0.starts_with("Projekt"),
        "Kopf ab Seite 2"
    );
}

fn billing_check_sample() -> PrintBillingCheck {
    PrintBillingCheck {
        invoice: "2. Abschlagsrechnung AR-2 vom 30.04.2027".into(),
        facts: vec![
            ("Auftragnehmer".into(), "Muster Regeltechnik GmbH".into()),
            ("Rechnungs-Nr.".into(), "AR-2".into()),
        ],
        sense: "kumuliert".into(),
        rows: vec![
            PrintBillingRow::Title {
                level: 0,
                ordinal: "01".into(),
                text: "Feldgeräte".into(),
                claimed: d("1500"),
                checked: d("1200"),
            },
            PrintBillingRow::Position(Box::new(PrintBillingPosition {
                ordinal: "01.0010".into(),
                text: "Raumtemperaturfühler".into(),
                unit: "St".into(),
                ordered: Some(d("20")),
                previous: d("5"),
                claimed: Some(d("15")),
                checked: d("12"),
                unit_price: Some(d("100")),
                claimed_total: Some(d("1500")),
                checked_total: d("1200"),
                reached_percent: Some(d("60")),
                basis: "Aufmaß".into(),
                notes: vec!["Menge gekürzt: 12 statt 15.".into()],
                mark: PrintMark::Warning,
                change_order: false,
            })),
        ],
        totals: vec![
            PrintAmountLine::new("Summe Rechnung (Firma), netto kumuliert", d("1500")),
            PrintAmountLine::new("− Nachlass", d("-36")).with_extra("3 %"),
            PrintAmountLine::strong("= Leistung geprüft, brutto", d("1385.16")),
        ],
        findings: vec!["Rechnungsbetrag netto passt nicht.".into()],
        remarks: vec!["Geprüft am 05.05.2027 von O. Planer.".into()],
        ..PrintBillingCheck::default()
    }
}

#[test]
fn billing_check_pdf_shows_claimed_against_checked() {
    let report = billing_check_sample();
    let world = crate::billing::billing_check_world(&report).unwrap();
    let document = world.layout().unwrap();
    let (text, _) = inspect(&document);
    for token in [
        "Aufmaßprüfung",
        "AR-2",
        "Raumtemperaturfühler",
        "1.500,00",
        "1.200,00",
        "−300,00",
        "60 %",
        "Aufmaß",
        "gekürzt",
        "1.385,16",
        "Geprüft am 05.05.2027",
    ] {
        assert!(text.contains(token), "{token} fehlt in: {text}");
    }
    assert!(world.pdf(&document).unwrap().starts_with(b"%PDF"));
}

#[test]
fn release_pdf_has_calculation_earlier_releases_and_signatures() {
    let report = PrintRelease {
        invoice: "3. Abschlagsrechnung AR-3 vom 30.06.2027".into(),
        facts: vec![("Rechnungs-Nr.".into(), "AR-3".into())],
        addresses: vec![
            (
                "Auftragnehmer".into(),
                vec![
                    "Muster Regeltechnik GmbH".into(),
                    "12345 Musterstadt".into(),
                ],
            ),
            ("Bauherr".into(), vec!["Stadt Musterstadt".into()]),
        ],
        overview: vec![("Gesamt, netto".into(), Some(d("100000")), Some(d("60000")))],
        performed_percent: d("60"),
        calculation: vec![
            PrintAmountLine::new("Leistung geprüft, netto kumuliert", d("60000")),
            PrintAmountLine::new("− Sicherheitseinbehalt", d("-6000")).with_extra("10 %"),
            PrintAmountLine::new("− anrechenbare Freigaben (bisher)", d("-30000")),
            PrintAmountLine::strong("= Freigabe, netto", d("24000")),
            PrintAmountLine::strong("= Freigabe, brutto", d("28560")),
        ],
        earlier: vec![PrintEarlierRelease {
            label: "1. Abschlagsrechnung AR-1".into(),
            date: "28.02.2027".into(),
            performance: d("33333.33"),
            release_net: d("30000"),
            release_gross: d("35700"),
        }],
        notes: vec!["Bauabzugsteuer (§ 48 EStG)".into()],
        signatures: vec![
            "Planer – sachlich und rechnerisch richtig".into(),
            "Bauherr – zur Zahlung freigegeben".into(),
        ],
        ..PrintRelease::default()
    };
    let world = crate::billing::release_world(&report).unwrap();
    let document = world.layout().unwrap();
    let (text, _) = inspect(&document);
    for token in [
        "Rechnungsfreigabe",
        "AR-3",
        "Musterstadt",
        "60,0 %",
        "−6.000,00 €",
        "24.000,00 €",
        "28.560,00 €",
        "Bisherige Freigaben",
        "35.700,00",
        "§ 48 EStG",
        "Ort, Datum, Unterschrift",
        "zur Zahlung freigegeben",
    ] {
        assert!(text.contains(token), "{token} fehlt in: {text}");
    }
    assert_eq!(document.pages().len(), 1, "eine Seite");
}

#[test]
fn cost_group_and_work_category_stand_at_the_right_of_the_short_text() {
    let mut lv = sample_lv();
    if let Some(PrintItem::Title(title)) = lv
        .items
        .iter_mut()
        .find(|item| matches!(item, PrintItem::Title(_)))
    {
        if let Some(PrintItem::Position(position)) = title
            .items
            .iter_mut()
            .find(|item| matches!(item, PrintItem::Position(_)))
        {
            position.cost_group = Some("481".into());
            position.work_category = Some("070".into());
        }
    }
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    assert!(all.contains("KG481LB070"), "{all}");
}

#[test]
fn price_comparison_shows_percent_flags_estimate_and_cash_discount() {
    let mut pc = sample_pc(3);
    pc.estimate = true;
    pc.estimate_net = Some(d("950"));
    pc.outlier_note = "EP weicht mehr als 20 % vom Mittelwert ab".into();
    pc.cash_discount_percent = Some(d("2"));
    pc.cash_discount_note = "bei Zahlung innerhalb von 14 Tagen".into();
    if let PcItem::Title(title) = &mut pc.items[0] {
        title.estimate = Some(d("950"));
        if let PcItem::Position(position) = &mut title.items[0] {
            position.prices[2].outlier = true;
            position.prices[1].error = true;
            position.estimate = BidPrice {
                unit_price: Some(d("95")),
                ..BidPrice::default()
            };
        }
    }
    let document = crate::price_comparison_world(&pc)
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    for token in [
        "Planerpreis",
        "95,00950,00", // EP and GP of the planner
        "120,00?",
        "900,00!", // outlier at the EP, error at the GP
        "+33,3",   // 1.200 vs. the cheapest 900
        "Skonto2%aufbrutto(beiZahlunginnerhalbvon14Tagen)–informativ,nichtgewertet",
        "?Ausreißer:EPweichtmehrals20%vomMittelwertab",
    ] {
        assert!(all.contains(token), "{token} fehlt: {all}");
    }
}

/// Abnahme qa3 (18.11.): bei einer Bedarfsposition standen die Bieter-GP in
/// Klammern, Mittelwert, Minimum und Planerpreis aber ohne.
#[test]
fn position_outside_the_sum_brackets_every_total_price() {
    let mut pc = sample_pc(3);
    pc.estimate = true;
    if let PcItem::Title(title) = &mut pc.items[0] {
        if let PcItem::Position(position) = &mut title.items[0] {
            position.in_total = false;
            position.kinds = vec![PositionKind::Optional];
            position.estimate = BidPrice {
                unit_price: Some(d("95")),
                ..BidPrice::default()
            };
        }
    }
    let data = view::price_comparison(&pc, &mut view::Files::default());
    let row = data["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["t"] == "pos")
        .expect("Positionszeile")
        .clone();
    assert_eq!(row["excluded"], true);
    let document = crate::price_comparison_world(&pc)
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    for value in ["mean_gp", "min_gp", "est_gp"] {
        let gp = row[value]
            .as_str()
            .unwrap_or_else(|| panic!("{value} fehlt: {row}"));
        assert!(
            all.contains(&format!("({gp})")),
            "{value} {gp} ohne Klammern: {all}"
        );
    }
}

/// Anwender 18.11.: Menge nach Angebotseingang geändert – Hinweis
/// „Achtung Massenmehrung …“ an der Position, Summe mit LV-Menge nur zur Info.
#[test]
fn quantity_changed_after_the_bids_shows_hint_and_information_sum() {
    let mut pc = sample_pc(2);
    if let PcItem::Title(title) = &mut pc.items[0] {
        if let PcItem::Position(position) = &mut title.items[0] {
            position.quantity_note = "Achtung Massenmehrung: LV-Menge 12 statt 10 St (+2)".into();
        }
    }
    pc.bidders[0].after_discount_at_lv_quantities = Some(d("1234.5"));
    let document = crate::price_comparison_world(&pc)
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    for token in [
        "AchtungMassenmehrung:LV-Menge12statt10St(+2)",
        "Netton.N.mitaktuellerLV-Menge(nurInformation)",
        "1.234,50",
    ] {
        assert!(all.contains(token), "{token} fehlt: {all}");
    }
    // Ohne Mengenänderung keine Zeile.
    let plain = sample_pc(2);
    let document = crate::price_comparison_world(&plain)
        .unwrap()
        .layout()
        .unwrap();
    let all: String = (0..document.pages().len())
        .map(|page| page_text(&document, page).0)
        .collect();
    assert!(!all.contains("aktuellerLV-Menge"), "{all}");
}

#[test]
fn title_without_positions_shows_text_information_instead_of_a_sum() {
    let mut lv = sample_lv();
    lv.items.insert(
        0,
        PrintItem::Title(PrintTitle {
            lump: false,
            oz: "00".into(),
            label: String::new(),
            text: "Allgemeines".into(),
            total: None,
            items: vec![PrintItem::Note(vec![TextBlock::text(
                "Nur Vorbemerkungen.",
            )])],
        }),
    );
    for mode in [LvPrintMode::Priced, LvPrintMode::Blank] {
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let all: String = (0..document.pages().len())
            .map(|page| page_text(&document, page).0)
            .collect();
        // 00 and the empty Untertitel 01.01 of the sample: sum row and Zusammenstellung each.
        assert!(
            all.contains("Summe00AllgemeinesNurTextinformation"),
            "{all}"
        );
        assert_eq!(all.matches("NurTextinformation").count(), 4, "{all}");
    }
}

// ---------- Break chances in long words (format::breakable) ----------

/// Longer than the 20 characters `format::breakable` leaves alone, so it gets
/// a break chance after the hyphen.
const COMPOUND: &str = "DDC-Automationsstation";

/// Text runs with page, baseline (mm from the top) and left edge (mm).
fn placed_runs(document: &PagedDocument) -> Vec<(usize, f64, f64, String)> {
    fn walk(
        frame: &Frame,
        dx: f64,
        dy: f64,
        page: usize,
        out: &mut Vec<(usize, f64, f64, String)>,
    ) {
        for (point, item) in frame.items() {
            let (x, y) = (dx + point.x.to_mm(), dy + point.y.to_mm());
            match item {
                FrameItem::Group(group) => walk(
                    &group.frame,
                    x + group.transform.tx.to_mm(),
                    y + group.transform.ty.to_mm(),
                    page,
                    out,
                ),
                FrameItem::Text(t) => out.push((page, y, x, t.text.to_string())),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for (page, p) in document.pages().iter().enumerate() {
        walk(&p.frame, 0.0, 0.0, page, &mut out);
    }
    out
}

/// The PDF text layer is made from the text runs: none of them and not the
/// document title may carry the invisible U+200B (it would be copied along,
/// „DDC-\u{200B}Automationsstation“). `printed` must appear in one piece.
fn assert_no_break_marks(document: &PagedDocument, what: &str, printed: &str) {
    use typst::model::Document;
    let runs = placed_runs(document);
    for (page, _, _, text) in &runs {
        assert!(
            !text.contains('\u{200B}'),
            "{what}, Seite {}: Nullbreite-Leerzeichen in {text:?}",
            page + 1
        );
    }
    let all: String = runs.iter().map(|run| run.3.as_str()).collect();
    assert!(all.contains(printed), "{what}: {printed} fehlt in {all}");
    if let Some(title) = &document.info().title {
        assert!(
            !title.contains('\u{200B}'),
            "{what}: Dokumenttitel {title:?}"
        );
    }
}

#[test]
fn break_chances_leave_no_character_in_the_pdf_text() {
    // LV: title block, document title, title row and running title in the
    // page header, short text, long text, remark.
    let mut lv = two_title_lv();
    lv.header.project_name = format!("Projekt {COMPOUND}");
    lv.header.client = COMPOUND.into();
    lv.header.frame = PrintFrame {
        header: ["{titel}".into(), String::new(), String::new()],
        ..PrintFrame::default()
    };
    if let PrintItem::Title(title) = &mut lv.items[0] {
        title.text = COMPOUND.into();
        if let PrintItem::Position(position) = &mut title.items[0] {
            position.short_text = COMPOUND.into();
            position.long_text = vec![TextBlock::text(format!(
                "Liefern einer {COMPOUND} mit Zubehör."
            ))];
            position.remarks = vec![format!("Hinweis zur {COMPOUND}")];
        }
    }
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        assert_no_break_marks(&document, "LV", COMPOUND);
        let header_title = placed_runs(&document)
            .iter()
            .filter(|run| run.0 == 1 && run.1 < 25.0)
            .map(|run| run.3.clone())
            .collect::<String>();
        assert!(
            header_title.contains(COMPOUND),
            "laufender Titel im Seitenkopf: {header_title}"
        );
    }

    let mut pc = sample_pc(3);
    pc.bidders[0].name = COMPOUND.into();
    let document = price_comparison_world(&pc).unwrap().layout().unwrap();
    assert_no_break_marks(&document, "Preisspiegel", COMPOUND);

    let list = PrintChangeList {
        entries: vec![PrintChange {
            kind: "neu".into(),
            row_kind: "Position".into(),
            ordinal: "01.0010".into(),
            text: COMPOUND.into(),
            ..PrintChange::default()
        }],
        ..PrintChangeList::default()
    };
    let mut files = view::Files::default();
    let data = view::change_list(&list, &mut files);
    let document = build_world(CHANGES_TEMPLATE, files, &data)
        .unwrap()
        .layout()
        .unwrap();
    assert_no_break_marks(&document, "Änderungsliste", COMPOUND);

    let submission = PrintSubmission {
        bidders: vec![SubmissionBidder {
            name: COMPOUND.into(),
            status: "Angebot abgegeben".into(),
            ..SubmissionBidder::default()
        }],
        ..PrintSubmission::default()
    };
    let document = crate::submission::submission_world(&submission)
        .unwrap()
        .layout()
        .unwrap();
    assert_no_break_marks(&document, "Niederschrift", COMPOUND);

    let mut check = billing_check_sample();
    if let PrintBillingRow::Position(position) = &mut check.rows[1] {
        position.text = COMPOUND.into();
    }
    let document = crate::billing::billing_check_world(&check)
        .unwrap()
        .layout()
        .unwrap();
    assert_no_break_marks(&document, "Rechnungsprüfung", COMPOUND);

    let release = PrintRelease {
        invoice: format!("Abschlagsrechnung {COMPOUND}"),
        ..PrintRelease::default()
    };
    let document = crate::billing::release_world(&release)
        .unwrap()
        .layout()
        .unwrap();
    assert_no_break_marks(&document, "Rechnungsfreigabe", COMPOUND);
}

#[test]
fn long_compounds_still_break_after_their_hyphens() {
    let word = "Raumautomation-Feldbusankopplung-Erweiterungsmodul-Automationsstation-Zusatzgehäuse-Wandmontage";
    let lv = PrintLv {
        items: vec![PrintItem::Title(PrintTitle {
            lump: false,
            oz: "01".into(),
            label: String::new(),
            text: "Feldgeräte".into(),
            total: None,
            items: vec![PrintItem::Position(PrintPosition {
                oz: "01.0010".into(),
                short_text: word.into(),
                quantity: Some(d("1")),
                unit: "St".into(),
                unit_price: Some(d("10")),
                ..PrintPosition::default()
            })],
        })],
        ..PrintLv::default()
    };
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        // Pieces of the short text, joined per line (same page and baseline).
        let mut pieces: Vec<(usize, f64, f64, String)> = placed_runs(&document)
            .into_iter()
            .filter(|run| run.3.chars().count() >= 5 && word.contains(run.3.as_str()))
            .collect();
        pieces.sort_by(|a, b| (a.0, a.1, a.2).partial_cmp(&(b.0, b.1, b.2)).unwrap());
        let mut lines: Vec<(usize, f64, String)> = Vec::new();
        for (page, y, _, text) in pieces {
            match lines.last_mut() {
                Some(line) if line.0 == page && (line.1 - y).abs() < 0.5 => line.2.push_str(&text),
                _ => lines.push((page, y, text)),
            }
        }
        let texts: Vec<&str> = lines.iter().map(|line| line.2.as_str()).collect();
        assert!(texts.len() >= 2, "Kurztext bricht um: {texts:?}");
        for line in &texts[..texts.len() - 1] {
            assert!(
                line.ends_with('-'),
                "Umbruch nach dem Bindestrich: {texts:?}"
            );
        }
        assert_eq!(
            texts.concat(),
            word,
            "vollständig, ohne zusätzliche Zeichen"
        );
    }
}

#[test]
fn every_template_prints_break_chances_without_a_character() {
    // common.typ defines `break-chances` and calls it in `setup`; cover.typ and
    // points.typ are only imported. Every other template is a printout of its own.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".typ")
            || name == "common.typ"
            || name == "cover.typ"
            || name == "points.typ"
        {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(
            source.contains("#show: setup.with(") || source.contains("#show: break-chances"),
            "{name}: weder setup noch break-chances – U+200B käme in den PDF-Text"
        );
        checked += 1;
    }
    assert!(checked >= 10, "{checked} Vorlagen geprüft");
    let common = include_str!("../templates/common.typ");
    assert!(
        common.contains("show: break-chances"),
        "setup ruft break-chances auf"
    );
}

#[test]
fn pdfs_stay_tagged() {
    // poppler up to 25.x reports „Suspects object is wrong type (boolean)“ for
    // tagged Typst PDFs (see PrintWorld::pdf) – no reason to switch tags off.
    let pdf = render_lv_pdf(&sample_lv(), LvPrintMode::Priced).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert!(
        raw.contains("/MarkInfo") && raw.contains("/Marked true"),
        "Tagged PDF"
    );
}

/// Währung of the LV (CSV import in CHF) also in Aufmaßprüfung,
/// Rechnungsfreigabe, Niederschrift, Kosten nach Kostengruppen and the
/// placeholder {waehrung} of header and footer.
#[test]
fn billing_submission_and_cost_groups_print_in_the_currency_of_the_lv() {
    let chf = PrintHeader {
        currency: "CHF".into(),
        frame: crate::frame::PrintFrame {
            footer: [
                "Alle Einzelbeträge netto in {waehrung}".into(),
                String::new(),
                String::new(),
            ],
            ..crate::frame::PrintFrame::default()
        },
        ..PrintHeader::default()
    };
    // Narrow columns break „100.000,00 CHF“ into two lines: one space.
    let text_of = |world: crate::world::PrintWorld| {
        inspect(&world.layout().unwrap())
            .0
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };

    let release = PrintRelease {
        header: chf.clone(),
        invoice: "1. Abschlagsrechnung AR-1".into(),
        overview: vec![("Gesamt, netto".into(), Some(d("100000")), Some(d("60000")))],
        calculation: vec![
            PrintAmountLine::new("− Sicherheitseinbehalt", d("-6000")),
            PrintAmountLine::strong("= Freigabe, netto", d("24000")),
        ],
        ..PrintRelease::default()
    };
    let text = text_of(crate::billing::release_world(&release).unwrap());
    for token in [
        "100.000,00 CHF",
        "−6.000,00 CHF",
        "24.000,00 CHF",
        "netto in CHF",
    ] {
        assert!(text.contains(token), "{token} fehlt in: {text}");
    }
    assert!(!text.contains('€'), "{text}");

    let mut check = billing_check_sample();
    check.header = chf.clone();
    let text = text_of(crate::billing::billing_check_world(&check).unwrap());
    assert!(text.contains(" CHF") && !text.contains(" €"), "{text}");

    let submission = PrintSubmission {
        header: chf.clone(),
        bidders: vec![crate::submission::SubmissionBidder {
            name: "Müller GmbH".into(),
            net: Some(Decimal::from(100_000)),
            ..crate::submission::SubmissionBidder::default()
        }],
        vat_percent: Decimal::from(19),
        ..PrintSubmission::default()
    };
    let text = text_of(crate::submission::submission_world(&submission).unwrap());
    assert!(
        text.contains("100.000,00 CHF") && text.contains("119.000,00 CHF"),
        "{text}"
    );

    let report = PrintCostGroups {
        header: chf,
        catalog: "DIN 276".into(),
        planner_total: Decimal::from(500),
        ..PrintCostGroups::default()
    };
    let mut files = view::Files::default();
    let data = view::cost_groups(&report, &mut files);
    let text = text_of(build_world(COST_GROUPS_TEMPLATE, files, &data).unwrap());
    assert!(text.contains("Beträge in CHF"), "{text}");
}

fn sample_cost_estimate(calculation: bool) -> PrintCostEstimate {
    let kinds: Vec<PrintPointKind> = [
        ("AI", "Analoge Eingabe", false),
        ("BI", "Binäre Eingabe", false),
        ("AV", "Analogwert", true),
    ]
    .iter()
    .map(|(key, label, virtual_point)| PrintPointKind {
        key: key.to_string(),
        label: label.to_string(),
        virtual_point: *virtual_point,
    })
    .collect();
    let sum = |count: u64, price: i64| PrintKindSum {
        count,
        unit_price: Decimal::from(price),
        amount: Decimal::from(count as i64 * price),
    };
    let shares = |total: i64| -> Vec<PrintShare> {
        [
            ("481", "Automationseinrichtungen", 60),
            ("484", "Kabel, Leitungen und Verlegesysteme", 40),
        ]
        .iter()
        .map(|(code, label, percent)| PrintShare {
            code: code.to_string(),
            label: label.to_string(),
            percent: Decimal::from(*percent),
            amount: Decimal::from(total * percent / 100),
        })
        .collect()
    };
    let section = |name: &str, counts: [u64; 3]| {
        let kinds = vec![
            sum(counts[0], 400),
            sum(counts[1], 400),
            sum(counts[2], 200),
        ];
        let total: i64 = kinds
            .iter()
            .map(|kind| kind.amount.to_string().parse::<i64>().unwrap())
            .sum();
        PrintCostSection {
            name: name.into(),
            rows: vec![PrintPointRow {
                location: "ASP01 · UG".into(),
                trade: "RLT".into(),
                plant: format!("Lüftung {name} & Co <1>"),
                remark: "#1 *fett*".into(),
                counts: counts.iter().map(|count| *count as u32).collect(),
            }],
            points: counts.iter().sum(),
            total: Decimal::from(total),
            shares: if calculation {
                shares(total)
            } else {
                Vec::new()
            },
            percent_sum: Decimal::ONE_HUNDRED,
            kinds,
            ..PrintCostSection::default()
        }
    };
    let sections = vec![
        section("Bestand", [10, 20, 30]),
        section("Neubau", [5, 5, 10]),
    ];
    let total: Decimal = sections.iter().map(|section| section.total).sum();
    PrintCostEstimate {
        calculation,
        stage: if calculation {
            "Kostenberechnung"
        } else {
            "Kostenschätzung"
        }
        .into(),
        level: if calculation {
            "DIN 276, 3. Ebene"
        } else {
            "DIN 276, 2. Ebene"
        }
        .into(),
        title: "Kostenberechnung LPH 3".into(),
        kinds,
        groups: if calculation {
            shares(total.to_string().parse().unwrap())
        } else {
            Vec::new()
        },
        total,
        points: 80,
        kind_sums: vec![sum(15, 400), sum(25, 400), sum(40, 200)],
        sections,
        rate: (!calculation).then(|| Decimal::from(450)),
        warnings: vec!["Bauteil „Neubau“: Die Aufteilung ergibt 98 % statt 100 %.".into()],
        control: Some(PrintCostControl {
            lines: vec![PrintControlLine {
                code: "481".into(),
                label: "Automationseinrichtungen".into(),
                estimate: Decimal::from(14_400),
                planner: Decimal::from(15_000),
                awarded: Decimal::ZERO,
            }],
            estimate: Decimal::from(14_400),
            planner: Decimal::from(15_000),
            ..PrintCostControl::default()
        }),
        ..PrintCostEstimate::default()
    }
}

#[test]
fn cost_estimate_pdf_shows_sections_split_check_and_control() {
    for calculation in [true, false] {
        let model = sample_cost_estimate(calculation);
        let mut files = view::Files::default();
        let data = crate::cost_estimate::cost_estimate_json(&model, &mut files);
        let world =
            crate::build_world(include_str!("../templates/cost_estimate.typ"), files, &data)
                .unwrap();
        let document = world.layout().unwrap();
        let page = &document.pages()[0].frame;
        assert!(page.width() > page.size().y, "A4 quer");
        let (text, _) = inspect(&document);
        let squeezed: String = text.split_whitespace().collect();
        for token in [
            "Bauteil:Bestand",
            "Bauteil:Neubau",
            "Zusammenfassung",
            "Lüftung",
            "&Co<1>",
            "#1*fett*",
            "Kostenkontrolle",
            "Erläuterungen",
            "Hinweise",
            "98%statt100%",
        ] {
            assert!(
                squeezed.contains(token),
                "{token} fehlt (calculation {calculation})"
            );
        }
        if calculation {
            assert!(
                squeezed.contains("Prüfsumme") && squeezed.contains("KG481"),
                "Aufteilung"
            );
            assert!(
                squeezed.contains("1.1.1") && squeezed.contains("1.2.1"),
                "DP-Nummern 1.1/1.2"
            );
        } else {
            assert!(squeezed.contains("450,00€/DP"), "Kennwert");
        }
    }
    let model = PrintCostEstimate {
        options: PrintCostOptions {
            cover: true,
            chart: false,
            plants: false,
            landscape: false,
        },
        ..sample_cost_estimate(true)
    };
    let (pdf, pages) = render_cost_estimate_pdf_counted(&model).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(
        pages >= 4,
        "Deckblatt, Zusammenfassung, zwei Bauteile: {pages}"
    );
}

// ---------- PDF-Lesezeichen ----------

/// The outline (Lesezeichen) of a PDF as (level, title) in reading order,
/// read with the PDF parser that is part of the print crate (no program).
pub(crate) fn outline_of(pdf: &[u8]) -> Vec<(usize, String)> {
    use hayro::hayro_syntax::object::{Dict, String as PdfString};
    use hayro::hayro_syntax::Pdf;
    // Text strings: UTF-16BE with byte order mark, otherwise ASCII.
    fn title(bytes: &[u8]) -> String {
        match bytes.strip_prefix(&[0xFE, 0xFF]) {
            Some(utf16) => String::from_utf16_lossy(
                &utf16
                    .chunks(2)
                    .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
                    .collect::<Vec<_>>(),
            ),
            None => bytes.iter().map(|&byte| byte as char).collect(),
        }
    }
    fn walk(mut item: Option<Dict<'_>>, level: usize, out: &mut Vec<(usize, String)>) {
        while let Some(dict) = item {
            assert!(out.len() < 100_000, "Gliederung ohne Ende");
            let text = dict
                .get::<PdfString>("Title")
                .map(|t| title(t.as_bytes()))
                .unwrap_or_default();
            out.push((level, text));
            walk(dict.get::<Dict>("First"), level + 1, out);
            item = dict.get::<Dict>("Next");
        }
    }
    let pdf = Pdf::new(pdf.to_vec()).expect("PDF lesbar");
    let xref = pdf.xref();
    let catalog: Dict = xref.get(xref.root_id()).expect("Katalog");
    let mut out = Vec::new();
    if let Some(outlines) = catalog.get::<Dict>("Outlines") {
        walk(outlines.get::<Dict>("First"), 1, &mut out);
    }
    out
}

/// Everything a page shows – text runs, shapes, images – with its place on
/// the page; tags (where the Lesezeichen live) and link areas are left out.
pub(crate) fn drawing(document: &PagedDocument) -> Vec<String> {
    fn walk(frame: &Frame, dx: f64, dy: f64, page: usize, out: &mut Vec<String>) {
        for (point, item) in frame.items() {
            let (x, y) = (dx + point.x.to_pt(), dy + point.y.to_pt());
            match item {
                FrameItem::Group(group) => walk(
                    &group.frame,
                    x + group.transform.tx.to_pt(),
                    y + group.transform.ty.to_pt(),
                    page,
                    out,
                ),
                FrameItem::Text(t) => out.push(format!(
                    "{page} {x:.3} {y:.3} Text {:?} {:?} {}",
                    t.text,
                    t.size,
                    t.glyphs.len()
                )),
                FrameItem::Shape(shape, _) => {
                    out.push(format!("{page} {x:.3} {y:.3} Form {shape:?}"))
                }
                FrameItem::Image(_, size, _) => {
                    out.push(format!("{page} {x:.3} {y:.3} Bild {size:?}"))
                }
                FrameItem::Link(..) | FrameItem::Tag(..) => {}
            }
        }
    }
    let mut out = Vec::new();
    for (page, p) in document.pages().iter().enumerate() {
        out.push(format!("Seite {page} {:?}", p.frame.size()));
        walk(&p.frame, 0.0, 0.0, page, &mut out);
    }
    out
}

/// Lays the printout out with and without Lesezeichen: same pages, same
/// texts and lines at the same places. Returns the outline of the PDF.
pub(crate) fn bookmarks_without_change(
    world: impl Fn() -> crate::world::PrintWorld,
    what: &str,
) -> Vec<(usize, String)> {
    let with = world();
    let document = with.layout().unwrap();
    let plain_world = world().without_bookmarks();
    let plain = plain_world.layout().unwrap();
    assert!(
        outline_of(&plain_world.pdf(&plain).unwrap()).is_empty(),
        "{what}: Vergleich ohne Lesezeichen"
    );
    let (a, b) = (drawing(&document), drawing(&plain));
    assert_eq!(
        a.len(),
        b.len(),
        "{what}: Druckbild mit/ohne Lesezeichen verschieden lang"
    );
    for (index, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(x, y, "{what}: Druckbild weicht ab bei Eintrag {index}");
    }
    let outline = outline_of(&with.pdf(&document).unwrap());
    // Plain one-line texts without break marks.
    for (_, text) in &outline {
        assert!(
            !text.is_empty()
                && !text.contains(['\u{200B}', '\n', '\r', '\t'])
                && text.trim() == text,
            "{what}: Lesezeichen {text:?}"
        );
    }
    outline
}

fn entries(outline: &[(usize, String)]) -> Vec<String> {
    outline
        .iter()
        .map(|(level, text)| format!("{level} {text}"))
        .collect()
}

fn bookmark_lv() -> PrintLv {
    let position = |oz: &str, text: &str| {
        PrintItem::Position(PrintPosition {
            oz: oz.into(),
            short_text: text.into(),
            long_text: vec![TextBlock::text("Liefern und montieren.")],
            quantity: Some(d("2")),
            unit: "St".into(),
            unit_price: Some(d("10")),
            ..PrintPosition::default()
        })
    };
    PrintLv {
        header: PrintHeader {
            project_name: "Rathaus 3".into(),
            lv_name: "LV 480 Gebäudeautomation".into(),
            date: "18.11.2026".into(),
            ..PrintHeader::default()
        },
        items: vec![
            PrintItem::Note(vec![TextBlock::text(
                "Allgemeine Vorbemerkung ohne Überschrift.",
            )]),
            PrintItem::Performance(
                "A0001".into(),
                vec![
                    TextBlock::paragraph(vec![TextRun::bold("Kabel für Bus-Anbindungen")]),
                    TextBlock::text("Die folgenden Kabel werden in Kabelrinnen verlegt."),
                ],
            ),
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: "01".into(),
                label: String::new(),
                text: "Automationseinrichtungen".into(),
                total: None,
                items: vec![PrintItem::Title(PrintTitle {
                    lump: false,
                    oz: "01.481".into(),
                    label: String::new(),
                    text: "Automationsstationen".into(),
                    total: None,
                    items: vec![
                        PrintItem::Note(vec![
                            TextBlock::paragraph(vec![TextRun::bold("Hinweis zur Montage")]),
                            TextBlock::text("Montage im Schaltschrank."),
                        ]),
                        position("01.481.0010", "DDC-Station"),
                        position("01.481.0020", "Bediengerät für Räume"),
                    ],
                })],
            }),
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: "02".into(),
                label: String::new(),
                text: "Feldgeräte".into(),
                total: None,
                items: vec![position("02.0010", "Raumfühler")],
            }),
        ],
        cover: Some(PrintCover {
            lv: "LV 480".into(),
            project: "Rathaus 3".into(),
            sections: vec![CoverSection {
                heading: "Sicherheiten".into(),
                rows: vec![("Vertragserfüllung".into(), "5 %".into())],
            }],
            ..PrintCover::default()
        }),
        ..PrintLv::default()
    }
}

#[test]
fn lv_bookmarks_follow_titles_positions_and_sections() {
    let lv = bookmark_lv();
    let options = PrintOptions {
        contents: true,
        ..PrintOptions::default()
    };
    let outline = bookmarks_without_change(
        || lv_world(&lv, LvPrintMode::Blank, &options).unwrap(),
        "Angebotsaufforderung",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 Deckblatt",
            "1 Vertragsbedingungen",
            "1 Inhaltsverzeichnis",
            "1 Vorbemerkungen",
            "1 A0001 Kabel für Bus-Anbindungen",
            "1 01 Automationseinrichtungen",
            "2 01.481 Automationsstationen",
            "3 Hinweis zur Montage",
            "3 01.481.0010 DDC-Station",
            "3 01.481.0020 Bediengerät für Räume",
            "1 02 Feldgeräte",
            "2 02.0010 Raumfühler",
            "1 Zusammenstellung",
            "1 Angaben des Bieters",
        ]
    );

    // Lesezeichen only for titles (print option): Hinweise and
    // Ausführungsbeschreibungen keep theirs; priced without Bieterangaben;
    // without Zusammenstellung the sums are „Gesamtsumme“.
    let mut lv = bookmark_lv();
    lv.cover = None;
    let options = PrintOptions {
        bookmark_positions: false,
        summary: false,
        ..PrintOptions::default()
    };
    let outline = bookmarks_without_change(
        || lv_world(&lv, LvPrintMode::Priced, &options).unwrap(),
        "LV mit Preisen",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 Vorbemerkungen",
            "1 A0001 Kabel für Bus-Anbindungen",
            "1 01 Automationseinrichtungen",
            "2 01.481 Automationsstationen",
            "3 Hinweis zur Montage",
            "1 02 Feldgeräte",
            "1 Gesamtsumme",
        ]
    );

    // Summenblatt: only the Zusammenstellung.
    let options = PrintOptions {
        summary_only: true,
        ..PrintOptions::default()
    };
    let outline = bookmarks_without_change(
        || lv_world(&lv, LvPrintMode::Priced, &options).unwrap(),
        "Zusammenstellung",
    );
    assert_eq!(entries(&outline), ["1 Zusammenstellung"]);
}

#[test]
fn titles_over_several_pages_have_one_bookmark_on_their_first_page() {
    // Title rows repeat on every page (table header): one Lesezeichen each.
    let lv = two_title_lv();
    for (options, positions) in [
        (PrintOptions::default(), 90),
        (
            PrintOptions {
                bookmark_positions: false,
                carry: true,
                contents: true,
                ..PrintOptions::default()
            },
            0,
        ),
    ] {
        let outline = bookmarks_without_change(
            || lv_world(&lv, LvPrintMode::Priced, &options).unwrap(),
            "LV über mehrere Seiten",
        );
        let titles: Vec<_> = outline
            .iter()
            .filter(|(level, _)| *level == 1)
            .map(|(_, text)| text.as_str())
            .collect();
        let expected_titles: &[&str] = if options.contents {
            &[
                "Inhaltsverzeichnis",
                "01 Anlage Nord",
                "02 Anlage Sued",
                "Zusammenstellung",
            ]
        } else {
            &["01 Anlage Nord", "02 Anlage Sued", "Zusammenstellung"]
        };
        assert_eq!(titles, expected_titles);
        assert_eq!(
            outline.iter().filter(|(level, _)| *level == 2).count(),
            positions
        );
        if positions > 0 {
            assert_eq!(outline[1], (2, "01.0001 Leistung 1".to_string()));
        }
    }
    // The Lesezeichen of „02 Anlage Sued“ points to the page its title row
    // is printed first.
    use typst::foundations::NativeElement;
    use typst::introspection::Introspector;
    let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let introspector = document.introspector();
    let pages: Vec<(String, usize)> = introspector
        .query(&typst::model::HeadingElem::ELEM.select())
        .iter()
        .filter_map(|heading| {
            let heading = heading.to_packed::<typst::model::HeadingElem>()?;
            let page = introspector.page(heading.location()?)?.get();
            Some((heading.body.plain_text().to_string(), page))
        })
        .collect();
    let first_page_of = |text: &str| {
        placed_texts(&document)
            .iter()
            .find(|t| t.1 == text)
            .map(|t| t.0 + 1)
            .unwrap()
    };
    let sued = pages
        .iter()
        .find(|(text, _)| text == "02 Anlage Sued")
        .unwrap();
    assert_eq!(sued.1, first_page_of("Anlage Sued"));
    assert!(sued.1 > 1, "Titel 02 nicht auf Seite 1: {pages:?}");
}

#[test]
fn every_printout_has_bookmarks_and_prints_as_before() {
    let outline = bookmarks_without_change(
        || price_comparison_world(&sample_pc(3)).unwrap(),
        "Preisspiegel",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 Übersicht der Angebote",
            "1 01 Feldgeräte",
            "1 Angebotssummen"
        ]
    );

    let report = PrintCostGroups {
        catalog: "DIN 276-18".into(),
        bidders: vec!["Bieter A".into()],
        ..PrintCostGroups::default()
    };
    let outline = bookmarks_without_change(
        || {
            let mut files = view::Files::default();
            let data = view::cost_groups(&report, &mut files);
            build_world(COST_GROUPS_TEMPLATE, files, &data).unwrap()
        },
        "Kostengruppen",
    );
    assert_eq!(entries(&outline), ["1 Kosten nach Kostengruppen"]);

    for cover in [false, true] {
        let mut model = sample_cost_estimate(true);
        model.options.cover = cover;
        let outline = bookmarks_without_change(
            || {
                let mut files = view::Files::default();
                let data = crate::cost_estimate::cost_estimate_json(&model, &mut files);
                build_world(include_str!("../templates/cost_estimate.typ"), files, &data).unwrap()
            },
            "Kostenermittlung",
        );
        let mut expected = vec![
            "1 Zusammenfassung",
            "1 Bauteil: Bestand",
            "1 Bauteil: Neubau",
            "1 Kostenkontrolle je Kostengruppe",
            "1 Erläuterungen und Annahmen",
        ];
        if cover {
            expected.insert(0, "1 Deckblatt");
        }
        assert_eq!(entries(&outline), expected);
    }

    let check = billing_check_sample();
    let outline = bookmarks_without_change(
        || crate::billing::billing_check_world(&check).unwrap(),
        "Aufmaßprüfung",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 2. Abschlagsrechnung AR-2 vom 30.04.2027",
            "1 01 Feldgeräte",
            "2 01.0010 Raumtemperaturfühler",
            "1 Summen und Befunde",
        ]
    );

    let release = PrintRelease {
        invoice: "3. Abschlagsrechnung AR-3".into(),
        earlier: vec![PrintEarlierRelease {
            label: "1. Abschlagsrechnung AR-1".into(),
            ..PrintEarlierRelease::default()
        }],
        signatures: vec!["Planer".into()],
        ..PrintRelease::default()
    };
    let outline = bookmarks_without_change(
        || crate::billing::release_world(&release).unwrap(),
        "Rechnungsfreigabe",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 3. Abschlagsrechnung AR-3",
            "1 Leistung",
            "1 Rechenweg der Freigabe",
            "1 Bisherige Freigaben",
            "1 Unterschriften",
        ]
    );

    let submission = PrintSubmission {
        bidders: vec![
            SubmissionBidder {
                name: "Müller GmbH".into(),
                ..SubmissionBidder::default()
            },
            SubmissionBidder {
                name: "Schmidt KG".into(),
                ..SubmissionBidder::default()
            },
        ],
        signatures: vec!["Verhandlungsleiter".into()],
        ..PrintSubmission::default()
    };
    let outline = bookmarks_without_change(
        || crate::submission::submission_world(&submission).unwrap(),
        "Niederschrift",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 Angebote",
            "2 Müller GmbH",
            "2 Schmidt KG",
            "1 Unterschriften"
        ]
    );

    let list = PrintChangeList {
        entries: vec![
            PrintChange {
                kind: "geändert".into(),
                row_kind: "Position".into(),
                ordinal: "01.484.10.0080".into(),
                text: "Kabelabzweigkasten".into(),
                ..PrintChange::default()
            },
            PrintChange {
                kind: "neu".into(),
                row_kind: "Titel".into(),
                ordinal: "02".into(),
                text: "Außenanlagen".into(),
                ..PrintChange::default()
            },
        ],
        ..PrintChangeList::default()
    };
    let outline = bookmarks_without_change(
        || {
            let mut files = view::Files::default();
            let data = view::change_list(&list, &mut files);
            build_world(CHANGES_TEMPLATE, files, &data).unwrap()
        },
        "Änderungsliste",
    );
    assert_eq!(
        entries(&outline),
        [
            "1 01.484.10.0080 Kabelabzweigkasten (geändert)",
            "1 02 Außenanlagen (neu)"
        ]
    );

    let letter = |name: &str| PrintLetter {
        recipient: vec![name.into(), "Hafenstraße 2".into()],
        subject: "Aufforderung zur Abgabe eines Angebots".into(),
        body: vec![TextBlock::text("Sehr geehrte Damen und Herren,")],
        ..PrintLetter::default()
    };
    let letters = PrintLetters {
        title: "Aufforderung".into(),
        letters: vec![letter("Müller GmbH"), letter("Schmidt KG")],
        ..PrintLetters::default()
    };
    let outline = bookmarks_without_change(
        || crate::letter::letters_world(&letters).unwrap(),
        "Schreiben",
    );
    assert_eq!(entries(&outline), ["1 Müller GmbH", "1 Schmidt KG"]);
}

/// Messung (docs/BUILD.md, Messprogramme): Lesezeichen bei 2000 Positionen in 20 Titeln.
/// One variant per process (Typst caches layouts within a process):
/// `PROAVA_LZ_VARIANT=ohne|titel|alle [PROAVA_LZ_ABSAETZE=8] cargo test -p
/// proava-print -- --ignored bookmark_cost --nocapture`.
#[test]
#[ignore]
fn bookmark_cost_with_2000_positions() {
    // Absätze Langtext je Position (PROAVA_LZ_ABSAETZE, Standard 2).
    let paragraphs: usize = std::env::var("PROAVA_LZ_ABSAETZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let items = (1..=20)
        .map(|t| {
            PrintItem::Title(PrintTitle {
                lump: false,
                oz: format!("{t:02}"),
                label: String::new(),
                text: format!("Anlage {t}"),
                total: None,
                items: (1..=100)
                    .map(|n| {
                        PrintItem::Position(PrintPosition {
                            oz: format!("{t:02}.{n:04}"),
                            short_text: format!("Raumtemperaturfühler Typ {n}"),
                            long_text: (0..paragraphs)
                                .map(|_| {
                                    TextBlock::text(
                                        "Liefern und montieren, einschließlich Befestigung, Anschluss und \
                                         Inbetriebnahme; Messbereich 0 bis 50 °C, Ausgang 0–10 V.",
                                    )
                                })
                                .collect(),
                            quantity: Some(d("3")),
                            unit: "St".into(),
                            unit_price: Some(d("85.5")),
                            ..PrintPosition::default()
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    let lv = PrintLv {
        items,
        ..PrintLv::default()
    };
    let variant = std::env::var("PROAVA_LZ_VARIANT").unwrap_or_else(|_| "alle".into());
    let options = PrintOptions {
        bookmark_positions: variant == "alle",
        ..PrintOptions::default()
    };
    let start = std::time::Instant::now();
    let mut world = lv_world(&lv, LvPrintMode::Priced, &options).unwrap();
    if variant == "ohne" {
        world = world.without_bookmarks();
    }
    let document = world.layout().unwrap();
    let laid_out = start.elapsed();
    let pdf = world.pdf(&document).unwrap();
    let written = start.elapsed() - laid_out;
    let outline = outline_of(&pdf);
    println!(
        "Lesezeichen {variant}: {} Seiten, Satz {:.2} s, PDF {:.2} s, {} KB, {} Lesezeichen",
        document.pages().len(),
        laid_out.as_secs_f64(),
        written.as_secs_f64(),
        pdf.len() / 1024,
        outline.len()
    );
}

/// Ein Langtext als *ein* Absatz mit vielen Zeilenumbrüchen (so liefern ihn andere
/// AVA-Programme), länger als eine Seite: Der letzte Block steht mit der Preiszeile
/// zusammen – er darf trotzdem nicht über das Seitenende hinauslaufen, sonst fehlt
/// der Rest des Textes im PDF.
#[test]
fn a_last_paragraph_longer_than_a_page_breaks_across_pages() {
    let mut lv = two_title_lv();
    let lines: Vec<String> = (1..=90)
        .map(|n| format!("Zeile {n} eines langen Absatzes mit etwas Text"))
        .collect();
    let long = format!("{}\nENDEDESTEXTES", lines.join("\n"));
    if let PrintItem::Title(title) = &mut lv.items[0] {
        if let PrintItem::Position(position) = &mut title.items[1] {
            position.long_text = vec![TextBlock::text(&long)];
            position.unit_price = Some(d("20000"));
        }
    }
    for mode in [LvPrintMode::Blank, LvPrintMode::Priced] {
        let document = lv_world(&lv, mode, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let height = document.pages()[0].frame.height().to_mm();
        let runs = placed_runs(&document);
        for (page, y, _, text) in &runs {
            assert!(
                *y < height,
                "Seite {}: {text:?} bei {y:.0} mm liegt unter dem Seitenende ({height:.0} mm)",
                page + 1
            );
        }
        let all: String = runs.iter().map(|run| run.3.as_str()).collect();
        assert!(all.contains("ENDEDESTEXTES"), "Textende fehlt");
        assert!(all.contains("Zeile 90 eines"), "letzte Zeile fehlt");
    }
}

/// Schlussbemerkung (Anwender 18.11., wie Planer-Ausdrucke sie setzen): nach den Summen
/// auf eigener Seite, mit Lesezeichen nach der Zusammenstellung und vor den
/// Angaben des Bieters – auch wenn sie im LV im letzten Titel steht.
#[test]
fn a_closing_text_prints_after_the_totals_with_a_bookmark() {
    let mut lv = bookmark_lv();
    lv.cover = None;
    let PrintItem::Title(last) = lv.items.last_mut().unwrap() else {
        panic!()
    };
    last.items.push(PrintItem::Closing(vec![
        TextBlock::paragraph(vec![TextRun::bold("Schlussblatt")]),
        TextBlock::text("Der Bieter gibt die Zahl der Arbeitskräfte an."),
    ]));
    let outline = bookmarks_without_change(
        || lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default()).unwrap(),
        "Angebotsaufforderung",
    );
    let names = entries(&outline);
    let at = |name: &str| names.iter().position(|entry| entry == name).expect(name);
    assert!(at("1 Zusammenstellung") < at("1 Schlussblatt"), "{names:?}");
    assert!(
        at("1 Schlussblatt") < at("1 Angaben des Bieters"),
        "{names:?}"
    );

    let document = lv_world(&lv, LvPrintMode::Blank, &PrintOptions::default())
        .unwrap()
        .layout()
        .unwrap();
    let runs = placed_runs(&document);
    let page_of = |text: &str| runs.iter().find(|run| run.3.contains(text)).expect(text).0;
    assert!(
        page_of("Arbeitskräfte") > page_of("brutto"),
        "eigene Seite nach den Summen"
    );
    assert!(page_of("Arbeitskräfte") > page_of("Raumfühler"));
}

/// The LV with the planner's prices prints as Kostenvoranschlag (DIN 276,
/// 4.3.5) in title block and page header; the Angebotsaufforderung keeps its name.
#[test]
fn priced_lv_is_called_kostenvoranschlag() {
    let lv = sample_lv();
    let mut files = crate::view::Files::default();
    let priced = crate::view::lv(
        &lv,
        LvPrintMode::Priced,
        &PrintOptions::default(),
        &mut files,
    );
    assert_eq!(priced["header"]["kind"], "Kostenvoranschlag");
    let blank = crate::view::lv(
        &lv,
        LvPrintMode::Blank,
        &PrintOptions::default(),
        &mut files,
    );
    assert_eq!(blank["header"]["kind"], "Angebotsaufforderung");
}

/// Baselines (y in pt from the top of the page) of the text items on the
/// first page with their text.
fn baselines(frame: &Frame, offset: f64, out: &mut Vec<(f64, String)>) {
    for (position, item) in frame.items() {
        let y = offset + position.y.to_pt();
        match item {
            FrameItem::Group(group) => baselines(&group.frame, y + group.transform.ty.to_pt(), out),
            FrameItem::Text(text) => out.push((y, text.text.to_string())),
            _ => {}
        }
    }
}

/// Short lines as paragraphs of their own must not stand further apart
/// than the lines of a paragraph. Default (as AVA printouts): every
/// line of the text the same distance (1.15 × font size), an empty
/// paragraph one empty line; Zeilenabstand and Abstand nach Absatz from the
/// settings (the text editor uses the same).
#[test]
fn text_lines_and_paragraphs_follow_the_spacing_setting() {
    let lines = ["Art: Wandschrank", "Platzreserve: 20 %", "Tiefe: 400 mm"];
    let mut long_text: Vec<TextBlock> = lines.iter().map(|line| TextBlock::text(*line)).collect();
    long_text.insert(1, TextBlock::paragraph(Vec::new()));
    long_text.push(TextBlock::text("Ende des Textes"));
    let mut lv = PrintLv {
        items: vec![PrintItem::Position(PrintPosition {
            oz: "01.0010".into(),
            short_text: "Schaltschrank".into(),
            long_text,
            quantity: Some(d("1")),
            unit: "St".into(),
            ..PrintPosition::default()
        })],
        ..PrintLv::default()
    };
    let tops = |lv: &PrintLv| -> Vec<f64> {
        let document = lv_world(lv, LvPrintMode::Priced, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let mut found = Vec::new();
        baselines(&document.pages()[0].frame, 0.0, &mut found);
        assert!(
            !found.iter().any(|(_, text)| text == "X"),
            "der leere Absatz druckt nichts"
        );
        ["Art:", "Platzreserve:", "Tiefe:", "Ende"]
            .iter()
            .map(|word| {
                found
                    .iter()
                    .find(|(_, text)| text.starts_with(word))
                    .unwrap_or_else(|| panic!("{word} fehlt: {found:?}"))
                    .0
            })
            .collect()
    };
    let near = |a: f64, b: f64| (a - b).abs() < 0.05;
    let pitch = 1.15 * 9.0;
    let y = tops(&lv);
    assert!(
        near(y[1] - y[0], 2.0 * pitch),
        "Leerabsatz = eine Zeile: {y:?}"
    );
    assert!(near(y[2] - y[1], pitch), "kurze Absätze wie Zeilen: {y:?}");
    assert!(
        near(y[3] - y[2], pitch),
        "auch vor dem letzten Absatz: {y:?}"
    );

    lv.header.look.line_spacing_percent = 150;
    lv.header.look.paragraph_gap = ParagraphGap::Points(6);
    let mut files = crate::view::Files::default();
    let data = crate::view::lv(
        &lv,
        LvPrintMode::Priced,
        &PrintOptions::default(),
        &mut files,
    );
    assert_eq!(data["header"]["look"]["line"], 1.5);
    assert_eq!(data["header"]["look"]["gap"], 6);
    let y = tops(&lv);
    assert!(near(y[2] - y[1], 1.5 * pitch + 6.0), "1,5 und 6 pt: {y:?}");
    assert!(near(y[3] - y[2], 1.5 * pitch + 6.0), "{y:?}");

    lv.header.look.line_spacing_percent = 100;
    lv.header.look.paragraph_gap = ParagraphGap::HalfLine;
    let y = tops(&lv);
    assert!(near(y[2] - y[1], 1.5 * pitch), "halbe Zeile: {y:?}");
}
