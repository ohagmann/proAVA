//! Comparing two states of an LV (LV-Vergleich, Änderungsliste): which rows
//! are new, gone, moved (other OZ, same content) or changed, and how.
//!
//! Matching, in this order:
//! 1. Titles and positions with the same OZ (pieces compared without
//!    leading zeros, so „1.2.10“ = „01.02.0010“) and the same row kind.
//! 2. Remaining positions by text similarity: the share of common words of
//!    Kurztext + Langtext (Jaccard over the word sets, case ignored); the
//!    best pair first, from [`MOVE_SIMILARITY`] on – then the position
//!    counts as moved (and maybe changed).
//! 3. Remaining titles with the same Kurztext (case ignored) – moved.
//! 4. Ausführungsbeschreibungen by their number; Hinweise in order by text
//!    similarity from [`NOTE_SIMILARITY`] on.
//!
//! Everything left over is new (only in the current LV) or removed (only in
//! the other state).

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::lv::{Lv, LvRow, RowId};
use crate::money::format_decimal_de;

/// Positions with at least this word similarity count as the same position
/// under another OZ.
pub const MOVE_SIMILARITY: f64 = 0.8;
/// Hints with at least this similarity count as the same (changed) hint.
pub const NOTE_SIMILARITY: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Added,
    Removed,
    /// Other OZ; `details` lists what else changed.
    Moved,
    Changed,
}

