use std::sync::Arc;

/// Formatted long text of a position, hint or performance description:
/// paragraphs, bullet points, tables and images. Images live in the project
/// (see [`crate::ImageAsset`]) and are referenced by their file name.
///
/// Character formatting sits on [`Run`]s (bold, italic, underline, font
/// family, size), alignment on paragraphs, list items and table cells.
///
/// The blocks are shared between copies (`Arc`, copied on the first change):
/// undo snapshots and caches of an LV keep one long text, not one per copy.
#[derive(Debug, Clone, Default)]
pub struct RichText {
    blocks: Arc<Vec<TextBlock>>,
}

impl PartialEq for RichText {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.blocks, &other.blocks) || self.blocks == other.blocks
    }
}

/// Standard font size in points when a run gives none.
pub const DEFAULT_SIZE_PT: f32 = 10.0;

/// Deepest list level (0 = outermost): eight levels, as deep as the texts
/// of other programs nest.
pub const MAX_LIST_LEVEL: u8 = 7;

/// Marker of a list item as shown and printed: „•“ on every level (as the
/// printouts of other AVA programs, Anwender 18.11.), numbers „1.“, „a)“,
/// „i.“ by level (`number` counts from 1).
pub fn list_marker(ordered: bool, level: u8, number: usize) -> String {
    if !ordered {
        return "•".to_string();
    }
    let number = number.max(1);
    match level % 3 {
        0 => format!("{number}."),
        1 => format!("{})", letters(number)),
        _ => format!("{}.", roman(number)),
    }
}

/// a, b … z, aa, ab …
fn letters(mut number: usize) -> String {
    let mut out = Vec::new();
    while number > 0 {
        number -= 1;
        out.push(char::from(b'a' + (number % 26) as u8));
        number /= 26;
    }
    out.iter().rev().collect()
}

/// i, ii, iii, iv … (lower case).
fn roman(mut number: usize) -> String {
    const PARTS: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, part) in PARTS {
        while number >= value {
            out.push_str(part);
            number -= value;
        }
    }
    out
}

/// Markers of a sequence of blocks (`Some((ordered, level))` for a list
/// item, `None` for anything else): numbers count per level and start
/// again after a block that is no list item, after an item on a higher
/// level (for the deeper ones) and when bullets and numbers change on a
/// level – as nested HTML lists number.
pub fn list_markers(items: impl IntoIterator<Item = Option<(bool, u8)>>) -> Vec<Option<String>> {
    list_numbering(items)
        .into_iter()
        .map(|item| item.map(|(ordered, level, number)| list_marker(ordered, level, number)))
        .collect()
}

/// Like [`list_markers`]: ordered flag, level (capped) and number (0 for
/// bullets) of each list item.
pub fn list_numbering(
    items: impl IntoIterator<Item = Option<(bool, u8)>>,
) -> Vec<Option<(bool, u8, usize)>> {
    let mut counters = [0usize; MAX_LIST_LEVEL as usize + 1];
    items
        .into_iter()
        .map(|item| {
            let Some((ordered, level)) = item else {
                counters = Default::default();
                return None;
            };
            let level = level.min(MAX_LIST_LEVEL);
            let at = usize::from(level);
            for deeper in counters.iter_mut().skip(at + 1) {
                *deeper = 0;
            }
            counters[at] = if ordered { counters[at] + 1 } else { 0 };
            Some((ordered, level, counters[at]))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextBlock {
    Paragraph {
        runs: Vec<Run>,
        align: Align,
    },
    /// Bullet point (`ordered`: numbered). `level` 0 is the outermost list,
    /// 1 an Unterpunkt of the item before … up to [`MAX_LIST_LEVEL`]: other
    /// programs nest lists (GAEB `<ul>` in `<li>`), proAVA keeps them flat
    /// with the depth.
    ListItem {
        ordered: bool,
        level: u8,
        runs: Vec<Run>,
        align: Align,
    },
    Table(Table),
    /// Image shown in the PDF; never exported to GAEB.
    Image {
        file: String,
        width_mm: Option<f32>,
    },
}

/// Two blanks per list level – the indent of plain-text list lines.
pub fn list_indent(level: u8) -> String {
    "  ".repeat(usize::from(level.min(MAX_LIST_LEVEL)))
}

/// Horizontal alignment of a paragraph, list item or table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// Blocksatz.
    Justify,
}

impl Align {
    pub const ALL: [Align; 4] = [Align::Left, Align::Center, Align::Right, Align::Justify];

    /// Name as in CSS/GAEB (`text-align`).
    pub fn css(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }

    pub fn from_css(value: &str) -> Option<Self> {
        Some(match value.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => Align::Left,
            "center" | "middle" => Align::Center,
            "right" | "end" => Align::Right,
            "justify" => Align::Justify,
            _ => return None,
        })
    }
}

/// Table with formatted cells. Rows may have different lengths (imported
/// texts); [`Table::column_count`] is the widest row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub rows: Vec<Vec<Cell>>,
    /// Number of leading rows that form the table head (printed bold on a
    /// tinted background, repeated on page breaks).
    pub header_rows: usize,
    /// Width per column in mm; `None` (or a missing entry) = automatic.
    pub column_widths_mm: Vec<Option<f32>>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    pub runs: Vec<Run>,
    pub align: Align,
}

