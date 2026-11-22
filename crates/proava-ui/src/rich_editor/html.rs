//! Clipboard HTML (Word, LibreOffice, browsers) to [`RichText`] and back.
//!
//! A small, forgiving reader: tags, attributes, entities and the usual
//! inline styles (bold, italic, underline, font family and size, text
//! alignment), paragraphs, line breaks, lists (nested lists as Unterpunkte,
//! also Word's „mso-list“ paragraphs with their level), tables (head rows,
//! widths). Fonts become the three families
//! proAVA knows (Times-like → „Times New Roman“, Courier-like → „Courier
//! New“, everything else → standard); the size most of the pasted text has
//! counts as the standard size, other sizes stay. Images in HTML are left
//! out (pasted pictures come as image data).

use proava_domain::complement_text::{field_char, field_index, has_fields};
use proava_domain::{
    font_class, normalize_runs, Align, Cell, ComplementKind, FontClass, RichText, Run, Table,
    TextBlock, TextComplement,
};

/// Text width the percentages of table widths refer to.
const TEXT_WIDTH_MM: f32 = 150.0;

// ---------- tokens ----------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        closed: bool,
    },
    Close(String),
    Text(String),
    /// Word: `<![if !supportLists]>` … `<![endif]>` holds the list marker.
    ListMarkerStart,
    ListMarkerEnd,
}

fn tokenize(html: &str) -> Vec<Token> {
    let chars: Vec<char> = html.chars().collect();
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let starts = |i: usize, pattern: &str| -> bool {
        pattern
            .chars()
            .enumerate()
            .all(|(k, p)| chars.get(i + k).is_some_and(|c| c.eq_ignore_ascii_case(&p)))
    };
    let find = |from: usize, pattern: &str| -> Option<usize> {
        (from..chars.len()).find(|&k| starts(k, pattern))
    };
    while i < chars.len() {
        if chars[i] != '<' {
            text.push(chars[i]);
            i += 1;
            continue;
        }
        if !text.is_empty() {
            tokens.push(Token::Text(std::mem::take(&mut text)));
        }
        if starts(i, "<!--") {
            i = find(i + 4, "-->").map_or(chars.len(), |end| end + 3);
            continue;
        }
        if starts(i, "<![if !supportlists]>") {
            tokens.push(Token::ListMarkerStart);
            i += "<![if !supportLists]>".len();
            continue;
        }
        if starts(i, "<![endif]>") {
            tokens.push(Token::ListMarkerEnd);
            i += "<![endif]>".len();
            continue;
        }
        if starts(i, "<!") || starts(i, "<?") {
            i = find(i, ">").map_or(chars.len(), |end| end + 1);
            continue;
        }
        // A tag: read up to the closing '>' outside quotes.
        let mut j = i + 1;
        let mut quote: Option<char> = None;
        while j < chars.len() {
            match (quote, chars[j]) {
                (Some(q), c) if c == q => quote = None,
                (None, '"' | '\'') => quote = Some(chars[j]),
                (None, '>') => break,
                _ => {}
            }
            j += 1;
        }
        let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
        i = j + 1;
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.split_whitespace().next().unwrap_or("");
            tokens.push(Token::Close(local(name)));
            continue;
        }
        let closed = inner.trim_end().ends_with('/');
        let inner = inner.trim_end().trim_end_matches('/');
        let name_end = inner
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inner.len());
        let name = local(&inner[..name_end]);
        if name.is_empty() {
            continue;
        }
        let attrs = parse_attrs(&inner[name_end..]);
        // Raw text elements: skip their content.
        if matches!(name.as_str(), "style" | "script" | "title" | "xml" | "head") && !closed {
            let end = format!("</{name}");
            i = find(i, &end)
                .and_then(|k| find(k, ">"))
                .map_or(chars.len(), |k| k + 1);
            continue;
        }
        tokens.push(Token::Open {
            name,
            attrs,
            closed,
        });
    }
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    tokens
}

/// Lower-case local name (`o:p` → `p` would be wrong, so keep prefixes).
fn local(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn parse_attrs(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut attrs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        let name: String = chars[start..i]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let quote = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        if !name.is_empty() {
            attrs.push((name, decode_entities(&value)));
        }
    }
    attrs
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace());
        let Some(end) = end.filter(|&end| rest.as_bytes().get(end + 1) == Some(&b';')) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end + 1];
        let decoded =
            if let Some(number) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(number, 16)
                    .ok()
                    .and_then(char::from_u32)
            } else if let Some(number) = name.strip_prefix('#') {
                number.parse().ok().and_then(char::from_u32)
            } else {
                named_entity(name)
            };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 2..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "Auml" => 'Ä',
        "Ouml" => 'Ö',
        "Uuml" => 'Ü',
        "szlig" => 'ß',
        "euro" => '€',
        "ndash" => '–',
        "mdash" => '—',
        "bdquo" => '„',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        "sbquo" => '‚',
        "hellip" => '…',
        "middot" => '·',
        "bull" => '•',
        "deg" => '°',
        "sup2" => '²',
        "sup3" => '³',
        "times" => '×',
        "plusmn" => '±',
        "frac12" => '½',
        "micro" => 'µ',
        "sect" => '§',
        "copy" => '©',
        "reg" => '®',
        "shy" => '\u{ad}',
        _ => return None,
    })
}

// ---------- styles ----------

#[derive(Debug, Clone, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    font: Option<String>,
    size_pt: Option<f32>,
    align: Option<Align>,
    /// Word: `mso-list` paragraph (list item).
    list: bool,
    /// Word: its level („mso-list:l0 level2 lfo1“ = 1).
    list_level: u8,
    hidden: bool,
}

