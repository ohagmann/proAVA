//! Preisspiegel: Spaltenaufteilung bei 1 bis 6 Bietern (Kurztext mit
//! Mindestbreite, nie unter Menge und Einheit), ohne Planerpreise keine
//! Spalte, Hinweise zu unbekannten OZ und Rechenfehlern.

use super::*;

const SHORT: &str = "Automationseinrichtung für ASP13 Raumautomation";
const QUANTITY: &str = "12,5";

/// `bidders` bids with long Kurztexte, long OZ and prices as in a GA-LV of
/// about a million euros – with the Planerpreis column (the widest case).
fn long_pc(bidders: usize) -> PrintPriceComparison {
    let mut pc = sample_pc(bidders);
    pc.estimate = true;
    pc.estimate_net = Some(d("930125.00"));
    let PcItem::Title(title) = &mut pc.items[0] else {
        unreachable!()
    };
    title.items = (0..10)
        .map(|i| {
            PcItem::Position(PcPosition {
                oz: format!("01.481.10.{:04}", 10 * (i + 1)),
                short_text: SHORT.into(),
                quantity: Some(d("12.5")),
                unit: "Stk".into(),
                prices: (0..bidders)
                    .map(|b| BidPrice {
                        unit_price: Some(d("8636.46") + Decimal::from(b)),
                        total_price: Some(d("107955.75") + Decimal::from(12 * b)),
                        outlier: b == 0,
                        error: b == 1,
                    })
                    .collect(),
                estimate: BidPrice {
                    unit_price: Some(d("7441.06")),
                    total_price: Some(d("93013.25")),
                    ..BidPrice::default()
                },
                ..PcPosition::default()
            })
        })
        .collect();
    pc
}

fn layout(pc: &PrintPriceComparison) -> PagedDocument {
    price_comparison_world(pc).unwrap().layout().unwrap()
}

/// On every page with the positions table: the Kurztext ends before the
/// quantity starts, „Kurztext“ before „Menge“, the unit after the quantity.
/// Returns the narrowest Kurztext column (start of the Kurztext to the
/// start of the quantity) in mm.
fn assert_short_text_clear(document: &PagedDocument) -> f64 {
    let texts = placed_texts(document);
    let mut narrowest = f64::MAX;
    let mut checked = 0;
    for page in 0..document.pages().len() {
        let on: Vec<_> = texts.iter().filter(|t| t.0 == page).collect();
        let Some(head) = on.iter().find(|t| t.1 == "Kurztext") else {
            continue;
        };
        let menge = on.iter().find(|t| t.1 == "Menge").expect("Kopf Menge");
        assert!(
            head.3 < menge.2,
            "Seite {page}: „Kurztext“ endet bei {} mm, „Menge“ beginnt bei {} mm",
            head.3,
            menge.2
        );
        let quantities: Vec<_> = on.iter().filter(|t| t.1 == QUANTITY).collect();
        if quantities.is_empty() {
            continue;
        }
        let quantity_start = quantities.iter().map(|t| t.2).fold(f64::MAX, f64::min);
        let quantity_end = quantities.iter().map(|t| t.3).fold(f64::MIN, f64::max);
        let unit_start = on
            .iter()
            .filter(|t| t.1 == "Stk")
            .map(|t| t.2)
            .fold(f64::MAX, f64::min);
        assert!(
            quantity_end < unit_start,
            "Seite {page}: Menge läuft in die Einheit"
        );
        for piece in on.iter().filter(|t| {
            let text = t.1.trim().trim_end_matches('-');
            text.chars().count() >= 4 && SHORT.contains(text) && t.2 >= head.2 - 0.3
        }) {
            assert!(
                piece.3 < quantity_start,
                "Seite {page}: „{}“ endet bei {} mm, die Menge beginnt bei {} mm",
                piece.1,
                piece.3,
                quantity_start
            );
            checked += 1;
        }
        narrowest = narrowest.min(quantity_start - head.2);
    }
    assert!(checked > 0, "Kurztexte gefunden");
    narrowest
}

