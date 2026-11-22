//! Leistungsbereich (Gewerk) of a position: a code like „070“ in the LV's
//! catalog of the GAEB type „work category“ (BoQInfo/Ctlg, CtlgAssign at the
//! item). Input is free – a Leistungsbereich of STLB-Bau or an own short
//! name such as „MSR“; the list offers the Leistungsbereiche of STLB-Bau for
//! technical building services.

use std::collections::HashMap;

use crate::lv::{Lv, LvRow, RowId};
use crate::title_default;

/// GAEB CtlgCode is a string of at most 100 characters.
pub const WORK_CATEGORY_LIMIT: usize = 100;

/// The catalog the Leistungsbereiche refer to (GAEB Ctlg).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkCatalog {
    /// CtlgID in GAEB, e.g. "idLB".
    pub id: String,
    /// CtlgType, „work category“ in GAEB DA XML 3.x.
    pub kind: String,
    /// CtlgName (at most 80 characters).
    pub name: String,
}

impl Default for WorkCatalog {
    fn default() -> Self {
        Self {
            id: "idLB".into(),
            kind: WORK_CATEGORY_TYPE.into(),
            name: "Leistungsbereiche".into(),
        }
    }
}

/// CtlgType of a work category catalog (GAEB DA XML 3.3, tgCtlgEnum).
pub const WORK_CATEGORY_TYPE: &str = "work category";

/// Leistungsbereiche of STLB-Bau for technical building services:
/// suggestions for the field „Leistungsbereich“ (free input allowed).
/// Numbers and names as published by GAEB/DBD (STLB-Bau 2024-04 to
/// 2026-04, see docs/FACHLICHES.md).
pub const STLB_WORK_CATEGORIES: &[(&str, &str)] = &[
    (
        "041",
        "Wärmeversorgungsanlagen – Leitungen, Armaturen, Heizflächen",
    ),
    ("042", "Gas- und Wasseranlagen – Leitungen, Armaturen"),
    (
        "045",
        "Gas-, Wasser- und Entwässerungsanlagen – Ausstattung, Elemente, Fertigbäder",
    ),
    (
        "047",
        "Dämm- und Brandschutzarbeiten an technischen Anlagen",
    ),
    ("050", "Blitzschutz-/Erdungsanlagen, Überspannungsschutz"),
    ("051", "Kabelleitungstiefbauarbeiten"),
    ("052", "Mittelspannungsanlagen"),
    (
        "053",
        "Niederspannungsanlagen – Kabel/Leitungen, Verlegesysteme, Installationsgeräte",
    ),
    (
        "054",
        "Niederspannungsanlagen – Verteilersysteme und Einbaugeräte",
    ),
    ("058", "Leuchten und Lampen"),
    ("059", "Sicherheitsbeleuchtungsanlagen"),
    ("070", "Gebäudeautomation"),
    ("075", "Raumlufttechnische Anlagen"),
    ("078", "Kälteanlagen für raumlufttechnische Anlagen"),
];

/// Name of a Leistungsbereich of the list, if the code is one.
pub fn work_category_label(code: &str) -> Option<&'static str> {
    STLB_WORK_CATEGORIES
        .iter()
        .find(|(known, _)| *known == code.trim())
        .map(|(_, name)| *name)
}

/// „070 Gebäudeautomation“ (with name when known), else the code itself.
pub fn work_category_text(code: &str) -> String {
    match work_category_label(code) {
        Some(name) => format!("{} {name}", code.trim()),
        None => code.trim().to_string(),
    }
}

