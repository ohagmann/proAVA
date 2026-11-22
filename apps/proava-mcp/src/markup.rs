//! Long texts as simple markup for AI assistants and back:
//! `**fett**`, `*kursiv*`, `__unterstrichen__`, lines with `- ` (bullet) or
//! `1. ` (numbered), two blanks (or a tab) more in front per Unterpunkt
//! level (`  - `, `    1. `), table lines `| a | b |` (a line `|---|:---:|---:|` after
//! the first row makes it the table head and sets the column alignment),
//! images as `[Bild: datei.png, 60 mm]`.
//! Alignment as line prefix `{mittig} `, `{rechts} `, `{blocksatz} ` (also
//! after `- `/`1. `); font and size as `{schrift: Times New Roman; größe: 12}Text{/}`.
//! A backslash keeps the next character as it is; a backslash at the end of
//! a line is a line break inside the paragraph. `{TE 11}` marks the place of
//! the Textergänzung 11 in a position's long text.

use proava_domain::{
    list_indent, list_numbering, Align, Cell, RichText, Run, Table, TextAnchor, TextBlock,
    MAX_LIST_LEVEL,
};

/// List level of a line: two blanks or a tab per level before the marker.
fn indent_level(line: &str) -> u8 {
    let width: usize = line
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .map(|character| if character == '\t' { 2 } else { 1 })
        .sum();
    u8::try_from(width / 2)
        .unwrap_or(u8::MAX)
        .min(MAX_LIST_LEVEL)
}

/// Private-use characters around the marker of a Textergänzung while a
/// text is parsed or rendered; they never stay in a text.
const TE_START: char = '\u{E000}';
const TE_END: char = '\u{E001}';

/// Places of Textergänzungen in a long text: marker (Kennung) and anchor.
pub(crate) type Places = Vec<(String, TextAnchor)>;

const ALIGN_TAGS: [(&str, Align); 4] = [
    ("{links}", Align::Left),
    ("{mittig}", Align::Center),
    ("{rechts}", Align::Right),
    ("{blocksatz}", Align::Justify),
];