#[test]
fn the_short_text_keeps_its_width_with_one_three_and_six_bidders() {
    for (bidders, a3) in [(1, false), (3, false), (6, true)] {
        let document = layout(&long_pc(bidders));
        let width = document.pages()[0].frame.width().to_mm();
        assert_eq!(
            (width - 420.0).abs() < 1.0,
            a3,
            "{bidders} Bieter: {width} mm breit"
        );
        let short = assert_short_text_clear(&document);
        assert!(
            short >= 44.0,
            "{bidders} Bieter: Kurztext nur {short:.1} mm breit"
        );
        let text = inspect(&document).0;
        assert!(
            !text.contains("Teil 1 von"),
            "{bidders} Bieter in einer Tabelle"
        );
        assert!(text.contains("Planerpreis"));
    }
}

#[test]
fn many_bidders_get_a_smaller_font_and_then_several_tables() {
    // One bidder: the usual 7.5 pt.
    let sizes = |document: &PagedDocument| -> Vec<f64> {
        fonts_of(document)
            .into_iter()
            .filter(|(text, _, _)| text.starts_with("Automationseinrichtung"))
            .map(|(_, _, size)| size)
            .collect()
    };
    let one = layout(&long_pc(1));
    assert!(sizes(&one).iter().all(|size| (*size - 7.5).abs() < 0.01));
    // Six bidders on A4 (A3 is the default from four on): they do not fit
    // next to a readable Kurztext even at 6 pt – several tables, each with
    // OZ, Kurztext, Menge and Einheit; Mittelwert, Minimum and Planerpreis
    // in the last one.
    let mut pc = long_pc(6);
    pc.paper = PaperSize::A4;
    let document = layout(&pc);
    assert!(document.pages()[0].frame.width().to_mm() < 300.0, "A4");
    let short = assert_short_text_clear(&document);
    assert!(short >= 44.0, "Kurztext nur {short:.1} mm breit");
    let text = inspect(&document).0;
    assert!(text.contains("Teil 1 von 2"), "{text}");
    assert!(text.contains("Teil 2 von 2"));
    assert!(text.contains("Bieter 1–3 von 6"));
    assert!(text.contains("Bieter 4–6 von 6, Mittelwert, Minimum, Planerpreis"));
    // Mittelwert and Minimum only from the second part on.
    let texts = placed_texts(&document);
    let second = texts
        .iter()
        .find(|t| t.1 == "Teil 2 von 2")
        .map(|t| t.0)
        .expect("Teil 2");
    assert!(second > 0, "Teil 2 auf einer neuen Seite");
    assert!(
        !texts.iter().any(|t| t.0 < second && t.1 == "Mittelwert"),
        "Mittelwert erst im letzten Teil"
    );
    assert!(texts.iter().any(|t| t.0 >= second && t.1 == "Mittelwert"));
}

/// Font size of the Kurztexte.
fn short_text_size(document: &PagedDocument) -> f64 {
    fonts_of(document)
        .into_iter()
        .find(|(text, _, _)| text.starts_with("Automationseinrichtung"))
        .map(|(_, _, size)| size)
        .expect("Kurztext")
}

#[test]
fn without_planner_prices_there_is_no_planner_column() {
    let without_estimate = |mut pc: PrintPriceComparison| {
        pc.estimate = false;
        pc.estimate_net = None;
        pc
    };
    let with_doc = layout(&long_pc(3));
    let without_doc = layout(&without_estimate(long_pc(3)));
    let text = inspect(&without_doc).0;
    assert!(!text.contains("Planerpreis"), "weder Spalte noch Erklärung");
    assert!(inspect(&with_doc).0.contains("Planerpreis"));
    // Three bidders: the room goes to the font (no smaller font needed) …
    assert!(short_text_size(&without_doc) > short_text_size(&with_doc));
    assert_short_text_clear(&without_doc);
    // … one bidder: to the Kurztext.
    let gained = assert_short_text_clear(&layout(&without_estimate(long_pc(1))))
        - assert_short_text_clear(&layout(&long_pc(1)));
    assert!(gained > 20.0, "Kurztext nur {gained:.1} mm breiter");
    let mut files = view::Files::default();
    let data = view::price_comparison(&without_estimate(long_pc(3)), &mut files);
    assert_eq!(data["estimate"], false);
}

