//! Herkunft des Texts einer Position: from which text source (AUSSCHREIBEN.DE,
//! STLB-Bau, another program), which manufacturer or catalogue, when and from
//! which file it was taken over. Only for the planner – it stays in the
//! project file and is shown in the position window, never written to GAEB or
//! printed (the terms of AUSSCHREIBEN.DE do not ask for a source note, the
//! text itself stays as delivered).

/// Name under which texts from AUSSCHREIBEN.DE are kept.
pub const AUSSCHREIBEN_DE: &str = "AUSSCHREIBEN.DE";
/// Name under which texts from STLB-Bau are kept.
pub const STLB_BAU: &str = "STLB-Bau";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextOrigin {
    /// Text source, e.g. „AUSSCHREIBEN.DE“, „STLB-Bau“ or the program named in
    /// the file.
    pub source: String,
    /// Manufacturer or catalogue as far as the file tells (Hersteller line of
    /// the text, otherwise the name of the delivered LV).
    pub supplier: String,
    /// Day of the take-over (download), ISO `YYYY-MM-DD`.
    pub retrieved: String,
    /// Name of the file the text came from, without folder.
    pub file: String,
    /// Article number or other reference of the text, when the file tells.
    pub reference: String,
    /// Note on what happened to the text since, e.g. „in freie Position
    /// umgewandelt am 18.11.2026 (STLB-Bau 10/2026 · LB 070 · Leistung 3578)“.
    pub note: String,
}

impl TextOrigin {
    pub fn is_empty(&self) -> bool {
        self.source.trim().is_empty()
            && self.supplier.trim().is_empty()
            && self.retrieved.trim().is_empty()
            && self.file.trim().is_empty()
            && self.reference.trim().is_empty()
            && self.note.trim().is_empty()
    }

