//! Langtext mit Textergänzungen an ihrer Stelle: turns a long text and the
//! complements of a position into blocks to show, with each complement as
//! a marked piece („[TE 1: Farbe …………]“, the value when filled in).
//!
//! Interface for the long-text editor:
//! - [`display`]`(text, complements) -> Vec<DisplayBlock>`: pure, no egui.
//!   Paragraphs and list items hold [`Piece`]s – formatted text or a
//!   [`ComplementMark`] (index into the complements, label, filled, owner).
//!   Complements without a usable place come last in a block with
//!   `trailing: true`.
//! - [`plain`] for a text version, [`show`] draws the blocks read-only.

use eframe::egui::{self, text::LayoutJob, FontId, RichText as UiText, TextFormat};
use proava_domain::{ComplementKind, RichText, Run, TextBlock, TextComplement};

use crate::theme::Palette;

/// One complement as shown in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplementMark {
    /// Index into the position's complements.
    pub index: usize,
    /// „[TE 1: Farbe …………]“ or „[TE 1: Farbe reinweiß]“.
    pub label: String,
    pub filled: bool,
    /// Auftraggeber (true) or Bieter (false) fills it in.
    pub owner: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(Run),
    Complement(ComplementMark),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayBlock {
    Paragraph(Vec<Piece>),
    ListItem {
        ordered: bool,
        pieces: Vec<Piece>,
    },
    Table(Vec<Vec<String>>),
    Image {
        file: String,
    },
    /// Complements on lines of their own; `trailing` = they have no place
    /// in the text (typed by hand, older projects) and follow it.
    Complements {
        marks: Vec<ComplementMark>,
        trailing: bool,
    },
}

/// „[TE 1: Farbe …………]“, filled: „[TE 1: Farbe reinweiß matt]“.
pub fn label(index: usize, complement: &TextComplement) -> String {
    let marker = if complement.marker.trim().is_empty() {
        (index + 1).to_string()
    } else {
        complement.marker.trim().to_string()
    };
    let body = complement.body.trim();
    let body = if body.is_empty() {
        "…………"
    } else {
        body
    };
    let text = [complement.caption.trim(), body, complement.tail.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let who = match complement.kind {
        ComplementKind::Bidder => " (Bieter)",
        ComplementKind::Owner => "",
    };
    format!("[TE {marker}: {text}{who}]")
}

fn mark(index: usize, complement: &TextComplement) -> ComplementMark {
    ComplementMark {
        index,
        label: label(index, complement),
        filled: !complement.body.trim().is_empty(),
        owner: complement.kind == ComplementKind::Owner,
    }
}

/// The long text with the complements at their place.
pub fn display(text: &RichText, complements: &[TextComplement]) -> Vec<DisplayBlock> {
    let blocks = text.blocks();
    // (offset, mark) inline per block; own lines before a block; trailing.
    let mut inline: Vec<Vec<(usize, ComplementMark)>> = vec![Vec::new(); blocks.len()];
    let mut own_lines: Vec<Vec<ComplementMark>> = vec![Vec::new(); blocks.len() + 1];
    let mut trailing = Vec::new();
    for (index, complement) in complements.iter().enumerate() {
        let mark = mark(index, complement);
        match complement.anchor {
            None => trailing.push(mark),
            Some(anchor) => {
                let block = anchor.block.min(blocks.len());
                let text_block = blocks.get(block).is_some_and(|block| {
                    matches!(
                        block,
                        TextBlock::Paragraph { .. } | TextBlock::ListItem { .. }
                    )
                });
                if anchor.own_line || !text_block {
                    own_lines[block].push(mark);
                } else {
                    inline[block].push((anchor.offset, mark));
                }
            }
        }
    }

    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if !own_lines[index].is_empty() {
            out.push(DisplayBlock::Complements {
                marks: std::mem::take(&mut own_lines[index]),
                trailing: false,
            });
        }
        let marks = std::mem::take(&mut inline[index]);
        out.push(match block {
            TextBlock::Paragraph { runs, .. } => DisplayBlock::Paragraph(weave(runs, marks)),
            TextBlock::ListItem { ordered, runs, .. } => DisplayBlock::ListItem {
                ordered: *ordered,
                pieces: weave(runs, marks),
            },
            TextBlock::Table(table) => DisplayBlock::Table(table.texts()),
            TextBlock::Image { file, .. } => DisplayBlock::Image { file: file.clone() },
        });
    }
    if let Some(last) = own_lines.pop().filter(|marks| !marks.is_empty()) {
        out.push(DisplayBlock::Complements {
            marks: last,
            trailing: false,
        });
    }
    if !trailing.is_empty() {
        out.push(DisplayBlock::Complements {
            marks: trailing,
            trailing: true,
        });
    }
    out
}

