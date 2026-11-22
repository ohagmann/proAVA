//! GAEB text (the XHTML subset of DA XML) to and from [`RichText`].
//!
//! Character formatting comes from tags (`b`, `i`, `u` …) and the `style`
//! attribute (`font-weight`, `font-style`, `text-decoration`, `font-family`,
//! `font-size`), alignment from `text-align` or `align` at paragraphs,
//! list items and table cells (GAEB DA XML 3.3 Fachdokumentation 11.3.2).

use super::dom::{Element, Node};
use super::image::{read_image, write_image};
use super::writer::escape;
use crate::model::{
    Align, Cell, ComplementNumber, RichText, Run, Table, TextAnchor, TextBlock, TextComplement,
    MAX_LIST_LEVEL,
};

/// Stands in the text where a TextComplement was until [`take_anchors`]
/// turns it into anchors.
pub(crate) const ANCHOR_MARK: char = '\u{FFFC}';

/// Font sizes GAEB allows (Fachdokumentation 3.3, 11.3.2 `font-size`).
const GAEB_SIZES: [f32; 19] = [
    3.0, 4.5, 6.5, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 15.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0,
    34.0, 36.0, 48.0,
];
/// Pixels in GAEB widths (`width:113` = 30 mm) are 1/96 inch.
const MM_PER_PX: f32 = 25.4 / 96.0;
/// Reference width for percentages in table widths.
const TEXT_WIDTH_MM: f32 = 160.0;

#[derive(Debug, Clone, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    font: Option<String>,
    size_pt: Option<f32>,
    align: Align,
}

fn parse_align(value: &str) -> Option<Align> {
    Some(match value.trim().to_ascii_lowercase().as_str() {
        "left" | "start" => Align::Left,
        "center" | "middle" => Align::Center,
        "right" | "end" => Align::Right,
        "justify" => Align::Justify,
        _ => return None,
    })
}

fn css_align(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
    }
}

fn number(text: &str) -> Option<f32> {
    let value: f32 = text.trim().replace(',', ".").parse().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

/// `font-size` in points: `10pt`, `10`, `13px`, keywords, `em`/`%` relative
/// to the inherited size.
fn parse_size(value: &str, inherited: Option<f32>) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let base = inherited.unwrap_or(10.0);
    let keyword = match value.as_str() {
        "xx-small" => Some(3.0),
        "x-small" => Some(4.5),
        "small" => Some(6.5),
        "medium" => Some(10.0),
        "large" => Some(15.0),
        "x-large" => Some(22.0),
        "xx-large" => Some(34.0),
        "smaller" => Some(base * 0.8),
        "larger" => Some(base * 1.2),
        _ => None,
    };
    if keyword.is_some() {
        return keyword;
    }
    let size = if let Some(points) = value.strip_suffix("pt") {
        number(points)?
    } else if let Some(pixels) = value.strip_suffix("px") {
        number(pixels)? * 0.75
    } else if let Some(em) = value.strip_suffix("em") {
        number(em)? * base
    } else if let Some(percent) = value.strip_suffix('%') {
        number(percent)? * base / 100.0
    } else {
        number(&value)?
    };
    Some((size * 10.0).round() / 10.0)
}

/// Width of a table cell in mm: `113` / `113px` (pixels), `30mm`, `3cm`, `20%`.
fn parse_width(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let mm = if let Some(mm) = value.strip_suffix("mm") {
        number(mm)?
    } else if let Some(cm) = value.strip_suffix("cm") {
        number(cm)? * 10.0
    } else if let Some(points) = value.strip_suffix("pt") {
        number(points)? * 25.4 / 72.0
    } else if let Some(percent) = value.strip_suffix('%') {
        number(percent)? * TEXT_WIDTH_MM / 100.0
    } else {
        number(value.strip_suffix("px").unwrap_or(&value))? * MM_PER_PX
    };
    Some((mm * 10.0).round() / 10.0)
}

/// First family of a `font-family` list, without quotes.
fn parse_family(value: &str) -> Option<String> {
    let first = value
        .split(',')
        .next()?
        .trim()
        .trim_matches(['"', '\''])
        .trim();
    (!first.is_empty()).then(|| first.to_string())
}

fn css_declarations(css: &str) -> impl Iterator<Item = (String, &str)> {
    css.split(';').filter_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        Some((key.trim().to_ascii_lowercase(), value.trim()))
    })
}

impl Style {
    fn apply(mut self, css: Option<&str>) -> Self {
        let Some(css) = css else { return self };
        for (key, value) in css_declarations(css) {
            let lower = value.to_ascii_lowercase();
            match key.as_str() {
                "font-weight" => {
                    self.bold = matches!(
                        lower.as_str(),
                        "bold" | "bolder" | "600" | "700" | "800" | "900"
                    )
                }
                "font-style" => self.italic = matches!(lower.as_str(), "italic" | "oblique"),
                "text-decoration" | "text-decoration-line" => {
                    self.underline = lower.contains("underline")
                }
                "font-family" => {
                    if let Some(family) = parse_family(value) {
                        self.font = Some(family);
                    }
                }
                "font-size" => {
                    // Only the sizes GAEB allows (4 pt in a file becomes 4.5 pt).
                    if let Some(size) = parse_size(value, self.size_pt) {
                        self.size_pt = Some(gaeb_size(size));
                    }
                }
                "text-align" => {
                    if let Some(align) = parse_align(value) {
                        self.align = align;
                    }
                }
                _ => {}
            }
        }
        self
    }

    /// Style of an element: tag meaning, `align` attribute, then `style`.
    fn of(&self, element: &Element) -> Self {
        let mut style = self.clone();
        match element.name.as_str() {
            "b" | "strong" => style.bold = true,
            "i" | "em" => style.italic = true,
            "u" => style.underline = true,
            _ => {}
        }
        if let Some(align) = element.attr("align").and_then(parse_align) {
            style.align = align;
        }
        if element.name == "font" {
            if let Some(face) = element.attr("face").and_then(parse_family) {
                style.font = Some(face);
            }
        }
        style.apply(element.attr("style"))
    }

