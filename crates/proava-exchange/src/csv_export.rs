//! CSV-Export in the proAVA CSV format (the counterpart of [`crate::csv_import`]):
//!
//! ```text
//! OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:
//! 01.481.01.0010;Kurztext;St;1;10,00;10,00;481;Position
//! ;Langtext;;;;;;
//! ;;;;;;;
//! ```
//!
//! One row per title, position, Ausführungsbeschreibung and Text, the
//! Langtext below it one paragraph per row, an empty row between the
//! entries; German numbers without thousands dots, CRLF, UTF-8 with BOM
//! (Excel, LibreOffice) or Windows-1252. Formatting, images and what else
//! has no column fall away – the returned notes name it.

use proava_application::CsvEncoding;
use proava_domain::{Decimal, Lv, LvRow, Position, PositionKind};

use crate::csv_import::Column;

/// The file and what fell away (for the protocol).
#[derive(Debug, Clone, PartialEq)]
pub struct CsvExport {
    pub bytes: Vec<u8>,
    pub notes: Vec<String>,
}

const DELIMITER: char = ';';

/// Positionsart as written in the file.
pub fn kind_text(position: &Position) -> String {
    let mut text = match position.kind() {
        PositionKind::Normal => "Position".to_string(),
        other => other.label().to_string(),
    };
    if let Some(surcharge) = position.surcharge() {
        text = match position.kind() {
            PositionKind::Normal => "Zuschlagsposition".to_string(),
            _ => format!("{text}, Zuschlagsposition"),
        };
        text.push_str(&format!(" ({})", surcharge.kind.key()));
    }
    if position.is_lump_sum() {
        text.push_str(", Pauschal");
    }
    if position.is_hourly() {
        text.push_str(", Stundenlohn");
    }
    text
}