    /// One line for tables and the check bar: „AUSSCHREIBEN.DE · Belimo ·
    /// 17.11.2026“ (empty parts left out).
    pub fn summary(&self) -> String {
        let date = german(&self.retrieved);
        [self.source.trim(), self.supplier.trim(), date.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// „2026-11-17“ → „17.11.2026“; anything else unchanged.
fn german(iso: &str) -> String {
    let iso = iso.trim();
    let parts: Vec<&str> = iso.split('-').collect();
    match parts.as_slice() {
        [year, month, day]
            if year.len() == 4
                && month.len() == 2
                && day.len() == 2
                && iso.chars().all(|c| c.is_ascii_digit() || c == '-') =>
        {
            format!("{day}.{month}.{year}")
        }
        _ => iso.to_string(),
    }
}

/// Text source from what a GAEB file says about the program that wrote it
/// (GAEBInfo/ProgName or ProgSystem) and from its name; `fallback` when
/// neither tells (e.g. the source chosen in the dialog).
pub fn source_of(program: &str, file_name: &str, has_stlb_key: bool, fallback: &str) -> String {
    let program_lower = program.to_lowercase();
    let file_lower = file_name.to_lowercase();
    if program_lower.contains("ausschreiben") || file_lower.contains("ausschreiben") {
        return AUSSCHREIBEN_DE.to_string();
    }
    if has_stlb_key || program_lower.contains("stlb") {
        return STLB_BAU.to_string();
    }
    if !fallback.trim().is_empty() {
        return fallback.trim().to_string();
    }
    program.trim().to_string()
}

/// Notes `source` in the Herkunft of read positions: with `replace_unknown`
/// (the user chose the source, e.g. „Aus AUSSCHREIBEN.DE übernehmen“) it
/// replaces a source the file names but proAVA does not know (an export of
/// AUSSCHREIBEN.DE may name another program); otherwise only an empty one.
/// `today` (ISO, the first ten characters count) fills a missing date.
pub fn adopt_source(
    positions: &mut [crate::Position],
    source: &str,
    replace_unknown: bool,
    today: &str,
) {
    for position in positions {
        let mut origin = position.text_origin().cloned().unwrap_or_default();
        let known = [AUSSCHREIBEN_DE, STLB_BAU].contains(&origin.source.as_str());
        if (replace_unknown && !known) || origin.source.trim().is_empty() {
            origin.source = source.to_string();
        }
        if origin.retrieved.trim().is_empty() {
            origin.retrieved = today.chars().take(10).collect();
        }
        position.set_text_origin(Some(origin));
    }
}

/// Labels that name the manufacturer in a text: „Hersteller: …“, „Fabrikat: …“.
const SUPPLIER_LABELS: [&str; 4] = ["hersteller", "fabrikat", "produkt von", "marke"];
/// Labels of an article number: „Artikelnummer: …“, „Art.-Nr. …“.
const REFERENCE_LABELS: [&str; 5] = [
    "artikelnummer",
    "artikel-nr",
    "art.-nr",
    "art.nr",
    "bestellnummer",
];

/// Manufacturer named in a line „Hersteller: Muster GmbH“ or „Fabrikat: …“ of
/// the text, if any. Lines meant for the bidder („Fabrikat: …………“, „vom Bieter
/// einzutragen“) do not count.
pub fn supplier_in_text(text: &str) -> Option<String> {
    labelled_value(text, &SUPPLIER_LABELS)
}

/// Article number named in a line „Artikelnummer: 01APS-101“ of the text.
pub fn reference_in_text(text: &str) -> Option<String> {
    labelled_value(text, &REFERENCE_LABELS)
}

fn labelled_value(text: &str, labels: &[&str]) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let lower = line.to_lowercase();
        let Some(label) = labels.iter().find(|label| lower.starts_with(*label)) else {
            continue;
        };
        // `line` and `lower` have the same length up to the label: labels are ASCII.
        let Some(rest) = line.get(label.len()..) else {
            continue;
        };
        // „Fabrikat/Typ: …“: skip the second word up to the colon.
        let rest = match rest.strip_prefix('/') {
            Some(after) => match after.find(':') {
                Some(colon) if colon <= 15 => &after[colon..],
                _ => continue,
            },
            None => rest,
        };
        let abbreviation = label.ends_with("nr");
        let rest = rest.trim_start_matches([' ', '\t', '.']);
        let rest = match rest.strip_prefix(':') {
            Some(after) => after,
            // „Art.-Nr. 4711“ needs no colon, prose („Hersteller und Typ …“) does.
            None if abbreviation && rest.len() < line.len() - label.len() => rest,
            None => continue,
        };
        let value = rest.trim();
        let placeholder = value.is_empty()
            || value
                .chars()
                .all(|c| matches!(c, '.' | '…' | '_' | '-' | ' ' | '\''))
            || value.to_lowercase().contains("bieter");
        if placeholder {
            continue;
        }
        let value: String = value.chars().take(80).collect();
        return Some(value.trim().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_leaves_out_empty_parts_and_shows_a_german_date() {
        let origin = TextOrigin {
            source: AUSSCHREIBEN_DE.into(),
            supplier: "Muster Antriebe GmbH".into(),
            retrieved: "2026-11-17".into(),
            file: "Merkliste.X83".into(),
            reference: String::new(),
            note: String::new(),
        };
        assert_eq!(
            origin.summary(),
            "AUSSCHREIBEN.DE · Muster Antriebe GmbH · 17.11.2026"
        );
        assert!(!origin.is_empty());
        assert!(TextOrigin::default().is_empty());
        let only_source = TextOrigin {
            source: "Programm X".into(),
            ..TextOrigin::default()
        };
        assert_eq!(only_source.summary(), "Programm X");
        assert_eq!(german("17.11.2026"), "17.11.2026");
    }

    #[test]
    fn source_from_program_name_file_name_key_or_fallback() {
        assert_eq!(
            source_of("AUSSCHREIBEN.DE 5.2", "x.X83", false, ""),
            AUSSCHREIBEN_DE
        );
        assert_eq!(
            source_of("", "ausschreiben_de_Merkliste.x81", false, ""),
            AUSSCHREIBEN_DE
        );
        assert_eq!(
            source_of("Irgendwas", "LV.X83", true, AUSSCHREIBEN_DE),
            STLB_BAU
        );
        assert_eq!(source_of("STLB-Bau Online", "LV.X83", false, ""), STLB_BAU);
        assert_eq!(
            source_of("Programm X", "LV.X83", false, AUSSCHREIBEN_DE),
            AUSSCHREIBEN_DE
        );
        assert_eq!(source_of("Programm X", "LV.X83", false, ""), "Programm X");
    }

    #[test]
    fn manufacturer_and_article_number_lines_are_found_but_not_bidder_gaps() {
        let text = "Klappenantrieb für Luftklappen\nFabrikat: …………\nHersteller: Muster Antriebe GmbH\nArtikelnummer: MA-24-SR\noder gleichwertig";
        assert_eq!(
            supplier_in_text(text).as_deref(),
            Some("Muster Antriebe GmbH")
        );
        assert_eq!(reference_in_text(text).as_deref(), Some("MA-24-SR"));
        assert_eq!(supplier_in_text("Fabrikat: vom Bieter einzutragen"), None);
        assert_eq!(
            supplier_in_text("Fabrikat/Typ: Muster AG, Typ 7"),
            Some("Muster AG, Typ 7".into())
        );
        assert_eq!(reference_in_text("Art.-Nr. 4711"), Some("4711".into()));
        assert_eq!(supplier_in_text("Kein Hinweis"), None);
        assert_eq!(supplier_in_text("Herstellerangaben: beachten"), None);
        assert_eq!(supplier_in_text("Hersteller und Typ sind anzugeben"), None);
        assert_eq!(reference_in_text("Art.-Nr.4711"), Some("4711".into()));
    }
}
