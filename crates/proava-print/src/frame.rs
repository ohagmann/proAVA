//! Kopf- und Fußzeile jeder Seite: three fields (links, Mitte, rechts) of text
//! and placeholders such as `{projekt}` or `Seite {seite} von {seiten}`.
//! The placeholders known when the data is built are replaced here; the page
//! number, the page count and the current title are left as segments for the
//! template (they are only known while the pages are laid out).

use serde_json::{json, Value};

/// Where the logo sits in the page header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameLogo {
    /// No logo in the page header (the title block on page 1 still shows it).
    #[default]
    Off,
    Left,
    Right,
}

impl FrameLogo {
    fn key(self) -> &'static str {
        match self {
            FrameLogo::Off => "off",
            FrameLogo::Left => "left",
            FrameLogo::Right => "right",
        }
    }
}

/// Page header and footer of a printout.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintFrame {
    /// Left, centre, right; a line break starts the second line.
    pub header: [String; 3],
    pub footer: [String; 3],
    /// Thin rule under the header / over the footer.
    pub header_rule: bool,
    pub footer_rule: bool,
    pub logo: FrameLogo,
    /// Font size of header and footer in pt.
    pub font_size_pt: f32,
    /// Values of the placeholders (name without braces → text), e.g.
    /// („vergabenummer“, „V-2026-0815“). Missing names fall back to the
    /// document header (projekt, dokumentart, datum, lv, bearbeiter) or stay empty.
    pub values: Vec<(String, String)>,
}

/// Standard frame: Projekt | Dokumentart | Datum over the page, LV |
/// Bearbeiter | „Seite x von y“ under it; the free text of the print dialog
/// as second line in the middle.
pub const DEFAULT_HEADER: [&str; 3] = ["{projekt}", "{dokumentart}\n{kopftext}", "{datum}"];
pub const DEFAULT_FOOTER: [&str; 3] = [
    "{lv}",
    "{bearbeiter}\n{fusstext}",
    "Seite {seite} von {seiten}",
];

impl Default for PrintFrame {
    fn default() -> Self {
        Self {
            header: DEFAULT_HEADER.map(String::from),
            footer: DEFAULT_FOOTER.map(String::from),
            header_rule: true,
            footer_rule: true,
            logo: FrameLogo::Off,
            font_size_pt: 8.0,
            values: Vec::new(),
        }
    }
}

impl PrintFrame {
    /// Header or footer show the current title (`{titel}`).
    pub fn uses_title(&self) -> bool {
        let placeholder = format!("{{{TITLE}}}");
        self.header
            .iter()
            .chain(&self.footer)
            .any(|field| field.contains(&placeholder))
    }

    /// Font size limited to what fits into the page margins.
    pub fn font_size(&self) -> f32 {
        if self.font_size_pt.is_finite() {
            self.font_size_pt.clamp(6.0, 11.0)
        } else {
            8.0
        }
    }
}

/// Placeholders that only the layout knows.
const PAGE: &str = "seite";
const PAGES: &str = "seiten";
const TITLE: &str = "titel";

/// One piece of a resolved line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Segment {
    Text(String),
    Page,
    Pages,
    Title,
}

/// Replaces the placeholders of one field. Returns at most two lines; lines
/// that only consisted of empty placeholders are dropped. Unknown placeholders
/// stay as written, so a typo is visible on the page.
pub(crate) fn resolve(field: &str, value: &dyn Fn(&str) -> Option<String>) -> Vec<Vec<Segment>> {
    let mut lines = Vec::new();
    for line in field.lines() {
        let mut segments: Vec<Segment> = Vec::new();
        let mut had_placeholder = false;
        let mut filled = false;
        let mut rest = line;
        let push_text = |segments: &mut Vec<Segment>, text: &str| {
            if text.is_empty() {
                return;
            }
            match segments.last_mut() {
                Some(Segment::Text(last)) => last.push_str(text),
                _ => segments.push(Segment::Text(text.to_string())),
            }
        };
        while let Some(start) = rest.find('{') {
            let Some(length) = rest[start..].find('}') else {
                break;
            };
            let name = &rest[start + 1..start + length];
            let known =
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !name.is_empty();
            push_text(&mut segments, &rest[..start]);
            if !known {
                push_text(&mut segments, &rest[start..=start + length]);
            } else {
                match name {
                    PAGE => {
                        segments.push(Segment::Page);
                        had_placeholder = true;
                        filled = true;
                    }
                    PAGES => {
                        segments.push(Segment::Pages);
                        had_placeholder = true;
                        filled = true;
                    }
                    TITLE => {
                        segments.push(Segment::Title);
                        had_placeholder = true;
                        filled = true;
                    }
                    _ => match value(name) {
                        Some(text) if !text.trim().is_empty() => {
                            // One line per field line: line breaks of a value become spaces.
                            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                            push_text(&mut segments, &text);
                            had_placeholder = true;
                            filled = true;
                        }
                        Some(_) => had_placeholder = true,
                        None => push_text(&mut segments, &rest[start..=start + length]),
                    },
                }
            }
            rest = &rest[start + length + 1..];
        }
        push_text(&mut segments, rest);
        let only_literal = segments
            .iter()
            .all(|segment| matches!(segment, Segment::Text(_)));
        let blank = segments.iter().all(|segment| match segment {
            Segment::Text(text) => text.trim().is_empty(),
            _ => false,
        });
        // „Stand: {datum}“ without a date disappears, plain text stays.
        if blank || (had_placeholder && !filled && only_literal) {
            continue;
        }
        if let Some(Segment::Text(text)) = segments.first_mut() {
            *text = text.trim_start().to_string();
        }
        if let Some(Segment::Text(text)) = segments.last_mut() {
            *text = text.trim_end().to_string();
        }
        lines.push(segments);
        if lines.len() == 2 {
            break;
        }
    }
    lines
}