fn align_prefix(text: &str) -> (Align, &str) {
    for (tag, align) in ALIGN_TAGS {
        if let Some(rest) = text.strip_prefix(tag) {
            return (align, rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    (Align::Left, text)
}

fn align_tag(align: Align) -> &'static str {
    ALIGN_TAGS
        .iter()
        .find(|(_, known)| *known == align)
        .map(|(tag, _)| *tag)
        .unwrap_or("")
}

#[derive(Default)]
struct PendingTable {
    rows: Vec<Vec<String>>,
    /// Row count before the separator line and the column alignments in it
    /// (a separator before the first row: no header).
    head: Option<(usize, Vec<Align>)>,
    /// Column widths in mm from the separator line (`|--- 15 mm|`).
    widths: Vec<Option<f32>>,
}

impl PendingTable {
    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn finish(self) -> Table {
        let (header_rows, aligns) = self.head.unwrap_or((0, Vec::new()));
        Table {
            rows: self
                .rows
                .into_iter()
                .map(|cells| {
                    cells
                        .into_iter()
                        .enumerate()
                        .map(|(column, cell)| {
                            let (align, rest) = align_prefix(&cell);
                            Cell {
                                runs: inline(rest),
                                align: if align != Align::Left {
                                    align
                                } else {
                                    aligns.get(column).copied().unwrap_or_default()
                                },
                            }
                        })
                        .collect()
                })
                .collect(),
            header_rows,
            column_widths_mm: if self.widths.iter().any(Option::is_some) {
                self.widths
            } else {
                Vec::new()
            },
        }
    }
}

/// Long text of a hint, a description or a title: no Textergänzungen.
pub(crate) fn parse_markup(
    text: &str,
    known_image: impl Fn(&str) -> bool,
) -> Result<RichText, String> {
    let (rich, places) = parse_markup_with_places(text, known_image)?;
    if !places.is_empty() {
        return Err("Textergänzungen ({TE …}) gibt es nur im Langtext von Positionen.".to_string());
    }
    Ok(rich)
}

/// Whether a line ends with a backslash that is not itself escaped.
fn ends_with_break(line: &str) -> bool {
    line.chars()
        .rev()
        .take_while(|character| *character == '\\')
        .count()
        % 2
        == 1
}

/// Long text of a position with the places of its Textergänzungen.
pub(crate) fn parse_markup_with_places(
    text: &str,
    known_image: impl Fn(&str) -> bool,
) -> Result<(RichText, Places), String> {
    let normalized = text.replace("\r\n", "\n");
    // Empty lines at the end stay empty paragraphs (as GAEB texts often end);
    // only one final line break and trailing blanks do not count.
    let trimmed = normalized.trim_end_matches([' ', '\t']);
    let body = trimmed.strip_suffix('\n').unwrap_or(trimmed);
    if body.trim().is_empty() {
        return Ok((RichText::default(), Vec::new()));
    }
    // A backslash at the end of a line: line break inside the paragraph.
    let mut lines: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    for raw in body.split('\n') {
        let line = match pending.take() {
            Some(mut before) => {
                before.push('\n');
                before.push_str(raw);
                before
            }
            None => raw.to_string(),
        };
        // Also in a table row: the break stays in the cell, the row goes on.
        if ends_with_break(&line) {
            pending = Some(line[..line.len() - 1].to_string());
        } else {
            lines.push(line);
        }
    }
    if let Some(line) = pending {
        lines.push(line);
    }

    let mut blocks = Vec::new();
    let mut table = PendingTable::default();
    for line in &lines {
        let line = line.as_str();
        // Only blanks: a paragraph that starts with a line break (`\` alone on
        // the line before) stays a paragraph, even if the next line looks like
        // a list item or a table row.
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.starts_with('|') {
            if is_table_separator(trimmed) {
                if table.head.is_none() {
                    let (aligns, widths) = separator_parts(trimmed);
                    table.head = Some((table.rows.len(), aligns));
                    table.widths = widths;
                }
            } else {
                table.rows.push(table_cells(trimmed));
            }
            continue;
        }
        if !table.is_empty() {
            blocks.push(TextBlock::Table(std::mem::take(&mut table).finish()));
        }

        if let Some(rest) = ["- ", "* ", "• "]
            .iter()
            .find_map(|marker| trimmed.strip_prefix(marker))
        {
            let (align, rest) = align_prefix(rest);
            blocks.push(TextBlock::ListItem {
                ordered: false,
                level: indent_level(line),
                runs: inline(rest),
                align,
            });
        } else if let Some(rest) = numbered_item(trimmed) {
            let (align, rest) = align_prefix(rest);
            blocks.push(TextBlock::ListItem {
                ordered: true,
                level: indent_level(line),
                runs: inline(rest),
                align,
            });
        } else if let Some((file, width_mm)) = image_line(trimmed)? {
            if !known_image(&file) {
                return Err(format!(
                    "Das Bild „{file}“ gibt es im Projekt nicht (Bilder werden in proAVA eingefügt)."
                ));
            }
            blocks.push(TextBlock::Image { file, width_mm });
        } else {
            let (align, rest) = align_prefix(line);
            let mut runs = inline(rest);
            if runs.is_empty() {
                runs.push(Run::plain(""));
            }
            blocks.push(TextBlock::Paragraph { runs, align });
        }
    }
    if !table.is_empty() {
        blocks.push(TextBlock::Table(table.finish()));
    }
    let places = take_places(&mut blocks)?;
    Ok((RichText::new(blocks), places))
}

/// Removes the `{TE …}` marks from the blocks and gives their places: a
/// paragraph with nothing but marks disappears, its Textergänzungen stand on
/// a line of their own before the next block.
fn take_places(blocks: &mut Vec<TextBlock>) -> Result<Places, String> {
    let mut places = Vec::new();
    let mut kept = Vec::with_capacity(blocks.len());
    for mut block in std::mem::take(blocks) {
        let runs = match &mut block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => runs,
            TextBlock::Table(table) => {
                if table
                    .rows
                    .iter()
                    .flatten()
                    .flat_map(|cell| &cell.runs)
                    .any(|run| run.text.contains(TE_START))
                {
                    return Err(
                        "Textergänzungen ({TE …}) gehen nicht in Tabellenzellen.".to_string()
                    );
                }
                kept.push(block);
                continue;
            }
            TextBlock::Image { .. } => {
                kept.push(block);
                continue;
            }
        };
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        if !joined.contains(TE_START) {
            kept.push(block);
            continue;
        }
        let mut found = Vec::new();
        let mut visible = String::new();
        let mut characters = joined.chars();
        while let Some(character) = characters.next() {
            if character == TE_START {
                let marker: String = characters.by_ref().take_while(|ch| *ch != TE_END).collect();
                found.push((marker, visible.chars().count()));
            } else {
                visible.push(character);
            }
        }
        let own_line = visible.trim().is_empty();
        for (marker, offset) in found {
            places.push((
                marker,
                TextAnchor {
                    block: kept.len(),
                    offset: if own_line { 0 } else { offset },
                    own_line,
                },
            ));
        }
        if own_line {
            continue;
        }
        let mut cleaned: Vec<Run> = Vec::new();
        for run in runs.drain(..) {
            let mut text = String::new();
            let mut characters = run.text.chars();
            while let Some(character) = characters.next() {
                if character == TE_START {
                    characters
                        .by_ref()
                        .take_while(|ch| *ch != TE_END)
                        .for_each(drop);
                } else {
                    text.push(character);
                }
            }
            if text.is_empty() {
                continue;
            }
            match cleaned.last_mut() {
                Some(last) if last.same_format(&run) => last.text.push_str(&text),
                _ => cleaned.push(run.with_text(text)),
            }
        }
        *runs = cleaned;
        kept.push(block);
    }
    *blocks = kept;
    Ok(places)
}

/// Long text of a position with `{TE …}` at the places of its
/// Textergänzungen (those without a place are left out).
pub(crate) fn render_markup_with_places(
    text: &RichText,
    places: &[(String, TextAnchor)],
) -> String {
    if places.is_empty() {
        return render_markup(text);
    }
    let mark = |marker: &str| format!("{TE_START}{marker}{TE_END}");
    let original = text.blocks();
    let mut blocks: Vec<TextBlock> = Vec::new();
    for index in 0..=original.len() {
        for (marker, _) in places.iter().filter(|(_, anchor)| {
            anchor.block.min(original.len()) == index
                && (anchor.own_line || index == original.len())
        }) {
            blocks.push(TextBlock::text(mark(marker)));
        }
        let Some(block) = original.get(index) else {
            continue;
        };
        let mut block = block.clone();
        let mut inline: Vec<&(String, TextAnchor)> = places
            .iter()
            .filter(|(_, anchor)| anchor.block == index && !anchor.own_line)
            .collect();
        // From the back, so that earlier offsets stay right.
        inline.sort_by_key(|(_, anchor)| std::cmp::Reverse(anchor.offset));
        if let TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } = &mut block {
            for (marker, anchor) in inline {
                insert_at_offset(runs, anchor.offset, &mark(marker));
            }
        } else {
            for (marker, _) in inline {
                blocks.push(TextBlock::text(mark(marker)));
            }
        }
        blocks.push(block);
    }
    render_markup(&RichText::new(blocks))
        .replace(TE_START, "{TE ")
        .replace(TE_END, "}")
}

