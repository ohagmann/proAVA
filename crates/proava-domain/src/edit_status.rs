//! Bearbeitungsstatus of positions (offen, in Bearbeitung, fertig) and the
//! lock that comes with „fertig“: a finished position must not change until
//! its status is set back. Titles carry no status of their own; they show
//! how many positions below them are finished.

use std::collections::HashSet;

use crate::lv::{Lv, LvRow, RowId};
use crate::position::Position;

/// How far the planner is with a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditStatus {
    /// Not marked (the default).
    #[default]
    Open,
    InProgress,
    /// Finished: the position is locked against changes.
    Done,
}

/// Hint shown wherever a change is refused because of the lock.
pub const LOCKED_HINT: &str =
    "zum Ändern Strg+Umschalt+F (offen) oder Strg+Umschalt+E (in Bearbeitung)";

impl EditStatus {
    pub const ALL: [EditStatus; 3] = [EditStatus::Open, EditStatus::InProgress, EditStatus::Done];

    pub fn label(self) -> &'static str {
        match self {
            EditStatus::Open => "offen",
            EditStatus::InProgress => "in Bearbeitung",
            EditStatus::Done => "fertig",
        }
    }

    /// Value in the project file; `None` for the default (no attribute).
    pub fn key(self) -> Option<&'static str> {
        match self {
            EditStatus::Open => None,
            EditStatus::InProgress => Some("in-progress"),
            EditStatus::Done => Some("done"),
        }
    }

    /// Reads the project file value (and the German words for tools);
    /// unknown values are an error so typos do not silently unlock.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_lowercase().as_str() {
            "" | "open" | "offen" => Ok(EditStatus::Open),
            "in-progress" | "in bearbeitung" | "in_bearbeitung" | "bearbeitung" => {
                Ok(EditStatus::InProgress)
            }
            "done" | "fertig" => Ok(EditStatus::Done),
            other => Err(format!(
                "Status „{other}“ gibt es nicht – offen, in Bearbeitung oder fertig."
            )),
        }
    }

    pub fn is_locked(self) -> bool {
        self == EditStatus::Done
    }
}

/// How many positions have which status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusCounts {
    pub open: usize,
    pub in_progress: usize,
    pub done: usize,
}

impl StatusCounts {
    pub fn total(&self) -> usize {
        self.open + self.in_progress + self.done
    }

    /// Whether any position carries a mark (in Bearbeitung or fertig).
    pub fn any_marked(&self) -> bool {
        self.in_progress + self.done > 0
    }

    fn add(&mut self, status: EditStatus) {
        match status {
            EditStatus::Open => self.open += 1,
            EditStatus::InProgress => self.in_progress += 1,
            EditStatus::Done => self.done += 1,
        }
    }
}

/// Text for one refused change: names up to three locked positions.
pub fn locked_message(ordinals: &[String]) -> String {
    let shown: Vec<&str> = ordinals.iter().take(3).map(String::as_str).collect();
    let more = ordinals.len().saturating_sub(shown.len());
    let list = if more > 0 {
        format!("{} und {more} weitere", shown.join(", "))
    } else {
        shown.join(", ")
    };
    if ordinals.len() == 1 {
        format!("Position {list} ist als fertig markiert – {LOCKED_HINT}.")
    } else {
        format!(
            "{} Positionen sind als fertig markiert ({list}) – {LOCKED_HINT}.",
            ordinals.len()
        )
    }
}

impl Position {
    /// Quantity missing: zero although the bidder does not fill it in, the
    /// position does not entfallen and is no STLB placeholder (that one has
    /// its own finding). Pauschal and Bedarfspositionen ohne GB count too –
    /// their GP is EP × Menge as well.
    pub fn quantity_missing(&self) -> bool {
        // A Zuschlagsposition has no quantity of its own.
        self.quantity().is_zero()
            && !self.is_surcharge()
            && !self.is_quantity_open()
            && !self.is_not_applicable()
            && !self.is_stlb_placeholder()
    }

    /// Unit or short text missing (STLB placeholders are still open anyway).
    pub fn text_missing(&self) -> bool {
        !self.is_stlb_placeholder()
            && ((self.unit().trim().is_empty() && !self.is_surcharge())
                || self.short_text().trim().is_empty())
    }
}

