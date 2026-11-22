//! Zeilenart umwandeln: Position, Hinweis (Text) und Ausführungsbeschreibung
//! ineinander (Anwender 18.11.: eine Standardbeschreibung aus STLB-Bau kam als
//! Position, ist aber ein Text).
//!
//! Was alle drei tragen, bleibt: Kurztext (Überschrift, Bezeichnung),
//! Langtext mit Bildern, Textergänzungen, STLB-Bau-Kennung bzw.
//! Standardleistungsnummer und die Textquelle. Was die Zielart nicht tragen
//! kann (Menge, Einheit, EP, Kostengruppe, Ordnungszahl, Nummer der
//! Ausführungsbeschreibung …), fällt weg; [`Lv::conversion`] nennt es vorher.
//! Die Zeile behält ihre Id: Strg+Z (ein Schnappschuss des LV) stellt alles
//! wieder her, auch die Zuordnung von Angebotspreisen.

use crate::edit_status::{EditStatus, LOCKED_HINT};
use crate::lv::{Lv, LvRow, Note};
use crate::money::format_decimal_de;
use crate::performance::PerformanceDescription;
use crate::position::{Position, PositionKind, TextComplement};
use crate::rich_text::RichText;
use crate::stlb::CatalogKeys;
use crate::text_origin::TextOrigin;
use crate::{Decimal, RowId};

/// The kinds of rows that can become one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowKind {
    Position,
    /// Hinweis (Text, Vorbemerkung) – GAEB Remark.
    Note,
    /// Ausführungsbeschreibung – GAEB PerfDescr.
    Performance,
}

impl RowKind {
    pub const ALL: [RowKind; 3] = [RowKind::Position, RowKind::Note, RowKind::Performance];

    /// Kind of `row`; None for titles and Abstandshalter.
    pub fn of(row: &LvRow) -> Option<Self> {
        match row {
            LvRow::Position(_) => Some(RowKind::Position),
            LvRow::Note(_) => Some(RowKind::Note),
            LvRow::Performance(_) => Some(RowKind::Performance),
            LvRow::Group(_) | LvRow::Spacer(_) => None,
        }
    }

    /// For menus and choices: „Position“, „Hinweis (Text)“, „Ausführungsbeschreibung“.
    pub fn label(self) -> &'static str {
        match self {
            RowKind::Position => "Position",
            RowKind::Note => "Hinweis (Text)",
            RowKind::Performance => "Ausführungsbeschreibung",
        }
    }

    /// For sentences: „Position“, „Hinweis“, „Ausführungsbeschreibung“.
    pub fn name(self) -> &'static str {
        match self {
            RowKind::Position => "Position",
            RowKind::Note => "Hinweis",
            RowKind::Performance => "Ausführungsbeschreibung",
        }
    }

    /// Key for the MCP server and settings: „position“, „hinweis“, „ausfuehrungsbeschreibung“.
    pub fn key(self) -> &'static str {
        match self {
            RowKind::Position => "position",
            RowKind::Note => "hinweis",
            RowKind::Performance => "ausfuehrungsbeschreibung",
        }
    }

    /// Reads [`Self::key`] and common words („text“, „ab“, „vorbemerkung“).
    pub fn parse(text: &str) -> Option<Self> {
        let key = text.trim().to_lowercase().replace(['ü', 'Ü'], "ue");
        Some(match key.as_str() {
            "position" | "pos" | "leistungsposition" => RowKind::Position,
            "hinweis" | "hinweistext" | "text" | "vorbemerkung" | "note" => RowKind::Note,
            "ausfuehrungsbeschreibung" | "ab" | "performance" => RowKind::Performance,
            _ => return None,
        })
    }
}

/// What a conversion drops and which references it removes – for the
/// question before it ([`Lv::conversion`]) and the report after it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub from: RowKind,
    pub to: RowKind,
    /// What the new kind cannot carry, in words, e.g. „Menge 5 St“.
    pub dropped: Vec<String>,
    /// Positions whose reference to this row goes (they refer to the
    /// Ausführungsbeschreibung or repeat the text of the position).
    pub unlinked: Vec<RowId>,
}

impl Conversion {
    /// Something is lost: ask before converting.
    pub fn loses_something(&self) -> bool {
        !self.dropped.is_empty() || !self.unlinked.is_empty()
    }
}

/// The parts all three kinds carry.
struct Parts {
    title: String,
    text: RichText,
    complements: Vec<TextComplement>,
    catalog: CatalogKeys,
    origin: Option<TextOrigin>,
}