impl ChangeKind {
    pub fn label(self) -> &'static str {
        match self {
            ChangeKind::Added => "neu",
            ChangeKind::Removed => "entfallen",
            ChangeKind::Moved => "verschoben",
            ChangeKind::Changed => "geändert",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowKind {
    Title,
    Position,
    Note,
    Performance,
    Spacer,
}

impl RowKind {
    pub fn label(self) -> &'static str {
        match self {
            RowKind::Title => "Titel",
            RowKind::Position => "Position",
            RowKind::Note => "Hinweis",
            RowKind::Performance => "Ausführungsbeschreibung",
            RowKind::Spacer => "Abstandshalter",
        }
    }

    fn of(row: &LvRow) -> Self {
        match row {
            LvRow::Group(_) => RowKind::Title,
            LvRow::Position(_) => RowKind::Position,
            LvRow::Note(_) => RowKind::Note,
            LvRow::Performance(_) => RowKind::Performance,
            LvRow::Spacer(_) => RowKind::Spacer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aspect {
    Ordinal,
    ShortText,
    LongText,
    Quantity,
    Unit,
    PositionKind,
    UnitPrice,
    Complements,
    /// The position got or lost its STLB-Bau key („In freie Position
    /// umwandeln“, Strg+Z); a new version shows the STLB-Bau update itself.
    Catalog,
    Level,
    /// The row became another kind (hint without text ↔ Abstandshalter).
    Kind,
}

impl Aspect {
    pub fn label(self) -> &'static str {
        match self {
            Aspect::Ordinal => "OZ",
            Aspect::ShortText => "Kurztext",
            Aspect::LongText => "Langtext",
            Aspect::Quantity => "Menge",
            Aspect::Unit => "Einheit",
            Aspect::PositionKind => "Positionsart",
            Aspect::UnitPrice => "EP",
            Aspect::Complements => "Textergänzungen",
            Aspect::Catalog => "STLB-Bau-Kennung",
            Aspect::Level => "Stufe",
            Aspect::Kind => "Art",
        }
    }
}

/// One difference of a row: old and new value as short text.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub aspect: Aspect,
    pub old: String,
    pub new: String,
}

impl Detail {
    /// „Menge: 652 → 150“
    pub fn line(&self) -> String {
        format!("{}: {} → {}", self.aspect.label(), self.old, self.new)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowChange {
    pub kind: ChangeKind,
    pub row_kind: RowKind,
    pub old_ordinal: String,
    pub new_ordinal: String,
    /// Row in the current LV (for a jump); None when removed.
    pub new_id: Option<RowId>,
    pub old_id: Option<RowId>,
    /// Kurztext (current, else old).
    pub text: String,
    pub details: Vec<Detail>,
}

impl RowChange {
    /// OZ to show: the current one, else the old one.
    pub fn ordinal(&self) -> &str {
        if self.new_ordinal.is_empty() {
            &self.old_ordinal
        } else {
            &self.new_ordinal
        }
    }

    pub fn has(&self, aspect: Aspect) -> bool {
        self.details.iter().any(|detail| detail.aspect == aspect)
    }

    /// One line for lists and the clipboard.
    pub fn line(&self) -> String {
        let ordinal = match self.kind {
            ChangeKind::Moved => format!("{} → {}", self.old_ordinal, self.new_ordinal),
            _ => self.ordinal().to_string(),
        };
        let mut line = format!(
            "{} | {} | {} | {}",
            self.kind.label(),
            self.row_kind.label(),
            ordinal,
            self.text
        );
        let details: Vec<String> = self
            .details
            .iter()
            .filter(|detail| detail.aspect != Aspect::Ordinal)
            .map(Detail::line)
            .collect();
        if !details.is_empty() {
            line.push_str(" | ");
            line.push_str(&details.join("; "));
        }
        line
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LvDiff {
    /// In the order of the current LV; removed rows near their old place.
    pub changes: Vec<RowChange>,
}

impl LvDiff {
    pub fn count(&self, kind: ChangeKind) -> usize {
        self.changes
            .iter()
            .filter(|change| change.kind == kind)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// „3 neu, 1 entfallen, 2 verschoben, 14 geändert“
    pub fn summary(&self) -> String {
        if self.changes.is_empty() {
            return "keine Unterschiede".to_string();
        }
        [
            ChangeKind::Added,
            ChangeKind::Removed,
            ChangeKind::Moved,
            ChangeKind::Changed,
        ]
        .into_iter()
        .map(|kind| (self.count(kind), kind))
        .filter(|(count, _)| *count > 0)
        .map(|(count, kind)| format!("{count} {}", kind.label()))
        .collect::<Vec<_>>()
        .join(", ")
    }

    /// The whole list as text (clipboard, MCP).
    pub fn text(&self) -> String {
        self.changes
            .iter()
            .map(RowChange::line)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Compares the `old` state with the `current` LV.
pub fn compare(old: &Lv, current: &Lv) -> LvDiff {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut used_old: HashSet<usize> = HashSet::new();
    let mut used_new: HashSet<usize> = HashSet::new();

    // 1. same OZ, same kind
    let mut by_key: HashMap<(RowKind, String), usize> = HashMap::new();
    for (index, row) in old.rows().iter().enumerate() {
        if row.has_ordinal() && !row.ordinal().trim().is_empty() {
            by_key
                .entry((RowKind::of(row), ordinal_key(row.ordinal())))
                .or_insert(index);
        }
    }
    for (index, row) in current.rows().iter().enumerate() {
        if !row.has_ordinal() || row.ordinal().trim().is_empty() {
            continue;
        }
        if let Some(&other) = by_key.get(&(RowKind::of(row), ordinal_key(row.ordinal()))) {
            if used_old.insert(other) {
                used_new.insert(index);
                pairs.push((other, index));
            }
        }
    }

    // 2. positions by similarity, best first
    let words_old: HashMap<usize, HashSet<String>> = unmatched(old, &used_old, RowKind::Position)
        .map(|index| (index, words(&old.rows()[index])))
        .collect();
    let words_new: HashMap<usize, HashSet<String>> =
        unmatched(current, &used_new, RowKind::Position)
            .map(|index| (index, words(&current.rows()[index])))
            .collect();
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (&a, old_words) in &words_old {
        for (&b, new_words) in &words_new {
            let score = jaccard(old_words, new_words);
            if score >= MOVE_SIMILARITY {
                candidates.push((score, a, b));
            }
        }
    }
    candidates.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    for (_, a, b) in candidates {
        if !used_old.contains(&a) && !used_new.contains(&b) {
            used_old.insert(a);
            used_new.insert(b);
            pairs.push((a, b));
        }
    }

    // 3. titles with the same text
    for b in unmatched(current, &used_new, RowKind::Title).collect::<Vec<_>>() {
        let text = current.rows()[b].short_text().trim().to_lowercase();
        let found = unmatched(old, &used_old, RowKind::Title)
            .find(|&a| old.rows()[a].short_text().trim().to_lowercase() == text);
        if let Some(a) = found {
            used_old.insert(a);
            used_new.insert(b);
            pairs.push((a, b));
        }
    }

    // 4. Ausführungsbeschreibungen by number, hints by similarity
    for b in unmatched(current, &used_new, RowKind::Performance).collect::<Vec<_>>() {
        let number = current.rows()[b]
            .as_performance()
            .map(|p| p.number().trim().to_string());
        let found = unmatched(old, &used_old, RowKind::Performance).find(|&a| {
            old.rows()[a]
                .as_performance()
                .map(|p| p.number().trim().to_string())
                == number
        });
        if let Some(a) = found {
            used_old.insert(a);
            used_new.insert(b);
            pairs.push((a, b));
        }
    }
    for b in unmatched(current, &used_new, RowKind::Note).collect::<Vec<_>>() {
        let new_words = words(&current.rows()[b]);
        let best = unmatched(old, &used_old, RowKind::Note)
            .map(|a| (jaccard(&words(&old.rows()[a]), &new_words), a))
            .filter(|(score, _)| *score >= NOTE_SIMILARITY)
            .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.cmp(&x.1)));
        if let Some((_, a)) = best {
            used_old.insert(a);
            used_new.insert(b);
            pairs.push((a, b));
        }
    }

    // Build the list in the order of the current LV.
    let new_of_old: HashMap<usize, usize> = pairs.iter().copied().collect();
    let mut keyed: Vec<(f64, RowChange)> = Vec::new();
    for &(a, b) in &pairs {
        let (o, n) = (&old.rows()[a], &current.rows()[b]);
        let details = details(o, n);
        let moved = o.has_ordinal() && ordinal_key(o.ordinal()) != ordinal_key(n.ordinal());
        let kind = if moved {
            ChangeKind::Moved
        } else if details.is_empty() {
            continue;
        } else {
            ChangeKind::Changed
        };
        keyed.push((b as f64, change(kind, Some(o), Some(n), details)));
    }
    // Abstandshalter only space the rows while editing: no change of content.
    for (b, row) in current.rows().iter().enumerate() {
        if !used_new.contains(&b) && !row.is_spacer() {
            keyed.push((
                b as f64,
                change(ChangeKind::Added, None, Some(row), Vec::new()),
            ));
        }
    }
    for (a, row) in old.rows().iter().enumerate() {
        if used_old.contains(&a) || row.is_spacer() {
            continue;
        }
        // after the current counterpart of the nearest earlier matched row
        let anchor = (0..a)
            .rev()
            .find_map(|earlier| new_of_old.get(&earlier))
            .map(|&b| b as f64 + 0.5)
            .unwrap_or(-0.5)
            + a as f64 * 1e-6;
        keyed.push((
            anchor,
            change(ChangeKind::Removed, Some(row), None, Vec::new()),
        ));
    }
    keyed.sort_by(|x, y| x.0.total_cmp(&y.0));
    LvDiff {
        changes: keyed.into_iter().map(|(_, change)| change).collect(),
    }
}

/// Compares two states of the *same* LV (Rückgängig/Wiederholen): the rows
/// keep their [`RowId`] from state to state, so they pair by id – exact even
/// when OZ shift, and fast for large LVs because unchanged rows share their
/// long texts (`Arc`) and are skipped by one comparison. For LVs from
/// different files (other ids) use [`compare`].
pub fn compare_states(old: &Lv, current: &Lv) -> LvDiff {
    let old_index: HashMap<RowId, usize> = old
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id(), index))
        .collect();
    let mut paired_old = vec![false; old.rows().len()];
    let mut new_of_old: HashMap<usize, usize> = HashMap::new();
    let mut keyed: Vec<(f64, RowChange)> = Vec::new();
    // Rows without OZ (hints, Ausführungsbeschreibungen, Abstandshalter) show
    // a move only by their place: both neighbours among the rows of both
    // states differ (an inserted or deleted neighbour changes nothing).
    let current_ids: HashSet<RowId> = current.rows().iter().map(LvRow::id).collect();
    let neighbours = |lv: &Lv,
                      index: usize,
                      present: &dyn Fn(RowId) -> bool|
     -> (Option<RowId>, Option<RowId>) {
        let rows = lv.rows();
        let before = rows[..index]
            .iter()
            .rev()
            .map(LvRow::id)
            .find(|id| present(*id));
        let after = rows[index + 1..]
            .iter()
            .map(LvRow::id)
            .find(|id| present(*id));
        (before, after)
    };
    for (b, n) in current.rows().iter().enumerate() {
        let Some(a) = old_index.get(&n.id()).copied() else {
            keyed.push((
                b as f64,
                change(ChangeKind::Added, None, Some(n), Vec::new()),
            ));
            continue;
        };
        paired_old[a] = true;
        new_of_old.insert(a, b);
        let o = &old.rows()[a];
        if o == n {
            if !n.has_ordinal() {
                let in_old = |id: RowId| old_index.contains_key(&id);
                let in_new = |id: RowId| current_ids.contains(&id);
                let (old_before, old_after) = neighbours(old, a, &in_new);
                let (new_before, new_after) = neighbours(current, b, &in_old);
                if old_before != new_before && old_after != new_after {
                    keyed.push((
                        b as f64,
                        change(ChangeKind::Moved, Some(o), Some(n), Vec::new()),
                    ));
                }
            }
            continue;
        }
        // Same row, other kind (Umwandeln in einen Abstandshalter and back).
        if RowKind::of(o) != RowKind::of(n) {
            let detail = Detail {
                aspect: Aspect::Kind,
                old: RowKind::of(o).label().to_string(),
                new: RowKind::of(n).label().to_string(),
            };
            keyed.push((
                b as f64,
                change(ChangeKind::Changed, Some(o), Some(n), vec![detail]),
            ));
            continue;
        }
        let details = details(o, n);
        let moved = o.has_ordinal() && ordinal_key(o.ordinal()) != ordinal_key(n.ordinal());
        let kind = if moved {
            ChangeKind::Moved
        } else if details.is_empty() {
            continue;
        } else {
            ChangeKind::Changed
        };
        keyed.push((b as f64, change(kind, Some(o), Some(n), details)));
    }
    for (a, row) in old.rows().iter().enumerate() {
        if paired_old[a] {
            continue;
        }
        let anchor = (0..a)
            .rev()
            .find_map(|earlier| new_of_old.get(&earlier))
            .map(|&b| b as f64 + 0.5)
            .unwrap_or(-0.5)
            + a as f64 * 1e-6;
        keyed.push((
            anchor,
            change(ChangeKind::Removed, Some(row), None, Vec::new()),
        ));
    }
    keyed.sort_by(|x, y| x.0.total_cmp(&y.0));
    LvDiff {
        changes: keyed.into_iter().map(|(_, change)| change).collect(),
    }
}

fn change(
    kind: ChangeKind,
    old: Option<&LvRow>,
    new: Option<&LvRow>,
    details: Vec<Detail>,
) -> RowChange {
    let row = new.or(old).expect("one side is there");
    RowChange {
        kind,
        row_kind: RowKind::of(row),
        old_ordinal: old.map(|row| row.ordinal().to_string()).unwrap_or_default(),
        new_ordinal: new.map(|row| row.ordinal().to_string()).unwrap_or_default(),
        new_id: new.map(LvRow::id),
        old_id: old.map(LvRow::id),
        text: row.short_text(),
        details,
    }
}

fn unmatched<'a>(
    lv: &'a Lv,
    used: &'a HashSet<usize>,
    kind: RowKind,
) -> impl Iterator<Item = usize> + 'a {
    lv.rows()
        .iter()
        .enumerate()
        .filter(move |(index, row)| !used.contains(index) && RowKind::of(row) == kind)
        .map(|(index, _)| index)
}

/// What differs between two matched rows (also for other comparisons, e.g.
/// a position before and after an STLB-Bau update).
pub fn row_details(old: &LvRow, new: &LvRow) -> Vec<Detail> {
    details(old, new)
}

/// What differs between two matched rows.
fn details(old: &LvRow, new: &LvRow) -> Vec<Detail> {
    let mut details = Vec::new();
    fn push(details: &mut Vec<Detail>, aspect: Aspect, a: String, b: String) {
        if a != b {
            details.push(Detail {
                aspect,
                old: a,
                new: b,
            });
        }
    }
    if old.has_ordinal() && ordinal_key(old.ordinal()) != ordinal_key(new.ordinal()) {
        push(
            &mut details,
            Aspect::Ordinal,
            old.ordinal().to_string(),
            new.ordinal().to_string(),
        );
    }
    match (old, new) {
        (LvRow::Group(a), LvRow::Group(b)) => {
            push(
                &mut details,
                Aspect::ShortText,
                a.short_text().trim().to_string(),
                b.short_text().trim().to_string(),
            );
            push(
                &mut details,
                Aspect::Level,
                (a.level() + 1).to_string(),
                (b.level() + 1).to_string(),
            );
        }
        (LvRow::Position(a), LvRow::Position(b)) => {
            push(
                &mut details,
                Aspect::ShortText,
                a.short_text().trim().to_string(),
                b.short_text().trim().to_string(),
            );
            let (la, lb) = (a.long_text().to_plain(), b.long_text().to_plain());
            if normalize_space(&la) != normalize_space(&lb) {
                let (removed, added) = word_diff(&la, &lb);
                details.push(Detail {
                    aspect: Aspect::LongText,
                    old: removed,
                    new: added,
                });
            }
            push(
                &mut details,
                Aspect::Quantity,
                quantity(a.quantity()),
                quantity(b.quantity()),
            );
            push(
                &mut details,
                Aspect::Unit,
                a.unit().trim().to_string(),
                b.unit().trim().to_string(),
            );
            push(
                &mut details,
                Aspect::PositionKind,
                kind_text(a),
                kind_text(b),
            );
            push(
                &mut details,
                Aspect::UnitPrice,
                a.unit_price()
                    .map(|p| format_decimal_de(p, 2))
                    .unwrap_or_else(|| "–".into()),
                b.unit_price()
                    .map(|p| format_decimal_de(p, 2))
                    .unwrap_or_else(|| "–".into()),
            );
            push(
                &mut details,
                Aspect::Complements,
                complements(a),
                complements(b),
            );
            let key = |position: &crate::Position| {
                if position.stlb_reference().is_some() {
                    "STLB-Bau".to_string()
                } else {
                    "–".to_string()
                }
            };
            push(&mut details, Aspect::Catalog, key(a), key(b));
        }
        (LvRow::Note(a), LvRow::Note(b)) => {
            let (ta, tb) = (a.text().to_plain(), b.text().to_plain());
            if normalize_space(&ta) != normalize_space(&tb) {
                let (removed, added) = word_diff(&ta, &tb);
                details.push(Detail {
                    aspect: Aspect::LongText,
                    old: removed,
                    new: added,
                });
            }
        }
        (LvRow::Performance(a), LvRow::Performance(b)) => {
            push(&mut details, Aspect::ShortText, a.title(), b.title());
            let (ta, tb) = (a.long_text().to_plain(), b.long_text().to_plain());
            if normalize_space(&ta) != normalize_space(&tb) {
                let (removed, added) = word_diff(&ta, &tb);
                details.push(Detail {
                    aspect: Aspect::LongText,
                    old: removed,
                    new: added,
                });
            }
        }
        _ => {}
    }
    details
}

fn quantity(value: Decimal) -> String {
    let value = value.normalize();
    format_decimal_de(value, value.scale().min(3))
}

fn kind_text(position: &crate::Position) -> String {
    let mut text = position.kind().label().to_string();
    for mark in position.marks() {
        if !position.kind().mark().is_empty() && mark == position.kind().mark() {
            continue;
        }
        // The key has its own aspect (and „STLB-Bau*“ is the Langtext).
        if mark.starts_with("STLB-Bau") {
            continue;
        }
        text.push_str(&format!(" + {mark}"));
    }
    text
}

fn complements(position: &crate::Position) -> String {
    let list: Vec<String> = position
        .complements()
        .iter()
        // As printed (the Änderungsliste is a printout): without the
        // editor's „(vom Bieter)“.
        .map(|complement| {
            complement
                .print_text()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    if list.is_empty() {
        "–".to_string()
    } else {
        list.join("; ")
    }
}

/// The words of a text for comparing: list bullets, dashes and other
/// tokens without letters or digits do not count (the same list may be
/// stored with or without markers).
fn text_words(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .collect()
}

fn normalize_space(text: &str) -> String {
    text_words(text).join(" ")
}

/// OZ pieces without leading zeros, upper case: „01.02.0010“ = „1.2.10“.
pub fn ordinal_key(ordinal: &str) -> String {
    ordinal
        .split(['.', ' '])
        .filter(|piece| !piece.is_empty())
        .map(|piece| {
            let trimmed = piece.trim_start_matches('0');
            if trimmed.is_empty() { "0" } else { trimmed }.to_uppercase()
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn words(row: &LvRow) -> HashSet<String> {
    let text = match row {
        LvRow::Position(position) => format!(
            "{} {}",
            position.short_text(),
            position.long_text().to_plain()
        ),
        LvRow::Note(note) => format!("{} {}", note.heading(), note.text().to_plain()),
        other => other.short_text(),
    };
    tokens(&text)
        .into_iter()
        .map(|word| word.to_lowercase())
        .collect()
}

fn tokens(text: &str) -> Vec<&str> {
    text.split(|character: char| {
        !character.is_alphanumeric() && character != ',' && character != '-'
    })
    .map(|word| word.trim_matches(|c: char| c == ',' || c == '-'))
    .filter(|word| !word.is_empty())
    .collect()
}

/// Share of common words (Jaccard); 1.0 for two empty texts.
pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let common = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    common / union
}

/// A short word diff: the removed and the added passages (up to three each,
/// shortened), e.g. („652 m“, „150 m“).
pub fn word_diff(old: &str, new: &str) -> (String, String) {
    let a: Vec<&str> = text_words(old);
    let b: Vec<&str> = text_words(new);
    // Common start and end first: long texts usually differ in a few words.
    let start = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let end = a[start..]
        .iter()
        .rev()
        .zip(b[start..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a, b) = (&a[start..a.len() - end], &b[start..b.len() - end]);
    let mut removed: Vec<Vec<&str>> = Vec::new();
    let mut added: Vec<Vec<&str>> = Vec::new();
    if a.len() * b.len() > 250_000 {
        removed.push(a.to_vec());
        added.push(b.to_vec());
    } else {
        // LCS table
        let mut table = vec![vec![0u32; b.len() + 1]; a.len() + 1];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                table[i][j] = if a[i] == b[j] {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        let (mut run_removed, mut run_added): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
        fn flush<'t>(
            removed: &mut Vec<Vec<&'t str>>,
            added: &mut Vec<Vec<&'t str>>,
            run_removed: &mut Vec<&'t str>,
            run_added: &mut Vec<&'t str>,
        ) {
            if !run_removed.is_empty() {
                removed.push(std::mem::take(run_removed));
            }
            if !run_added.is_empty() {
                added.push(std::mem::take(run_added));
            }
        }
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                flush(&mut removed, &mut added, &mut run_removed, &mut run_added);
                i += 1;
                j += 1;
            } else if j < b.len() && (i == a.len() || table[i][j + 1] >= table[i + 1][j]) {
                run_added.push(b[j]);
                j += 1;
            } else {
                run_removed.push(a[i]);
                i += 1;
            }
        }
        flush(&mut removed, &mut added, &mut run_removed, &mut run_added);
    }
    (passages(&removed), passages(&added))
}

fn passages(list: &[Vec<&str>]) -> String {
    if list.is_empty() {
        return "–".to_string();
    }
    let mut shown: Vec<String> = list
        .iter()
        .take(3)
        .map(|words| {
            let text = words.join(" ");
            if text.chars().count() > 80 {
                format!("„{}…“", text.chars().take(80).collect::<String>())
            } else {
                format!("„{text}“")
            }
        })
        .collect();
    if list.len() > 3 {
        shown.push(format!("(+{} weitere Stellen)", list.len() - 3));
    }
    shown.join(", ")
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::{LvGroup, Note, OzMask, PerformanceDescription, Position, PositionKind, RichText};

    fn position(oz: &str, short: &str, long: &str, quantity: i64) -> Position {
        let mut position = Position::new(oz, short, Decimal::from(quantity), "St", None);
        position.set_long_text(RichText::plain(long));
        position
    }

    fn old() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "01.482", "Schaltschränke"));
        lv.push_group(LvGroup::new(2, "01.482.10", "ISP"));
        lv.push_position(position(
            "01.482.10.0080",
            "Schrank ISP01",
            "Stahlblech, 2000 x 800 x 400 mm, IP54",
            1,
        ));
        lv.push_position(position(
            "01.482.10.0090",
            "Schrank ASP09",
            "Stahlblech, 1800 x 600 x 400 mm",
            1,
        ));
        lv.push_note(Note::new("Alle Schränke inklusive Sockel."));
        lv.push_group(LvGroup::new(1, "01.484", "Kabel"));
        lv.push_group(LvGroup::new(2, "01.484.10", "Leitungen"));
        lv.push_position(position(
            "01.484.10.0080",
            "J-Y(ST)Y 2x2x0,8",
            "Installationskabel symmetrisch, in vorhandene Trasse verlegen",
            652,
        ));
        lv.push_position(position(
            "01.484.10.0090",
            "NYM-J 3x1,5",
            "Mantelleitung, auf Putz verlegen, mit Schellen befestigen",
            120,
        ));
        lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "1",
            "Kabel",
            RichText::plain("Trassen vorhanden"),
        )));
        lv
    }

