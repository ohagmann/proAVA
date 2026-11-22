//! Positionsbibliothek: a library is an ordinary project whose LV collects
//! titles („Ordner“) and positions to reuse. This module holds what a
//! library entry carries besides an LV position (Stichwörter, Stand, date of
//! the Erfahrungspreis), where a position in an LV came from (for „Mit
//! Bibliothek abgleichen“), and the full-text search with filters.

use std::collections::HashSet;

use crate::lv::{Lv, LvRow, RowId};
use crate::position::{Position, PositionKind};

/// What a library entry knows about itself (kept on the position).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntryInfo {
    /// Stichwörter for the search, e.g. „KNX“, „Raumbedienung“.
    pub keywords: Vec<String>,
    /// Stand of the entry (TT.MM.JJJJ), set when it is stored or changed.
    pub as_of: String,
    /// Date of the unit price (Erfahrungspreis), TT.MM.JJJJ or „03/2026“.
    pub price_date: String,
}

impl EntryInfo {
    pub fn is_empty(&self) -> bool {
        self.keywords.is_empty()
            && self.as_of.trim().is_empty()
            && self.price_date.trim().is_empty()
    }

    /// „KNX, Raumbedienung“
    pub fn keywords_text(&self) -> String {
        self.keywords.join(", ")
    }

    /// Splits typed keywords at commas and semicolons, without doubles.
    pub fn parse_keywords(text: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        text.split([',', ';'])
            .map(|word| word.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|word| !word.is_empty() && seen.insert(word.to_lowercase()))
            .collect()
    }
}

/// Where a position of an LV came from: library and entry, and the Stand of
/// the entry when it was taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryOrigin {
    /// Name of the library (its project name, e.g. „proAVA-Bibliothek“).
    pub library: String,
    /// Id of the entry in the library's LV.
    pub entry: RowId,
    /// Stand of the entry when it was taken (may be empty).
    pub as_of: String,
}

/// Which positions the search shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KindFilter {
    #[default]
    All,
    Normal,
    /// Bedarfspositionen (mit und ohne GB).
    Provisional,
    /// Grund- und Wahlpositionen.
    Alternative,
    Supplement,
    LumpSum,
    Hourly,
}

impl KindFilter {
    pub const ALL: [KindFilter; 7] = [
        KindFilter::All,
        KindFilter::Normal,
        KindFilter::Provisional,
        KindFilter::Alternative,
        KindFilter::Supplement,
        KindFilter::LumpSum,
        KindFilter::Hourly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            KindFilter::All => "alle Arten",
            KindFilter::Normal => "Normalpositionen",
            KindFilter::Provisional => "Bedarfspositionen",
            KindFilter::Alternative => "Grund-/Wahlpositionen",
            KindFilter::Supplement => "Zulagen",
            KindFilter::LumpSum => "Pauschal",
            KindFilter::Hourly => "Stundenlohn",
        }
    }

    pub fn accepts(self, position: &Position) -> bool {
        match self {
            KindFilter::All => true,
            KindFilter::Normal => {
                position.kind() == PositionKind::Normal
                    && !position.is_lump_sum()
                    && !position.is_hourly()
            }
            KindFilter::Provisional => matches!(position.kind(), PositionKind::Provisional { .. }),
            KindFilter::Alternative => {
                matches!(
                    position.kind(),
                    PositionKind::Base { .. } | PositionKind::Alternative { .. }
                )
            }
            KindFilter::Supplement => position.kind() == PositionKind::Supplement,
            KindFilter::LumpSum => position.is_lump_sum(),
            KindFilter::Hourly => position.is_hourly(),
        }
    }
}

/// What to look for in a library.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchFilter {
    /// Words that must all appear in OZ, Kurztext, Langtext or Stichwörtern.
    pub text: String,
    /// Kostengruppe (also inherited from the title), prefix: „48“ finds 481–489.
    pub cost_group: String,
    pub kind: KindFilter,
    /// One Stichwort the entry must carry (exact, case-insensitive).
    pub keyword: String,
}

