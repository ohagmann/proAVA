//! Hard-wrapped text lines back to paragraphs, used by both codecs.
//!
//! GAEB 90 carries texts as fixed lines (Langtext 55, Kurztext 70,
//! vertragliche Regelungen 72 characters). Reading joins the lines of a
//! paragraph again and recognises bullet points; writing wraps them at word
//! boundaries.
//!
//! List levels: GAEB 90 knows only lines, so a deeper level is written as
//! two more blanks before the marker („  - “, „  1. “). Reading takes the
//! indent relative to the items before: the first item of a list is level
//! 0 however far it stands in, a marker further in (at least two columns)
//! one level deeper, one back on an earlier indent that level again.

use crate::model::{Run, TextBlock, MAX_LIST_LEVEL};

/// Words after a line-end hyphen that mean the hyphen belongs to a
/// shortened compound („Zu- und Abluft“), not to a split word.
const CONJUNCTIONS: [&str; 8] = ["und", "oder", "bzw.", "sowie", "bis", "als", "wie", "u."];

/// Lines that only hold a bullet (Wingdings bullets often arrive as „l“).
const LONE_BULLETS: [&str; 7] = ["l", "•", "-", "*", "·", "–", "o"];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Paragraph,
    Item { ordered: bool, level: u8 },
}

struct Open {
    kind: Kind,
    text: String,
    last_line: String,
}

#[derive(Default)]
struct Flow {
    blocks: Vec<TextBlock>,
    open: Option<Open>,
    /// Indents (columns before the marker) of the list levels so far,
    /// outermost first; cleared by a paragraph of text.
    indents: Vec<usize>,
}

/// Level of a marker standing `indent` columns in, after the levels
/// `indents` (see the module doc); gives the levels after it, too.
fn level_at(indents: &[usize], indent: usize) -> (u8, Vec<usize>) {
    let mut indents = indents.to_vec();
    while indents.last().is_some_and(|&top| indent + 1 < top) {
        indents.pop();
    }
    match indents.last() {
        Some(&top) if indent <= top + 1 => {}
        _ if indents.len() > usize::from(MAX_LIST_LEVEL) => {}
        _ => indents.push(indent),
    }
    (indents.len().saturating_sub(1) as u8, indents)
}

impl Flow {
    fn close(&mut self) {
        if let Some(done) = self.open.take() {
            let runs = vec![Run {
                text: done.text,
                ..Run::default()
            }];
            self.blocks.push(match done.kind {
                Kind::Paragraph => TextBlock::paragraph(runs),
                Kind::Item { ordered, level } => TextBlock::ListItem {
                    ordered,
                    level,
                    runs,
                    align: Default::default(),
                },
            });
        }
    }

    fn start(&mut self, kind: Kind, text: &str, line: &str) {
        self.close();
        if kind == Kind::Paragraph {
            self.indents.clear();
        }
        self.open = Some(Open {
            kind,
            text: text.to_string(),
            last_line: line.to_string(),
        });
    }

    /// A list item whose marker stands `indent` columns in.
    fn item(&mut self, ordered: bool, indent: usize, text: &str, line: &str) {
        let (level, indents) = level_at(&self.indents, indent);
        self.start(Kind::Item { ordered, level }, text, line);
        self.indents = indents;
    }

    fn line(&mut self, line: &str, width: usize) {
        let trimmed = line.trim_start();
        let indent = line.chars().count() - trimmed.chars().count();
        if let Some((number, rest)) = list_marker(trimmed) {
            let accepted = match number {
                None => true,
                Some(n) => {
                    let previous_ends = self.open.as_ref().is_none_or(|o| {
                        matches!(o.kind, Kind::Item { .. })
                            || o.last_line.ends_with(['.', ':', ';', '!', '?', ','])
                    });
                    let (level, _) = level_at(&self.indents, indent);
                    previous_ends && n == next_number(&self.blocks, self.open.as_ref(), level)
                }
            };
            if accepted {
                self.item(number.is_some(), indent, rest, line);
                return;
            }
        }
        match self.open.as_mut() {
            Some(current) if joins(&current.last_line, line, width) => {
                append(&mut current.text, trimmed);
                current.last_line = line.to_string();
            }
            _ => self.start(Kind::Paragraph, line, line),
        }
    }
}

