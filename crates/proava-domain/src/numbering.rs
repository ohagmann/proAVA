//! Renumbering with a plan: which order number every row gets, keeping the
//! numbers of finished (locked) positions and of the titles above them,
//! optionally only inside one title's block. The plan can be shown as a
//! preview (alt → neu) before it is applied.

use std::cell::Cell;
use std::collections::HashSet;

use crate::lv::{normalize_ordinal, Lv, LvRow, RowId};

/// The answer of an insert that finds no free order number between its
/// neighbours (0078 | 0079 in an LV with its own numbers).
pub const NO_FREE_ORDINAL: &str = "An dieser Stelle ist keine Ordnungszahl frei (das LV ist nicht \
     fortlaufend nummeriert). Erst neu nummerieren oder an anderer Stelle einfügen.";

/// Whether `error` is [`NO_FREE_ORDINAL`] – then [`renumbering_title`] may help.
pub fn is_no_free_ordinal(error: &str) -> bool {
    error == NO_FREE_ORDINAL
}

thread_local! {
    /// Set while [`renumbering_title`] runs.
    static RENUMBER_TITLE: Cell<bool> = const { Cell::new(false) };
}

/// Runs `change` so that every position it inserts takes its place and the
/// positions of its title are numbered again in steps (the title numbers,
/// everything outside the title and finished positions stay) – instead of
/// a free number between the neighbours or [`NO_FREE_ORDINAL`]. For the
/// answer „Einfügen“ to the question „Einfügen und neu nummerieren?“.
pub fn renumbering_title<T>(change: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            RENUMBER_TITLE.with(|flag| flag.set(self.0));
        }
    }
    let _restore = Restore(RENUMBER_TITLE.with(|flag| flag.replace(true)));
    change()
}

/// Whether a [`renumbering_title`] runs right now.
pub fn title_renumbering() -> bool {
    RENUMBER_TITLE.with(Cell::get)
}

/// How to renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenumberMode {
    /// Only positions; titles keep their numbers (481, 482 …).
    pub positions_only: bool,
    /// Finished positions (and the titles above them) keep their numbers.
    pub keep_done: bool,
    /// Only the block of this row (a title with everything below it).
    pub scope: Option<RowId>,
}

impl RenumberMode {
    pub const ALL: RenumberMode = RenumberMode {
        positions_only: false,
        keep_done: true,
        scope: None,
    };
}

/// One order number that changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalChange {
    pub id: RowId,
    pub old: String,
    pub new: String,
}

impl Lv {
    /// Finished positions plus every title above one of them – their numbers
    /// stay when renumbering keeps the locked rows.
    pub fn locked_numbers(&self) -> HashSet<RowId> {
        let mut keep = HashSet::new();
        for (index, row) in self.rows().iter().enumerate() {
            let Some(position) = row.as_position() else {
                continue;
            };
            if !position.is_locked() {
                continue;
            }
            keep.insert(row.id());
            let mut limit = usize::MAX;
            for above in self.rows()[..index].iter().rev() {
                if let LvRow::Group(group) = above {
                    if group.level() < limit {
                        keep.insert(above.id());
                        limit = group.level();
                        if limit == 0 {
                            break;
                        }
                    }
                }
            }
        }
        keep
    }