impl SearchFilter {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
            && self.cost_group.trim().is_empty()
            && self.kind == KindFilter::All
            && self.keyword.trim().is_empty()
    }
}

/// Lower case, umlauts and ß folded („Fühler“ = „fuehler“ = „FUHLER“ is not
/// needed – „fühler“ and „fuehler“ both match), whitespace collapsed.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        fold_char(character, &mut out);
    }
    out
}

/// One character of [`fold`] (the LV search maps hits back to the text with it).
pub(crate) fn fold_char(character: char, out: &mut String) {
    for character in character.to_lowercase() {
        match character {
            'ä' => out.push_str("ae"),
            'ö' => out.push_str("oe"),
            'ü' => out.push_str("ue"),
            'ß' => out.push_str("ss"),
            '\u{00ad}' => {}
            other => out.push(other),
        }
    }
}

/// The searchable text of every row of a library LV (built once per load).
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    /// Folded text per row (same order as the LV).
    texts: Vec<String>,
    /// Effective cost group per row.
    cost_groups: Vec<Option<String>>,
}

impl SearchIndex {
    pub fn build(lv: &Lv) -> Self {
        let cost_groups = lv
            .effective_cost_groups()
            .into_iter()
            .map(|group| group.map(|(code, _)| code))
            .collect();
        let texts = lv
            .rows()
            .iter()
            .map(|row| {
                let mut text = format!("{} {}", row.ordinal(), row.short_text());
                match row {
                    LvRow::Position(position) => {
                        text.push(' ');
                        text.push_str(&position.long_text().to_plain());
                        for sub in position.sub_descriptions() {
                            text.push(' ');
                            text.push_str(&sub.short_text);
                            text.push(' ');
                            text.push_str(&sub.long_text.to_plain());
                        }
                        for keyword in &position.library_info().keywords {
                            text.push(' ');
                            text.push_str(keyword);
                        }
                    }
                    LvRow::Note(note) => {
                        text.push(' ');
                        text.push_str(&note.text().to_plain());
                    }
                    LvRow::Performance(performance) => {
                        text.push(' ');
                        text.push_str(&performance.long_text().to_plain());
                    }
                    LvRow::Group(_) | LvRow::Spacer(_) => {}
                }
                fold(&text)
            })
            .collect();
        Self { texts, cost_groups }
    }

    /// Whether row `index` itself matches (titles only by their text).
    pub fn matches(&self, lv: &Lv, index: usize, filter: &SearchFilter) -> bool {
        let Some(row) = lv.rows().get(index) else {
            return false;
        };
        let words: Vec<String> = filter.text.split_whitespace().map(fold).collect();
        let text = self
            .texts
            .get(index)
            .map(String::as_str)
            .unwrap_or_default();
        if !words.iter().all(|word| text.contains(word.as_str())) {
            return false;
        }
        let narrowed = filter.kind != KindFilter::All
            || !filter.cost_group.trim().is_empty()
            || !filter.keyword.trim().is_empty();
        let Some(position) = row.as_position() else {
            // With a filter on positions, titles and hints only come along
            // as the path above a hit.
            return !narrowed && !words.is_empty();
        };
        if !filter.kind.accepts(position) {
            return false;
        }
        let group = filter.cost_group.trim();
        if !group.is_empty()
            && !self
                .cost_groups
                .get(index)
                .and_then(Option::as_deref)
                .is_some_and(|code| code.starts_with(group))
        {
            return false;
        }
        let keyword = fold(filter.keyword.trim());
        keyword.is_empty()
            || position
                .library_info()
                .keywords
                .iter()
                .any(|word| fold(word) == keyword)
    }

