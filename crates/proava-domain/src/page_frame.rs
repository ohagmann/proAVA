//! Kopf- und Fußzeile der Ausdrucke: je drei Felder (links, Mitte, rechts),
//! eine oder zwei Zeilen aus Text und Platzhaltern. Vorgabe in den
//! Einstellungen, im Projekt überschreibbar.

/// Where the logo sits in the page header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameLogoPlace {
    /// Only in the title block of the first page (as before).
    #[default]
    Off,
    Left,
    Right,
}

impl FrameLogoPlace {
    pub const ALL: [FrameLogoPlace; 3] = [
        FrameLogoPlace::Off,
        FrameLogoPlace::Left,
        FrameLogoPlace::Right,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FrameLogoPlace::Off => "aus",
            FrameLogoPlace::Left => "links",
            FrameLogoPlace::Right => "rechts",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|place| place.key() == key.trim().to_lowercase())
    }

    pub fn label(self) -> &'static str {
        match self {
            FrameLogoPlace::Off => "nicht im Seitenkopf",
            FrameLogoPlace::Left => "links im Seitenkopf",
            FrameLogoPlace::Right => "rechts im Seitenkopf",
        }
    }
}

/// Page header and footer of the printouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFrame {
    /// Left, centre, right; a line break starts the second line.
    pub header: [String; 3],
    pub footer: [String; 3],
    pub header_rule: bool,
    pub footer_rule: bool,
    pub logo: FrameLogoPlace,
    /// 7 to 10 pt.
    pub font_size_pt: u8,
}

/// Standard: Projekt | Dokumentart | Datum, under it LV | Bearbeiter |
/// „Seite x von y“; the free texts of the print dialog as second lines.
pub const DEFAULT_PAGE_HEADER: [&str; 3] = ["{projekt}", "{dokumentart}\n{kopftext}", "{datum}"];
pub const DEFAULT_PAGE_FOOTER: [&str; 3] = [
    "{lv}",
    "{bearbeiter}\n{fusstext}",
    "Seite {seite} von {seiten}",
];
pub const FRAME_FONT_SIZES: std::ops::RangeInclusive<u8> = 7..=10;

impl Default for PageFrame {
    fn default() -> Self {
        Self {
            header: DEFAULT_PAGE_HEADER.map(String::from),
            footer: DEFAULT_PAGE_FOOTER.map(String::from),
            header_rule: true,
            footer_rule: true,
            logo: FrameLogoPlace::Off,
            font_size_pt: 8,
        }
    }
}

impl PageFrame {
    /// The frame with every field cleaned: line breaks as „\n“, at most two
    /// lines, no trailing spaces; font size within [`FRAME_FONT_SIZES`].
    pub fn normalized(&self) -> Self {
        let clean = |field: &String| -> String {
            field
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .lines()
                .take(2)
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_string()
        };
        Self {
            header: [
                clean(&self.header[0]),
                clean(&self.header[1]),
                clean(&self.header[2]),
            ],
            footer: [
                clean(&self.footer[0]),
                clean(&self.footer[1]),
                clean(&self.footer[2]),
            ],
            font_size_pt: self
                .font_size_pt
                .clamp(*FRAME_FONT_SIZES.start(), *FRAME_FONT_SIZES.end()),
            ..self.clone()
        }
    }

    /// Placeholders in the fields that proAVA does not know (typos), e.g.
    /// `{Projekt}` – for a hint in the dialog.
    pub fn unknown_placeholders(&self) -> Vec<String> {
        let mut unknown = Vec::new();
        for field in self.header.iter().chain(&self.footer) {
            let mut rest = field.as_str();
            while let Some(start) = rest.find('{') {
                let Some(length) = rest[start..].find('}') else {
                    break;
                };
                let name = &rest[start + 1..start + length];
                if !PAGE_PLACEHOLDERS.iter().any(|(known, _)| *known == name)
                    && !unknown.iter().any(|seen| seen == name)
                {
                    unknown.push(name.to_string());
                }
                rest = &rest[start + length + 1..];
            }
        }
        unknown
    }
}

/// Placeholders of header and footer with their meaning (for the dialog,
/// the handbook and the MCP server). The print crate knows the same names.
pub const PAGE_PLACEHOLDERS: &[(&str, &str)] = &[
    ("projekt", "Projektname"),
    (
        "projektnummer",
        "Projektnummer des Auftraggebers (Kurzname)",
    ),
    ("projektbezeichnung", "Projektbezeichnung"),
    ("baumassnahme", "Baumaßnahme (erste Zeile)"),
    ("lv", "LV mit Nummer und Bezeichnung"),
    ("lv_nr", "LV-Nummer"),
    ("lv_bezeichnung", "LV-Bezeichnung"),
    ("dokumentart", "Dokumentart, z. B. Angebotsaufforderung"),
    ("titel", "Titel, in dem die Seite beginnt (LV-Ausdrucke)"),
    ("datum", "Druckdatum"),
    ("stand", "Stand (Druckdatum)"),
    ("seite", "Seitenzahl"),
    ("seiten", "Seitenanzahl"),
    ("planer", "Aufsteller / Planer"),
    (
        "bearbeiter",
        "Bearbeiter (Einstellungen › Drucken, sonst der Planer)",
    ),
    ("ersteller", "Ersteller, wie Bearbeiter"),
    ("bauherr", "Bauherr"),
    ("auftraggeber", "Auftraggeber / Vergabestelle"),
    ("vergabenummer", "Vergabenummer"),
    ("waehrung", "Währung des LV, z. B. EUR, CHF"),
    ("datei", "Dateiname des PDF"),
    ("kopftext", "Zusatzzeile Kopf aus dem Druckdialog"),
    ("fusstext", "Zusatzzeile Fuß aus dem Druckdialog"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_keeps_two_lines_and_finds_typos() {
        let mut frame = PageFrame::default();
        frame.header[0] = "{projekt}  \r\nzweite\r\ndritte".into();
        frame.footer[1] = "{Projekt} {datum} {unbekannt".into();
        frame.font_size_pt = 30;
        let clean = frame.normalized();
        assert_eq!(clean.header[0], "{projekt}\nzweite");
        assert_eq!(clean.font_size_pt, 10);
        assert_eq!(frame.unknown_placeholders(), vec!["Projekt".to_string()]);
        assert!(PageFrame::default().unknown_placeholders().is_empty());
        assert_eq!(
            FrameLogoPlace::from_key("Rechts"),
            Some(FrameLogoPlace::Right)
        );
    }
}