impl Lv {
    /// Ids of the locked positions among `ids`; a title stands for every
    /// position in its block.
    pub fn locked_positions(&self, ids: &[RowId]) -> Vec<RowId> {
        let mut seen = HashSet::new();
        let mut locked = Vec::new();
        for id in ids {
            let Some(index) = self.index_of(*id) else {
                continue;
            };
            for row in &self.rows()[self.block_range(index)] {
                if let LvRow::Position(position) = row {
                    if position.is_locked() && seen.insert(position.id()) {
                        locked.push(position.id());
                    }
                }
            }
        }
        locked
    }

    /// The central lock check: Ok when none of the rows (titles with their
    /// blocks) holds a finished position, otherwise a message naming them.
    pub fn ensure_editable(&self, ids: &[RowId]) -> Result<(), String> {
        let locked = self.locked_positions(ids);
        if locked.is_empty() {
            return Ok(());
        }
        let ordinals: Vec<String> = locked
            .iter()
            .filter_map(|id| self.position_by_id(*id))
            .map(|position| {
                if position.ordinal().trim().is_empty() {
                    format!("„{}“", position.short_text())
                } else {
                    position.ordinal().to_string()
                }
            })
            .collect();
        Err(locked_message(&ordinals))
    }

    /// Same check by row index.
    pub fn ensure_editable_rows(&self, indices: &[usize]) -> Result<(), String> {
        let ids: Vec<RowId> = indices
            .iter()
            .filter_map(|index| self.rows().get(*index).map(LvRow::id))
            .collect();
        self.ensure_editable(&ids)
    }