/// Joins fixed lines into paragraphs and list items. `width` is the line
/// length the writer of the file wrapped at. RTF remnants are removed,
/// `\\par` ends a paragraph.
pub(crate) fn reflow(lines: &[String], width: usize) -> Vec<TextBlock> {
    let mut cleaned: Vec<(String, bool)> = Vec::new();
    let rtf = lines.iter().any(|line| looks_like_rtf(line));
    for line in lines {
        let (text, hard_end) = if rtf {
            clean_line(line)
        } else {
            (line.trim_end().to_string(), false)
        };
        let parts: Vec<&str> = text.split('\n').collect();
        let last = parts.len() - 1;
        for (i, part) in parts.into_iter().enumerate() {
            cleaned.push((part.trim_end().to_string(), i < last || hard_end));
        }
    }

    let mut flow = Flow::default();
    // A line holding only a bullet, waiting for the text it marks.
    let mut bullet: Option<(&str, usize)> = None;
    // A bullet no text follows (empty line, another bullet, the end) is
    // text itself – a lone „*“ or „o“ must not vanish.
    let keep = |flow: &mut Flow, bullet: &mut Option<(&str, usize)>| {
        if let Some((text, _)) = bullet.take() {
            flow.start(Kind::Paragraph, text, text);
            flow.close();
        }
    };
    for (line, hard_end) in &cleaned {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            flow.close();
            keep(&mut flow, &mut bullet);
            flow.blocks.push(TextBlock::paragraph(vec![Run::default()]));
            continue;
        }
        if LONE_BULLETS.contains(&trimmed) {
            flow.close();
            keep(&mut flow, &mut bullet);
            bullet = Some((trimmed, line.chars().count() - trimmed.chars().count()));
            continue;
        }
        if let Some((_, indent)) = bullet.take() {
            flow.item(false, indent, trimmed, line);
        } else {
            flow.line(line, width);
        }
        if *hard_end {
            flow.close();
        }
    }
    flow.close();
    keep(&mut flow, &mut bullet);
    flow.blocks
}

/// Number the next ordered item on `level` must carry to continue the
/// list: numbered items on that level since the last block that ends it
/// (text, a bullet on that level, an item further out); deeper items are
/// Unterpunkte and do not count.
fn next_number(blocks: &[TextBlock], open: Option<&Open>, level: u8) -> u32 {
    let open = open.map(|open| match open.kind {
        Kind::Item { ordered, level } => Some((ordered, level)),
        Kind::Paragraph => None,
    });
    let kinds = blocks
        .iter()
        .map(|block| match block {
            TextBlock::ListItem { ordered, level, .. } => Some((*ordered, *level)),
            _ => None,
        })
        .chain(open);
    let kinds: Vec<_> = kinds.collect();
    let mut count = 1;
    for kind in kinds.iter().rev() {
        match kind {
            Some((_, deeper)) if *deeper > level => {}
            Some((true, same)) if *same == level => count += 1,
            _ => break,
        }
    }
    count
}

/// `- text`, `• text`, `* text`, `1. text`, `1) text`: number (if any) and text.
pub(crate) fn list_marker(line: &str) -> Option<(Option<u32>, &str)> {
    for bullet in ["- ", "• ", "* ", "· ", "– "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some((None, rest.trim_start()));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if (1..=2).contains(&digits) {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((line[..digits].parse().ok(), rest.trim_start()));
        }
    }
    None
}

pub(crate) fn tabular(line: &str) -> bool {
    line.trim_start().contains("   ") || line.contains('\t')
}

/// Does `next` continue the paragraph that ended with `previous`?
fn joins(previous: &str, next: &str, width: usize) -> bool {
    let previous = previous.trim_end();
    if next.starts_with("  ") || tabular(previous) || tabular(next) {
        return false;
    }
    let Some(first) = next.split_whitespace().next() else {
        return false;
    };
    let full = previous.chars().count() + 1 + first.chars().count() > width;
    let mut tail = previous.chars().rev();
    let split_word = tail.next() == Some('-') && tail.next().is_some_and(char::is_alphanumeric);
    if split_word {
        return true;
    }
    if previous.ends_with(':') {
        return full;
    }
    let lower = first.chars().next().is_some_and(char::is_lowercase);
    full || (lower && !previous.ends_with(['.', '!', '?', ';']))
}