/// A piece of text with one formatting.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Font family as written (e.g. "Arial", "Times New Roman"); `None` = standard.
    pub font: Option<String>,
    /// Size in points; `None` = standard ([`DEFAULT_SIZE_PT`]).
    pub size_pt: Option<f32>,
}

/// The three font families proAVA shows and prints (Liberation fonts,
/// metric-compatible with Arial, Times New Roman and Courier New).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontClass {
    #[default]
    Sans,
    Serif,
    Mono,
}

impl FontClass {
    pub const ALL: [FontClass; 3] = [FontClass::Sans, FontClass::Serif, FontClass::Mono];

    /// Font family that is shown and printed.
    pub fn liberation_name(self) -> &'static str {
        match self {
            FontClass::Sans => "Liberation Sans",
            FontClass::Serif => "Liberation Serif",
            FontClass::Mono => "Liberation Mono",
        }
    }

    /// Common name written into the text when the user chooses the family
    /// (other GAEB programs know these); `None` for the standard family.
    pub fn stored_name(self) -> Option<&'static str> {
        match self {
            FontClass::Sans => None,
            FontClass::Serif => Some("Times New Roman"),
            FontClass::Mono => Some("Courier New"),
        }
    }

    /// Label for menus.
    pub fn label(self) -> &'static str {
        match self {
            FontClass::Sans => "Arial (Liberation Sans)",
            FontClass::Serif => "Times (Liberation Serif)",
            FontClass::Mono => "Courier (Liberation Mono)",
        }
    }
}

/// Which family stands in for a font name: Times-like names → Serif,
/// Courier/monospace → Mono, everything else (Arial, Helvetica, unknown) → Sans.
pub fn font_class(name: Option<&str>) -> FontClass {
    let Some(name) = name else {
        return FontClass::Sans;
    };
    let name = name.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
    let first = name.split(',').next().unwrap_or("").trim();
    let mono = [
        "courier",
        "mono",
        "consolas",
        "lucida console",
        "fixedsys",
        "typewriter",
    ];
    let serif = [
        "times",
        "roman",
        "georgia",
        "garamond",
        "cambria",
        "palatino",
        "book antiqua",
        "bookman",
        "century",
        "baskerville",
        "minion",
    ];
    if mono.iter().any(|key| first.contains(key)) {
        FontClass::Mono
    } else if first == "serif"
        || (first.contains("serif") && !first.contains("sans"))
        || serif.iter().any(|key| first.contains(key))
    {
        FontClass::Serif
    } else {
        FontClass::Sans
    }
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: true,
            ..Self::default()
        }
    }

    /// Same formatting (text ignored).
    pub fn same_format(&self, other: &Run) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.underline == other.underline
            && self.font == other.font
            && self.size_pt == other.size_pt
    }

    /// The formatting without the text.
    pub fn format(&self) -> Run {
        Run {
            text: String::new(),
            ..self.clone()
        }
    }

    pub fn with_text(&self, text: impl Into<String>) -> Run {
        Run {
            text: text.into(),
            ..self.format()
        }
    }

    pub fn is_plain_format(&self) -> bool {
        self.same_format(&Run::default())
    }

    pub fn size_or_default(&self) -> f32 {
        self.size_pt.unwrap_or(DEFAULT_SIZE_PT)
    }

    pub fn font_class(&self) -> FontClass {
        font_class(self.font.as_deref())
    }
}

