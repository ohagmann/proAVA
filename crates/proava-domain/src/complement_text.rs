//! Long text with its Textergänzungen written in at their anchors – for
//! print and plain views. Inline anchors become a run inside the paragraph
//! or list item, own-line anchors (and anchors at blocks without text) a
//! paragraph before that block; complements without anchor are left out
//! (the caller places them, e.g. after the text).
//!
//! For editing, [`embed`] writes one *field character* per Textergänzung
//! into the text (a private-use character that carries the index of the
//! complement, see [`field_char`]); the editor moves, selects and deletes it
//! like one letter and draws it as a field. [`extract`] turns the edited
//! text back into the long text and the complements with their new anchors.

use std::collections::HashSet;

use crate::position::{ComplementKind, TextAnchor, TextComplement};
use crate::rich_text::{normalize_runs, RichText, Run, TextBlock};

/// First field character (Supplementary Private Use Area-A): field `i` is
/// `FIELD_BASE + i`. These characters never come from typing or GAEB.
pub const FIELD_BASE: u32 = 0xF0000;
/// Number of field characters (indices `0..MAX_FIELDS`).
pub const MAX_FIELDS: usize = 0xFFFE;

/// Field character of the complement with this index.
pub fn field_char(index: usize) -> char {
    char::from_u32(FIELD_BASE + index.min(MAX_FIELDS - 1) as u32).expect("private use area")
}

/// Index of a field character, None for every other character.
pub fn field_index(character: char) -> Option<usize> {
    let code = character as u32;
    (FIELD_BASE..FIELD_BASE + MAX_FIELDS as u32)
        .contains(&code)
        .then(|| (code - FIELD_BASE) as usize)
}

/// Whether the text holds a field character.
pub fn has_fields(text: &str) -> bool {
    text.chars()
        .any(|character| field_index(character).is_some())
}

/// `text` without field characters.
pub fn strip_field_chars(text: &str) -> String {
    text.chars()
        .filter(|character| field_index(*character).is_none())
        .collect()
}

/// `run_of` gives the run for a complement (text and formatting).
///
/// A complement inside a paragraph is set off by a space where the text
/// brings none: after it when the following text starts with a letter or
/// digit („………… vom Bieter einzutragen.“, but „…………." and „………… ,“ stay
/// as they are), before it when it starts with a letter, digit or gap and
/// the text before ends with a word or punctuation („Tasten, Fabrikat:“).
/// Only the printed form changes, never the long text.
pub fn with_complements(
    text: &RichText,
    complements: &[TextComplement],
    run_of: impl Fn(&TextComplement) -> Run,
) -> RichText {
    place(
        text,
        complements,
        |_, complement, _| run_of(complement),
        false,
        true,
    )
}

/// Characters after which a following word needs no space: openings.
const OPENING: &[char] = &['(', '[', '{', '„', '‚', '“', '"', '/', '-', '–', '\u{a0}'];

/// `text` (the printed form of a complement) with the spaces
/// [`with_complements`] puts around it between `before` and `after` – for
/// text that leaves proAVA (copy as plain text, CSV).
pub fn set_off(before: Option<char>, text: &str, after: Option<char>) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    if let (Some(before), Some(first)) = (before, text.chars().next()) {
        if needs_space(before, first) {
            out.push(' ');
        }
    }
    out.push_str(text);
    if let (Some(last), Some(after)) = (text.chars().last(), after) {
        if needs_space(last, after) && after.is_alphanumeric() {
            out.push(' ');
        }
    }
    out
}

/// Whether a space belongs between `before` and `after` where a complement
/// meets the text (see [`with_complements`]).
fn needs_space(before: char, after: char) -> bool {
    let word = |character: char| {
        character.is_alphanumeric() || character == crate::position::PRINT_GAP || character == '…'
    };
    !before.is_whitespace() && !OPENING.contains(&before) && word(after)
}