fn insert_at_offset(runs: &mut Vec<Run>, offset: usize, text: &str) {
    let mut seen = 0;
    for run in runs.iter_mut() {
        let count = run.text.chars().count();
        if offset <= seen + count {
            let at = run
                .text
                .char_indices()
                .nth(offset - seen)
                .map_or(run.text.len(), |(byte, _)| byte);
            run.text.insert_str(at, text);
            return;
        }
        seen += count;
    }
    match runs.last_mut() {
        Some(run) => run.text.push_str(text),
        None => runs.push(Run::plain(text)),
    }
}

/// One cell of a separator line: `---`, `:---:`, `---:` with an optional
/// width `15 mm` / `15,35 mm` after it. None if it is no separator cell.
fn separator_cell(cell: &str) -> Option<(Align, Option<f32>)> {
    let cell = cell.trim();
    let marks_end = cell
        .find(|character: char| !matches!(character, '-' | ':'))
        .unwrap_or(cell.len());
    let (marks, rest) = cell.split_at(marks_end);
    if marks.trim_matches(':').len() < 3 || !marks.trim_matches(':').chars().all(|ch| ch == '-') {
        return None;
    }
    let align = match (marks.starts_with(':'), marks.ends_with(':')) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Some((align, None));
    }
    let number = rest.strip_suffix("mm")?.trim().replace(',', ".");
    let width: f32 = number
        .parse()
        .ok()
        .filter(|width: &f32| *width > 0.0 && *width < 1000.0)?;
    Some((align, Some(width)))
}

fn separator_parts(line: &str) -> (Vec<Align>, Vec<Option<f32>>) {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| separator_cell(cell).unwrap_or((Align::Left, None)))
        .unzip()
}

