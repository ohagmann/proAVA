//! Eigenschaftstests „Rundlauf“: zufällige Projekte (siehe `zufall`)
//!
//! - Projektdatei schreiben → lesen = dasselbe Projekt (alles, auch Ids,
//!   Bilder, Angebote, Bieterverzeichnis, Stammdaten), erneut schreiben =
//!   dieselben Bytes;
//! - GAEB X83, X82 und D83 über denselben Weg wie die Oberfläche
//!   (`GaebPdfExchange`) schreiben → als neues Projekt lesen: Gliederung,
//!   Ordnungszahlen, Texte, Mengen, Einheiten, Preise (X82), Positionsarten
//!   und Bezüge bleiben – was das Format tragen kann.
//!
//! Feste Zufallssaat; längere Läufe siehe docs/BUILD.md.

mod zufall;

use std::path::PathBuf;

use proava_application::{ExchangeFiles, GaebExport};
use proava_domain::{Lv, LvRow, Project, ProjectName, RichText, TextBlock};
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::project_xml::write_project_xml;
use proava_infrastructure::{project_from_bytes, project_to_bytes};
use proptest::prelude::*;
use zufall::{config, project, xml_problem, Alphabet, Levels};

/// A folder only this test uses: a unique name, created exclusively (two
/// tests – also of other test runs at the same time – never share one); it is
/// removed with this value, nothing else.
struct TempDir(PathBuf, #[allow(dead_code)] tempfile::TempDir);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("proava-eigenschaften-{label}-"))
            .tempdir()
            .unwrap();
        Self(dir.path().to_path_buf(), dir)
    }
}

/// Whitespace collapsed to single spaces, trimmed.
fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every run text of a rich text (paragraphs, list items, table cells);
/// with `markers` the list items get the „- “/„1. “ GAEB 90 writes as text.
fn words(text: &RichText, markers: bool) -> String {
    let mut out = String::new();
    let mut number = 0;
    for block in text.blocks() {
        let ordered = matches!(block, TextBlock::ListItem { ordered: true, .. });
        number = if ordered { number + 1 } else { 0 };
        match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                // An empty list item carries nothing (GAEB 90 drops it).
                let blank = runs.iter().all(|run| run.text.trim().is_empty());
                if markers && !blank {
                    match block {
                        TextBlock::ListItem { ordered: true, .. } => {
                            out.push_str(&format!("{number}. "))
                        }
                        TextBlock::ListItem { .. } => out.push_str("- "),
                        _ => {}
                    }
                }
                for run in runs {
                    out.push_str(&run.text);
                }
            }
            TextBlock::Table(table) => {
                for row in &table.rows {
                    for cell in row {
                        out.push(' ');
                        for run in &cell.runs {
                            out.push_str(&run.text);
                        }
                    }
                }
            }
            TextBlock::Image { .. } => {}
        }
        out.push(' ');
    }
    norm(&out)
}

/// The first differing line of two debug outputs, with some context.
fn first_difference(expected: &str, got: &str) -> String {
    // The session key of an LV is no content (PartialEq skips it).
    let content = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("key:"))
            .map(str::to_string)
            .collect()
    };
    let expected = content(expected);
    let got = content(got);
    let at = expected
        .iter()
        .zip(&got)
        .position(|(left, right)| left != right)
        .unwrap_or(expected.len().min(got.len()));
    let from = at.saturating_sub(12);
    format!(
        "erste Abweichung in Zeile {at}:\n{}\n--- vorher: {:?}\n--- nachher: {:?}",
        expected[from..at].join("\n"),
        expected.get(at),
        got.get(at)
    )
}

/// The nearest font size GAEB allows (Fachdokumentation 3.3, 11.3.2).
fn gaeb_size(size: f32) -> f32 {
    const SIZES: [f32; 19] = [
        3.0, 4.5, 6.5, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 15.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0,
        34.0, 36.0, 48.0,
    ];
    SIZES
        .into_iter()
        .min_by(|left, right| (left - size).abs().total_cmp(&(right - size).abs()))
        .unwrap()
}

