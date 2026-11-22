//! Rough size of an LV printout before it is laid out: pages and the memory
//! the layout needs at its peak (Typst keeps the whole document and the
//! intermediate results of a layout pass in memory). Good enough to warn
//! before a printout that would not fit into the free memory – not to
//! predict page numbers.
//!
//! Calibrated with `examples/druck_speicher` (proava-exchange), 18.11.2026,
//! against the high-water mark of the process: 2000 positions on about 500
//! pages need 2.1–2.6 GB with Übertrag and Inhaltsverzeichnis (five layout
//! passes), about 1.5 GB without; long texts with many hard line breaks
//! (1500 positions on 1500 pages) about 5.7 GB.

use crate::model::{PrintItem, PrintLv, TextBlock, TextRun};
use crate::options::PrintOptions;

/// Estimated size of a printout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintEstimate {
    /// Rough number of pages.
    pub pages: usize,
    /// Rough peak of the memory the printout needs, in bytes.
    pub peak_bytes: u64,
    /// The peak without „Übertrag“ (None: Übertrag is off, or {titel} in
    /// header or footer needs the repeated layout passes anyway).
    pub peak_without_carry_bytes: Option<u64>,
}

/// Characters of a text line in the long-text column (A4 portrait, 9 pt).
const LINE_CHARS: f64 = 95.0;
/// Text lines on a page (A4 portrait, 9 pt, with the gaps between positions).
const PAGE_LINES: f64 = 50.0;
/// Memory independent of the size (fonts, Typst, the PDF writer).
const BASE_BYTES: f64 = 150.0 * 1_048_576.0;
/// Memory per page at the peak of the process: one layout pass, and with
/// Übertrag or {titel} (the layout then needs up to five passes). Measured
/// 3.6 MB (real LVs, many short positions or empty lines) to 6.2 MB (many
/// titles) per page with Übertrag; 4 MB errs by −25 % to +35 %.
const PAGE_BYTES: f64 = 2.6 * 1_048_576.0;
const PAGE_BYTES_REPEATED: f64 = 4.0 * 1_048_576.0;

fn runs_chars(runs: &[TextRun]) -> usize {
    runs.iter().map(|run| run.text.chars().count()).sum()
}

/// Lines of a paragraph: every hard line break starts a line, empty lines
/// included (long texts from other programs bring many of them).
fn runs_lines(runs: &[TextRun]) -> f64 {
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    text.split('\n')
        .map(|line| text_lines(line.chars().count()))
        .sum()
}

/// Lines a run of text takes: at least one, wrapped at `LINE_CHARS`.
fn text_lines(chars: usize) -> f64 {
    (chars as f64 / LINE_CHARS).ceil().max(1.0)
}

fn blocks_lines(blocks: &[TextBlock]) -> f64 {
    blocks
        .iter()
        .map(|block| match block {
            TextBlock::Paragraph { runs, .. } => runs_lines(runs) + 0.4,
            TextBlock::ListItem { runs, .. } => runs_lines(runs) + 0.2,
            TextBlock::Table { rows, .. } => rows
                .iter()
                .map(|cells| {
                    let widest = cells
                        .iter()
                        .map(|cell| runs_chars(&cell.runs))
                        .max()
                        .unwrap_or(0);
                    // A cell has a quarter of the width.
                    text_lines(widest * cells.len().max(1)) + 0.5
                })
                .sum::<f64>(),
            // About 6 cm high.
            TextBlock::Image(_) => 16.0,
        })
        .sum()
}

fn items_lines(items: &[PrintItem]) -> f64 {
    items
        .iter()
        .map(|item| match item {
            // Title row, sum row and the gap after it.
            PrintItem::Title(title) => 5.0 + items_lines(&title.items),
            PrintItem::Position(position) => {
                2.5 + text_lines(position.short_text.chars().count())
                    + blocks_lines(&position.long_text)
                    + position.takeoff.len() as f64
            }
            PrintItem::Note(blocks)
            | PrintItem::Preliminary(blocks)
            | PrintItem::Performance(_, blocks) => 2.0 + blocks_lines(blocks),
            // Schlussbemerkung: on a page of its own after the totals.
            PrintItem::Closing(blocks) => PAGE_LINES + blocks_lines(blocks),
        })
        .sum()
}

fn titles(items: &[PrintItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            PrintItem::Title(title) => 1 + titles(&title.items),
            _ => 0,
        })
        .sum()
}