/// Width for the separator line: up to two decimals, comma.
fn width_text(width: f32) -> String {
    let text = format!("{width:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', ",")
}

fn with_align(align: Align, text: String) -> String {
    if align == Align::Left {
        text
    } else {
        format!("{} {text}", align_tag(align))
    }
}

pub(crate) fn render_markup(text: &RichText) -> String {
    let mut lines = Vec::new();
    let numbering = list_numbering(text.blocks().iter().map(TextBlock::list_kind));
    for (block, numbering) in text.blocks().iter().zip(numbering) {
        let (indent, number) = numbering
            .map(|(_, level, number)| (list_indent(level), number))
            .unwrap_or_default();
        match block {
            TextBlock::Paragraph { runs, align } => {
                let line = render_runs(runs);
                let line = if *align == Align::Left && starts_like_markup(&line) {
                    format!("\\{line}")
                } else {
                    line
                };
                lines.push(with_align(*align, line));
            }
            TextBlock::ListItem {
                ordered: false,
                runs,
                align,
                ..
            } => lines.push(format!(
                "{indent}- {}",
                with_align(*align, render_runs(runs))
            )),
            TextBlock::ListItem {
                ordered: true,
                runs,
                align,
                ..
            } => {
                lines.push(format!(
                    "{indent}{number}. {}",
                    with_align(*align, render_runs(runs))
                ));
            }
            TextBlock::Table(table) => {
                let columns = table.column_count();
                let widths = table.column_widths_mm.iter().any(Option::is_some);
                let separator = || -> String {
                    let cells: Vec<String> = (0..columns.max(1))
                        .map(
                            |column| match table.column_widths_mm.get(column).copied().flatten() {
                                Some(width) => format!("--- {} mm", width_text(width)),
                                None => "---".to_string(),
                            },
                        )
                        .collect();
                    format!("|{}|", cells.join("|"))
                };
                if widths && table.header_rows == 0 {
                    lines.push(separator());
                }
                for (index, cells) in table.rows.iter().enumerate() {
                    let cells: Vec<String> = cells
                        .iter()
                        .map(|cell| {
                            with_align(cell.align, render_runs(&cell.runs).replace('|', "\\|"))
                        })
                        .collect();
                    lines.push(format!("| {} |", cells.join(" | ")));
                    if table.header_rows > 0 && index + 1 == table.header_rows {
                        lines.push(if widths {
                            separator()
                        } else {
                            format!("|{}", "---|".repeat(columns.max(1)))
                        });
                    }
                }
            }
            TextBlock::Image { file, width_mm } => match width_mm {
                Some(width) => lines.push(format!("[Bild: {file}, {} mm]", trim_float(*width))),
                None => lines.push(format!("[Bild: {file}]")),
            },
        }
    }
    let mut out = lines.join("\n");
    // A final empty paragraph needs one more line break than parse removes.
    if lines.len() > 1 && lines.last().is_some_and(|line| line.trim().is_empty()) {
        out.push('\n');
    }
    out
}

fn trim_float(value: f32) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches(".0").replace('.', ",")
}

/// `{schrift: …; größe: …}` at `index`: the font/size and the length of the tag.
fn font_tag(characters: &[char], index: usize) -> Option<(Option<String>, Option<f32>, usize)> {
    let rest: String = characters[index..].iter().take(120).collect();
    let end = rest.find('}')?;
    let inner = rest[1..end].trim();
    if inner == "/" {
        return Some((None, None, 3));
    }
    let mut font = None;
    let mut size = None;
    for part in inner.split(';') {
        let (key, value) = part.split_once(':')?;
        let value = value.trim();
        match key.trim().to_lowercase().as_str() {
            "schrift" | "font" => font = Some(value.to_string()).filter(|v| !v.is_empty()),
            "größe" | "groesse" | "size" => {
                let number = value.trim_end_matches("pt").trim().replace(',', ".");
                size = Some(number.parse::<f32>().ok().filter(|s| *s > 0.0)?);
            }
            _ => return None,
        }
    }
    (font.is_some() || size.is_some()).then(|| (font, size, rest[..=end].chars().count()))
}

/// `{TE 11}` at `index`: the marker and the length of the tag.
fn complement_tag(characters: &[char], index: usize) -> Option<(String, usize)> {
    let rest: String = characters[index..].iter().take(24).collect();
    let inner = rest.strip_prefix("{TE")?;
    let end = inner.find('}')?;
    let marker = inner[..end].trim();
    if marker.is_empty()
        || !inner[..end].starts_with(char::is_whitespace)
        || !marker.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return None;
    }
    Some((
        marker.to_string(),
        "{TE".chars().count() + inner[..=end].chars().count(),
    ))
}

