use crate::rich_text::RichText;
use crate::stlb::CatalogKeys;
use crate::{RowId, TextComplement, TextOrigin};

/// Ausführungsbeschreibung: a text that describes how work is done once,
/// referenced by any number of positions (GAEB `PerfDescr`). It stands at
/// its place in the outline (usually at the start of a title), has no
/// quantity or price and never counts in a sum. Positions point to it with
/// [`crate::Position::performance`]; the number is unique in the LV.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceDescription {
    id: RowId,
    number: String,
    label: String,
    short_text: String,
    long_text: RichText,
    /// STLB-Bau key or Standardleistungsnummer of the text.
    catalog: CatalogKeys,
    /// Textergänzungen in the text (GAEB PerfDescr may carry them, e.g. a
    /// Standardbeschreibung from STLB-Bau or a former position).
    complements: Vec<TextComplement>,
    /// Herkunft of the text, for the planner only.
    text_origin: Option<TextOrigin>,
}

impl PerformanceDescription {
    /// `number` like "1" (GAEB allows 1–9999); an empty or taken number is
    /// replaced by the next free one when the row is inserted into an LV.
    pub fn new(number: impl Into<String>, label: impl Into<String>, long_text: RichText) -> Self {
        Self {
            id: RowId::UNSET,
            number: number.into().trim().to_string(),
            label: label.into(),
            short_text: String::new(),
            long_text,
            catalog: CatalogKeys::default(),
            complements: Vec::new(),
            text_origin: None,
        }
    }

    pub fn complements(&self) -> &[TextComplement] {
        &self.complements
    }

    pub fn set_complements(&mut self, complements: Vec<TextComplement>) {
        self.complements = complements;
    }

    /// Where the text came from, if known.
    pub fn text_origin(&self) -> Option<&TextOrigin> {
        self.text_origin.as_ref()
    }

    pub fn set_text_origin(&mut self, origin: Option<TextOrigin>) {
        self.text_origin = origin.filter(|origin| !origin.is_empty());
    }

    pub fn catalog(&self) -> &CatalogKeys {
        &self.catalog
    }