/// Drops empty runs and joins neighbours with the same formatting.
pub fn normalize_runs(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.same_format(&run) => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    out
}

/// Text of the runs joined.
/// Whether `character` is a control character that does not belong in a
/// text: everything below U+0020 except tab and line break, DEL, the C1
/// controls and the non-characters U+FFFE/U+FFFF. Pasted texts (Word, PDF)
/// bring them along; XML 1.0 cannot hold most of them.
pub fn is_stray_control(character: char) -> bool {
    matches!(character, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{fffe}' | '\u{ffff}')
}

/// `text` without stray control characters ([`is_stray_control`]); a
/// carriage return (alone or before a line break) becomes one line break.
pub fn strip_control_chars(text: &str) -> std::borrow::Cow<'_, str> {
    if !text
        .chars()
        .any(|character| character == '\r' || is_stray_control(character))
    {
        return std::borrow::Cow::Borrowed(text);
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    std::borrow::Cow::Owned(
        normalized
            .chars()
            .filter(|character| !is_stray_control(*character))
            .collect(),
    )
}

pub fn runs_text(runs: &[Run]) -> String {
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// Applies `change` to the characters `start..end` (char indices) of the
/// runs; the result is normalized.
pub fn apply_format(runs: &[Run], start: usize, end: usize, change: impl Fn(&mut Run)) -> Vec<Run> {
    let (start, end) = (start.min(end), start.max(end));
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut offset = 0;
    for run in runs {
        let length = run.text.chars().count();
        let (from, to) = (
            start.clamp(offset, offset + length) - offset,
            end.clamp(offset, offset + length) - offset,
        );
        let chars: Vec<char> = run.text.chars().collect();
        let piece = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
        out.push(run.with_text(piece(0, from)));
        let mut middle = run.with_text(piece(from, to));
        change(&mut middle);
        out.push(middle);
        out.push(run.with_text(piece(to, length)));
        offset += length;
    }
    normalize_runs(out)
}

#[derive(Debug, Clone, Copy)]
struct Matching {
    case_sensitive: bool,
    whole_word: bool,
}

impl Matching {
    fn same(self, a: char, b: char) -> bool {
        a == b || (!self.case_sensitive && a.to_lowercase().eq(b.to_lowercase()))
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Replaces in one run list (`replace` = `None`: only counts); see
/// [`RichText::replace_matches`].
fn replace_in_runs(
    runs: &mut Vec<Run>,
    find: &str,
    replace: Option<&str>,
    options: Matching,
) -> usize {
    // Characters with the index of the run they belong to.
    let chars: Vec<(char, usize)> = runs
        .iter()
        .enumerate()
        .flat_map(|(index, run)| run.text.chars().map(move |ch| (ch, index)))
        .collect();
    let needle: Vec<char> = find.chars().collect();
    if needle.is_empty() || chars.len() < needle.len() {
        return 0;
    }
    let mut out: Vec<Run> = Vec::new();
    let push = |out: &mut Vec<Run>, format: &Run, text: &str| match out.last_mut() {
        Some(last) if last.same_format(format) => last.text.push_str(text),
        _ => out.push(format.with_text(text)),
    };
    let mut count = 0;
    let mut position = 0;
    while position < chars.len() {
        let end = position + needle.len();
        let hit = end <= chars.len()
            && chars[position..end]
                .iter()
                .zip(&needle)
                .all(|((ch, _), wanted)| options.same(*ch, *wanted))
            && (!options.whole_word
                || ((position == 0 || !is_word_char(chars[position - 1].0))
                    && (end == chars.len() || !is_word_char(chars[end].0))));
        if hit {
            count += 1;
            if let Some(replace) = replace {
                let format = runs[chars[position].1].format();
                if !replace.is_empty() {
                    push(&mut out, &format, replace);
                }
            }
            position = end;
        } else {
            let (ch, run) = chars[position];
            if replace.is_some() {
                push(&mut out, &runs[run].format(), ch.encode_utf8(&mut [0; 4]));
            }
            position += 1;
        }
    }
    if count > 0 && replace.is_some() {
        *runs = normalize_runs(out);
    }
    count
}

impl Cell {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            runs: if text.is_empty() {
                Vec::new()
            } else {
                vec![Run::plain(text)]
            },
            align: Align::Left,
        }
    }

    pub fn text(&self) -> String {
        runs_text(&self.runs)
    }
}

impl Table {
    /// Table of plain cell texts without head row.
    pub fn from_texts(rows: Vec<Vec<String>>) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|cells| cells.into_iter().map(Cell::plain).collect())
                .collect(),
            header_rows: 0,
            column_widths_mm: Vec::new(),
        }
    }

    /// Empty table with `rows` × `columns` cells.
    pub fn empty(rows: usize, columns: usize) -> Self {
        Self {
            rows: vec![vec![Cell::default(); columns]; rows],
            header_rows: 0,
            column_widths_mm: Vec::new(),
        }
    }

    pub fn column_count(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    pub fn column_width(&self, column: usize) -> Option<f32> {
        self.column_widths_mm.get(column).copied().flatten()
    }

    /// Cell texts, one row per entry.
    pub fn texts(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|cells| cells.iter().map(Cell::text).collect())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.rows
            .iter()
            .flatten()
            .all(|cell| cell.text().trim().is_empty())
    }
}

