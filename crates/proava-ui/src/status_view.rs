//! How the Bearbeitungsstatus and missing values show in the LV table:
//! the status column (text), row marks, the status filter and the footer line.

use eframe::egui::{self, Color32, Rect, Stroke};
use proava_domain::{EditStatus, Lv, LvRow, Position, StatusCounts};

use crate::theme::Palette;

/// Which rows the table shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    Open,
    InProgress,
    Done,
    /// Offen and in Bearbeitung.
    NotDone,
    /// Positions without quantity.
    MissingQuantity,
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 6] = [
        StatusFilter::All,
        StatusFilter::NotDone,
        StatusFilter::Open,
        StatusFilter::InProgress,
        StatusFilter::Done,
        StatusFilter::MissingQuantity,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "Filter: alle Zeilen",
            StatusFilter::Open => "Filter: nur offene Positionen",
            StatusFilter::InProgress => "Filter: nur Positionen in Bearbeitung",
            StatusFilter::Done => "Filter: nur fertige Positionen",
            StatusFilter::NotDone => "Filter: alles außer fertigen",
            StatusFilter::MissingQuantity => "Filter: nur Positionen ohne Menge",
        }
    }

    pub fn keeps(self, position: &Position) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Open => position.status() == EditStatus::Open,
            StatusFilter::InProgress => position.status() == EditStatus::InProgress,
            StatusFilter::Done => position.status() == EditStatus::Done,
            StatusFilter::NotDone => position.status() != EditStatus::Done,
            StatusFilter::MissingQuantity => position.quantity_missing(),
        }
    }
}

/// The visible rows narrowed by the filter: matching positions and the
/// titles above them; hints and Ausführungsbeschreibungen are hidden while
/// a filter is on.
pub fn filter_rows(lv: &Lv, visible: Vec<usize>, filter: StatusFilter) -> Vec<usize> {
    if filter == StatusFilter::All {
        return visible;
    }
    let rows = lv.rows();
    let keep_position: Vec<bool> = rows
        .iter()
        .map(|row| {
            row.as_position()
                .is_some_and(|position| filter.keeps(position))
        })
        .collect();
    visible
        .into_iter()
        .filter(|&index| match &rows[index] {
            LvRow::Position(_) => keep_position[index],
            LvRow::Group(_) => lv.block_range(index).any(|row| keep_position[row]),
            _ => false,
        })
        .collect()
}

/// How a row is marked besides selection and hover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowMarks {
    pub status: EditStatus,
    /// Position without quantity (red).
    pub missing_quantity: bool,
    /// Position without unit or short text, hint without text (yellow, can
    /// be switched off).
    pub gap: bool,
    /// Row holds a hit of Suchen/Ersetzen.
    pub hit: bool,
    /// GP of an imported file did not fit Menge × EP (yellow until
    /// confirmed; always shown).
    pub price_check: bool,
    /// Title: status counts of its block.
    pub counts: Option<StatusCounts>,
}

impl RowMarks {
    /// `show_gaps` (setting „Fehlende Angaben markieren“) switches the red
    /// and yellow backgrounds.
    pub fn of(lv: &Lv, index: usize, show_gaps: bool) -> Self {
        match &lv.rows()[index] {
            LvRow::Position(position) => Self {
                status: position.status(),
                missing_quantity: show_gaps && position.quantity_missing(),
                gap: show_gaps && position.text_missing(),
                price_check: position.price_check().is_some(),
                ..Self::default()
            },
            LvRow::Group(_) => Self {
                counts: Some(lv.status_counts_in(lv.block_range(index))),
                ..Self::default()
            },
            // Hint without text: yellow like a position without short text.
            LvRow::Note(note) => Self {
                gap: show_gaps && note.is_blank(),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }

    /// Background of a row that is neither selected nor hovered.
    pub fn fill(&self, palette: &Palette) -> Option<Color32> {
        if self.missing_quantity {
            Some(palette.missing_fill)
        } else if self.gap || self.price_check {
            Some(palette.gap_fill)
        } else {
            None
        }
    }

    /// Text for a title row, e.g. „3/5 fertig“; None while nothing is marked.
    pub fn title_progress(&self) -> Option<(String, bool)> {
        let counts = self.counts?;
        (counts.any_marked() && counts.total() > 0).then(|| {
            (
                format!("{}/{} fertig", counts.done, counts.total()),
                counts.done == counts.total(),
            )
        })
    }
}

/// Paints the marks that sit on top of the row background: a red edge for
/// a selected row without quantity (the selection colour stays) and an
/// outline for search hits.
pub fn paint_overlay(
    painter: &egui::Painter,
    rect: Rect,
    marks: &RowMarks,
    selected: bool,
    palette: &Palette,
) {
    if marks.missing_quantity && selected {
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(rect.right() - 5.0, rect.top()), rect.max),
            egui::CornerRadius::ZERO,
            palette.error,
        );
        painter.rect_stroke(
            rect.shrink(1.0),
            egui::CornerRadius::ZERO,
            Stroke::new(1.5, palette.error),
            egui::StrokeKind::Inside,
        );
    }
    if marks.hit {
        painter.rect_stroke(
            rect.shrink(1.0),
            egui::CornerRadius::same(2),
            Stroke::new(1.5, palette.hit),
            egui::StrokeKind::Inside,
        );
    }
}