#[test]
fn unknown_ordinals_and_arithmetic_errors_are_named_under_the_sums() {
    let mut pc = sample_pc(3);
    pc.bidders[1].unknown_ordinals = vec!["01.482.10.0090".into()];
    pc.bidders[1].arithmetic_errors = vec![PcArithmeticError {
        oz: "01.484.10.0080".into(),
        stated: d("20081.60"),
        computed: d("4620"),
    }];
    pc.bidders[2].unknown_ordinals = vec!["9.1".into(), "9.2".into()];
    let mut files = view::Files::default();
    let data = view::price_comparison(&pc, &mut files);
    assert_eq!(
        data["bid_notes"],
        serde_json::json!([
            "Angebot Beta $AG enthält 1 Position, die nicht im LV steht (OZ 01.482.10.0090); sie ist nicht gewertet.",
            "Angebot Beta $AG: 1 Rechenfehler, gewertet mit Menge × EP (OZ 01.484.10.0080: GP im Angebot 20.081,60, Menge × EP 4.620,00).",
            "Angebot Gamma <KG> enthält 2 Positionen, die nicht im LV stehen (OZ 9.1, 9.2); sie sind nicht gewertet.",
        ])
    );
    let text: String = placed_texts(&layout(&pc))
        .into_iter()
        .map(|t| t.1)
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("Hinweise zu den Angeboten"), "{text}");
    assert!(text.contains("nicht gewertet"));
    // Without remarks no heading.
    assert!(!inspect(&layout(&sample_pc(3)))
        .0
        .contains("Hinweise zu den Angeboten"));
}

/// Strukturabweichungen (Anwender 20.11.): a short note under the sums; the
/// lines inline when there are few, on an appendix page when there are many.
#[test]
fn structure_deviations_are_noted_and_many_go_to_the_appendix() {
    let mut pc = sample_pc(3);
    pc.bidders[0].structure = Some(PcStructure {
        summary: "Strukturabweichungen: 1 Menge".into(),
        sum_note: None,
        lines: vec!["Menge: 01.0010 – LV 10, Angebot 12 – gewertet mit der LV-Menge".into()],
    });
    pc.bidders[1].structure = Some(PcStructure {
        summary: "Strukturabweichungen: 12 umnummeriert (zugeordnet)".into(),
        sum_note: Some("Summe der Datei 1.200,00 € ≠ gewertet 1.000,00 € (Differenz 200,00 €): nicht zugeordnete Positionen 200,00 €".into()),
        lines: (1..=12)
            .map(|n| format!("umnummeriert: Angebot 09.{n:04} → LV 01.{n:04} – über Reihenfolge/Menge/Text zugeordnet (95 %)"))
            .collect(),
    });
    let mut files = view::Files::default();
    let data = view::price_comparison(&pc, &mut files);
    let notes: Vec<String> = data["bid_notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| note.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        notes[0],
        "Angebot Alpha #GmbH: Strukturabweichungen: 1 Menge – Menge: 01.0010 – LV 10, Angebot 12 – gewertet mit der LV-Menge."
    );
    assert!(notes[1].starts_with(
        "Angebot Beta $AG: Strukturabweichungen: 12 umnummeriert (zugeordnet); Summe der Datei"
    ));
    assert!(
        notes[1].ends_with("– Einzelliste (12 Einträge) im Anhang."),
        "{}",
        notes[1]
    );
    assert_eq!(data["appendix"].as_array().unwrap().len(), 1, "nur Beta");

    let document = layout(&pc);
    let texts = placed_texts(&document);
    let appendix = texts
        .iter()
        .find(|t| t.1 == "Anhang: Abweichungen der Angebote")
        .expect("Anhang");
    let last = document.pages().len() - 1;
    assert_eq!(appendix.0, last, "Anhang am Ende");
    let all: String = texts
        .iter()
        .filter(|t| t.0 == last)
        .map(|t| t.1.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(all.contains("Angebot 09.0012"), "{all}");
    assert!(all.contains("LV 01.0012"));
    // Without many deviations no appendix.
    pc.bidders[1].structure = None;
    let without = inspect(&layout(&pc)).0;
    assert!(!without.contains("Anhang: Abweichungen"));
    assert!(without.contains("Strukturabweichungen: 1 Menge"));
}