/// Runs with the marks put in at their character offsets (clamped to the end).
fn weave(runs: &[Run], mut marks: Vec<(usize, ComplementMark)>) -> Vec<Piece> {
    marks.sort_by_key(|(offset, mark)| (*offset, mark.index));
    let mut marks = marks.into_iter().peekable();
    let mut pieces = Vec::new();
    let mut position = 0;
    for run in runs {
        let mut current = String::new();
        for character in run.text.chars() {
            while let Some((_, mark)) = marks.next_if(|(offset, _)| *offset <= position) {
                if !current.is_empty() {
                    pieces.push(Piece::Text(Run {
                        text: std::mem::take(&mut current),
                        ..run.clone()
                    }));
                }
                pieces.push(Piece::Complement(mark));
            }
            current.push(character);
            position += 1;
        }
        if !current.is_empty() {
            pieces.push(Piece::Text(Run {
                text: current,
                ..run.clone()
            }));
        }
    }
    pieces.extend(marks.map(|(_, mark)| Piece::Complement(mark)));
    pieces
}

/// Text version, one line per block, complements as „[TE …]“.
pub fn plain(blocks: &[DisplayBlock]) -> String {
    let pieces_text = |pieces: &[Piece]| -> String {
        pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text(run) => run.text.clone(),
                Piece::Complement(mark) => mark.label.clone(),
            })
            .collect()
    };
    let mut lines = Vec::new();
    for block in blocks {
        match block {
            DisplayBlock::Paragraph(pieces) => lines.push(pieces_text(pieces)),
            DisplayBlock::ListItem { pieces, .. } => {
                lines.push(format!("- {}", pieces_text(pieces)))
            }
            DisplayBlock::Table(rows) => lines.extend(rows.iter().map(|cells| cells.join("\t"))),
            DisplayBlock::Image { .. } => {}
            DisplayBlock::Complements { marks, .. } => {
                lines.extend(marks.iter().map(|mark| mark.label.clone()))
            }
        }
    }
    lines.join("\n")
}