    /// The order number of every row (empty for hints) after renumbering
    /// with `mode`; rows in `keep` keep theirs, the others count around
    /// them without taking a kept number.
    pub fn numbering_plan(&self, mode: RenumberMode) -> Vec<String> {
        let mut keep: HashSet<RowId> = if mode.keep_done {
            self.locked_numbers()
        } else {
            HashSet::new()
        };
        if let Some(scope) = mode.scope.and_then(|id| self.index_of(id)) {
            let range = self.block_range(scope);
            for (index, row) in self.rows().iter().enumerate() {
                // Rows outside the block and a scoped title itself stay.
                if !range.contains(&index) || (index == scope && row.is_group()) {
                    keep.insert(row.id());
                }
            }
        }
        if mode.positions_only {
            for row in self.rows() {
                if row.is_group() {
                    keep.insert(row.id());
                }
            }
        }
        let mask = self.oz_mask();
        let level_count = mask.level_count();
        let step = self.effective_position_step();
        let width = mask.position_width().max(1);
        let mut taken: HashSet<String> = self
            .rows()
            .iter()
            .filter(|row| row.has_ordinal() && keep.contains(&row.id()))
            .map(|row| normalize_ordinal(row.ordinal()))
            .filter(|key| !key.is_empty())
            .collect();

        let mut counters = vec![0u32; level_count.max(1)];
        // Heading the positions count under: mask levels or a plain prefix.
        let mut levels: Option<Vec<u32>> = Some(Vec::new());
        let mut prefix = String::new();
        let mut position_counter = 0u32;
        // False below a title that got no number: its positions get none either.
        let mut heading_numbered = true;
        let mut plan = Vec::with_capacity(self.rows().len());

        for row in self.rows() {
            match row {
                LvRow::Group(group) => {
                    position_counter = 0;
                    if level_count == 0 {
                        heading_numbered = true;
                        plan.push(group.ordinal().to_string());
                        continue;
                    }
                    let level = group.level().min(level_count - 1);
                    if keep.contains(&row.id()) {
                        let own = group.ordinal().trim().to_string();
                        match mask.read_group(&own) {
                            Some(read) if read.len() == level + 1 => {
                                counters[..=level].copy_from_slice(&read);
                                for deeper in counters.iter_mut().skip(level + 1) {
                                    *deeper = 0;
                                }
                                levels = Some(if mode.positions_only {
                                    read
                                } else {
                                    counters.clone()
                                });
                            }
                            other => levels = other,
                        }
                        heading_numbered = !own.is_empty();
                        prefix = own.clone();
                        plan.push(group.ordinal().to_string());
                        continue;
                    }
                    let fits = |counters: &[u32]| {
                        counters
                            .iter()
                            .enumerate()
                            .all(|(level, value)| *value <= mask.level_max(level))
                    };
                    let counted = loop {
                        counters[level] = counters[level].saturating_add(1);
                        for deeper in counters.iter_mut().skip(level + 1) {
                            *deeper = 0;
                        }
                        // No number left on this level (or above): never one
                        // beyond the mask's digits („10“ with one digit).
                        if !fits(&counters[..=level]) {
                            break None;
                        }
                        let candidate = mask.format_group(&counters[..=level]);
                        if !taken.contains(&normalize_ordinal(&candidate)) {
                            break Some(candidate);
                        }
                    };
                    let ordinal = match counted {
                        Some(ordinal) => {
                            levels = Some(counters.clone());
                            heading_numbered = true;
                            ordinal
                        }
                        // The old number when it still fits, else none (LV
                        // prüfen reports it; apply_mask refuses such a mask).
                        None => {
                            let old = group.ordinal().trim();
                            let read = mask
                                .read_group(old)
                                .filter(|read| read.len() == level + 1 && mask.fits_group(old));
                            let ordinal = match &read {
                                Some(_) => untaken(old, &taken),
                                None => String::new(),
                            };
                            heading_numbered = !ordinal.is_empty();
                            levels = match read {
                                Some(read) if heading_numbered => {
                                    counters[..=level].copy_from_slice(&read);
                                    Some(counters.clone())
                                }
                                _ => None,
                            };
                            ordinal
                        }
                    };
                    if !ordinal.is_empty() {
                        taken.insert(normalize_ordinal(&ordinal));
                    }
                    prefix = ordinal.trim().to_string();
                    plan.push(ordinal);
                }
                LvRow::Position(position) => {
                    let render = |number: u32| match &levels {
                        Some(levels) if levels.len() == level_count => {
                            mask.format_position(levels, number, "")
                        }
                        _ if prefix.is_empty() => mask.format_position(&[], number, ""),
                        _ => format!("{prefix}.{number:0width$}"),
                    };
                    if keep.contains(&row.id()) {
                        if let Some(number) = own_number(position.ordinal(), &prefix) {
                            position_counter = position_counter.max(number);
                        }
                        plan.push(position.ordinal().to_string());
                        continue;
                    }
                    // When no number is left: the old one if it still fits
                    // (on the mask's own path) and nobody has it, else none.
                    let mask_path = prefix.is_empty()
                        || matches!(&levels, Some(levels) if levels.len() == level_count);
                    let fallback = |old: &str| {
                        if !mask_path || mask.fits_position(old) {
                            untaken(old, &taken)
                        } else {
                            String::new()
                        }
                    };
                    if !heading_numbered {
                        let ordinal = fallback(position.ordinal());
                        if !ordinal.is_empty() {
                            taken.insert(normalize_ordinal(&ordinal));
                        }
                        plan.push(ordinal);
                        continue;
                    }
                    let max = 10u32.saturating_pow(width.min(9) as u32).saturating_sub(1);
                    let mut number = (position_counter / step + 1).saturating_mul(step);
                    let ordinal = loop {
                        if number > max {
                            // The steps ran out (e.g. step 1000 with four digits):
                            // the next free number in between, else no number –
                            // never one that is taken.
                            let free = (position_counter.saturating_add(1)..=max)
                                .map(|number| (number, render(number)))
                                .find(|(_, candidate)| {
                                    !taken.contains(&normalize_ordinal(candidate))
                                });
                            break match free {
                                Some((number, candidate)) => {
                                    position_counter = number;
                                    candidate
                                }
                                None => fallback(position.ordinal()),
                            };
                        }
                        let candidate = render(number);
                        if !taken.contains(&normalize_ordinal(&candidate)) {
                            position_counter = number;
                            break candidate;
                        }
                        number += step;
                    };
                    if !ordinal.is_empty() {
                        taken.insert(normalize_ordinal(&ordinal));
                    }
                    plan.push(ordinal);
                }
                LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {
                    plan.push(String::new())
                }
            }
        }
        plan
    }