/// The text with one field character per complement at its anchor; an
/// inline field takes the format of the text around it. Complements without
/// anchor stand at the end, each on a line of its own (where print and GAEB
/// put them).
pub fn embed(text: &RichText, complements: &[TextComplement]) -> RichText {
    place(
        text,
        complements,
        |index, _, around| match around {
            Some(run) => run.with_text(field_char(index)),
            None => Run::plain(field_char(index)),
        },
        true,
        false,
    )
}

/// Writes the complements in. `run_of` gets the index, the complement and –
/// for inline places – the run it stands in; `loose` puts complements
/// without anchor at the end (else they are left out); `spaced` sets inline
/// complements off by a space where the text brings none.
fn place(
    text: &RichText,
    complements: &[TextComplement],
    run_of: impl Fn(usize, &TextComplement, Option<&Run>) -> Run,
    loose: bool,
    spaced: bool,
) -> RichText {
    let blocks = text.blocks();
    let mut own: Vec<Vec<Run>> = vec![Vec::new(); blocks.len() + 1];
    let mut inline: Vec<Vec<(usize, usize)>> = vec![Vec::new(); blocks.len()];
    for (index, complement) in complements.iter().enumerate() {
        let Some(anchor) = complement.anchor else {
            if loose {
                own[blocks.len()].push(run_of(index, complement, None));
            }
            continue;
        };
        let block = anchor.block.min(blocks.len());
        let has_runs = blocks.get(block).and_then(TextBlock::runs).is_some();
        if anchor.own_line || !has_runs {
            own[block].push(run_of(index, complement, None));
        } else {
            inline[block].push((anchor.offset, index));
        }
    }
    let mut out = Vec::with_capacity(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        out.extend(
            own[index]
                .drain(..)
                .map(|run| TextBlock::paragraph(vec![run])),
        );
        let mut marks = std::mem::take(&mut inline[index]);
        if marks.is_empty() {
            out.push(block.clone());
            continue;
        }
        marks.sort_by_key(|(offset, _)| *offset);
        let mut block = block.clone();
        match &mut block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                *runs = insert_runs(
                    runs,
                    marks,
                    |complement, around| run_of(complement, &complements[complement], around),
                    spaced,
                );
            }
            _ => unreachable!("only blocks with runs get inline complements"),
        }
        out.push(block);
    }
    out.extend(
        own[blocks.len()]
            .drain(..)
            .map(|run| TextBlock::paragraph(vec![run])),
    );
    RichText::new(out)
}

/// Runs with the runs of complements inserted at character offsets
/// (clamped to the end); `run_of` gets the run they stand in. `spaced`: a
/// space between a complement and the text where [`needs_space`] says so.
fn insert_runs(
    runs: &[Run],
    marks: Vec<(usize, usize)>,
    run_of: impl Fn(usize, Option<&Run>) -> Run,
    spaced: bool,
) -> Vec<Run> {
    let last_char = |out: &[Run]| out.iter().rev().find_map(|run| run.text.chars().last());
    // A complement run, with a space before it when needed.
    let push_mark = |out: &mut Vec<Run>, mut mark: Run| {
        let first = mark.text.chars().next();
        if let (true, Some(before), Some(after)) = (spaced, last_char(out), first) {
            if needs_space(before, after) {
                mark.text.insert(0, ' ');
            }
        }
        out.push(mark);
    };
    let mut marks = marks.into_iter().peekable();
    let mut out: Vec<Run> = Vec::new();
    let mut position = 0;
    // The last thing pushed was a complement (its last char decides).
    let mut after_mark = false;
    for run in runs {
        let mut current = run.with_text("");
        for character in run.text.chars() {
            while let Some((_, mark)) = marks.next_if(|(offset, _)| *offset <= position) {
                if !current.text.is_empty() {
                    out.push(std::mem::replace(&mut current, run.with_text("")));
                }
                push_mark(&mut out, run_of(mark, Some(run)));
                after_mark = true;
            }
            if std::mem::take(&mut after_mark) && spaced {
                if let Some(before) = last_char(&out) {
                    if needs_space(before, character) && character.is_alphanumeric() {
                        if let Some(mark) = out.last_mut() {
                            mark.text.push(' ');
                        }
                    }
                }
            }
            current.text.push(character);
            position += 1;
        }
        if !current.text.is_empty() {
            out.push(current);
        }
    }
    let last = runs.last();
    for (_, mark) in marks {
        push_mark(&mut out, run_of(mark, last));
    }
    out
}

