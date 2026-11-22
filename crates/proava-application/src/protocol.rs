//! Protocols and result lists – the import protocol, the preview of an
//! import, „Angebote eingelesen“, hints after an export: one fact per line,
//! „Bezeichnung | Wert“, details of a line as a list below it.
//!
//! The use cases build the entries; the screen only draws them and the
//! clipboard and the MCP server take [`Protocol::text`] („Bezeichnung: Wert“
//! per line).

use std::path::Path;

use proava_domain::price_check::amount;
use proava_domain::{Bid, Discount, Project};

/// How a value is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueKind {
    /// Plain text, wraps at word boundaries.
    #[default]
    Text,
    /// Count or amount: right-aligned with the other numbers.
    Number,
    /// File or folder: one line, shortened in the middle when too long.
    Path,
}

/// Colour of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    /// Something to look at (GP-Abweichung, Rechenfehler, Fehler).
    Warning,
}

/// One line of a protocol.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtocolEntry {
    /// „Datei“, „Positionen“, „Speicherort“ …
    pub label: String,
    pub value: String,
    pub kind: ValueKind,
    pub tone: Tone,
    /// Einzelheiten under the line, e.g. what was discarded.
    pub details: Vec<String>,
}

impl ProtocolEntry {
    pub fn text(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            ..Self::default()
        }
    }

    pub fn number(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: ValueKind::Number,
            ..Self::text(label, value)
        }
    }

    /// A count („171“).
    pub fn count(label: impl Into<String>, value: usize) -> Self {
        Self::number(label, value.to_string())
    }

    pub fn path(label: impl Into<String>, path: &Path) -> Self {
        Self {
            kind: ValueKind::Path,
            ..Self::text(label, path.display().to_string())
        }
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }

    pub fn warning(mut self) -> Self {
        self.tone = Tone::Warning;
        self
    }

    /// „Bezeichnung: Wert“
    pub fn line(&self) -> String {
        if self.value.is_empty() {
            self.label.clone()
        } else {
            format!("{}: {}", self.label, self.value)
        }
    }
}

/// A titled list of entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Protocol {
    pub title: String,
    /// One line under the title: what the list is.
    pub hint: String,
    pub entries: Vec<ProtocolEntry>,
}

impl Protocol {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn push(&mut self, entry: ProtocolEntry) {
        self.entries.push(entry);
    }

    /// The first entry with this label.
    pub fn entry(&self, label: &str) -> Option<&ProtocolEntry> {
        self.entries.iter().find(|entry| entry.label == label)
    }

    /// The entries as text: „Bezeichnung: Wert“ per line, details below
    /// with „  – “.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for entry in &self.entries {
            lines.push(entry.line());
            lines.extend(entry.details.iter().map(|detail| format!("  – {detail}")));
        }
        lines
    }

    /// Plain text for the clipboard and the MCP server: the title, then
    /// [`Self::lines`].
    pub fn text(&self) -> String {
        let mut text = format!("{}\n", self.title);
        for line in self.lines() {
            text.push_str(&line);
            text.push('\n');
        }
        text
    }

    /// Messages without own facts (hints of an export, of opening a project):
    /// the file, if any, and the messages as details of one line.
    pub fn messages(title: impl Into<String>, file: Option<&Path>, lines: Vec<String>) -> Self {
        let mut protocol = Self::new(title);
        if let Some(file) = file {
            protocol.push(ProtocolEntry::path("Datei", file));
        }
        protocol.push(ProtocolEntry::count("Hinweise", lines.len()).with_details(lines));
        protocol
    }
}