/// Text of the column „Status“: „fertig“ (green, the position is
/// write-protected), „in Bearb.“; nothing for offen.
pub fn status_label(status: EditStatus, palette: &Palette) -> Option<(&'static str, Color32)> {
    match status {
        EditStatus::Open => None,
        EditStatus::Done => Some(("fertig", palette.done)),
        EditStatus::InProgress => Some(("in Bearb.", palette.in_progress)),
    }
}

/// Text of the column „Status“ for a position: „GP prüfen“ (GP-Prüfung of
/// the import still open) before the Bearbeitungsstatus.
pub fn position_status_label(
    position: &proava_domain::Position,
    palette: &Palette,
) -> Option<(&'static str, Color32)> {
    if position.price_check().is_some() {
        return Some((proava_domain::PriceCheck::LABEL, palette.warn));
    }
    status_label(position.status(), palette)
}

/// Footer pieces: „412 von 534 Positionen fertig“ and „3 ohne Menge“.
pub fn footer_status(lv: &Lv) -> (Option<String>, Option<String>) {
    let counts = lv.status_counts();
    let progress = counts.any_marked().then(|| {
        let mut text = format!("{} von {} Positionen fertig", counts.done, counts.total());
        if counts.in_progress > 0 {
            text.push_str(&format!(", {} in Bearbeitung", counts.in_progress));
        }
        text
    });
    let missing = lv.missing_quantity_count();
    let missing = (missing > 0).then(|| format!("{missing} ohne Menge (F4)"));
    (progress, missing)
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, EditStatus, Lv, LvGroup, Note, OzMask, Position};

    use super::*;

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "A"));
        lv.push_group(LvGroup::new(1, "", "A1"));
        lv.push_note(Note::new("Hinweis"));
        let mut done = Position::new("", "fertig", Decimal::ONE, "St", None);
        done.set_status(EditStatus::Done);
        lv.push_position(done);
        lv.push_position(Position::new("", "leer", Decimal::ZERO, "", None));
        lv.push_group(LvGroup::new(0, "", "B"));
        lv.push_group(LvGroup::new(1, "", "B1"));
        lv.push_position(Position::new("", "offen", Decimal::ONE, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn filter_keeps_titles_above_matching_positions() {
        let lv = lv();
        let all: Vec<usize> = (0..lv.rows().len()).collect();
        assert_eq!(filter_rows(&lv, all.clone(), StatusFilter::All), all);
        assert_eq!(
            filter_rows(&lv, all.clone(), StatusFilter::Done),
            vec![0, 1, 3]
        );
        assert_eq!(
            filter_rows(&lv, all.clone(), StatusFilter::NotDone),
            vec![0, 1, 4, 5, 6, 7]
        );
        assert_eq!(
            filter_rows(&lv, all.clone(), StatusFilter::MissingQuantity),
            vec![0, 1, 4]
        );
        assert!(filter_rows(&lv, all, StatusFilter::InProgress).is_empty());
    }

    #[test]
    fn marks_and_footer() {
        let lv = lv();
        let marks = RowMarks::of(&lv, 4, true);
        assert!(marks.missing_quantity && marks.gap);
        assert!(!RowMarks::of(&lv, 4, false).gap);
        assert!(!RowMarks::of(&lv, 4, false).missing_quantity);
        let title = RowMarks::of(&lv, 0, true);
        assert_eq!(
            title.title_progress(),
            Some(("1/2 fertig".to_string(), false))
        );
        assert_eq!(RowMarks::of(&lv, 5, true).title_progress(), None);
        let (progress, missing) = footer_status(&lv);
        assert_eq!(progress.as_deref(), Some("1 von 3 Positionen fertig"));
        assert_eq!(missing.as_deref(), Some("1 ohne Menge (F4)"));
    }

    #[test]
    fn hints_without_text_are_marked_like_gaps() {
        let mut lv = lv();
        let blank = lv.push_note(Note::new(" "));
        let index = lv.index_of(blank).unwrap();
        assert!(RowMarks::of(&lv, index, true).gap);
        assert!(
            !RowMarks::of(&lv, index, false).gap,
            "Schalter „Fehlende Angaben markieren“"
        );
        assert!(!RowMarks::of(&lv, 2, true).gap, "Hinweis mit Text");
        lv.push_spacer();
        assert_eq!(
            RowMarks::of(&lv, lv.rows().len() - 1, true),
            RowMarks::default()
        );
    }
}