/// Appends a continuation line; a split word („Gebäude-“ + „automation“)
/// loses its hyphen, compounds („Peer-“ + „to-Peer“, „Zu-“ + „und“) keep it.
pub(crate) fn append(text: &mut String, next: &str) {
    let mut before = text.chars().rev();
    let hyphen = before.next() == Some('-');
    let letter = before.next();
    if !hyphen || !letter.is_some_and(char::is_alphanumeric) {
        text.push(' ');
        text.push_str(next);
        return;
    }
    let first = next.split_whitespace().next().unwrap_or("");
    if CONJUNCTIONS.contains(&first) {
        text.push(' ');
    } else if letter.is_some_and(char::is_lowercase)
        && first.chars().next().is_some_and(char::is_lowercase)
        && !first.contains('-')
    {
        text.pop();
    }
    text.push_str(next);
}

/// Text lines of blocks: one per paragraph, list items with their marker,
/// line breaks inside a paragraph kept.
pub(crate) fn logical_lines(blocks: &[TextBlock]) -> Vec<String> {
    let mut out = Vec::new();
    // Numbers per level, as nested lists count (see the domain).
    let mut counters = [0u32; MAX_LIST_LEVEL as usize + 1];
    for block in blocks {
        let mut number = 0;
        match block {
            TextBlock::ListItem { ordered, level, .. } => {
                let at = usize::from((*level).min(MAX_LIST_LEVEL));
                for deeper in counters.iter_mut().skip(at + 1) {
                    *deeper = 0;
                }
                counters[at] = if *ordered { counters[at] + 1 } else { 0 };
                number = counters[at];
            }
            _ => counters = Default::default(),
        }
        let indent = |level: &u8| "  ".repeat(usize::from((*level).min(MAX_LIST_LEVEL)));
        let text = match block {
            TextBlock::Paragraph { runs, .. } => runs_text(runs),
            // An empty list item is an empty line: a lone „-“ would be read
            // back glued to the next line („- Wort“).
            TextBlock::ListItem { runs, .. } if runs_text(runs).trim().is_empty() => String::new(),
            TextBlock::ListItem {
                ordered: false,
                level,
                runs,
                ..
            } => format!("{}- {}", indent(level), runs_text(runs)),
            TextBlock::ListItem {
                ordered: true,
                level,
                runs,
                ..
            } => {
                format!("{}{number}. {}", indent(level), runs_text(runs))
            }
            TextBlock::Table(table) => table
                .texts()
                .iter()
                .map(|cells| cells.join("   "))
                .collect::<Vec<_>>()
                .join("\n"),
            TextBlock::Image { .. } => continue,
        };
        out.extend(text.split('\n').map(str::to_string));
    }
    out
}

fn runs_text(runs: &[Run]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Wraps every line at `width` characters, preferably at a blank.
pub(crate) fn wrap_lines(lines: &[String], width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        let mut rest: Vec<char> = line.trim_end().chars().collect();
        loop {
            if rest.len() <= width {
                out.push(rest.iter().collect());
                break;
            }
            let blank_before = |end: usize| {
                rest[..end]
                    .iter()
                    .rposition(|&c| c == ' ')
                    .filter(|&pos| rest[..pos].iter().any(|&c| c != ' '))
            };
            let mut cut = blank_before(width + 1).unwrap_or(width);
            // A line holding only „-“, „*“, „o“ … reads back as a bullet (a
            // list item with the next line): take the word before along, or
            // split the long word one character earlier.
            let lone = |cut: usize| {
                let next: String = rest[cut..].iter().collect();
                LONE_BULLETS.contains(&next.trim())
            };
            if cut > 1 && lone(cut) {
                cut = blank_before(cut).unwrap_or(cut - 1);
            }
            out.push(
                rest[..cut]
                    .iter()
                    .collect::<String>()
                    .trim_end()
                    .to_string(),
            );
            let skip = rest[cut..].iter().take_while(|&&c| c == ' ').count();
            rest = rest[cut + skip..].to_vec();
        }
    }
    out
}

/// Short texts and contract terms are plain strings: paragraphs joined with
/// line breaks, list items with their marker.
pub(crate) fn reflow_plain(lines: &[String], width: usize) -> Vec<String> {
    logical_lines(&reflow(lines, width))
}

/// RTF control words some programs leave in GAEB 90 texts (`\\par` …).
const RTF_WORDS: [&str; 18] = [
    "par",
    "pard",
    "line",
    "tab",
    "b",
    "i",
    "ul",
    "ulnone",
    "plain",
    "f",
    "fs",
    "cf",
    "cb",
    "highlight",
    "lang",
    "li",
    "fi",
    "ri",
];