impl TextBlock {
    /// Left-aligned paragraph.
    pub fn paragraph(runs: Vec<Run>) -> Self {
        TextBlock::Paragraph {
            runs,
            align: Align::Left,
        }
    }

    /// Left-aligned paragraph of plain text.
    pub fn text(text: impl Into<String>) -> Self {
        Self::paragraph(vec![Run::plain(text)])
    }

    pub fn list_item(ordered: bool, runs: Vec<Run>) -> Self {
        Self::list_item_at(ordered, 0, runs)
    }

    /// Left-aligned list item on `level` (capped at [`MAX_LIST_LEVEL`]).
    pub fn list_item_at(ordered: bool, level: u8, runs: Vec<Run>) -> Self {
        TextBlock::ListItem {
            ordered,
            level: level.min(MAX_LIST_LEVEL),
            runs,
            align: Align::Left,
        }
    }

    /// Ordered flag and level of a list item.
    pub fn list_kind(&self) -> Option<(bool, u8)> {
        match self {
            TextBlock::ListItem { ordered, level, .. } => Some((*ordered, *level)),
            _ => None,
        }
    }

    /// Runs of a paragraph or list item.
    pub fn runs(&self) -> Option<&[Run]> {
        match self {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => Some(runs),
            _ => None,
        }
    }
}

impl RichText {
    pub fn new(blocks: Vec<TextBlock>) -> Self {
        Self {
            blocks: Arc::new(blocks),
        }
    }

    /// One paragraph per line.
    pub fn plain(text: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        Self::new(text.split('\n').map(TextBlock::text).collect())
    }

    pub fn blocks(&self) -> &[TextBlock] {
        &self.blocks
    }

    /// The blocks for changing; unshares them from other copies first.
    pub fn blocks_mut(&mut self) -> &mut Vec<TextBlock> {
        Arc::make_mut(&mut self.blocks)
    }

    pub fn push(&mut self, block: TextBlock) {
        self.blocks_mut().push(block);
    }

    /// Markers of the blocks („•“, „–“, „1.“, „a)“ …; `None` for blocks
    /// that are no list item), see [`list_markers`].
    pub fn list_markers(&self) -> Vec<Option<String>> {
        list_markers(self.blocks.iter().map(TextBlock::list_kind))
    }

    /// Lines for plain-text exchange (CSV export): like
    /// [`RichText::to_plain`], but numbered items with their number („1. “
    /// on every level, counted per level).
    pub fn to_lines(&self) -> String {
        let numbering = list_numbering(self.blocks.iter().map(TextBlock::list_kind));
        let mut lines = Vec::new();
        for (block, numbering) in self.blocks.iter().zip(numbering) {
            match (block, numbering) {
                (TextBlock::ListItem { runs, .. }, Some((ordered, level, number))) => {
                    let marker = if ordered {
                        format!("{number}. ")
                    } else {
                        "- ".to_string()
                    };
                    lines.push(format!("{}{marker}{}", list_indent(level), runs_text(runs)));
                }
                (TextBlock::Paragraph { runs, .. }, _) => lines.push(runs_text(runs)),
                (TextBlock::Table(table), _) => {
                    for cells in table.texts() {
                        lines.push(cells.join("\t"));
                    }
                }
                _ => {}
            }
        }
        lines.join("\n")
    }