    fn run(&self, text: &str) -> Run {
        Run {
            text: text.to_string(),
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            font: self.font.clone(),
            size_pt: self.size_pt,
        }
    }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<TextBlock>,
    open: Option<(Vec<Run>, Align)>,
    complements: Vec<TextComplement>,
    /// `<image>` elements without a readable picture.
    dropped_images: usize,
}

impl Builder {
    fn push_run(&mut self, text: &str, style: &Style) {
        if text.is_empty() {
            return;
        }
        self.open
            .get_or_insert_with(|| (Vec::new(), style.align))
            .0
            .push(style.run(text));
    }

    fn line_break(&mut self, style: &Style) {
        line_break(&mut self.open.get_or_insert_with(|| (Vec::new(), style.align)).0);
    }

    /// A tab of Word-HTML (empty span with `mso-tab-count`): one space
    /// between the words around it, as tabs inside spans become spaces –
    /// none at the start of a paragraph or after a space already there.
    fn push_tab(&mut self, style: &Style) {
        let ends_open = self.open.as_ref().is_some_and(|(runs, _)| {
            runs.iter()
                .rev()
                .find(|run| !run.text.is_empty())
                .is_some_and(|run| !run.text.ends_with(char::is_whitespace))
        });
        if ends_open {
            self.push_run(" ", style);
        }
    }

    fn close(&mut self) {
        if let Some((runs, align)) = self.open.take() {
            self.blocks.push(TextBlock::Paragraph {
                runs: normalize_runs(runs),
                align,
            });
        }
    }

    /// A block inside a paragraph (image) ends the text before it.
    fn push_block(&mut self, block: TextBlock) {
        if let Some((runs, align)) = self.open.take().filter(|(runs, _)| !runs.is_empty()) {
            self.blocks.push(TextBlock::Paragraph {
                runs: normalize_runs(runs),
                align,
            });
        }
        self.blocks.push(block);
    }

    fn image(&mut self, element: &Element) {
        match read_image(element) {
            Some(block) => self.push_block(block),
            None => {
                // Like a readable image, it does not leave an empty paragraph.
                if self.open.as_ref().is_some_and(|(runs, _)| runs.is_empty()) {
                    self.open = None;
                }
                self.dropped_images += 1;
            }
        }
    }

    fn finish(mut self) -> ReadText {
        self.close();
        ReadText {
            text: RichText {
                blocks: self.blocks,
            },
            complements: self.complements,
            dropped_images: self.dropped_images,
        }
    }
}

/// A text read from DA XML.
pub(crate) struct ReadText {
    pub text: RichText,
    pub complements: Vec<TextComplement>,
    pub dropped_images: usize,
}

/// A line break belongs to the run before it; at the start of a paragraph
/// it is a run of its own without formatting.
fn line_break(runs: &mut Vec<Run>) {
    match runs.last_mut() {
        Some(run) => run.text.push('\n'),
        None => runs.push(Run::plain("\n")),
    }
}

pub(crate) fn normalize_runs(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.same_format(&run) && !run.text.chars().all(|ch| ch == '\n') {
                last.text.push_str(&run.text);
                continue;
            }
        }
        out.push(run);
    }
    out
}

/// Text node content: indentation of pretty-printed files is dropped,
/// line breaks inside text count as one space.
fn normalize_text(text: &str) -> String {
    let structural = |ch: char| matches!(ch, '\n' | '\r' | '\t');
    if text.trim().is_empty() {
        return if text.contains(structural) {
            String::new()
        } else {
            text.to_string()
        };
    }
    let mut out = String::with_capacity(text.len());
    let mut gap = String::new();
    for ch in text.chars() {
        if ch == ' ' || structural(ch) {
            gap.push(ch);
            continue;
        }
        if !gap.is_empty() {
            if !gap.contains(structural) {
                out.push_str(&gap);
            } else if !out.is_empty() {
                out.push(' ');
            }
            gap.clear();
        }
        out.push(ch);
    }
    if !gap.contains(structural) {
        out.push_str(&gap);
    }
    out
}

/// Text inside a span: line breaks and tabs become spaces, spaces stay.
fn span_text(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            }
        })
        .collect()
}

fn read_flow(element: &Element, style: &Style, builder: &mut Builder) {
    for node in &element.children {
        match node {
            Node::Text(text) => builder.push_run(&normalize_text(text), style),
            Node::Element(child) => read_element(child, style, builder),
        }
    }
}

fn read_element(element: &Element, style: &Style, builder: &mut Builder) {
    match element.name.as_str() {
        "span" | "font" | "b" | "strong" | "i" | "em" | "u" | "a" | "sub" | "sup" => {
            // A span with its own `text-align` starts a bare paragraph
            // (proAVA, complement_place): alignment, or after `<br/>` a
            // new paragraph.
            if element.name == "span" {
                if let Some(align) = element.attr("style").and_then(own_align) {
                    if super::complement_place::paragraph_start(
                        &mut builder.open,
                        align,
                        ANCHOR_MARK,
                    ) {
                        builder.close();
                        builder.open = Some((Vec::new(), align));
                    }
                }
            }
            let style = style.of(element);
            // Word-HTML (other programs, pretty-printed files) writes a tab as
            // an empty span: without it the words around it would run together.
            if element.name == "span"
                && element.children.is_empty()
                && element
                    .attr("style")
                    .is_some_and(|css| css.contains("mso-tab-count"))
            {
                builder.push_tab(&style);
            }
            for node in &element.children {
                match node {
                    Node::Text(text) => builder.push_run(&span_text(text), &style),
                    Node::Element(child) => read_element(child, &style, builder),
                }
            }
        }
        "br" => builder.line_break(style),
        "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            builder.close();
            let style = style.of(element);
            builder.open = Some((Vec::new(), style.align));
            read_flow(element, &style, builder);
            builder.close();
        }
        "ul" | "ol" => {
            builder.close();
            read_list(element, style, builder, 0);
        }
        "table" => {
            builder.close();
            let mut images = Builder::default();
            let table = read_table(element, style, &mut images);
            if !table.rows.is_empty() {
                builder.blocks.push(TextBlock::Table(table));
            }
            // Images in cells follow the table (cells hold text only).
            builder.blocks.extend(images.blocks);
            builder.dropped_images += images.dropped_images;
            builder.complements.extend(images.complements);
        }
        // Text complements are kept apart from the Langtext, as in GAEB 90.
        // Their place is marked and read back by [`take_anchors`].
        "TextComplement" => {
            builder.complements.push(read_complement(element));
            builder
                .open
                .get_or_insert_with(|| (Vec::new(), Align::default()))
                .0
                .push(Run {
                    text: ANCHOR_MARK.to_string(),
                    ..Run::default()
                });
        }
        "page" => builder.close(),
        "Image" | "image" | "img" => builder.image(element),
        "Style" | "attachment" => {}
        _ => read_flow(element, &style.of(element), builder),
    }
}

