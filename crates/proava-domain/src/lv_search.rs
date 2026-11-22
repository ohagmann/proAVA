//! Full-text search in an LV: „Wo stehen die Raumbediengeräte?“ – over
//! Kurztext, Langtext, Bieterangaben (Vortext, Eintrag, Nachtext),
//! Unterbeschreibungen, Hinweise, Ausführungsbeschreibungen and titles.
//! Like the library search: case and umlauts do not matter („fühler“ =
//! „fuehler“ = „FÜHLER“), every word must occur. Filters: Kostengruppe (also
//! inherited from the title, a prefix like „48“ is enough) and Einheit.
//!
//! The same search runs in the program (dialog „Übernehmen aus Projekt, GAEB
//! oder Bibliothek“) and in the MCP tool `lv_suchen`; reading files and
//! walking folders is the application's part (`proava_application::lv_search`).

use rust_decimal::Decimal;

use crate::library::fold_char;
use crate::lv::{Lv, LvRow, RowId};
use crate::position::TextComplement;

/// What to look for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LvQuery {
    /// Words that must all occur (in any of the texts of a row).
    pub text: String,
    /// Kostengruppe or its beginning („481“, „48“); also inherited.
    pub cost_group: String,
    /// Einheit, compared without case („st“ = „St“, „m2“ = „m²“).
    pub unit: String,
}

impl LvQuery {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Nothing to look for.
    pub fn is_empty(&self) -> bool {
        self.words().is_empty() && self.cost_group.trim().is_empty() && self.unit.trim().is_empty()
    }

    /// The search words, folded.
    pub fn words(&self) -> Vec<String> {
        self.text
            .split_whitespace()
            .map(crate::library::fold)
            .filter(|word| !word.is_empty())
            .collect()
    }

    /// A filter that only positions can satisfy.
    fn narrowed(&self) -> bool {
        !self.cost_group.trim().is_empty() || !self.unit.trim().is_empty()
    }
}

/// What kind of row was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    /// A title – taking it brings everything below.
    Title,
    Position,
    Note,
    Performance,
}

impl HitKind {
    pub fn label(self) -> &'static str {
        match self {
            HitKind::Title => "Titel",
            HitKind::Position => "Position",
            HitKind::Note => "Hinweis",
            HitKind::Performance => "Ausführungsbeschreibung",
        }
    }
}

/// In which text of the row the words stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitField {
    /// Kurztext (heading of a hint, title text); the excerpt then shows the
    /// beginning of the long text.
    ShortText,
    LongText,
    /// Bieterangabe or Angabe des Auftraggebers (Textergänzung).
    Complement,
    SubDescription,
}

impl HitField {
    pub fn label(self) -> &'static str {
        match self {
            HitField::ShortText => "Kurztext",
            HitField::LongText => "Langtext",
            HitField::Complement => "Bieterangabe",
            HitField::SubDescription => "Unterbeschreibung",
        }
    }
}

/// One row that matches.
#[derive(Debug, Clone, PartialEq)]
pub struct LvHit {
    /// Index in the LV's rows.
    pub index: usize,
    pub id: RowId,
    pub kind: HitKind,
    /// OZ (titles, positions), „A0001“ (Ausführungsbeschreibung), empty (hint).
    pub ordinal: String,
    pub short_text: String,
    /// Positions only.
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    /// Kostengruppe as it applies (own or from the title).
    pub cost_group: Option<String>,
    /// Titles above, outermost first: „01 Raumautomation“.
    pub path: Vec<String>,
    /// Where the words stand (the first one not in the Kurztext).
    pub field: HitField,
    /// A short piece of text around the find (one line, „…“ where cut);
    /// for Kurztext hits the beginning of the long text.
    pub excerpt: String,
    /// Titles: positions in the block (what taking the title brings).
    pub positions: usize,
}

impl LvHit {
    /// „01 Raumautomation › 01.02 Bediengeräte“.
    pub fn path_text(&self) -> String {
        self.path.join(" › ")
    }
}

/// Length of the excerpt in characters (about).
const EXCERPT_BEFORE: usize = 45;
const EXCERPT_AFTER: usize = 75;