/// Level of a Word list paragraph from its `mso-list` value.
fn mso_level(value: &str) -> u8 {
    value
        .split_whitespace()
        .find_map(|part| part.strip_prefix("level"))
        .and_then(|number| number.parse::<u8>().ok())
        .map_or(0, |level| level.saturating_sub(1))
        .min(proava_domain::MAX_LIST_LEVEL)
}

fn number(text: &str) -> Option<f32> {
    let value: f32 = text.trim().replace(',', ".").parse().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

fn parse_size(value: &str, base: f32) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let size = match value.as_str() {
        "xx-small" => 7.0,
        "x-small" => 7.5,
        "small" => 10.0,
        "medium" => 12.0,
        "large" => 13.5,
        "x-large" => 18.0,
        "xx-large" => 24.0,
        "smaller" => base * 0.83,
        "larger" => base * 1.2,
        _ => {
            if let Some(v) = value.strip_suffix("pt") {
                number(v)?
            } else if let Some(v) = value.strip_suffix("px") {
                number(v)? * 0.75
            } else if let Some(v) = value.strip_suffix("em") {
                number(v)? * base
            } else if let Some(v) = value.strip_suffix('%') {
                number(v)? * base / 100.0
            } else {
                number(&value)?
            }
        }
    };
    Some((size * 2.0).round() / 2.0)
}

fn parse_align(value: &str) -> Option<Align> {
    Align::from_css(value)
}

/// Width in mm: `184.25pt`, `246` (px), `5cm`, `30mm`, `25%`.
fn parse_width(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let mm = if let Some(v) = value.strip_suffix("mm") {
        number(v)?
    } else if let Some(v) = value.strip_suffix("cm") {
        number(v)? * 10.0
    } else if let Some(v) = value.strip_suffix("pt") {
        number(v)? * 25.4 / 72.0
    } else if let Some(v) = value.strip_suffix("in") {
        number(v)? * 25.4
    } else if let Some(v) = value.strip_suffix('%') {
        number(v)? * TEXT_WIDTH_MM / 100.0
    } else {
        number(value.strip_suffix("px").unwrap_or(&value))? * 25.4 / 96.0
    };
    Some((mm * 10.0).round() / 10.0)
}

/// The three families: standard (Sans) = `None`.
fn family(value: &str) -> Option<String> {
    let first = value
        .split(',')
        .next()?
        .trim()
        .trim_matches(['"', '\''])
        .trim();
    if first.is_empty() {
        return None;
    }
    font_class(Some(first)).stored_name().map(str::to_string)
}