    /// Rows to show for `filter` with their depth: the hits and the titles
    /// above them, in LV order. An empty filter shows nothing (the tree
    /// is shown instead).
    pub fn hits(&self, lv: &Lv, filter: &SearchFilter) -> Vec<usize> {
        if filter.is_empty() {
            return Vec::new();
        }
        let mut keep = vec![false; lv.rows().len()];
        for index in 0..lv.rows().len() {
            if !self.matches(lv, index, filter) {
                continue;
            }
            keep[index] = true;
            let mut limit = match &lv.rows()[index] {
                LvRow::Group(group) => group.level(),
                _ => usize::MAX,
            };
            for above in (0..index).rev() {
                if let LvRow::Group(group) = &lv.rows()[above] {
                    if group.level() < limit {
                        keep[above] = true;
                        limit = group.level();
                    }
                }
            }
        }
        (0..lv.rows().len()).filter(|&index| keep[index]).collect()
    }

    /// Positions among the hits (for the count).
    pub fn position_hits(&self, lv: &Lv, filter: &SearchFilter) -> usize {
        self.hits(lv, filter)
            .into_iter()
            .filter(|&index| lv.rows()[index].is_position() && self.matches(lv, index, filter))
            .count()
    }
}

/// All Stichwörter of a library, sorted, without doubles.
pub fn keywords(lv: &Lv) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut words: Vec<String> = lv
        .positions()
        .flat_map(|position| position.library_info().keywords.iter().cloned())
        .filter(|word| seen.insert(fold(word)))
        .collect();
    words.sort_by_key(|word| fold(word));
    words
}

/// Depth of every row for an indented tree: titles their level, the rest
/// one below the title above.
pub fn depths(lv: &Lv) -> Vec<usize> {
    let mut current = 0;
    lv.rows()
        .iter()
        .map(|row| match row {
            LvRow::Group(group) => {
                current = group.level() + 1;
                group.level()
            }
            _ => current,
        })
        .collect()
}

/// Rows taken from library `library` (still with the ids they have there):
/// every position remembers its entry and the entry's Stand.
pub fn mark_origin(rows: &mut [LvRow], library: &str) {
    for row in rows {
        let entry = row.id();
        if let LvRow::Position(position) = row {
            let as_of = position.library_info().as_of.clone();
            position.set_origin(Some(LibraryOrigin {
                library: library.to_string(),
                entry,
                as_of,
            }));
        }
    }
}

/// Rows going into a library: they are entries now, not copies of one; the
/// Stand is `today` (TT.MM.JJJJ).
pub fn prepare_for_library(rows: &mut [LvRow], today: &str) {
    for row in rows {
        if let LvRow::Position(position) = row {
            position.set_origin(None);
            let mut info = position.library_info().clone();
            info.as_of = today.to_string();
            position.set_library_info(info);
        }
    }
}

/// A position of the LV and the library entry it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryLink {
    /// Position in the LV.
    pub position: RowId,
    /// Index of the library in the list handed in.
    pub library: usize,
    pub entry: RowId,
}

/// Result of looking up the origins of an LV.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Links {
    pub found: Vec<LibraryLink>,
    /// OZ of positions whose library or entry is gone.
    pub missing: Vec<String>,
}

/// Finds the library entry of every position that has an origin;
/// `libraries` are (name, LV) of the libraries at hand.
pub fn links(lv: &Lv, libraries: &[(&str, &Lv)]) -> Links {
    let mut links = Links::default();
    for position in lv.positions() {
        let Some(origin) = position.origin() else {
            continue;
        };
        let found = libraries
            .iter()
            .enumerate()
            .find_map(|(index, (name, library))| {
                (*name == origin.library && library.position_by_id(origin.entry).is_some())
                    .then_some(index)
            });
        match found {
            Some(library) => links.found.push(LibraryLink {
                position: position.id(),
                library,
                entry: origin.entry,
            }),
            None => links.missing.push(position.ordinal().to_string()),
        }
    }
    links
}

