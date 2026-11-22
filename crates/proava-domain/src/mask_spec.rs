//! Gliederung of an LV as a list of levels (the OZ-Masken-Assistent):
//! every level has a label, a width and digits or letters; then the position
//! and an optional index. Turns into an [`OzMask`] pattern and back and
//! checks the GAEB limits.

use crate::oz_mask::OzMask;

/// GAEB: all parts of an order number together have at most 14 characters.
pub const MAX_OZ_LENGTH: usize = 14;
/// GAEB: at most 5 levels before the position (a Los included).
pub const MAX_LEVELS: usize = 5;
pub const MAX_LEVEL_WIDTH: usize = 4;
pub const MAX_POSITION_WIDTH: usize = 4;
pub const MAX_INDEX_WIDTH: usize = 1;
/// GAEB: a level label (LblBoQBkdn) has at most 40 characters.
pub const MAX_LABEL_LENGTH: usize = 40;
/// The fixed label of a lot level (GAEB: only the outermost level may be one).
pub const LOT_LABEL: &str = "Los";

const DEFAULT_LABELS: [&str; 4] = ["Titel", "Untertitel", "Abschnitt", "Unterabschnitt"];

/// Label of a level when the LV has none of its own.
pub fn default_level_label(level: usize) -> String {
    DEFAULT_LABELS
        .get(level)
        .map(|label| label.to_string())
        .unwrap_or_else(|| format!("Stufe {}", level + 1))
}

