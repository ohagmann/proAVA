//! CSV-Import in the proAVA CSV format (also what Excel writes):
//!
//! ```text
//! OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:
//! 01.481.01.0010;Kurztext;St.;1;10;10;481;Position
//! ;Langtext;;;;;;
//! ;;;;;;;
//! ```
//!
//! - Trenner `;`, `,` or Tab (recognised), fields in quotes may hold the
//!   Trenner and line breaks; UTF-8 (with or without BOM) or Windows-1252.
//! - The Kopfzeile is optional (colons optional); its columns are matched by
//!   name, in any order; missing columns stay empty. Without one the order
//!   above applies.
//! - A row with OZ, Positionsart or a value (Einheit, Menge, EP, GP) starts
//!   an entry; following rows with only Positionstext are its Langtext (one
//!   paragraph each). Empty rows only separate – with or without them the
//!   result is the same.
//! - Positionsart: Position (also Bedarf, Grund-/Wahl, Zulage, Pauschal,
//!   Stundenlohn), Ausführungsbeschreibung, Text (Hinweis), Vorbemerkung,
//!   Titel. Without one, an OZ with fewer pieces than the positions is a
//!   Titel. Titles that only show in the OZ of their rows are added.
//! - The OZ-Maske follows the OZ of the file (pieces and widths).
//! - GP-Prüfung: a GP that is not Menge × EP (to the cent) marks the
//!   position ([`PriceCheck`]); the computed GP counts.

use proava_domain::{
    cost_group::parse_cost_group, Decimal, Lv, LvGroup, LvRow, Money, Note, OzMask,
    PerformanceDescription, Position, PositionKind, PriceCheck, Project, ProjectName, RichText,
};

/// Whether `file` is read as CSV (by its extension).
pub fn is_csv(file: &std::path::Path) -> bool {
    file.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "csv" | "txt" | "tsv"
            )
        })
}

/// What the import read besides the project.
#[derive(Debug, Clone)]
pub struct CsvRead {
    pub project: Project,
    /// Findings for the user (columns not used, values that did not fit …).
    pub warnings: Vec<String>,
    /// „CSV · Trenner Semikolon · UTF-8“
    pub format: String,
    /// Columns in the order of the file as recognised („OZ“, „Positionstext“
    /// …; „–“ for one that is not used).
    pub columns: Vec<String>,
    /// Currency named in the file (€, EUR, CHF, $ …), as code.
    pub currency: Option<String>,
    /// For the protocol, one fact each: („Trenner“, „Semikolon“),
    /// („Zeichensatz“, „UTF-8“).
    pub facts: Vec<(String, String)>,
}

/// The columns proAVA knows, in the order of the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Column {
    Ordinal,
    Text,
    Unit,
    Quantity,
    UnitPrice,
    Total,
    CostGroup,
    Kind,
    /// A Langtext in a column of its own (Excel lists).
    LongText,
}

impl Column {
    /// Order without a Kopfzeile (and of the export).
    pub const FORMAT: [Column; 8] = [
        Column::Ordinal,
        Column::Text,
        Column::Unit,
        Column::Quantity,
        Column::UnitPrice,
        Column::Total,
        Column::CostGroup,
        Column::Kind,
    ];

    /// Name in the Kopfzeile (without the colon).
    pub fn name(self) -> &'static str {
        match self {
            Column::Ordinal => "OZ",
            Column::Text => "Positionstext",
            Column::Unit => "Einheit",
            Column::Quantity => "Menge",
            Column::UnitPrice => "EP",
            Column::Total => "GP",
            Column::CostGroup => "KG",
            Column::Kind => "Positionsart",
            Column::LongText => "Langtext",
        }
    }

    /// The column a Kopfzeile cell names, if any.
    fn of_header(cell: &str) -> Option<Column> {
        let name = fold(cell.trim().trim_end_matches(':').trim());
        let name = name.trim_end_matches('.').trim();
        // „EP (€)“, „GP netto“ – the first word decides.
        let word = name
            .split(|c: char| c.is_whitespace() || c == '(' || c == '[')
            .next()
            .unwrap_or_default();
        let column = match name {
            "oz" | "ordnungszahl" | "pos" | "pos-nr" | "pos.-nr" | "posnr" | "position nr"
            | "nr" => Column::Ordinal,
            "positionstext" | "text" | "kurztext" | "bezeichnung" | "beschreibung" | "leistung" => {
                Column::Text
            }
            "langtext" => Column::LongText,
            "einheit" | "me" | "eh" | "mengeneinheit" => Column::Unit,
            "menge" | "anzahl" | "lv-menge" => Column::Quantity,
            "positionsart" | "art" | "typ" | "zeilenart" => Column::Kind,
            "kg" | "kostengruppe" | "din 276" => Column::CostGroup,
            _ => match word {
                "ep" | "einheitspreis" => Column::UnitPrice,
                "gp" | "gesamtpreis" | "gesamtbetrag" | "betrag" => Column::Total,
                "menge" => Column::Quantity,
                "kg" | "kostengruppe" => Column::CostGroup,
                _ => return None,
            },
        };
        Some(column)
    }
}