/// The content of a library entry put on an LV position: texts, unit,
/// kind, flags, complements, STLB keys and Unterbeschreibungen; the
/// position keeps OZ, quantity, status and its price (unless `price`).
pub fn take_entry_content(position: &mut Position, entry: &Position, price: bool) {
    let mut fresh = entry.clone();
    fresh.set_id(position.id());
    fresh.set_ordinal(position.ordinal().to_string());
    fresh.set_quantity(position.quantity());
    fresh.set_quantity_formula(position.quantity_formula().map(str::to_string));
    fresh.set_quantity_open(position.is_quantity_open());
    fresh.set_status(position.status());
    fresh.set_performance(position.performance());
    fresh.set_repeats(position.repeats());
    fresh.set_cost_group(position.cost_group().map(str::to_string));
    if !price {
        fresh.set_unit_price(position.unit_price());
        fresh.set_price_components(position.price_components().to_vec());
    }
    let origin = position.origin().cloned().map(|mut origin| {
        origin.as_of = entry.library_info().as_of.clone();
        origin
    });
    fresh.set_origin(origin);
    *position = fresh;
}

/// The LV as the libraries have it: every linked position carries the
/// content of its entry (see [`take_entry_content`], prices only with
/// `prices`). Compared with the LV (`lv_diff::compare(lv, &state)`) this
/// shows what „Aus Bibliothek aktualisieren“ would change.
pub fn library_state(lv: &Lv, libraries: &[(&str, &Lv)], links: &Links, prices: bool) -> Lv {
    let mut state = lv.clone();
    for link in &links.found {
        let Some((_, library)) = libraries.get(link.library) else {
            continue;
        };
        let (Some(entry), Some(index)) = (
            library.position_by_id(link.entry),
            state.index_of(link.position),
        ) else {
            continue;
        };
        if let Some(LvRow::Position(position)) = state.row_mut(index) {
            take_entry_content(position, entry, prices);
        }
    }
    state
}