fn inline(text: &str) -> Vec<Run> {
    let characters: Vec<char> = text.chars().collect();
    let mut runs = Vec::new();
    let mut current = Run::default();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        if character == '\\' && next.is_some() {
            current.text.push(next.unwrap_or_default());
            index += 2;
            continue;
        }
        if character == '{' {
            if let Some((marker, length)) = complement_tag(&characters, index) {
                current.text.push(TE_START);
                current.text.push_str(&marker);
                current.text.push(TE_END);
                index += length;
                continue;
            }
            if let Some((font, size, length)) = font_tag(&characters, index) {
                let flushed = std::mem::take(&mut current);
                current = flushed.format();
                if length == 3 {
                    current.font = None;
                    current.size_pt = None;
                } else {
                    if font.is_some() {
                        current.font = font;
                    }
                    if size.is_some() {
                        current.size_pt = size;
                    }
                }
                if !flushed.text.is_empty() {
                    runs.push(flushed);
                }
                index += length;
                continue;
            }
        }
        let toggle = match (character, next) {
            ('*', Some('*')) => Some(2),
            ('_', Some('_')) => Some(3),
            ('*', _) => Some(1),
            _ => None,
        };
        match toggle {
            Some(kind) => {
                let flushed = std::mem::take(&mut current);
                current = flushed.format();
                match kind {
                    1 => current.italic = !current.italic,
                    2 => current.bold = !current.bold,
                    _ => current.underline = !current.underline,
                }
                if !flushed.text.is_empty() {
                    runs.push(flushed);
                }
                index += if kind == 1 { 1 } else { 2 };
            }
            None => {
                current.text.push(character);
                index += 1;
            }
        }
    }
    if !current.text.is_empty() {
        runs.push(current);
    }
    runs
}

fn render_runs(runs: &[Run]) -> String {
    let mut out = String::new();
    for run in runs {
        let mut open = String::new();
        if run.bold {
            open.push_str("**");
        }
        if run.italic {
            open.push('*');
        }
        if run.underline {
            open.push_str("__");
        }
        let close: String = open.chars().rev().collect();
        let mut tag = Vec::new();
        if let Some(font) = &run.font {
            tag.push(format!("schrift: {}", font.replace(['}', ';'], " ")));
        }
        if let Some(size) = run.size_pt {
            tag.push(format!("größe: {}", trim_float(size)));
        }
        if !tag.is_empty() {
            out.push_str(&format!("{{{}}}", tag.join("; ")));
        }
        out.push_str(&open);
        out.push_str(&escape(&run.text));
        out.push_str(&close);
        if !tag.is_empty() {
            out.push_str("{/}");
        }
    }
    out
}

fn escape(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (index, character) in characters.iter().enumerate() {
        if *character == '\n' {
            // Line break inside the paragraph: backslash at the end of the line.
            out.push_str("\\\n");
            continue;
        }
        let doubled_underscore = *character == '_' && characters.get(index + 1) == Some(&'_');
        if matches!(character, '\\' | '*' | '{') || doubled_underscore {
            out.push('\\');
        }
        out.push(*character);
    }
    out
}

/// Whether a rendered paragraph would be read as other markup. A `{` at the
/// start is always a font tag here (escape() masks a literal one), so it
/// needs no backslash – one would turn the tag into text.
fn starts_like_markup(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["- ", "* ", "• ", "|", "[Bild:"]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
        || numbered_item(trimmed).is_some()
}

fn numbered_item(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = &line[digits..];
    rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))
}

fn image_line(line: &str) -> Result<Option<(String, Option<f32>)>, String> {
    let Some(inner) = line
        .trim_end()
        .strip_prefix("[Bild:")
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Ok(None);
    };
    let (file, width) = match inner.split_once(',') {
        Some((file, width)) => (file.trim(), Some(width.trim())),
        None => (inner.trim(), None),
    };
    let width_mm = match width {
        Some(width) => {
            let number = width.trim_end_matches("mm").trim().replace(',', ".");
            Some(
                number
                    .parse::<f32>()
                    .map_err(|_| format!("Bildbreite „{width}“ ist keine Zahl (z. B. „60 mm“)."))?,
            )
        }
        None => None,
    };
    Ok(Some((file.to_string(), width_mm)))
}

