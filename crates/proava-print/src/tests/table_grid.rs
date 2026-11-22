//! Druckoption „Tabellen voll umrandet (Gitter)“ (Anwender 20.11.): an
//! alle Spalten- und Zeilenlinien, aus die bisherigen Linien; im LV ein
//! Rahmen je Position mit der Preiszeile in Feldern. Erste Seite des
//! Preisspiegels: Übersicht mit netto und brutto, Legende daneben.

use super::*;

/// Line width of the Gitter (common.typ `grid-thin`).
const GRID: f64 = 0.4;
/// Width of the thin rules without the Gitter (common.typ `thin`).
const THIN: f64 = 0.5;

fn find<'a>(
    texts: &'a [(usize, String, f64, f64, f64)],
    wanted: &str,
) -> &'a (usize, String, f64, f64, f64) {
    texts
        .iter()
        .find(|t| t.1 == wanted)
        .unwrap_or_else(|| panic!("„{wanted}“ fehlt"))
}

#[test]
fn the_price_comparison_has_every_column_line_only_with_the_grid() {
    for grid in [true, false] {
        let mut pc = sample_pc(3);
        pc.header.look.table_grid = grid;
        let document = price_comparison_world(&pc).unwrap().layout().unwrap();
        let texts = texts_at(&document);
        let row = find(&texts, "01.0010");
        let lines = ruled_lines(&document);
        // Inside the text of the position row.
        let y = row.4 - 1.0;
        let grid_columns = columns_at(&lines, row.0, y, GRID);
        if grid {
            // OZ, Kurztext, Menge, Einheit, 3 × EP/GP/%, Mittelwert und
            // Minimum je EP/GP: 17 Spalten, 18 Linien mit Außenrahmen.
            assert_eq!(grid_columns.len(), 18, "{grid_columns:?}");
            // Lines between the rows: above the position (Gitter) and below
            // it (the stronger line over the Titelsumme).
            let around = lines
                .iter()
                .filter(|l| {
                    l.page == row.0
                        && !l.vertical()
                        && l.width >= GRID - 0.05
                        && (l.y0 - row.4).abs() < 6.0
                })
                .count();
            assert!(around >= 2, "waagrechte Linien um die Zeile: {around}");
        } else {
            assert!(grid_columns.is_empty(), "ohne Gitter keine Gitterlinien");
            // Only the lines between the bidders and the statistics.
            assert_eq!(columns_at(&lines, row.0, y, THIN).len(), 5);
        }
    }
}

#[test]
fn the_lv_frames_every_position_and_its_price_line_with_the_grid() {
    for grid in [true, false] {
        let mut lv = sample_lv();
        lv.header.look.table_grid = grid;
        let document = lv_world(&lv, LvPrintMode::Priced, &PrintOptions::default())
            .unwrap()
            .layout()
            .unwrap();
        let texts = texts_at(&document);
        let lines = ruled_lines(&document);
        let oz = find(&texts, "01.0010");
        let gp = texts
            .iter()
            .find(|t| t.1 == "2.469,00" && t.0 == oz.0)
            .expect("GP der Position");
        let at_text = columns_at(&lines, oz.0, oz.4 - 1.0, GRID);
        let at_prices = columns_at(&lines, gp.0, gp.4 - 1.0, GRID);
        if grid {
            // Text: frame left, OZ | Leistung, frame right – no lines in
            // the text. Price line: also Menge, Einheit, EP, GP.
            assert_eq!(at_text.len(), 3, "{at_text:?}");
            assert_eq!(at_prices.len(), 7, "{at_prices:?}");
        } else {
            assert!(at_text.is_empty() && at_prices.is_empty());
        }
    }
}

#[test]
fn the_first_page_puts_the_legend_beside_the_overview_with_net_and_gross() {
    let document = price_comparison_world(&sample_pc(3))
        .unwrap()
        .layout()
        .unwrap();
    let texts = texts_at(&document);
    let overview = find(&texts, "Übersicht der Angebote");
    let legend = find(&texts, "Legende");
    assert_eq!((overview.0, legend.0), (0, 0), "beide auf Seite 1");
    assert!((overview.4 - legend.4).abs() < 1.0, "auf einer Höhe");
    // Amounts of the overview (with „€“) end left of the legend.
    let below = find(&texts, "Alle Einzelbeträge netto in EUR");
    let amounts: Vec<_> = texts
        .iter()
        .filter(|t| t.0 == 0 && t.1.ends_with(" €") && t.4 > overview.4 && t.4 < below.4)
        .collect();
    assert!(!amounts.is_empty());
    for amount in &amounts {
        assert!(
            amount.3 < legend.2 - 2.0,
            "„{}“ endet bei {:.1} mm, die Legende beginnt bei {:.1} mm",
            amount.1,
            amount.3,
            legend.2
        );
    }
    // Netto (after the discount of Alpha: 1000 − 20 %) and brutto.
    find(&texts, "netto n. Nachlass");
    find(&texts, "brutto");
    find(&texts, "800,00 €");
    find(&texts, "952,00 €");
    // One entry per row in the legend, no running text across the page.
    let all: String = texts
        .iter()
        .filter(|t| t.0 == 0)
        .map(|t| t.1.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    for entry in [
        "niedrigster Preis",
        "höchster Preis",
        "Preis vom Bieter nicht angegeben",
        "GP des Bieters ≠ Menge × EP",
        "Abstand zum günstigsten Preis der Zeile",
        "über alle Bieter mit Preis",
        "Alle Einzelbeträge netto in EUR",
    ] {
        assert!(all.contains(entry), "Legende: {entry}");
    }
    assert!(!all.contains("Kennzeichnung je Zeile"));
    // The positions start on the first page.
    assert_eq!(find(&texts, "01.0010").0, 0);
}