/// The long text and its complements from a text with field characters
/// (what [`embed`] made and the editor changed). `fields[i]` is the
/// complement of field character `i`; complements whose character is gone
/// are dropped, a character that stands twice (copied in the text) gives a
/// copy with a new Kennung. The result follows the order in the text.
///
/// A paragraph holding nothing but fields (and spaces) disappears; its
/// fields stand on a line of their own, like GAEB keeps them. Fields at the
/// very end that had no anchor before keep none (after the text).
pub fn extract(text: &RichText, fields: &[TextComplement]) -> (RichText, Vec<TextComplement>) {
    let mut blocks: Vec<TextBlock> = Vec::with_capacity(text.blocks().len());
    let mut found: Vec<(usize, TextAnchor)> = Vec::new();
    for block in text.blocks() {
        match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                if !has_fields(&joined) {
                    blocks.push(block.clone());
                    continue;
                }
                let only_fields = matches!(block, TextBlock::Paragraph { .. })
                    && joined.chars().all(|character| {
                        field_index(character).is_some() || character.is_whitespace()
                    });
                let mut offset = 0;
                for character in joined.chars() {
                    match field_index(character) {
                        Some(index) => found.push((
                            index,
                            TextAnchor {
                                block: blocks.len(),
                                offset: if only_fields { 0 } else { offset },
                                own_line: only_fields,
                            },
                        )),
                        None => offset += 1,
                    }
                }
                if only_fields {
                    continue;
                }
                let mut block = block.clone();
                if let TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } =
                    &mut block
                {
                    for run in runs.iter_mut() {
                        run.text = strip_field_chars(&run.text);
                    }
                    *runs = normalize_runs(std::mem::take(runs));
                }
                blocks.push(block);
            }
            TextBlock::Table(table) => {
                // Fields belong into the running text: one in a cell stands
                // on its own line after the table.
                let mut table = table.clone();
                let mut moved = Vec::new();
                for cell in table.rows.iter_mut().flatten() {
                    for run in cell.runs.iter_mut() {
                        moved.extend(run.text.chars().filter_map(field_index));
                        run.text = strip_field_chars(&run.text);
                    }
                    cell.runs = normalize_runs(std::mem::take(&mut cell.runs));
                }
                blocks.push(TextBlock::Table(table));
                found.extend(moved.into_iter().map(|index| {
                    (
                        index,
                        TextAnchor {
                            block: blocks.len(),
                            offset: 0,
                            own_line: true,
                        },
                    )
                }));
            }
            TextBlock::Image { .. } => blocks.push(block.clone()),
        }
    }
    let end = blocks.len();
    let mut seen = HashSet::new();
    let mut complements: Vec<TextComplement> = Vec::with_capacity(found.len());
    for (index, anchor) in found {
        let Some(field) = fields.get(index) else {
            continue;
        };
        let mut complement = field.clone();
        if !seen.insert(index) {
            let mut taken: Vec<TextComplement> = complements.clone();
            taken.extend(fields.iter().cloned());
            complement.marker = next_marker(&taken);
        }
        let at_end = anchor.own_line && anchor.block >= end;
        complement.anchor = if at_end && field.anchor.is_none() {
            None
        } else {
            Some(anchor)
        };
        complements.push(complement);
    }
    (RichText::new(blocks), complements)
}

/// The next free Kennung („01“, „02“ …): one above the highest number.
pub fn next_marker(complements: &[TextComplement]) -> String {
    let highest = complements
        .iter()
        .filter_map(|complement| complement.marker.trim().parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{:02}", highest + 1)
}

/// Where [`insert`] puts a new Textergänzung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldPlace {
    /// On a line of its own under the long text (after it, „unter dem
    /// Langtext“).
    End,
    /// On a line of its own after the first `n` blocks of the text
    /// (paragraphs, list items, tables, images; 0 = before all, past the
    /// end = at the end).
    AfterBlock(usize),
    /// Right after the first occurrence of this text (in a paragraph or
    /// list item).
    After(String),
}