    /// Sets the status of the given positions; a title sets every position
    /// in its block. Returns how many positions changed.
    pub fn set_status(&mut self, ids: &[RowId], status: EditStatus) -> usize {
        let mut targets = HashSet::new();
        for id in ids {
            if let Some(index) = self.index_of(*id) {
                for row in &self.rows()[self.block_range(index)] {
                    if row.is_position() {
                        targets.insert(row.id());
                    }
                }
            }
        }
        let mut changed = 0;
        for index in 0..self.rows().len() {
            if let Some(LvRow::Position(position)) = self.row_mut(index) {
                if targets.contains(&position.id()) && position.status() != status {
                    position.set_status(status);
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Status counts of the positions in `range` (e.g. a title's block).
    pub fn status_counts_in(&self, range: std::ops::Range<usize>) -> StatusCounts {
        let mut counts = StatusCounts::default();
        for row in &self.rows()[range] {
            if let LvRow::Position(position) = row {
                counts.add(position.status());
            }
        }
        counts
    }

    /// Status counts of the whole LV.
    pub fn status_counts(&self) -> StatusCounts {
        self.status_counts_in(0..self.rows().len())
    }

    /// Positions whose quantity is missing (see [`Position::quantity_missing`]).
    pub fn missing_quantity_count(&self) -> usize {
        self.positions()
            .filter(|position| position.quantity_missing())
            .count()
    }

    /// Next row after `after` (wrapping) holding a position without quantity.
    pub fn next_missing_quantity(&self, after: Option<usize>) -> Option<usize> {
        let count = self.rows().len();
        if count == 0 {
            return None;
        }
        let start = after.map(|index| index + 1).unwrap_or(0);
        (0..count)
            .map(|offset| (start + offset) % count)
            .find(|&index| {
                self.rows()[index]
                    .as_position()
                    .is_some_and(Position::quantity_missing)
            })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{locked_message, EditStatus};
    use crate::{Lv, LvGroup, LvRow, OzMask, Position, PositionKind};

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        lv.push_position(Position::new("", "Modul", Decimal::ZERO, "St", None));
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_position(Position::new("", "Fühler", Decimal::TEN, "", None));
        lv.renumber();
        lv
    }

    #[test]
    fn status_round_trips_through_its_key() {
        for status in EditStatus::ALL {
            let key = status.key().unwrap_or("");
            assert_eq!(EditStatus::parse(key), Ok(status));
            assert_eq!(EditStatus::parse(status.label()), Ok(status));
        }
        assert!(EditStatus::parse("fertich").is_err());
        assert!(EditStatus::Done.is_locked());
        assert!(!EditStatus::InProgress.is_locked());
        assert_eq!(EditStatus::default(), EditStatus::Open);
    }

    #[test]
    fn a_title_sets_and_counts_its_positions() {
        let mut lv = lv();
        let title = lv.rows()[0].id();
        assert_eq!(lv.set_status(&[title], EditStatus::Done), 2);
        assert_eq!(lv.set_status(&[title], EditStatus::Done), 0);
        let counts = lv.status_counts();
        assert_eq!((counts.open, counts.in_progress, counts.done), (1, 0, 2));
        assert_eq!(lv.status_counts_in(lv.block_range(0)).done, 2);
        assert!(lv.status_counts_in(lv.block_range(4)).total() == 1);
    }

    #[test]
    fn locked_positions_refuse_changes_with_a_clear_message() {
        let mut lv = lv();
        let ddc = lv.rows()[2].id();
        let field = lv.rows()[5].id();
        assert!(lv.ensure_editable(&[ddc, field]).is_ok());
        lv.set_status(&[ddc], EditStatus::Done);
        let error = lv.ensure_editable(&[ddc]).unwrap_err();
        assert!(error.contains("01.01.0010"), "{error}");
        assert!(error.contains("in Bearbeitung"), "{error}");
        // Der Titel darüber umfasst die fertige Position.
        assert!(lv.ensure_editable(&[lv.rows()[0].id()]).is_err());
        assert!(lv.ensure_editable_rows(&[4, 5]).is_ok());
        // OZ von Hand: weder die Position noch ihr Titel.
        assert!(lv.set_row_number(2, "0015").is_err());
        assert!(lv.set_row_number(1, "02").is_err());
        assert!(lv.set_row_number(4, "05").is_ok());
        lv.set_status(&[ddc], EditStatus::InProgress);
        assert!(lv.ensure_editable(&[ddc]).is_ok());
        let many = locked_message(&["1".into(), "2".into(), "3".into(), "4".into()]);
        assert!(many.starts_with("4 Positionen"), "{many}");
        assert!(many.contains("1 weitere"), "{many}");
    }

    #[test]
    fn copies_of_a_block_start_open() {
        let mut lv = lv();
        lv.set_status(&[lv.rows()[0].id()], EditStatus::Done);
        let first = lv.duplicate_block(0).unwrap();
        let copied: Vec<_> = lv.rows()[first..]
            .iter()
            .filter_map(LvRow::as_position)
            .take(2)
            .map(Position::status)
            .collect();
        assert_eq!(copied, vec![EditStatus::Open, EditStatus::Open]);
        assert_eq!(
            lv.rows()[2].as_position().unwrap().status(),
            EditStatus::Done
        );
    }

    #[test]
    fn missing_quantities_and_texts() {
        let mut lv = lv();
        assert_eq!(lv.missing_quantity_count(), 1);
        assert_eq!(lv.next_missing_quantity(None), Some(3));
        assert_eq!(lv.next_missing_quantity(Some(3)), Some(3));
        let mut position = Position::new("", "x", Decimal::ZERO, "psch", None);
        position.set_lump_sum(true);
        assert!(
            position.quantity_missing(),
            "Pauschal ohne Menge ergibt GP 0"
        );
        position.set_kind(PositionKind::Provisional { with_total: false });
        assert!(position.quantity_missing());
        position.set_quantity_open(true);
        assert!(!position.quantity_missing());
        position.set_quantity_open(false);
        position.set_not_applicable(true);
        assert!(!position.quantity_missing());
        assert!(!Position::stlb("x").quantity_missing());
        assert!(lv.rows()[5].as_position().unwrap().text_missing());
        assert!(!lv.rows()[2].as_position().unwrap().text_missing());
        if let Some(position) = lv.row_mut(3).and_then(LvRow::as_position_mut) {
            position.set_quantity(Decimal::TWO);
        }
        assert_eq!(lv.next_missing_quantity(None), None);
    }
}
