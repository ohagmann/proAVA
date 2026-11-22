//! Does the last text block of a position stay with its price line? The LV
//! template keeps it together with the prices unless it is taller than
//! 110 mm, measured at 100 mm width (`keep-with-prices` in lv.typ). That
//! measurement is a second layout of every position (2000 positions: about
//! 0.5 GB and 2–3 s more). Most blocks are clearly shorter or clearly taller:
//! this decides them from the font metrics with a wide margin, the template
//! measures only the rest. Whenever something is unclear (table, image,
//! Blocksatz, Mengenermittlung, EP-Anteile, a word wider than the line), the
//! answer is "measure" – a wrong answer would change the printout.
//!
//! Upper bound: first-fit line breaking at spaces only. Typst breaks ragged
//! paragraphs first-fit too, with more break chances (hyphens, slashes), and
//! more break chances never need more lines. Lower bound: the width of the
//! visible characters over the line width.

use std::sync::LazyLock;

use typst::foundations::Bytes;
use typst::text::{Font, FontStyle, FontWeight};

use crate::model::{PrintPosition, TextAlign, TextBlock, TextRun, FONT_SANS};
use crate::view::family;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    /// Certainly at most 110 mm: stays with the prices (unbreakable).
    Short,
    /// Certainly taller than 110 mm: may break.
    Tall,
}

impl Keep {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Keep::Short => "short",
            Keep::Tall => "tall",
        }
    }
}

const PT_MM: f64 = 25.4 / 72.0;
/// Width the template measures at, and the limit (lv.typ).
const MEASURE_MM: f64 = 100.0;
const LIMIT_MM: f64 = 110.0;
/// `text-part`: 3 pt left, the price column (48 mm) and 3 pt right.
const TEXT_MM: f64 = MEASURE_MM - 48.0 - 6.0 * PT_MM;
/// List item (`blocks` in common.typ): 4 mm per level and the mark (3.5 mm,
/// numbered „1.“, „a)“ … 6 mm).
const LIST_LEVEL_MM: f64 = 4.0;
const LIST_MARK_MM: f64 = 3.5;
const LIST_NUMBER_MM: f64 = 6.0;
/// Line pitch at Zeilenabstand „einfach“ (1.15 em, `text-pitch` in
/// common.typ) – wide bounds on both sides; other Zeilenabstände scale them.
const PITCH_MAX_EM: f64 = 1.45;
const PITCH_MIN_EM: f64 = 1.1;
/// Price line (one grid row; up to three lines when „nicht in Summe“ or a
/// unit wraps) and the gaps between text and prices.
const PRICES_EM: f64 = 3.0 * PITCH_MAX_EM + 2.0;
/// Margins on top of the bounds.
const SHORT_UP_TO_MM: f64 = LIMIT_MM * 0.9;
const TALL_FROM_MM: f64 = LIMIT_MM * 1.15;
/// Shaping (kerning, ligatures) makes text a little narrower than the sum
/// of the advances: the lower bound counts only this share.
const SHAPED_SHARE: f64 = 0.85;

/// [`keep_with_prices_at`] at Zeilenabstand „einfach“.
#[cfg(test)]
pub(crate) fn keep_with_prices(position: &PrintPosition, font_pt: f64) -> Option<Keep> {
    keep_with_prices_at(position, font_pt, 1.0)
}

/// The clear cases of `keep-with-prices` for a position (base font size
/// `font_pt`, Zeilenabstand `line` as factor on „einfach“); None = the
/// template measures.
pub(crate) fn keep_with_prices_at(
    position: &PrintPosition,
    font_pt: f64,
    line: f64,
) -> Option<Keep> {
    if !position.takeoff.is_empty() || !position.price_parts.is_empty() {
        return None;
    }
    let (runs, width_mm) = match position.long_text.last()? {
        TextBlock::Paragraph { runs, align } if *align != TextAlign::Justify => (runs, TEXT_MM),
        TextBlock::ListItem {
            level,
            marker,
            runs,
            align,
        } if *align != TextAlign::Justify => (
            runs,
            TEXT_MM
                - LIST_LEVEL_MM * f64::from(*level)
                - if marker.is_some() {
                    LIST_NUMBER_MM
                } else {
                    LIST_MARK_MM
                },
        ),
        _ => return None,
    };
    if runs.is_empty() {
        // An empty paragraph is a small gap.
        return Some(Keep::Short);
    }
    let sizes = runs.iter().map(|run| run_size(run, font_pt));
    let largest = sizes.clone().fold(font_pt, f64::max);
    let smallest = sizes.fold(font_pt, f64::min);
    let lines = upper_lines(runs, font_pt, width_mm)?;
    let tallest = lines * PITCH_MAX_EM * line * largest * PT_MM + PRICES_EM * largest * PT_MM;
    if tallest <= SHORT_UP_TO_MM {
        return Some(Keep::Short);
    }
    let lowest = lower_lines(runs, font_pt, width_mm) * PITCH_MIN_EM * line * smallest * PT_MM;
    (lowest >= TALL_FROM_MM).then_some(Keep::Tall)
}