/// Paragraphs of a container (list item, table cell) as one run list with
/// line breaks between them; other blocks (nested tables, images) are given
/// back separately. The alignment is that of the first paragraph.
fn flatten(
    element: &Element,
    style: &Style,
    outer: &mut Builder,
) -> (Vec<Run>, Align, Vec<TextBlock>) {
    flatten_nodes(&element.children, style, outer)
}

/// [`flatten`] for a part of a container's children.
fn flatten_nodes(
    nodes: &[Node],
    style: &Style,
    outer: &mut Builder,
) -> (Vec<Run>, Align, Vec<TextBlock>) {
    let mut inner = Builder::default();
    for node in nodes {
        match node {
            Node::Text(text) => inner.push_run(&normalize_text(text), style),
            Node::Element(child) => read_element(child, style, &mut inner),
        }
    }
    let ReadText {
        text,
        complements,
        dropped_images,
    } = inner.finish();
    outer.complements.extend(complements);
    outer.dropped_images += dropped_images;
    let mut runs: Vec<Run> = Vec::new();
    let mut align = None;
    let mut trailing = Vec::new();
    for block in text.blocks {
        match block {
            TextBlock::Paragraph {
                runs: paragraph,
                align: paragraph_align,
            }
            | TextBlock::ListItem {
                runs: paragraph,
                align: paragraph_align,
                ..
            } => {
                if !runs.is_empty() {
                    line_break(&mut runs);
                }
                align.get_or_insert(paragraph_align);
                runs.extend(paragraph);
            }
            other => trailing.push(other),
        }
    }
    (normalize_runs(runs), align.unwrap_or(style.align), trailing)
}

/// A list on `level`: its items, and lists nested in an item (GAEB 3.3:
/// `<ul>`/`<ol>` in `<li>`; older files also put them straight into the
/// list) one level deeper.
fn read_list(element: &Element, style: &Style, builder: &mut Builder, level: u8) {
    let ordered = element.name == "ol";
    let style = style.of(element);
    let deeper = (level + 1).min(MAX_LIST_LEVEL);
    for child in element.elements() {
        match child.name.as_str() {
            "li" => read_item(child, ordered, level, &style.of(child), builder),
            "ul" | "ol" => read_list(child, &style, builder, deeper),
            _ => read_element(child, &style, builder),
        }
    }
}

/// One `<li>`: its own text is the item; a list in it gives the Unterpunkte
/// one level deeper. An item that holds nothing but such lists (some
/// programs indent that way) gives no empty item of its own; text after a
/// nested list is an item on this level again – wording and order stay.
fn read_item(li: &Element, ordered: bool, level: u8, style: &Style, builder: &mut Builder) {
    let is_list = |node: &Node| matches!(node, Node::Element(child) if matches!(child.name.as_str(), "ul" | "ol"));
    let push = |nodes: &[Node], builder: &mut Builder, keep_empty: bool| {
        let nothing = nodes.iter().all(|node| match node {
            Node::Text(text) => text.trim().is_empty(),
            Node::Element(_) => false,
        });
        if nothing && !keep_empty {
            return;
        }
        let (runs, align, trailing) = flatten_nodes(nodes, style, builder);
        builder.blocks.push(TextBlock::ListItem {
            ordered,
            level,
            runs,
            align,
        });
        builder.blocks.extend(trailing);
    };
    if !li.children.iter().any(is_list) {
        push(&li.children, builder, true);
        return;
    }
    let deeper = (level + 1).min(MAX_LIST_LEVEL);
    let mut start = 0;
    for (index, node) in li.children.iter().enumerate() {
        if let Node::Element(list) = node {
            if is_list(node) {
                push(&li.children[start..index], builder, false);
                read_list(list, style, builder, deeper);
                start = index + 1;
            }
        }
    }
    push(&li.children[start..], builder, false);
}

fn read_table(element: &Element, style: &Style, images: &mut Builder) -> Table {
    // Alignment and width of the table do not pass on to the cells.
    let style = Style {
        align: Align::Left,
        ..style.clone()
    };
    let mut table = Table::default();
    let mut head_done = false;
    collect_rows(element, &style, false, &mut table, &mut head_done, images);
    let columns = table.column_count();
    for row in &mut table.rows {
        row.resize(columns, Cell::default());
    }
    if table.column_widths_mm.iter().all(Option::is_none) {
        table.column_widths_mm.clear();
    } else {
        table.column_widths_mm.resize(columns, None);
    }
    table
}