/// Lower case, umlauts as ae/oe/ue, ß as ss.
fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars().flat_map(char::to_lowercase) {
        match character {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

// ---------- Text, Trenner, Felder ----------

/// The text of the file and the name of its encoding.
fn decode(bytes: &[u8]) -> (String, &'static str) {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), "UTF-8"),
        Err(_) => (
            proava_gaeb::datan::decode(bytes, proava_gaeb::datan::Encoding::Windows1252),
            "Windows-1252",
        ),
    }
}

/// The Trenner: the one of `;`, Tab, `,` found most often (outside quotes)
/// in the first line with content; `;` wins a tie.
fn delimiter(text: &str) -> char {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    let mut counts = [(';', 0usize), ('\t', 0), (',', 0)];
    let mut quoted = false;
    for character in line.chars() {
        if character == '"' {
            quoted = !quoted;
        } else if !quoted {
            for (candidate, count) in &mut counts {
                if character == *candidate {
                    *count += 1;
                }
            }
        }
    }
    counts
        .iter()
        .fold((';', 0), |best, &(candidate, count)| {
            if count > best.1 {
                (candidate, count)
            } else {
                best
            }
        })
        .0
}

fn delimiter_name(delimiter: char) -> &'static str {
    match delimiter {
        ';' => "Semikolon",
        '\t' => "Tabulator",
        _ => "Komma",
    }
}

/// Records of the file: fields split at the Trenner, quotes removed
/// (`""` = one quote), line breaks inside quotes kept.
fn records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    characters.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(character);
            }
            continue;
        }
        match character {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
            }
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c if c == delimiter => record.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

// ---------- Zahlen ----------

/// Whether the numbers of the file use the decimal comma: some value in a
/// number column has a comma with one to three digits behind it or
/// thousands dots before it.
fn decimal_comma(values: &[&str]) -> bool {
    values.iter().any(|value| {
        let value = value.trim();
        match (value.rfind(','), value.rfind('.')) {
            // „1.234,56“: the comma behind the dot is the decimal mark.
            (Some(comma), Some(dot)) => comma > dot,
            // „10,5“ – but not „1,234“ (could be thousands).
            (Some(comma), None) => value.matches(',').count() == 1 && value[comma + 1..].len() != 3,
            _ => false,
        }
    })
}

/// A number as written in the file: „1.234,56“, „1234,5“, „1,234.56“,
/// „1'234.50“, „10 €“. `comma`: the file uses the decimal comma (decides
/// „1.234“).
fn parse_number(text: &str, comma: bool) -> Result<Option<Decimal>, String> {
    let mut cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\'' | '€' | '$' | '\u{a0}' | '\u{202f}'))
        .collect();
    for code in ["EUR", "CHF", "USD", "SFR", "eur", "chf", "usd", "sfr"] {
        cleaned = cleaned.replace(code, "");
    }
    let cleaned = cleaned.as_str();
    if cleaned.is_empty() {
        return Ok(None);
    }
    let dots = cleaned.matches('.').count();
    let commas = cleaned.matches(',').count();
    let normal = match (dots, commas) {
        (0, 0) => cleaned.to_string(),
        (_, 0) if dots > 1 => cleaned.replace('.', ""),
        (1, 0) if comma => {
            // „1.234“ in a file with decimal comma: thousands.
            let (_, after) = cleaned.split_once('.').unwrap_or_default();
            if after.len() == 3 {
                cleaned.replace('.', "")
            } else {
                cleaned.to_string()
            }
        }
        (1, 0) => cleaned.to_string(),
        (0, 1) => cleaned.replace(',', "."),
        (0, _) => cleaned.replace(',', ""),
        _ => {
            // Both: the last one is the decimal mark.
            if cleaned.rfind(',') > cleaned.rfind('.') {
                cleaned.replace('.', "").replace(',', ".")
            } else {
                cleaned.replace(',', "")
            }
        }
    };
    normal
        .parse::<Decimal>()
        .map(Some)
        .map_err(|_| format!("„{}“ ist keine Zahl", text.trim()))
}

/// Currency named in an amount or a Kopfzeile cell.
fn currency_in(text: &str) -> Option<&'static str> {
    let upper = text.to_uppercase();
    if text.contains('€') || upper.contains("EUR") {
        Some("EUR")
    } else if upper.contains("CHF") || upper.contains("SFR") {
        Some("CHF")
    } else if text.contains('$') || upper.contains("USD") {
        Some("USD")
    } else {
        None
    }
}

// ---------- Einträge ----------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Title,
    Position {
        kind: PositionKindName,
        lump_sum: bool,
        hourly: bool,
    },
    Performance,
    Note {
        preliminary: bool,
        closing: bool,
    },
}

/// Positionsart as written, before Wahlgruppen are numbered.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PositionKindName {
    Normal,
    Provisional { with_total: bool },
    Base,
    Alternative,
    Supplement,
}