/// All rows of `lv` that match `query`, in LV order. Titles count only by
/// their own text and only without KG/Einheit filter on the text alone;
/// with a KG or Einheit filter only positions match. An empty query finds
/// nothing.
pub fn search(lv: &Lv, query: &LvQuery) -> Vec<LvHit> {
    let words = query.words();
    if query.is_empty() {
        return Vec::new();
    }
    let cost_groups = lv.effective_cost_groups();
    let wanted_group = query.cost_group.trim();
    let wanted_unit = unit_key(&query.unit);
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut hits = Vec::new();
    for (index, row) in lv.rows().iter().enumerate() {
        if let LvRow::Group(group) = row {
            while path
                .last()
                .is_some_and(|(level, _)| *level >= group.level())
            {
                path.pop();
            }
        }
        let titles: Vec<String> = path.iter().map(|(_, text)| text.clone()).collect();
        let cost_group = cost_groups
            .get(index)
            .cloned()
            .flatten()
            .map(|(code, _)| code);
        let hit = match row {
            LvRow::Spacer(_) => None,
            LvRow::Group(group) => {
                let accepted = !words.is_empty()
                    && query.unit.trim().is_empty()
                    && (wanted_group.is_empty()
                        || group_matches(&cost_groups, index, wanted_group));
                accepted
                    .then(|| {
                        let fields = [(HitField::ShortText, group.short_text().to_string())];
                        find(&fields, &words)
                    })
                    .flatten()
                    .map(|(field, excerpt)| LvHit {
                        index,
                        id: row.id(),
                        kind: HitKind::Title,
                        ordinal: group.ordinal().to_string(),
                        short_text: group.short_text().to_string(),
                        quantity: None,
                        unit: String::new(),
                        unit_price: None,
                        cost_group: cost_group.clone(),
                        path: titles.clone(),
                        field,
                        excerpt,
                        positions: lv.rows()[lv.block_range(index)]
                            .iter()
                            .filter(|row| row.is_position())
                            .count(),
                    })
            }
            LvRow::Position(position) => {
                let accepted = (wanted_group.is_empty()
                    || group_matches(&cost_groups, index, wanted_group))
                    && (wanted_unit.is_empty() || unit_key(position.unit()) == wanted_unit);
                accepted
                    .then(|| {
                        let mut fields = vec![
                            (HitField::ShortText, position.short_text().to_string()),
                            (HitField::LongText, position.long_text().to_plain()),
                        ];
                        fields.extend(complement_fields(position.complements()));
                        for sub in position.sub_descriptions() {
                            fields.push((
                                HitField::SubDescription,
                                format!("{} {}", sub.short_text, sub.long_text.to_plain()),
                            ));
                        }
                        find(&fields, &words)
                    })
                    .flatten()
                    .map(|(field, excerpt)| LvHit {
                        index,
                        id: row.id(),
                        kind: HitKind::Position,
                        ordinal: position.ordinal().to_string(),
                        short_text: position.short_text().to_string(),
                        quantity: Some(position.quantity()),
                        unit: position.unit().to_string(),
                        unit_price: position.unit_price(),
                        cost_group: cost_group.clone(),
                        path: titles.clone(),
                        field,
                        excerpt,
                        positions: 0,
                    })
            }
            LvRow::Note(note) => (!query.narrowed())
                .then(|| {
                    let mut fields = vec![
                        (HitField::ShortText, note.heading().to_string()),
                        (HitField::LongText, note.text().to_plain()),
                    ];
                    fields.extend(complement_fields(note.complements()));
                    find(&fields, &words)
                })
                .flatten()
                .map(|(field, excerpt)| LvHit {
                    index,
                    id: row.id(),
                    kind: HitKind::Note,
                    ordinal: String::new(),
                    short_text: note.title(),
                    quantity: None,
                    unit: String::new(),
                    unit_price: None,
                    cost_group: cost_group.clone(),
                    path: titles.clone(),
                    field,
                    excerpt,
                    positions: 0,
                }),
            LvRow::Performance(performance) => (!query.narrowed())
                .then(|| {
                    let fields = [
                        (
                            HitField::ShortText,
                            format!("{} {}", performance.label(), performance.short_text()),
                        ),
                        (HitField::LongText, performance.long_text().to_plain()),
                    ];
                    find(&fields, &words)
                })
                .flatten()
                .map(|(field, excerpt)| LvHit {
                    index,
                    id: row.id(),
                    kind: HitKind::Performance,
                    ordinal: performance.mark(),
                    short_text: performance.title(),
                    quantity: None,
                    unit: String::new(),
                    unit_price: None,
                    cost_group: cost_group.clone(),
                    path: titles.clone(),
                    field,
                    excerpt,
                    positions: 0,
                }),
        };
        hits.extend(hit);
        if let LvRow::Group(group) = row {
            path.push((
                group.level(),
                format!("{} {}", group.ordinal(), group.short_text())
                    .trim()
                    .to_string(),
            ));
        }
    }
    hits
}

