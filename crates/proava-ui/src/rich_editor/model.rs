//! Editing logic of the rich text editor, without egui: a text is a list of
//! blocks; a paragraph keeps its text and one format per character, so that
//! typing, splitting, joining and formatting a range are plain list work.

use proava_domain::{normalize_runs, Align, Cell, RichText, Run, Table, TextBlock, MAX_LIST_LEVEL};

/// Character format (a [`Run`] without text).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub font: Option<String>,
    pub size_pt: Option<f32>,
}

impl Fmt {
    pub fn of(run: &Run) -> Self {
        Self {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            font: run.font.clone(),
            size_pt: run.size_pt,
        }
    }

    pub fn run(&self, text: String) -> Run {
        Run {
            text,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            font: self.font.clone(),
            size_pt: self.size_pt,
        }
    }
}

/// What a paragraph is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParaKind {
    #[default]
    Text,
    Bullet,
    Numbered,
}

/// One paragraph: `fmts` has exactly one entry per char of `text`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Para {
    pub text: String,
    pub fmts: Vec<Fmt>,
    pub kind: ParaKind,
    /// List level (0 = outermost; only for list items).
    pub level: u8,
    pub align: Align,
    /// Format for text typed into an empty paragraph.
    pub empty_fmt: Fmt,
}

/// A formatting change for a range.
#[derive(Debug, Clone, PartialEq)]
pub enum FmtChange {
    /// Bold on when not every char is bold, else off (same for the others).
    ToggleBold,
    ToggleItalic,
    ToggleUnderline,
    Font(Option<String>),
    Size(Option<f32>),
    /// „normal“: all character formatting removed.
    Clear,
}