    /// Text without formatting and without images, one line per block,
    /// table cells separated by tabs; list items as „- “, two blanks more
    /// per level.
    pub fn to_plain(&self) -> String {
        let mut lines = Vec::new();
        for block in self.blocks.iter() {
            match block {
                TextBlock::Paragraph { runs, .. } => lines.push(runs_text(runs)),
                TextBlock::ListItem { runs, level, .. } => {
                    lines.push(format!("{}- {}", list_indent(*level), runs_text(runs)))
                }
                TextBlock::Table(table) => {
                    for cells in table.texts() {
                        lines.push(cells.join("\t"));
                    }
                }
                TextBlock::Image { .. } => {}
            }
        }
        lines.join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|block| match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                runs.iter().all(|run| run.text.trim().is_empty())
            }
            TextBlock::Table(table) => table.is_empty(),
            TextBlock::Image { .. } => false,
        })
    }

    /// Whether any run, paragraph or cell carries formatting beyond plain
    /// left-aligned text (tables and lists count as formatting).
    pub fn has_formatting(&self) -> bool {
        self.blocks.iter().any(|block| match block {
            TextBlock::Paragraph { runs, align } => {
                *align != Align::Left || runs.iter().any(|run| !run.is_plain_format())
            }
            _ => true,
        })
    }

    /// File names of all images in this text.
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.blocks.iter().filter_map(|block| match block {
            TextBlock::Image { file, .. } => Some(file.as_str()),
            _ => None,
        })
    }

    pub fn has_images(&self) -> bool {
        self.images().next().is_some()
    }

    /// Removes stray control characters from every run (see
    /// [`strip_control_chars`]); returns whether anything changed.
    pub fn strip_control_chars(&mut self) -> bool {
        let dirty = |runs: &[Run]| {
            runs.iter().any(|run| {
                run.text
                    .chars()
                    .any(|character| character == '\r' || is_stray_control(character))
            })
        };
        let any = self.blocks.iter().any(|block| match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => dirty(runs),
            TextBlock::Table(table) => table.rows.iter().flatten().any(|cell| dirty(&cell.runs)),
            TextBlock::Image { .. } => false,
        });
        if !any {
            return false;
        }
        let clean = |runs: &mut Vec<Run>| {
            for run in runs.iter_mut() {
                if let std::borrow::Cow::Owned(text) = strip_control_chars(&run.text) {
                    run.text = text;
                }
            }
        };
        for block in self.blocks_mut().iter_mut() {
            match block {
                TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => clean(runs),
                TextBlock::Table(table) => {
                    for row in &mut table.rows {
                        for cell in row {
                            clean(&mut cell.runs);
                        }
                    }
                }
                TextBlock::Image { .. } => {}
            }
        }
        true
    }

    /// Same text without image blocks – what GAEB can carry.
    pub fn without_images(&self) -> RichText {
        RichText::new(
            self.blocks
                .iter()
                .filter(|block| !matches!(block, TextBlock::Image { .. }))
                .cloned()
                .collect(),
        )
    }

    /// Replaces every occurrence of `find` in paragraphs, list items and table
    /// cells, also across run boundaries; the replacement takes the format of
    /// the first replaced character, formatting around it stays. Returns the
    /// number of replacements. Matches do not cross paragraphs or cells.
    pub fn replace_all(&mut self, find: &str, replace: &str, case_sensitive: bool) -> usize {
        self.replace_matches(find, replace, case_sensitive, false)
    }

    /// Like [`RichText::replace_all`]; with `whole_word` only hits that are
    /// not part of a longer word (letters, digits, `_` around them).
    pub fn replace_matches(
        &mut self,
        find: &str,
        replace: &str,
        case_sensitive: bool,
        whole_word: bool,
    ) -> usize {
        let options = Matching {
            case_sensitive,
            whole_word,
        };
        // Texts without a hit stay shared with their copies (undo snapshots).
        if find.is_empty() || self.count_matches(find, case_sensitive, whole_word) == 0 {
            return 0;
        }
        let mut count = 0;
        for block in self.blocks_mut() {
            match block {
                TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                    count += replace_in_runs(runs, find, Some(replace), options);
                }
                TextBlock::Table(table) => {
                    for cell in table.rows.iter_mut().flatten() {
                        count += replace_in_runs(&mut cell.runs, find, Some(replace), options);
                    }
                }
                TextBlock::Image { .. } => {}
            }
        }
        count
    }

    /// Number of occurrences of `find` (same rules as [`RichText::replace_matches`]).
    pub fn count_matches(&self, find: &str, case_sensitive: bool, whole_word: bool) -> usize {
        let options = Matching {
            case_sensitive,
            whole_word,
        };
        let mut count = 0;
        for block in self.blocks.iter() {
            match block {
                TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                    count += replace_in_runs(&mut runs.clone(), find, None, options);
                }
                TextBlock::Table(table) => {
                    for cell in table.rows.iter().flatten() {
                        count += replace_in_runs(&mut cell.runs.clone(), find, None, options);
                    }
                }
                TextBlock::Image { .. } => {}
            }
        }
        count
    }

    /// Runs of every paragraph, list item and cell normalized (empty runs
    /// dropped, equal neighbours joined).
    pub fn normalized(&self) -> RichText {
        let mut text = self.clone();
        for block in text.blocks_mut() {
            match block {
                TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                    *runs = normalize_runs(std::mem::take(runs));
                }
                TextBlock::Table(table) => {
                    for cell in table.rows.iter_mut().flatten() {
                        cell.runs = normalize_runs(std::mem::take(&mut cell.runs));
                    }
                }
                TextBlock::Image { .. } => {}
            }
        }
        text
    }
}

