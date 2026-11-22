use proava_domain::{default_level_label, mask_spec, OzMask, DEFAULT_POSITION_STEP};
use proava_gaeb::{BreakdownKind, BreakdownLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunKind {
    Level { numeric: bool },
    Position,
    Index,
}

#[derive(Debug, Clone, Copy)]
struct Run {
    kind: RunKind,
    width: usize,
}

/// The pieces of an OZ mask as the exchange needs them; read from the
/// (already validated) pattern of the domain's [`OzMask`].
#[derive(Debug, Clone)]
pub(crate) struct MaskLayout {
    runs: Vec<Run>,
}

/// An order number cut into levels, position and index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OzParts {
    pub levels: Vec<String>,
    pub position: String,
    pub index: String,
}

impl OzParts {
    /// Pieces joined by dots as GAEB references them ("01.02.0010.1").
    pub fn dotted(&self) -> String {
        let mut pieces = self.levels.clone();
        pieces.push(self.position.clone());
        if !self.index.is_empty() {
            pieces.push(self.index.clone());
        }
        pieces.join(".")
    }
}

impl MaskLayout {
    pub fn of(mask: &OzMask) -> Self {
        let mut runs = Vec::new();
        for part in mask.pattern().split('.') {
            let mut characters = part.chars().peekable();
            while let Some(first) = characters.next() {
                let mut width = 1;
                while characters.peek() == Some(&first) {
                    characters.next();
                    width += 1;
                }
                let kind = match first {
                    'P' => RunKind::Position,
                    'I' => RunKind::Index,
                    'L' => RunKind::Level { numeric: false },
                    _ => RunKind::Level { numeric: true },
                };
                runs.push(Run { kind, width });
            }
        }
        Self { runs }
    }

    fn levels(&self) -> impl Iterator<Item = &Run> {
        self.runs
            .iter()
            .filter(|run| matches!(run.kind, RunKind::Level { .. }))
    }

    pub fn level_count(&self) -> usize {
        self.levels().count()
    }

    pub fn level_width(&self, level: usize) -> usize {
        self.levels().nth(level).map(|run| run.width).unwrap_or(1)
    }

    fn count(&self, kind: RunKind) -> usize {
        self.runs.iter().filter(|run| run.kind == kind).count()
    }

    fn width(&self, kind: RunKind) -> usize {
        self.runs
            .iter()
            .filter(|run| run.kind == kind)
            .map(|run| run.width)
            .sum()
    }

    /// Cuts an order number written like the domain writes it (one piece per
    /// run, joined by dots, index optional). None when the piece count does
    /// not fit the mask.
    pub fn split_position(&self, ordinal: &str) -> Option<OzParts> {
        let pieces = ordinal_pieces(ordinal);
        let levels = self.level_count();
        let positions = self.count(RunKind::Position);
        let indexes = self.count(RunKind::Index);
        if pieces.len() < levels + positions || pieces.len() > levels + positions + indexes {
            return None;
        }
        Some(OzParts {
            levels: pieces[..levels].to_vec(),
            position: pieces[levels..levels + positions].concat(),
            index: pieces[levels + positions..].concat(),
        })
    }

    /// Like [`Self::split_position`] for a position directly below the
    /// heading `path` that is not on the deepest level: the ordinal is the
    /// path, the position and maybe the index.
    pub fn split_position_below(&self, ordinal: &str, path: &[String]) -> Option<OzParts> {
        let pieces = ordinal_pieces(ordinal);
        let positions = self.count(RunKind::Position);
        let indexes = self.count(RunKind::Index);
        let rest = pieces.len().checked_sub(path.len())?;
        if path.len() >= self.level_count()
            || pieces[..path.len()] != *path
            || rest < positions
            || rest > positions + indexes
        {
            return None;
        }
        let parts = OzParts {
            levels: path.to_vec(),
            position: pieces[path.len()..path.len() + positions].concat(),
            index: pieces[path.len() + positions..].concat(),
        };
        // Imported numbers are padded to the mask; "01.00.0010" of an
        // own position is a level piece "00", not a position.
        let numeric = parts.position.bytes().all(|byte| byte.is_ascii_digit());
        let width = self.width(RunKind::Position);
        let fits = if numeric {
            parts.position.len() == width
        } else {
            parts.position.chars().count() <= width
        };
        (fits && parts.index.chars().count() <= self.width(RunKind::Index)).then_some(parts)
    }

    pub fn zeros(&self, level: usize) -> String {
        "0".repeat(self.level_width(level))
    }
}

pub(crate) fn ordinal_pieces(ordinal: &str) -> Vec<String> {
    ordinal
        .split(['.', ' '])
        .filter(|piece| !piece.is_empty())
        .map(str::to_string)
        .collect()
}

/// OZ mask → GAEB breakdown (BoQBkdn): one level per heading piece, then
/// the position and the index.
pub fn breakdown_from_mask(mask: &OzMask) -> Vec<BreakdownLevel> {
    breakdown_with_labels(mask, &[])
}