/// „Angebote eingelesen“: what was read from one bid file (also the MCP
/// answer of angebot_einlesen); `warnings` = the findings of the reader.
pub fn bid_protocol(project: &Project, bid: &Bid, warnings: &[String]) -> Protocol {
    let lv = project.lv();
    let currency = lv.currency();
    let money = |value| amount(value, currency);
    let mut protocol = Protocol::new(format!("Angebot {}", bid.name()));
    let file = bid.source().trim();
    if !file.is_empty() {
        protocol.push(ProtocolEntry::text(
            "Datei",
            Path::new(file)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| file.to_string()),
        ));
    }
    protocol.push(ProtocolEntry::text("Bieter", bid.name()));
    let address = bid.bidder();
    let place = format!("{} {}", address.postal_code.trim(), address.city.trim());
    if !place.trim().is_empty() {
        protocol.push(ProtocolEntry::text("Ort", place.trim()));
    }
    if let Some(date) = bid.bid_date().filter(|date| !date.trim().is_empty()) {
        protocol.push(ProtocolEntry::text("Angebotsdatum", date.trim()));
    }
    let net = bid.net_total(lv);
    protocol.push(ProtocolEntry::number("Angebotssumme netto", money(net)));
    match bid.discount() {
        Discount::None => {}
        Discount::Percent(value) => {
            protocol.push(ProtocolEntry::number(
                "Nachlass",
                format!("{} %", proava_domain::format_decimal_de(value, 2)),
            ));
        }
        Discount::Amount(value) => protocol.push(ProtocolEntry::number("Nachlass", money(value))),
    }
    if !matches!(bid.discount(), Discount::None) {
        let after = net - bid.discount_amount(net);
        protocol.push(ProtocolEntry::number("Netto nach Nachlass", money(after)));
    }
    let missing = bid.missing_positions(lv).len();
    let entry = ProtocolEntry::count("Positionen ohne EP", missing);
    protocol.push(if missing > 0 { entry.warning() } else { entry });
    let errors: Vec<String> = bid
        .arithmetic_errors(lv)
        .into_iter()
        .map(|(position, stated, computed)| {
            format!(
                "{}: GP laut Angebot {}, errechnet {}",
                position.ordinal(),
                money(stated),
                money(computed)
            )
        })
        .collect();
    let entry = ProtocolEntry::count("Rechenfehler", errors.len()).with_details(errors);
    protocol.push(if entry.details.is_empty() {
        entry
    } else {
        entry.warning()
    });
    let unknown = bid.unknown_ordinals();
    if !unknown.is_empty() {
        protocol.push(
            ProtocolEntry::count("Unbekannte OZ", unknown.len())
                .with_details(unknown.to_vec())
                .warning(),
        );
    }
    if !warnings.is_empty() {
        let entry =
            ProtocolEntry::count("Hinweise", warnings.len()).with_details(warnings.to_vec());
        // „ACHTUNG“: the bid belongs to another LV version.
        let serious = warnings.iter().any(|warning| warning.contains("ACHTUNG"));
        protocol.push(if serious { entry.warning() } else { entry });
    }
    protocol
}

/// The Abweichungsprotokoll of a bid as text: summary, where the file sum
/// differs, the deviations (at most `limit`, then „… und n weitere“) and
/// the proposals. Empty when the file matched the LV.
pub fn structure_lines(project: &Project, bid: &Bid, limit: usize) -> Vec<String> {
    let lv = project.lv();
    let structure = bid.structure();
    let absent = bid.absent_positions(lv);
    let mut lines = Vec::new();
    if let Some(summary) = structure.summary(absent.len()) {
        lines.push(summary);
    }
    if let Some(breakdown) = bid.total_breakdown(lv) {
        lines.push(breakdown.text());
    }
    let details: Vec<String> = structure
        .lines(lv, &absent)
        .iter()
        .map(proava_domain::DeviationLine::text)
        .collect();
    let shown = details.len().min(limit);
    lines.extend(details.iter().take(shown).map(|line| format!("- {line}")));
    if details.len() > shown {
        lines.push(format!("- … und {} weitere", details.len() - shown));
    }
    for suggestion in &structure.suggestions {
        let target = lv
            .position_by_id(suggestion.row)
            .map(|position| {
                let text = position
                    .short_text()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {text}", position.ordinal())
            })
            .unwrap_or_default();
        lines.push(format!(
            "Vorschlag ({}, {} %): Angebot {} → LV {target}",
            if suggestion.sure { "sicher" } else { "prüfen" },
            suggestion.similarity,
            suggestion.ordinal
        ));
    }
    lines
}

/// A file that could not be read as a bid.
pub fn bid_failure(file_name: &str, error: &str) -> Protocol {
    let mut protocol = Protocol::new(format!("Angebot {file_name}"));
    protocol.push(ProtocolEntry::text("Datei", file_name));
    protocol.push(ProtocolEntry::text("Fehler", error).warning());
    protocol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_has_one_fact_per_line_and_details_below() {
        let mut protocol = Protocol::new("Importprotokoll a.X82");
        protocol.push(ProtocolEntry::text("Datei", "a.X82"));
        protocol.push(ProtocolEntry::count("Positionen", 171));
        protocol.push(ProtocolEntry::path(
            "Speicherort",
            Path::new("/Projekte/Rathaus 3"),
        ));
        protocol.push(
            ProtocolEntry::count("Verworfen oder umgewandelt", 2)
                .with_details(vec!["erstens".into(), "zweitens".into()]),
        );
        assert_eq!(
            protocol.text(),
            "Importprotokoll a.X82\nDatei: a.X82\nPositionen: 171\nSpeicherort: /Projekte/Rathaus 3\n\
             Verworfen oder umgewandelt: 2\n  – erstens\n  – zweitens\n"
        );
        assert_eq!(
            protocol.entry("Positionen").unwrap().kind,
            ValueKind::Number
        );
        assert_eq!(protocol.entry("Speicherort").unwrap().kind, ValueKind::Path);
        assert!(protocol.entry("Fehlt").is_none());
    }

    #[test]
    fn messages_name_the_file_and_count_the_hints() {
        let protocol = Protocol::messages(
            "Export X83",
            Some(Path::new("/a/LV.X83")),
            vec!["Hinweis eins".into()],
        );
        assert_eq!(
            protocol.lines(),
            ["Datei: /a/LV.X83", "Hinweise: 1", "  – Hinweis eins"]
        );
    }
}