fn segments_json(lines: &[Vec<Segment>]) -> Value {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|segment| match segment {
                    Segment::Text(text) => json!({ "t": "s", "s": text }),
                    Segment::Page => json!({ "t": "page" }),
                    Segment::Pages => json!({ "t": "pages" }),
                    Segment::Title => json!({ "t": "title" }),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The frame for the template; `defaults` are the values of the document
/// header (used where the frame has none).
pub(crate) fn frame_json(frame: &PrintFrame, defaults: &[(&str, String)]) -> Value {
    let value = |name: &str| -> Option<String> {
        frame
            .values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, text)| text.clone())
            .or_else(|| {
                defaults
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, text)| text.clone())
            })
            .or_else(|| {
                crate::frame::PLACEHOLDER_NAMES
                    .contains(&name)
                    .then(String::new)
            })
    };
    let fields = |fields: &[String; 3]| -> Value {
        fields
            .iter()
            .map(|field| segments_json(&resolve(field, &value)))
            .collect()
    };
    json!({
        "head": fields(&frame.header),
        "foot": fields(&frame.footer),
        "head_rule": frame.header_rule,
        "foot_rule": frame.footer_rule,
        "logo": frame.logo.key(),
        "size": frame.font_size(),
    })
}

/// All placeholders proAVA fills (without braces); others stay as written.
pub const PLACEHOLDER_NAMES: &[&str] = &[
    "projekt",
    "projektnummer",
    "projektbezeichnung",
    "baumassnahme",
    "lv",
    "lv_nr",
    "lv_bezeichnung",
    "dokumentart",
    "titel",
    "datum",
    "stand",
    "seite",
    "seiten",
    "planer",
    "bearbeiter",
    "ersteller",
    "bauherr",
    "auftraggeber",
    "vergabenummer",
    "waehrung",
    "datei",
    "kopftext",
    "fusstext",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn values(name: &str) -> Option<String> {
        match name {
            "projekt" => Some("Musterprojekt".into()),
            "datum" => Some("17.11.2026".into()),
            "kopftext" | "vergabenummer" => Some(String::new()),
            "planer" => Some("Planungsbüro\nBeispiel".into()),
            _ => None,
        }
    }

    fn text(lines: &[Vec<Segment>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|segment| match segment {
                        Segment::Text(text) => text.clone(),
                        Segment::Page => "<s>".into(),
                        Segment::Pages => "<n>".into(),
                        Segment::Title => "<t>".into(),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn placeholders_are_replaced_and_empty_lines_dropped() {
        assert_eq!(
            text(&resolve("{projekt} – {datum}", &values)),
            ["Musterprojekt – 17.11.2026"]
        );
        assert_eq!(
            text(&resolve("Seite {seite} von {seiten}", &values)),
            ["Seite <s> von <n>"]
        );
        assert_eq!(text(&resolve("{titel}", &values)), ["<t>"]);
        // Empty value: the line goes, plain text stays.
        assert_eq!(
            text(&resolve("{dokumentart}\n{kopftext}", &|name| {
                (name == "dokumentart")
                    .then(|| "LV".to_string())
                    .or(values(name))
            })),
            ["LV"]
        );
        assert!(resolve("Vergabe-Nr. {vergabenummer}", &values).is_empty());
        assert_eq!(text(&resolve("Vorabzug", &values)), ["Vorabzug"]);
        // Unknown placeholders and stray braces stay visible.
        assert_eq!(text(&resolve("{unbekannt} {", &values)), ["{unbekannt} {"]);
        assert_eq!(text(&resolve("{Projekt}", &values)), ["{Projekt}"]);
        // Values with line breaks stay on one line; at most two lines.
        assert_eq!(
            text(&resolve("{planer}", &values)),
            ["Planungsbüro Beispiel"]
        );
        assert_eq!(text(&resolve("a\nb\nc", &values)).len(), 2);
        assert!(resolve("", &values).is_empty());
    }

    #[test]
    fn default_frame_looks_like_before() {
        let frame = PrintFrame::default();
        let json = frame_json(
            &frame,
            &[
                ("projekt", "P".into()),
                ("dokumentart", "Angebotsaufforderung".into()),
                ("datum", "17.11.2026".into()),
                ("lv", "LV 480".into()),
                ("bearbeiter", "O. H.".into()),
            ],
        );
        assert_eq!(json["head"][0][0][0]["s"], "P");
        assert_eq!(
            json["head"][1].as_array().unwrap().len(),
            1,
            "ohne Kopftext eine Zeile"
        );
        assert_eq!(json["foot"][2][0][1]["t"], "page");
        assert_eq!(json["logo"], "off");
        assert_eq!(json["size"], 8.0);
    }
}