fn collect_rows(
    element: &Element,
    style: &Style,
    in_head: bool,
    table: &mut Table,
    head_done: &mut bool,
    images: &mut Builder,
) {
    for child in element.elements() {
        match child.name.as_str() {
            "tr" => {
                let row_style = style.of(child);
                let mut cells = Vec::new();
                let mut all_head = true;
                for cell in child.elements() {
                    if cell.name != "td" && cell.name != "th" {
                        continue;
                    }
                    all_head &= cell.name == "th";
                    let cell_style = row_style.of(cell);
                    let (mut runs, align, trailing) = flatten(cell, &cell_style, images);
                    for block in trailing {
                        match block {
                            TextBlock::Image { .. } => images.blocks.push(block),
                            // Nested tables become text lines in the cell.
                            TextBlock::Table(inner) => {
                                for line in inner.texts() {
                                    line_break(&mut runs);
                                    runs.push(Run::plain(line.join("  ")));
                                }
                            }
                            _ => {}
                        }
                    }
                    let span: usize = cell
                        .attr("colspan")
                        .and_then(|value| value.trim().parse().ok())
                        .filter(|span| *span >= 1)
                        .unwrap_or(1)
                        .min(64);
                    if span == 1 {
                        let column = cells.len();
                        let width = cell
                            .attr("style")
                            .into_iter()
                            .flat_map(css_declarations)
                            .find(|(key, _)| key == "width")
                            .and_then(|(_, value)| parse_width(value))
                            .or_else(|| cell.attr("width").and_then(parse_width));
                        if let Some(width) = width {
                            if table.column_widths_mm.len() <= column {
                                table.column_widths_mm.resize(column + 1, None);
                            }
                            table.column_widths_mm[column].get_or_insert(width);
                        }
                    }
                    cells.push(Cell {
                        runs: trim_runs(runs),
                        align,
                    });
                    // Spanned columns stay as empty cells so that columns line up.
                    for _ in 1..span {
                        cells.push(Cell::default());
                    }
                }
                let head = (in_head || (all_head && !cells.is_empty())) && !*head_done;
                if head && table.header_rows == table.rows.len() {
                    table.header_rows += 1;
                } else {
                    *head_done = true;
                }
                table.rows.push(cells);
            }
            "thead" => collect_rows(child, style, true, table, head_done, images),
            "tbody" | "tfoot" => collect_rows(child, style, false, table, head_done, images),
            _ => {}
        }
    }
}

/// Cell text without leading/trailing blanks and line breaks.
fn trim_runs(mut runs: Vec<Run>) -> Vec<Run> {
    let blank = |ch: char| ch.is_whitespace();
    while let Some(first) = runs.first_mut() {
        let trimmed = first.text.trim_start_matches(blank).to_string();
        if trimmed.is_empty() {
            runs.remove(0);
        } else {
            first.text = trimmed;
            break;
        }
    }
    while let Some(last) = runs.last_mut() {
        let trimmed = last.text.trim_end_matches(blank).to_string();
        if trimmed.is_empty() {
            runs.pop();
        } else {
            last.text = trimmed;
            break;
        }
    }
    runs
}

fn blocks_to_plain(text: &RichText) -> String {
    text.to_plain_lines()
}