fn run_size(run: &TextRun, font_pt: f64) -> f64 {
    run.size_pt
        .filter(|size| size.is_finite() && *size > 0.0)
        .map_or(font_pt, f64::from)
}

/// One embedded font: family and style as Typst sees them, the metrics
/// from the font file.
struct Face {
    family: String,
    bold: bool,
    italic: bool,
    metrics: ttf_parser::Face<'static>,
}

static FACES: LazyLock<Vec<Face>> = LazyLock::new(|| {
    crate::world::font_files()
        .iter()
        .filter_map(|data| {
            let info = Font::new(Bytes::new(*data), 0)?.info().clone();
            Some(Face {
                family: info.family,
                bold: info.variant.weight >= FontWeight::SEMIBOLD,
                italic: info.variant.style != FontStyle::Normal,
                metrics: ttf_parser::Face::parse(data, 0).ok()?,
            })
        })
        .collect()
});

fn face_of(name: &str, bold: bool, italic: bool) -> Option<&'static Face> {
    FACES
        .iter()
        .find(|face| face.family == name && face.bold == bold && face.italic == italic)
}

fn face(run: &TextRun) -> Option<&'static Face> {
    face_of(family(run.font.as_deref()), run.bold, run.italic)
}

/// Advance of `c` in em; `missing` for characters the font does not have.
fn advance_em(face: Option<&Face>, c: char, missing: f64) -> f64 {
    let Some(face) = face else { return missing };
    let metrics = &face.metrics;
    metrics
        .glyph_index(c)
        .and_then(|glyph| metrics.glyph_hor_advance(glyph))
        .map_or(missing, |advance| {
            f64::from(advance) / f64::from(metrics.units_per_em())
        })
}

/// Characters that take no room on the page (soft hyphen, zero-width space
/// and joiners, byte order mark).
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}'
        )
}

/// Pieces of one hard line: (width of a word in mm, width of the space
/// before it in mm); a word is what stands between two spaces.
type Line = Vec<(f64, f64)>;

/// Splits the runs at hard line breaks and spaces. A Lücke (`gap`) is one
/// box: no break inside.
fn lines(runs: &[TextRun], font_pt: f64) -> Vec<Line> {
    let mut lines = vec![Line::new()];
    let mut word = 0.0;
    let mut space = 0.0;
    let mut open = false;
    for run in runs {
        let font = face(run).or_else(|| face_of(FONT_SANS, run.bold, run.italic));
        let size_mm = run_size(run, font_pt) * PT_MM;
        // Characters the fonts do not have: a full em (upper bound).
        let width = |c: char| advance_em(font, c, 1.0) * size_mm;
        for c in run.text.chars() {
            if c == '\n' {
                lines.last_mut().unwrap().push((word, space));
                lines.push(Line::new());
                (word, space, open) = (0.0, 0.0, false);
            } else if c == ' ' && !run.gap {
                if open {
                    lines.last_mut().unwrap().push((word, space));
                    word = 0.0;
                }
                // Each further space widens the gap before the next word.
                space = if open { width(c) } else { space + width(c) };
                open = true;
            } else {
                word += width(c);
                open = true;
            }
        }
    }
    lines.last_mut().unwrap().push((word, space));
    lines
}

/// At most this many lines (first-fit at spaces); None when a word does not
/// fit on a line.
fn upper_lines(runs: &[TextRun], font_pt: f64, width_mm: f64) -> Option<f64> {
    let mut count = 0.0;
    for line in lines(runs, font_pt) {
        count += 1.0;
        let mut filled = 0.0;
        for (word, space) in line {
            if word > width_mm {
                return None;
            }
            if filled == 0.0 {
                // Leading spaces and a word that do not fit: the word goes on.
                if space + word > width_mm {
                    count += 1.0;
                    filled = word;
                } else {
                    filled = space + word;
                }
            } else if filled + space + word <= width_mm {
                filled += space + word;
            } else {
                count += 1.0;
                filled = word;
            }
        }
    }
    Some(count)
}