    /// The changes `mode` would make, for a preview (alt → neu).
    pub fn numbering_preview(&self, mode: RenumberMode) -> Vec<OrdinalChange> {
        self.rows()
            .iter()
            .zip(self.numbering_plan(mode))
            .filter(|(row, new)| row.has_ordinal() && row.ordinal() != new)
            .map(|(row, new)| OrdinalChange {
                id: row.id(),
                old: row.ordinal().to_string(),
                new,
            })
            .collect()
    }

    /// Renumbers with `mode`; returns how many order numbers changed.
    pub fn renumber_with(&mut self, mode: RenumberMode) -> usize {
        let plan = self.numbering_plan(mode);
        let level_count = self.level_count();
        let mut changed = 0;
        for (index, ordinal) in plan.into_iter().enumerate() {
            let Some(row) = self.row_mut(index) else {
                continue;
            };
            match row {
                LvRow::Group(group) => {
                    if level_count > 0 && group.level() >= level_count {
                        group.set_level(level_count - 1);
                    }
                    if group.ordinal() != ordinal {
                        group.set_ordinal(ordinal);
                        changed += 1;
                    }
                }
                LvRow::Position(position) => {
                    if position.ordinal() != ordinal {
                        position.set_ordinal(ordinal);
                        changed += 1;
                    }
                }
                LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {}
            }
        }
        changed
    }

    /// Order numbers of finished positions that `mode` would change if it
    /// did not keep them – the list for the question before renumbering.
    pub fn done_positions_affected(&self, mode: RenumberMode) -> Vec<String> {
        let free = RenumberMode {
            keep_done: false,
            ..mode
        };
        let plan = self.numbering_plan(free);
        self.rows()
            .iter()
            .zip(plan)
            .filter(|(row, new)| {
                row.as_position()
                    .is_some_and(|position| position.is_locked())
                    && row.ordinal() != new
            })
            .map(|(row, _)| row.ordinal().to_string())
            .collect()
    }
}

/// The position number after the heading prefix ("01.02.0030" → 30).
fn own_number(ordinal: &str, prefix: &str) -> Option<u32> {
    let ordinal = ordinal.trim();
    let rest = if prefix.is_empty() {
        ordinal
    } else {
        ordinal.strip_prefix(&format!("{prefix}."))?
    };
    rest.split('.').next()?.parse().ok()
}