#[cfg(test)]
mod list_tests {
    use super::*;

    /// Markers as printed: bullets by level, numbers „1.“, „a)“, „i.“ by
    /// level, counted per level as nested lists count.
    #[test]
    fn markers_count_per_level() {
        let text = RichText::new(vec![
            TextBlock::list_item(true, vec![Run::plain("eins")]),
            TextBlock::list_item_at(true, 1, vec![Run::plain("a")]),
            TextBlock::list_item_at(true, 1, vec![Run::plain("b")]),
            TextBlock::list_item_at(false, 2, vec![Run::plain("Strich")]),
            TextBlock::list_item_at(true, 2, vec![Run::plain("i")]),
            TextBlock::list_item_at(true, 2, vec![Run::plain("ii")]),
            TextBlock::list_item(true, vec![Run::plain("zwei")]),
            TextBlock::list_item_at(true, 1, vec![Run::plain("a wieder")]),
            TextBlock::list_item_at(false, 1, vec![Run::plain("Strich")]),
            TextBlock::text("Text"),
            TextBlock::list_item(true, vec![Run::plain("neu")]),
            TextBlock::list_item_at(true, 9, vec![Run::plain("zu tief")]),
        ]);
        let markers: Vec<String> = text
            .list_markers()
            .into_iter()
            .map(Option::unwrap_or_default)
            .collect();
        assert_eq!(
            markers,
            vec!["1.", "a)", "b)", "•", "i.", "ii.", "2.", "a)", "•", "", "1.", "a)"]
        );
        assert_eq!(text.blocks()[11].list_kind(), Some((true, MAX_LIST_LEVEL)));
        assert_eq!(list_marker(true, 1, 28), "ab)");
        assert_eq!(list_marker(true, 2, 14), "xiv.");
    }