/// The long text and complements with `complement` added at `place`; the
/// Kennung is filled in when empty. Errors when the text to insert after
/// is not found.
pub fn insert(
    text: &RichText,
    complements: &[TextComplement],
    mut complement: TextComplement,
    place: &FieldPlace,
) -> Result<(RichText, Vec<TextComplement>), String> {
    if complement.marker.trim().is_empty() {
        complement.marker = next_marker(complements);
    }
    let mut fields = complements.to_vec();
    let index = fields.len();
    match place {
        FieldPlace::End => {
            complement.anchor = None;
            fields.push(complement);
            Ok((text.clone(), fields))
        }
        FieldPlace::AfterBlock(count) => {
            let block = (*count).min(text.blocks().len());
            complement.anchor = Some(TextAnchor {
                block,
                offset: 0,
                own_line: true,
            });
            fields.push(complement);
            let embedded = embed(text, &fields);
            Ok(extract(&embedded, &fields))
        }
        FieldPlace::After(needle) => {
            let needle = needle.trim();
            if needle.is_empty() {
                return Err("Der Text, hinter den die Angabe soll, ist leer.".to_string());
            }
            fields.push(complement);
            let mut embedded = embed(text, complements);
            let mut done = false;
            for block in embedded.blocks_mut().iter_mut() {
                let (TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. }) = block
                else {
                    continue;
                };
                let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
                let visible = strip_field_chars(&joined);
                let Some(byte) = visible.find(needle) else {
                    continue;
                };
                let after = visible[..byte + needle.len()].chars().count();
                *runs = insert_after_visible(runs, after, index);
                done = true;
                break;
            }
            if !done {
                return Err(format!("„{needle}“ steht nicht im Langtext."));
            }
            Ok(extract(&embedded, &fields))
        }
    }
}

/// Runs with field `index` after `after` visible characters (field
/// characters do not count); the field takes the format to its left and a
/// space before it when the text there has none („Tasten, Fabrikat: …“).
fn insert_after_visible(runs: &[Run], after: usize, index: usize) -> Vec<Run> {
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut seen = 0;
    let mut done = false;
    for run in runs {
        if done {
            out.push(run.clone());
            continue;
        }
        let mut current = run.with_text("");
        for character in run.text.chars() {
            current.text.push(character);
            if field_index(character).is_none() {
                seen += 1;
            }
            if !done && seen == after {
                let spaced = character.is_whitespace();
                out.push(std::mem::replace(&mut current, run.with_text("")));
                let field = field_char(index).to_string();
                out.push(run.with_text(if spaced { field } else { format!(" {field}") }));
                done = true;
            }
        }
        if !current.text.is_empty() {
            out.push(current);
        }
    }
    normalize_runs(out)
}

/// Anchors after a block (image, PDF clip …) was inserted at `at`: all
/// places from that block on move one block down – a field keeps its place
/// in front of the text that follows it, fields at the end stay at the end.
pub fn shift_after_insert(complements: &mut [TextComplement], at: usize) {
    for anchor in complements
        .iter_mut()
        .filter_map(|complement| complement.anchor.as_mut())
    {
        if anchor.block >= at {
            anchor.block += 1;
        }
    }
}

/// Anchors after block `at` was removed: later places move one block up;
/// a field inside the removed block stands on its own line there.
pub fn shift_after_remove(complements: &mut [TextComplement], at: usize) {
    for anchor in complements
        .iter_mut()
        .filter_map(|complement| complement.anchor.as_mut())
    {
        if anchor.block > at {
            anchor.block -= 1;
        } else if anchor.block == at && !anchor.own_line {
            anchor.own_line = true;
            anchor.offset = 0;
        }
    }
}