/// Pages and peak memory of an LV printout with these options.
pub fn estimate_lv_print(lv: &PrintLv, options: &PrintOptions) -> PrintEstimate {
    let titles = titles(&lv.items) as f64;
    let mut lines = if options.summary_only {
        0.0
    } else {
        items_lines(&lv.items)
    };
    // Zusammenstellung and Inhaltsverzeichnis: a line per title.
    if options.summary || options.summary_only {
        lines += titles * 1.3;
    }
    if options.contents && !options.summary_only {
        lines += titles * 1.3;
    }
    // Larger type: fewer characters per line and fewer lines per page.
    let scale = (f64::from(options.font_size()) / 9.0).powi(2);
    let landscape = if options.landscape { 1.0 / 1.35 } else { 1.0 };
    let pages = (lines * scale * landscape / PAGE_LINES).ceil().max(1.0);
    let peak = |per_page: f64| (BASE_BYTES + pages * per_page) as u64;
    let uses_title = lv.header.frame.uses_title();
    PrintEstimate {
        pages: pages as usize,
        peak_bytes: peak(if options.carry || uses_title {
            PAGE_BYTES_REPEATED
        } else {
            PAGE_BYTES
        }),
        peak_without_carry_bytes: (options.carry && !uses_title).then(|| peak(PAGE_BYTES)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PrintPosition, PrintTitle};

    fn lv(positions: usize) -> PrintLv {
        let position = |index: usize| {
            PrintItem::Position(PrintPosition {
                oz: format!("01.{index:04}"),
                short_text: "Raumtemperaturfühler".into(),
                long_text: vec![TextBlock::text("Fühler für die Raumtemperatur, Messbereich 0 bis 50 °C, mit Montage und Anschluss an die Automationsstation.".repeat(3))],
                ..PrintPosition::default()
            })
        };
        PrintLv {
            items: vec![PrintItem::Title(PrintTitle {
                lump: false,
                oz: "01".into(),
                label: String::new(),
                text: "Feldgeräte".into(),
                total: None,
                items: (0..positions).map(position).collect(),
            })],
            ..PrintLv::default()
        }
    }

    const MB: u64 = 1_048_576;

    #[test]
    fn pages_and_memory_grow_with_the_lv() {
        let options = PrintOptions::default();
        let small = estimate_lv_print(&lv(100), &options);
        let big = estimate_lv_print(&lv(1000), &options);
        // About eight positions with four lines of long text on a page.
        assert!((10..20).contains(&small.pages), "{small:?}");
        assert!((100..170).contains(&big.pages), "{big:?}");
        assert!(big.peak_bytes > small.peak_bytes);
        // Fonts and Typst at least, a few MB per page.
        assert!(small.peak_bytes > 150 * MB);
        assert!(big.peak_bytes < 150 * MB + big.pages as u64 * 6 * MB);
    }

    #[test]
    fn carry_and_title_in_the_header_need_more_and_say_what_carry_costs() {
        let plain = estimate_lv_print(&lv(500), &PrintOptions::default());
        assert_eq!(plain.peak_without_carry_bytes, None);
        let carry = PrintOptions {
            carry: true,
            ..PrintOptions::default()
        };
        let with_carry = estimate_lv_print(&lv(500), &carry);
        assert_eq!(with_carry.pages, plain.pages);
        assert!(
            with_carry.peak_bytes > plain.peak_bytes * 5 / 4,
            "{with_carry:?} {plain:?}"
        );
        assert_eq!(with_carry.peak_without_carry_bytes, Some(plain.peak_bytes));
        // {titel} in the header: the passes stay without Übertrag.
        let mut titled = lv(500);
        titled.header.frame.header[0] = "{projekt}\n{titel}".into();
        let with_title = estimate_lv_print(&titled, &carry);
        assert_eq!(with_title.peak_bytes, with_carry.peak_bytes);
        assert_eq!(with_title.peak_without_carry_bytes, None);
        assert_eq!(
            estimate_lv_print(&titled, &PrintOptions::default()).peak_bytes,
            with_carry.peak_bytes
        );
    }

    #[test]
    fn hard_line_breaks_count_as_lines() {
        let broken = |breaks: usize| PrintLv {
            items: (0..100)
                .map(|index| {
                    PrintItem::Position(PrintPosition {
                        oz: format!("01.{index:04}"),
                        long_text: vec![TextBlock::text("Fühler\n".repeat(breaks))],
                        ..PrintPosition::default()
                    })
                })
                .collect(),
            ..PrintLv::default()
        };
        let options = PrintOptions::default();
        let few = estimate_lv_print(&broken(1), &options).pages;
        let many = estimate_lv_print(&broken(60), &options).pages;
        // 100 positions with 61 lines each: over 120 pages of 50 lines.
        assert!(many > 120 && many > 8 * few, "{few} {many}");
    }

    #[test]
    fn the_summary_alone_is_small_and_larger_type_takes_more_pages() {
        let summary = PrintOptions {
            summary_only: true,
            ..PrintOptions::default()
        };
        assert_eq!(estimate_lv_print(&lv(1000), &summary).pages, 1);
        let large = PrintOptions {
            font_size_pt: 11,
            ..PrintOptions::default()
        };
        let normal = estimate_lv_print(&lv(1000), &PrintOptions::default()).pages;
        assert!(estimate_lv_print(&lv(1000), &large).pages > normal * 13 / 10);
    }
}