    pub fn set_catalog(&mut self, catalog: CatalogKeys) {
        self.catalog = catalog;
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    pub(crate) fn set_id(&mut self, id: RowId) {
        self.id = id;
    }

    /// Number as stored and written to GAEB (PerfNo without zeros: „1“); change
    /// it with [`crate::Lv::set_performance_number`]. Shown with
    /// [`Self::shown_number`].
    pub fn number(&self) -> &str {
        &self.number
    }

    /// The digits as the number field shows them: four digits,
    /// „1“ → „0001“. Everywhere else the number appears as [`Self::mark`].
    pub fn shown_number(&self) -> String {
        shown_performance_number(&self.number)
    }

    pub(crate) fn set_number(&mut self, number: impl Into<String>) {
        self.number = number.into().trim().to_string();
    }

    /// Bezeichnung (GAEB PerfLbl, up to 55 characters in GAEB).
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Kurztext (GAEB OutlineText), often empty; [`Self::title`] falls back
    /// to it when there is no label.
    pub fn short_text(&self) -> &str {
        &self.short_text
    }

    pub fn set_short_text(&mut self, text: impl Into<String>) {
        self.short_text = text.into();
    }

    pub fn long_text(&self) -> &RichText {
        &self.long_text
    }

    pub fn long_text_mut(&mut self) -> &mut RichText {
        &mut self.long_text
    }

    pub fn set_long_text(&mut self, text: RichText) {
        self.long_text = text;
    }

    /// Label, else short text, else the first line of the long text.
    pub fn title(&self) -> String {
        [self.label.trim(), self.short_text.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.long_text
                    .to_plain()
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
    }

    /// Mark „A0003“ (Anwender 18.11.): OZ column, badges at the
    /// positions, window titles, link field, print, MCP.
    pub fn mark(&self) -> String {
        performance_mark(&self.number)
    }

    /// "Ausführungsbeschreibung A0003 – Kabeltrassen" for lists and remarks.
    pub fn heading(&self) -> String {
        let title = self.title();
        let mark = self.mark();
        if title.is_empty() {
            format!("Ausführungsbeschreibung {mark}")
        } else {
            format!("Ausführungsbeschreibung {mark} – {title}")
        }
    }
}

/// „A0003“ for the number „3“ – the mark of an Ausführungsbeschreibung in
/// the LV, the printout and messages.
pub fn performance_mark(number: &str) -> String {
    format!("A{}", shown_performance_number(number))
}

/// Whether a badge or text is the mark of an Ausführungsbeschreibung
/// („A0003“).
pub fn is_performance_mark(text: &str) -> bool {
    text.strip_prefix('A').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// What a user or a KI types for a number: „7“, „0007“, „A0007“, „AB 0007“
/// (any case) – without the letters, trimmed.
pub fn performance_number_input(text: &str) -> &str {
    let text = text.trim();
    // `get`, not `[..2]`: the text may start with „€“ or „▓“ (Eingabe, MCP,
    // GAEB-Datei) – two bytes are then no character boundary.
    let rest = if text
        .get(..2)
        .is_some_and(|start| start.eq_ignore_ascii_case("AB"))
    {
        &text[2..]
    } else if text.starts_with(['A', 'a']) {
        &text[1..]
    } else {
        text
    };
    let rest = rest.trim();
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        text
    } else {
        rest
    }
}

/// A number of an Ausführungsbeschreibung as shown: four digits with leading
/// zeros („1“ → „0001“, „0012“ → „0012“); anything that is no
/// number from 0 to 9999 stays as it is (the export check reports it).
pub fn shown_performance_number(number: &str) -> String {
    let number = number.trim();
    match number.parse::<u32>() {
        Ok(value) if value <= 9999 && number.bytes().all(|byte| byte.is_ascii_digit()) => {
            format!("{value:04}")
        }
        _ => number.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::PerformanceDescription;
    use crate::{Lv, LvGroup, LvRow, OzMask, Position, RichText};

    fn lv() -> Lv {
        Lv::new("LV", OzMask::parse("11.PPPP").unwrap())
    }

    fn description(number: &str, label: &str) -> LvRow {
        LvRow::Performance(PerformanceDescription::new(
            number,
            label,
            RichText::plain("Kabeltrassen aus verzinktem Stahl"),
        ))
    }

    fn position(ordinal: &str) -> LvRow {
        LvRow::Position(Position::new(
            ordinal,
            "Trasse",
            Decimal::from(5),
            "m",
            None,
        ))
    }

    fn numbers(lv: &Lv) -> Vec<(String, String)> {
        lv.performances()
            .map(|performance| {
                (
                    performance.number().to_string(),
                    performance.label().to_string(),
                )
            })
            .collect()
    }

    /// Befund 7 (17.11.): numbered 1, 2, 3 … in LV order, kept that way on
    /// every insert, copy, move and delete; the positions keep their reference.
    #[test]
    fn numbers_count_up_by_place_and_follow_every_change() {
        let mut lv = lv();
        lv.insert_numbered(0, description("", "A")).unwrap();
        lv.insert_numbered(1, description("", "B")).unwrap();
        lv.insert_numbered(2, position("01.0010")).unwrap();
        assert!(lv.performances_in_order());
        let uses_b = lv.rows()[2].id();
        let b = lv.rows()[1].id();
        lv.link_performance(uses_b, Some(b)).unwrap();

        // New one in front: everything counts again by place.
        lv.insert_numbered(0, description("7", "Neu")).unwrap();
        assert_eq!(
            numbers(&lv),
            vec![
                ("1".into(), "Neu".into()),
                ("2".into(), "A".into()),
                ("3".into(), "B".into())
            ]
        );
        assert_eq!(
            lv.performance_of(lv.rows()[3].as_position().unwrap())
                .unwrap()
                .label(),
            "B",
            "Bezug bleibt"
        );

        // Copy, move and delete keep 1, 2, 3 …
        lv.duplicate_block(1).unwrap(); // copy of A right after A
        assert_eq!(
            numbers(&lv)
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "3", "4"]
        );
        lv.move_block(0, false).unwrap(); // „Neu“ one down
        assert_eq!(numbers(&lv)[0], ("1".into(), "A".into()));
        assert_eq!(numbers(&lv)[1], ("2".into(), "Neu".into()));
        lv.move_rows(0..1, lv.rows().len()).unwrap(); // A to the end
        assert_eq!(numbers(&lv).last().unwrap(), &("4".into(), "A".into()));
        lv.remove_row(0).unwrap();
        assert!(lv.performances_in_order());
        assert_eq!(lv.performance_count(), 3);
        let position = lv.positions().next().unwrap();
        assert_eq!(
            lv.performance_of(position).unwrap().label(),
            "B",
            "Bezug bleibt nach allem"
        );
    }

    #[test]
    fn imported_or_hand_numbers_stay_until_renumbered() {
        let mut lv = lv();
        // As read from GAEB (push_row keeps numbers): 10, 20.
        lv.push_row(description("10", "A"));
        lv.push_row(description("20", "B"));
        assert!(!lv.performances_in_order());
        lv.insert_numbered(0, description("", "Neu")).unwrap();
        assert_eq!(
            numbers(&lv)[0].0,
            "21",
            "nächste freie, die anderen bleiben"
        );
        assert_eq!(numbers(&lv)[1].0, "10");
        assert_eq!(lv.renumber_performances(), 3);
        assert!(lv.performances_in_order());
        assert_eq!(
            numbers(&lv)
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "3"]
        );
    }

    #[test]
    fn export_check_finds_missing_wrong_and_double_numbers() {
        let mut lv = lv();
        lv.push_row(description("1", "A"));
        assert!(lv.performance_number_problems().is_empty());
        lv.push_row(description("1", "B")); // push_row gives a free number
        assert!(lv.performance_number_problems().is_empty());
        let id = lv.rows()[1].id();
        if let Some(LvRow::Performance(performance)) = lv.row_mut(lv.index_of(id).unwrap()) {
            performance.set_number("1");
        }
        assert_eq!(lv.performance_number_problems().len(), 1, "doppelt");
        if let Some(LvRow::Performance(performance)) = lv.row_mut(1) {
            performance.set_number("");
        }
        assert!(lv.performance_number_problems()[0]
            .1
            .contains("keine Nummer"));
        if let Some(LvRow::Performance(performance)) = lv.row_mut(1) {
            performance.set_number("12000");
        }
        assert!(lv.performance_number_problems()[0].1.contains("1 bis 9999"));
        assert_eq!(lv.performances().next().unwrap().mark(), "A0001");
    }

    #[test]
    fn numbers_are_unique_and_found_without_leading_zeros() {
        let mut lv = lv();
        let first = lv.push_row(description("1", "Trassen"));
        let second = lv.push_row(description("1", "Doppelt"));
        let empty = lv.push_row(description("", "Ohne Nummer"));
        assert_eq!(lv.performance_by_id(second).unwrap().number(), "2");
        assert_eq!(lv.performance_by_id(empty).unwrap().number(), "3");
        assert_eq!(lv.performance_by_number("001").unwrap().id(), first);
        assert_eq!(lv.next_performance_number(), "4");
        assert!(lv.set_performance_number(second, "3").is_err());
        assert!(lv.set_performance_number(second, " ").is_err());
        lv.set_performance_number(second, "10").unwrap();
        assert_eq!(lv.performance_by_number("10").unwrap().id(), second);
        assert_eq!(lv.performance_count(), 3);
        assert_eq!(
            lv.performance_by_id(first).unwrap().heading(),
            "Ausführungsbeschreibung A0001 – Trassen"
        );
    }

    /// Anwender 18.11.: mark „A0001“, number field 0001, stored and written
    /// to GAEB without leading zeros (PerfNo „1“).
    #[test]
    fn numbers_are_shown_with_four_digits() {
        use super::shown_performance_number as shown;
        assert_eq!(shown("1"), "0001");
        assert_eq!(shown(" 12 "), "0012");
        assert_eq!(shown("0007"), "0007");
        assert_eq!(shown("9999"), "9999");
        assert_eq!(
            shown("12000"),
            "12000",
            "zu groß: bleibt, die Exportprüfung meldet es"
        );
        assert_eq!(shown("A1"), "A1");
        assert_eq!(shown(""), "");

        let mut lv = lv();
        lv.insert_numbered(0, description("", "Trassen")).unwrap();
        lv.insert_numbered(1, description("", "Kabel")).unwrap();
        let shown_numbers: Vec<String> = lv.performances().map(|p| p.shown_number()).collect();
        assert_eq!(
            shown_numbers,
            ["0001", "0002"],
            "zählt automatisch nach Lage"
        );
        let stored: Vec<&str> = lv.performances().map(|p| p.number()).collect();
        assert_eq!(
            stored,
            ["1", "2"],
            "gespeichert als GAEB PerfNo ohne führende Nullen"
        );
        let second = lv.rows()[1].id();
        assert_eq!(lv.performance_by_id(second).unwrap().mark(), "A0002");
        assert_eq!(
            lv.performance_by_id(second).unwrap().heading(),
            "Ausführungsbeschreibung A0002 – Kabel"
        );
        assert!(super::is_performance_mark("A0002"));
        assert!(
            !super::is_performance_mark("AB 2")
                && !super::is_performance_mark("A")
                && !super::is_performance_mark("Alt")
        );

        // Von Hand mit oder ohne Nullen und Kennbuchstaben: gespeichert wie
        // GAEB, gezeigt vierstellig.
        for (typed, stored) in [
            ("0005", "5"),
            ("A0006", "6"),
            ("AB 0007", "7"),
            ("ab7", "7"),
            ("a 0008", "8"),
            ("9", "9"),
        ] {
            lv.set_performance_number(second, typed).unwrap();
            assert_eq!(
                lv.performance_by_id(second).unwrap().number(),
                stored,
                "{typed}"
            );
        }
        lv.set_performance_number(second, "5").unwrap();
        assert_eq!(lv.performance_by_id(second).unwrap().shown_number(), "0005");
        assert_eq!(lv.performance_by_id(second).unwrap().mark(), "A0005");
        assert!(
            lv.set_performance_number(second, "001").is_err(),
            "0001 hat schon die erste"
        );
        assert!(
            lv.set_performance_number(second, "A0001").is_err(),
            "auch mit A"
        );
        assert_eq!(lv.performance_by_number("A0005").unwrap().id(), second);
        assert_eq!(lv.performance_by_number("AB 5").unwrap().id(), second);
        assert_eq!(lv.next_performance_number(), "6");

        // Read with leading zeros (another program): kept for GAEB, counts as in order.
        let mut read = self::lv();
        read.push_row(description("0001", "A"));
        read.push_row(description("0002", "B"));
        assert!(read.performances_in_order());
        assert_eq!(read.renumber_performances(), 0, "nichts zu ändern");
        assert_eq!(read.performances().next().unwrap().number(), "0001");
        assert_eq!(read.performances().next().unwrap().shown_number(), "0001");
    }

    #[test]
    fn positions_link_only_to_existing_descriptions() {
        let mut lv = lv();
        lv.push_group(LvGroup::new(0, "01", "Trassen"));
        let performance = lv.push_row(description("1", "Trassen"));
        let a = lv.push_row(position("01.0010"));
        let b = lv.push_row(position("01.0020"));
        lv.link_performance(a, Some(performance)).unwrap();
        lv.link_performance(b, Some(performance)).unwrap();
        assert_eq!(lv.positions_using(performance), vec![a, b]);
        assert_eq!(
            lv.performance_of(lv.position_by_id(a).unwrap())
                .unwrap()
                .number(),
            "1"
        );
        assert!(
            lv.link_performance(a, Some(a)).is_err(),
            "a position is no description"
        );
        assert!(
            lv.link_performance(performance, None).is_err(),
            "not a position"
        );
        lv.link_performance(b, None).unwrap();
        assert_eq!(lv.positions_using(performance), vec![a]);
    }

    #[test]
    fn removing_a_description_clears_its_references() {
        let mut lv = lv();
        lv.push_group(LvGroup::new(0, "01", "Trassen"));
        let performance = lv.push_row(description("1", "Trassen"));
        let a = lv.push_row(position("01.0010"));
        let b = lv.push_row(position("01.0020"));
        lv.link_performance(a, Some(performance)).unwrap();
        lv.link_performance(b, Some(performance)).unwrap();
        let (removed, cleared) = lv.remove_performance(performance).unwrap();
        assert_eq!(removed.number(), "1");
        assert_eq!(cleared, 2);
        assert!(lv
            .positions()
            .all(|position| position.performance().is_none()));
        assert!(
            lv.remove_performance(a).is_none(),
            "a position is no description"
        );

        // Removing the row or the whole title does the same.
        let performance = lv.insert_row(1, description("1", "Trassen"));
        lv.link_performance(a, Some(performance)).unwrap();
        lv.remove_row(lv.index_of(performance).unwrap());
        assert!(lv.position_by_id(a).unwrap().performance().is_none());
        let performance = lv.insert_row(1, description("1", "Trassen"));
        lv.link_performance(a, Some(performance)).unwrap();
        assert_eq!(lv.remove_block(0).len(), 4);
        assert_eq!(lv.unlink_missing_performances(), 0);
    }

    #[test]
    fn descriptions_have_no_number_in_the_outline_and_do_not_count() {
        let mut lv = lv();
        lv.push_group(LvGroup::new(0, "01", "Trassen"));
        lv.push_row(description("1", "Trassen"));
        lv.push_position(Position::new(
            "01.0010",
            "Trasse",
            Decimal::from(2),
            "m",
            Some(Decimal::from(10)),
        ));
        assert!(lv.is_auto_numbered());
        lv.renumber();
        assert_eq!(lv.rows()[1].ordinal(), "");
        assert!(!lv.rows()[1].has_ordinal());
        assert!(lv.duplicate_ordinals().is_empty());
        assert_eq!(lv.net_total().to_decimal(), Decimal::from(20));
        assert_eq!(
            lv.free_ordinal_at(2, &description("", "x")),
            Some(String::new())
        );
    }

    #[test]
    fn duplicated_title_gets_its_own_description() {
        let mut lv = lv();
        lv.push_group(LvGroup::new(0, "01", "Trassen"));
        let outside = lv.push_row(description("7", "Allgemein"));
        lv.push_group(LvGroup::new(0, "02", "Kanäle"));
        let inside = lv.push_row(description("1", "Kanäle"));
        let a = lv.push_row(position("02.0010"));
        let b = lv.push_row(position("02.0020"));
        lv.link_performance(a, Some(inside)).unwrap();
        lv.link_performance(b, Some(outside)).unwrap();
        let start = lv.duplicate_block(2).unwrap();
        let copy = lv.rows()[start + 1].as_performance().unwrap();
        assert_eq!(copy.number(), "8");
        let copy_id = copy.id();
        assert_eq!(
            lv.rows()[start + 2].as_position().unwrap().performance(),
            Some(copy_id)
        );
        assert_eq!(
            lv.rows()[start + 3].as_position().unwrap().performance(),
            Some(outside)
        );
        assert_eq!(lv.positions_using(inside), vec![a]);
    }
}

#[cfg(test)]
mod repetition_tests {
    use rust_decimal::Decimal;