impl Parts {
    fn of(row: &LvRow) -> Option<Self> {
        Some(match row {
            LvRow::Position(position) => Parts {
                title: position.short_text().to_string(),
                text: position.long_text().clone(),
                complements: position.complements().to_vec(),
                catalog: CatalogKeys {
                    stlb: position.stlb_reference().cloned(),
                    standard_number: position.standard_number().cloned(),
                },
                origin: position.text_origin().cloned(),
            },
            LvRow::Note(note) => Parts {
                title: note.heading().to_string(),
                text: note.text().clone(),
                complements: note.complements().to_vec(),
                catalog: note.catalog().clone(),
                origin: note.text_origin().cloned(),
            },
            LvRow::Performance(performance) => Parts {
                title: performance_title(performance),
                text: performance.long_text().clone(),
                complements: performance.complements().to_vec(),
                catalog: performance.catalog().clone(),
                origin: performance.text_origin().cloned(),
            },
            LvRow::Group(_) | LvRow::Spacer(_) => return None,
        })
    }
}

/// Kurztext of an Ausführungsbeschreibung for another kind: its Kurztext,
/// else the Bezeichnung.
fn performance_title(performance: &PerformanceDescription) -> String {
    if performance.short_text().trim().is_empty() {
        performance.label().to_string()
    } else {
        performance.short_text().to_string()
    }
}

/// `row` as a row of kind `to` with the same id; order number and number of
/// an Ausführungsbeschreibung are left empty (the LV gives them when the row
/// is inserted). None for titles and Abstandshalter.
pub fn converted(row: &LvRow, to: RowKind) -> Option<LvRow> {
    let parts = Parts::of(row)?;
    let converted = match to {
        RowKind::Note => {
            let mut note = Note::rich(parts.text);
            note.set_heading(parts.title);
            note.set_complements(parts.complements);
            note.set_catalog(parts.catalog);
            note.set_text_origin(parts.origin);
            LvRow::Note(note)
        }
        RowKind::Position => {
            let mut position = Position::new("", parts.title, Decimal::ZERO, "", None);
            position.set_long_text(parts.text);
            position.set_complements(parts.complements);
            position.set_stlb_reference(parts.catalog.stlb);
            position.set_standard_number(parts.catalog.standard_number);
            position.set_text_origin(parts.origin);
            LvRow::Position(position)
        }
        RowKind::Performance => {
            let mut performance = PerformanceDescription::new("", parts.title.clone(), parts.text);
            performance.set_short_text(parts.title);
            performance.set_complements(parts.complements);
            performance.set_catalog(parts.catalog);
            performance.set_text_origin(parts.origin);
            LvRow::Performance(performance)
        }
    };
    Some(converted.with_id(row.id()))
}

impl Lv {
    /// What converting row `index` into `to` would drop, or why it cannot be
    /// done (title, Abstandshalter, same kind, finished position, a position
    /// outside a title that holds positions – see [`Lv::holds_positions_directly`]).
    pub fn conversion(&self, index: usize, to: RowKind) -> Result<Conversion, String> {
        let row = self
            .rows()
            .get(index)
            .ok_or("Die Zeile gibt es nicht (mehr).")?;
        let from =
            RowKind::of(row).ok_or("Titel und Abstandshalter lassen sich nicht umwandeln.")?;
        if from == to {
            return Err(format!("Die Zeile ist schon {}.", article(to)));
        }
        if let LvRow::Position(position) = row {
            if position.status() == EditStatus::Done {
                return Err(format!(
                    "Die Position {} ist fertig und damit gesperrt – {LOCKED_HINT}.",
                    position.ordinal()
                ));
            }
        }
        if to == RowKind::Position && self.level_count() > 0 {
            let deepest = self.level_count() - 1;
            let title = self.rows()[..index].iter().rposition(LvRow::is_group);
            if !title.is_some_and(|at| self.holds_positions_directly(at)) {
                return Err(format!(
                    "Hier kann keine Position stehen – Positionen gehören unter einen Titel der \
                     untersten Stufe („{}“) oder einer Stufe, auf der schon Positionen stehen. Die \
                     Zeile erst dorthin verschieben.",
                    self.level_label(deepest)
                ));
            }
        }
        let mut dropped = Vec::new();
        let mut unlinked = Vec::new();
        match row {
            LvRow::Position(position) => {
                dropped = position_parts(self, position);
                unlinked = self.positions_repeating(position.id());
            }
            LvRow::Note(note) => {
                if note.is_preliminary() {
                    dropped.push("Kennzeichen Vorbemerkung".to_string());
                }
                if note.is_closing() {
                    dropped.push("Kennzeichen Schlussbemerkung".to_string());
                }
            }
            LvRow::Performance(performance) => {
                dropped.push(format!("Nummer {}", performance.mark()));
                let title = performance_title(performance);
                let label = performance.label().trim();
                if !label.is_empty() && label != title.trim() {
                    dropped.push(format!("Bezeichnung „{label}“"));
                }
                unlinked = self.positions_using(performance.id());
            }
            LvRow::Group(_) | LvRow::Spacer(_) => {}
        }
        Ok(Conversion {
            from,
            to,
            dropped,
            unlinked,
        })
    }

