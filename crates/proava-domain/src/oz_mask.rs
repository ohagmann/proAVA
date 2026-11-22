/// Orders a bill of quantities: "11.22.PPPPI" means
/// two digits for the title, two for the subtitle, four digits for the position
/// and one index digit. Every change of kind starts a new piece, so that mask
/// reads as "01.02.0015.1".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OzMask {
    pattern: String,
    runs: Vec<Segment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    LevelDigits(usize),
    LevelLetters(usize),
    PositionDigits(usize),
    IndexDigits(usize),
}

impl OzMask {
    /// Mask assumed when a file names none (project files without
    /// `oz-mask`, GAEB without breakdown); also the mask of many tests.
    /// New LVs and projects take [`OzMask::NEW_LV_PATTERN`].
    pub const DEFAULT_PATTERN: &'static str = "11.22.PPPPI";
    /// Vorgabe for new LVs and projects (Einstellungen ▸ Neue Projekte, when
    /// the settings file names no mask): Los/Abschnitt with two digits,
    /// Titel with three (room for the Kostengruppe, 01.481), position with
    /// four digits and index.
    pub const NEW_LV_PATTERN: &'static str = "11.222.PPPPI";

    pub fn parse(pattern: &str) -> Result<Self, String> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Err("Die OZ-Maske ist leer.".to_string());
        }

        let mut runs: Vec<Segment> = Vec::new();
        let mut has_position = false;

        for part in trimmed.split('.') {
            if part.is_empty() {
                return Err(format!("Die OZ-Maske hat ein leeres Glied: {trimmed}"));
            }

            let mut characters = part.chars().peekable();
            while let Some(first) = characters.next() {
                let mut width = 1;
                while characters.peek().copied() == Some(first) {
                    characters.next();
                    width += 1;
                }

                let segment = match first {
                    '0'..='9' => Segment::LevelDigits(width),
                    'L' => Segment::LevelLetters(width),
                    'P' => {
                        has_position = true;
                        Segment::PositionDigits(width)
                    }
                    'I' => Segment::IndexDigits(width),
                    _ => {
                        return Err(format!("Unbekanntes Zeichen „{first}“ in der OZ-Maske."));
                    }
                };
                runs.push(segment);
            }
        }

        if !has_position {
            return Err("Der OZ-Maske fehlt das Positionsglied (P).".to_string());
        }

        Ok(Self {
            pattern: trimmed.to_string(),
            runs,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn level_count(&self) -> usize {
        self.runs
            .iter()
            .filter(|run| matches!(run, Segment::LevelDigits(_) | Segment::LevelLetters(_)))
            .count()
    }

    /// Width of one hierarchy level in characters.
    pub fn level_width(&self, level: usize) -> usize {
        let levels: Vec<Segment> = self
            .runs
            .iter()
            .filter(|run| matches!(run, Segment::LevelDigits(_) | Segment::LevelLetters(_)))
            .copied()
            .collect();

        match levels.get(level) {
            Some(Segment::LevelDigits(width)) => *width,
            Some(Segment::LevelLetters(width)) => *width,
            _ => 1,
        }
    }

    /// Whether a heading level is written with letters (L) instead of digits.
    pub fn level_is_letters(&self, level: usize) -> bool {
        self.runs
            .iter()
            .filter(|run| matches!(run, Segment::LevelDigits(_) | Segment::LevelLetters(_)))
            .nth(level)
            .is_some_and(|run| matches!(run, Segment::LevelLetters(_)))
    }

    /// "01.02" – the order number of a heading on level `levels.len() - 1`;
    /// deeper levels, position and index stay away.
    pub fn format_group(&self, levels: &[u32]) -> String {
        let mut level_index = 0;
        let mut pieces: Vec<String> = Vec::new();

        for run in &self.runs {
            if level_index >= levels.len() {
                break;
            }
            match *run {
                Segment::LevelDigits(width) => {
                    pieces.push(format!("{:0width$}", levels[level_index], width = width));
                    level_index += 1;
                }
                Segment::LevelLetters(width) => {
                    pieces.push(letters(levels[level_index], width));
                    level_index += 1;
                }
                Segment::PositionDigits(_) | Segment::IndexDigits(_) => {}
            }
        }

        pieces.join(".")
    }

    /// "01.02.0015.1" – the order number of a position.
    /// Missing levels are written as zeros; an empty `index` leaves the index away.
    pub fn format_position(&self, levels: &[u32], position: u32, index: &str) -> String {
        let mut level_index = 0;
        let mut position_offset = 0;
        let mut index_offset = 0;
        let position_digits = format!("{:0width$}", position, width = self.position_width().max(1));
        let index_digits = index.to_string();
        let mut pieces: Vec<String> = Vec::new();

        for run in &self.runs {
            match *run {
                Segment::LevelDigits(width) => {
                    let value = levels.get(level_index).copied().unwrap_or(0);
                    level_index += 1;
                    pieces.push(format!("{:0width$}", value, width = width));
                }
                Segment::LevelLetters(width) => {
                    let value = levels.get(level_index).copied().unwrap_or(1);
                    level_index += 1;
                    pieces.push(letters(value, width));
                }
                Segment::PositionDigits(width) => {
                    pieces.push(slice(&position_digits, position_offset, width).to_string());
                    position_offset += width;
                }
                Segment::IndexDigits(width) => {
                    let piece = slice(&index_digits, index_offset, width);
                    if !piece.is_empty() {
                        pieces.push(piece.to_string());
                    }
                    index_offset += width;
                }
            }
        }

        pieces.join(".")
    }

    /// A readable example for the dialog: "01.01.0001".
    pub fn example(&self) -> String {
        self.example_with_step(1)
    }

    /// A readable example whose position receives `step`, e.g. 100 → "01.01.0100".
    pub fn example_with_step(&self, step: u32) -> String {
        let levels = vec![1; self.level_count()];
        self.format_position(&levels, step.max(1), "")
    }

    /// Width of the position part in digits.
    pub fn position_width(&self) -> usize {
        self.runs
            .iter()
            .map(|run| match run {
                Segment::PositionDigits(width) => *width,
                _ => 0,
            })
            .sum()
    }

    pub fn index_width(&self) -> usize {
        self.runs
            .iter()
            .map(|run| match run {
                Segment::IndexDigits(width) => *width,
                _ => 0,
            })
            .sum()
    }

    /// Largest value one hierarchy level can hold (99 for two digits, 26 for letters).
    pub fn level_max(&self, level: usize) -> u32 {
        let levels: Vec<Segment> = self
            .runs
            .iter()
            .filter(|run| matches!(run, Segment::LevelDigits(_) | Segment::LevelLetters(_)))
            .copied()
            .collect();
        match levels.get(level) {
            Some(Segment::LevelLetters(_)) => 26,
            Some(Segment::LevelDigits(width)) => max_for_width(*width),
            _ => 0,
        }
    }

    /// Largest position step whose first number still fits the position
    /// digits: 100 for „PPP“ (1000 would leave no number), 1 for „P“.
    pub fn largest_step(&self) -> u32 {
        let width = self.position_width().clamp(1, 9) as u32;
        10u32.pow(width - 1)
    }

    /// Why `step` does not fit this mask (None when it does).
    pub fn step_problem(&self, step: u32) -> Option<String> {
        let largest = self.largest_step();
        (step == 0 || step > largest).then(|| {
            format!(
                "Schrittweite {step} passt nicht zu {} Positionsstelle(n) – höchstens {largest}.",
                self.position_width().max(1)
            )
        })
    }

    /// Largest position number the mask can hold (9999 for PPPP).
    pub fn position_max(&self) -> u32 {
        max_for_width(self.position_width())
    }

    /// Reads a heading number written with this mask: "01.02" → [1, 2].
    pub fn read_group(&self, ordinal: &str) -> Option<Vec<u32>> {
        let mut pieces = ordinal.trim().split('.');
        let mut levels = Vec::new();
        for run in &self.runs {
            let value = match *run {
                Segment::LevelDigits(_) => match pieces.next() {
                    Some(piece) => digits_value(piece)?,
                    None => break,
                },
                Segment::LevelLetters(_) => match pieces.next() {
                    Some(piece) => letters_value(piece)?,
                    None => break,
                },
                Segment::PositionDigits(_) | Segment::IndexDigits(_) => continue,
            };
            levels.push(value);
        }
        (!levels.is_empty() && pieces.next().is_none()).then_some(levels)
    }

    /// Whether `ordinal` is a heading number exactly as this mask writes it:
    /// every level within its digits („10“ does not fit „1“, „1“ not „11“).
    pub fn fits_group(&self, ordinal: &str) -> bool {
        let ordinal = ordinal.trim();
        self.read_group(ordinal).is_some_and(|levels| {
            levels
                .iter()
                .enumerate()
                .all(|(level, value)| *value <= self.level_max(level))
                && self.format_group(&levels) == ordinal
        })
    }

    /// Whether `ordinal` is a position number exactly as this mask writes it
    /// (index optional).
    pub fn fits_position(&self, ordinal: &str) -> bool {
        let ordinal = ordinal.trim();
        self.read_position(ordinal).is_some_and(|(levels, number)| {
            let base = self.format_position(&levels, number, "");
            levels
                .iter()
                .enumerate()
                .all(|(level, value)| *value <= self.level_max(level))
                && number <= self.position_max()
                && (ordinal == base || ordinal.starts_with(&format!("{base}.")))
        })
    }

    /// A Positionsnummer with letters („02.01.03.3A“ in an 11.22.33.PPPP mask,
    /// or „02.01.3A“ right under title 02.01): the ordinal with the titles as
    /// the mask writes them, the number as given in capitals (at most the width
    /// of the position, at least one letter and one digit), an optional index;
    /// and the title levels. None if `oz` is no such number.
    pub fn read_lettered_position(&self, oz: &str) -> Option<(String, Vec<u32>)> {
        let pieces: Vec<&str> = oz.split('.').map(str::trim).collect();
        let fits = |text: &str| text.chars().all(|c| c.is_ascii_alphanumeric());
        let number_fits = |number: &str| {
            !number.is_empty()
                && fits(number)
                && number.chars().any(|c| c.is_ascii_alphabetic())
                && number.chars().any(|c| c.is_ascii_digit())
                && number.chars().count() <= self.position_width()
        };
        // Without index first, then with one.
        for index_pieces in [0, 1] {
            if index_pieces == 1 && self.index_width() == 0 {
                continue;
            }
            let Some(depth) = pieces.len().checked_sub(1 + index_pieces) else {
                continue;
            };
            if depth == 0 || depth > self.level_count() {
                continue;
            }
            let Some(levels) = self.read_group(&pieces[..depth].join(".")) else {
                continue;
            };
            let number = pieces[depth];
            let index = if index_pieces == 1 {
                pieces[depth + 1]
            } else {
                ""
            };
            if levels.len() != depth
                || !number_fits(number)
                || !fits(index)
                || index.chars().count() > self.index_width()
            {
                continue;
            }
            let mut ordinal = self.format_group(&levels);
            ordinal.push('.');
            ordinal.push_str(&number.to_uppercase());
            if !index.is_empty() {
                ordinal.push('.');
                ordinal.push_str(&index.to_uppercase());
            }
            return Some((ordinal, levels));
        }
        None
    }

    /// Reads a position number written with this mask: "01.02.0015.1" →
    /// ([1, 2], 15); the index is optional and ignored. It may be a digit or
    /// a letter ("01.02.0015.A"), as GAEB allows and the import keeps it.
    pub fn read_position(&self, ordinal: &str) -> Option<(Vec<u32>, u32)> {
        let mut pieces = ordinal.trim().split('.').peekable();
        let mut levels = Vec::new();
        let mut position = String::new();
        for run in &self.runs {
            match *run {
                Segment::LevelDigits(_) => levels.push(digits_value(pieces.next()?)?),
                Segment::LevelLetters(_) => levels.push(letters_value(pieces.next()?)?),
                Segment::PositionDigits(_) => {
                    let piece = pieces.next()?;
                    digits_value(piece)?;
                    position.push_str(piece);
                }
                Segment::IndexDigits(_) => {
                    if let Some(piece) = pieces.next() {
                        if piece.is_empty() || !piece.chars().all(|c| c.is_ascii_alphanumeric()) {
                            return None;
                        }
                    }
                }
            }
        }
        if pieces.next().is_some() {
            return None;
        }
        Some((levels, position.parse().ok()?))
    }
}

fn max_for_width(width: usize) -> u32 {
    10u32.saturating_pow(width.min(9) as u32).saturating_sub(1)
}

fn digits_value(piece: &str) -> Option<u32> {
    if piece.is_empty() || !piece.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    piece.parse().ok()
}

fn letters_value(piece: &str) -> Option<u32> {
    let first = piece.chars().next()?;
    (first.is_ascii_uppercase() && piece.chars().all(|character| character == first))
        .then(|| u32::from(first) - u32::from('A') + 1)
}

fn slice(digits: &str, offset: usize, width: usize) -> &str {
    let start = offset.min(digits.len());
    let end = (offset + width).min(digits.len());
    &digits[start..end]
}

fn letters(value: u32, width: usize) -> String {
    let character = char::from(b'A' + ((value.saturating_sub(1)) % 26) as u8);
    std::iter::repeat(character).take(width.max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::OzMask;

    #[test]
    fn lettered_position_numbers_on_any_title_level() {
        let mask = OzMask::parse("11.22.33.PPPP").unwrap();
        assert_eq!(
            mask.read_lettered_position("2.1.3a"),
            Some(("02.01.3A".to_string(), vec![2, 1]))
        );
        assert_eq!(
            mask.read_lettered_position("02.01.03.12B"),
            Some(("02.01.03.12B".to_string(), vec![2, 1, 3]))
        );
        // digits only, too long, no title, too deep: no lettered number
        assert_eq!(mask.read_lettered_position("02.01.03.0012"), None);
        assert_eq!(mask.read_lettered_position("02.01.03.1234A"), None);
        assert_eq!(mask.read_lettered_position("3A"), None);
        assert_eq!(mask.read_lettered_position("01.01.01.01.3A"), None);
    }

    #[test]
    fn default_mask_formats_groups_and_positions() {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        assert_eq!(mask.level_count(), 2);
        assert_eq!(mask.format_group(&[1, 2]), "01.02");
        assert_eq!(mask.format_position(&[1, 2], 15, "1"), "01.02.0015.1");
        assert_eq!(mask.format_position(&[1, 2], 15, ""), "01.02.0015");
        assert_eq!(mask.format_group(&[3]), "03");
        assert_eq!(mask.example(), "01.01.0001");
    }

    #[test]
    fn mixed_position_part_is_split_into_runs() {
        let mask = OzMask::parse("11.22.PPPPI").unwrap();
        assert_eq!(mask.format_position(&[3, 4], 7, "2"), "03.04.0007.2");
    }

    #[test]
    fn three_levels_work_too() {
        let mask = OzMask::parse("11.22.33.PPPPI").unwrap();
        assert_eq!(mask.level_count(), 3);
        assert_eq!(mask.format_group(&[1, 2, 9]), "01.02.09");
        assert_eq!(mask.format_position(&[1, 2, 9], 3, "1"), "01.02.09.0003.1");
    }

    #[test]
    fn reads_numbers_back() {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        assert_eq!(mask.read_group("01.02"), Some(vec![1, 2]));
        assert_eq!(mask.read_group("03"), Some(vec![3]));
        assert_eq!(mask.read_group("01.02.03"), None);
        assert_eq!(mask.read_position("01.02.0015"), Some((vec![1, 2], 15)));
        assert_eq!(mask.read_position("01.02.0015.1"), Some((vec![1, 2], 15)));
        assert_eq!(mask.read_position("1.x.10"), None);
        // GAEB index with a letter (Index not numeric), as the import keeps it
        assert_eq!(mask.read_position("01.02.0015.A"), Some((vec![1, 2], 15)));
        assert!(mask.fits_position("01.02.0015.B"));
        assert_eq!(mask.read_position("01.02.0015.-"), None);
        assert_eq!(mask.read_position("01.02.0015."), None);
        assert_eq!(mask.read_position("01.02.0015.Ä"), None);
        assert_eq!(mask.position_max(), 9999);
        assert_eq!(mask.level_max(0), 99);
        let letters = OzMask::parse("L.PPP").unwrap();
        assert_eq!(letters.read_group("C"), Some(vec![3]));
        assert_eq!(letters.read_position("B.020"), Some((vec![2], 20)));
        assert_eq!(letters.level_max(0), 26);
    }

    #[test]
    fn refuses_masks_without_position_part() {
        assert!(OzMask::parse("22.44").is_err());
        assert!(OzMask::parse("22.XP").is_err());
    }
}
