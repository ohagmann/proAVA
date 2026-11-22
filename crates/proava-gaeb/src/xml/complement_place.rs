//! Text complements at their place in the long text (DA XML).
//!
//! GAEB keeps a complement between the `<Text>` parts of `DetailTxt`; text
//! without `<p>` continues the paragraph before it across that border
//! („Farbe <TextComplement/>.“). The writer therefore writes a paragraph
//! holding complements as bare spans, split at the anchors. [`segments`]
//! is that layout; [`as_read`] is what the reader makes of it (used by
//! `restrict::as_written`), so writing and reading agree.
//!
//! Bare spans carry no paragraph: alignment and the border between two such
//! paragraphs need a `<p>`, which cannot reach across a complement. So a
//! bare paragraph that is not left-aligned or follows another one starts
//! with an empty `<span style="text-align:…"/>` ([`Segment::Start`]): after
//! the `<br/>` between two of them proAVA reads a new paragraph, other
//! programs see the line break as before (in the files of other programs
//! no span carries `text-align`).

use crate::model::{Align, RichText, Run, TextBlock, TextComplement};

/// One piece of `DetailTxt` in writing order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Segment {
    /// A whole block written as usual (`<p>`, list, table, image).
    Block(usize),
    /// Text continuing the open paragraph (spans without `<p>`).
    Bare(Vec<Run>),
    /// `<br/>` between two bare paragraphs (other programs: a line break).
    Break,
    /// Start of a bare paragraph with its alignment: an empty span with
    /// `text-align` (after a [`Segment::Break`]: a new paragraph).
    Start(Align),
    /// The complement with this index.
    Complement(usize),
}

fn is_paragraph(block: &TextBlock) -> bool {
    matches!(block, TextBlock::Paragraph { .. })
}

fn char_len(runs: &[Run]) -> usize {
    runs.iter().map(|run| run.text.chars().count()).sum()
}

/// The layout with complements at their anchors; None when no complement
/// stands inside the text (then everything follows the text as before).
pub(crate) fn segments(text: &RichText, complements: &[TextComplement]) -> Option<Vec<Segment>> {
    let count = text.blocks.len();
    let inside = complements
        .iter()
        .any(|complement| complement.anchor.is_some_and(|anchor| anchor.block < count));
    if !inside {
        return None;
    }
    // Own lines before block i (i == count: at the end), inline per block.
    let mut own: Vec<Vec<usize>> = vec![Vec::new(); count + 1];
    let mut inline: Vec<Vec<(usize, usize)>> = vec![Vec::new(); count];
    let mut trailing = Vec::new();
    for (index, complement) in complements.iter().enumerate() {
        let Some(anchor) = complement.anchor else {
            trailing.push(index);
            continue;
        };
        let block = anchor.block.min(count);
        let paragraph = text.blocks.get(block).is_some_and(is_paragraph);
        if anchor.own_line || !paragraph {
            own[block].push(index);
        } else {
            let runs = match &text.blocks[block] {
                TextBlock::Paragraph { runs, .. } => runs,
                _ => unreachable!("checked above"),
            };
            inline[block].push((anchor.offset.min(char_len(runs)), index));
        }
    }
    let mut out = Vec::new();
    let mut previous_bare = false;
    for (index, block) in text.blocks.iter().enumerate() {
        for &complement in &own[index] {
            out.push(Segment::Complement(complement));
            previous_bare = false;
        }
        let marks = &mut inline[index];
        if marks.is_empty() {
            out.push(Segment::Block(index));
            previous_bare = false;
            continue;
        }
        marks.sort();
        let TextBlock::Paragraph { runs, align } = block else {
            unreachable!("only paragraphs get inline marks")
        };
        if previous_bare {
            out.push(Segment::Break);
            out.push(Segment::Start(*align));
        } else if *align != Align::Left {
            out.push(Segment::Start(*align));
        }
        split(runs, marks, &mut out);
        previous_bare = true;
    }
    out.extend(own[count].iter().map(|&index| Segment::Complement(index)));
    out.extend(trailing.into_iter().map(Segment::Complement));
    Some(out)
}