/// Rows to show for a search in an outline: the rows `hits` (indices) and
/// the titles above them, in LV order (like the library search).
pub fn rows_with_titles(lv: &Lv, hits: impl IntoIterator<Item = usize>) -> Vec<usize> {
    let mut keep = vec![false; lv.rows().len()];
    for hit in hits {
        let Some(slot) = keep.get_mut(hit) else {
            continue;
        };
        *slot = true;
        let mut limit = match &lv.rows()[hit] {
            LvRow::Group(group) => group.level(),
            _ => usize::MAX,
        };
        for above in (0..hit).rev() {
            if let LvRow::Group(group) = &lv.rows()[above] {
                if group.level() < limit {
                    keep[above] = true;
                    limit = group.level();
                }
            }
        }
    }
    (0..keep.len()).filter(|&index| keep[index]).collect()
}

fn group_matches(cost_groups: &[Option<(String, bool)>], index: usize, wanted: &str) -> bool {
    cost_groups
        .get(index)
        .and_then(Option::as_ref)
        .is_some_and(|(code, _)| code.starts_with(wanted))
}

/// „St“, „st “ → „st“; „m²“ → „m2“.
fn unit_key(unit: &str) -> String {
    crate::library::fold(unit.trim())
        .replace('²', "2")
        .replace('³', "3")
        .replace(' ', "")
}

fn complement_fields(complements: &[TextComplement]) -> Vec<(HitField, String)> {
    complements
        .iter()
        .map(|complement| {
            (
                HitField::Complement,
                [
                    complement.caption.trim(),
                    complement.body.trim(),
                    complement.tail.trim(),
                ]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            )
        })
        .collect()
}

/// Whether every word occurs in one of `fields`; then the field of the
/// first word that is not in the Kurztext and the excerpt there.
fn find(fields: &[(HitField, String)], words: &[String]) -> Option<(HitField, String)> {
    let folded: Vec<Folded> = fields.iter().map(|(_, text)| Folded::new(text)).collect();
    if !words.iter().all(|word| {
        folded
            .iter()
            .any(|text| text.folded.contains(word.as_str()))
    }) {
        return None;
    }
    for word in words {
        if folded
            .first()
            .is_some_and(|short| short.folded.contains(word.as_str()))
            && fields
                .first()
                .is_some_and(|(field, _)| *field == HitField::ShortText)
        {
            continue;
        }
        for (place, text) in folded.iter().enumerate() {
            if let Some(at) = text.folded.find(word.as_str()) {
                return Some((fields[place].0, text.excerpt(at, word.len())));
            }
        }
    }
    // Everything in the Kurztext (or only filters): the long text's beginning.
    let beginning = fields
        .iter()
        .zip(&folded)
        .find(|((field, text), _)| *field == HitField::LongText && !text.trim().is_empty())
        .map(|(_, text)| text.excerpt(0, 0))
        .unwrap_or_default();
    Some((HitField::ShortText, beginning))
}

/// A text, its folded form and where each character went.
struct Folded {
    chars: Vec<char>,
    folded: String,
    /// Byte offset in `folded` of every character (and the end).
    starts: Vec<usize>,
}