/// Whether a label names a lot: exactly "Los" (as proAVA writes it for a
/// GAEB Lot). Other spellings such as „LOS“ stay a normal level
/// (GAEB BoQLevel with that label), so imported LVs keep their breakdown.
pub fn is_lot_label(label: &str) -> bool {
    label.trim() == LOT_LABEL
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelSpec {
    pub label: String,
    pub width: usize,
    pub letters: bool,
}

impl LevelSpec {
    pub fn digits(label: impl Into<String>, width: usize) -> Self {
        Self {
            label: label.into(),
            width,
            letters: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskSpec {
    pub levels: Vec<LevelSpec>,
    pub position_width: usize,
    pub index_width: usize,
}

impl MaskSpec {
    /// The levels of `mask` with the labels of the LV (missing ones get the default).
    pub fn of(mask: &OzMask, labels: &[String]) -> Self {
        let levels = (0..mask.level_count())
            .map(|level| LevelSpec {
                label: labels
                    .get(level)
                    .map(|label| label.trim().to_string())
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| default_level_label(level)),
                width: mask.level_width(level),
                letters: mask.level_is_letters(level),
            })
            .collect();
        Self {
            levels,
            position_width: mask.position_width(),
            index_width: mask.index_width(),
        }
    }

    /// "11.222.33.PPPP" – level n is written with the digit n (letters: L).
    pub fn pattern(&self) -> String {
        let mut parts: Vec<String> = self
            .levels
            .iter()
            .enumerate()
            .map(|(index, level)| {
                let character = if level.letters {
                    'L'
                } else {
                    char::from(b'1' + (index % 9) as u8)
                };
                character.to_string().repeat(level.width.max(1))
            })
            .collect();
        parts.push(format!(
            "{}{}",
            "P".repeat(self.position_width.max(1)),
            "I".repeat(self.index_width)
        ));
        parts.join(".")
    }

    /// Characters of a full order number (without dots).
    pub fn total_length(&self) -> usize {
        self.levels.iter().map(|level| level.width).sum::<usize>()
            + self.position_width
            + self.index_width
    }

    pub fn has_lot(&self) -> bool {
        self.levels
            .first()
            .is_some_and(|level| is_lot_label(&level.label))
    }

    /// Everything that keeps this Gliederung from being used, in German.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.levels.len() > MAX_LEVELS {
            problems.push(format!(
                "Höchstens {MAX_LEVELS} Stufen vor der Position (GAEB), hier {}.",
                self.levels.len()
            ));
        }
        for (index, level) in self.levels.iter().enumerate() {
            let name = if level.label.trim().is_empty() {
                format!("Stufe {}", index + 1)
            } else {
                format!("Stufe {} „{}“", index + 1, level.label.trim())
            };
            if level.label.trim().is_empty() {
                problems.push(format!("{name}: bitte eine Bezeichnung eintragen."));
            }
            let length = level.label.trim().chars().count();
            if length > MAX_LABEL_LENGTH {
                problems.push(format!(
                    "Stufe {}: die Bezeichnung hat {length} Zeichen – GAEB erlaubt höchstens {MAX_LABEL_LENGTH}.",
                    index + 1
                ));
            }
            if level.width == 0 || level.width > MAX_LEVEL_WIDTH {
                problems.push(format!("{name}: 1 bis {MAX_LEVEL_WIDTH} Stellen."));
            }
            if index > 0 && is_lot_label(&level.label) {
                problems.push(format!("{name}: ein Los kann nur die erste Stufe sein."));
            }
        }
        if self.position_width == 0 || self.position_width > MAX_POSITION_WIDTH {
            problems.push(format!("Position: 1 bis {MAX_POSITION_WIDTH} Stellen."));
        }
        if self.index_width > MAX_INDEX_WIDTH {
            problems.push("Index: keine oder genau 1 Stelle.".to_string());
        }
        let total = self.total_length();
        if total > MAX_OZ_LENGTH {
            problems.push(format!(
                "Die OZ hat {total} Stellen – GAEB erlaubt höchstens {MAX_OZ_LENGTH}."
            ));
        }
        problems
    }

    /// The mask, or the first problem.
    pub fn to_mask(&self) -> Result<OzMask, String> {
        if let Some(problem) = self.problems().into_iter().next() {
            return Err(problem);
        }
        OzMask::parse(&self.pattern())
    }

    pub fn labels(&self) -> Vec<String> {
        self.levels
            .iter()
            .map(|level| level.label.trim().to_string())
            .collect()
    }

    /// An example number with `numbers` for the levels (missing ones = 1),
    /// e.g. [1, 481, 10] and step 10 → "01.481.10.0010".
    pub fn example(&self, numbers: &[u32], step: u32) -> String {
        match OzMask::parse(&self.pattern()) {
            Ok(mask) => {
                let levels: Vec<u32> = (0..self.levels.len())
                    .map(|level| numbers.get(level).copied().unwrap_or(1))
                    .collect();
                let index = if self.index_width > 0 { "1" } else { "" };
                mask.format_position(&levels, step.max(1), index)
            }
            Err(problem) => problem,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typical() -> MaskSpec {
        MaskSpec {
            levels: vec![
                LevelSpec::digits("Titel", 2),
                LevelSpec::digits("Kostengruppe", 3),
                LevelSpec::digits("Untertitel", 2),
            ],
            position_width: 4,
            index_width: 0,
        }
    }

    #[test]
    fn builds_the_pattern_and_example() {
        let spec = typical();
        assert_eq!(spec.pattern(), "11.222.33.PPPP");
        assert!(spec.problems().is_empty());
        assert_eq!(spec.example(&[1, 481, 10], 10), "01.481.10.0010");
        assert_eq!(spec.total_length(), 11);
        let mask = spec.to_mask().unwrap();
        assert_eq!(mask.level_count(), 3);
        assert_eq!(MaskSpec::of(&mask, &spec.labels()), spec);
    }

    #[test]
    fn reads_a_mask_with_default_labels_and_letters() {
        let mask = OzMask::parse("1.LL.PPPI").unwrap();
        let spec = MaskSpec::of(&mask, &["Los".to_string()]);
        assert_eq!(spec.levels[0].label, "Los");
        assert_eq!(spec.levels[1].label, "Untertitel");
        assert!(spec.levels[1].letters);
        assert_eq!(spec.levels[1].width, 2);
        assert_eq!(spec.index_width, 1);
        assert!(spec.has_lot());
        assert_eq!(spec.pattern(), "1.LL.PPPI");
    }

    #[test]
    fn checks_the_gaeb_limits() {
        let mut spec = typical();
        spec.levels.push(LevelSpec::digits("Abschnitt", 4));
        assert!(
            spec.problems()[0].contains("15 Stellen"),
            "{:?}",
            spec.problems()
        );
        spec.levels.pop();
        for _ in 0..3 {
            spec.levels.push(LevelSpec::digits("x", 1));
        }
        assert!(spec
            .problems()
            .iter()
            .any(|p| p.contains("Höchstens 5 Stufen")));
        let mut spec = typical();
        spec.levels[1].label = "Los".into();
        assert!(spec.problems()[0].contains("erste Stufe"));
        spec.levels[1].label = " ".into();
        assert!(spec.problems()[0].contains("Bezeichnung"));
        spec.levels[1].label = "x".repeat(41);
        assert!(spec.problems()[0].contains("41 Zeichen"));
        let mut spec = typical();
        spec.index_width = 2;
        spec.position_width = 5;
        assert_eq!(spec.problems().len(), 2);
        assert!(spec.to_mask().is_err());
        spec.levels[0].width = 0;
        assert!(spec.problems()[0].contains("1 bis 4"));
    }

    #[test]
    fn an_lv_without_titles_is_allowed() {
        let spec = MaskSpec {
            levels: Vec::new(),
            position_width: 4,
            index_width: 1,
        };
        assert_eq!(spec.pattern(), "PPPPI");
        assert_eq!(spec.example(&[], 10), "0010.1");
        assert!(spec.to_mask().is_ok());
    }
}