impl RichText {
    /// Plain text without list markers (cells separated by tabs).
    fn to_plain_lines(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|block| match block {
                TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                    Some(runs.iter().map(|run| run.text.as_str()).collect::<String>())
                }
                TextBlock::Table(table) => Some(
                    table
                        .texts()
                        .iter()
                        .map(|cells| cells.join("\t"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                TextBlock::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) fn read_complement(element: &Element) -> TextComplement {
    let part = |name: &str| element.child(name).map(read_plain_raw).unwrap_or_default();
    let kind = element.attr("Kind").unwrap_or_default().trim();
    TextComplement {
        marker: element
            .attr("MarkLbl")
            .unwrap_or_default()
            .trim()
            .to_string(),
        kind: if kind.eq_ignore_ascii_case("owner") {
            "Owner".into()
        } else if kind.eq_ignore_ascii_case("bidder") {
            "Bidder".into()
        } else {
            kind.to_string()
        },
        // Caption and tail as written (spaces, line breaks): STLB-Bau Check
        // compares them; GAEB: the owner's text must not change in exchange.
        caption: outer_lines(&part("ComplCaption")),
        body: complement_body(&part("ComplBody")),
        tail: outer_lines(&part("ComplTail")),
        stlb_characteristic: element
            .attr("ArtChrIdent")
            .and_then(|value| value.trim().parse().ok()),
        number: if element.child("ComplBodyDec").is_some() {
            Some(ComplementNumber::Decimal)
        } else if element.child("ComplBodyInt").is_some() {
            Some(ComplementNumber::Integer)
        } else {
            None
        },
        anchor: None,
    }
}

/// Caption/tail text: an element holding nothing but blanks counts as
/// empty, everything else stays as written.
fn outer_lines(text: &str) -> String {
    if text.trim().is_empty() {
        String::new()
    } else {
        text.to_string()
    }
}

/// Body as GAEB 90 gives it: without the enclosing apostrophes, a dot
/// placeholder ("……") as empty text.
pub(crate) fn complement_body(raw: &str) -> String {
    let trim = |text: &str| {
        text.trim_matches(|ch: char| ch == '\n' || ch == ' ')
            .to_string()
    };
    let mut body = trim(raw);
    if let Some(inner) = body.strip_prefix('\'') {
        body = trim(inner.strip_suffix('\'').unwrap_or(inner));
    } else if let Some(inner) = body.strip_suffix('\'') {
        body = trim(inner);
    }
    if body.chars().all(|ch| matches!(ch, '.' | ' ' | '\n' | '…')) {
        String::new()
    } else {
        body
    }
}

/// Formatted text of a container (DetailTxt, DetailAddText …) with the
/// text complements found in it.
pub(crate) fn read_rich(element: &Element) -> ReadText {
    let mut builder = Builder::default();
    read_flow(element, &Style::default(), &mut builder);
    builder.finish()
}

fn read_plain_raw(element: &Element) -> String {
    let mut text = read_rich(element).text;
    take_anchors(&mut text);
    blocks_to_plain(&text)
}

/// Removes the complement marks from a text read by [`read_rich`] and
/// returns their places in order. Call it after every other change to the
/// text (e.g. joining hard-wrapped lines). A paragraph holding nothing but
/// marks disappears; its complements stand on a line of their own.
pub(crate) fn take_anchors(text: &mut RichText) -> Vec<TextAnchor> {
    let mut anchors = Vec::new();
    let mut blocks = Vec::with_capacity(text.blocks.len());
    for mut block in std::mem::take(&mut text.blocks) {
        let runs = match &mut block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => runs,
            _ => {
                blocks.push(block);
                continue;
            }
        };
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        if !joined.contains(ANCHOR_MARK) {
            blocks.push(block);
            continue;
        }
        let only_marks = joined
            .chars()
            .all(|character| character == ANCHOR_MARK || character.is_whitespace());
        let mut offset = 0;
        for character in joined.chars() {
            if character == ANCHOR_MARK {
                anchors.push(TextAnchor {
                    block: blocks.len(),
                    offset: if only_marks { 0 } else { offset },
                    own_line: only_marks,
                });
            } else {
                offset += 1;
            }
        }
        if only_marks {
            continue;
        }
        for run in runs.iter_mut() {
            run.text.retain(|character| character != ANCHOR_MARK);
        }
        // Without the mark, runs of the same format meet again: join them
        // all (also line-break-only runs), so that the reader and
        // `as_written` agree however the breaks around a complement fell.
        *runs = merge_same_format(normalize_runs(std::mem::take(runs)));
        blocks.push(block);
    }
    text.blocks = blocks;
    anchors
}

/// Adjacent runs of the same format as one run.
fn merge_same_format(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for run in runs {
        match out.last_mut() {
            Some(last) if last.same_format(&run) => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    out
}

/// Plain text of a short-text element (OutlTxt, LblTx …), lines joined with "\n".
pub(crate) fn read_plain(element: &Element) -> String {
    read_plain_raw(element).trim().to_string()
}

/// Placeholder written for an empty body in tenders, so that the bidder sees
/// where to fill in; read back as empty text.
const PLACEHOLDER: &str = "....................................................";

/// Font (family, size) of a run as CSS for the caption and tail of a
/// complement that follows it – only where it differs from the standard
/// font (Arial 10 pt), e.g. 9 pt; bold/italic/underline stay with the text.
pub(crate) fn font_css(run: &Run) -> String {
    let size = run
        .size_pt
        .filter(|size| (gaeb_size(*size) - 10.0).abs() > f32::EPSILON);
    let font = run
        .font
        .clone()
        .filter(|font| !font.trim().is_empty() && !font.trim().eq_ignore_ascii_case("Arial"));
    if size.is_none() && font.is_none() {
        return String::new();
    }
    run_css(&Run {
        text: String::new(),
        font,
        size_pt: size,
        ..Run::default()
    })
}

/// The font of the last text in a block, if it has words.
pub(crate) fn complement_font(block: &TextBlock) -> Option<String> {
    let runs = match block {
        TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => runs,
        _ => return None,
    };
    runs.iter()
        .rev()
        .find(|run| !run.text.trim().is_empty())
        .map(font_css)
}

/// Plain text as spans with `style` (may be empty) and `<br/>` for line breaks.
fn write_inline_styled(text: &str, css: &str, out: &mut String) {
    if css.is_empty() {
        write_inline_plain(text, out);
        return;
    }
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("<br/>");
        }
        if !line.is_empty() {
            out.push_str(&format!("<span style=\"{}\">", escape(css)));
            out.push_str(&escape(line));
            out.push_str("</span>");
        }
    }
}

/// `restricted` = X84: only the body, empty bodies stay empty.
pub(crate) fn write_complement(complement: &TextComplement, restricted: bool, out: &mut String) {
    write_complement_in(complement, restricted, "", out);
}

/// Like [`write_complement`]; `font` (CSS, may be empty) is the font of the
/// text around it – caption and tail are written in it, the value not.
pub(crate) fn write_complement_in(
    complement: &TextComplement,
    restricted: bool,
    font: &str,
    out: &mut String,
) {
    let kind = if complement.kind == "Owner" {
        "Owner"
    } else {
        "Bidder"
    };
    let marker = complement_marker(&complement.marker);
    let characteristic = complement
        .stlb_characteristic
        .map(|ident| format!(" ArtChrIdent=\"{ident}\""))
        .unwrap_or_default();
    // No Empty="Yes": STLB-Bau and other programs do not write it either (the
    // dots of the placeholder say it); the reader ignores it.
    let empty = complement.body.trim().is_empty();
    out.push_str(&format!(
        "<TextComplement MarkLbl=\"{marker}\" Kind=\"{kind}\"{characteristic}>"
    ));
    if !restricted && !complement.caption.is_empty() {
        out.push_str("<ComplCaption>");
        write_inline_styled(&complement.caption, font, out);
        out.push_str("</ComplCaption>");
    }
    // The numeric value of an STLB-Bau characteristic, repeated in the body.
    if let Some((number, value)) = complement.number.and_then(|number| {
        number
            .value_of(&complement.body)
            .map(|value| (number, value))
    }) {
        let element = match number {
            ComplementNumber::Decimal => "ComplBodyDec",
            ComplementNumber::Integer => "ComplBodyInt",
        };
        out.push_str(&format!("<{element} Value=\"{value}\"/>"));
    }
    // Body between apostrophes as STLB-Bau and the GAEB examples write it
    // (the reader takes them off again).
    let body = if empty && !restricted {
        format!("'{PLACEHOLDER}'")
    } else if empty {
        String::new()
    } else {
        format!("'{}'", complement.body)
    };
    if body.is_empty() {
        out.push_str("<ComplBody/>");
    } else {
        out.push_str("<ComplBody>");
        write_inline_plain(&body, out);
        out.push_str("</ComplBody>");
    }
    if !restricted && !complement.tail.is_empty() {
        out.push_str("<ComplTail>");
        write_inline_styled(&complement.tail, font, out);
        out.push_str("</ComplTail>");
    }
    // An empty ComplTail as STLB-Bau writes it (part of the
    // structure STLB-Bau Check compares).
    if !restricted && complement.tail.is_empty() {
        out.push_str("<ComplTail/>");
    }
    out.push_str("</TextComplement>");
}

/// MarkLbl must be an integer.
pub(crate) fn complement_marker(marker: &str) -> String {
    if !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_digit()) {
        marker.to_string()
    } else {
        "0".into()
    }
}

/// Plain text as spans with `<br/>` for line breaks (content of MLText).
pub(crate) fn write_inline_plain(text: &str, out: &mut String) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push_str("<br/>");
        }
        if !line.is_empty() {
            out.push_str("<span>");
            out.push_str(&escape(line));
            out.push_str("</span>");
        }
    }
}