fn css(style: Option<&str>) -> Vec<(String, String)> {
    style
        .unwrap_or("")
        .split(';')
        .filter_map(|declaration| {
            let (key, value) = declaration.split_once(':')?;
            Some((key.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

impl Style {
    fn of(&self, name: &str, attrs: &[(String, String)]) -> Style {
        let mut style = self.clone();
        // Block alignment does not pass into nested blocks via inline tags,
        // but paragraphs inherit it from a div or cell.
        match name {
            "b" | "strong" | "th" => style.bold = true,
            "i" | "em" | "cite" | "var" => style.italic = true,
            "u" | "ins" => style.underline = true,
            "h1" => {
                style.bold = true;
                style.size_pt = Some(16.0);
            }
            "h2" => {
                style.bold = true;
                style.size_pt = Some(14.0);
            }
            "h3" | "h4" | "h5" | "h6" => style.bold = true,
            "code" | "pre" | "tt" | "kbd" | "samp" => style.font = Some("Courier New".into()),
            "font" => {
                if let Some(face) = attr(attrs, "face") {
                    style.font = family(face);
                }
                if let Some(size) = attr(attrs, "size").and_then(|s| s.trim().parse::<u8>().ok()) {
                    style.size_pt = Some(match size {
                        1 => 8.0,
                        2 => 10.0,
                        3 => 12.0,
                        4 => 14.0,
                        5 => 18.0,
                        6 => 24.0,
                        _ => 36.0,
                    });
                }
            }
            _ => {}
        }
        if let Some(align) = attr(attrs, "align").and_then(parse_align) {
            style.align = Some(align);
        }
        if attr(attrs, "class").is_some_and(|class| class.to_ascii_lowercase().contains("msolist"))
        {
            style.list = true;
        }
        for (key, value) in css(attr(attrs, "style")) {
            let lower = value.to_ascii_lowercase();
            match key.as_str() {
                "font-weight" => {
                    style.bold = matches!(
                        lower.as_str(),
                        "bold" | "bolder" | "600" | "700" | "800" | "900"
                    )
                }
                "font-style" => style.italic = lower == "italic" || lower == "oblique",
                "text-decoration" | "text-decoration-line" | "text-underline" => {
                    style.underline = lower.contains("underline") || lower == "single"
                }
                "font-family" => style.font = family(&value),
                "font-size" => {
                    if let Some(size) = parse_size(&value, style.size_pt.unwrap_or(10.0)) {
                        style.size_pt = Some(size);
                    }
                }
                "text-align" => style.align = parse_align(&value).or(style.align),
                "mso-list" if !lower.contains("ignore") && lower != "none" => {
                    style.list = true;
                    style.list_level = mso_level(&lower);
                }
                "mso-list" => style.hidden = true,
                "display" if lower == "none" => style.hidden = true,
                "visibility" if lower == "hidden" => style.hidden = true,
                _ => {}
            }
        }
        style
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

// ---------- building ----------

#[derive(Debug)]
struct Open {
    runs: Vec<Run>,
    align: Align,
    /// List item: numbered?, level.
    list: Option<(bool, u8)>,
}

struct Builder {
    /// Textergänzungen read from `data-proava-feld` spans (None = read
    /// such spans as plain text).
    fields: Option<Vec<TextComplement>>,
    blocks: Vec<TextBlock>,
    open: Option<Open>,
    /// Ordered/unordered of the lists we are in.
    lists: Vec<bool>,
    /// The list items we are in (numbered?, level): text after a nested
    /// list continues as an item on that level.
    items: Vec<(bool, u8)>,
    preformatted: usize,
    /// Text inside Word's list marker (to see whether it is a number).
    marker: Option<String>,
}

impl Builder {
    fn new() -> Self {
        Self {
            fields: None,
            blocks: Vec::new(),
            open: None,
            lists: Vec::new(),
            items: Vec::new(),
            preformatted: 0,
            marker: None,
        }
    }

    fn start_para(&mut self, style: &Style, list: Option<(bool, u8)>) {
        self.close();
        self.open = Some(Open {
            runs: Vec::new(),
            align: style.align.unwrap_or_default(),
            list,
        });
    }

    fn text(&mut self, text: &str, style: &Style) {
        if style.hidden {
            return;
        }
        if let Some(marker) = &mut self.marker {
            marker.push_str(text);
            return;
        }
        let text = decode_entities(text);
        let text = if self.preformatted > 0 {
            text
        } else {
            let collapsed: String = text
                .chars()
                .map(|c| {
                    if c.is_whitespace() && c != '\u{a0}' {
                        ' '
                    } else {
                        c
                    }
                })
                .collect::<String>();
            let mut out = String::with_capacity(collapsed.len());
            for c in collapsed.chars() {
                if c == ' ' && out.ends_with(' ') {
                    continue;
                }
                out.push(c);
            }
            out
        };
        let text = text.replace('\u{a0}', " ");
        if text.is_empty() {
            return;
        }
        if self.open.is_none() {
            if text.trim().is_empty() {
                return;
            }
            self.start_para(style, self.items.last().copied());
        }
        let Some(Open { runs, .. }) = &mut self.open else {
            return;
        };
        let at_start = runs
            .iter()
            .all(|run| run.text.is_empty() || run.text.ends_with('\n'));
        let text = if at_start && self.preformatted == 0 {
            text.trim_start().to_string()
        } else {
            text
        };
        if !text.is_empty() {
            runs.push(style.run(&text));
        }
    }

    fn line_break(&mut self, style: &Style) {
        if self.open.is_none() {
            self.start_para(style, None);
        }
        let Some(Open { runs, .. }) = &mut self.open else {
            return;
        };
        if let Some(last) = runs.last_mut() {
            // No trailing space before a break.
            let trimmed = last.text.trim_end_matches(' ').len();
            last.text.truncate(trimmed);
        }
        runs.push(style.run("\n"));
    }

    fn close(&mut self) {
        let Some(Open {
            mut runs,
            align,
            list,
        }) = self.open.take()
        else {
            return;
        };
        // Trailing blanks.
        while let Some(last) = runs.last_mut() {
            let trimmed = last.text.trim_end_matches(' ').to_string();
            if trimmed.is_empty() {
                runs.pop();
            } else {
                last.text = trimmed;
                break;
            }
        }
        let runs = normalize_runs(runs);
        self.blocks.push(match list {
            Some((ordered, level)) => TextBlock::ListItem {
                ordered,
                level,
                runs,
                align,
            },
            None => TextBlock::Paragraph { runs, align },
        });
    }
}

fn read(
    tokens: &[Token],
    mut index: usize,
    style: &Style,
    stop: Option<&str>,
    builder: &mut Builder,
) -> usize {
    while index < tokens.len() {
        match &tokens[index] {
            Token::Text(text) => builder.text(text, style),
            Token::ListMarkerStart => builder.marker = Some(String::new()),
            Token::ListMarkerEnd => {
                if let Some(marker) = builder.marker.take() {
                    let ordered = decode_entities(&marker)
                        .chars()
                        .any(|c| c.is_ascii_alphanumeric());
                    if let Some(Open { list, .. }) = &mut builder.open {
                        let level = list.map_or(0, |(_, level)| level);
                        *list = Some((ordered, level));
                    }
                }
            }
            Token::Close(name) => {
                if stop == Some(name.as_str()) {
                    return index + 1;
                }
                // A stray close of an outer element ends this one too.
                if stop.is_some()
                    && (is_block(name)
                        || matches!(
                            name.as_str(),
                            "ul" | "ol" | "table" | "tr" | "td" | "th" | "li"
                        ))
                {
                    return index;
                }
            }
            Token::Open {
                name,
                attrs,
                closed,
            } => {
                let name = name.as_str();
                let inner = style.of(name, attrs);
                // A Bieterangabe from another proAVA: a field again.
                if let (Some(fields), Some(data)) =
                    (builder.fields.as_mut(), attr(attrs, FIELD_ATTR))
                {
                    if let Some(complement) = decode_field(data) {
                        fields.push(complement);
                        let character = field_char(fields.len() - 1).to_string();
                        builder.text(&character, &inner);
                        index = if *closed {
                            index + 1
                        } else {
                            skip_element(tokens, index + 1, name)
                        };
                        continue;
                    }
                }
                match name {
                    "br" => builder.line_break(style),
                    "img" | "o:p" | "v:shape" | "v:imagedata" => {}
                    "ul" | "ol" if !closed => {
                        builder.close();
                        builder.lists.push(name == "ol");
                        index = read(tokens, index + 1, &inner, Some(name), builder);
                        builder.close();
                        builder.lists.pop();
                        continue;
                    }
                    "li" if !closed => {
                        let ordered = builder.lists.last().copied().unwrap_or(false);
                        let level = u8::try_from(builder.lists.len().saturating_sub(1))
                            .unwrap_or(u8::MAX)
                            .min(proava_domain::MAX_LIST_LEVEL);
                        builder.start_para(&inner, Some((ordered, level)));
                        builder.items.push((ordered, level));
                        index = read(
                            tokens,
                            index + 1,
                            &Style {
                                align: None,
                                ..inner
                            },
                            Some("li"),
                            builder,
                        );
                        builder.close();
                        builder.items.pop();
                        continue;
                    }
                    "table" if !closed => {
                        builder.close();
                        let (table, next) = read_table(
                            tokens,
                            index + 1,
                            &Style {
                                align: None,
                                ..inner
                            },
                        );
                        if !table.rows.is_empty() {
                            builder.blocks.push(TextBlock::Table(table));
                        }
                        index = next;
                        continue;
                    }
                    "pre" if !closed => {
                        builder.start_para(&inner, None);
                        builder.preformatted += 1;
                        index = read(tokens, index + 1, &inner, Some("pre"), builder);
                        builder.preformatted -= 1;
                        builder.close();
                        continue;
                    }
                    _ if is_block(name) && !closed => {
                        // Within a list item a paragraph continues the item.
                        let in_item = matches!(builder.open, Some(Open { list: Some(_), .. }))
                            && stop == Some("li");
                        if in_item {
                            if let Some(Open { runs, align, .. }) = &mut builder.open {
                                if !runs.is_empty() {
                                    runs.push(Run::plain("\n"));
                                }
                                if let Some(a) = inner.align {
                                    *align = a;
                                }
                            }
                        } else {
                            // In a list item after a nested list: the item
                            // goes on; a Word list paragraph with its level.
                            let list = match builder.items.last() {
                                Some(item) if stop == Some("li") => Some(*item),
                                _ => inner.list.then_some((false, inner.list_level)),
                            };
                            builder.start_para(&inner, list);
                        }
                        index = read(
                            tokens,
                            index + 1,
                            &Style {
                                list: false,
                                ..inner
                            },
                            Some(name),
                            builder,
                        );
                        if !in_item {
                            builder.close();
                        }
                        continue;
                    }
                    _ if !closed && !is_void(name) => {
                        index = read(tokens, index + 1, &inner, Some(name), builder);
                        continue;
                    }
                    _ => {}
                }
            }
        }
        index += 1;
    }
    index
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "address"
            | "center"
            | "section"
            | "article"
    )
}

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "br" | "img"
            | "hr"
            | "meta"
            | "link"
            | "input"
            | "col"
            | "area"
            | "base"
            | "wbr"
            | "source"
    )
}

/// Reads rows up to `</table>`; returns the table and the next token.
fn read_table(tokens: &[Token], mut index: usize, style: &Style) -> (Table, usize) {
    let mut table = Table::default();
    let mut in_head = false;
    let mut head_done = false;
    while index < tokens.len() {
        match &tokens[index] {
            Token::Close(name) if name == "table" => {
                index += 1;
                break;
            }
            Token::Close(name) if name == "thead" => in_head = false,
            Token::Open { name, .. } if name == "thead" => in_head = true,
            Token::Open {
                name,
                attrs,
                closed: false,
            } if name == "tr" => {
                let row_style = style.of("tr", attrs);
                let (cells, all_head, next) = read_row(tokens, index + 1, &row_style, &mut table);
                if !cells.is_empty() {
                    if (in_head || all_head) && !head_done && table.header_rows == table.rows.len()
                    {
                        table.header_rows += 1;
                    } else {
                        head_done = true;
                    }
                    table.rows.push(cells);
                }
                index = next;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    let columns = table.column_count();
    for row in &mut table.rows {
        row.resize(columns, Cell::default());
    }
    if table.column_widths_mm.iter().all(Option::is_none) {
        table.column_widths_mm.clear();
    } else {
        table.column_widths_mm.resize(columns, None);
    }
    (table, index)
}

fn read_row(
    tokens: &[Token],
    mut index: usize,
    style: &Style,
    table: &mut Table,
) -> (Vec<Cell>, bool, usize) {
    let mut cells = Vec::new();
    let mut all_head = true;
    let first_row = table.rows.is_empty();
    while index < tokens.len() {
        match &tokens[index] {
            Token::Close(name) if name == "tr" => {
                let head = all_head && !cells.is_empty();
                return (cells, head, index + 1);
            }
            Token::Close(name) if name == "table" => {
                let head = all_head && !cells.is_empty();
                return (cells, head, index);
            }
            Token::Open {
                name,
                attrs,
                closed,
            } if name == "td" || name == "th" => {
                all_head &= name == "th";
                let cell_style = style.of(name, attrs);
                let span: usize = attr(attrs, "colspan")
                    .and_then(|s| s.trim().parse().ok())
                    .filter(|s| *s >= 1)
                    .unwrap_or(1)
                    .min(64);
                if first_row && span == 1 {
                    let width = css(attr(attrs, "style"))
                        .into_iter()
                        .find(|(key, _)| key == "width")
                        .and_then(|(_, value)| parse_width(&value))
                        .or_else(|| attr(attrs, "width").and_then(parse_width));
                    let column = cells.len();
                    if let Some(width) = width {
                        if table.column_widths_mm.len() <= column {
                            table.column_widths_mm.resize(column + 1, None);
                        }
                        table.column_widths_mm[column] = Some(width);
                    }
                }
                let mut builder = Builder::new();
                let next = if *closed {
                    index + 1
                } else {
                    read(
                        tokens,
                        index + 1,
                        &Style {
                            align: None,
                            ..cell_style.clone()
                        },
                        Some(name.as_str()),
                        &mut builder,
                    )
                };
                builder.close();
                let mut runs: Vec<Run> = Vec::new();
                let mut align = cell_style.align;
                for block in builder.blocks {
                    match block {
                        TextBlock::Paragraph {
                            runs: part,
                            align: a,
                        }
                        | TextBlock::ListItem {
                            runs: part,
                            align: a,
                            ..
                        } => {
                            if part.is_empty() {
                                continue;
                            }
                            if !runs.is_empty() {
                                runs.push(Run::plain("\n"));
                            }
                            if align.is_none() && a != Align::Left {
                                align = Some(a);
                            }
                            runs.extend(part);
                        }
                        TextBlock::Table(inner) => {
                            for line in inner.texts() {
                                if !runs.is_empty() {
                                    runs.push(Run::plain("\n"));
                                }
                                runs.push(Run::plain(line.join("  ")));
                            }
                        }
                        TextBlock::Image { .. } => {}
                    }
                }
                cells.push(Cell {
                    runs: normalize_runs(runs),
                    align: align.unwrap_or_default(),
                });
                for _ in 1..span {
                    cells.push(Cell::default());
                }
                index = next;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    let head = all_head && !cells.is_empty();
    (cells, head, index)
}

/// Clipboard HTML → text. Only the marked fragment counts when there is one.
pub fn from_html(html: &str) -> RichText {
    read_html(html, false).0
}

/// Like [`from_html`]; spans of Bieterangaben copied in proAVA
/// (`data-proava-feld`) become field chars (index into the list).
pub fn from_html_with_fields(html: &str) -> (RichText, Vec<TextComplement>) {
    read_html(html, true)
}

fn read_html(html: &str, with_fields: bool) -> (RichText, Vec<TextComplement>) {
    // Windows "HTML Format" starts with a header (Version:0.9 StartHTML:…).
    let mut html = if html.trim_start().starts_with("Version:") {
        &html[html.find('<').unwrap_or(html.len())..]
    } else {
        html
    };
    if let (Some(from), Some(to)) = (
        html.find("<!--StartFragment-->"),
        html.find("<!--EndFragment-->"),
    ) {
        if from < to {
            html = &html[from + "<!--StartFragment-->".len()..to];
        }
    }
    let tokens = tokenize(html);
    let mut builder = Builder::new();
    if with_fields {
        builder.fields = Some(Vec::new());
    }
    read(&tokens, 0, &Style::default(), None, &mut builder);
    builder.close();
    let fields = builder.fields.take().unwrap_or_default();
    let mut text = RichText::new(builder.blocks);
    // Empty paragraphs at the ends are left over from markup.
    let blocks = text.blocks_mut();
    while matches!(blocks.last(), Some(TextBlock::Paragraph { runs, .. }) if runs.is_empty()) {
        blocks.pop();
    }
    while matches!(blocks.first(), Some(TextBlock::Paragraph { runs, .. }) if runs.is_empty()) {
        blocks.remove(0);
    }
    standardize_size(&mut text);
    (text, fields)
}

/// Index after the element opened before `from` (nested ones of the same
/// name counted).
fn skip_element(tokens: &[Token], from: usize, name: &str) -> usize {
    let mut depth = 1;
    let mut index = from;
    while index < tokens.len() {
        match &tokens[index] {
            Token::Open {
                name: open,
                closed: false,
                ..
            } if open == name => depth += 1,
            Token::Close(close) if close == name => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    index
}

/// Attribute of a field span in the clipboard HTML: the Textergänzung as
/// „kind|Kennung|Vortext|Eintrag|Nachtext“, each part percent-encoded.
const FIELD_ATTR: &str = "data-proava-feld";

fn encode_part(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(character, '%' | '|' | '"' | '\'' | '<' | '>' | '&') || character.is_control() {
            let mut buffer = [0u8; 4];
            for byte in character.encode_utf8(&mut buffer).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn decode_part(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub(super) fn encode_field(complement: &TextComplement) -> String {
    let kind = match complement.kind {
        ComplementKind::Bidder => "B",
        ComplementKind::Owner => "A",
    };
    [
        kind,
        &complement.marker,
        &complement.caption,
        &complement.body,
        &complement.tail,
    ]
    .iter()
    .map(|part| encode_part(part))
    .collect::<Vec<_>>()
    .join("|")
}

fn decode_field(data: &str) -> Option<TextComplement> {
    let parts: Vec<&str> = data.split('|').collect();
    let [kind, marker, caption, body, tail] = parts.as_slice() else {
        return None;
    };
    Some(TextComplement {
        kind: match *kind {
            "A" => ComplementKind::Owner,
            "B" => ComplementKind::Bidder,
            _ => return None,
        },
        marker: decode_part(marker)?,
        caption: decode_part(caption)?,
        body: decode_part(body)?,
        tail: decode_part(tail)?,
        anchor: None,
    })
}

/// The size most chars have counts as the standard size (no size).
fn standardize_size(text: &mut RichText) {
    let mut counts: Vec<(Option<u32>, usize)> = Vec::new();
    let mut each = |runs: &[Run]| {
        for run in runs {
            let key = run.size_pt.map(|s| (s * 10.0) as u32);
            let n = run.text.chars().count();
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, count)) => *count += n,
                None => counts.push((key, n)),
            }
        }
    };
    for block in text.blocks() {
        match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => each(runs),
            TextBlock::Table(table) => table
                .rows
                .iter()
                .flatten()
                .for_each(|cell| each(&cell.runs)),
            TextBlock::Image { .. } => {}
        }
    }
    let Some((Some(dominant), _)) = counts.into_iter().max_by_key(|(_, n)| *n) else {
        return;
    };
    let fix = |runs: &mut Vec<Run>| {
        for run in runs.iter_mut() {
            if run.size_pt.map(|s| (s * 10.0) as u32) == Some(dominant) {
                run.size_pt = None;
            }
        }
        *runs = normalize_runs(std::mem::take(runs));
    };
    for block in text.blocks_mut() {
        match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => fix(runs),
            TextBlock::Table(table) => table
                .rows
                .iter_mut()
                .flatten()
                .for_each(|cell| fix(&mut cell.runs)),
            TextBlock::Image { .. } => {}
        }
    }
}

// ---------- writing ----------

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `before`/`after`: the chars around the run (fields are set off by
/// spaces like in the PDF).
fn run_html(
    run: &Run,
    fields: &[TextComplement],
    before: Option<char>,
    after: Option<char>,
    out: &mut String,
) {
    if fields.is_empty() || !has_fields(&run.text) {
        return plain_run_html(run, out);
    }
    // Field chars as spans that carry their Textergänzung; other programs
    // show the printed form of the field.
    let characters: Vec<char> = run.text.chars().collect();
    let mut piece = String::new();
    let mut last = before;
    for (at, character) in characters.iter().enumerate() {
        let Some(complement) = field_index(*character).and_then(|index| fields.get(index)) else {
            piece.push(*character);
            last = Some(*character);
            continue;
        };
        if !piece.is_empty() {
            plain_run_html(&run.with_text(std::mem::take(&mut piece)), out);
        }
        // The spaces inside the span: another proAVA turns the span back
        // into the field, other programs keep them.
        let next = characters.get(at + 1).copied().or(after);
        let label = super::fields::outward_of(complement, last, next);
        let mut inner = String::new();
        plain_run_html(&run.with_text(label.clone()), &mut inner);
        out.push_str(&format!(
            "<span {FIELD_ATTR}=\"{}\">{inner}</span>",
            encode_field(complement)
        ));
        last = label.chars().last().or(last);
    }
    if !piece.is_empty() {
        plain_run_html(&run.with_text(piece), out);
    }
}

fn plain_run_html(run: &Run, out: &mut String) {
    let mut css = String::new();
    if let Some(font) = &run.font {
        let name = match font_class(Some(font)) {
            FontClass::Serif => "'Times New Roman', serif".to_string(),
            FontClass::Mono => "'Courier New', monospace".to_string(),
            FontClass::Sans => format!(
                "'{}', Arial, sans-serif",
                font.replace(['\'', '"', ';'], "")
            ),
        };
        css.push_str(&format!("font-family:{name};"));
    }
    if let Some(size) = run.size_pt {
        css.push_str(&format!("font-size:{size}pt;"));
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
    let text = escape(&run.text).replace('\n', "<br>");
    if css.is_empty() {
        out.push_str(&text);
    } else {
        out.push_str(&format!("<span style=\"{css}\">{text}</span>"));
    }
}

fn runs_html(runs: &[Run], fields: &[TextComplement], out: &mut String) {
    for (index, run) in runs.iter().enumerate() {
        let before = runs[..index]
            .iter()
            .rev()
            .find_map(|run| run.text.chars().last());
        let after = runs[index + 1..]
            .iter()
            .find_map(|run| run.text.chars().next());
        run_html(run, fields, before, after, out);
    }
}

fn align_attr(align: Align) -> String {
    if align == Align::Left {
        String::new()
    } else {
        format!(" style=\"text-align:{}\"", align.css())
    }
}

/// List items as nested HTML lists: a deeper level stands in the `<li>`
/// of the item before (as word processors and GAEB write it); a level
/// without an item of its own gets an empty `<li>` to hold it.
fn list_html(items: &[TextBlock], fields: &[TextComplement], out: &mut String) {
    fn tag(ordered: bool) -> &'static str {
        if ordered {
            "ol"
        } else {
            "ul"
        }
    }
    // Open lists, innermost last: numbered?, an `<li>` open in it?
    let mut open: Vec<(bool, bool)> = Vec::new();
    let close_list = |open: &mut Vec<(bool, bool)>, out: &mut String| {
        if let Some((ordered, item)) = open.pop() {
            if item {
                out.push_str("</li>");
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
        let level = usize::from((*level).min(proava_domain::MAX_LIST_LEVEL));
        while open.len() > level + 1 {
            close_list(&mut open, out);
        }
        if open.len() == level + 1 {
            if open[level].0 != *ordered {
                close_list(&mut open, out);
            } else if open[level].1 {
                out.push_str("</li>");
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
            let list_ordered = open.len() == level && *ordered;
            out.push_str(&format!("<{}>", tag(list_ordered)));
            open.push((list_ordered, false));
        }
        out.push_str(&format!("<li{}>", align_attr(*align)));
        runs_html(runs, fields, out);
        open[level].1 = true;
    }
    while !open.is_empty() {
        close_list(&mut open, out);
    }
}

/// Text → clipboard HTML (fragment in a minimal document).
pub fn to_html(text: &RichText) -> String {
    to_html_with_fields(text, &[])
}

/// Like [`to_html`] for a text with field chars: each field becomes a span
/// with its text, carrying the Textergänzung for another proAVA.
pub fn to_html_with_fields(text: &RichText, fields: &[TextComplement]) -> String {
    let mut out = String::from("<html><body><!--StartFragment-->");
    let blocks = text.blocks();
    let mut index = 0;
    while index < blocks.len() {
        match &blocks[index] {
            TextBlock::Paragraph { runs, align } => {
                out.push_str(&format!("<p{}>", align_attr(*align)));
                runs_html(runs, fields, &mut out);
                out.push_str("</p>");
            }
            TextBlock::ListItem { .. } => {
                let end = blocks[index..]
                    .iter()
                    .position(|block| !matches!(block, TextBlock::ListItem { .. }))
                    .map_or(blocks.len(), |offset| index + offset);
                list_html(&blocks[index..end], fields, &mut out);
                index = end;
                continue;
            }
            TextBlock::Table(table) => {
                out.push_str("<table border=\"1\" style=\"border-collapse:collapse\">");
                for (row_index, cells) in table.rows.iter().enumerate() {
                    out.push_str("<tr>");
                    let tag = if row_index < table.header_rows {
                        "th"
                    } else {
                        "td"
                    };
                    for (column, cell) in cells.iter().enumerate() {
                        let mut style = String::new();
                        if cell.align != Align::Left {
                            style.push_str(&format!("text-align:{};", cell.align.css()));
                        }
                        if row_index == 0 {
                            if let Some(width) = table.column_width(column) {
                                style.push_str(&format!("width:{width}mm;"));
                            }
                        }
                        if style.is_empty() {
                            out.push_str(&format!("<{tag}>"));
                        } else {
                            out.push_str(&format!("<{tag} style=\"{style}\">"));
                        }
                        runs_html(&cell.runs, fields, &mut out);
                        out.push_str(&format!("</{tag}>"));
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
            TextBlock::Image { .. } => {}
        }
        index += 1;
    }
    out.push_str("<!--EndFragment--></body></html>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold(text: &str) -> Run {
        Run::bold(text)
    }

    fn kinds(text: &RichText) -> Vec<(String, Option<(bool, u8)>)> {
        text.blocks()
            .iter()
            .map(|block| {
                (
                    block
                        .runs()
                        .map(proava_domain::runs_text)
                        .unwrap_or_default(),
                    block.list_kind(),
                )
            })
            .collect()
    }

    /// Unterpunkte from the clipboard: Word gives the level in `mso-list`
    /// („level2“), LibreOffice and browsers nest the lists.
    #[test]
    fn nested_lists_and_word_levels_become_unterpunkte() {
        let word = r#"<html><body><!--StartFragment-->
<p class=MsoListParagraphCxSpFirst style='mso-list:l0 level1 lfo1'><![if !supportLists]><span>1.<span>&nbsp;</span></span><![endif]>Automation</p>
<p class=MsoListParagraphCxSpMiddle style='margin-left:72.0pt;mso-list:l0 level2 lfo1'><![if !supportLists]><span>a.<span>&nbsp;</span></span><![endif]>Parameter</p>
<p class=MsoListParagraphCxSpLast style='margin-left:108.0pt;mso-list:l0 level3 lfo1'><![if !supportLists]><span style='font-family:Wingdings'>§<span>&nbsp;</span></span><![endif]>Alarme</p>
<!--EndFragment--></body></html>"#;
        assert_eq!(
            kinds(&from_html(word)),
            vec![
                ("Automation".to_string(), Some((true, 0))),
                ("Parameter".to_string(), Some((true, 1))),
                // A Wingdings sign is a bullet.
                ("Alarme".to_string(), Some((false, 2))),
            ]
        );
        let nested =
            "<ul><li><p>Automation</p><ul><li><p>Parameter</p></li></ul><p>weiter</p></li>\
                      <li>Leitebene</li></ul><ol><li>eins<ol><li>a</li></ol></li></ol>";
        let text = from_html(nested);
        assert_eq!(
            kinds(&text),
            vec![
                ("Automation".to_string(), Some((false, 0))),
                ("Parameter".to_string(), Some((false, 1))),
                ("weiter".to_string(), Some((false, 0))),
                ("Leitebene".to_string(), Some((false, 0))),
                ("eins".to_string(), Some((true, 0))),
                ("a".to_string(), Some((true, 1))),
            ]
        );
        // Copied out as nested lists, read back the same.
        let html = to_html(&text);
        assert!(
            html.contains("<li>Automation<ul><li>Parameter</li></ul></li>"),
            "{html}"
        );
        assert_eq!(kinds(&from_html(&html)), kinds(&text));
    }

    #[test]
    fn word_paragraphs_lists_and_fonts() {
        // Shortened from what Word puts on the clipboard (Version/StartHTML header,
        // conditional comments, mso styles, 11 pt Calibri body).
        let html = r#"Version:0.9
StartHTML:0000000105
<html xmlns:o="urn:schemas-microsoft-com:office:office"><head><style><!-- p.MsoNormal {margin:0cm;} --></style></head>
<body lang=DE><!--StartFragment-->
<p class=MsoNormal align=center style='text-align:center'><b><span style='font-size:14.0pt;font-family:"Times New Roman",serif'>Leistungsumfang<o:p></o:p></span></b></p>
<p class=MsoNormal><span style='font-size:11.0pt;font-family:"Calibri",sans-serif'>Liefern und
montieren, <i>kursiv</i> und <u>unterstrichen</u>.&nbsp;Ende<o:p></o:p></span></p>
<p class=MsoListParagraphCxSpFirst style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'><![if !supportLists]><span style='font-family:Symbol'>·<span style='font:7.0pt "Times New Roman"'>&nbsp;&nbsp; </span></span><![endif]><span style='font-size:11.0pt'>Erster Punkt</span></p>
<p class=MsoListParagraphCxSpLast style='text-indent:-18.0pt;mso-list:l1 level1 lfo2'><![if !supportLists]><span>1.<span>&nbsp;&nbsp;</span></span><![endif]><span style='font-size:11.0pt'>Nummer eins</span></p>
<p class=MsoNormal style='text-align:justify'><span style='font-size:11.0pt;font-family:"Courier New"'>Code</span></p>
<!--EndFragment--></body></html>"#;
        let text = from_html(html);
        let blocks = text.blocks();
        assert_eq!(blocks.len(), 5, "{blocks:#?}");
        let TextBlock::Paragraph { runs, align } = &blocks[0] else {
            panic!()
        };
        assert_eq!(*align, Align::Center);
        assert_eq!(runs[0].font.as_deref(), Some("Times New Roman"));
        assert_eq!(runs[0].size_pt, Some(14.0));
        assert!(runs[0].bold);
        // 11 pt is the body size → standard; Calibri → standard family.
        assert_eq!(
            blocks[1],
            TextBlock::paragraph(vec![
                Run::plain("Liefern und montieren, "),
                Run {
                    italic: true,
                    ..Run::plain("kursiv")
                },
                Run::plain(" und "),
                Run {
                    underline: true,
                    ..Run::plain("unterstrichen")
                },
                Run::plain(". Ende"),
            ])
        );
        assert_eq!(
            blocks[2],
            TextBlock::list_item(false, vec![Run::plain("Erster Punkt")])
        );
        assert_eq!(
            blocks[3],
            TextBlock::list_item(true, vec![Run::plain("Nummer eins")])
        );
        let TextBlock::Paragraph { runs, align } = &blocks[4] else {
            panic!()
        };
        assert_eq!(
            (*align, runs[0].font.as_deref()),
            (Align::Justify, Some("Courier New"))
        );
    }

    #[test]
    fn libreoffice_lists_tables_and_font_tags() {
        let html = r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.0 Transitional//EN"><html><head><meta http-equiv="content-type" content="text/html; charset=utf-8"/><title></title></head>
<body lang="de-DE" dir="ltr"><p align="right" style="margin-bottom: 0cm"><font face="Liberation Serif, serif">rechts</font></p>
<ul><li><p style="margin-bottom: 0cm">Punkt <b>fett</b></p></li><li><p>zwei</p></li></ul>
<ol><li><p>eins</p></li></ol>
<table width="100%" cellpadding="4" cellspacing="0"><col width="128*"/><col width="128*"/>
<tr valign="top"><td width="50%" style="border: 1px solid #000000"><p><b>Typ</b></p></td><td width="50%"><p>Wert</p></td></tr>
<tr valign="top"><td><p>DI</p></td><td><p align="right">48</p></td></tr></table>
<p>&auml;&#246;&#xFC; &amp; &lt;tag&gt;</p></body></html>"#;
        let text = from_html(html);
        let blocks = text.blocks();
        assert_eq!(
            blocks[0],
            TextBlock::Paragraph {
                runs: vec![Run {
                    font: Some("Times New Roman".into()),
                    ..Run::plain("rechts")
                }],
                align: Align::Right
            }
        );
        assert_eq!(
            blocks[1],
            TextBlock::list_item(false, vec![Run::plain("Punkt "), bold("fett")])
        );
        assert_eq!(
            blocks[2],
            TextBlock::list_item(false, vec![Run::plain("zwei")])
        );
        assert_eq!(
            blocks[3],
            TextBlock::list_item(true, vec![Run::plain("eins")])
        );
        let TextBlock::Table(table) = &blocks[4] else {
            panic!("{blocks:#?}")
        };
        assert_eq!(table.texts(), vec![vec!["Typ", "Wert"], vec!["DI", "48"]]);
        assert_eq!(table.rows[1][1].align, Align::Right);
        assert_eq!(table.column_widths_mm, vec![Some(75.0), Some(75.0)]);
        assert_eq!(blocks[5], TextBlock::text("äöü & <tag>"));
    }

    #[test]
    fn browser_fragment_with_breaks_head_rows_and_spans() {
        let html = "<meta charset='utf-8'><html><body><!--StartFragment--><h2>Titel</h2>\
            <div style=\"text-align: center;\">Zeile 1<br>Zeile   2</div>\
            <table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td colspan=\"2\">breit</td></tr></tbody></table>\
            <span style=\"font-weight:700\">nur Span</span><!--EndFragment--></body></html>";
        let text = from_html(html);
        let blocks = text.blocks();
        let TextBlock::Paragraph { runs, .. } = &blocks[0] else {
            panic!()
        };
        assert!(runs[0].bold);
        assert_eq!(
            blocks[1],
            TextBlock::Paragraph {
                runs: vec![Run::plain("Zeile 1\nZeile 2")],
                align: Align::Center
            }
        );
        let TextBlock::Table(table) = &blocks[2] else {
            panic!()
        };
        assert_eq!(table.header_rows, 1);
        assert_eq!(table.texts()[1], vec!["breit", ""]);
        assert_eq!(blocks[3], TextBlock::paragraph(vec![bold("nur Span")]));
    }

    #[test]
    fn written_html_reads_back() {
        let text = RichText::new(vec![
            TextBlock::Paragraph {
                runs: vec![
                    Run {
                        font: Some("Times New Roman".into()),
                        size_pt: Some(14.0),
                        ..bold("Titel")
                    },
                    Run::plain(" & <mehr>"),
                ],
                align: Align::Center,
            },
            TextBlock::paragraph(vec![
                Run::plain("normal "),
                Run {
                    italic: true,
                    underline: true,
                    ..Run::plain("ku\nrsiv")
                },
            ]),
            TextBlock::list_item(true, vec![Run::plain("eins")]),
            TextBlock::list_item(true, vec![Run::plain("zwei")]),
            TextBlock::Table(Table {
                rows: vec![
                    vec![Cell::plain("Kopf"), Cell::plain("B")],
                    vec![
                        Cell {
                            runs: vec![Run::plain("1")],
                            align: Align::Right,
                        },
                        Cell::plain("x"),
                    ],
                ],
                header_rows: 1,
                column_widths_mm: vec![Some(30.0), None],
            }),
        ]);
        let html = to_html(&text);
        assert!(html.contains("<p style=\"text-align:center\">"), "{html}");
        let mut back = from_html(&html);
        // Head cells come back bold (th).
        if let TextBlock::Table(table) = &mut back.blocks_mut()[4] {
            for cell in &mut table.rows[0] {
                for run in &mut cell.runs {
                    run.bold = false;
                }
            }
        }
        assert_eq!(back, text);
    }

    #[test]
    fn plain_text_and_junk_do_not_panic() {
        assert!(from_html("").is_empty());
        assert_eq!(from_html("einfach Text").to_plain(), "einfach Text");
        assert_eq!(
            from_html("<p>a &unknown; & b</p><p").to_plain(),
            "a &unknown; & b"
        );
        let _ = from_html("<table><tr><td>offen");
        let _ = from_html("</p></div><li>x");
    }
}