/// Input of the field „Leistungsbereich“: empty = none; a line of the list
/// („070 Gebäudeautomation“) gives its code, „70“ the three-digit code;
/// anything else stays as typed (own Gewerk names), at most 100 characters.
pub fn parse_work_category(text: &str) -> Result<Option<String>, String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Ok(None);
    }
    let first = text.split(' ').next().unwrap_or(&text);
    if first.bytes().all(|byte| byte.is_ascii_digit()) && first.len() <= 3 {
        let code = format!("{first:0>3}");
        let rest = text[first.len()..].trim();
        // „070“, „70“ or „070 Gebäudeautomation“ (from the list).
        if rest.is_empty() || work_category_label(&code).is_some_and(|name| name == rest) {
            return Ok(Some(code));
        }
    }
    if text.chars().count() > WORK_CATEGORY_LIMIT {
        return Err(format!(
            "Leistungsbereich hat {} Zeichen – GAEB überträgt höchstens {WORK_CATEGORY_LIMIT}.",
            text.chars().count()
        ));
    }
    Ok(Some(text))
}

/// Own Leistungsbereich of a title or position.
fn own_work_category(row: &LvRow) -> Option<&str> {
    match row {
        LvRow::Position(position) => position.work_category(),
        LvRow::Group(group) => group.work_category(),
        _ => None,
    }
}

/// Question before a title's Leistungsbereich changes over finished
/// positions (`count` from [`Lv::locked_work_category_heirs`]).
pub fn work_category_heirs_question(count: usize, code: Option<&str>) -> String {
    let (subject, verb_new, verb_lose) = if count == 1 {
        ("1 fertige Position".to_string(), "erbt", "verliert")
    } else {
        (format!("{count} fertige Positionen"), "erben", "verlieren")
    };
    match code {
        Some(code) => format!(
            "{subject} {verb_new} den neuen Leistungsbereich {}.",
            work_category_text(code)
        ),
        None => format!("{subject} {verb_lose} den Leistungsbereich des Titels."),
    }
}

impl Lv {
    /// Whether any title or position has a Leistungsbereich (then GAEB gets
    /// the catalog).
    pub fn has_work_categories(&self) -> bool {
        self.rows()
            .iter()
            .any(|row| own_work_category(row).is_some())
    }

    /// Leistungsbereich that applies to a row: its own, else that of the
    /// nearest title above it. `true` = inherited.
    pub fn effective_work_category(&self, index: usize) -> Option<(String, bool)> {
        title_default::effective_value(self, index, own_work_category)
    }

    /// Index of the title a row inherits its Leistungsbereich from (None:
    /// own value or none at all).
    pub fn work_category_source(&self, index: usize) -> Option<usize> {
        title_default::inherited_from(self, index, own_work_category)
    }

    /// [`Lv::effective_work_category`] of every row in one pass.
    pub fn effective_work_categories(&self) -> Vec<Option<(String, bool)>> {
        title_default::effective_values(self, own_work_category, &HashMap::new())
    }

    /// Finished positions whose Leistungsbereich would change if the titles
    /// at `titles` got `code`.
    pub fn locked_work_category_heirs(&self, titles: &[usize], code: Option<&str>) -> Vec<RowId> {
        title_default::locked_heirs(self, titles, code, own_work_category)
    }

    /// Leistungsbereiche in use in this LV (titles and positions), sorted,
    /// each once.
    pub fn work_categories_in_use(&self) -> Vec<String> {
        let mut codes: Vec<String> = self
            .rows()
            .iter()
            .filter_map(|row| own_work_category(row).map(str::to_string))
            .collect();
        codes.sort();
        codes.dedup();
        codes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Decimal, OzMask, Position};

    #[test]
    fn input_gives_codes_or_free_text() {
        assert_eq!(parse_work_category("  "), Ok(None));
        assert_eq!(parse_work_category("070"), Ok(Some("070".into())));
        assert_eq!(parse_work_category("70"), Ok(Some("070".into())));
        assert_eq!(
            parse_work_category("070 Gebäudeautomation"),
            Ok(Some("070".into())),
            "Zeile der Liste"
        );
        assert_eq!(
            parse_work_category("  MSR   Technik "),
            Ok(Some("MSR Technik".into())),
            "eigener Name, Leerraum bereinigt"
        );
        assert_eq!(
            parse_work_category("070 Regelung"),
            Ok(Some("070 Regelung".into())),
            "unbekannter Zusatz bleibt Text"
        );
        assert!(parse_work_category(&"x".repeat(101)).is_err());
        assert_eq!(work_category_text("070"), "070 Gebäudeautomation");
        assert_eq!(work_category_text("MSR"), "MSR");
        assert!(STLB_WORK_CATEGORIES
            .iter()
            .all(|(code, name)| code.len() == 3 && !name.is_empty()));
    }