/// Nearest font size GAEB allows.
fn gaeb_size(size: f32) -> f32 {
    GAEB_SIZES
        .iter()
        .copied()
        .min_by(|a, b| (a - size).abs().total_cmp(&(b - size).abs()))
        .unwrap_or(10.0)
}

fn format_points(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

/// `style` of a run: only what differs from plain text.
fn run_css(run: &Run) -> String {
    let mut css = String::new();
    if let Some(font) = &run.font {
        let font: String = font
            .chars()
            .filter(|ch| !matches!(ch, ';' | '"' | ':'))
            .collect();
        if !font.trim().is_empty() {
            css.push_str(&format!("font-family:{};", font.trim()));
        }
    }
    if let Some(size) = run.size_pt.filter(|size| size.is_finite() && *size > 0.0) {
        css.push_str(&format!("font-size:{}pt;", format_points(gaeb_size(size))));
    }
    if run.bold {
        css.push_str("font-weight:bold;");
    }
    if run.italic {
        css.push_str("font-style:italic;");
    }
    if run.underline {
        css.push_str("text-decoration:underline;");
    }
    css
}

pub(crate) fn write_runs(runs: &[Run], out: &mut String) {
    for run in runs {
        let css = run_css(run);
        for (index, line) in run.text.split('\n').enumerate() {
            if index > 0 {
                out.push_str("<br/>");
            }
            if line.is_empty() {
                continue;
            }
            if css.is_empty() {
                out.push_str("<span>");
            } else {
                out.push_str(&format!("<span style=\"{}\">", escape(&css)));
            }
            out.push_str(&escape(line));
            out.push_str("</span>");
        }
    }
}

/// `text-align` of an element's own style attribute.
fn own_align(css: &str) -> Option<Align> {
    css.split(';').find_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        (key.trim().eq_ignore_ascii_case("text-align"))
            .then(|| parse_align(value))
            .flatten()
    })
}

/// Empty span that starts a bare paragraph with its alignment (see
/// `complement_place`, [`Segment::Start`](super::complement_place::Segment)).
pub(crate) fn paragraph_start_span(align: Align) -> String {
    format!("<span style=\"text-align:{};\"/>", css_align(align))
}

/// Opening tag with `text-align` when not left.
fn open_tag(name: &str, align: Align, extra_css: &str) -> String {
    let mut css = String::new();
    if align != Align::Left {
        css.push_str(&format!("text-align:{};", css_align(align)));
    }
    css.push_str(extra_css);
    if css.is_empty() {
        format!("<{name}")
    } else {
        format!("<{name} style=\"{}\"", escape(&css))
    }
}

fn write_paragraph(runs: &[Run], align: Align, out: &mut String) {
    let mut inner = String::new();
    write_runs(runs, &mut inner);
    let tag = open_tag("p", align, "");
    if inner.is_empty() {
        out.push_str(&tag);
        out.push_str("/>");
    } else {
        out.push_str(&tag);
        out.push('>');
        out.push_str(&inner);
        out.push_str("</p>");
    }
}