    /// Plain text: two blanks per level; the exchange lines number.
    #[test]
    fn plain_text_and_lines_indent_unterpunkte() {
        let text = RichText::new(vec![
            TextBlock::text("Inhalte:"),
            TextBlock::list_item(true, vec![Run::plain("Automation")]),
            TextBlock::list_item_at(false, 1, vec![Run::plain("Parameter")]),
            TextBlock::list_item(true, vec![Run::plain("Leitebene")]),
        ]);
        assert_eq!(
            text.to_plain(),
            "Inhalte:\n- Automation\n  - Parameter\n- Leitebene"
        );
        assert_eq!(
            text.to_lines(),
            "Inhalte:\n1. Automation\n  - Parameter\n2. Leitebene"
        );
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;

    #[test]
    fn stray_control_characters_go_tab_and_line_breaks_stay() {
        assert_eq!(strip_control_chars("a\tb\nc"), "a\tb\nc");
        assert!(matches!(
            strip_control_chars("Haus"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(
            strip_control_chars("Zeile 1\r\nZeile 2\rZeile 3"),
            "Zeile 1\nZeile 2\nZeile 3"
        );
        assert_eq!(
            strip_control_chars("x\u{b}y\u{c}z\u{1}\u{1b}\u{7f}\u{85}\u{0}!"),
            "xyz!"
        );
        let mut text = RichText::new(vec![
            TextBlock::paragraph(vec![Run::plain("A\u{b}B")]),
            TextBlock::Table(Table::from_texts(vec![vec!["C\u{1}".into()]])),
        ]);
        assert!(text.strip_control_chars());
        assert_eq!(text.to_plain(), "AB\nC");
        assert!(!text.strip_control_chars());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_round_trips_and_images_are_dropped_for_gaeb() {
        let mut text = RichText::plain("Zeile 1\nZeile 2");
        assert_eq!(text.to_plain(), "Zeile 1\nZeile 2");
        text.push(TextBlock::Image {
            file: "bild.png".into(),
            width_mm: Some(60.0),
        });
        assert!(text.has_images());
        assert_eq!(text.images().collect::<Vec<_>>(), vec!["bild.png"]);
        assert!(!text.without_images().has_images());
        assert_eq!(text.without_images().to_plain(), "Zeile 1\nZeile 2");
    }

    #[test]
    fn copies_share_the_blocks_until_one_changes() {
        let text = RichText::plain("eins\nzwei");
        let mut copy = text.clone();
        assert!(Arc::ptr_eq(&text.blocks, &copy.blocks));
        assert_eq!(copy.replace_matches("drei", "x", true, false), 0);
        assert!(
            Arc::ptr_eq(&text.blocks, &copy.blocks),
            "ohne Treffer bleibt der Text geteilt"
        );
        assert_eq!(copy.replace_matches("zwei", "drei", true, false), 1);
        assert!(!Arc::ptr_eq(&text.blocks, &copy.blocks));
        assert_eq!(text.to_plain(), "eins\nzwei");
        assert_eq!(copy.to_plain(), "eins\ndrei");
        assert_ne!(text, copy);
        copy.blocks_mut()[1] = TextBlock::text("zwei");
        assert_eq!(text, copy);
    }

    #[test]
    fn empty_text() {
        assert!(RichText::plain("").is_empty());
        assert!(RichText::plain("  ").is_empty());
        assert!(!RichText::plain("x").is_empty());
        let mut table = RichText::default();
        table.push(TextBlock::Table(Table::empty(2, 2)));
        assert!(table.is_empty());
    }

    #[test]
    fn normalize_joins_equal_runs_and_drops_empty_ones() {
        let runs = vec![
            Run::plain("a"),
            Run::plain(""),
            Run::plain("b"),
            Run::bold("c"),
            Run {
                size_pt: Some(12.0),
                ..Run::bold("d")
            },
        ];
        let out = normalize_runs(runs);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].text, "ab");
        assert_eq!(out[1], Run::bold("c"));
        assert_eq!(out[2].size_pt, Some(12.0));
    }

    #[test]
    fn apply_format_splits_runs_at_char_boundaries() {
        let runs = vec![Run::plain("Fußboden"), Run::bold(" glatt")];
        let out = apply_format(&runs, 2, 10, |run| run.italic = true);
        assert_eq!(runs_text(&out), "Fußboden glatt");
        assert_eq!(out[0], Run::plain("Fu"));
        assert_eq!(out[1].text, "ßboden");
        assert!(out[1].italic && !out[1].bold);
        assert_eq!(out[2].text, " g");
        assert!(out[2].italic && out[2].bold);
        assert_eq!(out[3], Run::bold("latt"));
        // Removing the format again joins the runs.
        let back = apply_format(&out, 0, 14, |run| run.italic = false);
        assert_eq!(back, runs);
        // Reversed and out-of-range bounds are clamped.
        let all = apply_format(&runs, 99, 0, |run| run.font = Some("Courier New".into()));
        assert_eq!(all.len(), 2);
        assert!(all
            .iter()
            .all(|run| run.font.as_deref() == Some("Courier New")));
    }

    #[test]
    fn replace_all_keeps_formatting() {
        let mut text = RichText::new(vec![
            TextBlock::Paragraph {
                runs: vec![
                    Run::plain("DDC-Station und "),
                    Run::bold("ddc"),
                    Run::plain("-Modul"),
                ],
                align: Align::Center,
            },
            TextBlock::Table(Table::from_texts(vec![vec!["DDC".into(), "keins".into()]])),
            TextBlock::Image {
                file: "ddc.png".into(),
                width_mm: None,
            },
        ]);
        assert_eq!(text.count_matches("ddc", false, false), 3);
        assert_eq!(text.count_matches("ddc", true, false), 1);
        // Whole words: "DDC" in "DDC-Station" counts (hyphen is no word char).
        assert_eq!(text.count_matches("ddc", false, true), 3);
        assert_eq!(text.count_matches("Station", true, true), 1);
        assert_eq!(text.count_matches("Stat", true, true), 0);
        assert_eq!(text.replace_all("DDC", "AS", true), 2);
        let TextBlock::Paragraph { runs, align } = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(*align, Align::Center);
        assert_eq!(
            runs,
            &vec![
                Run::plain("AS-Station und "),
                Run::bold("ddc"),
                Run::plain("-Modul")
            ]
        );
        // Case-insensitive, across a run boundary: the replacement takes the
        // format of the first replaced character.
        assert_eq!(text.replace_all("und DDC-m", "+ ", false), 1);
        let TextBlock::Paragraph { runs, .. } = &text.blocks()[0] else {
            panic!()
        };
        assert_eq!(runs, &vec![Run::plain("AS-Station + odul")]);
        assert_eq!(text.to_plain(), "AS-Station + odul\nAS\tkeins");
        assert_eq!(text.replace_all("", "x", true), 0);
        assert_eq!(text.replace_all("s", "", false), 4);
        assert_eq!(text.to_plain(), "A-tation + odul\nA\tkein");
        assert_eq!(text.images().count(), 1);
    }

    #[test]
    fn font_names_map_to_liberation_families() {
        assert_eq!(font_class(None), FontClass::Sans);
        assert_eq!(font_class(Some("Arial")), FontClass::Sans);
        assert_eq!(font_class(Some("Helvetica")), FontClass::Sans);
        assert_eq!(font_class(Some("Microsoft Sans Serif")), FontClass::Sans);
        assert_eq!(font_class(Some("Times New Roman")), FontClass::Serif);
        assert_eq!(font_class(Some("'Times', serif")), FontClass::Serif);
        assert_eq!(font_class(Some("serif")), FontClass::Serif);
        assert_eq!(font_class(Some("Courier New")), FontClass::Mono);
        assert_eq!(font_class(Some("Liberation Mono")), FontClass::Mono);
        assert_eq!(font_class(Some("Wingdings")), FontClass::Sans);
    }

    #[test]
    fn align_css_names() {
        for align in Align::ALL {
            assert_eq!(Align::from_css(align.css()), Some(align));
        }
        assert_eq!(Align::from_css(" CENTER "), Some(Align::Center));
        assert_eq!(Align::from_css("bogus"), None);
    }

    #[test]
    fn tables_from_texts_and_formatting_flag() {
        let table = Table::from_texts(vec![vec!["a".into(), "b".into()], vec!["c".into()]]);
        assert_eq!(table.column_count(), 2);
        assert_eq!(table.texts()[1], vec!["c".to_string()]);
        let text = RichText::new(vec![TextBlock::Table(table)]);
        assert_eq!(text.to_plain(), "a\tb\nc");
        assert!(text.has_formatting());
        assert!(!RichText::plain("x").has_formatting());
        let centered = RichText::new(vec![TextBlock::Paragraph {
            runs: vec![Run::plain("x")],
            align: Align::Center,
        }]);
        assert!(centered.has_formatting());
    }
}