impl Folded {
    fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut folded = String::with_capacity(text.len());
        let mut starts = Vec::with_capacity(chars.len() + 1);
        for &character in &chars {
            starts.push(folded.len());
            fold_char(character, &mut folded);
        }
        starts.push(folded.len());
        Self {
            chars,
            folded,
            starts,
        }
    }

    /// Character index of the folded byte `at`.
    fn char_at(&self, at: usize) -> usize {
        self.starts
            .partition_point(|&start| start <= at)
            .saturating_sub(1)
    }

    /// One line around the find at folded byte `at` with folded length `len`,
    /// cut at word boundaries, „…“ where cut.
    fn excerpt(&self, at: usize, len: usize) -> String {
        let first = self.char_at(at);
        let last = self.char_at((at + len).saturating_sub(1)).max(first);
        let mut start = first.saturating_sub(EXCERPT_BEFORE);
        if start > 0 {
            // Begin after a space, not in the middle of a word.
            start = (start..first)
                .find(|&index| self.chars[index].is_whitespace())
                .map(|index| index + 1)
                .unwrap_or(start);
        }
        let mut end = (last + 1 + EXCERPT_AFTER).min(self.chars.len());
        if end < self.chars.len() {
            end = (last + 1..end)
                .rev()
                .find(|&index| self.chars[index].is_whitespace())
                .unwrap_or(end);
        }
        let piece: String = self.chars[start..end].iter().collect();
        let piece = piece.split_whitespace().collect::<Vec<_>>().join(" ");
        if piece.is_empty() {
            return String::new();
        }
        format!(
            "{}{piece}{}",
            if start > 0 { "…" } else { "" },
            if end < self.chars.len() { "…" } else { "" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::{ComplementKind, Position, SubDescription};
    use crate::{LvGroup, Note, OzMask, PerformanceDescription, RichText};

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
        let mut room = LvGroup::new(0, "", "Raumautomation");
        room.set_cost_group(Some("481".into()));
        lv.push_group(room);
        lv.push_group(LvGroup::new(1, "", "Raumbediengeräte"));
        let mut knx = Position::new(
            "",
            "Raumbediengerät KNX",
            Decimal::from(12),
            "St",
            Some(Decimal::from(180)),
        );
        knx.set_long_text(RichText::plain(
            "Liefern und montieren: Taster mit Display für KNX, Farbe reinweiß, \
             Unterputzmontage in Schalterdose, Temperaturfühler eingebaut.",
        ));
        knx.set_complements(vec![TextComplement {
            marker: "01".into(),
            kind: ComplementKind::Bidder,
            caption: "Fabrikat:".into(),
            body: String::new(),
            tail: String::new(),
            anchor: None,
        }]);
        lv.push_position(knx);
        let mut cable = Position::new("", "Busleitung", Decimal::from(300), "m", None);
        cable.set_cost_group(Some("484".into()));
        cable.set_long_text(RichText::plain("J-Y(St)Y 2x2x0,8 für den Bus"));
        cable.set_sub_descriptions(vec![SubDescription {
            number: "1".into(),
            short_text: "Verlegung".into(),
            long_text: RichText::plain("auf Kabelrinne über abgehängter Decke"),
            quantity: None,
            unit: String::new(),
            catalog: Default::default(),
        }]);
        lv.push_position(cable);
        let mut note = Note::new("Alle Geräte in Farbe reinweiß wie Schalterprogramm.");
        note.set_heading("Farben");
        lv.push_note(note);
        lv.push_group(LvGroup::new(0, "", "Feldgeräte"));
        lv.push_row(LvRow::Performance(PerformanceDescription::new(
            "1",
            "Fühler allgemein",
            RichText::plain("Alle Fühler mit Messumformer 0-10 V"),
        )));
        let mut probe = Position::new("", "Außenfühler", Decimal::ONE, "St", None);
        probe.set_long_text(RichText::plain("Witterungsgeschützt, mit Strahlungsschutz"));
        lv.push_position(probe);
        lv.renumber();
        lv
    }

    fn short(hits: &[LvHit]) -> Vec<String> {
        hits.iter().map(|hit| hit.short_text.clone()).collect()
    }

    #[test]
    fn words_umlauts_and_case_do_not_matter() {
        let lv = lv();
        assert_eq!(
            search(&lv, &LvQuery::text("RAUMBEDIENGERAET")).len(),
            2,
            "ä = ae, title and position"
        );
        assert!(
            search(&lv, &LvQuery::text("raumbediengerat")).is_empty(),
            "ä is ae, not a"
        );
        let hits = search(&lv, &LvQuery::text("raumbediengeraete"));
        assert_eq!(short(&hits), ["Raumbediengeräte"], "title");
        assert_eq!(hits[0].kind, HitKind::Title);
        assert_eq!(hits[0].positions, 2);
        assert_eq!(hits[0].path, ["01 Raumautomation"]);
        let hits = search(&lv, &LvQuery::text("raumbediengerät knx"));
        assert_eq!(short(&hits), ["Raumbediengerät KNX"]);
        let hit = &hits[0];
        assert_eq!(hit.kind, HitKind::Position);
        assert_eq!(hit.ordinal, "01.01.0010");
        assert_eq!(hit.quantity, Some(Decimal::from(12)));
        assert_eq!(hit.unit, "St");
        assert_eq!(hit.unit_price, Some(Decimal::from(180)));
        assert_eq!(hit.cost_group.as_deref(), Some("481"), "inherited");
        assert_eq!(
            hit.path_text(),
            "01 Raumautomation › 01.01 Raumbediengeräte"
        );
        assert_eq!(hit.field, HitField::ShortText);
        assert!(
            hit.excerpt.starts_with("Liefern und montieren: Taster") && hit.excerpt.ends_with('…'),
            "beginning of the long text: {}",
            hit.excerpt
        );
    }

    #[test]
    fn long_text_hits_show_the_place() {
        let lv = lv();
        let hits = search(&lv, &LvQuery::text("display REINWEISS"));
        assert_eq!(short(&hits), ["Raumbediengerät KNX"], "ß = ss, both words");
        assert_eq!(hits[0].field, HitField::LongText);
        assert!(
            hits[0].excerpt.contains("Taster mit Display für KNX"),
            "{}",
            hits[0].excerpt
        );
        let hits = search(&lv, &LvQuery::text("reinweiß"));
        assert_eq!(
            short(&hits),
            ["Raumbediengerät KNX", "Farben"],
            "hint found too"
        );
        assert_eq!(hits[1].kind, HitKind::Note);
        assert_eq!(hits[1].field, HitField::LongText);
        // Words in different texts of the same row still match.
        let hits = search(&lv, &LvQuery::text("knx temperaturfühler"));
        assert_eq!(short(&hits), ["Raumbediengerät KNX"]);
        assert!(
            hits[0].excerpt.contains("Temperaturfühler"),
            "{}",
            hits[0].excerpt
        );
        assert!(search(&lv, &LvQuery::text("knx strahlungsschutz")).is_empty());
    }

    #[test]
    fn bidder_fields_sub_descriptions_and_performance_descriptions() {
        let lv = lv();
        let hits = search(&lv, &LvQuery::text("fabrikat"));
        assert_eq!(short(&hits), ["Raumbediengerät KNX"]);
        assert_eq!(hits[0].field, HitField::Complement);
        let hits = search(&lv, &LvQuery::text("kabelrinne"));
        assert_eq!(short(&hits), ["Busleitung"]);
        assert_eq!(hits[0].field, HitField::SubDescription);
        let hits = search(&lv, &LvQuery::text("messumformer"));
        assert_eq!(hits[0].kind, HitKind::Performance);
        assert_eq!(hits[0].ordinal, "A0001");
        assert_eq!(hits[0].path, ["02 Feldgeräte"]);
    }

    #[test]
    fn cost_group_and_unit_filter_positions() {
        let lv = lv();
        let by_group = LvQuery {
            cost_group: "48".into(),
            ..LvQuery::default()
        };
        assert_eq!(
            short(&search(&lv, &by_group)),
            ["Raumbediengerät KNX", "Busleitung"],
            "only positions; 484 own, 481 inherited"
        );
        let by_group = LvQuery {
            cost_group: "484".into(),
            ..LvQuery::default()
        };
        assert_eq!(short(&search(&lv, &by_group)), ["Busleitung"]);
        let by_unit = LvQuery {
            unit: " st ".into(),
            ..LvQuery::default()
        };
        assert_eq!(
            short(&search(&lv, &by_unit)),
            ["Raumbediengerät KNX", "Außenfühler"]
        );
        let both = LvQuery {
            text: "fühler".into(),
            cost_group: "481".into(),
            unit: String::new(),
        };
        assert_eq!(
            short(&search(&lv, &both)),
            ["Raumbediengerät KNX"],
            "Außenfühler has no KG, the AB is no position"
        );
        assert!(search(&lv, &LvQuery::default()).is_empty());
        assert!(search(&lv, &LvQuery::text("   ")).is_empty());
    }

    #[test]
    fn outline_rows_keep_the_titles_above() {
        let lv = lv();
        let hits = search(&lv, &LvQuery::text("witterungsgeschützt"));
        let rows = rows_with_titles(&lv, hits.iter().map(|hit| hit.index));
        let texts: Vec<String> = rows
            .iter()
            .map(|&row| lv.rows()[row].short_text())
            .collect();
        assert_eq!(texts, ["Feldgeräte", "Außenfühler"]);
    }

    #[test]
    fn excerpts_are_cut_at_words() {
        let text = format!("{} Treffer {}", "Anfang ".repeat(20), "Ende ".repeat(30));
        let folded = Folded::new(&text);
        let at = folded.folded.find("treffer").unwrap();
        let piece = folded.excerpt(at, "treffer".len());
        assert!(piece.starts_with("…Anfang"), "{piece}");
        assert!(piece.ends_with("Ende…"), "{piece}");
        assert!(piece.contains("Treffer"));
        assert!(piece.chars().count() < 140, "{piece}");
        // Umlauts before the find keep the mapping right.
        let folded = Folded::new("Größe über Außenfühler");
        let at = folded.folded.find("aussen").unwrap();
        assert_eq!(folded.excerpt(at, 6), "Größe über Außenfühler");
    }
}