    /// Converts row `index` into `to` (same id, same place). A position gets a
    /// free order number (an automatically numbered LV is renumbered), an
    /// Ausführungsbeschreibung the next free number. References to the row
    /// that the new kind cannot have are removed. Returns what was dropped.
    pub fn convert_row(&mut self, index: usize, to: RowKind) -> Result<Conversion, String> {
        let conversion = self.conversion(index, to)?;
        let row =
            converted(&self.rows()[index], to).ok_or("Die Zeile lässt sich nicht umwandeln.")?;
        // Checked before: without the row the LV is not continuous any more.
        let auto = self.is_auto_numbered();
        let mut trial = self.clone();
        trial.remove_row(index);
        if auto {
            let mut renumbered = trial.clone();
            renumbered.insert_row(index, row.clone());
            renumbered.renumber();
            let numbered = renumbered
                .rows()
                .get(index)
                .is_some_and(|row| !row.has_ordinal() || !row.ordinal().trim().is_empty());
            if numbered && renumbered.duplicate_ordinals().is_empty() {
                *self = renumbered;
                return Ok(conversion);
            }
        }
        trial.insert_with_free_ordinal(index, row)?;
        *self = trial;
        Ok(conversion)
    }
}

/// „eine Position“, „ein Hinweis“, „eine Ausführungsbeschreibung“.
fn article(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Position => "eine Position",
        RowKind::Note => "ein Hinweis",
        RowKind::Performance => "eine Ausführungsbeschreibung",
    }
}

