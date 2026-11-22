//! Suchen und Ersetzen im LV, ohne UI: which rows match, the next hit and
//! replacing in short texts, long texts (formatting kept), hints,
//! Ausführungsbeschreibungen and units. Finished (locked) positions are
//! skipped and counted.

use std::collections::HashSet;

use proava_domain::{Lv, LvRow, RichText, RowId};

/// Where to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    /// Kurztext of positions and titles.
    pub short: bool,
    /// Langtext of positions.
    pub long: bool,
    /// Hinweise (text and heading).
    pub notes: bool,
    /// Ausführungsbeschreibungen (text and short text).
    pub performances: bool,
    /// Einheit of positions.
    pub unit: bool,
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            short: true,
            long: true,
            notes: true,
            performances: true,
            unit: false,
        }
    }
}

impl Scope {
    pub fn is_empty(&self) -> bool {
        !(self.short || self.long || self.notes || self.performances || self.unit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub scope: Scope,
    /// Only these rows (and the rows inside these titles); None = whole LV.
    pub only: Option<HashSet<RowId>>,
}

/// Result of „Alle ersetzen“.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplaceReport {
    /// Replaced occurrences.
    pub replaced: usize,
    /// Rows that changed.
    pub rows: usize,
    /// Occurrences in finished positions, left as they are.
    pub locked_hits: usize,
    /// Finished positions with occurrences.
    pub locked_rows: usize,
    /// Occurrences in the Langtext or the unit of STLB-Bau positions
    /// (write-protected), left as they are.
    pub stlb_hits: usize,
}

impl ReplaceReport {
    /// One line for the status bar.
    pub fn message(&self, find: &str) -> String {
        let mut text = match (self.replaced, self.rows) {
            (0, _) => format!("„{find}“ nicht ersetzt – keine änderbaren Treffer."),
            (1, _) => "1 Treffer ersetzt.".to_string(),
            (count, 1) => format!("{count} Treffer in 1 Zeile ersetzt."),
            (count, rows) => format!("{count} Treffer in {rows} Zeilen ersetzt."),
        };
        if self.locked_hits > 0 {
            text.push_str(&format!(
                " {} Treffer in {} nicht ersetzt.",
                self.locked_hits,
                crate::format::count(self.locked_rows, "fertigen Position", "fertigen Positionen")
            ));
        }
        if self.stlb_hits > 0 {
            text.push_str(&format!(
                " {} Treffer im Langtext oder in der Einheit von STLB-Bau-Positionen nicht ersetzt (schreibgeschützt).",
                self.stlb_hits
            ));
        }
        text
    }
}

// ---------- plain text ----------

/// Byte ranges of all non-overlapping occurrences of `find` in `text`.
pub fn find_all(
    text: &str,
    find: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<(usize, usize)> {
    if find.is_empty() {
        return Vec::new();
    }
    let needle: Vec<char> = find.chars().collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let same = |a: char, b: char| {
        if case_sensitive {
            a == b
        } else {
            a == b || a.to_lowercase().eq(b.to_lowercase())
        }
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut hits = Vec::new();
    let mut start = 0;
    while start + needle.len() <= chars.len() {
        let matches = needle
            .iter()
            .enumerate()
            .all(|(offset, &wanted)| same(chars[start + offset].1, wanted));
        if matches {
            let end = start + needle.len();
            let bounded = !whole_word
                || ((start == 0 || !is_word(chars[start - 1].1))
                    && (end == chars.len() || !is_word(chars[end].1)));
            if bounded {
                let from = chars[start].0;
                let to = chars.get(end).map_or(text.len(), |(byte, _)| *byte);
                hits.push((from, to));
                start = end;
                continue;
            }
        }
        start += 1;
    }
    hits
}

pub fn count_in(text: &str, find: &str, options: &SearchOptions) -> usize {
    find_all(text, find, options.case_sensitive, options.whole_word).len()
}

/// Replaces all occurrences; returns the new text and the count.
pub fn replace_in_str(
    text: &str,
    find: &str,
    replacement: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> (String, usize) {
    let hits = find_all(text, find, case_sensitive, whole_word);
    if hits.is_empty() {
        return (text.to_string(), 0);
    }
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for (from, to) in &hits {
        result.push_str(&text[last..*from]);
        result.push_str(replacement);
        last = *to;
    }
    result.push_str(&text[last..]);
    (result, hits.len())
}

// ---------- rich text ----------

/// Replaces in a formatted text and keeps the formatting (paragraphs, list
/// items and table cells); a hit across run borders takes the formatting of
/// the run it starts in, images stay untouched. See
/// [`RichText::replace_matches`].
pub fn replace_in_rich_text(
    text: &mut RichText,
    find: &str,
    replacement: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> usize {
    text.replace_matches(find, replacement, case_sensitive, whole_word)
}

/// Occurrences in a formatted text (same rules as replacing).
pub fn count_in_rich_text(
    text: &RichText,
    find: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> usize {
    text.count_matches(find, case_sensitive, whole_word)
}

// ---------- LV ----------

/// Rows the options cover: all, or the chosen rows with their blocks.
fn rows_in_reach(lv: &Lv, options: &SearchOptions) -> Vec<bool> {
    let mut reach = vec![options.only.is_none(); lv.rows().len()];
    if let Some(only) = &options.only {
        for id in only {
            if let Some(index) = lv.index_of(*id) {
                for row in lv.block_range(index) {
                    reach[row] = true;
                }
            }
        }
    }
    reach
}

/// Occurrences in one row under the options.
/// Hits in the write-protected parts (Langtext, unit) of an STLB-Bau position.
fn stlb_hits(row: &LvRow, find: &str, options: &SearchOptions) -> usize {
    let Some(position) = row
        .as_position()
        .filter(|position| position.is_text_locked())
    else {
        return 0;
    };
    let (case, word) = (options.case_sensitive, options.whole_word);
    let mut hits = 0;
    if options.scope.long {
        hits += count_in_rich_text(position.long_text(), find, case, word);
    }
    if options.scope.unit {
        hits += find_all(position.unit(), find, case, word).len();
    }
    hits
}

pub fn hits_in_row(row: &LvRow, find: &str, options: &SearchOptions) -> usize {
    if find.is_empty() {
        return 0;
    }
    let scope = options.scope;
    let (case, word) = (options.case_sensitive, options.whole_word);
    let plain = |text: &str| find_all(text, find, case, word).len();
    let rich = |text: &RichText| count_in_rich_text(text, find, case, word);
    match row {
        LvRow::Group(group) => {
            if scope.short {
                plain(group.short_text())
            } else {
                0
            }
        }
        LvRow::Position(position) => {
            let mut hits = 0;
            if scope.short {
                hits += plain(position.short_text());
            }
            if scope.long {
                hits += rich(position.long_text());
            }
            if scope.unit {
                hits += plain(position.unit());
            }
            hits
        }
        LvRow::Note(note) => {
            if scope.notes {
                plain(note.heading()) + rich(note.text())
            } else {
                0
            }
        }
        LvRow::Performance(performance) => {
            if scope.performances {
                plain(performance.short_text()) + rich(performance.long_text())
            } else {
                0
            }
        }
        LvRow::Spacer(_) => 0,
    }
}

/// Ids of all rows with at least one hit (for marking them in the table).
pub fn matching_rows(lv: &Lv, find: &str, options: &SearchOptions) -> HashSet<RowId> {
    if find.is_empty() || options.scope.is_empty() {
        return HashSet::new();
    }
    let reach = rows_in_reach(lv, options);
    lv.rows()
        .iter()
        .enumerate()
        .filter(|(index, row)| reach[*index] && hits_in_row(row, find, options) > 0)
        .map(|(_, row)| row.id())
        .collect()
}

/// Next row after `after` (wrapping) with a hit.
pub fn find_next(
    lv: &Lv,
    find: &str,
    options: &SearchOptions,
    after: Option<usize>,
) -> Option<usize> {
    let count = lv.rows().len();
    if find.is_empty() || count == 0 || options.scope.is_empty() {
        return None;
    }
    let reach = rows_in_reach(lv, options);
    let start = after.map(|index| index + 1).unwrap_or(0);
    (0..count)
        .map(|offset| (start + offset) % count)
        .find(|&index| reach[index] && hits_in_row(&lv.rows()[index], find, options) > 0)
}

/// The first row with a hit in `range` (no wrap) – for the search over
/// several LVs, one piece after the other.
pub fn find_in(
    lv: &Lv,
    find: &str,
    options: &SearchOptions,
    range: std::ops::Range<usize>,
) -> Option<usize> {
    let count = lv.rows().len();
    if find.is_empty() || count == 0 || options.scope.is_empty() {
        return None;
    }
    let reach = rows_in_reach(lv, options);
    let end = range.end.min(count);
    (range.start.min(end)..end)
        .find(|&index| reach[index] && hits_in_row(&lv.rows()[index], find, options) > 0)
}

/// Replaces in one row. A finished position is not changed: Err with the
/// lock message. Returns the number of replacements.
pub fn replace_in_row(
    lv: &mut Lv,
    index: usize,
    find: &str,
    replacement: &str,
    options: &SearchOptions,
) -> Result<usize, String> {
    let Some(row) = lv.rows().get(index) else {
        return Ok(0);
    };
    if hits_in_row(row, find, options) == 0 {
        return Ok(0);
    }
    if row
        .as_position()
        .is_some_and(|position| position.is_locked())
    {
        lv.ensure_editable(&[row.id()])?;
    }
    // STLB-Bau text: Langtext and unit stay; only hits there – explain.
    if stlb_hits(row, find, options) > 0
        && stlb_hits(row, find, options) == hits_in_row(row, find, options)
    {
        lv.ensure_text_editable(&[row.id()])?;
    }
    let scope = options.scope;
    let (case, word) = (options.case_sensitive, options.whole_word);
    let plain = |text: &str| replace_in_str(text, find, replacement, case, word);
    let mut count = 0;
    match lv.row_mut(index) {
        Some(LvRow::Group(group)) if scope.short => {
            let (new, hits) = plain(group.short_text());
            if hits > 0 {
                group.set_short_text(new);
                count += hits;
            }
        }
        Some(LvRow::Position(position)) => {
            if scope.short {
                let (new, hits) = plain(position.short_text());
                if hits > 0 {
                    position.set_short_text(new);
                    count += hits;
                }
            }
            let free = !position.is_text_locked();
            if scope.long && free {
                count +=
                    replace_in_rich_text(position.long_text_mut(), find, replacement, case, word);
            }
            if scope.unit && free {
                let (new, hits) = plain(position.unit());
                if hits > 0 {
                    position.set_unit(new);
                    count += hits;
                }
            }
        }
        Some(LvRow::Note(note)) if scope.notes => {
            let (new, hits) = plain(note.heading());
            if hits > 0 {
                note.set_heading(new);
                count += hits;
            }
            count += replace_in_rich_text(note.text_mut(), find, replacement, case, word);
        }
        Some(LvRow::Performance(performance)) if scope.performances => {
            let (new, hits) = plain(performance.short_text());
            if hits > 0 {
                performance.set_short_text(new);
                count += hits;
            }
            let mut text = performance.long_text().clone();
            let hits = replace_in_rich_text(&mut text, find, replacement, case, word);
            if hits > 0 {
                performance.set_long_text(text);
                count += hits;
            }
        }
        _ => {}
    }
    Ok(count)
}

/// „Alle ersetzen“ over the reach of the options; finished positions are
/// skipped and counted.
pub fn replace_all(
    lv: &mut Lv,
    find: &str,
    replacement: &str,
    options: &SearchOptions,
) -> ReplaceReport {
    let mut report = ReplaceReport::default();
    if find.is_empty() || options.scope.is_empty() {
        return report;
    }
    let reach = rows_in_reach(lv, options);
    for index in 0..lv.rows().len() {
        if !reach[index] {
            continue;
        }
        let row = &lv.rows()[index];
        if row
            .as_position()
            .is_some_and(|position| position.is_locked())
        {
            let hits = hits_in_row(row, find, options);
            if hits > 0 {
                report.locked_hits += hits;
                report.locked_rows += 1;
            }
            continue;
        }
        report.stlb_hits += stlb_hits(row, find, options);
        if let Ok(count) = replace_in_row(lv, index, find, replacement, options) {
            if count > 0 {
                report.replaced += count;
                report.rows += 1;
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proava_domain::{
        Decimal, EditStatus, Lv, LvGroup, LvRow, Note, OzMask, Position, RichText, Run, Table,
        TextBlock,
    };

    use super::*;

    fn bold(text: &str) -> Run {
        Run {
            text: text.into(),
            bold: true,
            ..Run::default()
        }
    }

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation Sauter"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        let mut ddc = Position::new("", "DDC Sauter EY6", Decimal::ONE, "St", None);
        ddc.set_long_text(RichText::new(vec![TextBlock::paragraph(vec![
            Run::plain("Fabrikat "),
            bold("Sau"),
            Run::plain("ter, Typ EY6"),
        ])]));
        lv.push_position(ddc);
        let mut done = Position::new("", "Sauter Modul", Decimal::TWO, "St", None);
        done.set_status(EditStatus::Done);
        lv.push_position(done);
        lv.push_note(Note::new("Alle Geräte Sauter oder gleichwertig"));
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_position(Position::new("", "Fühler", Decimal::TEN, "Stck", None));
        lv.renumber();
        lv
    }

    #[test]
    fn plain_matching_cases_and_words() {
        assert_eq!(
            find_all("Sauter sauter SAUTER", "sauter", false, false).len(),
            3
        );
        assert_eq!(
            find_all("Sauter sauter SAUTER", "sauter", true, false).len(),
            1
        );
        assert_eq!(
            find_all("Raum Raumfühler Raum.", "Raum", true, true).len(),
            2
        );
        assert_eq!(
            find_all("Größe GRÖSSE größe", "größe", false, false).len(),
            2
        );
        assert_eq!(find_all("aaaa", "aa", true, false), vec![(0, 2), (2, 4)]);
        assert!(find_all("x", "", false, false).is_empty());
        let (text, count) = replace_in_str("Fühler-Fühler", "fühler", "Sensor", false, false);
        assert_eq!((text.as_str(), count), ("Sensor-Sensor", 2));
    }

    #[test]
    fn rich_text_keeps_formatting() {
        let mut text = RichText::new(vec![
            TextBlock::paragraph(vec![
                Run::plain("Typ "),
                bold("EY6"),
                Run::plain(" von EY6"),
            ]),
            TextBlock::Table(Table::from_texts(vec![vec!["EY6".into(), "x".into()]])),
            TextBlock::Image {
                file: "EY6.png".into(),
                width_mm: None,
            },
        ]);
        assert_eq!(count_in_rich_text(&text, "ey6", false, false), 3);
        assert_eq!(
            replace_in_rich_text(&mut text, "EY6", "EY7", true, false),
            3
        );
        let TextBlock::Paragraph { runs, .. } = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(runs[1].text, "EY7");
        assert!(runs[1].bold);
        assert_eq!(runs[2].text, " von EY7");
        assert!(!runs[2].bold);
        assert!(matches!(&text.blocks()[2], TextBlock::Image { file, .. } if file == "EY6.png"));

        // Über eine Laufgrenze: Formatierung des ersten Laufs.
        let mut text = RichText::new(vec![TextBlock::paragraph(vec![
            Run::plain("Fabrikat Sa"),
            bold("uter"),
            Run::plain(" EY"),
        ])]);
        assert_eq!(
            replace_in_rich_text(&mut text, "Sauter EY", "Kieback", true, false),
            1
        );
        assert_eq!(text.to_plain(), "Fabrikat Kieback");
        let TextBlock::Paragraph { runs, .. } = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(runs.len(), 1, "{runs:?}");
    }

    #[test]
    fn find_next_scope_and_selection() {
        let lv = lv();
        let options = SearchOptions::default();
        assert_eq!(find_next(&lv, "sauter", &options, None), Some(0));
        assert_eq!(find_next(&lv, "sauter", &options, Some(0)), Some(2));
        assert_eq!(find_next(&lv, "sauter", &options, Some(3)), Some(4));
        assert_eq!(find_next(&lv, "sauter", &options, Some(4)), Some(0));
        let no_notes = SearchOptions {
            scope: Scope {
                notes: false,
                ..Scope::default()
            },
            ..SearchOptions::default()
        };
        assert_eq!(find_next(&lv, "gleichwertig", &no_notes, None), None);
        let units = SearchOptions {
            scope: Scope {
                short: false,
                long: false,
                notes: false,
                performances: false,
                unit: true,
            },
            whole_word: true,
            ..SearchOptions::default()
        };
        assert_eq!(find_next(&lv, "St", &units, None), Some(2));
        assert_eq!(matching_rows(&lv, "St", &units).len(), 2);
        let only = SearchOptions {
            only: Some(HashSet::from([lv.rows()[5].id()])),
            ..SearchOptions::default()
        };
        assert_eq!(find_next(&lv, "sauter", &only, None), None);
        assert_eq!(find_next(&lv, "fühler", &only, None), Some(6));
    }

    #[test]
    fn replace_all_skips_finished_positions() {
        let mut lv = lv();
        let report = replace_all(&mut lv, "Sauter", "Kieback", &SearchOptions::default());
        assert_eq!(report.locked_hits, 1);
        assert_eq!(report.locked_rows, 1);
        // Titel, DDC-Kurztext, DDC-Langtext (über Laufgrenze), Hinweis.
        assert_eq!(report.replaced, 4);
        assert_eq!(report.rows, 3);
        assert!(report.message("Sauter").contains("fertigen"));
        assert_eq!(lv.rows()[3].short_text(), "Sauter Modul");
        assert_eq!(lv.rows()[2].short_text(), "DDC Kieback EY6");
        assert_eq!(
            lv.rows()[2].as_position().unwrap().long_text().to_plain(),
            "Fabrikat Kieback, Typ EY6"
        );
        let error =
            replace_in_row(&mut lv, 3, "Sauter", "x", &SearchOptions::default()).unwrap_err();
        assert!(error.contains("fertig"));
        assert!(
            matches!(&lv.rows()[4], LvRow::Note(note) if note.text().to_plain().contains("Kieback"))
        );
    }
}