/// Formatting of a rich text as GAEB DA XML carries it: per block its kind
/// and alignment, the text with whitespace collapsed and a marker wherever
/// bold/italic/underline/font/size change (whitespace has no format).
fn formatting(text: &RichText) -> String {
    use proava_domain::{Align, Run};
    fn runs_signature(runs: &[Run]) -> String {
        let mut out = String::new();
        let mut current: Option<String> = None;
        let mut space = false;
        for run in runs {
            let format = format!(
                "{}{}{}{}{}",
                if run.bold { "b" } else { "" },
                if run.italic { "i" } else { "" },
                if run.underline { "u" } else { "" },
                // The import keeps standard font and size as „standard“.
                run.font
                    .as_deref()
                    .filter(|font| !matches!(
                        font.trim().to_ascii_lowercase().as_str(),
                        "arial" | "helvetica" | "liberation sans"
                    ))
                    .map(|font| format!(" {font}"))
                    .unwrap_or_default(),
                run.size_pt
                    .map(gaeb_size)
                    .filter(|size| *size != 10.0)
                    .map(|size| format!(" {size}pt"))
                    .unwrap_or_default(),
            );
            for character in run.text.chars() {
                if character.is_whitespace() {
                    space = true;
                    continue;
                }
                if space && !out.is_empty() {
                    out.push(' ');
                }
                space = false;
                if current.as_deref() != Some(format.as_str()) {
                    out.push_str(&format!("[{format}]"));
                    current = Some(format.clone());
                }
                out.push(character);
            }
        }
        out
    }
    // Alignment only counts where there is text to align.
    let align = |align: &Align, runs: &str| {
        if runs.is_empty() {
            String::new()
        } else {
            format!("{align:?}")
        }
    };
    text.blocks()
        .iter()
        .filter_map(|block| match block {
            TextBlock::Paragraph { runs, align: a } => {
                let runs = runs_signature(runs);
                Some(format!("P{} {runs}", align(a, &runs)))
            }
            TextBlock::ListItem {
                ordered,
                level,
                runs,
                align: a,
            } => {
                let runs = runs_signature(runs);
                Some(format!(
                    "L{}{level}{} {runs}",
                    if *ordered { "1" } else { "-" },
                    align(a, &runs)
                ))
            }
            TextBlock::Table(table) => Some(format!(
                "T{} {:?} {}",
                table.header_rows,
                // All automatic = no widths; GAEB writes whole pixels (1/96 inch).
                if table.column_widths_mm.iter().all(Option::is_none) {
                    Vec::new()
                } else {
                    let mut widths: Vec<Option<i64>> = table
                        .column_widths_mm
                        .iter()
                        .map(|mm| mm.map(|mm| (mm * 96.0 / 25.4).round() as i64))
                        .collect();
                    // A missing entry is automatic too.
                    while widths.last() == Some(&None) {
                        widths.pop();
                    }
                    widths
                },
                table
                    .rows
                    .iter()
                    .map(|cells| cells
                        .iter()
                        .map(|cell| {
                            let runs = runs_signature(&cell.runs);
                            format!("{} {runs}", align(&cell.align, &runs))
                        })
                        .collect::<Vec<_>>()
                        .join(" | "))
                    .collect::<Vec<_>>()
                    .join(" / ")
            )),
            TextBlock::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(" ¶ ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    X83,
    X82,
    D83,
}

impl Format {
    fn file(self) -> &'static str {
        match self {
            Format::X83 => "lv.X83",
            Format::X82 => "lv.X82",
            Format::D83 => "lv.D83",
        }
    }

    fn kind(self) -> GaebExport {
        match self {
            Format::X82 => GaebExport::PricedLv,
            Format::X83 | Format::D83 => GaebExport::RequestForQuotation,
        }
    }

    /// The text as the format can carry it. GAEB 90: code page 850; a
    /// bullet alone on a line („*“, „-“, „o“ …) may mark the next line as a
    /// list item when read back – such single characters do not count; and
    /// lines are 55 characters: a longer word is cut and read back with a
    /// blank, so blanks do not count either (the XML round trip checks the
    /// exact words); a backslash with lowercase letters may be read as RTF.
    fn carry(self, text: &str) -> String {
        match self {
            Format::D83 => {
                // RTF-looking control words („\\b0“) are taken for remnants
                // of other programs and removed (before the code page, as
                // when reading).
                let mut plain = String::new();
                let mut rest = text;
                while let Some(at) = rest.find('\\') {
                    plain.push_str(&rest[..at]);
                    let after = &rest[at + 1..];
                    // „\\'e4“: a character written the RTF way.
                    let hex = after
                        .strip_prefix('\'')
                        .and_then(|hex| hex.get(..2))
                        .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                    if let Some(byte) = hex {
                        plain.push_str(&proava_gaeb::datan::decode(
                            &[byte],
                            proava_gaeb::datan::Encoding::Windows1252,
                        ));
                        rest = &after[3..];
                        continue;
                    }
                    let word = after.len()
                        - after
                            .trim_start_matches(|c: char| c.is_ascii_lowercase())
                            .len();
                    let digits = after[word..].len()
                        - after[word..]
                            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
                            .len();
                    if word > 0 {
                        rest = &after[word + digits..];
                    } else {
                        plain.push('\\');
                        rest = after;
                    }
                }
                plain.push_str(rest);
                let encoding = proava_gaeb::datan::Encoding::Cp850;
                let plain = proava_gaeb::datan::decode(
                    &proava_gaeb::datan::encode(&plain, encoding),
                    encoding,
                );
                plain
                    .split_whitespace()
                    .filter(|word| !["l", "•", "-", "*", "·", "–", "o"].contains(word))
                    .collect::<String>()
            }
            _ => text.to_string(),
        }
    }
}

/// The long text after one pass through the editor's fields (embed,
/// extract). A paragraph with a field inside goes to GAEB DA XML as bare
/// spans (`complement_place`); its alignment and the border to a following
/// such paragraph travel in an empty `<span style="text-align:…"/>`, so the
/// layout comes back as it was.
fn editor_form(position: &proava_domain::Position) -> RichText {
    use proava_domain::complement_text::{embed, extract};
    extract(
        &embed(position.long_text(), position.complements()),
        position.complements(),
    )
    .0
}

/// „2.“ and „1.“ alike: digits before a full stop become „#“.
fn any_number(text: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if character == '.' && !digits.is_empty() {
            out.push('#');
        } else {
            out.push_str(&digits);
        }
        digits.clear();
        out.push(character);
    }
    out.push_str(&digits);
    out
}

/// What must survive a GAEB round trip, row by row.
fn outline(lv: &Lv, format: Format, words_only: &std::collections::HashSet<String>) -> Vec<String> {
    let text = |text: &str| norm(&format.carry(text));
    let rich = |rich: &RichText| match format {
        Format::D83 => norm(&format.carry(&words(rich, true))),
        _ => formatting(rich),
    };
    let long_text = |position: &proava_domain::Position| {
        let text = editor_form(position);
        match format {
            _ if !words_only.contains(position.ordinal()) => rich(&text),
            // GAEB 90 has no lists, only „1. “ in the text: a field line
            // between numbered items restarts the count – the marker counts,
            // not its number.
            Format::D83 => any_number(&rich(&text)),
            _ => norm(&words(&text, false)),
        }
    };
    let ordinal_of = |id| {
        lv.position_by_id(id)
            .map(|position| position.ordinal().to_string())
    };
    lv.rows()
        .iter()
        .filter_map(|row| {
            Some(match row {
                LvRow::Group(group) => {
                    format!(
                        "T{} {} {}",
                        group.level(),
                        group.ordinal(),
                        text(group.short_text())
                    )
                }
                LvRow::Note(_) | LvRow::Spacer(_) => return None,
                LvRow::Performance(performance) => format!(
                    "A {} {} | {}",
                    performance.number(),
                    text(performance.label()),
                    rich(performance.long_text())
                ),
                LvRow::Position(position) => format!(
                    "P {} {} | {} {} {} {:?} psch={} h={} AB={:?} WH={:?} | {}",
                    position.ordinal(),
                    text(position.short_text()),
                    // In a request for quotation the bidder fills in an open
                    // quantity (GAEB QtyTBD): none is sent.
                    if position.is_quantity_open() && format != Format::X82 {
                        "offen".to_string()
                    } else {
                        position.quantity().normalize().to_string()
                    },
                    text(position.unit()),
                    match format {
                        Format::X82 =>
                            format!("{:?}", position.unit_price().map(|price| price.normalize())),
                        _ => String::new(),
                    },
                    // GAEB has no Zulageposition: an ordinary item that
                    // counts the same (zulage_is_an_ordinary_item…).
                    match position.kind() {
                        proava_domain::PositionKind::Supplement =>
                            proava_domain::PositionKind::Normal,
                        kind => kind,
                    },
                    // GAEB 90 knows „pauschal“ only by the unit „psch“
                    // (the export warns about lump sums with another unit).
                    match format {
                        Format::D83 => position.unit().trim().eq_ignore_ascii_case("psch"),
                        _ => position.is_lump_sum(),
                    },
                    position.is_hourly(),
                    // GAEB carries one reference per item: the repetition
                    // wins (the export warns).
                    lv.performance_of(position)
                        .filter(|_| position.repeats().is_none())
                        .map(|performance| performance.number().to_string()),
                    position.repeats().and_then(ordinal_of),
                    // As the editor holds it: a paragraph of nothing but
                    // Bieterangaben becomes their own line (`extract`), like
                    // GAEB keeps them.
                    long_text(position)
                ),
            })
        })
        .collect()
}

/// Notes (Vorbemerkungen move to the front in GAEB): as a sorted list.
fn notes(lv: &Lv, format: Format) -> Vec<String> {
    let mut notes: Vec<String> = lv
        .rows()
        .iter()
        .filter_map(|row| match row {
            // A note without any text (only empty paragraphs or list
            // items) does not go out. GAEB 90 has no heading of a hint: it
            // opens the text (the words count).
            LvRow::Note(note)
                if !note.text().is_empty()
                    || (format == Format::D83 && !note.heading().trim().is_empty()) =>
            {
                let mut text = words(note.text(), format == Format::D83);
                if format == Format::D83 && !note.heading().trim().is_empty() {
                    text = format!("{} {text}", note.heading().trim());
                }
                Some(norm(&format.carry(&text)))
            }
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect();
    notes.sort();
    notes
}

fn gaeb_round_trip(
    project: &Project,
    format: Format,
    folder: &TempDir,
) -> Result<(), TestCaseError> {
    let exchange = GaebPdfExchange;
    let file = folder.0.join(format.file());
    let warnings = match exchange.export_gaeb(project, format.kind(), &file) {
        Ok(warnings) => warnings,
        // A mask GAEB 90 cannot hold is refused with a clear message.
        Err(error) if format == Format::D83 && error.contains("passt nicht in GAEB 90") => {
            return Ok(())
        }
        Err(error) => {
            return Err(TestCaseError::fail(format!(
                "{format:?} schreiben: {error}"
            )))
        }
    };
    let (back, import_warnings) = exchange
        .import_lv(
            &file,
            &ProjectName::parse("Rundlauf").unwrap(),
            "2026-11-17T21:00:00",
        )
        .map_err(|error| {
            TestCaseError::fail(format!("{format:?} lesen: {error}\nExport: {warnings:?}"))
        })?;
    // What the format cannot carry is announced.
    let warned = |text: &str| warnings.iter().any(|warning| warning.contains(text));
    let lv = project.lv();
    if lv
        .positions()
        .any(|position| position.repeats().is_some() && position.performance().is_some())
    {
        prop_assert!(warned("nur einen Bezug"), "{:?}: {:?}", format, warnings);
    }
    if format == Format::D83
        && lv.positions().any(|position| {
            position.is_lump_sum() && !position.unit().trim().eq_ignore_ascii_case("psch")
        })
    {
        prop_assert!(warned("„psch“"), "{:?}: {:?}", format, warnings);
    }
    // GAEB 90 has no place for a field in the text: there the words count.
    let words_only: std::collections::HashSet<String> = lv
        .positions()
        .filter(|position| {
            format == Format::D83
                && position
                    .complements()
                    .iter()
                    .any(|field| field.anchor.is_some())
        })
        .map(|position| position.ordinal().to_string())
        .collect();
    let expected = outline(project.lv(), format, &words_only);
    let got = outline(back.lv(), format, &words_only);
    if expected != got {
        let first = expected
            .iter()
            .zip(&got)
            .position(|(left, right)| left != right)
            .unwrap_or(expected.len().min(got.len()));
        return Err(TestCaseError::fail(format!(
            "{format:?}: Zeile {first} weicht ab ({} → {} Zeilen)\nvorher:  {:?}\nnachher: {:?}\nExport: {warnings:?}\nImport: {import_warnings:?}",
            expected.len(),
            got.len(),
            expected.get(first),
            got.get(first),
        )));
    }
    prop_assert_eq!(
        notes(project.lv(), format),
        notes(back.lv(), format),
        "{:?}: Hinweistexte",
        format
    );
    // GAEB 90 has no letter levels in its mask (record 00): digits there.
    let pattern = |lv: &Lv| match format {
        Format::D83 => lv.oz_mask().pattern().replace('L', "1"),
        _ => lv.oz_mask().pattern().to_string(),
    };
    prop_assert_eq!(pattern(back.lv()), pattern(project.lv()), "{:?}", format);
    if format == Format::X82 {
        prop_assert_eq!(
            back.lv().net_total(),
            project.lv().net_total(),
            "{:?}: Summe",
            format
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(config(48))]

    /// Texts pasted from Word or PDF bring control characters along; the
    /// X83/X82 for the bidders must still be well-formed XML 1.0 and readable.
    #[test]
    fn gaeb_xml_stays_well_formed_with_control_characters(project in project(Alphabet::All, Levels::Free, 1, 16)) {
        for phase in [proava_gaeb::Phase::Dp83, proava_gaeb::Phase::Dp82] {
            let (document, _) = proava_exchange::lv_to_gaeb(&project, phase);
            let xml = proava_gaeb::xml::write(&document, phase).map_err(|error| TestCaseError::fail(error.to_string()))?;
            if let Some(problem) = xml_problem(&xml) {
                return Err(TestCaseError::fail(format!("{phase:?}: {problem}")));
            }
            proava_gaeb::xml::read(xml.as_bytes()).map_err(|error| TestCaseError::fail(error.to_string()))?;
        }
    }

    #[test]
    fn project_file_keeps_everything(project in project(Alphabet::All, Levels::Free, 3, 24)) {
        project_file_round_trip(&project)?;
        let loaded = project_from_bytes(&project_to_bytes(&project).unwrap()).unwrap();
        prop_assert_eq!(loaded.images(), project.images());
        prop_assert_eq!(loaded.bids(), project.bids());
        prop_assert_eq!(loaded.invitees(), project.invitees());
        for (left, right) in loaded.lvs().zip(project.lvs()) {
            prop_assert_eq!(left.next_row_id(), right.next_row_id());
        }
    }

    #[test]
    fn gaeb_files_keep_what_they_can_carry(project in project(Alphabet::Printable, Levels::Gaeb, 1, 24)) {
        let folder = TempDir::new("gaeb");
        for format in [Format::X83, Format::X82, Format::D83] {
            gaeb_round_trip(&project, format, &folder)?;
        }
    }
}

/// Bieterangaben als Felder im Langtext: der Editor bettet sie als
/// Feldzeichen ein (`embed`) und holt sie nach jeder Änderung zurück
/// (`extract`). Für jeden Langtext und jede Lage der Anker: keine Angabe
/// geht verloren oder doppelt, der Text ohne Felder bleibt, und ein zweiter
/// Durchgang ändert nichts mehr (sonst wandert ein Feld bei jedem Tastendruck).
fn fields_survive_editing(project: &Project) -> Result<(), TestCaseError> {
    use proava_domain::complement_text::{embed, extract, has_fields, with_complements};
    for lv in project.lvs() {
        for position in lv.positions() {
            let (text, complements) = (position.long_text(), position.complements());
            // Print view: never panics, every anchored complement once.
            let _ = with_complements(text, complements, |complement| {
                proava_domain::Run::plain(complement.display())
            });
            let embedded = embed(text, complements);
            let (once, fields) = extract(&embedded, complements);
            prop_assert_eq!(
                fields.len(),
                complements.len(),
                "{}: Angaben verloren oder doppelt",
                position.ordinal()
            );
            let mut before: Vec<&str> = complements.iter().map(|c| c.marker.as_str()).collect();
            let mut after: Vec<&str> = fields.iter().map(|c| c.marker.as_str()).collect();
            before.sort_unstable();
            after.sort_unstable();
            prop_assert_eq!(before, after);
            prop_assert!(!once
                .blocks()
                .iter()
                .filter_map(proava_domain::TextBlock::runs)
                .flatten()
                .any(|run| has_fields(&run.text)));
            let (twice, again) = extract(&embed(&once, &fields), &fields);
            prop_assert_eq!(
                &twice,
                &once,
                "{}: zweiter Durchgang ändert den Text",
                position.ordinal()
            );
            prop_assert_eq!(
                &again,
                &fields,
                "{}: zweiter Durchgang ändert die Anker",
                position.ordinal()
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config(200))]

    #[test]
    fn bidder_fields_survive_editing(project in project(Alphabet::Printable, Levels::Free, 1, 16)) {
        fields_survive_editing(&project)?;
    }
}

fn project_file_round_trip(project: &Project) -> Result<(), TestCaseError> {
    let bytes = project_to_bytes(project).map_err(TestCaseError::fail)?;
    let loaded = project_from_bytes(&bytes).map_err(TestCaseError::fail)?;
    // Also with control characters in the texts: valid XML 1.0.
    if let Some(problem) = xml_problem(&write_project_xml(project).unwrap()) {
        return Err(TestCaseError::fail(format!("projekt.xml: {problem}")));
    }
    if &loaded != project {
        return Err(TestCaseError::fail(first_difference(
            &format!("{project:#?}"),
            &format!("{loaded:#?}"),
        )));
    }
    prop_assert_eq!(
        write_project_xml(&loaded).unwrap(),
        write_project_xml(project).unwrap()
    );
    Ok(())
}

proptest! {
    #![proptest_config(config(40))]

    /// Teurer Lauf (`cargo test -- --ignored lang_`): große Projekte.
    #[test]
    #[ignore]
    fn lang_big_projects_survive_file_and_gaeb(
        file_project in project(Alphabet::All, Levels::Free, 4, 150),
        gaeb_project in project(Alphabet::Printable, Levels::Gaeb, 1, 150),
    ) {
        project_file_round_trip(&file_project)?;
        let folder = TempDir::new("gaeb-lang");
        for format in [Format::X83, Format::X82, Format::D83] {
            gaeb_round_trip(&gaeb_project, format, &folder)?;
        }
    }
}