/// What only a position has and carries now, in words.
fn position_parts(lv: &Lv, position: &Position) -> Vec<String> {
    let mut parts = Vec::new();
    if !position.ordinal().trim().is_empty() {
        parts.push(format!("Ordnungszahl {}", position.ordinal()));
    }
    let quantity = position.quantity();
    if !quantity.is_zero() || !position.unit().trim().is_empty() {
        let amount = format_decimal_de(quantity, quantity.normalize().scale());
        parts.push(
            format!("Menge {amount} {}", position.unit())
                .trim()
                .to_string(),
        );
    }
    if position.quantity_formula().is_some() {
        parts.push("Rechenansatz der Menge".to_string());
    }
    if !position.takeoff().is_empty() {
        parts.push(format!(
            "Mengenermittlung ({} Zeilen)",
            position.takeoff().len()
        ));
    }
    if let Some(price) = position.unit_price() {
        parts.push(format!(
            "Einheitspreis {} {}",
            format_decimal_de(price, 2),
            lv.currency()
        ));
    }
    match position.kind() {
        PositionKind::Normal => {}
        kind => parts.push(format!("Positionsart {}", kind.label())),
    }
    if position.is_lump_sum() {
        parts.push("pauschal".to_string());
    }
    if position.is_hourly() {
        parts.push("Stundenlohn".to_string());
    }
    if let Some(code) = position.cost_group() {
        parts.push(format!("Kostengruppe {code}"));
    }
    if let Some(code) = position.work_category() {
        parts.push(format!("Leistungsbereich {code}"));
    }
    if !position.sub_descriptions().is_empty() {
        parts.push(format!(
            "Unterbeschreibungen ({})",
            position.sub_descriptions().len()
        ));
    }
    if let Some(performance) = position
        .performance()
        .and_then(|id| lv.performance_by_id(id))
    {
        parts.push(format!("Bezug auf {}", performance.mark()));
    }
    if let Some(base) = position.repeats().and_then(|id| lv.position_by_id(id)) {
        parts.push(format!("Wiederholung von Position {}", base.ordinal()));
    }
    for (set, word) in [
        (position.is_quantity_open(), "Menge vom Bieter"),
        (position.has_price_breakdown(), "EP-Aufgliederung"),
        (position.is_not_applicable(), "Kennzeichen „entfällt“"),
        (position.is_key_item(), "Schwerpunktposition"),
        (
            position.price_components().iter().any(Option::is_some),
            "EP-Anteile",
        ),
        (position.time_quantity().is_some(), "Zeitansatz"),
        (
            position.origin().is_some(),
            "Verknüpfung mit der Bibliothek",
        ),
        (
            position.is_stlb_placeholder(),
            "Kennzeichen STLB-Bau-Platzhalter",
        ),
    ] {
        if set {
            parts.push(word.to_string());
        }
    }
    if position.status() == EditStatus::InProgress {
        parts.push("Status „in Bearbeitung“".to_string());
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComplementKind, LvGroup, OzMask, StandardNumber, StlbReference, TextOrigin};

    fn stlb() -> StlbReference {
        StlbReference {
            catalog: "STLB-Bau".into(),
            version: Some("2026-10".into()),
            work_category: "070".into(),
            group: 4711,
            ..StlbReference::default()
        }
    }

    fn origin() -> TextOrigin {
        TextOrigin {
            source: "STLB-Bau".into(),
            file: "Test.X81".into(),
            ..TextOrigin::default()
        }
    }

    fn bidder() -> TextComplement {
        TextComplement {
            marker: "1".into(),
            kind: ComplementKind::Bidder,
            caption: "Fabrikat ".into(),
            ..TextComplement::default()
        }
    }

    /// LV 11.PPPP with one title, a position carrying everything and a
    /// second position after it.
    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Gebäudeautomation"));
        let mut text = Position::new(
            "",
            "Standardbeschreibung Management",
            Decimal::from(5),
            "St",
            Some(Decimal::new(1250, 2)),
        );
        text.set_long_text(RichText::plain("Gefordert sind die Einrichtungen."));
        text.set_complements(vec![bidder()]);
        text.attach_stlb(stlb());
        text.set_text_origin(Some(origin()));
        text.set_cost_group(Some("481".into()));
        lv.push_position(text);
        lv.push_position(Position::new("", "Danach", Decimal::ONE, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn position_becomes_a_hint_and_back_with_its_text() {
        let mut lv = lv();
        let id = lv.rows()[1].id();
        let preview = lv.conversion(1, RowKind::Note).unwrap();
        assert_eq!(
            preview.dropped,
            vec![
                "Ordnungszahl 01.0010",
                "Menge 5 St",
                "Einheitspreis 12,50 EUR",
                "Kostengruppe 481"
            ]
        );
        lv.convert_row(1, RowKind::Note).unwrap();
        let LvRow::Note(note) = &lv.rows()[1] else {
            panic!("{:?}", lv.rows()[1])
        };
        assert_eq!(note.id(), id, "dieselbe Id");
        assert_eq!(note.heading(), "Standardbeschreibung Management");
        assert_eq!(note.text().to_plain(), "Gefordert sind die Einrichtungen.");
        assert_eq!(note.complements(), &[bidder()]);
        assert_eq!(note.catalog().stlb.as_ref().unwrap().group, 4711);
        assert_eq!(note.text_origin(), Some(&origin()));
        assert_eq!(
            lv.rows()[2].ordinal(),
            "01.0010",
            "fortlaufend neu nummeriert"
        );

        lv.convert_row(1, RowKind::Position).unwrap();
        let position = lv.rows()[1].as_position().unwrap();
        assert_eq!(position.id(), id);
        assert_eq!(position.ordinal(), "01.0010");
        assert_eq!(lv.rows()[2].ordinal(), "01.0020");
        assert_eq!(position.short_text(), "Standardbeschreibung Management");
        assert_eq!((position.quantity(), position.unit()), (Decimal::ZERO, ""));
        assert_eq!(position.stlb_reference().unwrap().group, 4711);
        assert!(!position.stlb_text_changed(), "Text wie geliefert");
        assert_eq!(position.complements(), &[bidder()]);
        assert_eq!(position.text_origin(), Some(&origin()));
    }

    #[test]
    fn performance_takes_text_and_gives_up_its_users() {
        let mut lv = lv();
        lv.convert_row(1, RowKind::Performance).unwrap();
        let performance = lv.rows()[1].as_performance().unwrap().clone();
        assert_eq!(performance.number(), "1");
        assert_eq!(performance.label(), "Standardbeschreibung Management");
        assert_eq!(performance.complements(), &[bidder()]);
        assert_eq!(performance.catalog().stlb.as_ref().unwrap().group, 4711);
        let user = lv.rows()[2].id();
        lv.link_performance(user, Some(performance.id())).unwrap();

        let preview = lv.conversion(1, RowKind::Note).unwrap();
        assert_eq!(preview.dropped, vec!["Nummer A0001"]);
        assert_eq!(preview.unlinked, vec![user]);
        lv.convert_row(1, RowKind::Note).unwrap();
        assert_eq!(
            lv.position_by_id(user).unwrap().performance(),
            None,
            "Bezug gelöst"
        );
        assert_eq!(
            lv.rows()[1].as_note().unwrap().heading(),
            "Standardbeschreibung Management"
        );
    }

    #[test]
    fn what_cannot_be_converted_says_why() {
        let mut lv = lv();
        assert!(lv
            .conversion(0, RowKind::Note)
            .unwrap_err()
            .contains("Titel"));
        assert!(lv
            .conversion(1, RowKind::Position)
            .unwrap_err()
            .contains("schon eine Position"));
        lv.row_mut(1)
            .and_then(LvRow::as_position_mut)
            .unwrap()
            .set_status(EditStatus::Done);
        let error = lv.conversion(1, RowKind::Note).unwrap_err();
        assert!(error.contains("gesperrt"), "{error}");

        // A hint before the first title cannot become a position.
        lv.insert_row(0, LvRow::Note(Note::new("Vorbemerkung")));
        let error = lv.conversion(0, RowKind::Position).unwrap_err();
        assert!(error.contains("untersten Stufe"), "{error}");
        assert!(lv.conversion(0, RowKind::Performance).is_ok());
    }

    /// Like a Positionsbibliothek: mask with four title levels, positions
    /// under titles of level 2 – a hint there may become a position.
    #[test]
    fn a_hint_under_a_title_with_positions_above_the_deepest_level_becomes_one() {
        let mut lv = Lv::new("LV", OzMask::parse("11.222.33.44.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "Hardware"));
        lv.push_position(Position::new("", "A", Decimal::ONE, "St", None));
        lv.push_row(LvRow::Note(Note::new("Router")));
        lv.push_group(LvGroup::new(1, "", "Software"));
        lv.renumber();
        assert!(lv.holds_positions_directly(1), "Hardware hat Positionen");
        assert!(
            lv.holds_positions_directly(4),
            "Software: gleiche Stufe, keine Untertitel"
        );
        assert!(!lv.holds_positions_directly(0), "Automation hat Untertitel");
        lv.convert_row(3, RowKind::Position).unwrap();
        assert_eq!(lv.rows()[3].ordinal(), "01.001.00.00.0020");
        // A title of level 1 with subtitles: no position directly there.
        lv.insert_row(1, LvRow::Note(Note::new("Vortext")));
        let error = lv.conversion(1, RowKind::Position).unwrap_err();
        assert!(error.contains("untersten Stufe"), "{error}");
    }

    #[test]
    fn hint_into_a_position_of_an_lv_with_own_numbers_gets_a_free_number() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "GA"));
        lv.push_position(Position::new("01.0100", "A", Decimal::ONE, "St", None));
        let mut note = Note::new("Text");
        note.set_preliminary(true);
        note.set_catalog(CatalogKeys {
            stlb: None,
            standard_number: Some(StandardNumber {
                number: "123".into(),
                catalog: "StLB".into(),
            }),
        });
        lv.push_note(note);
        lv.push_position(Position::new("01.0200", "B", Decimal::ONE, "St", None));
        assert!(!lv.is_auto_numbered());
        let preview = lv.conversion(2, RowKind::Position).unwrap();
        assert_eq!(preview.dropped, vec!["Kennzeichen Vorbemerkung"]);
        lv.convert_row(2, RowKind::Position).unwrap();
        let position = lv.rows()[2].as_position().unwrap();
        let ordinal = position.ordinal();
        assert!(ordinal > "01.0100" && ordinal < "01.0200", "{ordinal}");
        assert_eq!(position.standard_number().unwrap().number, "123");
        assert_eq!(
            lv.rows()[1].ordinal(),
            "01.0100",
            "Nachbarn behalten ihre OZ"
        );
        assert_eq!(lv.rows()[3].ordinal(), "01.0200");
    }

    #[test]
    fn kinds_read_their_keys() {
        for kind in RowKind::ALL {
            assert_eq!(RowKind::parse(kind.key()), Some(kind));
        }
        assert_eq!(RowKind::parse("Text"), Some(RowKind::Note));
        assert_eq!(RowKind::parse("AB"), Some(RowKind::Performance));
        assert_eq!(RowKind::parse("Titel"), None);
    }
}