/// Whether a text holds RTF remnants: a control word with a number
/// (`\\b0`, `\\fs20`) or one only RTF uses (`\\par`, `\\plain` …). A lone
/// `\\b` or `\\i` in ordinary text („a\\b“) is text, not RTF.
fn looks_like_rtf(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.iter().enumerate().any(|(index, &character)| {
        if character != '\\' {
            return false;
        }
        if chars.get(index + 1) == Some(&'\'') {
            return chars
                .get(index + 2..index + 4)
                .is_some_and(|hex| hex.iter().all(char::is_ascii_hexdigit));
        }
        let word: String = chars[index + 1..]
            .iter()
            .take_while(|c| c.is_ascii_lowercase())
            .collect();
        if !RTF_WORDS.contains(&word.as_str()) {
            return false;
        }
        let after = chars.get(index + 1 + word.len());
        matches!(
            word.as_str(),
            "par" | "pard" | "plain" | "ulnone" | "highlight"
        ) || after.is_some_and(|after| after.is_ascii_digit() || *after == '-')
    })
}

/// Removes RTF control words; `\\par` and `\\line` become line breaks.
/// Returns the text and whether it ended with such a break.
pub(crate) fn clean_line(line: &str) -> (String, bool) {
    if !line.contains('\\') {
        return (line.trim_end().to_string(), false);
    }
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && chars.get(i + 1) == Some(&'\'') {
            let hex: String = chars.iter().skip(i + 2).take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push_str(&crate::datan::decode(
                    &[byte],
                    crate::datan::Encoding::Windows1252,
                ));
                i += 4;
                continue;
            }
        }
        if c == '\\' {
            let word: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_lowercase())
                .collect();
            if RTF_WORDS.contains(&word.as_str()) {
                let mut j = i + 1 + word.len();
                if chars.get(j) == Some(&'-') {
                    j += 1;
                }
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                if chars.get(j) == Some(&' ') {
                    j += 1;
                }
                match word.as_str() {
                    "par" | "line" => out.push('\n'),
                    "tab" => out.push(' '),
                    _ => {}
                }
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    let hard_end = out.trim_end_matches(' ').ends_with('\n');
    let out = out.trim_end().to_string();
    (out, hard_end)
}