/// `ordinal` when nobody else has it yet, otherwise no number (a duplicate
/// would break the export; LV prüfen reports the missing number).
fn untaken(ordinal: &str, taken: &HashSet<String>) -> String {
    let key = normalize_ordinal(ordinal);
    if key.is_empty() || taken.contains(&key) {
        String::new()
    } else {
        ordinal.to_string()
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::RenumberMode;
    use crate::{EditStatus, Lv, LvGroup, OzMask, Position};

    fn position(text: &str) -> Position {
        Position::new("", text, Decimal::ONE, "St", None)
    }

    fn lv() -> Lv {
        let mut lv = Lv::new("GA", OzMask::parse("11.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "A"));
        lv.push_group(LvGroup::new(1, "", "A1"));
        lv.push_position(position("a"));
        lv.push_position(position("b"));
        lv.push_position(position("c"));
        lv.push_group(LvGroup::new(0, "", "B"));
        lv.push_group(LvGroup::new(1, "", "B1"));
        lv.push_position(position("d"));
        lv.renumber();
        lv
    }

    fn ordinals(lv: &Lv) -> Vec<String> {
        lv.rows()
            .iter()
            .map(|row| row.ordinal().to_string())
            .collect()
    }

    fn done(lv: &mut Lv, index: usize) {
        let id = lv.rows()[index].id();
        lv.set_status(&[id], EditStatus::Done);
    }

    #[test]
    fn without_locks_the_plan_is_the_plain_renumbering() {
        let lv = lv();
        assert_eq!(
            ordinals(&lv),
            [
                "01",
                "01.01",
                "01.01.0010",
                "01.01.0020",
                "01.01.0030",
                "02",
                "02.01",
                "02.01.0010"
            ]
        );
        assert!(lv.numbering_preview(RenumberMode::ALL).is_empty());
        assert!(lv.is_auto_numbered());
    }

    #[test]
    fn finished_positions_keep_their_number_on_insert() {
        let mut lv = lv();
        done(&mut lv, 3); // 01.01.0020
        let id = lv
            .insert_numbered(2, crate::LvRow::Position(position("neu")))
            .unwrap();
        let index = lv.index_of(id).unwrap();
        assert_eq!(
            lv.rows()[index].ordinal(),
            "01.01.0005",
            "free number before 0010"
        );
        assert_eq!(lv.rows()[4].ordinal(), "01.01.0020", "finished keeps 0020");
        assert!(lv.duplicate_ordinals().is_empty());
    }

    #[test]
    fn renumbering_counts_around_finished_positions_and_their_titles() {
        let mut lv = lv();
        let title_a = lv.rows()[0].id();
        let title_b = lv.rows()[5].id();
        lv.set_row_number(0, "03").unwrap();
        lv.set_row_number(5, "05").unwrap();
        done(&mut lv, 4); // 03.01.0030
                          // move "a" behind "c": plain renumbering would make c 01.01.0020
        let a = lv.rows()[2].clone();
        lv.remove_row(2);
        lv.insert_row(4, a);
        assert_eq!(
            lv.done_positions_affected(RenumberMode::ALL),
            ["03.01.0030"]
        );
        lv.renumber();
        assert_eq!(lv.rows()[3].ordinal(), "03.01.0030", "c keeps its number");
        assert_eq!(lv.rows()[2].ordinal(), "03.01.0010");
        assert_eq!(
            lv.rows()[4].ordinal(),
            "03.01.0040",
            "a counts on after 0030"
        );
        assert_eq!(
            lv.row_by_id(title_a).unwrap().ordinal(),
            "03",
            "title of a finished position"
        );
        assert_eq!(
            lv.row_by_id(title_b).unwrap().ordinal(),
            "04",
            "free titles count on"
        );
        assert!(lv.duplicate_ordinals().is_empty());
        // without keeping, everything moves
        lv.renumber_with(RenumberMode {
            keep_done: false,
            ..RenumberMode::ALL
        });
        assert_eq!(lv.row_by_id(title_a).unwrap().ordinal(), "01");
    }

    #[test]
    fn a_title_block_alone_with_preview() {
        let mut lv = lv();
        lv.set_row_number(1, "05").unwrap(); // 01.05 with positions 01.05.00x0
        lv.set_row_number(6, "07").unwrap(); // 02.07
        let scope = Some(lv.rows()[1].id());
        let mode = RenumberMode {
            positions_only: false,
            keep_done: true,
            scope,
        };
        let a = lv.rows()[2].clone();
        lv.remove_row(2);
        lv.insert_row(4, a);
        let preview = lv.numbering_preview(mode);
        assert_eq!(preview.len(), 3);
        assert_eq!(preview[0].old, "01.05.0020");
        assert_eq!(preview[0].new, "01.05.0010");
        lv.renumber_with(mode);
        assert_eq!(
            lv.rows()[1].ordinal(),
            "01.05",
            "scoped title keeps its number"
        );
        assert_eq!(lv.rows()[6].ordinal(), "02.07", "outside stays");
    }

    #[test]
    fn positions_only_keeps_titles_and_finished() {
        let mut lv = Lv::new("GA", OzMask::parse("11.222.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "A"));
        lv.push_group(LvGroup::new(1, "01.481", "A1"));
        lv.push_position(Position::new("01.481.0010", "a", Decimal::ONE, "St", None));
        lv.push_position(Position::new("01.481.0050", "b", Decimal::ONE, "St", None));
        lv.push_position(Position::new("01.481.0070", "c", Decimal::ONE, "St", None));
        done(&mut lv, 4);
        let mode = RenumberMode {
            positions_only: true,
            keep_done: true,
            scope: None,
        };
        lv.renumber_with(mode);
        assert_eq!(
            ordinals(&lv)[2..],
            ["01.481.0010", "01.481.0020", "01.481.0070"]
        );
        assert_eq!(lv.rows()[1].ordinal(), "01.481");
    }

    /// Gefunden mit den Eigenschaftstests: Schrittweite 1000 bei „PPP“ ließ
    /// beim Neunummerieren die alten OZ stehen (passten nicht zur Maske).
    #[test]
    fn step_larger_than_the_position_digits_counts_in_the_largest_step() {
        use crate::{LvGroup, LvRow, OzMask, Position};
        let mut lv = Lv::new("LV", OzMask::parse("1.PPP").unwrap());
        lv.set_position_step(1000);
        lv.push_group(LvGroup::new(0, "", "Titel"));
        lv.push_position(Position::new(
            "01.001.01.0001",
            "A",
            rust_decimal::Decimal::ONE,
            "St",
            None,
        ));
        lv.push_position(Position::new(
            "x",
            "B",
            rust_decimal::Decimal::ONE,
            "St",
            None,
        ));
        assert_eq!(lv.effective_position_step(), 100);
        lv.renumber();
        let ordinals: Vec<&str> = lv.rows().iter().map(LvRow::ordinal).collect();
        assert_eq!(ordinals, ["1", "1.100", "1.200"]);
        assert!(lv.is_auto_numbered());
    }

    /// Gefunden mit den Eigenschaftstests (Saat 7): bei einer Stelle je
    /// Titelstufe gab es nach „9“ den Titel „10“, und ein Titel ohne freie
    /// Nummer behielt seine alte, die nicht zur Maske passt („03.001“).
    #[test]
    fn titles_never_get_numbers_beyond_the_level_digits() {
        use crate::{LvGroup, LvRow, OzMask, Position};
        let mut lv = Lv::new("LV", OzMask::parse("1.PPP").unwrap());
        lv.set_position_step(1);
        for _ in 0..10 {
            lv.push_group(LvGroup::new(0, "03.001", "Titel"));
            lv.push_position(Position::new(
                "",
                "P",
                rust_decimal::Decimal::ONE,
                "St",
                None,
            ));
        }
        lv.renumber();
        let ordinals: Vec<&str> = lv.rows().iter().map(LvRow::ordinal).collect();
        assert_eq!(ordinals[..2], ["1", "1.001"]);
        assert_eq!(
            ordinals[16..],
            ["9", "9.001", "", ""],
            "kein „10“, keine alte OZ"
        );
        assert!(lv.duplicate_ordinals().is_empty());
        assert!(OzMask::parse("1.PPP").unwrap().fits_group("9"));
        assert!(!OzMask::parse("1.PPP").unwrap().fits_group("10"));
        assert!(!OzMask::parse("11.PPPP").unwrap().fits_group("1"));
        assert!(OzMask::parse("11.PPPPI")
            .unwrap()
            .fits_position("01.0010.2"));
        assert!(!OzMask::parse("11.PPPP").unwrap().fits_position("01.010"));
    }
}