/// Bare pieces of a paragraph with the complements at their offsets.
fn split(runs: &[Run], marks: &[(usize, usize)], out: &mut Vec<Segment>) {
    let mut marks = marks.iter().peekable();
    let mut pending: Vec<Run> = Vec::new();
    let mut position = 0;
    let flush = |pending: &mut Vec<Run>, out: &mut Vec<Segment>| {
        let runs: Vec<Run> = std::mem::take(pending)
            .into_iter()
            .filter(|run| !run.text.is_empty())
            .collect();
        if !runs.is_empty() {
            out.push(Segment::Bare(runs));
        }
    };
    for run in runs {
        let mut current = Run {
            text: String::new(),
            ..run.clone()
        };
        for character in run.text.chars() {
            while let Some(&&(_, index)) = marks.peek().filter(|(offset, _)| *offset <= position) {
                marks.next();
                pending.push(std::mem::replace(
                    &mut current,
                    Run {
                        text: String::new(),
                        ..run.clone()
                    },
                ));
                flush(&mut pending, out);
                out.push(Segment::Complement(index));
            }
            current.text.push(character);
            position += 1;
        }
        pending.push(current);
    }
    flush(&mut pending, out);
    for &(_, index) in marks {
        out.push(Segment::Complement(index));
    }
}

/// The text the reader gets back from [`segments`]: with the anchor marks
/// still in it (`mark` per complement, in segment order) – the caller joins
/// hard-wrapped lines and takes the anchors like the reader does. Images
/// are dropped when they are not written.
pub(crate) fn as_read(text: &RichText, segments: &[Segment], images: bool, mark: char) -> RichText {
    let mut blocks = Vec::new();
    let mut open: Option<(Vec<Run>, Align)> = None;
    let close = |open: &mut Option<(Vec<Run>, Align)>, blocks: &mut Vec<TextBlock>| {
        if let Some((runs, align)) = open.take() {
            blocks.push(TextBlock::Paragraph {
                runs: super::rich_text::normalize_runs(runs),
                align,
            });
        }
    };
    for segment in segments {
        match segment {
            Segment::Block(index) => {
                close(&mut open, &mut blocks);
                let block = &text.blocks[*index];
                if images || !matches!(block, TextBlock::Image { .. }) {
                    blocks.push(block.clone());
                }
            }
            Segment::Bare(runs) => open
                .get_or_insert_with(|| (Vec::new(), Align::Left))
                .0
                .extend(runs.iter().cloned()),
            Segment::Break => {
                let runs = &mut open.get_or_insert_with(|| (Vec::new(), Align::Left)).0;
                match runs.last_mut() {
                    Some(run) => run.text.push('\n'),
                    None => runs.push(Run::plain("\n")),
                }
            }
            Segment::Start(align) => {
                if paragraph_start(&mut open, *align, mark) {
                    close(&mut open, &mut blocks);
                    open = Some((Vec::new(), *align));
                }
            }
            Segment::Complement(_) => open
                .get_or_insert_with(|| (Vec::new(), Align::Left))
                .0
                .push(Run::plain(mark.to_string())),
        }
    }
    close(&mut open, &mut blocks);
    RichText { blocks }
}

/// What the reader does with a [`Segment::Start`] (`<span style=
/// "text-align:…"/>`) for the open paragraph: none open – opens one with the
/// alignment; one holding only complement marks – takes the alignment; one
/// ending in a line break (`<br/>` before it) – drops that break and returns
/// true: the paragraph ends, a new one starts. Otherwise nothing.
pub(crate) fn paragraph_start(
    open: &mut Option<(Vec<Run>, Align)>,
    align: Align,
    mark: char,
) -> bool {
    let Some((runs, current)) = open else {
        *open = Some((Vec::new(), align));
        return false;
    };
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    if text.ends_with('\n') {
        if let Some(index) = runs.iter().rposition(|run| !run.text.is_empty()) {
            runs[index].text.pop();
            if runs[index].text.is_empty() {
                runs.remove(index);
            }
        }
        return true;
    }
    if text.chars().all(|character| character == mark) {
        *current = align;
    }
    false
}

/// Complement indices in writing order.
pub(crate) fn order(segments: &[Segment]) -> Vec<usize> {
    segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Complement(index) => Some(*index),
            _ => None,
        })
        .collect()
}