fn is_table_separator(line: &str) -> bool {
    let inner = line.trim().trim_matches('|');
    !inner.trim().is_empty() && inner.split('|').all(|cell| separator_cell(cell).is_some())
}

fn table_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut characters = line.trim().chars();
    while let Some(character) = characters.next() {
        match character {
            // Only the pipe is a table matter; other escapes stay for inline().
            '\\' => match characters.next() {
                Some('|') => current.push('|'),
                Some(next) => {
                    current.push('\\');
                    current.push(next);
                }
                None => current.push('\\'),
            },
            '|' => cells.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    cells.push(current);
    if cells.first().is_some_and(|cell| cell.trim().is_empty()) {
        cells.remove(0);
    }
    if cells.last().is_some_and(|cell| cell.trim().is_empty()) {
        cells.pop();
    }
    cells
        .into_iter()
        .map(|cell| cell.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use proava_domain::{Align, RichText, TextBlock};

    use super::{parse_markup, render_markup};

    fn parse(text: &str) -> RichText {
        parse_markup(text, |file| file == "plan.png").unwrap()
    }

    /// Unterpunkte: two blanks (or a tab) more per level before „- “ or
    /// „1. “; numbers count per level; round trip.
    #[test]
    fn indented_list_lines_are_unterpunkte() {
        let source = "Inhalte:\n1. Automation\n  - Parameter\n  - Alarme\n2. Leitebene\n  1. eins\n\t\t- ganz tief\n  2. zwei";
        let text = parse(source);
        let kinds: Vec<_> = text.blocks().iter().map(TextBlock::list_kind).collect();
        assert_eq!(
            kinds,
            vec![
                None,
                Some((true, 0)),
                Some((false, 1)),
                Some((false, 1)),
                Some((true, 0)),
                Some((true, 1)),
                Some((false, 2)),
                Some((true, 1)),
            ]
        );
        let rendered = render_markup(&text);
        assert_eq!(
            rendered,
            "Inhalte:\n1. Automation\n  - Parameter\n  - Alarme\n2. Leitebene\n  1. eins\n    - ganz tief\n  2. zwei"
        );
        assert_eq!(parse(&rendered), text);
        // A paragraph indented by blanks stays a paragraph with its blanks.
        let paragraph = parse("  eingerückt");
        assert!(
            matches!(&paragraph.blocks()[0], TextBlock::Paragraph { runs, .. } if runs[0].text == "  eingerückt")
        );
    }

    #[test]
    fn formatting_lists_tables_and_images_round_trip() {
        let source = "Liefern und montieren:\n- **DDC-Station** mit *Bedienteil*\n- __Netzteil__ 24 V\n1. erstens\n2. zweitens\n| Typ | Anzahl |\n|---|---|\n| AI | 8 |\n[Bild: plan.png, 60 mm]\nEnde";
        let text = parse(source);
        assert_eq!(text.blocks().len(), 8);
        assert!(
            matches!(&text.blocks()[1], TextBlock::ListItem { ordered: false, runs, .. } if runs[0].bold && runs[0].text == "DDC-Station")
        );
        assert!(
            matches!(&text.blocks()[5], TextBlock::Table(table) if table.texts() == vec![vec!["Typ".to_string(), "Anzahl".into()], vec!["AI".into(), "8".into()]] && table.header_rows == 1)
        );
        assert!(
            matches!(&text.blocks()[6], TextBlock::Image { file, width_mm: Some(width) } if file == "plan.png" && *width == 60.0)
        );
        let rendered = render_markup(&text);
        assert_eq!(parse(&rendered), text);
        assert!(rendered.contains("- **DDC-Station** mit *Bedienteil*"));
    }

    #[test]
    fn special_characters_survive() {
        let text = RichText::plain("- kein Punkt\n5 * 3 = a__b | c\\d");
        let rendered = render_markup(&text);
        assert_eq!(parse(&rendered), text);
    }

    #[test]
    fn alignment_fonts_sizes_and_table_heads_round_trip() {
        let source = "{mittig} {schrift: Times New Roman; größe: 14}**Titel**{/}\n\
                      {blocksatz} Text mit {größe: 8}kleiner{/} Schrift\n\
                      - {rechts} rechts\n\
                      | Typ | Wert |\n|---|---:|\n| AI | 8 |\n\
                      \\{mittig} bleibt Text";
        let text = parse(source);
        let TextBlock::Paragraph { runs, align } = &text.blocks()[0] else {
            panic!("{text:?}")
        };
        assert_eq!(*align, Align::Center);
        assert_eq!(runs[0].font.as_deref(), Some("Times New Roman"));
        assert_eq!(runs[0].size_pt, Some(14.0));
        assert!(runs[0].bold);
        let TextBlock::Paragraph { runs, align } = &text.blocks()[1] else {
            panic!()
        };
        assert_eq!(*align, Align::Justify);
        assert_eq!(runs[1].size_pt, Some(8.0));
        assert_eq!(runs[2].size_pt, None);
        assert!(matches!(
            text.blocks()[2],
            TextBlock::ListItem {
                align: Align::Right,
                ..
            }
        ));
        let TextBlock::Table(table) = &text.blocks()[3] else {
            panic!()
        };
        assert_eq!(table.header_rows, 1);
        assert_eq!(table.rows[1][1].align, Align::Right);
        assert_eq!(text.blocks()[4], TextBlock::text("{mittig} bleibt Text"));
        let rendered = render_markup(&text);
        assert_eq!(parse(&rendered), text, "{rendered}");
    }

    #[test]
    fn empty_paragraphs_at_the_end_stay() {
        let text = parse("Erster Absatz\n\nZweiter\n\n");
        assert_eq!(text.blocks().len(), 4, "{text:?}");
        assert_eq!(text.blocks()[3], TextBlock::text(""));
        assert_eq!(parse(&render_markup(&text)), text);
        // one final line break and trailing blanks do not count
        assert_eq!(parse("Text\n").blocks().len(), 1);
        assert_eq!(parse("Text  \t").blocks().len(), 1);
        assert_eq!(parse("Text\r\n").blocks().len(), 1);
        assert!(parse("\n").is_empty());
        let list = parse("- Punkt\n\n");
        assert_eq!(list.blocks().len(), 2);
        assert_eq!(parse(&render_markup(&list)), list);
    }

    #[test]
    fn line_breaks_inside_a_paragraph() {
        let text = parse("Erste Zeile\\\nzweite Zeile\n- Punkt\\\nweiter\nPfad C:\\\\\nneu");
        assert_eq!(text.blocks().len(), 4, "{text:?}");
        assert_eq!(
            text.blocks()[0],
            TextBlock::text("Erste Zeile\nzweite Zeile")
        );
        assert!(
            matches!(&text.blocks()[1], TextBlock::ListItem { runs, .. } if runs[0].text == "Punkt\nweiter")
        );
        assert_eq!(
            text.blocks()[2],
            TextBlock::text("Pfad C:\\"),
            "doppelter Backslash bleibt Text"
        );
        assert_eq!(parse(&render_markup(&text)), text);
    }

    #[test]
    fn a_paragraph_starting_with_a_line_break_stays_a_paragraph() {
        // Declarations like "dass:\n\n- er …" with a literal dash after a line
        // break at the start of the paragraph: no list item, the break stays.
        let text =
            parse("dass:\\\n\\\n- erstens,\n\\\n- zweitens\n\\\n| keine Tabelle |\n  - Punkt");
        assert_eq!(text.blocks().len(), 4, "{text:?}");
        assert_eq!(text.blocks()[0], TextBlock::text("dass:\n\n- erstens,"));
        assert_eq!(text.blocks()[1], TextBlock::text("\n- zweitens"));
        assert_eq!(text.blocks()[2], TextBlock::text("\n| keine Tabelle |"));
        assert!(
            matches!(&text.blocks()[3], TextBlock::ListItem { ordered: false, runs, .. } if runs[0].text == "Punkt"),
            "eingerückter Punkt bleibt ein Listenpunkt"
        );
        assert_eq!(parse(&render_markup(&text)), text);
    }

    #[test]
    fn complement_places_round_trip() {
        use super::{parse_markup_with_places, render_markup_with_places};
        let source = "Spannung 400 V AC,\n{TE 11}\nHz, Strom {TE 12}A,\nEnde";
        let (text, places) = parse_markup_with_places(source, |_| false).unwrap();
        assert_eq!(text.blocks().len(), 3);
        assert_eq!(places[0].0, "11");
        assert!(places[0].1.own_line && places[0].1.block == 1);
        assert_eq!(
            (places[1].1.block, places[1].1.offset, places[1].1.own_line),
            (1, 10, false)
        );
        assert_eq!(text.blocks()[1], TextBlock::text("Hz, Strom A,"));
        let rendered = render_markup_with_places(&text, &places);
        assert_eq!(rendered, source);
        assert!(
            parse_markup("Text {TE 1}", |_| false).is_err(),
            "nur in Positionen"
        );
        assert!(parse_markup_with_places("| a {TE 1} |", |_| false).is_err());
        assert_eq!(parse("\\{TE 1} bleibt"), RichText::plain("{TE 1} bleibt"));
    }

    #[test]
    fn a_paragraph_starting_with_a_font_tag_round_trips() {
        // Echtes LV: Absätze in 9 pt beginnen mit dem Tag; position_lesen
        // darf es nicht maskieren, sonst wird es beim Zurückschreiben Text.
        let text = parse(
            "{größe: 9}Klein am Anfang{/}\n\\{mittig} wörtlich\n{rechts} {größe: 9}rechts{/}",
        );
        let rendered = render_markup(&text);
        assert!(rendered.starts_with("{größe: 9}Klein"), "{rendered}");
        assert_eq!(parse(&rendered), text);
        let TextBlock::Paragraph { runs, .. } = &text.blocks()[1] else {
            panic!()
        };
        assert_eq!(runs[0].text, "{mittig} wörtlich");
    }

    #[test]
    fn table_cells_keep_line_breaks_and_special_characters() {
        // Zelle mit Zeilenumbruch (GAEB <br/> bzw. zwei Absätze in einer Zelle) und
        // Zeichen, die maskiert werden: beides muss den Rundlauf überstehen.
        let TextBlock::Table(mut table) = parse("| a | b |").blocks()[0].clone() else {
            panic!()
        };
        table.rows[0][0].runs = vec![proava_domain::Run::plain("4 Digitale\nAusgänge")];
        table.rows[0][1].runs = vec![proava_domain::Run::plain("5 * 3 * 2 {mittig} a__b \\ c")];
        let text = RichText::new(vec![TextBlock::Table(table)]);
        let rendered = render_markup(&text);
        assert_eq!(parse(&rendered), text, "{rendered}");
        // von Hand geschrieben: Umbruch mit \ am Zeilenende auch in der Zelle
        let typed = parse("| erste\\\nzweite Zeile | b |\n| c | d |");
        let TextBlock::Table(table) = &typed.blocks()[0] else {
            panic!("{typed:?}")
        };
        assert_eq!(table.rows.len(), 2, "{typed:?}");
        assert_eq!(table.rows[0][0].runs[0].text, "erste\nzweite Zeile");
        assert_eq!(typed.blocks().len(), 1);
    }

    #[test]
    fn table_column_widths_in_the_separator_line() {
        // Trennzeile vor der ersten Zeile: Breiten ohne Kopfzeile
        let text = parse("| Anzahl | Ein- / Ausgangsart |\n| {mittig} 3 | Digitale Ausgänge |");
        assert!(
            matches!(&text.blocks()[0], TextBlock::Table(table) if table.column_widths_mm.is_empty())
        );
        let text = parse("|--- 15,35 mm |:---: 48.95mm|\n| Anzahl | Art |\n| {mittig} 3 | DA |");
        let TextBlock::Table(table) = &text.blocks()[0] else {
            panic!("{text:?}")
        };
        assert_eq!(table.header_rows, 0);
        assert_eq!(table.column_widths_mm, vec![Some(15.35), Some(48.95)]);
        assert_eq!(table.rows[1][0].align, Align::Center);
        assert_eq!(
            table.rows[0][1].align,
            Align::Center,
            "Ausrichtung der Spalte"
        );
        let rendered = render_markup(&text);
        assert!(
            rendered.starts_with("|--- 15,35 mm|--- 48,95 mm|"),
            "{rendered}"
        );
        assert_eq!(parse(&rendered), text);
        // mit Kopfzeile
        let text = parse("| Typ | Anzahl |\n|--- 20 mm|---:|\n| AI | 8 |");
        let TextBlock::Table(table) = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(
            (table.header_rows, table.column_widths_mm.clone()),
            (1, vec![Some(20.0), None])
        );
        assert_eq!(parse(&render_markup(&text)), text);
        // eine Datenzeile mit Strichen und mm ist keine Trennzeile
        let text = parse("| - | 1,5 mm |\n| a | b |");
        let TextBlock::Table(table) = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(table.rows.len(), 2);
    }

    #[test]
    fn unknown_images_are_refused() {
        assert!(parse_markup("[Bild: fehlt.png]", |_| false).is_err());
        assert!(parse_markup("", |_| false).unwrap().is_empty());
    }
}