/// Counts of a list of complements: (Bieterangaben, Angaben des Auftraggebers).
pub fn count_kinds(complements: &[TextComplement]) -> (usize, usize) {
    let bidder = complements
        .iter()
        .filter(|complement| complement.kind == ComplementKind::Bidder)
        .count();
    (bidder, complements.len() - bidder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{ComplementKind, TextAnchor, TextComplement};
    use crate::rich_text::{RichText, Run, TextBlock};

    fn complement(caption: &str, anchor: Option<TextAnchor>) -> TextComplement {
        TextComplement {
            marker: "1".into(),
            kind: ComplementKind::Owner,
            caption: caption.into(),
            body: "x".into(),
            tail: String::new(),
            anchor,
        }
    }

    #[test]
    fn complements_go_to_their_place() {
        let text = RichText::plain("Farbe .\nZweite Zeile");
        let inline = complement(
            "A",
            Some(TextAnchor {
                block: 0,
                offset: 6,
                own_line: false,
            }),
        );
        let own = complement(
            "B",
            Some(TextAnchor {
                block: 1,
                offset: 0,
                own_line: true,
            }),
        );
        let loose = complement("C", None);
        let result = with_complements(&text, &[inline, own, loose], |c| {
            Run::plain(format!("[{}]", c.caption))
        });
        assert_eq!(result.to_plain(), "Farbe [A].\n[B]\nZweite Zeile");
    }

    fn field(caption: &str, kind: ComplementKind, anchor: Option<TextAnchor>) -> TextComplement {
        TextComplement {
            marker: String::new(),
            kind,
            caption: caption.into(),
            body: String::new(),
            tail: String::new(),
            anchor,
        }
    }

    fn at(block: usize, offset: usize, own_line: bool) -> Option<TextAnchor> {
        Some(TextAnchor {
            block,
            offset,
            own_line,
        })
    }

    /// Embedding and extracting again gives the same text and anchors.
    #[test]
    fn embed_and_extract_round_trip() {
        let text = RichText::new(vec![
            TextBlock::paragraph(vec![
                Run::plain("Regler, "),
                Run {
                    text: "fett".into(),
                    bold: true,
                    ..Run::default()
                },
            ]),
            TextBlock::paragraph(vec![Run::plain("Zweite Zeile")]),
        ]);
        let complements = vec![
            field("Fabrikat:", ComplementKind::Bidder, at(0, 8, false)),
            field("Typ:", ComplementKind::Bidder, at(1, 0, true)),
            field("Nennweite", ComplementKind::Owner, at(0, 12, false)),
            field("Hersteller:", ComplementKind::Bidder, None),
        ];
        let embedded = embed(&text, &complements);
        assert_eq!(
            embedded.blocks().len(),
            4,
            "Typ und Hersteller auf eigener Zeile"
        );
        let plain = embedded.to_plain();
        assert_eq!(
            plain,
            format!(
                "Regler, {}fett{}\n{}\nZweite Zeile\n{}",
                field_char(0),
                field_char(2),
                field_char(1),
                field_char(3)
            )
        );
        // The inline field after „Regler, “ is plain, the one after „fett“ bold.
        let runs = embedded.blocks()[0].runs().unwrap();
        assert!(runs
            .iter()
            .any(|run| run.bold && run.text.ends_with(field_char(2))));
        let (back, again) = extract(&embedded, &complements);
        assert_eq!(back, text);
        assert_eq!(again[0], complements[0]);
        assert_eq!(again[1], complements[2], "Reihenfolge wie im Text");
        assert_eq!(again[2], complements[1]);
        assert_eq!(again[3], complements[3], "ohne Anker bleibt ohne Anker");
    }

    /// A deleted field drops its complement, a doubled one gets a copy with
    /// a new Kennung, a field in a table cell moves after the table.
    #[test]
    fn extract_drops_doubles_and_moves() {
        let mut first = field("Fabrikat:", ComplementKind::Bidder, None);
        first.marker = "01".into();
        let mut second = field("Typ:", ComplementKind::Bidder, None);
        second.marker = "02".into();
        let fields = vec![first.clone(), second];
        let text = RichText::new(vec![
            TextBlock::paragraph(vec![Run::plain(format!(
                "A {} B {}",
                field_char(0),
                field_char(0)
            ))]),
            TextBlock::Table(crate::rich_text::Table {
                rows: vec![vec![crate::rich_text::Cell {
                    runs: vec![Run::plain(format!("x{}", field_char(0)))],
                    ..Default::default()
                }]],
                ..Default::default()
            }),
        ]);
        let (clean, complements) = extract(&text, &fields);
        assert_eq!(clean.to_plain().lines().next(), Some("A  B "));
        assert_eq!(complements.len(), 3, "Typ ist weg, Fabrikat dreimal");
        assert_eq!(complements[0].marker, "01");
        assert_eq!(complements[1].marker, "03");
        assert_eq!(complements[2].marker, "04");
        assert_eq!(complements[0].anchor, at(0, 2, false));
        assert_eq!(complements[1].anchor, at(0, 5, false));
        assert_eq!(
            complements[2].anchor, None,
            "nach der Tabelle am Ende, vorher ohne Anker"
        );
    }

    #[test]
    fn insert_at_the_three_places() {
        let text = RichText::plain("Raumbediengerät mit Display\nMontage auf UP-Dose");
        let new = |caption: &str| field(caption, ComplementKind::Bidder, None);
        let (same, list) = insert(&text, &[], new("Fabrikat:"), &FieldPlace::End).unwrap();
        assert_eq!(same, text);
        assert_eq!(list[0].marker, "01");
        assert_eq!(list[0].anchor, None);
        let (same, list) = insert(&text, &list, new("Typ:"), &FieldPlace::AfterBlock(1)).unwrap();
        assert_eq!(same, text);
        assert_eq!(
            (list[0].caption.as_str(), list[0].marker.as_str()),
            ("Typ:", "02"),
            "Reihenfolge wie im Text"
        );
        assert_eq!(list[0].anchor, at(1, 0, true));
        assert_eq!(list[1].anchor, None);
        let (same, list) = insert(
            &text,
            &list,
            new("Farbe:"),
            &FieldPlace::After("Display".into()),
        )
        .unwrap();
        assert_eq!(
            same.to_plain(),
            "Raumbediengerät mit Display \nMontage auf UP-Dose",
            "Leerzeichen davor"
        );
        assert_eq!(list[0].caption, "Farbe:", "Reihenfolge wie im Text");
        assert_eq!(list[0].anchor, at(0, 28, false));
        assert!(insert(
            &text,
            &list,
            new("x"),
            &FieldPlace::After("gibt es nicht".into())
        )
        .is_err());
        let printed = with_complements(&same, &list, |c| Run::plain(format!("[{}]", c.caption)));
        assert_eq!(
            printed.to_plain(),
            "Raumbediengerät mit Display [Farbe:]\n[Typ:]\nMontage auf UP-Dose"
        );
    }

    #[test]
    fn field_chars_are_recognised() {
        assert_eq!(field_index(field_char(0)), Some(0));
        assert_eq!(field_index(field_char(4711)), Some(4711));
        assert_eq!(field_index('a'), None);
        assert_eq!(field_index('\u{FFFC}'), None);
        assert_eq!(strip_field_chars(&format!("a{}b", field_char(3))), "ab");
    }

    /// Block inserted or removed outside the editor (image, PDF clip): the
    /// fields keep their place in the text.
    #[test]
    fn anchors_follow_inserted_and_removed_blocks() {
        let mut list = vec![
            field("A", ComplementKind::Bidder, at(0, 3, false)),
            field("B", ComplementKind::Bidder, at(1, 0, true)),
            field("C", ComplementKind::Bidder, at(2, 0, true)),
            field("D", ComplementKind::Bidder, None),
        ];
        shift_after_insert(&mut list, 1);
        assert_eq!(list[0].anchor, at(0, 3, false), "davor bleibt");
        assert_eq!(list[1].anchor, at(2, 0, true), "vor dem Text, der folgt");
        assert_eq!(list[2].anchor, at(3, 0, true), "am Ende bleibt am Ende");
        assert_eq!(list[3].anchor, None);
        shift_after_remove(&mut list, 1);
        assert_eq!(list[1].anchor, at(1, 0, true));
        assert_eq!(list[2].anchor, at(2, 0, true));
        shift_after_remove(&mut list, 0);
        assert_eq!(list[0].anchor, at(0, 0, true), "Block weg: eigene Zeile");

        let mut position = crate::Position::new("", "x", crate::Decimal::ONE, "St", None);
        position.set_long_text(RichText::plain("Eins\nZwei"));
        position.set_complements(vec![field("Typ:", ComplementKind::Bidder, at(1, 4, false))]);
        position.insert_long_text_block(
            0,
            TextBlock::Image {
                file: "a.png".into(),
                width_mm: None,
            },
        );
        assert_eq!(position.complements()[0].anchor, at(2, 4, false));
        let printed = with_complements(position.long_text(), position.complements(), |c| {
            Run::plain(c.caption.clone())
        });
        assert_eq!(
            printed.blocks()[2]
                .runs()
                .unwrap()
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>(),
            "Zwei Typ:"
        );
    }

    /// Printed form: a complement inside a paragraph is set off by a space
    /// where the text brings none – after it before a letter or digit, not
    /// before a punctuation mark; the long text itself stays as it is.
    #[test]
    fn printed_complements_are_set_off_by_a_space() {
        // One paragraph (a line break inside it, as GAEB `<br/>` gives).
        let print = |text: &str, complement: TextComplement| {
            let rich = RichText::new(vec![TextBlock::paragraph(vec![Run::plain(text)])]);
            with_complements(&rich, &[complement], |c| Run::plain(c.print_text())).to_plain()
        };
        let gap = |caption: &str, block, offset| TextComplement {
            kind: ComplementKind::Bidder,
            caption: caption.into(),
            anchor: at(block, offset, false),
            ..TextComplement::default()
        };
        // The text after the field says „vom Bieter einzutragen“ itself.
        assert_eq!(
            print(
                "Eingabe von Programmen,\nvom Bieter einzutragen.",
                gap("Hardwareliste Beiblatt 70-4 ", 0, 24)
            ),
            "Eingabe von Programmen,\nHardwareliste Beiblatt 70-4 ………… vom Bieter einzutragen."
        );
        // Before a punctuation mark: none.
        assert_eq!(
            print("Farbe ., Ende", gap("RAL", 0, 6)),
            "Farbe RAL …………., Ende"
        );
        // Before the field: after a word or comma, not after a space or an opening.
        assert_eq!(
            print("Tasten,mehr", gap("Fabrikat:", 0, 7)),
            "Tasten, Fabrikat: ………… mehr"
        );
        assert_eq!(print("Typ (bitte)", gap("", 0, 5)), "Typ (………… bitte)");
        // A space already there: none added.
        assert_eq!(print("A  B", gap("X", 0, 2)), "A X ………… B");
        // Owner value between words.
        let owner = TextComplement {
            kind: ComplementKind::Owner,
            body: "220".into(),
            anchor: at(0, 12, false),
            ..TextComplement::default()
        };
        assert_eq!(
            print("Spannung DC V, 50 Hz", owner),
            "Spannung DC 220 V, 50 Hz"
        );
        // Two fields at the same place.
        let rich = RichText::plain("Gerät, Ende");
        let both = [gap("Fabrikat:", 0, 7), gap("Typ:", 0, 7)];
        assert_eq!(
            with_complements(&rich, &both, |c| Run::plain(c.print_text())).to_plain(),
            "Gerät, Fabrikat: ………… Typ: ………… Ende"
        );
        // The same rule for text that leaves proAVA.
        assert_eq!(set_off(Some(','), "Typ: …………", Some('v')), " Typ: ………… ");
        assert_eq!(set_off(Some('\n'), "…………", Some('.')), "…………");
        assert_eq!(set_off(None, "…………", None), "…………");
    }
}