/// Draws the blocks read-only (used below the long-text editor).
pub fn show(ui: &mut egui::Ui, palette: &Palette, blocks: &[DisplayBlock]) {
    let width = ui.available_width();
    let size = 13.0;
    let text_format = |run: &Run| {
        let font = if run.bold {
            crate::fonts::bold(size)
        } else if run.italic {
            crate::fonts::italic(size)
        } else {
            FontId::proportional(size)
        };
        TextFormat {
            font_id: font,
            color: palette.text,
            underline: if run.underline {
                egui::Stroke::new(1.0, palette.text)
            } else {
                egui::Stroke::NONE
            },
            ..TextFormat::default()
        }
    };
    let mark_format = |mark: &ComplementMark| {
        let (color, fill) = if mark.filled {
            (palette.accent, palette.accent_soft)
        } else {
            (palette.warn, palette.warn.gamma_multiply(0.16))
        };
        TextFormat {
            font_id: FontId::proportional(size),
            color,
            background: fill,
            ..TextFormat::default()
        }
    };
    let paragraph = |ui: &mut egui::Ui, prefix: &str, pieces: &[Piece]| {
        let mut job = LayoutJob::default();
        job.wrap.max_width = width;
        if !prefix.is_empty() {
            job.append(prefix, 0.0, text_format(&Run::default()));
        }
        for piece in pieces {
            match piece {
                Piece::Text(run) => job.append(&run.text, 0.0, text_format(run)),
                Piece::Complement(mark) => job.append(&mark.label, 2.0, mark_format(mark)),
            }
        }
        ui.label(job);
    };
    for block in blocks {
        match block {
            DisplayBlock::Paragraph(pieces) => paragraph(ui, "", pieces),
            DisplayBlock::ListItem { ordered, pieces } => {
                paragraph(ui, if *ordered { "1. " } else { "• " }, pieces)
            }
            DisplayBlock::Table(rows) => {
                for cells in rows {
                    ui.label(
                        UiText::new(cells.join("  |  "))
                            .size(size)
                            .color(palette.text),
                    );
                }
            }
            DisplayBlock::Image { file } => {
                ui.label(
                    UiText::new(format!("[Bild {file}]"))
                        .size(12.0)
                        .color(palette.text_dim),
                );
            }
            DisplayBlock::Complements { marks, trailing } => {
                if *trailing {
                    ui.label(
                        UiText::new("Textergänzungen (ohne Platz im Text):")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                }
                for mark in marks {
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = width;
                    job.append(&mark.label, 0.0, mark_format(mark));
                    ui.label(job);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proava_domain::{ComplementKind, RichText, Run, TextAnchor, TextBlock, TextComplement};

    use super::*;

    fn complement(body: &str, anchor: Option<TextAnchor>) -> TextComplement {
        TextComplement {
            marker: "1".into(),
            kind: ComplementKind::Owner,
            caption: "Farbe".into(),
            body: body.into(),
            tail: String::new(),
            anchor,
        }
    }

    #[test]
    fn complement_stands_at_its_place_and_keeps_formatting() {
        let text = RichText::new(vec![TextBlock::paragraph(vec![
            Run::plain("Gerät, "),
            Run {
                text: "Farbe .".into(),
                bold: true,
                ..Run::default()
            },
        ])]);
        let anchor = TextAnchor {
            block: 0,
            offset: 13,
            own_line: false,
        };
        let blocks = display(&text, &[complement("", Some(anchor))]);
        assert_eq!(plain(&blocks), "Gerät, Farbe [TE 1: Farbe …………].");
        let DisplayBlock::Paragraph(pieces) = &blocks[0] else {
            panic!()
        };
        assert!(matches!(&pieces[1], Piece::Text(run) if run.bold && run.text == "Farbe "));
        assert!(matches!(&pieces[2], Piece::Complement(mark) if !mark.filled && mark.owner));
        assert!(matches!(&pieces[3], Piece::Text(run) if run.bold && run.text == "."));

        let filled = display(&text, &[complement("reinweiß", Some(anchor))]);
        assert_eq!(plain(&filled), "Gerät, Farbe [TE 1: Farbe reinweiß].");
    }

    #[test]
    fn own_lines_trailing_and_clamping() {
        let text = RichText::plain("Zeile 1\nZeile 2");
        let mut bidder = complement("", None);
        bidder.kind = ComplementKind::Bidder;
        bidder.caption = "Fabrikat:".into();
        let own = complement(
            "x",
            Some(TextAnchor {
                block: 1,
                offset: 0,
                own_line: true,
            }),
        );
        let far = complement(
            "y",
            Some(TextAnchor {
                block: 9,
                offset: 99,
                own_line: false,
            }),
        );
        let blocks = display(&text, &[own, far, bidder]);
        assert_eq!(
            plain(&blocks),
            "Zeile 1\n[TE 1: Farbe x]\nZeile 2\n[TE 1: Farbe y]\n[TE 1: Fabrikat: ………… (Bieter)]"
        );
        assert!(matches!(
            blocks.last(),
            Some(DisplayBlock::Complements { trailing: true, .. })
        ));
        // Versatz hinter dem Textende: ans Ende des Absatzes.
        let short = display(
            &RichText::plain("ab"),
            &[complement(
                "z",
                Some(TextAnchor {
                    block: 0,
                    offset: 50,
                    own_line: false,
                }),
            )],
        );
        assert_eq!(plain(&short), "ab[TE 1: Farbe z]");
    }
}