fn write_table(table: &Table, out: &mut String) {
    let columns = table.column_count();
    if columns == 0 {
        return;
    }
    out.push_str("<table>");
    for (row_index, cells) in table.rows.iter().enumerate() {
        out.push_str("<tr>");
        let name = if row_index < table.header_rows {
            "th"
        } else {
            "td"
        };
        let empty = Cell::default();
        for column in 0..columns {
            let cell = cells.get(column).unwrap_or(&empty);
            // Widths in pixels at 96 dpi on the first row, as GAEB programs write them.
            let width = if row_index == 0 {
                table
                    .column_widths_mm
                    .get(column)
                    .copied()
                    .flatten()
                    .filter(|mm| *mm > 0.0)
                    .map(|mm| format!("width:{};", (mm / MM_PER_PX).round() as i64))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let tag = open_tag(name, cell.align, &width);
            let mut inner = String::new();
            write_runs(&cell.runs, &mut inner);
            if inner.is_empty() {
                out.push_str(&tag);
                out.push_str("/>");
            } else {
                out.push_str(&tag);
                out.push('>');
                out.push_str(&inner);
                out.push_str(&format!("</{name}>"));
            }
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
}

/// List items as nested XHTML lists (GAEB 3.3: a deeper list stands in
/// the `<li>` of the item before). A level without an item of its own (a
/// list that starts deeper) gets an empty `<li>` to hold the deeper list;
/// bullets and numbers changing on a level start a new list there.
fn write_list(items: &[TextBlock], out: &mut String) {
    fn tag(ordered: bool) -> &'static str {
        if ordered {
            "ol"
        } else {
            "ul"
        }
    }
    // An item without text and without Unterpunkte stays `<li/>`.
    fn close_item(out: &mut String) {
        if out.ends_with("<li><p/>") {
            out.truncate(out.len() - "<li><p/>".len());
            out.push_str("<li/>");
        } else {
            out.push_str("</li>");
        }
    }
    // The open lists, innermost last: numbered?, an `<li>` open in it?
    let mut open: Vec<(bool, bool)> = Vec::new();
    let close_list = |open: &mut Vec<(bool, bool)>, out: &mut String| {
        if let Some((ordered, item)) = open.pop() {
            if item {
                close_item(out);
            }
            out.push_str(&format!("</{}>", tag(ordered)));
        }
    };
    for item in items {
        let TextBlock::ListItem {
            ordered,
            level,
            runs,
            align,
        } = item
        else {
            continue;
        };
        let level = usize::from((*level).min(MAX_LIST_LEVEL));
        while open.len() > level + 1 {
            close_list(&mut open, out);
        }
        if open.len() == level + 1 {
            if open[level].0 != *ordered {
                close_list(&mut open, out);
            } else if open[level].1 {
                close_item(out);
                open[level].1 = false;
            }
        }
        while open.len() < level + 1 {
            if let Some(parent) = open.last_mut() {
                if !parent.1 {
                    out.push_str("<li>");
                    parent.1 = true;
                }
            }
            // Levels in between: plain bullet lists that hold the deeper one.
            let list_ordered = open.len() == level && *ordered;
            out.push_str(&format!("<{}>", tag(list_ordered)));
            open.push((list_ordered, false));
        }
        out.push_str("<li>");
        write_paragraph(runs, *align, out);
        open[level].1 = true;
    }
    while !open.is_empty() {
        close_list(&mut open, out);
    }
}

/// Writes the blocks as XHTML paragraphs, lists and tables (content of
/// `Text` or `DetailAddText`); images only with `images`.
pub(crate) fn write_blocks(text: &RichText, images: bool, out: &mut String) {
    let mut index = 0;
    while index < text.blocks.len() {
        match &text.blocks[index] {
            TextBlock::Paragraph { runs, align } => {
                write_paragraph(runs, *align, out);
                index += 1;
            }
            TextBlock::ListItem { .. } => {
                let end = text.blocks[index..]
                    .iter()
                    .position(|block| !matches!(block, TextBlock::ListItem { .. }))
                    .map_or(text.blocks.len(), |offset| index + offset);
                write_list(&text.blocks[index..end], out);
                index = end;
            }
            TextBlock::Image { data, width_mm, .. } => {
                let mut inner = String::new();
                if images && write_image(data, *width_mm, &mut inner) {
                    out.push_str("<p>");
                    out.push_str(&inner);
                    out.push_str("</p>");
                }
                index += 1;
            }
            TextBlock::Table(table) => {
                write_table(table, out);
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use crate::xml::dom::parse;

    fn read(xml: &str) -> RichText {
        read_rich(&parse(xml).unwrap()).text
    }

    fn write(text: &RichText) -> String {
        let mut out = String::new();
        write_blocks(text, false, &mut out);
        out
    }

    /// Written blocks without the `<span>` around every run.
    fn structure(text: &RichText) -> String {
        write(text).replace("<span>", "").replace("</span>", "")
    }

    fn items(text: &RichText) -> Vec<(String, bool, u8)> {
        text.blocks
            .iter()
            .filter_map(|block| match block {
                TextBlock::ListItem {
                    ordered,
                    level,
                    runs,
                    ..
                } => Some((
                    runs.iter().map(|run| run.text.as_str()).collect(),
                    *ordered,
                    *level,
                )),
                _ => None,
            })
            .collect()
    }

    /// Unterpunkte as GAEB 3.3 has them (and other programs write them):
    /// the deeper list in the `<li>` of the item. They keep their level and
    /// are written back the same way.
    #[test]
    fn nested_lists_keep_their_levels() {
        let source = "<p>Inhalte:</p><ul><li><p>Automationsebene</p><ul><li><p>Parameter</p></li>\
            <li><p>Alarme</p></li></ul></li><li><p>Leitebene</p><ul><li><p>Software</p></li></ul>\
            </li></ul>";
        let text = read(&format!("<Text>{source}</Text>"));
        let item = |text: &str, level| (text.to_string(), false, level);
        assert_eq!(
            items(&text),
            vec![
                item("Automationsebene", 0),
                item("Parameter", 1),
                item("Alarme", 1),
                item("Leitebene", 0),
                item("Software", 1),
            ]
        );
        assert_eq!(structure(&text), source);
        assert_eq!(read(&format!("<Text>{}</Text>", write(&text))), text);
    }

    /// Other forms of nesting: an item that only holds a deeper list (some
    /// programs indent that way) gives no empty item; text after a nested
    /// list is an item on the level of its `<li>` (wording and order stay);
    /// a list straight in a list (older files) is one level deeper; numbers
    /// in bullets.
    #[test]
    fn indenting_items_text_after_a_nested_list_and_lists_in_lists() {
        let text = read(
            "<Text><ul><li><ul><li><span>tief</span></li></ul><span>danach</span></li></ul>\
             <ol><li><ol><li><ol><li>ganz tief</li></ol></li></ol></li></ol>\
             <ul><li>x</li><ul><li>y</li></ul><li>z<ol><li>eins</li><li>zwei</li></ol></li></ul></Text>",
        );
        assert_eq!(
            items(&text),
            vec![
                ("tief".to_string(), false, 1),
                ("danach".to_string(), false, 0),
                ("ganz tief".to_string(), true, 2),
                ("x".to_string(), false, 0),
                ("y".to_string(), false, 1),
                ("z".to_string(), false, 0),
                ("eins".to_string(), true, 1),
                ("zwei".to_string(), true, 1),
            ]
        );
        // Written: deeper lists in the item before, a level without an item
        // of its own as an empty `<li>`; read back the same.
        let again = read(&format!("<Text>{}</Text>", write(&text)));
        assert_eq!(items(&again), items(&text));
        assert!(structure(&text)
            .contains("<li><p>z</p><ol><li><p>eins</p></li><li><p>zwei</p></li></ol></li>"));
    }

    /// An empty item stays `<li/>`; with Unterpunkte it keeps an empty
    /// paragraph, so that it is not taken for an indenting item.
    #[test]
    fn empty_items_stay_with_and_without_unterpunkte() {
        let text = RichText {
            blocks: vec![
                TextBlock::list_item(false, Vec::new()),
                TextBlock::list_item(false, vec![Run::plain("a")]),
                TextBlock::list_item(false, Vec::new()),
                TextBlock::list_item_at(false, 1, vec![Run::plain("b")]),
            ],
        };
        let written = write(&text);
        assert_eq!(
            structure(&text),
            "<ul><li/><li><p>a</p></li><li><p/><ul><li><p>b</p></li></ul></li></ul>"
        );
        assert_eq!(
            items(&read(&format!("<Text>{written}</Text>"))),
            items(&text)
        );
    }

    /// Word-HTML tab as an empty span between two words (pretty-printed, as
    /// some programs write it): one space, not glued; nothing doubled.
    #[test]
    fn empty_tab_span_separates_words() {
        let text = read(
            "<Text><ul><li>\n  <span>3 Kellerräume</span>\n  <span style=\"mso-tab-count:1;\"/>\n  <span>unter dem Hof</span>\n</li></ul>\n\
             <p><span>A </span><span style=\"mso-tab-count:2\"/><span>B</span></p>\n\
             <p><span style=\"mso-tab-count:1\"/><span>vorn</span></p></Text>",
        );
        let words: Vec<String> = text
            .blocks
            .iter()
            .map(|block| match block {
                TextBlock::ListItem { runs, .. } | TextBlock::Paragraph { runs, .. } => {
                    runs.iter().map(|run| run.text.as_str()).collect()
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(words, ["3 Kellerräume unter dem Hof", "A B", "vorn"]);
    }

    #[test]
    fn sizes_widths_and_families_parse() {
        assert_eq!(parse_size("10pt", None), Some(10.0));
        assert_eq!(parse_size("10", None), Some(10.0));
        assert_eq!(parse_size("16px", None), Some(12.0));
        assert_eq!(parse_size("large", None), Some(15.0));
        assert_eq!(parse_size("1.5em", Some(12.0)), Some(18.0));
        assert_eq!(parse_size("bogus", None), None);
        assert_eq!(parse_width("113"), Some(29.9));
        assert_eq!(parse_width("30mm"), Some(30.0));
        assert_eq!(parse_width("50%"), Some(80.0));
        assert_eq!(
            parse_family(" 'Times New Roman', serif"),
            Some("Times New Roman".into())
        );
        assert_eq!(gaeb_size(13.0), 12.0);
        assert_eq!(gaeb_size(4.0), 4.5);
        assert_eq!(gaeb_size(100.0), 48.0);
    }

    #[test]
    fn style_attributes_become_run_and_paragraph_formatting() {
        let text = read(
            r#"<Text><p style="text-align:center;font-family:Arial;font-size:14pt"><span style="font-weight:bold">Titel</span></p>
            <p align="right"><span style="font-family:'Courier New';font-size:9pt;font-style:italic;text-decoration:underline">rechts</span></p>
            <ul><li><p style="text-align:justify"><span>Punkt</span></p></li></ul></Text>"#,
        );
        assert_eq!(text.blocks.len(), 3);
        let TextBlock::Paragraph { runs, align } = &text.blocks[0] else {
            panic!()
        };
        assert_eq!(*align, Align::Center);
        assert_eq!(runs[0].font.as_deref(), Some("Arial"));
        assert_eq!(runs[0].size_pt, Some(14.0));
        assert!(runs[0].bold);
        let TextBlock::Paragraph { runs, align } = &text.blocks[1] else {
            panic!()
        };
        assert_eq!(*align, Align::Right);
        assert_eq!(runs[0].font.as_deref(), Some("Courier New"));
        assert!(runs[0].italic && runs[0].underline && !runs[0].bold);
        assert!(matches!(
            text.blocks[2],
            TextBlock::ListItem {
                align: Align::Justify,
                ..
            }
        ));
    }

    #[test]
    fn formatting_round_trips_through_xml() {
        let text = RichText {
            blocks: vec![
                TextBlock::Paragraph {
                    runs: vec![
                        Run {
                            font: Some("Times New Roman".into()),
                            size_pt: Some(12.0),
                            ..Run::plain("Serif ")
                        },
                        Run {
                            bold: true,
                            ..Run::plain("fett")
                        },
                    ],
                    align: Align::Center,
                },
                TextBlock::ListItem {
                    ordered: true,
                    level: 0,
                    runs: vec![Run::plain("eins")],
                    align: Align::Right,
                },
                TextBlock::Table(Table {
                    rows: vec![
                        vec![Cell::plain("Kopf A"), Cell::plain("Kopf B")],
                        vec![
                            Cell {
                                runs: vec![Run::plain("1,5")],
                                align: Align::Right,
                            },
                            Cell {
                                runs: vec![Run {
                                    italic: true,
                                    ..Run::plain("Zeile\nzwei")
                                }],
                                align: Align::Left,
                            },
                        ],
                    ],
                    header_rows: 1,
                    column_widths_mm: vec![Some(30.0), None],
                }),
            ],
        };
        let xml = write(&text);
        assert!(xml.contains("<p style=\"text-align:center;\">"), "{xml}");
        assert!(
            xml.contains("font-family:Times New Roman;font-size:12pt;"),
            "{xml}"
        );
        assert!(xml.contains("<th style=\"width:113;\">"), "{xml}");
        assert!(xml.contains("<td style=\"text-align:right;\">"), "{xml}");
        let back = read(&format!("<Text>{xml}</Text>"));
        // Identical except for the width (whole pixels).
        let TextBlock::Table(table) = &back.blocks[2] else {
            panic!("{back:?}")
        };
        assert_eq!(table.column_widths_mm, vec![Some(29.9), None]);
        let TextBlock::Table(original) = &text.blocks[2] else {
            panic!()
        };
        assert_eq!(table.rows, original.rows);
        assert_eq!(table.header_rows, 1);
        assert_eq!(back.blocks[..2], text.blocks[..2]);
    }

    #[test]
    fn colspan_cells_keep_the_columns_in_line() {
        let text = read(
            r#"<Text><table><thead><tr><th>A</th><th>B</th><th>C</th></tr></thead>
            <tr><td colspan="2" style="width:200">breit</td><td style="width:48">x</td></tr>
            <tr><td>1</td><td>2</td><td>3</td></tr></table></Text>"#,
        );
        let TextBlock::Table(table) = &text.blocks[0] else {
            panic!()
        };
        assert_eq!(table.header_rows, 1);
        assert_eq!(table.texts()[1], vec!["breit", "", "x"]);
        assert_eq!(table.column_widths_mm, vec![None, None, Some(12.7)]);
    }
}