    #[test]
    fn lv_lists_the_codes_in_use() {
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        assert!(!lv.has_work_categories());
        for code in [Some("075"), None, Some("070"), Some("075")] {
            let mut position = Position::new("", "P", Decimal::ONE, "St", None);
            position.set_work_category(code.map(str::to_string));
            lv.push_position(position);
        }
        assert!(lv.has_work_categories());
        assert_eq!(lv.work_categories_in_use(), ["070", "075"]);
        assert_eq!(lv.effective_work_category(0), Some(("075".into(), false)));
        assert_eq!(
            lv.effective_work_category(1),
            None,
            "ohne Titel nichts zu erben"
        );
        assert_eq!(WorkCatalog::default().kind, "work category");
        assert!(WorkCatalog::default().name.chars().count() <= 80);
    }

    #[test]
    fn titles_pass_their_work_category_down() {
        use crate::{EditStatus, LvGroup};
        let mut lv = Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        let mut title = LvGroup::new(0, "", "Automation");
        title.set_work_category(Some("070".into()));
        lv.push_group(title);
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        let station = lv.push_position(Position::new("", "DDC", Decimal::ONE, "St", None));
        let mut own = Position::new("", "Kabel", Decimal::TEN, "m", None);
        own.set_work_category(Some("053".into()));
        lv.push_position(own);
        lv.push_group(LvGroup::new(0, "", "Lüftung"));
        lv.push_position(Position::new("", "Klappe", Decimal::ONE, "St", None));
        lv.renumber();
        // 0 Automation, 1 ASP01, 2 DDC, 3 Kabel, 4 Lüftung, 5 Klappe
        assert_eq!(lv.effective_work_category(1), Some(("070".into(), true)));
        assert_eq!(lv.effective_work_category(2), Some(("070".into(), true)));
        assert_eq!(
            lv.effective_work_category(3),
            Some(("053".into(), false)),
            "eigener gewinnt"
        );
        assert_eq!(lv.effective_work_category(5), None, "anderer Titel");
        assert_eq!(lv.work_category_source(1), Some(0), "Untertitel erbt");
        assert_eq!(lv.work_category_source(2), Some(0), "über zwei Ebenen");
        assert_eq!(lv.work_category_source(3), None, "eigener Wert");
        assert_eq!(lv.work_category_source(0), None, "Titel mit eigenem");
        assert_eq!(lv.work_category_source(5), None, "nichts zu erben");
        assert_eq!(
            lv.effective_work_categories()[2],
            lv.effective_work_category(2)
        );
        assert_eq!(lv.work_categories_in_use(), ["053", "070"]);

        lv.set_status(&[station], EditStatus::Done);
        assert_eq!(lv.locked_work_category_heirs(&[0], Some("075")), [station]);
        assert_eq!(
            lv.locked_work_category_heirs(&[0], Some("070")),
            Vec::<RowId>::new(),
            "gleich"
        );
        assert_eq!(
            lv.locked_work_category_heirs(&[2], Some("075")),
            Vec::<RowId>::new(),
            "kein Titel"
        );
        assert_eq!(
            work_category_heirs_question(1, Some("075")),
            "1 fertige Position erbt den neuen Leistungsbereich 075 Raumlufttechnische Anlagen."
        );
        assert_eq!(
            work_category_heirs_question(2, None),
            "2 fertige Positionen verlieren den Leistungsbereich des Titels."
        );
    }
}