    use crate::{Lv, LvGroup, LvRow, OzMask, Position, RowId};

    #[test]
    fn repetitions_point_to_other_positions_and_are_cleared_with_them() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        let base = lv.push_position(Position::new("01.0010", "Basis", Decimal::ONE, "St", None));
        let copy = lv.push_position(Position::new("01.0020", "wie 10", Decimal::ONE, "St", None));
        assert!(lv.link_repetition(copy, Some(copy)).is_err());
        assert!(lv.link_repetition(copy, Some(RowId(99))).is_err());
        lv.link_repetition(copy, Some(base)).unwrap();
        assert_eq!(lv.positions_repeating(base), vec![copy]);
        let start = lv.duplicate_block(0).unwrap();
        let rows = lv.rows();
        assert_eq!(
            rows[start + 2].as_position().unwrap().repeats(),
            Some(rows[start + 1].id()),
            "the copy repeats the copied base"
        );
        lv.remove_row(lv.index_of(base).unwrap());
        assert!(lv.position_by_id(copy).unwrap().repeats().is_none());
        assert!(matches!(lv.rows()[0], LvRow::Group(_)));
    }

    /// Gefunden mit dem Eigenschaftstest „kaputte GAEB-Dateien“ (18.11.,
    /// Saat 1): eine Ausführungsbeschreibung mit der Nummer „▓…“ (beschädigte
    /// D83) – `text[..2]` lag mitten im Zeichen, Absturz beim Einlesen. Ebenso
    /// „€“ als Eingabe im Verknüpfungsfeld.
    #[test]
    fn numbers_starting_with_wide_characters_do_not_panic() {
        for text in ["▓", "▓1", "€", "€7", "ÄB7", "A€", "😀"] {
            assert_eq!(
                crate::performance::performance_number_input(text),
                text,
                "{text}"
            );
        }
        assert_eq!(
            crate::performance::performance_number_input("ab 0007"),
            "0007"
        );
        assert_eq!(crate::performance::performance_number_input("A7"), "7");
    }
}