/// At least this many lines: per hard line with visible characters their
/// shaped width over the line width.
fn lower_lines(runs: &[TextRun], font_pt: f64, width_mm: f64) -> f64 {
    let mut count = 0.0;
    let mut current = 0.0;
    let mut close = |current: f64| {
        if current > 0.0 {
            count += (current * SHAPED_SHARE / width_mm).ceil().max(1.0);
        }
    };
    for run in runs {
        let font = face(run);
        let size_mm = run_size(run, font_pt) * PT_MM;
        for c in run.text.chars() {
            if c == '\n' {
                close(current);
                current = 0.0;
            } else if !invisible(c) && !c.is_whitespace() {
                // Characters the fonts do not have count nothing (lower bound).
                current += advance_em(font, c, 0.0) * size_mm;
            }
        }
    }
    close(current);
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(blocks: Vec<TextBlock>) -> PrintPosition {
        PrintPosition {
            long_text: blocks,
            ..PrintPosition::default()
        }
    }

    fn paragraph(text: &str) -> TextBlock {
        TextBlock::text(text)
    }

    #[test]
    fn a_few_lines_stay_with_the_prices_without_measuring() {
        let text = "Lieferung und Montage eines Raumtemperaturfühlers mit Anschluss an die Automationsstation.";
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(text)]), 9.0),
            Some(Keep::Short)
        );
        assert_eq!(
            keep_with_prices(&position(vec![TextBlock::bullet(text)]), 9.0),
            Some(Keep::Short)
        );
        // Only the last block counts.
        let long = "Zeile\n".repeat(200);
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(&long), paragraph(text)]), 9.0),
            Some(Keep::Short)
        );
    }

    #[test]
    fn many_lines_may_break_without_measuring() {
        let long = "Fühler mit Anschluss an die Automationsstation\n".repeat(60);
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(&long)]), 9.0),
            Some(Keep::Tall)
        );
        // Empty lines are not counted for the lower bound.
        let empty = "\n".repeat(400);
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(&empty)]), 9.0),
            None
        );
    }

    #[test]
    fn unclear_blocks_are_measured() {
        // Near the limit: about 25 lines at 9 pt.
        let near = "Fühler mit Anschluss\n".repeat(25);
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(&near)]), 9.0),
            None
        );
        // A word wider than the line, Blocksatz, a table, Mengenermittlung, EP-Anteile.
        let word = "Raumtemperaturfühleranschlussleitungsverlegungsarbeiten";
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(word)]), 9.0),
            None
        );
        let justified = TextBlock::Paragraph {
            runs: vec![TextRun::plain("kurz")],
            align: TextAlign::Justify,
        };
        assert_eq!(keep_with_prices(&position(vec![justified]), 9.0), None);
        let table = TextBlock::table(false, vec![vec!["a".into()]]);
        assert_eq!(keep_with_prices(&position(vec![table]), 9.0), None);
        let mut with_parts = position(vec![paragraph("kurz")]);
        with_parts.price_parts = vec![("Lohn".into(), None)];
        assert_eq!(keep_with_prices(&with_parts, 9.0), None);
        assert_eq!(keep_with_prices(&position(Vec::new()), 9.0), None);
    }

    #[test]
    fn larger_type_and_narrow_list_items_take_more_lines() {
        let text = "Fühler mit Anschluss an die Automationsstation. ".repeat(8);
        assert_eq!(
            keep_with_prices(&position(vec![paragraph(&text)]), 8.0),
            Some(Keep::Short)
        );
        let deep = TextBlock::ListItem {
            level: 4,
            marker: Some("iv.".into()),
            runs: vec![TextRun::plain(text.clone())],
            align: TextAlign::Left,
        };
        let shallow = upper_lines(&[TextRun::plain(text.clone())], 9.0, TEXT_MM).unwrap();
        let narrow = upper_lines(&[TextRun::plain(text)], 9.0, TEXT_MM - 19.5).unwrap();
        assert!(narrow > shallow, "{narrow} {shallow}");
        assert_ne!(
            keep_with_prices(&position(vec![deep]), 12.0),
            Some(Keep::Short)
        );
    }

    #[test]
    fn the_fonts_give_real_widths() {
        let run = TextRun::plain("WWWW iiii");
        let lines = lines(std::slice::from_ref(&run), 10.0);
        assert_eq!(lines.len(), 1);
        let (wide, _) = lines[0][0];
        let (narrow, space) = lines[0][1];
        assert!(wide > 3.0 * narrow, "{wide} {narrow}");
        assert!(space > 0.5 && space < 1.5, "{space}");
    }
}
