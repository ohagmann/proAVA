//! Fixed-column access to one DATAN record and the OZ mask.

use rust_decimal::Decimal;

use crate::model::{BreakdownKind, BreakdownLevel};

/// One record (line) of a DATAN file, addressed by 1-based columns as in the
/// GAEB 90 tables. Short lines read as padded with spaces.
pub(crate) struct Record {
    chars: Vec<char>,
    /// 1-based line number in the file (for messages).
    pub line: usize,
}

impl Record {
    pub fn new(text: &str, line: usize) -> Self {
        Self {
            chars: text.chars().collect(),
            line,
        }
    }

    pub fn kind(&self) -> String {
        self.raw(1, 2)
    }

    /// Columns `from..=to` exactly as written.
    pub fn raw(&self, from: usize, to: usize) -> String {
        let start = from.saturating_sub(1).min(self.chars.len());
        let end = to.min(self.chars.len());
        let mut text: String = self.chars[start..end.max(start)].iter().collect();
        let missing = (to + 1 - from).saturating_sub(text.chars().count());
        text.extend(std::iter::repeat_n(' ', missing));
        text
    }

    /// Columns `from..=to`, trailing blanks removed.
    pub fn text(&self, from: usize, to: usize) -> String {
        self.raw(from, to).trim_end().to_string()
    }

    /// Columns `from..=to`, blanks removed on both sides.
    pub fn trimmed(&self, from: usize, to: usize) -> String {
        self.raw(from, to).trim().to_string()
    }

    pub fn flag(&self, col: usize) -> char {
        self.raw(col, col).chars().next().unwrap_or(' ')
    }

    /// Numeric field with an implied decimal point; `sign_col` holds '-' for
    /// negative values. Blank = `Ok(None)`, anything but digits = `Err`.
    pub fn number(
        &self,
        from: usize,
        to: usize,
        decimals: u32,
        sign_col: Option<usize>,
    ) -> Result<Option<Decimal>, String> {
        let raw = self.raw(from, to);
        let Some(value) = parse_fixed(&raw, decimals)? else {
            return Ok(None);
        };
        let negative = sign_col.is_some_and(|col| self.flag(col) == '-');
        Ok(Some(if negative { -value } else { value }))
    }
}

pub(crate) fn parse_fixed(raw: &str, decimals: u32) -> Result<Option<Decimal>, String> {
    let digits = raw.trim();
    if digits.is_empty() {
        return Ok(None);
    }
    if !digits.chars().all(|c| c.is_ascii_digit() || c == ' ') {
        return Err(format!("„{digits}“ ist keine Zahl"));
    }
    let digits: String = digits.chars().filter(char::is_ascii_digit).collect();
    let mantissa: i64 = digits
        .parse()
        .map_err(|_| format!("„{digits}“ ist zu groß"))?;
    Ok(Some(Decimal::new(mantissa, decimals)))
}

/// One segment of the 9-character OZ mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaskSegment {
    pub kind: BreakdownKind,
    pub start: usize,
    pub len: usize,
}

/// OZMASKE such as `1122PPPPI`: up to four levels, the item number and an
/// optional index, padded with `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OzMask {
    pub segments: Vec<MaskSegment>,
}

impl OzMask {
    pub fn parse(mask: &str) -> Result<Self, String> {
        let chars: Vec<char> = mask.chars().collect();
        let mut segments: Vec<MaskSegment> = Vec::new();
        let mut last: Option<char> = None;
        let mut next_level = '1';
        for (pos, &c) in chars.iter().enumerate() {
            if c == '0' || c == ' ' {
                last = None;
                continue;
            }
            if last == Some(c) {
                segments.last_mut().expect("segment").len += 1;
                continue;
            }
            let kind = match c {
                '1'..='4' if c == next_level => {
                    next_level = (c as u8 + 1) as char;
                    BreakdownKind::Level
                }
                'P' if !segments.iter().any(|s| s.kind == BreakdownKind::Item) => {
                    BreakdownKind::Item
                }
                'I' if segments.last().map(|s| s.kind) == Some(BreakdownKind::Item) => {
                    BreakdownKind::Index
                }
                _ => return Err(format!("OZ-Maske „{mask}“ ist ungültig (Zeichen „{c}“)")),
            };
            segments.push(MaskSegment {
                kind,
                start: pos,
                len: 1,
            });
            last = Some(c);
        }
        if !segments.iter().any(|s| s.kind == BreakdownKind::Item) {
            return Err(format!(
                "OZ-Maske „{mask}“ enthält keine Positionsstellen (P)"
            ));
        }
        Ok(Self { segments })
    }