    fn current() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "01.482", "Schaltschränke und Verteiler"));
        lv.push_group(LvGroup::new(2, "01.482.10", "ISP"));
        lv.push_position(position(
            "01.482.10.0080",
            "Schrank ISP01",
            "Stahlblech, 2000 x 800 x 500 mm, IP54",
            1,
        ));
        lv.push_note(Note::new("Alle Schränke inklusive Sockel und Beleuchtung."));
        lv.push_group(LvGroup::new(1, "01.484", "Kabel"));
        lv.push_group(LvGroup::new(2, "01.484.10", "Leitungen"));
        let mut cable = position(
            "1.484.10.80",
            "J-Y(ST)Y 2x2x0,8",
            "Installationskabel symmetrisch, in vorhandene Trasse verlegen",
            150,
        );
        cable.set_unit("m");
        lv.push_position(cable);
        lv.push_position(position(
            "01.484.10.0100",
            "NYM-J 3x1,5",
            "Mantelleitung, auf Putz verlegen, mit Schellen befestigen",
            120,
        ));
        let mut extra = position("01.484.10.0110", "Patchkabel", "Kat. 6A, 2 m", 10);
        extra.set_kind(PositionKind::Provisional { with_total: false });
        lv.push_position(extra);
        lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "1",
            "Kabel",
            RichText::plain("Trassen vorhanden, neue Trassen gesondert"),
        )));
        lv
    }

    fn find<'a>(diff: &'a LvDiff, ordinal: &str) -> &'a RowChange {
        diff.changes
            .iter()
            .find(|change| change.ordinal() == ordinal)
            .unwrap_or_else(|| panic!("{ordinal} not in {}", diff.text()))
    }

    #[test]
    fn finds_new_removed_moved_and_changed_rows() {
        let diff = compare(&old(), &current());
        let removed = find(&diff, "01.482.10.0090");
        assert_eq!(removed.kind, ChangeKind::Removed);
        assert_eq!(removed.text, "Schrank ASP09");

        let cable = find(&diff, "1.484.10.80");
        assert_eq!(
            cable.kind,
            ChangeKind::Changed,
            "same OZ without leading zeros"
        );
        let quantity = cable
            .details
            .iter()
            .find(|d| d.aspect == Aspect::Quantity)
            .unwrap();
        assert_eq!(quantity.line(), "Menge: 652 → 150");
        assert!(cable.has(Aspect::Unit));
        assert!(!cable.has(Aspect::Ordinal));

        let moved = find(&diff, "01.484.10.0100");
        assert_eq!(moved.kind, ChangeKind::Moved);
        assert_eq!(moved.old_ordinal, "01.484.10.0090");
        assert!(
            moved.details.iter().all(|d| d.aspect == Aspect::Ordinal),
            "{:?}",
            moved.details
        );

        let added = find(&diff, "01.484.10.0110");
        assert_eq!(added.kind, ChangeKind::Added);

        let cabinet = find(&diff, "01.482.10.0080");
        let long = cabinet
            .details
            .iter()
            .find(|d| d.aspect == Aspect::LongText)
            .unwrap();
        assert_eq!((long.old.as_str(), long.new.as_str()), ("„400“", "„500“"));

        let title = find(&diff, "01.482");
        assert_eq!(
            title.details[0].line(),
            "Kurztext: Schaltschränke → Schaltschränke und Verteiler"
        );

        let note = diff
            .changes
            .iter()
            .find(|c| c.row_kind == RowKind::Note)
            .unwrap();
        assert_eq!(note.kind, ChangeKind::Changed);
        assert!(note.details[0].new.contains("und Beleuchtung"));
        let performance = diff
            .changes
            .iter()
            .find(|c| c.row_kind == RowKind::Performance)
            .unwrap();
        assert_eq!(performance.kind, ChangeKind::Changed);

        assert_eq!(
            diff.summary(),
            "1 neu, 1 entfallen, 1 verschoben, 5 geändert"
        );
        // order: removed 0090 comes right after the cabinet 0080
        let at = |ordinal: &str| {
            diff.changes
                .iter()
                .position(|c| c.ordinal() == ordinal)
                .unwrap()
        };
        assert_eq!(at("01.482.10.0090"), at("01.482.10.0080") + 1);
        assert!(diff
            .text()
            .contains("entfallen | Position | 01.482.10.0090 | Schrank ASP09"));
    }

    #[test]
    fn identical_states_have_no_differences_and_kinds_show() {
        assert!(compare(&old(), &old()).is_empty());
        assert_eq!(compare(&old(), &old()).summary(), "keine Unterschiede");
        let mut changed = old();
        let index = changed
            .rows()
            .iter()
            .position(|r| r.ordinal() == "01.484.10.0090")
            .unwrap();
        let position = changed.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_lump_sum(true);
        position.set_unit_price(Some(Decimal::new(1250, 2)));
        let diff = compare(&old(), &changed);
        let change = &diff.changes[0];
        assert_eq!(
            change.details.iter().map(Detail::line).collect::<Vec<_>>(),
            [
                "Positionsart: Normalposition → Normalposition + Psch",
                "EP: – → 12,50"
            ]
        );
    }

    #[test]
    fn similarity_threshold_and_word_diff() {
        let a: HashSet<String> = ["a", "b", "c", "d"].map(String::from).into();
        let b: HashSet<String> = ["a", "b", "c", "e"].map(String::from).into();
        assert!((jaccard(&a, &b) - 0.6).abs() < 1e-9);
        assert_eq!(
            word_diff("a b c d", "a x c d"),
            ("„b“".to_string(), "„x“".to_string())
        );
        assert_eq!(
            word_diff("a b", "a b c"),
            ("–".to_string(), "„c“".to_string())
        );
        assert_eq!(
            normalize_space("- Punkt eins\n- Punkt • zwei"),
            normalize_space("Punkt eins Punkt zwei")
        );
        // a renamed position with other text is not moved but removed + new
        let mut other = old();
        let index = other
            .rows()
            .iter()
            .position(|r| r.ordinal() == "01.482.10.0090")
            .unwrap();
        other.remove_row(index);
        other.push_position(position(
            "01.482.10.0095",
            "Ganz anderer Schrank",
            "Kunststoff",
            1,
        ));
        let diff = compare(&old(), &other);
        assert_eq!(diff.count(ChangeKind::Moved), 0);
        assert_eq!(diff.count(ChangeKind::Added), 1);
        assert_eq!(diff.count(ChangeKind::Removed), 1);
    }

    #[test]
    fn spacers_are_no_change() {
        let mut spaced = old();
        spaced
            .insert_numbered(4, LvRow::Spacer(crate::Spacer::new()))
            .unwrap();
        spaced.push_spacer();
        assert!(
            compare(&old(), &spaced).is_empty(),
            "{:?}",
            compare(&old(), &spaced).text()
        );
        assert!(compare(&spaced, &old()).is_empty());
    }

    #[test]
    fn states_of_the_same_lv_pair_by_row_id() {
        // Rückgängig: Zeilen behalten ihre Kennung, auch wenn die OZ wandern.
        let mut before = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        before.push_group(LvGroup::new(0, "", "Automation"));
        before.push_position(position("", "DDC", "Station mit Echtzeituhr", 1));
        before.push_position(position("", "Modul", "Erweiterung", 2));
        before.push_position(position("", "Netzteil", "24 V", 1));
        before.renumber();
        assert!(
            compare_states(&before, &before.clone()).changes.is_empty(),
            "unverändert"
        );

        // Neue Position vor „Modul“: die folgenden rücken eine Nummer weiter.
        let mut after = before.clone();
        let id = after.insert_row(2, LvRow::Position(position("", "Fühler", "Temperatur", 4)));
        after.renumber();
        let diff = compare_states(&before, &after);
        assert_eq!(diff.count(ChangeKind::Added), 1);
        assert_eq!(
            diff.changes[0].new_id,
            Some(id),
            "in LV-Reihenfolge: zuerst die neue Zeile"
        );
        assert_eq!(
            diff.count(ChangeKind::Moved),
            2,
            "Modul und Netzteil mit neuer OZ: {}",
            diff.text()
        );
        assert_eq!(diff.count(ChangeKind::Changed), 0);

        // Zurück: die Zeile ist entfernt, die anderen haben wieder ihre OZ.
        let back = compare_states(&after, &before);
        assert_eq!(back.count(ChangeKind::Removed), 1);
        assert_eq!(back.count(ChangeKind::Moved), 2);

        // Nur der Kurztext geändert.
        let mut renamed = before.clone();
        if let Some(LvRow::Position(position)) = renamed.row_mut(1) {
            position.set_short_text("DDC-Station");
        }
        let diff = compare_states(&before, &renamed);
        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].kind, ChangeKind::Changed);
        assert_eq!(diff.changes[0].old_id, Some(before.rows()[1].id()));
    }

    #[test]
    fn states_name_spacers_moved_and_converted() {
        let mut before = old();
        let spacer = before.insert_row(4, LvRow::Spacer(crate::Spacer::new()));
        // Unchanged: nothing, also for the spacer.
        assert!(compare_states(&before, &before.clone()).is_empty());
        // Moved one row down: one change, „verschoben“, without OZ.
        let mut moved = before.clone();
        moved.move_block(4, false).unwrap();
        let diff = compare_states(&before, &moved);
        assert_eq!(diff.changes.len(), 1, "{}", diff.text());
        assert_eq!(
            (diff.changes[0].kind, diff.changes[0].row_kind),
            (ChangeKind::Moved, RowKind::Spacer)
        );
        assert_eq!(diff.changes[0].ordinal(), "");
        // A neighbour deleted: the spacer stays where it was.
        let mut fewer = before.clone();
        fewer.remove_row(3);
        let diff = compare_states(&before, &fewer);
        assert_eq!(diff.changes.len(), 1, "{}", diff.text());
        assert_eq!(diff.changes[0].row_kind, RowKind::Position);
        // Hint without text → spacer: one change of the kind.
        let mut note = before.clone();
        let blank = note.insert_row(5, LvRow::Note(Note::new(" ")));
        let mut converted = note.clone();
        converted.blank_notes_to_spacers(&[blank]);
        let diff = compare_states(&note, &converted);
        assert_eq!(diff.changes.len(), 1, "{}", diff.text());
        assert!(diff.changes[0].has(Aspect::Kind));
        assert_eq!(
            diff.changes[0].details[0].line(),
            "Art: Hinweis → Abstandshalter"
        );
        let _ = spacer;
    }
}
