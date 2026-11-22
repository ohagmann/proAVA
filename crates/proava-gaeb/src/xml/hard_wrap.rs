//! Long texts that some programs export with a hard line break per printed
//! line (AVAPLAN: one `<p>` per line; "GAEB 90 Import": one `<br/>` per
//! line) are joined into paragraphs like GAEB 90 texts. Texts with real
//! paragraphs, lists, tables, images or formatting stay as they are.

use crate::model::{RichText, Run, TextBlock};
use crate::text_reflow::{list_marker, reflow};

/// Longest line a hard-wrapping exporter produces.
const MAX_WIDTH: usize = 80;
/// Narrowest wrap seen in practice (manufacturer texts at ~32 characters).
const MIN_WIDTH: usize = 30;
/// Lines followed by another line needed to judge.
const MIN_CANDIDATES: usize = 4;

fn plain(run: &Run) -> bool {
    !run.bold && !run.italic && !run.underline
}

fn lines_of(text: &RichText) -> Option<Vec<String>> {
    let mut lines = Vec::new();
    for block in &text.blocks {
        let TextBlock::Paragraph { runs, .. } = block else {
            return None;
        };
        if !runs.iter().all(plain) {
            return None;
        }
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        lines.extend(joined.split('\n').map(|line| line.trim_end().to_string()));
    }
    Some(lines)
}

/// Width the text was wrapped at, when it looks hard-wrapped: of the lines
/// followed by another line (list items and the lines before them aside),
/// at least 70 % reach three quarters of the longest line, and the longest
/// line is 30 to 80 characters long.
fn wrap_width(lines: &[String]) -> Option<usize> {
    let width = lines.iter().map(|line| line.chars().count()).max()?;
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        return None;
    }
    let prose = |line: &str| !line.trim().is_empty() && list_marker(line.trim_start()).is_none();
    let candidates: Vec<usize> = lines
        .windows(2)
        .filter(|pair| prose(&pair[0]) && prose(&pair[1]))
        .map(|pair| pair[0].chars().count())
        .collect();
    if candidates.len() < MIN_CANDIDATES {
        return None;
    }
    let full = candidates
        .iter()
        .filter(|&&length| length * 4 >= width * 3)
        .count();
    (full * 10 >= candidates.len() * 7).then_some(width)
}

/// Joins a hard-wrapped text into paragraphs (see the module doc).
pub(crate) fn unwrap(text: &mut RichText) {
    let Some(lines) = lines_of(text) else { return };
    let Some(width) = wrap_width(&lines) else {
        return;
    };
    text.blocks = reflow(&lines, width)
        .into_iter()
        .map(|block| match block {
            TextBlock::Paragraph { runs, .. } => TextBlock::paragraph(
                runs.into_iter()
                    .filter(|run| !run.text.is_empty())
                    .collect(),
            ),
            TextBlock::ListItem {
                ordered,
                level,
                runs,
                ..
            } => TextBlock::ListItem {
                ordered,
                level,
                runs: runs
                    .into_iter()
                    .filter(|run| !run.text.is_empty())
                    .collect(),
                align: Default::default(),
            },
            other => other,
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[&str], one_paragraph_per_line: bool) -> RichText {
        let blocks = if one_paragraph_per_line {
            lines
                .iter()
                .map(|line| {
                    TextBlock::paragraph(if line.is_empty() {
                        Vec::new()
                    } else {
                        vec![Run {
                            text: line.to_string(),
                            ..Run::default()
                        }]
                    })
                })
                .collect()
        } else {
            vec![TextBlock::paragraph(vec![Run {
                text: lines.join("\n"),
                ..Run::default()
            }])]
        };
        RichText { blocks }
    }

    const WRAPPED: [&str; 9] = [
        "Vorbereiten und Teilnahme in Zusammenwirken mit den AN",
        "Sanitär, Lüftung, Kälte, Heizung und Elektro an den",
        "Abnahmen der Sachverständigen inkl. Bereitstellung der",
        "erforderlichen Hilf- und Betriebsmittel.",
        "",
        "Es kann sich hierbei um Teilabnahmen handeln und eine",
        "mehrmalige Teilnahme erforderlich werden. Sofern die",
        "Abnahmen mangelhaft sind, hat der AN an Wiederholungs-",
        "prüfungen teilzunehmen.",
    ];

    #[test]
    fn hard_wrapped_lines_become_paragraphs() {
        for per_line in [true, false] {
            let mut wrapped = text(&WRAPPED, per_line);
            unwrap(&mut wrapped);
            assert_eq!(
                wrapped.to_plain(),
                "Vorbereiten und Teilnahme in Zusammenwirken mit den AN Sanitär, Lüftung, \
                 Kälte, Heizung und Elektro an den Abnahmen der Sachverständigen inkl. \
                 Bereitstellung der erforderlichen Hilf- und Betriebsmittel.\n\nEs kann sich \
                 hierbei um Teilabnahmen handeln und eine mehrmalige Teilnahme erforderlich \
                 werden. Sofern die Abnahmen mangelhaft sind, hat der AN an \
                 Wiederholungsprüfungen teilzunehmen."
            );
            assert_eq!(wrapped.blocks[1], TextBlock::paragraph(Vec::new()));
        }
    }

    #[test]
    fn real_paragraphs_and_short_line_lists_stay() {
        let real = [
            "Anschluss an bauseitige Absperrklappen-Flansche herstellen",
            "",
            "Die bauseitige Grundinstallation wird ausgehend vom Heizungsverteiler im \
             Technik-Untergeschoss in einem Installations-Steigschacht bis ins 6. OG installiert",
            "Je Geschoss wird es in jeder der 4 Nutzungseinheiten einen Übergabepunkt geben.",
        ];
        let list = [
            "Transformator 230/24VAC 160VA",
            "nach VDE 0570/EN61558",
            "primär: 230VAC ±10%",
            "sekundär: 24VAC, 160VA",
            "Schutzklasse: I",
            "Schutzart: IP00",
            "Typ: TF160",
        ];
        let few = [
            "gemäß Ausführungsbeschreibung:",
            "GA MBE- / DDC- Bedien- und Historisierungskonzept",
            "Punkt: 3.1.1 Anforderungen an den MBE-Server",
        ];
        for lines in [&real[..], &list[..], &few[..]] {
            let original = text(lines, true);
            let mut copy = original.clone();
            unwrap(&mut copy);
            assert_eq!(copy, original);
        }
        let mut bold = text(&WRAPPED, true);
        if let TextBlock::Paragraph { runs, .. } = &mut bold.blocks[0] {
            runs[0].bold = true;
        }
        let original = bold.clone();
        unwrap(&mut bold);
        assert_eq!(bold, original, "Formatierung bleibt erhalten");
    }
}