    pub fn from_breakdown(levels: &[BreakdownLevel]) -> Result<Self, String> {
        let mut segments = Vec::new();
        let mut start = 0;
        for level in levels.iter().filter(|l| l.kind != BreakdownKind::Lot) {
            segments.push(MaskSegment {
                kind: level.kind,
                start,
                len: level.length as usize,
            });
            start += level.length as usize;
        }
        let mask = Self { segments };
        let text = mask.to_mask_string();
        if start > 9 || mask.level_count() > 4 {
            return Err(format!(
                "Die OZ-Gliederung passt nicht in GAEB 90 (höchstens 9 Stellen und 4 Ebenen, hier „{text}“)"
            ));
        }
        Self::parse(&text)
    }

    pub fn to_mask_string(&self) -> String {
        let mut out = vec!['0'; 9];
        let mut level = 0;
        for seg in &self.segments {
            let c = match seg.kind {
                BreakdownKind::Level | BreakdownKind::Lot => {
                    level += 1;
                    char::from(b'0' + level)
                }
                BreakdownKind::Item => 'P',
                BreakdownKind::Index => 'I',
            };
            for slot in out.iter_mut().skip(seg.start).take(seg.len) {
                *slot = c;
            }
        }
        out.into_iter().collect()
    }

    pub fn level_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| s.kind == BreakdownKind::Level)
            .count()
    }

    pub fn breakdown(&self) -> Vec<BreakdownLevel> {
        self.segments
            .iter()
            .map(|seg| BreakdownLevel {
                kind: seg.kind,
                label: String::new(),
                length: seg.len as u8,
                numeric: seg.kind != BreakdownKind::Index,
            })
            .collect()
    }

    /// Splits a 9-character OZ into its normalized level parts, item part and
    /// index. Numeric parts are right-aligned and padded with `0`; blank parts
    /// come back empty.
    pub fn split(&self, oz: &str) -> SplitOz {
        let chars: Vec<char> = oz.chars().chain(std::iter::repeat(' ')).take(9).collect();
        let mut out = SplitOz::default();
        for seg in &self.segments {
            let part: String = chars[seg.start..seg.start + seg.len].iter().collect();
            match seg.kind {
                BreakdownKind::Index => out.index = part.trim().to_uppercase(),
                kind => {
                    let value = normalize_part(&part, seg.len);
                    if kind == BreakdownKind::Item {
                        out.item = value;
                    } else {
                        out.levels.push(value);
                    }
                }
            }
        }
        out
    }

    /// Builds the 9-character OZ field from parts (missing levels blank).
    pub fn join(&self, levels: &[String], item: Option<&str>, index: &str) -> String {
        let mut out = vec![' '; 9];
        let mut level = 0;
        for seg in &self.segments {
            let value = match seg.kind {
                BreakdownKind::Level | BreakdownKind::Lot => {
                    level += 1;
                    levels.get(level - 1).map(String::as_str)
                }
                BreakdownKind::Item => item,
                BreakdownKind::Index => Some(index),
            };
            let Some(value) = value.filter(|v| !v.is_empty()) else {
                continue;
            };
            let value: Vec<char> = if seg.kind == BreakdownKind::Index {
                value.chars().take(seg.len).collect()
            } else {
                let trimmed: Vec<char> = value.trim().chars().collect();
                let skip = trimmed.len().saturating_sub(seg.len);
                let mut padded = vec!['0'; seg.len - (trimmed.len() - skip)];
                padded.extend(&trimmed[skip..]);
                padded
            };
            for (i, c) in value.into_iter().enumerate() {
                out[seg.start + i] = c;
            }
        }
        out.into_iter().collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct SplitOz {
    /// Level parts in order; a blank level gives an empty string.
    pub levels: Vec<String>,
    pub item: String,
    pub index: String,
}

impl SplitOz {
    /// Level parts that are actually set (blank levels skipped).
    pub fn path(&self) -> Vec<String> {
        self.levels
            .iter()
            .filter(|l| !l.is_empty())
            .cloned()
            .collect()
    }

    pub fn dotted(&self) -> String {
        let mut pieces = self.path();
        if !self.item.is_empty() {
            pieces.push(self.item.clone());
        }
        if !self.index.is_empty() {
            pieces.push(self.index.clone());
        }
        pieces.join(".")
    }
}

fn normalize_part(part: &str, len: usize) -> String {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        format!("{trimmed:0>len$}")
    } else {
        trimmed.to_string()
    }
}