/// A field, in quotes when it holds the Trenner, a quote or a line break.
pub(crate) fn field(text: &str) -> String {
    if text.contains([DELIMITER, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// German number without thousands dots, at least `places` decimals.
pub(crate) fn number(value: Decimal, places: u32) -> String {
    let value = value.normalize();
    let value = if value.scale() < places {
        let mut padded = value;
        padded.rescale(places);
        padded
    } else {
        value
    };
    value.to_string().replace('.', ",")
}

fn line(cells: [String; 8]) -> String {
    cells
        .iter()
        .map(|cell| field(cell))
        .collect::<Vec<_>>()
        .join(";")
}

fn text_rows(rows: &mut Vec<String>, text: &str) {
    for paragraph in text
        .lines()
        .filter(|paragraph| !paragraph.trim().is_empty())
    {
        rows.push(line([
            String::new(),
            paragraph.to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]));
    }
}

use proava_domain::wording::count;

/// The LV as CSV.
pub fn lv_to_csv(lv: &Lv, encoding: CsvEncoding) -> CsvExport {
    let header = Column::FORMAT
        .iter()
        .map(|column| format!("{}:", column.name()))
        .collect::<Vec<_>>()
        .join(";");
    let mut rows = vec![header];
    let surcharges = lv.surcharge_values();
    let groups = lv.effective_cost_groups();
    let mut images = 0;
    let mut formatted = false;
    let mut tables = 0;
    let mut sub_descriptions = 0;
    let mut complements = 0;
    let mut empty_paragraphs = 0;
    // OZ for rows without one: the last position of the same title, else the title.
    let mut place = String::new();
    for (index, row) in lv.rows().iter().enumerate() {
        // Abstandshalter: the empty lines between the rows space them anyway.
        if row.is_spacer() {
            continue;
        }
        if rows.len() > 1 {
            rows.push(";;;;;;;".to_string());
        }
        let group = groups[index]
            .as_ref()
            .map(|(code, _)| code.clone())
            .unwrap_or_default();
        let text = match row {
            LvRow::Position(position) => position.long_text(),
            LvRow::Note(note) => note.text(),
            LvRow::Performance(performance) => performance.long_text(),
            LvRow::Group(_) | LvRow::Spacer(_) => &proava_domain::RichText::default(),
        };
        for block in text.blocks() {
            match block {
                proava_domain::TextBlock::Image { .. } => images += 1,
                proava_domain::TextBlock::Table(_) => tables += 1,
                proava_domain::TextBlock::Paragraph { runs, .. } => {
                    if runs.iter().all(|run| run.text.trim().is_empty()) {
                        empty_paragraphs += 1;
                    }
                    if runs
                        .iter()
                        .any(|run| run.bold || run.italic || run.underline)
                    {
                        formatted = true;
                    }
                }
                proava_domain::TextBlock::ListItem { .. } => formatted = true,
            }
        }
        // Textergänzungen as printed, at their place; those without one after
        // the text (each on a line of its own).
        let fields: &[proava_domain::TextComplement] = match row {
            LvRow::Position(position) => position.complements(),
            LvRow::Note(note) => note.complements(),
            LvRow::Performance(performance) => performance.complements(),
            LvRow::Group(_) | LvRow::Spacer(_) => &[],
        };
        let mut plain = proava_domain::complement_text::with_complements(text, fields, |field| {
            proava_domain::Run::plain(field.print_text().trim_end())
        })
        // Lists as lines: „- “ / „1. “, Unterpunkte two blanks further in.
        .to_lines();
        for field in fields.iter().filter(|field| field.anchor.is_none()) {
            if !plain.is_empty() {
                plain.push('\n');
            }
            plain.push_str(field.print_text().trim_end());
        }
        match row {
            LvRow::Group(title) => {
                place = title.ordinal().to_string();
                rows.push(line([
                    title.ordinal().to_string(),
                    title.short_text().to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    title.cost_group().unwrap_or_default().to_string(),
                    "Titel".to_string(),
                ]));
            }
            LvRow::Position(position) => {
                place = position.ordinal().to_string();
                if !position.sub_descriptions().is_empty() {
                    sub_descriptions += 1;
                }
                complements += position.complements().len();
                let price = position.unit_price();
                // Zuschlagsposition: „%“ as unit, the percentage as EP, the
                // Zuschlag (base × percentage) as GP, no quantity.
                let surcharge = surcharges.get(&position.id());
                let total = surcharge
                    .map(|value| value.amount)
                    .unwrap_or_else(|| position.total_price());
                rows.push(line([
                    position.ordinal().to_string(),
                    position.short_text().to_string(),
                    if surcharge.is_some() {
                        "%".to_string()
                    } else {
                        position.unit().to_string()
                    },
                    if surcharge.is_some() {
                        String::new()
                    } else {
                        number(position.quantity(), 0)
                    },
                    price.map(|price| number(price, 2)).unwrap_or_default(),
                    price
                        .map(|_| number(total.to_decimal(), 2))
                        .unwrap_or_default(),
                    group,
                    kind_text(position),
                ]));
                text_rows(&mut rows, &plain);
            }
            LvRow::Performance(performance) => {
                complements += performance.complements().len();
                rows.push(line([
                    place.clone(),
                    performance.title(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    group,
                    "Ausführungsbeschreibung".to_string(),
                ]));
                text_rows(&mut rows, &plain);
            }
            LvRow::Note(note) => {
                complements += note.complements().len();
                rows.push(line([
                    place.clone(),
                    note.heading().to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    group,
                    if note.is_preliminary() {
                        "Vorbemerkung"
                    } else if note.is_closing() {
                        "Schlussbemerkung"
                    } else {
                        "Text"
                    }
                    .to_string(),
                ]));
                text_rows(&mut rows, &plain);
            }
            LvRow::Spacer(_) => {}
        }
    }
    let mut text = rows.join("\r\n");
    text.push_str("\r\n");

    let mut notes = Vec::new();
    if formatted {
        notes.push(
            "Formatierung der Langtexte (fett, kursiv, Listen) fällt weg – reiner Text."
                .to_string(),
        );
    }
    if images > 0 {
        notes.push(format!(
            "{} aus den Langtexten nicht übernommen.",
            count(images, "Bild", "Bilder")
        ));
    }
    if tables > 0 {
        notes.push(format!(
            "{} im Langtext als Text (Zellen mit Tabulator getrennt).",
            count(tables, "Tabelle", "Tabellen")
        ));
    }
    if empty_paragraphs > 0 {
        notes.push(
            "Leere Absätze im Langtext entfallen (eine leere Zeile trennt die Einträge)."
                .to_string(),
        );
    }
    if sub_descriptions > 0 {
        notes.push(format!(
            "Unterbeschreibungen von {} nicht übernommen.",
            count(sub_descriptions, "Position", "Positionen")
        ));
    }
    if complements > 0 {
        notes.push(format!(
            "{} nur als Text im Langtext.",
            count(complements, "Textergänzung", "Textergänzungen")
        ));
    }
    if lv
        .positions()
        .any(|position| position.performance().is_some() || position.repeats().is_some())
    {
        notes.push(
            "Bezüge auf Ausführungsbeschreibungen und Wiederholungen nicht übernommen.".to_string(),
        );
    }
    let bytes = encode_csv(&text, encoding, &mut notes);
    CsvExport { bytes, notes }
}

/// The text as file bytes: UTF-8 with BOM or Windows-1252 (characters it
/// lacks become „?“ – noted).
pub(crate) fn encode_csv(text: &str, encoding: CsvEncoding, notes: &mut Vec<String>) -> Vec<u8> {
    match encoding {
        CsvEncoding::Utf8Bom => {
            let mut bytes = b"\xEF\xBB\xBF".to_vec();
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        CsvEncoding::Windows1252 => {
            use proava_gaeb::datan::{decode, encode, Encoding};
            let unknown = text
                .chars()
                .filter(|c| {
                    let one = c.to_string();
                    decode(&encode(&one, Encoding::Windows1252), Encoding::Windows1252) != one
                })
                .count();
            if unknown > 0 {
                notes.push(format!(
                    "{unknown} Zeichen gibt es in Windows-1252 nicht – als „?“ geschrieben (UTF-8 behält sie)."
                ));
            }
            encode(text, Encoding::Windows1252)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv_import::read_csv;
    use proava_domain::{LvGroup, Note, OzMask, PerformanceDescription, ProjectName, RichText};

    fn sample() -> Lv {
        let mut lv = Lv::new("Automation", OzMask::parse("11.222.33.PPPP").unwrap());
        let mut preliminary = Note::rich(RichText::plain("Allgemeines\nzweiter Absatz"));
        preliminary.set_heading("Vorbemerkungen");
        preliminary.set_preliminary(true);
        lv.push_note(preliminary);
        let mut title = LvGroup::new(0, "01", "Gebäudeautomation");
        title.set_cost_group(Some("480".into()));
        lv.push_group(title);
        lv.push_group(LvGroup::new(1, "01.481", "Automationseinrichtungen"));
        lv.push_group(LvGroup::new(2, "01.481.01", "Raumautomation; Etage 1"));
        lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "",
            "Kabeltrassen",
            RichText::plain("Trassen nach DIN"),
        )));
        let mut controller = Position::new(
            "01.481.01.0010",
            "Raumregler \"Komfort\"",
            Decimal::new(25, 1),
            "St",
            Some(Decimal::new(123450, 2)),
        );
        controller.set_long_text(RichText::plain("mit Display\n\nund Taster"));
        controller.set_cost_group(Some("481".into()));
        lv.push_position(controller);
        let mut provisional =
            Position::new("01.481.01.0020", "Fühler", Decimal::from(3), "St", None);
        provisional.set_kind(PositionKind::Provisional { with_total: false });
        lv.push_position(provisional);
        let mut lump = Position::new(
            "01.481.01.0030",
            "Inbetriebnahme",
            Decimal::ONE,
            "psch",
            Some(Decimal::from(800)),
        );
        lump.set_kind(PositionKind::Supplement);
        lump.set_lump_sum(true);
        lv.push_position(lump);
        let mut base = Position::new("01.481.01.0040", "Grund", Decimal::ONE, "St", None);
        base.set_kind(PositionKind::Base { group: 1 });
        lv.push_position(base);
        let mut alternative = Position::new("01.481.01.0050", "Wahl", Decimal::ONE, "St", None);
        alternative.set_kind(PositionKind::Alternative {
            group: 1,
            serial: 1,
        });
        lv.push_position(alternative);
        let mut hint = Note::rich(RichText::plain("Hinweis zum Titel"));
        hint.set_heading("");
        lv.push_note(hint);
        lv
    }

    /// Rows with everything the format carries (KG as it applies).
    fn outline(lv: &Lv) -> Vec<String> {
        let groups = lv.effective_cost_groups();
        lv.rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let code = groups[index]
                    .as_ref()
                    .map(|(code, _)| code.as_str())
                    .unwrap_or("-");
                let text = |rich: &RichText| {
                    rich.to_plain()
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join("/")
                };
                match row {
                    LvRow::Group(group) => format!(
                        "T{} {} {} KG {code}",
                        group.level(),
                        group.ordinal(),
                        group.short_text()
                    ),
                    LvRow::Position(position) => format!(
                        "P {} {} | {} {} | {:?} | {} | {} {} {} | {} | KG {code}",
                        position.ordinal(),
                        position.short_text(),
                        position.quantity().normalize(),
                        position.unit(),
                        position.unit_price().map(|price| price.normalize()),
                        position.total_price(),
                        position.kind().label(),
                        position.is_lump_sum(),
                        position.is_hourly(),
                        text(position.long_text()),
                    ),
                    LvRow::Note(note) => format!(
                        "H {} {} | {}",
                        note.is_preliminary(),
                        note.heading(),
                        text(note.text())
                    ),
                    LvRow::Performance(performance) => {
                        format!(
                            "AB {} | {}",
                            performance.title(),
                            text(performance.long_text())
                        )
                    }
                    LvRow::Spacer(_) => "Abstand".to_string(),
                }
            })
            .collect()
    }

    #[test]
    fn export_then_import_gives_the_same_lv() {
        let lv = sample();
        for encoding in [CsvEncoding::Utf8Bom, CsvEncoding::Windows1252] {
            let export = lv_to_csv(&lv, encoding);
            let read = read_csv(
                &export.bytes,
                ProjectName::parse("Rundlauf").unwrap(),
                "2026-11-18",
            )
            .unwrap();
            let back = read.project.lv();
            assert_eq!(outline(back), outline(&lv), "{encoding:?}");
            assert_eq!(back.oz_mask().pattern(), "11.222.33.PPPP");
            assert!(
                back.positions()
                    .all(|position| position.price_check().is_none()),
                "GP stimmt"
            );
            assert!(
                !read
                    .warnings
                    .iter()
                    .any(|w| w.contains("Titel aus den OZ angelegt")),
                "{:?}",
                read.warnings
            );
        }
    }

    #[test]
    fn the_file_looks_like_the_sample() {
        let export = lv_to_csv(&sample(), CsvEncoding::Utf8Bom);
        assert!(export.bytes.starts_with(b"\xEF\xBB\xBF"), "BOM");
        let text = String::from_utf8(export.bytes[3..].to_vec()).unwrap();
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(
            lines[0],
            "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:"
        );
        assert_eq!(lines[1], ";Vorbemerkungen;;;;;;Vorbemerkung");
        assert_eq!(lines[2], ";Allgemeines;;;;;;");
        assert_eq!(lines[3], ";zweiter Absatz;;;;;;");
        assert_eq!(lines[4], ";;;;;;;");
        assert_eq!(lines[5], "01;Gebäudeautomation;;;;;480;Titel");
        assert!(lines.contains(&"01.481.01;\"Raumautomation; Etage 1\";;;;;;Titel"));
        assert!(lines.contains(&"01.481.01;Kabeltrassen;;;;;480;Ausführungsbeschreibung"));
        assert!(lines.contains(
            &"01.481.01.0010;\"Raumregler \"\"Komfort\"\"\";St;2,5;1234,50;3086,25;481;Position"
        ));
        assert!(lines.contains(&";mit Display;;;;;;"));
        assert!(lines.contains(
            &"01.481.01.0030;Inbetriebnahme;psch;1;800,00;800,00;480;Zulageposition, Pauschal"
        ));
        assert!(lines.contains(&"01.481.01.0020;Fühler;St;3;;;480;Bedarfsposition ohne GB"));
        assert!(
            text.ends_with(
                ";;;;;;;\r\n01.481.01.0050;;;;;;480;Text\r\n;Hinweis zum Titel;;;;;;\r\n"
            ),
            "{text}"
        );
        assert!(
            export
                .notes
                .iter()
                .any(|note| note.contains("Leere Absätze")),
            "{:?}",
            export.notes
        );
    }

    /// Textergänzungen go out as printed: at their place, set off by spaces,
    /// Lists in the Langtext: „- “ and numbers, Unterpunkte two blanks
    /// further in per level.
    #[test]
    fn list_levels_go_out_indented() {
        use proava_domain::{Run, TextBlock};
        let mut lv = sample();
        let index = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == "01.481.01.0010")
            .unwrap();
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_long_text(proava_domain::RichText::new(vec![
            TextBlock::text("Inhalte:"),
            TextBlock::list_item(true, vec![Run::plain("Automation")]),
            TextBlock::list_item_at(false, 1, vec![Run::plain("Parameter")]),
            TextBlock::list_item(true, vec![Run::plain("Leitebene")]),
        ]));
        let export = lv_to_csv(&lv, CsvEncoding::Utf8Bom);
        let text = String::from_utf8(export.bytes[3..].to_vec()).unwrap();
        assert!(
            text.contains(
                ";Inhalte:;;;;;;\r\n;1. Automation;;;;;;\r\n;  - Parameter;;;;;;\r\n;2. Leitebene;;;;;;"
            ),
            "{text}"
        );
    }

    /// without the editor's „(vom Bieter)“; those without place after the text.
    #[test]
    fn complements_go_out_as_printed() {
        use proava_domain::{ComplementKind, TextAnchor, TextComplement};
        let mut lv = sample();
        let index = lv
            .rows()
            .iter()
            .position(|row| row.ordinal() == "01.481.01.0010")
            .unwrap();
        let position = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        position.set_long_text(proava_domain::RichText::new(vec![
            proava_domain::TextBlock::paragraph(vec![proava_domain::Run::plain(
                "Gerät mit Display,vom Bieter einzutragen.",
            )]),
        ]));
        position.set_complements(vec![
            TextComplement {
                marker: "01".into(),
                kind: ComplementKind::Bidder,
                caption: "Fabrikat: ".into(),
                anchor: Some(TextAnchor {
                    block: 0,
                    offset: 18,
                    own_line: false,
                }),
                ..TextComplement::default()
            },
            TextComplement {
                marker: "02".into(),
                kind: ComplementKind::Bidder,
                caption: "Typ:".into(),
                ..TextComplement::default()
            },
        ]);
        let export = lv_to_csv(&lv, CsvEncoding::Utf8Bom);
        let text = String::from_utf8(export.bytes[3..].to_vec()).unwrap();
        assert!(
            text.contains(";Gerät mit Display, Fabrikat: ………… vom Bieter einzutragen.;;;;;;\r\n;Typ: …………;;;;;;"),
            "{text}"
        );
        assert!(!text.contains("(vom Bieter)"), "{text}");
    }
}