/// Takes the library content into the positions `chosen` of the active LV
/// of `project` (images of the library come along). Finished positions are
/// left out. Returns (updated, skipped as finished).
pub fn refresh_from_library(
    project: &mut crate::Project,
    libraries: &[(&str, &crate::Project)],
    links: &Links,
    chosen: &[RowId],
    prices: bool,
) -> (usize, Vec<String>) {
    let mut updated = 0;
    let mut finished = Vec::new();
    for link in links
        .found
        .iter()
        .filter(|link| chosen.contains(&link.position))
    {
        let Some((_, library)) = libraries.get(link.library) else {
            continue;
        };
        let Some(entry) = library.lv().position_by_id(link.entry).cloned() else {
            continue;
        };
        let Some(index) = project.lv().index_of(link.position) else {
            continue;
        };
        if let Some(LvRow::Position(position)) = project.lv().rows().get(index) {
            if position.is_locked() {
                finished.push(position.ordinal().to_string());
                continue;
            }
        }
        let used: HashSet<String> = crate::transfer::texts(&LvRow::Position(entry.clone()))
            .into_iter()
            .flat_map(|text| text.images().map(str::to_string).collect::<Vec<_>>())
            .collect();
        let names = crate::transfer::bring_images(project, library, &used);
        let mut entry = entry;
        if !names.is_empty() {
            let mut text = entry.long_text().clone();
            crate::transfer::rename_images(&mut text, &names);
            entry.set_long_text(text);
            for sub in entry.sub_descriptions_mut() {
                crate::transfer::rename_images(&mut sub.long_text, &names);
            }
        }
        if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
            take_entry_content(position, &entry, prices);
            updated += 1;
        }
    }
    (updated, finished)
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::{LvGroup, OzMask, RichText};

    fn library() -> Lv {
        let mut lv = Lv::new("Bibliothek", OzMask::parse("11.222.33.44.PPPP").unwrap());
        let mut room = LvGroup::new(0, "", "Raumautomation");
        room.set_cost_group(Some("481".into()));
        lv.push_group(room);
        lv.push_group(LvGroup::new(1, "", "Bediengeräte"));
        let mut knx = Position::new(
            "",
            "Raumbediengerät",
            Decimal::ONE,
            "St",
            Some(Decimal::from(180)),
        );
        knx.set_long_text(RichText::plain(
            "Taster mit Display für KNX, Farbe reinweiß",
        ));
        knx.set_library_info(EntryInfo {
            keywords: vec!["KNX".into(), "Raumbedienung".into()],
            as_of: "01.11.2026".into(),
            price_date: "10/2026".into(),
        });
        lv.push_position(knx);
        let mut sensor = Position::new("", "Präsenzmelder", Decimal::ONE, "St", None);
        sensor.set_kind(PositionKind::Provisional { with_total: false });
        lv.push_position(sensor);
        lv.push_group(LvGroup::new(0, "", "Feldgeräte"));
        let mut probe = Position::new("", "Fühler Außentemperatur", Decimal::ONE, "St", None);
        probe.set_cost_group(Some("482".into()));
        probe.set_lump_sum(true);
        lv.push_position(probe);
        lv.renumber();
        lv
    }

    fn texts(lv: &Lv, rows: &[usize]) -> Vec<String> {
        rows.iter()
            .map(|&index| lv.rows()[index].short_text())
            .collect()
    }

    #[test]
    fn search_finds_long_text_and_keywords_with_their_titles() {
        let lv = library();
        let index = SearchIndex::build(&lv);
        let filter = |text: &str| SearchFilter {
            text: text.into(),
            ..SearchFilter::default()
        };
        assert_eq!(
            texts(&lv, &index.hits(&lv, &filter("reinweiss display"))),
            ["Raumautomation", "Bediengeräte", "Raumbediengerät"],
            "Langtext, ß/ss, mehrere Wörter"
        );
        assert_eq!(
            texts(&lv, &index.hits(&lv, &filter("raumbedienung"))).len(),
            3,
            "Stichwort"
        );
        assert_eq!(
            texts(&lv, &index.hits(&lv, &filter("fuehler"))),
            ["Feldgeräte", "Fühler Außentemperatur"]
        );
        assert_eq!(
            texts(&lv, &index.hits(&lv, &filter("feldgeräte"))),
            ["Feldgeräte"],
            "Titel"
        );
        assert!(index.hits(&lv, &SearchFilter::default()).is_empty());
        assert_eq!(index.position_hits(&lv, &filter("raum")), 1);
    }

    #[test]
    fn filters_by_cost_group_kind_and_keyword() {
        let lv = library();
        let index = SearchIndex::build(&lv);
        let by_group = SearchFilter {
            cost_group: "481".into(),
            ..SearchFilter::default()
        };
        assert_eq!(
            texts(&lv, &index.hits(&lv, &by_group)),
            [
                "Raumautomation",
                "Bediengeräte",
                "Raumbediengerät",
                "Präsenzmelder"
            ],
            "inherited from the title"
        );
        let by_prefix = SearchFilter {
            cost_group: "48".into(),
            ..SearchFilter::default()
        };
        assert_eq!(index.position_hits(&lv, &by_prefix), 3);
        let provisional = SearchFilter {
            kind: KindFilter::Provisional,
            ..SearchFilter::default()
        };
        assert_eq!(
            texts(&lv, &index.hits(&lv, &provisional)).last().unwrap(),
            "Präsenzmelder"
        );
        let lump = SearchFilter {
            kind: KindFilter::LumpSum,
            ..SearchFilter::default()
        };
        assert_eq!(index.position_hits(&lv, &lump), 1);
        let keyword = SearchFilter {
            keyword: "knx".into(),
            ..SearchFilter::default()
        };
        assert_eq!(index.position_hits(&lv, &keyword), 1);
        assert_eq!(keywords(&lv), ["KNX", "Raumbedienung"]);
    }

    #[test]
    fn positions_from_the_library_are_compared_and_refreshed() {
        use crate::transfer::{self, TransferOptions};
        use crate::{EditStatus, ProjectName, TextBlock};

        let mut library =
            crate::Project::new(ProjectName::parse("Büro").unwrap(), "2026-11-01", library());
        let image = library.add_image("knx.png", vec![1, 2, 3]);
        let knx_id = library.lv().positions().next().unwrap().id();
        // Take the Bediengeräte into a project.
        let title = library.lv().rows()[1].id();
        let mut taken =
            transfer::collect(&library, library.lv(), &[title], TransferOptions::default());
        mark_origin(&mut taken.rows, "Büro");
        let mut project = crate::Project::new(
            ProjectName::parse("Schule").unwrap(),
            "2026-11-17",
            Lv::new("GA", OzMask::parse("11.22.PPPP").unwrap()),
        );
        transfer::insert(&mut project, None, &taken).unwrap();
        let in_lv = project.lv().positions().next().unwrap().clone();
        let origin = in_lv.origin().unwrap();
        assert_eq!((origin.library.as_str(), origin.entry), ("Büro", knx_id));
        assert_eq!(origin.as_of, "01.11.2026");

        // Nothing changed yet.
        let libraries = [("Büro", library.lv())];
        let found = links(project.lv(), &libraries);
        assert_eq!(found.found.len(), 2);
        let state = library_state(project.lv(), &libraries, &found, false);
        assert!(crate::lv_diff::compare(project.lv(), &state).is_empty());

        // The library entry gets a better text and an image; the LV position a quantity.
        let index = library.lv().index_of(knx_id).unwrap();
        if let Some(LvRow::Position(entry)) = library.lv_mut().row_mut(index) {
            entry.set_long_text(RichText::new(vec![
                TextBlock::paragraph(vec![crate::Run::plain("Taster mit Farbdisplay für KNX")]),
                TextBlock::Image {
                    file: image,
                    width_mm: None,
                },
            ]));
            entry.set_library_info(EntryInfo {
                as_of: "15.11.2026".into(),
                ..entry.library_info().clone()
            });
        }
        let position = project.lv().index_of(in_lv.id()).unwrap();
        if let Some(LvRow::Position(row)) = project.lv_mut().row_mut(position) {
            row.set_quantity(Decimal::from(12));
        }
        let libraries = [("Büro", library.lv())];
        let found = links(project.lv(), &libraries);
        let state = library_state(project.lv(), &libraries, &found, false);
        let diff = crate::lv_diff::compare(project.lv(), &state);
        assert_eq!(diff.changes.len(), 1, "{}", diff.text());
        assert!(diff.changes[0].has(crate::lv_diff::Aspect::LongText));
        assert!(
            !diff.changes[0].has(crate::lv_diff::Aspect::Quantity),
            "quantity stays the LV's"
        );

        let projects = [("Büro", &library)];
        let (updated, finished) =
            refresh_from_library(&mut project, &projects, &found, &[in_lv.id()], false);
        assert_eq!((updated, finished.len()), (1, 0));
        let refreshed = project.lv().position_by_id(in_lv.id()).unwrap().clone();
        assert!(refreshed.long_text().to_plain().contains("Farbdisplay"));
        assert_eq!(refreshed.quantity(), Decimal::from(12));
        assert_eq!(refreshed.unit_price(), None, "price only on request");
        assert_eq!(refreshed.origin().unwrap().as_of, "15.11.2026");
        assert_eq!(project.images().len(), 1, "image came along");

        // Finished positions stay.
        let index = project.lv().index_of(in_lv.id()).unwrap();
        if let Some(LvRow::Position(row)) = project.lv_mut().row_mut(index) {
            row.set_status(EditStatus::Done);
        }
        let (updated, finished) =
            refresh_from_library(&mut project, &projects, &found, &[in_lv.id()], true);
        assert_eq!((updated, finished), (0, vec![in_lv.ordinal().to_string()]));

        // A gone library entry is reported.
        let gone = [("Andere", library.lv())];
        assert_eq!(links(project.lv(), &gone).missing.len(), 2);

        let mut rows = vec![LvRow::Position(refreshed.clone())];
        prepare_for_library(&mut rows, "17.11.2026");
        let stored = rows[0].as_position().unwrap();
        assert!(stored.origin().is_none());
        assert_eq!(stored.library_info().as_of, "17.11.2026");
    }

    #[test]
    fn keywords_are_split_and_deduplicated() {
        assert_eq!(
            EntryInfo::parse_keywords(" KNX, Raum  bedienung;knx ,, DALI"),
            ["KNX", "Raum bedienung", "DALI"]
        );
        assert_eq!(depths(&library()), [0, 1, 2, 2, 0, 1]);
    }
}