/// Like [`breakdown_from_mask`] with the LV's level labels (LblBoQBkdn);
/// a first level labelled „Los“ becomes the GAEB lot level.
pub fn breakdown_with_labels(mask: &OzMask, labels: &[String]) -> Vec<BreakdownLevel> {
    let layout = MaskLayout::of(mask);
    let mut breakdown: Vec<BreakdownLevel> = layout
        .levels()
        .enumerate()
        .map(|(index, run)| {
            let label = labels
                .get(index)
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| default_level_label(index));
            let kind = if index == 0 && mask_spec::is_lot_label(&label) {
                BreakdownKind::Lot
            } else {
                BreakdownKind::Level
            };
            (run, kind, label)
        })
        .map(|(run, kind, label)| BreakdownLevel {
            kind,
            label,
            length: run.width as u8,
            numeric: matches!(run.kind, RunKind::Level { numeric: true }),
        })
        .collect();
    breakdown.push(BreakdownLevel {
        kind: BreakdownKind::Item,
        label: String::new(),
        length: layout.width(RunKind::Position) as u8,
        numeric: true,
    });
    let index_width = layout.width(RunKind::Index);
    if index_width > 0 {
        breakdown.push(BreakdownLevel {
            kind: BreakdownKind::Index,
            label: String::new(),
            length: index_width as u8,
            numeric: false,
        });
    }
    breakdown
}

/// Labels of the heading levels of a GAEB breakdown; a lot is called „Los“.
pub fn labels_from_breakdown(breakdown: &[BreakdownLevel]) -> Vec<String> {
    breakdown
        .iter()
        .filter_map(|level| match level.kind {
            BreakdownKind::Lot => Some(mask_spec::LOT_LABEL.to_string()),
            BreakdownKind::Level => Some(level.label.trim().to_string()),
            BreakdownKind::Item | BreakdownKind::Index => None,
        })
        .collect()
}

/// GAEB breakdown → OZ mask, e.g. Titel 2, Untertitel 2, Position 4,
/// Index 1 → "11.22.PPPPI". A lot (Los) becomes the outermost level.
pub fn mask_from_breakdown(breakdown: &[BreakdownLevel]) -> Result<OzMask, String> {
    let mut parts: Vec<String> = Vec::new();
    let mut item_width = None;
    let mut index_width = 0usize;
    for level in breakdown {
        let width = usize::from(level.length.max(1));
        match level.kind {
            BreakdownKind::Lot | BreakdownKind::Level => {
                if parts.len() >= 9 {
                    return Err("Die Gliederung hat mehr als neun Stufen.".to_string());
                }
                let character = if level.numeric {
                    char::from(b'1' + parts.len() as u8)
                } else {
                    'L'
                };
                parts.push(character.to_string().repeat(width));
            }
            BreakdownKind::Item => item_width = Some(width),
            BreakdownKind::Index => index_width += width,
        }
    }
    let item_width = item_width
        .ok_or_else(|| "Die Gliederung der Datei hat keine Positionsstufe.".to_string())?;
    parts.push(format!(
        "{}{}",
        "P".repeat(item_width),
        "I".repeat(index_width)
    ));
    OzMask::parse(&parts.join("."))
}

/// Whether a title piece of an order number is unused (zeros, blanks or
/// underscores only – GAEB „unbelegte Hierarchiestufe“).
pub(crate) fn unused_piece(piece: &str) -> bool {
    piece.chars().all(|c| matches!(c, '0' | '_' | ' '))
}

/// Keys of a position of the LV for matching a file's items (bids,
/// Aufmaß, Rechnungen): (full key with index, key without index). A
/// position under a higher title („01.001.00.00.0010“) also gets the keys
/// without the unused levels before its number – GAEB writes it that way
/// (`to_gaeb`); a file of the older export with an empty intermediate
/// category „00“ still matches the first pair.
pub(crate) fn position_keys(layout: &MaskLayout, ordinal: &str) -> Vec<(String, String)> {
    let Some(parts) = layout.split_position(ordinal) else {
        let key = match_key(&ordinal_pieces(ordinal));
        return vec![(key.clone(), key)];
    };
    let keys = |levels: &[String]| {
        let mut pieces = levels.to_vec();
        pieces.push(parts.position.clone());
        let base = match_key(&pieces);
        pieces.push(parts.index.clone());
        (match_key(&pieces), base)
    };
    let mut out = vec![keys(&parts.levels)];
    let used = parts
        .levels
        .iter()
        .rposition(|piece| !unused_piece(piece))
        .map_or(0, |at| at + 1);
    if used > 0 && used < parts.levels.len() {
        out.push(keys(&parts.levels[..used]));
    }
    out
}

/// Comparison key for order numbers: pieces without leading zeros, upper
/// case, joined by dots ("01.02.0010" and "1.2.10" are the same).
pub(crate) fn match_key<S: AsRef<str>>(pieces: &[S]) -> String {
    pieces
        .iter()
        .map(|piece| {
            let piece = piece.as_ref().trim();
            let trimmed = piece.trim_start_matches('0');
            if trimmed.is_empty() && !piece.is_empty() {
                "0".to_string()
            } else {
                trimmed.to_uppercase()
            }
        })
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

/// Step between position numbers: 1000, 100 or 10 when every number is a
/// multiple of it, otherwise 1.
pub(crate) fn detect_step(numbers: &[u32]) -> u32 {
    let numbers: Vec<u32> = numbers
        .iter()
        .copied()
        .filter(|number| *number > 0)
        .collect();
    if numbers.is_empty() {
        return DEFAULT_POSITION_STEP;
    }
    [1000, 100, 10]
        .into_iter()
        .find(|step| numbers.iter().all(|number| number % step == 0))
        .unwrap_or(1)
}