/// Reads a Positionsart; None for an empty or unknown one (unknown: with
/// the text for the warning).
fn kind_of(text: &str) -> Result<Option<Kind>, String> {
    let folded = fold(text.trim());
    if folded.is_empty() {
        return Ok(None);
    }
    match folded.as_str() {
        "titel" | "ueberschrift" | "abschnitt" | "gruppe" | "los" | "untertitel" => {
            return Ok(Some(Kind::Title))
        }
        "ausfuehrungsbeschreibung" | "ab" => return Ok(Some(Kind::Performance)),
        "text" | "hinweis" | "hinweistext" | "bemerkung" => {
            return Ok(Some(Kind::Note {
                preliminary: false,
                closing: false,
            }))
        }
        "vorbemerkung" | "vorbemerkungen" => {
            return Ok(Some(Kind::Note {
                preliminary: true,
                closing: false,
            }))
        }
        "schlussbemerkung" | "schlussbemerkungen" | "schlussblatt" | "nachbemerkung" => {
            return Ok(Some(Kind::Note {
                preliminary: false,
                closing: true,
            }))
        }
        _ => {}
    }
    let mut kind = PositionKindName::Normal;
    let mut lump_sum = false;
    let mut hourly = false;
    let mut known = false;
    for part in folded
        .split([',', '+', '/', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let with_total = part.contains("mit gb") || part.contains("mit gesamtbetrag");
        let recognised = if part.starts_with("bedarf") || part.starts_with("eventual") {
            kind = PositionKindName::Provisional { with_total };
            true
        } else if part.starts_with("grund") {
            kind = PositionKindName::Base;
            true
        } else if part.starts_with("wahl") || part.starts_with("alternativ") {
            kind = PositionKindName::Alternative;
            true
        } else if part.starts_with("zulage") {
            kind = PositionKindName::Supplement;
            true
        } else if part.starts_with("pausch") || part == "psch" {
            lump_sum = true;
            true
        } else if part.starts_with("stundenlohn") || part == "std" {
            hourly = true;
            true
        } else {
            matches!(
                part,
                "position" | "normalposition" | "normal" | "pos" | "leistung"
            )
        };
        if !recognised {
            return Err(text.trim().to_string());
        }
        known = true;
    }
    Ok(known.then_some(Kind::Position {
        kind,
        lump_sum,
        hourly,
    }))
}

/// One entry of the file: its first row and the Langtext lines after it.
#[derive(Debug, Clone, Default)]
struct Entry {
    /// Row number in the file (1-based, for messages).
    line: usize,
    ordinal: String,
    text: String,
    unit: String,
    quantity: String,
    unit_price: String,
    total: String,
    cost_group: String,
    kind: String,
    long: Vec<String>,
}

impl Entry {
    fn label(&self) -> String {
        if self.ordinal.is_empty() {
            format!("Zeile {}", self.line)
        } else {
            self.ordinal.clone()
        }
    }
}

/// Pieces of an OZ („01.481.01.0010“ → 4).
fn pieces(ordinal: &str) -> Vec<&str> {
    ordinal
        .split('.')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .collect()
}

/// Reads a CSV file into a project with one LV named after the file.
pub fn read_csv(bytes: &[u8], name: ProjectName, created_iso: &str) -> Result<CsvRead, String> {
    let (text, encoding) = decode(bytes);
    let delimiter = delimiter(&text);
    let mut rows = records(&text, delimiter);
    let mut warnings = Vec::new();
    let mut currency = None;

    // Kopfzeile
    let header: Vec<Option<Column>> = rows
        .first()
        .map(|first| first.iter().map(|cell| Column::of_header(cell)).collect())
        .unwrap_or_default();
    let named = header.iter().filter(|column| column.is_some()).count();
    let (columns, header_line): (Vec<Option<Column>>, bool) = if named >= 2 {
        if let Some(first) = rows.first() {
            for (cell, column) in first.iter().zip(&header) {
                if column.is_none() && !cell.trim().is_empty() {
                    warnings.push(format!(
                        "Spalte „{}“ nicht zugeordnet – nicht übernommen.",
                        cell.trim().trim_end_matches(':')
                    ));
                }
                if matches!(column, Some(Column::UnitPrice | Column::Total)) {
                    currency = currency.or(currency_in(cell));
                }
            }
        }
        (header, true)
    } else {
        (Column::FORMAT.iter().copied().map(Some).collect(), false)
    };
    let mut seen = std::collections::HashSet::new();
    let columns: Vec<Option<Column>> = columns
        .into_iter()
        .map(|column| column.filter(|column| seen.insert(*column)))
        .collect();
    if header_line {
        rows.remove(0);
    }
    let missing: Vec<&str> = [Column::Ordinal, Column::Text]
        .into_iter()
        .filter(|wanted| !columns.contains(&Some(*wanted)))
        .map(Column::name)
        .collect();
    if missing.contains(&"Positionstext") {
        return Err(format!(
            "In der CSV-Datei fehlt die Spalte „Positionstext“ (erkannt: {}).",
            column_names(&columns).join(", ")
        ));
    }
    let cell = |row: &[String], wanted: Column| -> String {
        columns
            .iter()
            .position(|column| *column == Some(wanted))
            .and_then(|at| row.get(at))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    };

    // Einträge
    let mut entries: Vec<Entry> = Vec::new();
    let mut loose = 0;
    for (number, row) in rows.iter().enumerate() {
        let line = number + 1 + usize::from(header_line);
        if row.iter().all(|value| value.trim().is_empty()) {
            continue;
        }
        let entry = Entry {
            line,
            ordinal: cell(row, Column::Ordinal),
            text: cell(row, Column::Text),
            unit: cell(row, Column::Unit),
            quantity: cell(row, Column::Quantity),
            unit_price: cell(row, Column::UnitPrice),
            total: cell(row, Column::Total),
            cost_group: cell(row, Column::CostGroup),
            kind: cell(row, Column::Kind),
            long: Vec::new(),
        };
        let long = cell(row, Column::LongText);
        for value in [&entry.unit_price, &entry.total] {
            currency = currency.or(currency_in(value));
        }
        let starts = !entry.ordinal.is_empty()
            || !entry.kind.is_empty()
            || !entry.unit.is_empty()
            || !entry.quantity.is_empty()
            || !entry.unit_price.is_empty()
            || !entry.total.is_empty();
        if starts {
            let mut entry = entry;
            entry.long.extend(long.lines().map(str::to_string));
            entries.push(entry);
        } else if let Some(last) = entries.last_mut() {
            // Langtext: one paragraph per row (a cell may hold several lines).
            last.long.extend(entry.text.lines().map(str::to_string));
            last.long.extend(long.lines().map(str::to_string));
            if !entry.cost_group.is_empty() && last.cost_group.is_empty() {
                last.cost_group = entry.cost_group;
            }
        } else {
            // Text before the first entry: a Hinweis.
            loose += 1;
            entries.push(Entry {
                line,
                text: String::new(),
                kind: "Text".to_string(),
                long: entry
                    .text
                    .lines()
                    .chain(long.lines())
                    .map(str::to_string)
                    .collect(),
                ..Entry::default()
            });
        }
    }
    if loose > 0 {
        warnings.push(format!(
            "Text vor dem ersten Eintrag ohne OZ als Hinweis übernommen ({loose} Zeilen)."
        ));
    }
    if entries.is_empty() {
        return Err("Die CSV-Datei enthält keine Einträge.".to_string());
    }

    // Arten
    let mut kinds: Vec<Option<Kind>> = Vec::with_capacity(entries.len());
    for entry in &entries {
        match kind_of(&entry.kind) {
            Ok(kind) => kinds.push(kind),
            Err(unknown) => {
                warnings.push(format!(
                    "{}: Positionsart „{unknown}“ unbekannt – als Position übernommen.",
                    entry.label()
                ));
                kinds.push(Some(Kind::Position {
                    kind: PositionKindName::Normal,
                    lump_sum: false,
                    hourly: false,
                }));
            }
        }
    }
    // Pieces of a position's OZ: from the positions named as such, else the
    // longest OZ.
    let named_positions = entries
        .iter()
        .zip(&kinds)
        .filter(|(_, kind)| matches!(kind, Some(Kind::Position { .. })))
        .map(|(entry, _)| pieces(&entry.ordinal).len())
        .max();
    let longest = entries
        .iter()
        .map(|entry| pieces(&entry.ordinal).len())
        .max()
        .unwrap_or(0);
    let position_pieces = named_positions
        .filter(|count| *count > 0)
        .unwrap_or(longest)
        .max(1);
    for (entry, kind) in entries.iter().zip(kinds.iter_mut()) {
        if kind.is_none() {
            let count = pieces(&entry.ordinal).len();
            *kind = Some(if count > 0 && count < position_pieces {
                Kind::Title
            } else {
                Kind::Position {
                    kind: PositionKindName::Normal,
                    lump_sum: false,
                    hourly: false,
                }
            });
        }
    }

    // OZ-Maske
    let levels = position_pieces - 1;
    let mut widths = vec![0usize; position_pieces];
    let mut letters = vec![false; levels];
    for entry in &entries {
        for (at, piece) in pieces(&entry.ordinal)
            .into_iter()
            .enumerate()
            .take(position_pieces)
        {
            widths[at] = widths[at].max(piece.chars().count());
            if at < levels && !piece.chars().all(|c| c.is_ascii_digit()) {
                letters[at] = true;
            }
        }
    }
    let mut pattern: Vec<String> = (0..levels)
        .map(|level| {
            let mark = if letters[level] {
                'L'
            } else {
                char::from_digit(((level + 1) % 10) as u32, 10).unwrap_or('1')
            };
            mark.to_string().repeat(widths[level].clamp(1, 9))
        })
        .collect();
    // Without any OZ the positions get four digits when they are numbered.
    let position_width = if widths[levels] == 0 {
        4
    } else {
        widths[levels].min(9)
    };
    pattern.push("P".repeat(position_width));
    let mask = OzMask::parse(&pattern.join(".")).map_err(|error| format!("OZ-Maske: {error}"))?;
    // Tabellenprogramme drop leading zeros („01“ → „1“): pad numeric pieces
    // to the width of their level.
    let mut padded = Vec::new();
    for entry in &mut entries {
        let all = pieces(&entry.ordinal);
        let fixed: Vec<String> = all
            .iter()
            .enumerate()
            .map(|(at, piece)| {
                let width = widths.get(at).copied().unwrap_or(0);
                if piece.chars().all(|c| c.is_ascii_digit()) && piece.chars().count() < width {
                    format!("{piece:0>width$}")
                } else {
                    piece.to_string()
                }
            })
            .collect();
        let fixed = fixed.join(".");
        if !entry.ordinal.is_empty() && fixed != entry.ordinal {
            padded.push(format!("{} → {fixed}", entry.ordinal));
            entry.ordinal = fixed;
        }
    }
    if !padded.is_empty() {
        warnings.push(format!(
            "OZ ohne führende Nullen ergänzt ({}{}).",
            padded
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if padded.len() > 5 {
                format!(" und {} weitere", padded.len() - 5)
            } else {
                String::new()
            }
        ));
    }

    // LV
    let comma = decimal_comma(
        &entries
            .iter()
            .flat_map(|entry| {
                [
                    entry.quantity.as_str(),
                    entry.unit_price.as_str(),
                    entry.total.as_str(),
                ]
            })
            .collect::<Vec<_>>(),
    );
    let mut lv = Lv::new(name.as_str(), mask);
    // The currency named in the file; Euro otherwise (the dialog may change it).
    if let Some(code) = currency {
        lv.set_currency(code);
    }
    // Open titles per level (their OZ).
    let mut open: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    let mut group_number = 0u32;
    let mut serial = 0u32;
    for (entry, kind) in entries.iter().zip(&kinds) {
        let kind = kind.expect("every entry has a kind");
        let all = pieces(&entry.ordinal);
        // Titles this row lies in: for a title its parents, for a position
        // all but the last piece, for AB/Text with a title's OZ that title.
        let inside = match kind {
            Kind::Title => all.len().saturating_sub(1),
            Kind::Position { .. } => all.len().saturating_sub(1).min(levels),
            Kind::Performance | Kind::Note { .. } => {
                if all.len() >= position_pieces {
                    levels
                } else {
                    all.len()
                }
            }
        }
        .min(levels);
        if !entry.ordinal.is_empty() {
            for level in 0..inside {
                let prefix = all[..=level].join(".");
                if open.get(level) == Some(&prefix) {
                    continue;
                }
                open.truncate(level);
                lv.push_group(LvGroup::new(
                    level,
                    prefix.clone(),
                    format!("Titel {prefix}"),
                ));
                added.push(prefix.clone());
                open.push(prefix);
            }
        }
        let long = RichText::plain(&entry.long.join("\n"));
        let cost_group = match parse_cost_group(&entry.cost_group) {
            Ok(code) => code,
            Err(_) => {
                warnings.push(format!(
                    "{}: Kostengruppe „{}“ ist keine dreistellige KG nach DIN 276 – nicht übernommen.",
                    entry.label(),
                    entry.cost_group
                ));
                None
            }
        };
        match kind {
            Kind::Title => {
                let level = all.len().saturating_sub(1).min(levels.saturating_sub(1));
                if levels == 0 {
                    warnings.push(format!(
                        "{}: Titel „{}“ – die OZ der Positionen haben keine Titelstufe; als Hinweis übernommen.",
                        entry.label(),
                        entry.text
                    ));
                    let mut note = Note::rich(long);
                    note.set_heading(entry.text.clone());
                    lv.push_note(note);
                    continue;
                }
                if all.len() > levels {
                    warnings.push(format!(
                        "{}: Titel mit so vielen Stufen wie eine Position – auf Stufe {} gesetzt.",
                        entry.label(),
                        level + 1
                    ));
                }
                open.truncate(level);
                let mut title = LvGroup::new(level, entry.ordinal.clone(), entry.text.clone());
                title.set_cost_group(cost_group);
                lv.push_group(title);
                open.push(entry.ordinal.clone());
                if !entry.long.is_empty() {
                    lv.push_note(Note::rich(long));
                    warnings.push(format!(
                        "{}: Langtext eines Titels als Hinweis darunter übernommen.",
                        entry.label()
                    ));
                }
            }
            Kind::Performance => {
                lv.push_row(LvRow::Performance(PerformanceDescription::new(
                    "",
                    entry.text.clone(),
                    long,
                )));
            }
            Kind::Note {
                preliminary,
                closing,
            } => {
                let mut note = Note::rich(long);
                note.set_heading(entry.text.clone());
                note.set_preliminary(preliminary);
                note.set_closing(closing);
                lv.push_note(note);
            }
            Kind::Position {
                kind,
                lump_sum,
                hourly,
            } => {
                let number =
                    |text: &str, label: &str, warnings: &mut Vec<String>| match parse_number(
                        text, comma,
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            warnings.push(format!(
                                "{}: {label} {error} – leer gelassen.",
                                entry.label()
                            ));
                            None
                        }
                    };
                let quantity = number(&entry.quantity, "Menge", &mut warnings);
                let unit_price = number(&entry.unit_price, "EP", &mut warnings);
                let total = number(&entry.total, "GP", &mut warnings);
                let kind = match kind {
                    PositionKindName::Normal => PositionKind::Normal,
                    PositionKindName::Provisional { with_total } => {
                        PositionKind::Provisional { with_total }
                    }
                    PositionKindName::Supplement => PositionKind::Supplement,
                    PositionKindName::Base => {
                        group_number += 1;
                        serial = 0;
                        PositionKind::Base {
                            group: group_number,
                        }
                    }
                    PositionKindName::Alternative => {
                        if group_number == 0 {
                            group_number = 1;
                        }
                        serial += 1;
                        PositionKind::Alternative {
                            group: group_number,
                            serial,
                        }
                    }
                };
                let mut position = Position::new(
                    entry.ordinal.clone(),
                    entry.text.clone(),
                    quantity.unwrap_or(Decimal::ZERO),
                    entry.unit.clone(),
                    unit_price,
                );
                position.set_long_text(long);
                position.set_kind(kind);
                position.set_lump_sum(lump_sum);
                position.set_hourly(hourly);
                position.set_cost_group(cost_group);
                if let Some(stated) = total {
                    let stated = Money::from_decimal(stated);
                    match unit_price {
                        Some(_) => {
                            let computed = position.total_price();
                            if stated != computed {
                                position.set_price_check(Some(PriceCheck { stated, computed }));
                            }
                        }
                        None if !stated.is_zero() => warnings.push(format!(
                            "{}: GP {} ohne EP – der EP fehlt, GP nicht übernommen.",
                            entry.label(),
                            stated.plain()
                        )),
                        None => {}
                    }
                }
                if entry.ordinal.is_empty() {
                    warnings.push(format!(
                        "{}: Position „{}“ ohne OZ – bekommt beim Neu-Nummerieren eine.",
                        entry.label(),
                        entry.text
                    ));
                }
                lv.push_position(position);
            }
        }
    }
    if !added.is_empty() {
        warnings.push(format!(
            "{} Titel aus den OZ angelegt ({}) – Bezeichnung bitte ergänzen.",
            added.len(),
            added.join(", ")
        ));
    }
    let duplicates = lv.duplicate_ordinals();
    if !duplicates.is_empty() {
        warnings.push(format!(
            "OZ mehrfach vergeben: {}.",
            duplicates
                .into_iter()
                .take(10)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if missing.contains(&"OZ") {
        warnings.push(
            "Keine Spalte „OZ“ – die Positionen bekommen beim Neu-Nummerieren ihre OZ.".to_string(),
        );
    }
    let project = Project::new(name, created_iso, lv);
    Ok(CsvRead {
        project,
        warnings,
        format: format!("CSV · Trenner {} · {encoding}", delimiter_name(delimiter)),
        columns: column_names(&columns),
        currency: currency.map(str::to_string),
        facts: vec![
            ("Trenner".to_string(), delimiter_name(delimiter).to_string()),
            ("Zeichensatz".to_string(), encoding.to_string()),
        ],
    })
}

fn column_names(columns: &[Option<Column>]) -> Vec<String> {
    columns
        .iter()
        .map(|column| column.map_or("–".to_string(), |column| column.name().to_string()))
        .collect()
}

/// Positionen with a GP-Prüfung, for protocols: „OZ: GP laut Datei …“.
pub fn price_check_lines(lv: &Lv) -> Vec<String> {
    lv.positions()
        .filter_map(|position| {
            position
                .price_check()
                .map(|check| format!("{}: {}", position.ordinal(), check.text(lv.currency())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::EditStatus;

    const SAMPLE: &str = "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:;KG:;Positionsart:\n\
        01.481.01.0010;Kurztext;St.;1;10;10;481;Position\n\
        ;Langtext;;;;;;\n\
        ;;;;;;;\n\
        01.481.01.0010;Ausführungsbeschreibung;;;;;481;Ausführungsbeschreibung\n\
        ;Langtext;;;;;;\n\
        ;;;;;;;\n\
        01.481.01.0010;Vorbemerkung;;;;;481;Text\n\
        ;Langtext;;;;;;\n";

    fn read(text: &str) -> CsvRead {
        read_csv(
            text.as_bytes(),
            ProjectName::parse("Probe").unwrap(),
            "2026-11-18",
        )
        .unwrap()
    }

    /// Rows as „kind OZ text“ lines for comparisons.
    fn outline(lv: &Lv) -> Vec<String> {
        lv.rows()
            .iter()
            .map(|row| match row {
                LvRow::Group(group) => format!(
                    "T{} {} {}",
                    group.level(),
                    group.ordinal(),
                    group.short_text()
                ),
                LvRow::Position(position) => format!(
                    "P {} {} | {} {} | {:?} | {} | {:?}",
                    position.ordinal(),
                    position.short_text(),
                    position.quantity(),
                    position.unit(),
                    position.unit_price(),
                    position.long_text().to_plain().replace('\n', "/"),
                    position.cost_group()
                ),
                LvRow::Note(note) => format!(
                    "H {} | {}",
                    note.heading(),
                    note.text().to_plain().replace('\n', "/")
                ),
                LvRow::Performance(performance) => format!(
                    "AB {} | {}",
                    performance.title(),
                    performance.long_text().to_plain().replace('\n', "/")
                ),
                LvRow::Spacer(_) => "Abstand".to_string(),
            })
            .collect()
    }

    #[test]
    fn the_sample_gives_titles_position_ab_and_hint() {
        let read = read(SAMPLE);
        let lv = read.project.lv();
        assert_eq!(lv.oz_mask().pattern(), "11.222.33.PPPP");
        assert_eq!(
            outline(lv),
            [
                "T0 01 Titel 01",
                "T1 01.481 Titel 01.481",
                "T2 01.481.01 Titel 01.481.01",
                "P 01.481.01.0010 Kurztext | 1 St. | Some(10) | Langtext | Some(\"481\")",
                "AB Ausführungsbeschreibung | Langtext",
                "H Vorbemerkung | Langtext",
            ]
        );
        assert_eq!(read.format, "CSV · Trenner Semikolon · UTF-8");
        assert_eq!(
            read.columns,
            [
                "OZ",
                "Positionstext",
                "Einheit",
                "Menge",
                "EP",
                "GP",
                "KG",
                "Positionsart"
            ]
        );
        assert!(read
            .warnings
            .iter()
            .any(|w| w.contains("3 Titel aus den OZ angelegt (01, 01.481, 01.481.01)")));
        assert!(lv
            .positions()
            .all(|position| position.status() == EditStatus::Open));
        assert!(
            lv.positions()
                .all(|position| position.price_check().is_none()),
            "GP stimmt"
        );
    }

    #[test]
    fn empty_rows_are_optional() {
        let without: String = SAMPLE
            .lines()
            .filter(|line| !line.chars().all(|c| c == ';'))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            outline(read(SAMPLE).project.lv()),
            outline(read(&without).project.lv())
        );
        // Also CRLF and several empty rows.
        let crlf = SAMPLE
            .replace('\n', "\r\n")
            .replace(";;;;;;;\r\n", ";;;;;;;\r\n\r\n;;;;;;;\r\n");
        assert_eq!(
            outline(read(SAMPLE).project.lv()),
            outline(read(&crlf).project.lv())
        );
    }

    #[test]
    fn columns_are_found_by_name_in_any_order() {
        let text = "Positionsart;Menge;OZ;Bemerkung;Einheit;Positionstext;EP\n\
                    Titel;;01;;;Automation;\n\
                    ;2,5;01.0010;intern;m;Kabel;12,40\n\
                    ;;;;;erste Zeile;\n\
                    ;;;;;zweite Zeile;\n";
        let read = read(text);
        assert_eq!(
            read.columns,
            [
                "Positionsart",
                "Menge",
                "OZ",
                "–",
                "Einheit",
                "Positionstext",
                "EP"
            ]
        );
        assert!(read
            .warnings
            .iter()
            .any(|w| w.contains("Spalte „Bemerkung“ nicht zugeordnet")));
        assert_eq!(
            outline(read.project.lv()),
            [
                "T0 01 Automation",
                "P 01.0010 Kabel | 2.5 m | Some(12.40) | erste Zeile/zweite Zeile | None"
            ]
        );
        assert_eq!(read.project.lv().oz_mask().pattern(), "11.PPPP");
    }

    #[test]
    fn without_header_the_format_order_applies_and_commas_or_tabs_split() {
        let semicolon = "01;Titel A;;;;;;Titel\n01.0010;Pos;St;3;1.234,50;3.703,50;;\n";
        let read = read(semicolon);
        let position = read.project.lv().positions().next().unwrap().clone();
        assert_eq!(position.unit_price(), Some(Decimal::new(123450, 2)));
        assert!(position.price_check().is_none());
        let tab = semicolon.replace(';', "\t");
        assert_eq!(
            outline(read.project.lv()),
            outline(self::read(&tab).project.lv())
        );
        assert!(self::read(&tab).format.contains("Tabulator"));
        let comma = "OZ,Positionstext,Einheit,Menge,EP\n01.0010,\"Kabel, NYM\",m,\"2,5\",10.5\n";
        let read = self::read(comma);
        let position = read.project.lv().positions().next().unwrap();
        assert_eq!(position.short_text(), "Kabel, NYM");
        assert_eq!(position.quantity(), Decimal::new(25, 1));
        assert_eq!(position.unit_price(), Some(Decimal::new(105, 1)));
    }

    #[test]
    fn windows_1252_and_quoted_line_breaks() {
        let text = "OZ;Positionstext;Einheit;Menge\n01.0010;\"Fühler für Außen\nmit Schutz\";St;1\n;Größe 3;;\n";
        let bytes = proava_gaeb::datan::encode(text, proava_gaeb::datan::Encoding::Windows1252);
        let read = read_csv(&bytes, ProjectName::parse("P").unwrap(), "").unwrap();
        assert!(read.format.ends_with("Windows-1252"), "{}", read.format);
        let position = read.project.lv().positions().next().unwrap();
        assert_eq!(position.short_text(), "Fühler für Außen\nmit Schutz");
        assert_eq!(position.long_text().to_plain(), "Größe 3");
        // UTF-8 with BOM.
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(text.as_bytes());
        let read = read_csv(&bom, ProjectName::parse("P").unwrap(), "").unwrap();
        assert_eq!(read.columns[0], "OZ");
    }

    #[test]
    fn a_wrong_gp_marks_the_position_and_the_computed_one_counts() {
        let text = "OZ:;Positionstext:;Einheit:;Menge:;EP:;GP:\n\
                    01.0010;Gerät;St;1;10;12;\n\
                    01.0020;Kabel;m;3;0,333;1,00\n\
                    01.0030;Psch;psch;1;500;500\n";
        let read = read(text);
        let lv = read.project.lv();
        let positions: Vec<&Position> = lv.positions().collect();
        let check = positions[0].price_check().copied().unwrap();
        assert_eq!(
            check.text("EUR"),
            "GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €"
        );
        assert_eq!(positions[0].total_price(), Money::from_cents(1000));
        assert!(
            positions[1].price_check().is_none(),
            "3 × 0,333 = 0,999 → 1,00 auf Cent"
        );
        assert!(positions[2].price_check().is_none());
        assert_eq!(
            price_check_lines(lv),
            ["01.0010: GP laut Datei 12,00 €, errechnet 10,00 € – Abweichung 2,00 €"]
        );
        assert_eq!(
            proava_domain::price_check::open_checks(lv),
            (1, Money::from_cents(200))
        );
        // Changing the EP answers the mark.
        let mut position = positions[0].clone();
        position.set_unit_price(Some(Decimal::from(12)));
        assert!(position.price_check().is_none());
    }

    #[test]
    fn kinds_titles_without_kind_and_values() {
        let text = "OZ;Positionstext;Einheit;Menge;EP;GP;KG;Positionsart\n\
                    1;Rohbau;;;;;;\n\
                    1.1;Erdarbeiten;;;;;;\n\
                    1.1.10;Aushub;m3;100;5;;;Bedarfsposition mit GB\n\
                    1.1.20;Grund;St;1;;;;Grundposition\n\
                    1.1.30;Wahl;St;1;;;;Wahlposition\n\
                    1.1.40;Zulage;St;1;;;;Zulage, Pauschal\n\
                    1.1.50;Regie;h;8;;;;Stundenlohn\n\
                    1.1.60;Vorab;;;;;;Vorbemerkung\n\
                    1.1.70;Merkwürdig;St;1;;;99;Sonderling\n";
        let read = read(text);
        let lv = read.project.lv();
        assert_eq!(lv.oz_mask().pattern(), "1.2.PP");
        assert_eq!(
            lv.group_count(),
            2,
            "Titel ohne Positionsart über die Stufen"
        );
        let kinds: Vec<(PositionKind, bool, bool)> = lv
            .positions()
            .map(|position| {
                (
                    position.kind(),
                    position.is_lump_sum(),
                    position.is_hourly(),
                )
            })
            .collect();
        assert_eq!(
            kinds,
            [
                (PositionKind::Provisional { with_total: true }, false, false),
                (PositionKind::Base { group: 1 }, false, false),
                (
                    PositionKind::Alternative {
                        group: 1,
                        serial: 1
                    },
                    false,
                    false
                ),
                (PositionKind::Supplement, true, false),
                (PositionKind::Normal, false, true),
                (PositionKind::Normal, false, false),
            ]
        );
        assert!(lv
            .rows()
            .iter()
            .any(|row| matches!(row, LvRow::Note(note) if note.is_preliminary())));
        assert!(read
            .warnings
            .iter()
            .any(|w| w.contains("Positionsart „Sonderling“ unbekannt")));
        assert!(read
            .warnings
            .iter()
            .any(|w| w.contains("Kostengruppe „99“")));
    }

    #[test]
    fn numbers_in_german_english_and_swiss_style() {
        for (text, comma, expected) in [
            ("1.234,56", true, "1234.56"),
            ("1234,5", true, "1234.5"),
            ("1.234", true, "1234"),
            ("1.234", false, "1.234"),
            ("1,234.50", false, "1234.50"),
            ("1'234.50", false, "1234.50"),
            ("10 €", true, "10"),
            ("CHF 12.50", false, "12.50"),
            ("-3,5", true, "-3.5"),
        ] {
            assert_eq!(
                parse_number(text, comma).unwrap().unwrap().to_string(),
                expected,
                "{text}"
            );
        }
        assert!(parse_number("zehn", true).is_err());
        assert_eq!(parse_number(" ", true).unwrap(), None);
        assert!(decimal_comma(&["1", "10,5"]));
        assert!(!decimal_comma(&["1", "10.5", "1,234.00"]));
        assert_eq!(currency_in("EP (CHF)"), Some("CHF"));
    }

    #[test]
    fn leading_zeros_dropped_by_a_spreadsheet_are_restored() {
        // LibreOffice/Excel made „01“ a number: 1.
        let text = "OZ;Positionstext;Einheit;Menge;EP;GP;KG;Positionsart\n\
                    1;Automation;;;;;;Titel\n\
                    01.0010;Regler;St;1;10;10;;Position\n\
                    1.20;Fühler;St;1;5;5;;Position\n";
        let read = read(text);
        assert_eq!(
            outline(read.project.lv()),
            [
                "T0 01 Automation",
                "P 01.0010 Regler | 1 St | Some(10) |  | None",
                "P 01.0020 Fühler | 1 St | Some(5) |  | None",
            ]
        );
        assert!(
            read.warnings
                .iter()
                .any(|w| w.contains("OZ ohne führende Nullen ergänzt (1 → 01, 1.20 → 01.0020)")),
            "{:?}",
            read.warnings
        );
    }

    #[test]
    fn a_file_without_text_column_is_refused() {
        let error =
            read_csv(b"OZ;Menge\n01;1\n", ProjectName::parse("P").unwrap(), "").unwrap_err();
        assert!(error.contains("Positionstext"), "{error}");
    }
}