impl FmtChange {
    fn apply(&self, fmt: &mut Fmt, all: &AllSet) {
        match self {
            FmtChange::ToggleBold => fmt.bold = !all.bold,
            FmtChange::ToggleItalic => fmt.italic = !all.italic,
            FmtChange::ToggleUnderline => fmt.underline = !all.underline,
            FmtChange::Font(font) => fmt.font = font.clone(),
            FmtChange::Size(size) => fmt.size_pt = *size,
            FmtChange::Clear => *fmt = Fmt::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AllSet {
    bold: bool,
    italic: bool,
    underline: bool,
}

impl AllSet {
    fn of<'a>(fmts: impl Iterator<Item = &'a Fmt> + Clone) -> Self {
        let any = fmts.clone().next().is_some();
        Self {
            bold: any && fmts.clone().all(|f| f.bold),
            italic: any && fmts.clone().all(|f| f.italic),
            underline: any && fmts.clone().all(|f| f.underline),
        }
    }
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn byte_of(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

impl Para {
    pub fn new(kind: ParaKind, align: Align) -> Self {
        Self {
            kind,
            align,
            ..Self::default()
        }
    }

    pub fn from_runs(runs: &[Run], kind: ParaKind, align: Align) -> Self {
        let mut para = Self::new(kind, align);
        for run in runs {
            let fmt = Fmt::of(run);
            for ch in run.text.chars() {
                para.text.push(ch);
                para.fmts.push(fmt.clone());
            }
        }
        if let Some(run) = runs.first() {
            para.empty_fmt = Fmt::of(run);
        }
        para
    }

    pub fn plain(text: &str) -> Self {
        Self::from_runs(&[Run::plain(text)], ParaKind::Text, Align::Left)
    }

    pub fn len(&self) -> usize {
        self.fmts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fmts.is_empty()
    }

    pub fn to_runs(&self) -> Vec<Run> {
        // Chars of one format in a row make one run at once (normalize_runs
        // would join them anyway), not one string per char.
        let mut runs = Vec::new();
        let mut piece = String::new();
        let mut current: Option<&Fmt> = None;
        for (ch, fmt) in self.text.chars().zip(&self.fmts) {
            if let Some(last) = current.filter(|last| *last != fmt) {
                runs.push(last.run(std::mem::take(&mut piece)));
            }
            current = Some(fmt);
            piece.push(ch);
        }
        if let Some(last) = current {
            runs.push(last.run(piece));
        }
        normalize_runs(runs)
    }

    /// No visible char (what `RichText::is_empty` asks of the runs).
    fn is_blank(&self) -> bool {
        self.text
            .chars()
            .take(self.fmts.len())
            .all(char::is_whitespace)
    }

    /// Format that typing at `caret` produces: that of the char to the left,
    /// at the start that of the first char.
    pub fn fmt_at(&self, caret: usize) -> Fmt {
        if self.fmts.is_empty() {
            return self.empty_fmt.clone();
        }
        let index = caret.min(self.fmts.len()).saturating_sub(1);
        self.fmts[index].clone()
    }

    /// Format shown in the toolbar for a selection (or the caret).
    pub fn fmt_of_range(&self, start: usize, end: usize) -> Fmt {
        let (start, end) = (start.min(end), start.max(end).min(self.len()));
        if start >= end {
            return self.fmt_at(start);
        }
        let slice = &self.fmts[start..end];
        let all = AllSet::of(slice.iter());
        let first = &slice[0];
        Fmt {
            bold: all.bold,
            italic: all.italic,
            underline: all.underline,
            font: slice
                .iter()
                .all(|f| f.font == first.font)
                .then(|| first.font.clone())
                .flatten(),
            size_pt: slice
                .iter()
                .all(|f| f.size_pt == first.size_pt)
                .then_some(first.size_pt)
                .flatten(),
        }
    }

    /// Takes the text after a plain edit in a text field: the changed middle
    /// is found by comparing with the old text; new chars get `typed` or the
    /// format to the left of the change. Returns the char range inserted.
    pub fn apply_edit(&mut self, new_text: &str, typed: Option<&Fmt>) -> (usize, usize) {
        let old: Vec<char> = self.text.chars().collect();
        let new: Vec<char> = new_text.chars().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let max_suffix = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        let removed = prefix..old.len() - suffix;
        let inserted = new.len() - suffix - prefix;
        let fmt = typed.cloned().unwrap_or_else(|| {
            if prefix == 0 && removed.is_empty() && !self.fmts.is_empty() {
                self.fmts[0].clone()
            } else if !removed.is_empty() {
                self.fmts[removed.start].clone()
            } else {
                self.fmt_at(prefix)
            }
        });
        if self.fmts.is_empty() || (removed.start == 0 && removed.end == old.len()) {
            self.empty_fmt = fmt.clone();
        }
        self.fmts
            .splice(removed, std::iter::repeat_n(fmt, inserted));
        self.text = new_text.to_string();
        (prefix, prefix + inserted)
    }

    /// Applies a format change to chars `start..end`.
    pub fn format_range(&mut self, start: usize, end: usize, change: &FmtChange) {
        let (start, end) = (start.min(end), start.max(end).min(self.len()));
        if start >= end {
            return;
        }
        let all = AllSet::of(self.fmts[start..end].iter());
        for fmt in &mut self.fmts[start..end] {
            change.apply(fmt, &all);
        }
    }

    /// Format that results from applying `change` to `fmt` alone (for typing
    /// after a toolbar click without selection).
    pub fn changed_fmt(fmt: &Fmt, change: &FmtChange) -> Fmt {
        let mut out = fmt.clone();
        change.apply(&mut out, &AllSet::of(std::iter::once(fmt)));
        out
    }

    /// Splits at `at` (char index); the second part keeps kind and alignment.
    pub fn split(&mut self, at: usize) -> Para {
        let at = at.min(self.len());
        let byte = byte_of(&self.text, at);
        let rest_text = self.text.split_off(byte);
        let rest_fmts = self.fmts.split_off(at);
        let empty_fmt = rest_fmts
            .first()
            .cloned()
            .unwrap_or_else(|| self.fmt_at(at));
        Para {
            text: rest_text,
            fmts: rest_fmts,
            kind: self.kind,
            level: self.level,
            align: self.align,
            empty_fmt,
        }
    }

    /// Appends `other`; returns the char index where it starts.
    pub fn join(&mut self, other: Para) -> usize {
        let at = self.len();
        self.text.push_str(&other.text);
        self.fmts.extend(other.fmts);
        at
    }

    /// Positions of line breaks typed into the text (`\n`).
    pub fn break_positions(&self) -> Vec<usize> {
        self.text
            .chars()
            .enumerate()
            .filter(|(_, ch)| *ch == '\n')
            .map(|(index, _)| index)
            .collect()
    }

    /// Removes the char at `index` (a typed `\n` before splitting there).
    pub fn remove_char(&mut self, index: usize) {
        if index < self.len() {
            let byte = byte_of(&self.text, index);
            self.text.remove(byte);
            self.fmts.remove(index);
        }
    }

    /// Replaces the whole text and formats (e.g. after a table cell edit).
    pub fn text_len(&self) -> usize {
        char_len(&self.text)
    }
}

/// Table cells are paragraphs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableModel {
    pub rows: Vec<Vec<Para>>,
    pub header_rows: usize,
    pub widths_mm: Vec<Option<f32>>,
}

impl TableModel {
    pub fn new(rows: usize, columns: usize) -> Self {
        Self {
            rows: vec![vec![Para::default(); columns.max(1)]; rows.max(1)],
            header_rows: 0,
            widths_mm: Vec::new(),
        }
    }

    pub fn columns(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Cell after (Tab) or before (Umschalt+Tab) `cell`, row by row; `None`
    /// after the last cell (Tab there adds a row) and before the first.
    pub fn tab_target(&self, (row, column): (usize, usize), back: bool) -> Option<(usize, usize)> {
        let columns = self.columns().max(1);
        let index = row * columns + column.min(columns - 1);
        let target = if back {
            index.checked_sub(1)?
        } else {
            index + 1
        };
        (target < self.rows.len() * columns).then_some((target / columns, target % columns))
    }

    /// Every row as wide as the widest; widths list as long as the columns.
    pub fn even(&mut self) {
        let columns = self.columns().max(1);
        for row in &mut self.rows {
            row.resize(columns, Para::default());
        }
        if self.widths_mm.iter().all(Option::is_none) {
            self.widths_mm.clear();
        } else {
            self.widths_mm.resize(columns, None);
        }
        self.header_rows = self.header_rows.min(self.rows.len());
    }

    pub fn insert_row(&mut self, at: usize) {
        let columns = self.columns().max(1);
        let at = at.min(self.rows.len());
        self.rows.insert(at, vec![Para::default(); columns]);
        if at < self.header_rows {
            self.header_rows += 1;
        }
    }

    pub fn remove_row(&mut self, at: usize) -> bool {
        if self.rows.len() <= 1 || at >= self.rows.len() {
            return false;
        }
        self.rows.remove(at);
        if at < self.header_rows {
            self.header_rows -= 1;
        }
        true
    }

    pub fn insert_column(&mut self, at: usize) {
        self.even();
        for row in &mut self.rows {
            let at = at.min(row.len());
            row.insert(at, Para::default());
        }
        if !self.widths_mm.is_empty() {
            let at = at.min(self.widths_mm.len());
            self.widths_mm.insert(at, None);
        }
    }

    pub fn remove_column(&mut self, at: usize) -> bool {
        self.even();
        if self.columns() <= 1 || at >= self.columns() {
            return false;
        }
        for row in &mut self.rows {
            row.remove(at);
        }
        if at < self.widths_mm.len() {
            self.widths_mm.remove(at);
        }
        self.even();
        true
    }

    pub fn set_width(&mut self, column: usize, width_mm: Option<f32>) {
        self.even();
        let columns = self.columns();
        if column >= columns {
            return;
        }
        self.widths_mm.resize(columns, None);
        self.widths_mm[column] = width_mm.map(|mm| (mm * 10.0).round() / 10.0);
        self.even();
    }

    fn from_table(table: &Table) -> Self {
        let mut model = Self {
            rows: table
                .rows
                .iter()
                .map(|cells| {
                    cells
                        .iter()
                        .map(|cell| Para::from_runs(&cell.runs, ParaKind::Text, cell.align))
                        .collect()
                })
                .collect(),
            header_rows: table.header_rows,
            widths_mm: table.column_widths_mm.clone(),
        };
        if model.rows.is_empty() {
            model.rows.push(vec![Para::default()]);
        }
        model.even();
        model
    }

    fn to_table(&self) -> Table {
        Table {
            rows: self
                .rows
                .iter()
                .map(|cells| {
                    cells
                        .iter()
                        .map(|para| Cell {
                            runs: para.to_runs(),
                            align: para.align,
                        })
                        .collect()
                })
                .collect(),
            header_rows: self.header_rows.min(self.rows.len()),
            column_widths_mm: if self.widths_mm.iter().all(Option::is_none) {
                Vec::new()
            } else {
                self.widths_mm.clone()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DocBlock {
    Para(Para),
    Table(TableModel),
    Image { file: String, width_mm: Option<f32> },
}

/// Where the caret is: a block, or a cell of a table block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Spot {
    pub block: usize,
    pub cell: Option<(usize, usize)>,
}

impl Spot {
    pub fn block(block: usize) -> Self {
        Self { block, cell: None }
    }
}

/// The editable text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doc {
    pub blocks: Vec<DocBlock>,
}

impl Doc {
    pub fn from_rich(text: &RichText) -> Self {
        let mut doc = Doc {
            blocks: text
                .blocks()
                .iter()
                .map(|block| match block {
                    TextBlock::Paragraph { runs, align } => {
                        DocBlock::Para(Para::from_runs(runs, ParaKind::Text, *align))
                    }
                    TextBlock::ListItem {
                        ordered,
                        level,
                        runs,
                        align,
                    } => {
                        let mut para = Para::from_runs(
                            runs,
                            if *ordered {
                                ParaKind::Numbered
                            } else {
                                ParaKind::Bullet
                            },
                            *align,
                        );
                        para.level = (*level).min(MAX_LIST_LEVEL);
                        DocBlock::Para(para)
                    }
                    TextBlock::Table(table) => DocBlock::Table(TableModel::from_table(table)),
                    TextBlock::Image { file, width_mm } => DocBlock::Image {
                        file: file.clone(),
                        width_mm: *width_mm,
                    },
                })
                .collect(),
        };
        doc.ensure_text_ends();
        doc
    }

    /// The text for the domain. A single empty paragraph is no text at all.
    pub fn to_rich(&self) -> RichText {
        let blocks: Vec<TextBlock> = self
            .blocks
            .iter()
            .map(|block| match block {
                DocBlock::Para(para) => {
                    let runs = para.to_runs();
                    match para.kind {
                        ParaKind::Text => TextBlock::Paragraph {
                            runs,
                            align: para.align,
                        },
                        ParaKind::Bullet | ParaKind::Numbered => TextBlock::ListItem {
                            ordered: para.kind == ParaKind::Numbered,
                            level: para.level.min(MAX_LIST_LEVEL),
                            runs,
                            align: para.align,
                        },
                    }
                }
                DocBlock::Table(table) => TextBlock::Table(table.to_table()),
                DocBlock::Image { file, width_mm } => TextBlock::Image {
                    file: file.clone(),
                    width_mm: *width_mm,
                },
            })
            .collect();
        // Leading and trailing empty plain paragraphs only exist so that one
        // can type above/below a table or image; they are not part of the text.
        let mut end = blocks.len();
        while end > 0 {
            match &blocks[end - 1] {
                TextBlock::Paragraph { runs, align }
                    if runs.is_empty() && *align == Align::Left =>
                {
                    end -= 1
                }
                _ => break,
            }
        }
        let is_filler = |block: &TextBlock| matches!(block, TextBlock::Paragraph { runs, align } if runs.is_empty() && *align == Align::Left);
        let mut start = 0;
        while start < end && is_filler(&blocks[start]) {
            start += 1;
        }
        let mut blocks = blocks;
        blocks.truncate(end);
        blocks.drain(..start);
        RichText::new(blocks)
    }

    /// Same as `self.to_rich().is_empty()`, without building the text
    /// (asked every frame for the hint in an empty editor).
    pub fn is_empty(&self) -> bool {
        self.blocks.iter().all(|block| match block {
            DocBlock::Para(para) => para.is_blank(),
            DocBlock::Table(table) => table.rows.iter().flatten().all(Para::is_blank),
            DocBlock::Image { .. } => false,
        })
    }

    /// A paragraph before the first and after the last table or image, so
    /// that the caret can always get there.
    pub fn ensure_text_ends(&mut self) {
        if !matches!(self.blocks.last(), Some(DocBlock::Para(_))) {
            self.blocks.push(DocBlock::Para(Para::default()));
        }
        if !matches!(self.blocks.first(), Some(DocBlock::Para(_))) {
            self.blocks.insert(0, DocBlock::Para(Para::default()));
        }
    }

    pub fn para(&self, spot: Spot) -> Option<&Para> {
        match (self.blocks.get(spot.block)?, spot.cell) {
            (DocBlock::Para(para), None) => Some(para),
            (DocBlock::Table(table), Some((row, column))) => table.rows.get(row)?.get(column),
            _ => None,
        }
    }

    pub fn para_mut(&mut self, spot: Spot) -> Option<&mut Para> {
        match (self.blocks.get_mut(spot.block)?, spot.cell) {
            (DocBlock::Para(para), None) => Some(para),
            (DocBlock::Table(table), Some((row, column))) => {
                table.rows.get_mut(row)?.get_mut(column)
            }
            _ => None,
        }
    }

    /// Splits the paragraph `block` at `at`; returns the new block index.
    /// An empty list item goes one level out instead, on the outermost
    /// level it ends the list (becomes plain text) – as in word processors.
    pub fn split(&mut self, block: usize, at: usize) -> usize {
        let Some(DocBlock::Para(para)) = self.blocks.get_mut(block) else {
            return block;
        };
        if para.kind != ParaKind::Text && para.is_empty() {
            if para.level > 0 {
                para.level -= 1;
            } else {
                para.kind = ParaKind::Text;
            }
            return block;
        }
        let rest = para.split(at);
        self.blocks.insert(block + 1, DocBlock::Para(rest));
        block + 1
    }

    /// Backspace at the start of paragraph `block`: an Unterpunkt goes one
    /// level out, a list item on the outermost level becomes text;
    /// otherwise it is joined to the paragraph before. Returns the block
    /// and caret after the change, or `None` when nothing happens.
    pub fn join_with_previous(&mut self, block: usize) -> Option<(usize, usize)> {
        let DocBlock::Para(para) = self.blocks.get_mut(block)? else {
            return None;
        };
        if para.kind != ParaKind::Text {
            if para.level > 0 {
                para.level -= 1;
            } else {
                para.kind = ParaKind::Text;
            }
            return Some((block, 0));
        }
        if block == 0 {
            return None;
        }
        match &self.blocks[block - 1] {
            DocBlock::Para(_) => {
                let DocBlock::Para(current) = self.blocks.remove(block) else {
                    unreachable!()
                };
                let DocBlock::Para(previous) = &mut self.blocks[block - 1] else {
                    unreachable!()
                };
                let caret = previous.join(current);
                Some((block - 1, caret))
            }
            // Before a table or image: an empty paragraph disappears, else nothing.
            _ => {
                let empty = matches!(&self.blocks[block], DocBlock::Para(p) if p.is_empty());
                if empty && block + 1 < self.blocks.len() {
                    self.blocks.remove(block);
                    return Some((block.min(self.blocks.len() - 1), 0));
                }
                None
            }
        }
    }

    /// Delete at the end of paragraph `block`: joins the next paragraph.
    pub fn join_next(&mut self, block: usize) -> Option<usize> {
        if !matches!(self.blocks.get(block + 1), Some(DocBlock::Para(_))) {
            return None;
        }
        let DocBlock::Para(next) = self.blocks.remove(block + 1) else {
            unreachable!()
        };
        let DocBlock::Para(current) = self.blocks.get_mut(block)? else {
            return None;
        };
        Some(current.join(next))
    }

    /// Inserts a block after paragraph `block`, splitting it at `caret`
    /// when the caret is inside; returns the index of the new block.
    pub fn insert_block(&mut self, spot: Spot, caret: usize, new: DocBlock) -> usize {
        let block = spot.block.min(self.blocks.len().saturating_sub(1));
        let at = match (self.blocks.get_mut(block), spot.cell) {
            (Some(DocBlock::Para(para)), None) => {
                if caret == 0 && !para.is_empty() {
                    block
                } else if caret < para.len() {
                    let rest = para.split(caret);
                    self.blocks.insert(block + 1, DocBlock::Para(rest));
                    block + 1
                } else {
                    block + 1
                }
            }
            _ => (block + 1).min(self.blocks.len()),
        };
        self.blocks.insert(at, new);
        // An empty paragraph the block replaced is not needed any more.
        if at > 0
            && matches!(&self.blocks[at - 1], DocBlock::Para(p) if p.is_empty() && p.kind == ParaKind::Text)
            && at - 1 > 0
        {
            self.blocks.remove(at - 1);
            self.ensure_text_ends();
            return at - 1;
        }
        self.ensure_text_ends();
        at
    }

    /// Removes a table or image block.
    pub fn remove_block(&mut self, block: usize) {
        if block < self.blocks.len() {
            self.blocks.remove(block);
        }
        self.merge_empty_neighbours();
        self.ensure_text_ends();
    }

    /// Moves a table or image one place up or down past the neighbour.
    pub fn move_block(&mut self, block: usize, up: bool) -> usize {
        let target = if up {
            block.checked_sub(1)
        } else {
            (block + 1 < self.blocks.len()).then_some(block + 1)
        };
        let Some(target) = target else { return block };
        self.blocks.swap(block, target);
        self.ensure_text_ends();
        target
    }

    /// Two empty plain paragraphs in a row become one.
    fn merge_empty_neighbours(&mut self) {
        let empty = |block: &DocBlock| matches!(block, DocBlock::Para(p) if p.is_empty() && p.kind == ParaKind::Text);
        let mut index = 1;
        while index < self.blocks.len() {
            if empty(&self.blocks[index]) && empty(&self.blocks[index - 1]) {
                self.blocks.remove(index);
            } else {
                index += 1;
            }
        }
    }

    /// Marker of the list item `block` as printed („•“, „1.“, „a)“ …,
    /// numbers counted per level); None for anything else.
    pub fn marker_of(&self, block: usize) -> Option<String> {
        let end = block.min(self.blocks.len().saturating_sub(1));
        proava_domain::list_markers(self.blocks[..=end].iter().map(|candidate| match candidate {
            DocBlock::Para(para) if para.kind != ParaKind::Text => {
                Some((para.kind == ParaKind::Numbered, para.level))
            }
            _ => None,
        }))
        .pop()
        .flatten()
    }

    /// Tab / Umschalt+Tab: the list items among the paragraphs from `a` to
    /// `b` one level in (`deeper`) or out. Text paragraphs stay; returns
    /// whether a list item was touched (then the key belongs to the list,
    /// even at the first or last level).
    pub fn shift_level(&mut self, a: Pos, b: Pos, deeper: bool) -> bool {
        let mut any = false;
        for block in self.paras_in(a, b) {
            if let Some(para) = self.para_at_mut(block) {
                if para.kind == ParaKind::Text {
                    continue;
                }
                any = true;
                para.level = if deeper {
                    (para.level + 1).min(MAX_LIST_LEVEL)
                } else {
                    para.level.saturating_sub(1)
                };
            }
        }
        any
    }
}

/// A caret place in the running text: paragraph block and char offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pos {
    pub block: usize,
    pub offset: usize,
}

impl Pos {
    pub fn new(block: usize, offset: usize) -> Self {
        Self { block, offset }
    }
}

/// Ordered pair of two positions.
pub fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Selection-level editing over several paragraphs (tables and images in
/// between are whole blocks: copied, deleted, but never entered).
impl Doc {
    fn para_at(&self, block: usize) -> Option<&Para> {
        match self.blocks.get(block)? {
            DocBlock::Para(para) => Some(para),
            _ => None,
        }
    }

    fn para_at_mut(&mut self, block: usize) -> Option<&mut Para> {
        match self.blocks.get_mut(block)? {
            DocBlock::Para(para) => Some(para),
            _ => None,
        }
    }

    pub fn is_para(&self, block: usize) -> bool {
        self.para_at(block).is_some()
    }

    /// First paragraph at or after `block` (or before, when there is none).
    fn para_near(&self, block: usize) -> usize {
        (block..self.blocks.len())
            .find(|&b| self.is_para(b))
            .or_else(|| {
                (0..block.min(self.blocks.len()))
                    .rev()
                    .find(|&b| self.is_para(b))
            })
            .unwrap_or(0)
    }

    /// The same place, inside the text (on a paragraph, offset ≤ length).
    pub fn clamp(&self, pos: Pos) -> Pos {
        let block = if self.is_para(pos.block) {
            pos.block
        } else {
            self.para_near(pos.block)
        };
        let len = self.para_at(block).map_or(0, Para::len);
        Pos::new(block, pos.offset.min(len))
    }

    pub fn start(&self) -> Pos {
        self.clamp(Pos::new(0, 0))
    }

    pub fn end(&self) -> Pos {
        let block = (0..self.blocks.len())
            .rev()
            .find(|&b| self.is_para(b))
            .unwrap_or(0);
        Pos::new(block, self.para_at(block).map_or(0, Para::len))
    }

    fn prev_para(&self, block: usize) -> Option<usize> {
        (0..block).rev().find(|&b| self.is_para(b))
    }

    fn next_para(&self, block: usize) -> Option<usize> {
        (block + 1..self.blocks.len()).find(|&b| self.is_para(b))
    }

    /// Previous paragraph block (for ↑ in the first line).
    pub fn para_before(&self, block: usize) -> Option<usize> {
        self.prev_para(block)
    }

    pub fn para_after(&self, block: usize) -> Option<usize> {
        self.next_para(block)
    }

    pub fn left(&self, pos: Pos) -> Pos {
        if pos.offset > 0 {
            return Pos::new(pos.block, pos.offset - 1);
        }
        match self.prev_para(pos.block) {
            Some(block) => Pos::new(block, self.para_at(block).map_or(0, Para::len)),
            None => pos,
        }
    }

    pub fn right(&self, pos: Pos) -> Pos {
        let len = self.para_at(pos.block).map_or(0, Para::len);
        if pos.offset < len {
            return Pos::new(pos.block, pos.offset + 1);
        }
        match self.next_para(pos.block) {
            Some(block) => Pos::new(block, 0),
            None => pos,
        }
    }

    fn chars_of(&self, block: usize) -> Vec<char> {
        self.para_at(block)
            .map(|para| para.text.chars().collect())
            .unwrap_or_default()
    }

    /// Start of the word before the caret (Strg+←).
    pub fn word_left(&self, pos: Pos) -> Pos {
        if pos.offset == 0 {
            return self.left(pos);
        }
        let chars = self.chars_of(pos.block);
        let mut i = pos.offset.min(chars.len());
        while i > 0 && !is_word_char(chars[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word_char(chars[i - 1]) {
            i -= 1;
        }
        Pos::new(pos.block, i)
    }

    /// Start of the next word (Strg+→).
    pub fn word_right(&self, pos: Pos) -> Pos {
        let chars = self.chars_of(pos.block);
        if pos.offset >= chars.len() {
            return self.right(pos);
        }
        let mut i = pos.offset;
        while i < chars.len() && is_word_char(chars[i]) {
            i += 1;
        }
        while i < chars.len() && !is_word_char(chars[i]) {
            i += 1;
        }
        Pos::new(pos.block, i)
    }

    /// The word around `pos` (double click).
    pub fn word_at(&self, pos: Pos) -> (Pos, Pos) {
        let chars = self.chars_of(pos.block);
        let mut start = pos.offset.min(chars.len());
        let mut end = start;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_word_char(chars[end]) {
            end += 1;
        }
        (Pos::new(pos.block, start), Pos::new(pos.block, end))
    }

    /// Removes everything between `a` and `b` (any order); blocks fully
    /// inside go away, the paragraphs at both ends are joined. Returns the
    /// caret afterwards.
    pub fn delete_range(&mut self, a: Pos, b: Pos) -> Pos {
        let (a, b) = ordered(self.clamp(a), self.clamp(b));
        if a == b {
            return a;
        }
        if a.block == b.block {
            if let Some(para) = self.para_at_mut(a.block) {
                para.remove_range(a.offset, b.offset);
            }
            return a;
        }
        let tail = match self.para_at_mut(b.block) {
            Some(last) => {
                let mut tail = last.split(b.offset);
                // The joined paragraph keeps the kind and alignment of the first.
                tail.kind = ParaKind::Text;
                Some(tail)
            }
            None => None,
        };
        if let Some(first) = self.para_at_mut(a.block) {
            first.split(a.offset);
            if let Some(tail) = tail {
                first.join(tail);
            }
        }
        self.blocks.drain(a.block + 1..=b.block);
        self.ensure_text_ends();
        a
    }

    /// Types `text` at `pos` with `fmt` (a `\n` is a line break inside the
    /// paragraph). Returns the caret after it.
    pub fn insert_text(&mut self, pos: Pos, text: &str, fmt: &Fmt) -> Pos {
        let pos = self.clamp(pos);
        let Some(para) = self.para_at_mut(pos.block) else {
            return pos;
        };
        let inserted = para.insert(pos.offset, text, fmt);
        Pos::new(pos.block, pos.offset + inserted)
    }

    /// Enter at `pos`: splits the paragraph (an empty list item ends the list).
    pub fn break_paragraph(&mut self, pos: Pos) -> Pos {
        let pos = self.clamp(pos);
        let next = self.split(pos.block, pos.offset);
        if next == pos.block {
            Pos::new(pos.block, 0)
        } else {
            Pos::new(next, 0)
        }
    }

    /// Backspace without selection.
    pub fn backspace(&mut self, pos: Pos) -> Pos {
        let pos = self.clamp(pos);
        if pos.offset > 0 {
            return self.delete_range(Pos::new(pos.block, pos.offset - 1), pos);
        }
        match self.join_with_previous(pos.block) {
            Some((block, offset)) => Pos::new(block, offset),
            None => pos,
        }
    }

    /// Delete without selection.
    pub fn delete_forward(&mut self, pos: Pos) -> Pos {
        let pos = self.clamp(pos);
        let len = self.para_at(pos.block).map_or(0, Para::len);
        if pos.offset < len {
            return self.delete_range(pos, Pos::new(pos.block, pos.offset + 1));
        }
        self.join_next(pos.block);
        pos
    }

    /// Paragraph char ranges a selection covers: (block, start, end).
    fn spans(&self, a: Pos, b: Pos) -> Vec<(usize, usize, usize)> {
        let (a, b) = ordered(a, b);
        (a.block..=b.block.min(self.blocks.len().saturating_sub(1)))
            .filter_map(|block| {
                let len = self.para_at(block)?.len();
                let start = if block == a.block {
                    a.offset.min(len)
                } else {
                    0
                };
                let end = if block == b.block {
                    b.offset.min(len)
                } else {
                    len
                };
                Some((block, start, end))
            })
            .collect()
    }

    /// Format change over a selection across paragraphs; toggles look at
    /// all selected chars together.
    pub fn format_selection(&mut self, a: Pos, b: Pos, change: &FmtChange) {
        let spans = self.spans(a, b);
        let all = AllSet::of(
            spans
                .iter()
                .flat_map(|&(block, start, end)| {
                    self.para_at(block)
                        .map(|para| &para.fmts[start..end])
                        .unwrap_or(&[])
                        .iter()
                })
                .collect::<Vec<_>>()
                .into_iter(),
        );
        for (block, start, end) in spans {
            if let Some(para) = self.para_at_mut(block) {
                for fmt in &mut para.fmts[start..end] {
                    change.apply(fmt, &all);
                }
            }
        }
    }

    /// Format shown in the toolbar for a selection.
    pub fn fmt_of_selection(&self, a: Pos, b: Pos) -> Fmt {
        let spans = self.spans(a, b);
        let chars: Vec<&Fmt> = spans
            .iter()
            .flat_map(|&(block, start, end)| {
                self.para_at(block)
                    .map(|para| &para.fmts[start..end])
                    .unwrap_or(&[])
                    .iter()
            })
            .collect();
        let Some(first) = chars.first() else {
            let caret = ordered(a, b).1;
            return self
                .para_at(caret.block)
                .map_or_else(Fmt::default, |p| p.fmt_at(caret.offset));
        };
        let all = AllSet::of(chars.iter().copied());
        Fmt {
            bold: all.bold,
            italic: all.italic,
            underline: all.underline,
            font: chars
                .iter()
                .all(|f| f.font == first.font)
                .then(|| first.font.clone())
                .flatten(),
            size_pt: chars
                .iter()
                .all(|f| f.size_pt == first.size_pt)
                .then_some(first.size_pt)
                .flatten(),
        }
    }

    /// Paragraph blocks touched by a selection.
    pub fn paras_in(&self, a: Pos, b: Pos) -> Vec<usize> {
        self.spans(a, b)
            .into_iter()
            .map(|(block, _, _)| block)
            .collect()
    }

    pub fn set_align(&mut self, a: Pos, b: Pos, align: Align) {
        for block in self.paras_in(a, b) {
            if let Some(para) = self.para_at_mut(block) {
                para.align = align;
            }
        }
    }

    /// List button: all touched paragraphs become `kind`, or text again when
    /// they all already are.
    pub fn toggle_kind(&mut self, a: Pos, b: Pos, kind: ParaKind) {
        let blocks = self.paras_in(a, b);
        let all = blocks
            .iter()
            .all(|&block| self.para_at(block).is_some_and(|p| p.kind == kind));
        for block in blocks {
            if let Some(para) = self.para_at_mut(block) {
                para.kind = if all { ParaKind::Text } else { kind };
                if para.kind == ParaKind::Text {
                    para.level = 0;
                }
            }
        }
    }

    /// Copy of the selection as its own document (partial paragraphs at the
    /// ends, whole tables and images in between).
    pub fn fragment(&self, a: Pos, b: Pos) -> Doc {
        let (a, b) = ordered(self.clamp(a), self.clamp(b));
        let mut blocks = Vec::new();
        for block in a.block..=b.block {
            match &self.blocks[block] {
                DocBlock::Para(para) => {
                    let len = para.len();
                    let start = if block == a.block { a.offset } else { 0 };
                    let end = if block == b.block { b.offset } else { len };
                    let mut part = para.clone();
                    part.split(end);
                    let piece = part.split(start);
                    blocks.push(DocBlock::Para(piece));
                }
                other => blocks.push(other.clone()),
            }
        }
        Doc { blocks }
    }

    /// Selected text as plain text (paragraphs on lines, cells with tabs).
    pub fn plain_text(&self, a: Pos, b: Pos) -> String {
        let fragment = self.fragment(a, b);
        let mut lines = Vec::new();
        for block in &fragment.blocks {
            match block {
                DocBlock::Para(para) => lines.push(para.text.clone()),
                DocBlock::Table(table) => {
                    for row in &table.rows {
                        lines.push(
                            row.iter()
                                .map(|cell| cell.text.clone())
                                .collect::<Vec<_>>()
                                .join("\t"),
                        );
                    }
                }
                DocBlock::Image { .. } => {}
            }
        }
        lines.join("\n")
    }

    /// Inserts a fragment at `pos`: one paragraph goes into the text with its
    /// formats; several blocks split the paragraph there. Returns the caret
    /// after the inserted text.
    pub fn insert_fragment(&mut self, pos: Pos, fragment: &Doc) -> Pos {
        let pos = self.clamp(pos);
        let pieces = &fragment.blocks;
        match pieces.as_slice() {
            [] => pos,
            [DocBlock::Para(only)] => {
                let Some(para) = self.para_at_mut(pos.block) else {
                    return pos;
                };
                para.insert_para(pos.offset, only);
                Pos::new(pos.block, pos.offset + only.len())
            }
            _ => {
                let Some(target) = self.para_at_mut(pos.block) else {
                    return pos;
                };
                let rest = target.split(pos.offset);
                let mut new_blocks: Vec<DocBlock> = Vec::new();
                let mut iter = pieces.iter().peekable();
                // First piece joins the text before the caret.
                if let Some(DocBlock::Para(first)) = iter.peek() {
                    let target_empty = target.is_empty();
                    target.join(first.clone());
                    if target_empty {
                        target.kind = first.kind;
                        target.align = first.align;
                    }
                    iter.next();
                }
                new_blocks.extend(iter.cloned());
                // The rest after the caret joins the last pasted paragraph.
                let caret = match new_blocks.last_mut() {
                    Some(DocBlock::Para(last)) => {
                        let len = last.len();
                        last.join(rest);
                        len
                    }
                    _ => {
                        new_blocks.push(DocBlock::Para(rest));
                        0
                    }
                };
                let count = new_blocks.len();
                let at = pos.block + 1;
                for (index, block) in new_blocks.into_iter().enumerate() {
                    self.blocks.insert(at + index, block);
                }
                self.ensure_text_ends();
                if count == 0 {
                    Pos::new(pos.block, caret)
                } else {
                    Pos::new(at + count - 1, caret)
                }
            }
        }
    }
}

impl Para {
    /// Inserts `text` with one format at char `at`; returns the char count.
    pub fn insert(&mut self, at: usize, text: &str, fmt: &Fmt) -> usize {
        let at = at.min(self.len());
        let count = text.chars().count();
        let byte = byte_of(&self.text, at);
        self.text.insert_str(byte, text);
        self.fmts
            .splice(at..at, std::iter::repeat_n(fmt.clone(), count));
        if count > 0 && self.fmts.len() == count {
            self.empty_fmt = fmt.clone();
        }
        count
    }

    /// Inserts another paragraph's chars with their formats.
    pub fn insert_para(&mut self, at: usize, other: &Para) {
        let at = at.min(self.len());
        let byte = byte_of(&self.text, at);
        self.text.insert_str(byte, &other.text);
        self.fmts.splice(at..at, other.fmts.iter().cloned());
    }

    /// Removes chars `start..end`.
    pub fn remove_range(&mut self, start: usize, end: usize) {
        let (start, end) = (start.min(self.len()), end.min(self.len()));
        if start >= end {
            return;
        }
        if start == 0 && end == self.len() {
            self.empty_fmt = self.fmts[0].clone();
        }
        let (from, to) = (byte_of(&self.text, start), byte_of(&self.text, end));
        self.text.replace_range(from..to, "");
        self.fmts.drain(start..end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_walks_the_cells_row_by_row() {
        let table = TableModel::new(2, 3);
        assert_eq!(table.tab_target((0, 0), false), Some((0, 1)));
        assert_eq!(table.tab_target((0, 2), false), Some((1, 0)));
        assert_eq!(table.tab_target((1, 2), false), None, "letzte Zelle");
        assert_eq!(table.tab_target((1, 0), true), Some((0, 2)));
        assert_eq!(table.tab_target((0, 0), true), None, "erste Zelle");
    }

    fn bold(text: &str) -> Run {
        Run::bold(text)
    }

    #[test]
    fn runs_and_emptiness_match_the_char_by_char_way() {
        let mut big = Run::plain("groß");
        big.size_pt = Some(14.0);
        big.font = Some("Liberation Serif".into());
        let texts = [
            RichText::default(),
            RichText::plain(" \n\t"),
            RichText::plain("Zeile\n\nnoch eine"),
            RichText::new(vec![
                TextBlock::paragraph(vec![
                    Run::plain("ä "),
                    bold("öü"),
                    bold(" fett"),
                    big.clone(),
                    Run::plain("ß"),
                ]),
                TextBlock::list_item(true, vec![bold("Punkt"), Run::plain(" eins")]),
            ]),
            RichText::new(vec![TextBlock::Table(Table::from_texts(vec![vec![
                " ".into(),
                "".into(),
            ]]))]),
            RichText::new(vec![TextBlock::Table(Table::from_texts(vec![vec![
                " ".into(),
                "x".into(),
            ]]))]),
            RichText::new(vec![TextBlock::Image {
                file: "a.png".into(),
                width_mm: None,
            }]),
        ];
        for text in texts {
            let doc = Doc::from_rich(&text);
            assert_eq!(doc.is_empty(), doc.to_rich().is_empty(), "{text:?}");
            for block in &doc.blocks {
                let paras: Vec<&Para> = match block {
                    DocBlock::Para(para) => vec![para],
                    DocBlock::Table(table) => table.rows.iter().flatten().collect(),
                    DocBlock::Image { .. } => Vec::new(),
                };
                for para in paras {
                    let char_by_char = normalize_runs(
                        para.text
                            .chars()
                            .zip(&para.fmts)
                            .map(|(ch, fmt)| fmt.run(ch.to_string()))
                            .collect(),
                    );
                    assert_eq!(para.to_runs(), char_by_char);
                }
            }
        }
    }

    #[test]
    fn runs_round_trip_through_chars() {
        let runs = vec![Run::plain("ä"), bold("öü"), Run::plain("ß")];
        let para = Para::from_runs(&runs, ParaKind::Text, Align::Right);
        assert_eq!(para.len(), 4);
        assert_eq!(para.to_runs(), runs);
    }

    #[test]
    fn typing_inherits_the_format_to_the_left() {
        let mut para =
            Para::from_runs(&[Run::plain("ab"), bold("cd")], ParaKind::Text, Align::Left);
        // Type "X" after "abc" (inside the bold run).
        assert_eq!(para.apply_edit("abcXd", None), (3, 4));
        assert_eq!(para.to_runs(), vec![Run::plain("ab"), bold("cXd")]);
        // Type at the end of the plain run: stays plain.
        para.apply_edit("abYcXd", None);
        assert_eq!(para.to_runs(), vec![Run::plain("abY"), bold("cXd")]);
        // At the very start: format of the first char.
        para.apply_edit("ZabYcXd", None);
        assert_eq!(para.to_runs()[0], Run::plain("ZabY"));
        // Replacing a selection takes the format of the replaced text.
        para.apply_edit("ZabY12", None);
        assert_eq!(para.to_runs(), vec![Run::plain("ZabY"), bold("12")]);
        // Deleting.
        para.apply_edit("Za12", None);
        assert_eq!(para.to_runs(), vec![Run::plain("Za"), bold("12")]);
        // A chosen format for typing wins.
        let italic = Fmt {
            italic: true,
            ..Fmt::default()
        };
        para.apply_edit("Za_12", Some(&italic));
        assert!(para.fmts[2].italic && !para.fmts[2].bold);
        assert_eq!(para.fmts.len(), para.text.chars().count());
    }

    #[test]
    fn repeated_letters_are_diffed_safely() {
        let mut para =
            Para::from_runs(&[Run::plain("aa"), bold("aa")], ParaKind::Text, Align::Left);
        para.apply_edit("aaaaa", None);
        assert_eq!(para.len(), 5);
        assert_eq!(
            para.to_runs()
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "aaaaa"
        );
        para.apply_edit("", None);
        assert!(para.is_empty());
        para.apply_edit("neu", None);
        assert_eq!(para.to_runs(), vec![Run::plain("neu")]);
    }

    #[test]
    fn empty_paragraph_keeps_the_format_for_typing() {
        let mut para = Para::from_runs(&[bold("x")], ParaKind::Text, Align::Left);
        para.apply_edit("", None);
        para.apply_edit("y", None);
        assert_eq!(para.to_runs(), vec![bold("y")]);
    }

    #[test]
    fn toggling_and_setting_formats_on_a_range() {
        let mut para = Para::from_runs(&[Run::plain("abcdef")], ParaKind::Text, Align::Left);
        para.format_range(1, 3, &FmtChange::ToggleBold);
        assert_eq!(
            para.to_runs(),
            vec![Run::plain("a"), bold("bc"), Run::plain("def")]
        );
        // Partly bold → all bold; all bold → off.
        para.format_range(0, 3, &FmtChange::ToggleBold);
        assert_eq!(para.to_runs(), vec![bold("abc"), Run::plain("def")]);
        para.format_range(3, 0, &FmtChange::ToggleBold);
        assert_eq!(para.to_runs(), vec![Run::plain("abcdef")]);
        para.format_range(2, 4, &FmtChange::Font(Some("Courier New".into())));
        para.format_range(2, 4, &FmtChange::Size(Some(14.0)));
        let fmt = para.fmt_of_range(2, 4);
        assert_eq!(fmt.font.as_deref(), Some("Courier New"));
        assert_eq!(fmt.size_pt, Some(14.0));
        assert_eq!(para.fmt_of_range(0, 4).font, None, "gemischt");
        para.format_range(0, 6, &FmtChange::Clear);
        assert_eq!(para.to_runs(), vec![Run::plain("abcdef")]);
        assert!(Para::changed_fmt(&Fmt::default(), &FmtChange::ToggleItalic).italic);
    }

    #[test]
    fn split_and_join_paragraphs() {
        let mut doc = Doc::from_rich(&RichText::new(vec![TextBlock::Paragraph {
            runs: vec![Run::plain("Hallo "), bold("Welt")],
            align: Align::Center,
        }]));
        let next = doc.split(0, 8);
        assert_eq!(next, 1);
        let rich = doc.to_rich();
        assert_eq!(
            rich.blocks()[1],
            TextBlock::Paragraph {
                runs: vec![bold("lt")],
                align: Align::Center
            }
        );
        assert_eq!(doc.join_with_previous(1), Some((0, 8)));
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.to_rich().to_plain(), "Hallo Welt");
        assert_eq!(doc.join_with_previous(0), None);
    }

    #[test]
    fn lists_end_with_an_empty_item_and_backspace_unlists() {
        let mut doc = Doc::from_rich(&RichText::new(vec![TextBlock::list_item(
            false,
            vec![Run::plain("Punkt")],
        )]));
        let next = doc.split(0, 5);
        assert!(matches!(&doc.blocks[next], DocBlock::Para(p) if p.kind == ParaKind::Bullet));
        // Enter in the empty item ends the list.
        assert_eq!(doc.split(next, 0), next);
        assert!(matches!(&doc.blocks[next], DocBlock::Para(p) if p.kind == ParaKind::Text));
        // Backspace at the start of a list item makes it text.
        assert_eq!(doc.join_with_previous(0), Some((0, 0)));
        assert!(matches!(&doc.blocks[0], DocBlock::Para(p) if p.kind == ParaKind::Text));
    }

    #[test]
    fn tables_and_images_keep_text_around_them() {
        let text = RichText::new(vec![
            TextBlock::Image {
                file: "a.png".into(),
                width_mm: Some(50.0),
            },
            TextBlock::Table(Table::from_texts(vec![vec!["a".into(), "b".into()]])),
        ]);
        let doc = Doc::from_rich(&text);
        assert_eq!(
            doc.blocks.len(),
            4,
            "Absatz vor dem Bild und nach der Tabelle"
        );
        assert_eq!(doc.to_rich(), text);
    }

    #[test]
    fn inserting_a_table_splits_the_paragraph() {
        let mut doc = Doc::from_rich(&RichText::plain("vorher nachher"));
        let at = doc.insert_block(Spot::block(0), 7, DocBlock::Table(TableModel::new(2, 3)));
        assert_eq!(at, 1);
        assert_eq!(doc.blocks.len(), 3);
        let rich = doc.to_rich();
        assert_eq!(rich.blocks()[0], TextBlock::text("vorher "));
        let TextBlock::Table(table) = &rich.blocks()[1] else {
            panic!()
        };
        assert_eq!((table.rows.len(), table.column_count()), (2, 3));
        assert_eq!(rich.blocks()[2], TextBlock::text("nachher"));
        // In an empty text the table replaces the empty paragraph.
        let mut empty = Doc::from_rich(&RichText::default());
        let at = empty.insert_block(Spot::block(0), 0, DocBlock::Table(TableModel::new(1, 1)));
        assert_eq!(at, 1);
        assert!(matches!(empty.blocks[at], DocBlock::Table(_)));
        empty.remove_block(at);
        assert_eq!(empty.blocks.len(), 1);
    }

    #[test]
    fn table_rows_columns_header_and_widths() {
        let mut table = TableModel::new(2, 2);
        table.header_rows = 1;
        table.insert_row(0);
        assert_eq!((table.rows.len(), table.header_rows), (3, 2));
        table.insert_column(1);
        assert_eq!(table.columns(), 3);
        table.set_width(2, Some(33.33));
        assert_eq!(table.widths_mm, vec![None, None, Some(33.3)]);
        assert!(table.remove_column(0));
        assert_eq!(table.widths_mm, vec![None, Some(33.3)]);
        assert!(table.remove_row(0));
        assert_eq!(table.header_rows, 1);
        let mut one = TableModel::new(1, 1);
        assert!(!one.remove_row(0));
        assert!(!one.remove_column(0));
        // Cells keep their format and alignment in the domain table.
        table.rows[1][0] = Para::from_runs(&[bold("x")], ParaKind::Text, Align::Right);
        let domain = table.to_table();
        assert_eq!(domain.rows[1][0].runs, vec![bold("x")]);
        assert_eq!(domain.rows[1][0].align, Align::Right);
        assert_eq!(TableModel::from_table(&domain), table);
    }

    #[test]
    fn numbers_count_within_a_list() {
        let doc = Doc::from_rich(&RichText::new(vec![
            TextBlock::list_item(true, vec![Run::plain("a")]),
            TextBlock::list_item(true, vec![Run::plain("b")]),
            TextBlock::text("x"),
            TextBlock::list_item(true, vec![Run::plain("c")]),
        ]));
        assert_eq!(
            (doc.marker_of(0), doc.marker_of(1), doc.marker_of(3)),
            (Some("1.".into()), Some("2.".into()), Some("1.".into()))
        );
    }

    #[test]
    fn unterpunkte_count_per_level_and_tab_shifts_the_level() {
        let mut doc = Doc::from_rich(&RichText::new(vec![
            TextBlock::list_item(true, vec![Run::plain("eins")]),
            TextBlock::list_item_at(true, 1, vec![Run::plain("eins a")]),
            TextBlock::list_item_at(true, 1, vec![Run::plain("eins b")]),
            TextBlock::list_item_at(false, 2, vec![Run::plain("Strich")]),
            TextBlock::list_item(true, vec![Run::plain("zwei")]),
            TextBlock::text("Text"),
        ]));
        let markers: Vec<_> = (0..6).map(|block| doc.marker_of(block)).collect();
        assert_eq!(
            markers,
            vec![
                Some("1.".into()),
                Some("a)".into()),
                Some("b)".into()),
                Some("•".into()),
                Some("2.".into()),
                None
            ]
        );
        // Tab over „eins b“ … „zwei“ and the text: the list items one in,
        // the text paragraph stays.
        assert!(doc.shift_level(Pos::new(2, 0), Pos::new(5, 0), true));
        let levels = |doc: &Doc| -> Vec<u8> {
            doc.blocks
                .iter()
                .map(|block| match block {
                    DocBlock::Para(para) => para.level,
                    _ => 99,
                })
                .collect()
        };
        assert_eq!(levels(&doc), vec![0, 1, 2, 3, 1, 0]);
        assert!(doc.shift_level(Pos::new(4, 0), Pos::new(4, 0), false));
        assert!(
            !doc.shift_level(Pos::new(5, 0), Pos::new(5, 0), true),
            "Text: kein Listenpunkt"
        );
        // Enter on an empty Unterpunkt goes one level out, then ends the list.
        let mut doc = Doc::from_rich(&RichText::new(vec![
            TextBlock::list_item(false, vec![Run::plain("a")]),
            TextBlock::list_item_at(false, 1, Vec::new()),
        ]));
        assert_eq!(doc.split(1, 0), 1);
        assert!(
            matches!(&doc.blocks[1], DocBlock::Para(p) if p.kind == ParaKind::Bullet && p.level == 0)
        );
        doc.split(1, 0);
        assert!(matches!(&doc.blocks[1], DocBlock::Para(p) if p.kind == ParaKind::Text));
        // Splitting an Unterpunkt keeps its level; the text round trips.
        let mut doc = Doc::from_rich(&RichText::new(vec![TextBlock::list_item_at(
            false,
            2,
            vec![Run::plain("vorne hinten")],
        )]));
        let next = doc.split(0, 6);
        assert!(
            matches!(&doc.blocks[next], DocBlock::Para(p) if p.level == 2 && p.text == "hinten")
        );
        let rich = doc.to_rich();
        assert_eq!(rich.blocks()[1].list_kind(), Some((false, 2)));
        // Rücktaste at the start of an Unterpunkt: one level out first.
        let mut doc = Doc::from_rich(&rich);
        assert_eq!(doc.join_with_previous(1), Some((1, 0)));
        assert!(
            matches!(&doc.blocks[1], DocBlock::Para(p) if p.kind == ParaKind::Bullet && p.level == 1)
        );
    }

    #[test]
    fn rich_text_round_trips_through_the_doc() {
        let text = RichText::new(vec![
            TextBlock::Paragraph {
                runs: vec![Run {
                    font: Some("Times New Roman".into()),
                    size_pt: Some(12.0),
                    ..bold("Titel")
                }],
                align: Align::Center,
            },
            TextBlock::list_item(true, vec![Run::plain("eins")]),
            TextBlock::Paragraph {
                runs: vec![Run::plain("Zeile\nUmbruch")],
                align: Align::Justify,
            },
        ]);
        assert_eq!(Doc::from_rich(&text).to_rich(), text);
        assert!(Doc::from_rich(&RichText::default()).to_rich().is_empty());
    }

    fn doc(paras: &[&str]) -> Doc {
        Doc::from_rich(&RichText::new(
            paras.iter().map(|text| TextBlock::text(*text)).collect(),
        ))
    }

    fn texts(doc: &Doc) -> Vec<String> {
        doc.blocks
            .iter()
            .map(|block| match block {
                DocBlock::Para(para) => para.text.clone(),
                DocBlock::Table(_) => "[Tabelle]".into(),
                DocBlock::Image { .. } => "[Bild]".into(),
            })
            .collect()
    }

    #[test]
    fn caret_moves_across_paragraphs_and_words() {
        let doc = doc(&["ab", "Wort zwei"]);
        assert_eq!(doc.right(Pos::new(0, 2)), Pos::new(1, 0));
        assert_eq!(doc.left(Pos::new(1, 0)), Pos::new(0, 2));
        assert_eq!(doc.left(Pos::new(0, 0)), Pos::new(0, 0));
        assert_eq!(doc.end(), Pos::new(1, 9));
        assert_eq!(doc.word_right(Pos::new(1, 0)), Pos::new(1, 5));
        assert_eq!(doc.word_left(Pos::new(1, 9)), Pos::new(1, 5));
        assert_eq!(doc.word_left(Pos::new(1, 0)), Pos::new(0, 2));
        assert_eq!(
            doc.word_at(Pos::new(1, 2)),
            (Pos::new(1, 0), Pos::new(1, 4))
        );
    }

    #[test]
    fn deleting_a_selection_over_paragraphs_joins_them() {
        let mut d = doc(&["eins", "zwei", "drei"]);
        let caret = d.delete_range(Pos::new(2, 2), Pos::new(0, 2));
        assert_eq!(caret, Pos::new(0, 2));
        assert_eq!(texts(&d), vec!["eiei"]);
        // Tables in between go away too.
        let mut d = Doc::from_rich(&RichText::new(vec![
            TextBlock::text("vor"),
            TextBlock::Table(Table::from_texts(vec![vec!["x".into()]])),
            TextBlock::text("nach"),
        ]));
        d.delete_range(Pos::new(0, 1), Pos::new(2, 1));
        assert_eq!(texts(&d), vec!["vach"]);
    }

    #[test]
    fn typing_enter_backspace_and_delete() {
        let mut d = doc(&["ab"]);
        let fmt = Fmt {
            bold: true,
            ..Fmt::default()
        };
        let caret = d.insert_text(Pos::new(0, 1), "XY", &fmt);
        assert_eq!(caret, Pos::new(0, 3));
        assert_eq!(
            d.to_rich().blocks()[0],
            TextBlock::paragraph(vec![Run::plain("a"), bold("XY"), Run::plain("b")])
        );
        let caret = d.break_paragraph(caret);
        assert_eq!(
            (caret, texts(&d)),
            (Pos::new(1, 0), vec!["aXY".to_string(), "b".into()])
        );
        let caret = d.backspace(caret);
        assert_eq!(
            (caret, texts(&d)),
            (Pos::new(0, 3), vec!["aXYb".to_string()])
        );
        let caret = d.backspace(caret);
        assert_eq!(
            (caret, texts(&d)),
            (Pos::new(0, 2), vec!["aXb".to_string()])
        );
        d.break_paragraph(Pos::new(0, 3));
        assert_eq!(d.delete_forward(Pos::new(0, 3)), Pos::new(0, 3));
        assert_eq!(texts(&d), vec!["aXb"]);
    }

    #[test]
    fn formatting_alignment_and_lists_over_a_selection() {
        let mut d = doc(&["abc", "def", "ghi"]);
        d.format_selection(Pos::new(0, 2), Pos::new(2, 1), &FmtChange::ToggleBold);
        let rich = d.to_rich();
        assert_eq!(
            rich.blocks()[0],
            TextBlock::paragraph(vec![Run::plain("ab"), bold("c")])
        );
        assert_eq!(rich.blocks()[1], TextBlock::paragraph(vec![bold("def")]));
        assert_eq!(
            rich.blocks()[2],
            TextBlock::paragraph(vec![bold("g"), Run::plain("hi")])
        );
        assert!(d.fmt_of_selection(Pos::new(0, 2), Pos::new(2, 1)).bold);
        assert!(!d.fmt_of_selection(Pos::new(0, 0), Pos::new(2, 1)).bold);
        // All bold → toggling removes it.
        d.format_selection(Pos::new(0, 2), Pos::new(2, 1), &FmtChange::ToggleBold);
        assert!(!d.to_rich().has_formatting());
        d.set_align(Pos::new(0, 1), Pos::new(1, 0), Align::Center);
        d.toggle_kind(Pos::new(1, 0), Pos::new(2, 0), ParaKind::Bullet);
        let rich = d.to_rich();
        assert!(matches!(
            rich.blocks()[0],
            TextBlock::Paragraph {
                align: Align::Center,
                ..
            }
        ));
        assert!(matches!(
            rich.blocks()[1],
            TextBlock::ListItem {
                align: Align::Center,
                ..
            }
        ));
        assert!(matches!(rich.blocks()[2], TextBlock::ListItem { .. }));
        d.toggle_kind(Pos::new(1, 0), Pos::new(2, 0), ParaKind::Bullet);
        assert!(matches!(
            d.to_rich().blocks()[2],
            TextBlock::Paragraph { .. }
        ));
    }

    #[test]
    fn copy_and_paste_fragments() {
        let mut d = Doc::from_rich(&RichText::new(vec![
            TextBlock::paragraph(vec![Run::plain("Hallo "), bold("Welt")]),
            TextBlock::Table(Table::from_texts(vec![vec!["a".into(), "b".into()]])),
            TextBlock::text("Ende"),
        ]));
        let fragment = d.fragment(Pos::new(0, 6), Pos::new(2, 2));
        assert_eq!(texts(&fragment), vec!["Welt", "[Tabelle]", "En"]);
        assert_eq!(
            d.plain_text(Pos::new(0, 6), Pos::new(2, 2)),
            "Welt\na\tb\nEn"
        );
        // Pasting one paragraph keeps its formats inside the text.
        let one = d.fragment(Pos::new(0, 6), Pos::new(0, 10));
        let caret = d.insert_fragment(Pos::new(2, 0), &one);
        assert_eq!(caret, Pos::new(2, 4));
        assert_eq!(
            d.to_rich().blocks()[2],
            TextBlock::paragraph(vec![bold("Welt"), Run::plain("Ende")])
        );
        // Several blocks split the paragraph.
        let mut target = doc(&["XY"]);
        let caret = target.insert_fragment(Pos::new(0, 1), &fragment);
        assert_eq!(texts(&target), vec!["XWelt", "[Tabelle]", "EnY"]);
        assert_eq!(caret, Pos::new(2, 2));
    }
}
