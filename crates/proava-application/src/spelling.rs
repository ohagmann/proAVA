//! Spell checking of a whole LV: which texts there are (with a place the
//! user recognises) and replacing a word everywhere. The checking itself is
//! in `proava-spell`; this module only knows the LV.

use proava_domain::{Lv, LvRow, RichText, RowId};

/// Which text of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellField {
    /// Kurztext (titles, positions, Ausführungsbeschreibungen) or heading of a hint.
    Short,
    /// Langtext, hint text, text of an Ausführungsbeschreibung.
    Long,
}

/// One text of the LV.
#[derive(Debug, Clone, PartialEq)]
pub struct SpellPlace {
    pub row: RowId,
    /// „01.02.0010“, „Hinweis nach 01.02“, „A0001“ …
    pub label: String,
    pub field: SpellField,
    pub text: String,
}

/// All texts of the LV in order.
pub fn lv_texts(lv: &Lv) -> Vec<SpellPlace> {
    let mut places = Vec::new();
    let mut last_ordinal = String::new();
    let mut push = |row: RowId, label: &str, field: SpellField, text: String| {
        if !text.trim().is_empty() {
            places.push(SpellPlace {
                row,
                label: label.to_string(),
                field,
                text,
            });
        }
    };
    for row in lv.rows() {
        let id = row.id();
        match row {
            LvRow::Group(group) => {
                last_ordinal = group.ordinal().to_string();
                push(
                    id,
                    group.ordinal(),
                    SpellField::Short,
                    group.short_text().to_string(),
                );
            }
            LvRow::Position(position) => {
                last_ordinal = position.ordinal().to_string();
                push(
                    id,
                    position.ordinal(),
                    SpellField::Short,
                    position.short_text().to_string(),
                );
                push(
                    id,
                    position.ordinal(),
                    SpellField::Long,
                    position.long_text().to_plain(),
                );
            }
            LvRow::Note(note) => {
                let label = if last_ordinal.is_empty() {
                    "Hinweis am Anfang".to_string()
                } else {
                    format!("Hinweis nach {last_ordinal}")
                };
                push(id, &label, SpellField::Short, note.heading().to_string());
                push(id, &label, SpellField::Long, note.text().to_plain());
            }
            LvRow::Performance(performance) => {
                let label = performance.mark();
                push(
                    id,
                    &label,
                    SpellField::Short,
                    performance.label().to_string(),
                );
                push(
                    id,
                    &label,
                    SpellField::Short,
                    performance.short_text().to_string(),
                );
                push(
                    id,
                    &label,
                    SpellField::Long,
                    performance.long_text().to_plain(),
                );
            }
            LvRow::Spacer(_) => {}
        }
    }
    places
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Replaces `word` as a whole word (case-sensitive) in a plain text.
pub fn replace_word_in(text: &str, word: &str, replacement: &str) -> (String, usize) {
    if word.is_empty() {
        return (text.to_string(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    let mut i = 0;
    while i < chars.len() {
        let end = i + needle.len();
        let hit = end <= chars.len()
            && chars[i..end] == needle[..]
            && (i == 0 || !is_word_char(chars[i - 1]))
            && (end == chars.len() || !is_word_char(chars[end]));
        if hit {
            out.push_str(replacement);
            count += 1;
            i = end;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, count)
}

/// Replaces `word` everywhere in the LV (whole words, case-sensitive,
/// formatting kept); finished positions and the Langtext of STLB-Bau
/// positions stay untouched. Returns (replaced, skipped).
pub fn replace_word(lv: &mut Lv, word: &str, replacement: &str) -> (usize, usize) {
    let mut replaced = 0;
    let mut skipped = 0;
    for index in 0..lv.rows().len() {
        let id = lv.rows()[index].id();
        let editable = lv.ensure_editable(&[id]).is_ok();
        let short = |text: &str, count: &mut usize| {
            let (new, hits) = replace_word_in(text, word, replacement);
            *count += hits;
            (hits > 0).then_some(new)
        };
        let rich = |text: &mut RichText| text.replace_matches(word, replacement, true, true);
        if !editable {
            let mut hits = 0;
            for place in lv_texts_of_row(&lv.rows()[index]) {
                hits += replace_word_in(&place, word, replacement).1;
            }
            skipped += hits;
            continue;
        }
        let Some(row) = lv.row_mut(index) else {
            continue;
        };
        match row {
            LvRow::Group(group) => {
                if let Some(new) = short(group.short_text(), &mut replaced) {
                    group.set_short_text(new);
                }
            }
            LvRow::Position(position) => {
                if let Some(new) = short(position.short_text(), &mut replaced) {
                    position.set_short_text(new);
                }
                if position.is_text_locked() {
                    // STLB-Bau text: write-protected, counted as skipped.
                    skipped +=
                        position
                            .long_text()
                            .clone()
                            .replace_matches(word, replacement, true, true);
                } else {
                    replaced += rich(position.long_text_mut());
                }
            }
            LvRow::Note(note) => {
                if let Some(new) = short(note.heading(), &mut replaced) {
                    note.set_heading(new);
                }
                replaced += rich(note.text_mut());
            }
            LvRow::Performance(performance) => {
                if let Some(new) = short(performance.label(), &mut replaced) {
                    performance.set_label(new);
                }
                if let Some(new) = short(performance.short_text(), &mut replaced) {
                    performance.set_short_text(new);
                }
                replaced += rich(performance.long_text_mut());
            }
            LvRow::Spacer(_) => {}
        }
    }
    (replaced, skipped)
}

fn lv_texts_of_row(row: &LvRow) -> Vec<String> {
    match row {
        LvRow::Group(group) => vec![group.short_text().to_string()],
        LvRow::Position(position) => {
            vec![
                position.short_text().to_string(),
                position.long_text().to_plain(),
            ]
        }
        LvRow::Note(note) => vec![note.heading().to_string(), note.text().to_plain()],
        LvRow::Performance(performance) => vec![
            performance.label().to_string(),
            performance.short_text().to_string(),
            performance.long_text().to_plain(),
        ],
        LvRow::Spacer(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{LvGroup, Note, OzMask, Position, RichText, Run, TextBlock};

    use super::*;

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Stationen mit Fehlr"));
        let mut position = Position::new("", "Fehlr im Kurztext", 1.into(), "St", None);
        position.set_long_text(RichText::new(vec![TextBlock::paragraph(vec![
            Run::plain("Ein "),
            Run::bold("Fehlr"),
            Run::plain(" und Fehlrhaft"),
        ])]));
        lv.push_position(position);
        lv.push_note(Note::new("Hinweis ohne Fehlr"));
        lv.renumber();
        lv
    }

    #[test]
    fn texts_with_places() {
        let places = lv_texts(&lv());
        let labels: Vec<(&str, SpellField)> =
            places.iter().map(|p| (p.label.as_str(), p.field)).collect();
        assert_eq!(
            labels,
            vec![
                ("01", SpellField::Short),
                ("01.0010", SpellField::Short),
                ("01.0010", SpellField::Long),
                ("Hinweis nach 01.0010", SpellField::Long),
            ]
        );
    }

    #[test]
    fn replaces_whole_words_everywhere_and_keeps_formats() {
        let mut lv = lv();
        assert_eq!(replace_word(&mut lv, "Fehlr", "Fehler"), (4, 0));
        let position = lv.rows()[1].as_position().unwrap();
        assert_eq!(position.short_text(), "Fehler im Kurztext");
        assert_eq!(
            position.long_text().blocks()[0],
            TextBlock::paragraph(vec![
                Run::plain("Ein "),
                Run::bold("Fehler"),
                Run::plain(" und Fehlrhaft"),
            ])
        );
        assert_eq!(
            replace_word_in("a-Fehlr, Fehlr.", "Fehlr", "X"),
            ("a-X, X.".to_string(), 2)
        );
    }
}