/// Text field without RTF control words, breaks as blanks.
pub(crate) fn strip_rtf(text: &str) -> String {
    if !looks_like_rtf(text) {
        return text.trim_end().to_string();
    }
    clean_line(text).0.replace('\n', " ").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::{logical_lines, wrap_lines, LONE_BULLETS};
    use crate::model::{Run, TextBlock};

    fn item(ordered: bool, text: &str) -> TextBlock {
        TextBlock::ListItem {
            ordered,
            level: 0,
            runs: vec![Run {
                text: text.to_string(),
                ..Run::default()
            }],
            align: Default::default(),
        }
    }

    /// Gefunden mit den Eigenschaftstests: ein leerer Listenpunkt wurde in
    /// GAEB 90 ein einzelnes „-“ und beim Lesen mit der nächsten Zeile zu
    /// „- Wort“ verklebt.
    /// GAEB 90 has no lists: Unterpunkte go out two blanks further in per
    /// level, numbers counted per level, and come back with their level –
    /// also when the whole list stands in (the first item is level 0).
    #[test]
    fn list_levels_go_out_as_indent_and_come_back() {
        let item = |ordered, level, text: &str| TextBlock::ListItem {
            ordered,
            level,
            runs: vec![Run::plain(text)],
            align: Default::default(),
        };
        let blocks = vec![
            TextBlock::paragraph(vec![Run::plain("Inhalte:")]),
            item(true, 0, "Automation"),
            item(false, 1, "Parameter"),
            item(false, 1, "Alarme"),
            item(true, 0, "Leitebene"),
            item(true, 1, "eins"),
            item(true, 1, "zwei"),
        ];
        let lines = logical_lines(&blocks);
        assert_eq!(
            lines,
            vec![
                "Inhalte:",
                "1. Automation",
                "  - Parameter",
                "  - Alarme",
                "2. Leitebene",
                "  1. eins",
                "  2. zwei",
            ]
        );
        let back = super::reflow(&lines, 55);
        let levels = |blocks: &[TextBlock]| -> Vec<(bool, u8)> {
            blocks
                .iter()
                .filter_map(|block| match block {
                    TextBlock::ListItem { ordered, level, .. } => Some((*ordered, *level)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(levels(&back), levels(&blocks));
        // Another program indents the whole list: level from the indent
        // relative to the first item.
        let indented: Vec<String> = ["Text:", "   - a", "      - b", "   - c"]
            .iter()
            .map(|line| line.to_string())
            .collect();
        assert_eq!(
            levels(&super::reflow(&indented, 55)),
            vec![(false, 0), (false, 1), (false, 0)]
        );
    }

    #[test]
    fn empty_list_items_become_empty_lines() {
        let blocks = [
            item(false, " "),
            TextBlock::paragraph(vec![Run {
                text: "Wort".into(),
                ..Run::default()
            }]),
            item(true, "a"),
            item(true, ""),
            item(true, "b"),
        ];
        assert_eq!(logical_lines(&blocks), ["", "Wort", "1. a", "", "3. b"]);
    }

    /// Gefunden mit den Eigenschaftstests: ein Langtext „*“ (oder „o“) kam
    /// aus GAEB 90 leer zurück – die Zeile galt als Aufzählungszeichen der
    /// nächsten, die es nicht gab.
    #[test]
    fn wrapping_never_leaves_a_lone_bullet_line() {
        // Gefunden mit dem Rundlauf-Eigenschaftstest (18.11., Saat 7): ein
        // langes Wort „…<!--“ wurde bei 55 Zeichen so geteilt, dass die
        // Folgezeile nur „-“ hielt – beim Lesen ein Aufzählungszeichen.
        let word = format!("{}<!--", "a".repeat(52));
        let lines = wrap_lines(std::slice::from_ref(&word), 55);
        assert!(
            lines
                .iter()
                .all(|line| !LONE_BULLETS.contains(&line.trim())),
            "{lines:?}"
        );
        assert_eq!(lines.concat(), word);
        assert!(lines.iter().all(|line| line.chars().count() <= 55));
        // At a blank: the word before goes along with the dash.
        let text = format!("{} Ende -", "Wort ".repeat(10).trim_end());
        let lines = wrap_lines(&[text], 55);
        assert!(
            lines
                .iter()
                .all(|line| !LONE_BULLETS.contains(&line.trim())),
            "{lines:?}"
        );
        let words = |lines: &[String]| {
            lines
                .join(" ")
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(words(&lines).last().map(String::as_str), Some("-"));
        assert_eq!(words(&lines).len(), 12);
    }

    #[test]
    fn lone_bullets_without_text_stay_text() {
        let lines = |texts: &[&str]| {
            texts
                .iter()
                .map(|text| text.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(logical_lines(&super::reflow(&lines(&["*"]), 55)), ["*"]);
        assert_eq!(
            logical_lines(&super::reflow(&lines(&["o", "", "Text"]), 55)),
            ["o", "", "Text"]
        );
        assert_eq!(
            logical_lines(&super::reflow(&lines(&["*", "-", "Punkt"]), 55)),
            ["*", "- Punkt"]
        );
        // A bullet with its text stays a list item.
        assert_eq!(
            logical_lines(&super::reflow(&lines(&["•", "Punkt"]), 55)),
            ["- Punkt"]
        );
    }

    /// Gefunden mit den Eigenschaftstests: „\\b“, „\\i“ oder „\\f“ in einem
    /// gewöhnlichen Text (ohne weitere RTF-Spuren) verschwanden als RTF-Rest.
    #[test]
    fn backslashes_in_plain_text_stay() {
        let lines = |texts: &[&str]| {
            texts
                .iter()
                .map(|text| text.to_string())
                .collect::<Vec<_>>()
        };
        for text in [r"\bA", r"a\i b", r"Pfad \f", r"C:\fotos\b"] {
            assert_eq!(
                logical_lines(&super::reflow(&lines(&[text]), 55)),
                [text],
                "{text}"
            );
            assert_eq!(super::strip_rtf(text), text);
        }
        // RTF remnants still go (also on other lines of the same text).
        assert_eq!(
            logical_lines(&super::reflow(
                &lines(&[r"\b Fett", r"normal\b0  Ende"]),
                55
            )),
            ["Fett normal Ende"]
        );
        assert_eq!(super::strip_rtf(r"\plain\f1 Titel"), "Titel");
    }
}
