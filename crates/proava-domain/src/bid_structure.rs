//! Strukturvergleich eines Angebots mit dem LV (Anwender 20.11.: „soll
//! warnen, auch bei sonstigen Strukturabweichungen“).
//!
//! Bieter geben oft auf eine Fassung ab, in der OZ umnummeriert, Titel
//! umgestellt oder Mengen geändert sind; die Dateien enthalten meist nur OZ,
//! EP und GP, selten Texte. Deshalb:
//! 1. Zuordnung über die OZ, geprüft gegen Menge (angegeben oder GP ÷ EP),
//!    Einheit und – wenn vorhanden – Text; Widersprüche sind „Konflikte“.
//! 2. Die übrigen Positionen werden zwischen festen Ankern der Reihe nach
//!    abgeglichen ([`align`]): gleich viele auf beiden Seiten und nichts
//!    widerspricht = „sicher“, sonst Vorschlag „prüfen“.
//! 3. Was bleibt, steht im Abweichungsprotokoll ([`StructureReport`]) am
//!    Angebot. Gewertet wird immer mit Menge und Text des LV.

use std::collections::HashSet;

use rust_decimal::Decimal;

use crate::{Lv, Money, Position, PositionKind, RowId};

/// One position of the bid file as read: kept for positions that are not
/// assigned by their OZ (for the protocol and a later assignment).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BidItem {
    /// OZ as in the file, dotted („03.489.10.0010“).
    pub ordinal: String,
    pub short_text: String,
    /// Langtext as plain text, shortened to [`LONG_TEXT_KEPT`] characters.
    pub long_text: String,
    pub unit: String,
    /// Quantity written in the file (Vordersatz of the bidder).
    pub quantity: Option<Decimal>,
    pub unit_price: Option<Decimal>,
    /// GP as written in the file.
    pub total: Option<Decimal>,
    pub not_offered: bool,
    /// Kind as [`kind_label`] names it; empty = not in the file.
    pub kind: String,
}

/// How much of a Langtext is kept at an item (enough to compare).
pub const LONG_TEXT_KEPT: usize = 1200;

impl BidItem {
    /// The quantity the bidder priced: written, else GP ÷ EP.
    pub fn priced_quantity(&self) -> Option<Decimal> {
        self.quantity.or_else(|| {
            let (price, total) = (self.unit_price?, self.total?);
            (!price.is_zero()).then(|| (total / price).round_dp(3))
        })
    }

    /// Whether the item fits an LV quantity: the written quantity is the
    /// same, or GP = quantity × EP (to the cent, or 0.05 ‰ of large sums).
    /// None when the file tells nothing (no quantity, EP 0 or no GP).
    pub fn quantity_fits(&self, quantity: Decimal) -> Option<bool> {
        if let Some(own) = self.quantity {
            return Some(own.normalize() == quantity.normalize());
        }
        let (price, total) = (self.unit_price?, self.total?);
        if price.is_zero() {
            return None;
        }
        let computed = Money::total(price, quantity).to_decimal();
        let tolerance = Decimal::new(2, 2).max(total.abs() * Decimal::new(5, 5));
        Some((computed - total).abs() <= tolerance)
    }

    /// EP for the LV: written, else GP ÷ written quantity.
    pub fn price(&self) -> Option<Decimal> {
        self.unit_price.or_else(|| {
            let (total, quantity) = (self.total?, self.quantity?);
            (!quantity.is_zero()).then(|| (total / quantity).round_dp(3))
        })
    }
}

/// „Normalposition“, „Bedarfsposition mit GB“ …, „, pauschal“ behind.
pub fn kind_label(kind: PositionKind, lump_sum: bool) -> String {
    let label = match kind {
        PositionKind::Normal => "Normalposition",
        PositionKind::Provisional { with_total: true } => "Bedarfsposition mit GB",
        PositionKind::Provisional { with_total: false } => "Bedarfsposition ohne GB",
        PositionKind::Base { .. } => "Grundposition",
        PositionKind::Alternative { .. } => "Wahlposition",
        PositionKind::Supplement => "Zulageposition",
    };
    if lump_sum {
        format!("{label}, pauschal")
    } else {
        label.to_string()
    }
}

/// [`kind_label`] of an LV position.
pub fn position_kind_label(position: &Position) -> String {
    kind_label(position.kind(), position.is_lump_sum())
}

/// Text for comparing: lower case, typographic marks plain, one space.
pub fn normalized(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\u{2013}' | '\u{2014}' | '\u{2212}' | '\u{2010}' | '\u{2011}' => '-',
            '\u{201E}' | '\u{201C}' | '\u{201D}' | '\u{00AB}' | '\u{00BB}' => '"',
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{00B4}' | '`' => '\'',
            '\u{00A0}' | '\u{202F}' | '\u{2009}' => ' ',
            other => other,
        })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(text: &str) -> HashSet<String> {
    normalized(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Similarity of two texts, 0.0 – 1.0: 1.0 when equal after
/// [`normalized`], else the share of common words.
pub fn text_similarity(a: &str, b: &str) -> f64 {
    if normalized(a) == normalized(b) {
        return 1.0;
    }
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(&b).count() as f64 / a.union(&b).count() as f64
}

/// Units compared without case, dot and the usual spellings of „Stück“.
pub fn same_unit(a: &str, b: &str) -> bool {
    let plain = |unit: &str| {
        let unit = normalized(unit).replace(['.', ' '], "");
        match unit.as_str() {
            "st" | "stk" | "stck" | "stück" | "stueck" => "st".to_string(),
            "psch" | "pschl" | "pauschal" | "pau" => "psch".to_string(),
            "m2" | "m²" | "qm" => "m2".to_string(),
            "m3" | "m³" | "cbm" => "m3".to_string(),
            "h" | "std" | "stunde" | "stunden" => "h".to_string(),
            _ => unit,
        }
    };
    plain(a) == plain(b)
}

/// Assessment of an item for an LV position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairCheck {
    /// 0–100.
    pub similarity: u8,
    /// Something the file says speaks against the pair (quantity, text).
    pub contradicts: bool,
    /// Something the file says confirms the pair (quantity or text).
    pub confirmed: bool,
}

/// How well `item` fits `position`: texts (when both have one), quantity,
/// unit.
pub fn check_pair(item: &BidItem, position: &Position) -> PairCheck {
    let texts = !item.short_text.trim().is_empty() && !position.short_text().trim().is_empty();
    let quantity = item.quantity_fits(position.quantity());
    let unit_known = !item.unit.trim().is_empty() && !position.unit().trim().is_empty();
    let unit_same = unit_known && same_unit(&item.unit, position.unit());
    let mut score: f64;
    let mut contradicts = quantity == Some(false);
    let mut confirmed = quantity == Some(true);
    if texts {
        let short = text_similarity(&item.short_text, position.short_text());
        let long_plain = position.long_text().to_plain();
        let text = if !item.long_text.trim().is_empty() && !long_plain.trim().is_empty() {
            0.6 * short + 0.4 * text_similarity(&item.long_text, &long_plain)
        } else {
            short
        };
        score = 100.0 * text;
        if text < 0.4 {
            contradicts = true;
        }
        if text >= 0.8 {
            confirmed = true;
        }
        if quantity == Some(false) {
            score -= 30.0;
        }
    } else {
        // Only the place in the order speaks for the pair.
        score = 70.0;
        match quantity {
            Some(true) => score += 25.0,
            Some(false) => score -= 40.0,
            None => {}
        }
    }
    if unit_known {
        if unit_same {
            score += 5.0;
        } else {
            score -= 15.0;
        }
    }
    PairCheck {
        similarity: score.clamp(0.0, 100.0).round() as u8,
        contradicts,
        confirmed,
    }
}

/// A proposed assignment of a file item to an LV position.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// Index into the items given to [`align`].
    pub item: usize,
    pub row: RowId,
    /// 0–100.
    pub similarity: u8,
    /// Same number of positions on both sides between two fixed points,
    /// nothing contradicts and the quantities or texts confirm it.
    pub sure: bool,
}

/// Result of [`align`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Alignment {
    /// For every item: its LV position, if any.
    pub rows: Vec<Option<RowId>>,
    /// Items assigned by [`align`] (not by their OZ): index and similarity.
    pub by_structure: Vec<(usize, u8)>,
    /// Items assigned by their OZ although the file contradicts it.
    pub conflicts: Vec<usize>,
    /// Proposals not taken over (all of them unless `take_sure`).
    pub suggestions: Vec<Suggestion>,
}

/// Matches the file items (in file order) with the LV positions.
/// `by_ordinal`: the position each item has by its OZ; `manual`: items
/// assigned by hand (index, row) – they win. With `take_sure` the sure
/// proposals are taken over, a contradicted OZ pair giving way.
pub fn align(
    lv: &Lv,
    items: &[BidItem],
    by_ordinal: &[Option<RowId>],
    manual: &[(usize, RowId)],
    take_sure: bool,
) -> Alignment {
    let positions: Vec<&Position> = lv.positions().collect();
    let order: std::collections::HashMap<RowId, usize> = positions
        .iter()
        .enumerate()
        .map(|(index, position)| (position.id(), index))
        .collect();
    let mut rows: Vec<Option<RowId>> = by_ordinal.to_vec();
    rows.resize(items.len(), None);
    // By hand first: a row taken by hand is free for nobody else.
    let manual_rows: HashSet<RowId> = manual.iter().map(|(_, row)| *row).collect();
    for row in rows.iter_mut() {
        if row.is_some_and(|row| manual_rows.contains(&row)) {
            *row = None;
        }
    }
    let manual_items: HashSet<usize> = manual.iter().map(|(item, _)| *item).collect();
    for (item, row) in manual {
        if *item < rows.len() {
            rows[*item] = Some(*row);
        }
    }
    let position_of = |row: RowId| order.get(&row).map(|&index| positions[index]);
    let conflicts: Vec<usize> = (0..items.len())
        .filter(|item| !manual_items.contains(item))
        .filter(|&item| {
            rows[item]
                .and_then(position_of)
                .is_some_and(|position| check_pair(&items[item], position).contradicts)
        })
        .collect();
    let conflict_set: HashSet<usize> = conflicts.iter().copied().collect();
    // Fixed points: assigned items without contradiction.
    let anchor = |item: usize| rows[item].is_some() && !conflict_set.contains(&item);
    let taken: HashSet<RowId> = (0..items.len())
        .filter(|&item| anchor(item))
        .filter_map(|item| rows[item])
        .collect();
    let free = |index: usize| !taken.contains(&positions[index].id());

    let mut suggestions = Vec::new();
    let mut start = 0;
    let mut previous_lv: Option<usize> = None;
    while start < items.len() {
        if anchor(start) {
            previous_lv = rows[start].and_then(|row| order.get(&row).copied());
            start += 1;
            continue;
        }
        let mut end = start;
        while end < items.len() && !anchor(end) {
            end += 1;
        }
        let next_lv = (end < items.len())
            .then(|| rows[end].and_then(|row| order.get(&row).copied()))
            .flatten();
        let from = previous_lv.map_or(0, |index| index + 1);
        let to = next_lv.unwrap_or(positions.len());
        let gap_items: Vec<usize> = (start..end).collect();
        let gap_positions: Vec<usize> = if from < to {
            (from..to).filter(|&index| free(index)).collect()
        } else {
            Vec::new()
        };
        suggestions.extend(gap(items, &positions, &gap_items, &gap_positions));
        start = end;
    }
    // A conflict item proposed for its own row is no proposal.
    suggestions.retain(|suggestion| rows[suggestion.item] != Some(suggestion.row));

    let mut by_structure = Vec::new();
    if take_sure {
        let mut kept = Vec::new();
        for suggestion in suggestions {
            if !suggestion.sure {
                kept.push(suggestion);
                continue;
            }
            // The row may still carry a contradicted OZ pair: it gives way.
            for (other, row) in rows.iter_mut().enumerate() {
                if *row == Some(suggestion.row) && other != suggestion.item {
                    *row = None;
                }
            }
            rows[suggestion.item] = Some(suggestion.row);
            by_structure.push((suggestion.item, suggestion.similarity));
        }
        suggestions = kept;
    }
    // A contradicted OZ pair is a conflict only when a proposal names
    // another position; else it is a plain deviation (quantity, text).
    let conflicts = conflicts
        .into_iter()
        .filter(|&item| {
            !by_structure.iter().any(|(moved, _)| *moved == item)
                && suggestions.iter().any(|suggestion| suggestion.item == item)
        })
        .collect();
    Alignment {
        rows,
        by_structure,
        conflicts,
        suggestions,
    }
}

/// Proposals for one gap between two fixed points.
fn gap(
    items: &[BidItem],
    positions: &[&Position],
    gap_items: &[usize],
    gap_positions: &[usize],
) -> Vec<Suggestion> {
    if gap_items.is_empty() || gap_positions.is_empty() {
        return Vec::new();
    }
    if gap_items.len() == gap_positions.len() {
        let checks: Vec<PairCheck> = gap_items
            .iter()
            .zip(gap_positions)
            .map(|(&item, &index)| check_pair(&items[item], positions[index]))
            .collect();
        let clean = checks.iter().all(|check| !check.contradicts);
        let confirmed = checks.iter().any(|check| check.confirmed);
        return gap_items
            .iter()
            .zip(gap_positions)
            .zip(&checks)
            .map(|((&item, &index), check)| {
                // In the right order and confirmed elsewhere in the gap.
                let sure = clean && confirmed;
                let similarity = if sure && !check.confirmed {
                    check.similarity.max(90)
                } else {
                    check.similarity
                };
                Suggestion {
                    item,
                    row: positions[index].id(),
                    similarity,
                    sure: sure && similarity >= 90,
                }
            })
            .collect();
    }
    // Different numbers: the best order-keeping pairs (dynamic programming
    // over the similarities), only pairs that do not contradict.
    let (n, m) = (gap_items.len(), gap_positions.len());
    if n * m > 250_000 {
        return Vec::new();
    }
    let check = |i: usize, j: usize| check_pair(&items[gap_items[i]], positions[gap_positions[j]]);
    let mut best = vec![vec![0i32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let pair = check(i, j);
            let take = if pair.contradicts || pair.similarity < 50 {
                i32::MIN
            } else {
                i32::from(pair.similarity) + best[i + 1][j + 1]
            };
            best[i][j] = take.max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let pair = check(i, j);
        let take = !pair.contradicts
            && pair.similarity >= 50
            && best[i][j] == i32::from(pair.similarity) + best[i + 1][j + 1];
        if take {
            out.push(Suggestion {
                item: gap_items[i],
                row: positions[gap_positions[j]].id(),
                similarity: pair.similarity,
                // Unequal numbers: only an identical text is sure.
                sure: pair.confirmed
                    && !items[gap_items[i]].short_text.is_empty()
                    && pair.similarity >= 95,
            });
            i += 1;
            j += 1;
        } else if best[i][j] == best[i + 1][j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// How a position came to its price from another OZ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignedBy {
    /// Over order, quantities and texts (sure proposal).
    Structure,
    /// By the user (dialog, MCP).
    Hand,
}

impl AssignedBy {
    pub fn key(self) -> &'static str {
        match self {
            AssignedBy::Structure => "struktur",
            AssignedBy::Hand => "hand",
        }
    }

    pub fn from_key(key: &str) -> Self {
        if key == "hand" {
            AssignedBy::Hand
        } else {
            AssignedBy::Structure
        }
    }
}

/// A file item that went to an LV position with another OZ.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub item: BidItem,
    pub row: RowId,
    pub by: AssignedBy,
    pub similarity: u8,
}

/// A stored proposal (item by its OZ in the file).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSuggestion {
    pub ordinal: String,
    pub row: RowId,
    pub similarity: u8,
    pub sure: bool,
}

/// What the bidder changed against the LV, stored at the bid
/// (Abweichungsprotokoll). Rows are LV positions.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructureReport {
    /// File items without an LV position (not ranked), with their prices.
    pub foreign: Vec<BidItem>,
    /// Items assigned to another OZ (structure or hand).
    pub assigned: Vec<Assignment>,
    /// Items assigned by their OZ although quantity or text speak against
    /// it (kept; a proposal may name the right position).
    pub conflicts: Vec<(RowId, BidItem)>,
    /// Proposals for `foreign` and `conflicts`.
    pub suggestions: Vec<StoredSuggestion>,
    /// Quantity of the bidder ≠ LV (ranked with the LV quantity).
    pub quantities: Vec<(RowId, Decimal)>,
    pub units: Vec<(RowId, String)>,
    pub short_texts: Vec<(RowId, String)>,
    /// Langtext clearly changed: similarity in percent.
    pub long_texts: Vec<(RowId, u8)>,
    /// Kind of the bidder ([`kind_label`]) ≠ LV.
    pub kinds: Vec<(RowId, String)>,
    /// Titles changed, moved, only on one side – as lines.
    pub titles: Vec<String>,
    /// Sum of the file before discount, if it has one.
    pub file_total: Option<Money>,
}

/// One line of the protocol: what, bidder OZ, LV OZ, details.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviationLine {
    pub what: &'static str,
    pub bid_ordinal: String,
    pub lv_ordinal: String,
    pub detail: String,
}

/// Why the sum of the file differs from the ranked sum.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TotalBreakdown {
    pub file: Money,
    pub ranked: Money,
    /// GP of the file items without LV position.
    pub foreign: Money,
    /// GP of the file − bidder quantity × EP.
    pub arithmetic: Money,
    /// (bidder quantity − LV quantity) × EP.
    pub quantities: Money,
    /// What none of these explains (rounding, Zuschläge …).
    pub rest: Money,
}

impl TotalBreakdown {
    /// „Summe der Datei … ≠ gewertet … (Differenz …): nicht zugeordnete
    /// Positionen …, Rechenfehler …, Mengenabweichungen …, Rest …“.
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        for (amount, label) in [
            (self.foreign, "nicht zugeordnete Positionen"),
            (self.arithmetic, "Rechenfehler"),
            (self.quantities, "Mengenabweichungen"),
            (self.rest, "Rest"),
        ] {
            if amount.cents().abs() > 1 {
                parts.push(format!("{label} {amount}"));
            }
        }
        format!(
            "Summe der Datei {} ≠ gewertet {} (Differenz {}){}",
            self.file,
            self.ranked,
            self.file - self.ranked,
            if parts.is_empty() {
                String::new()
            } else {
                format!(": {}", parts.join(", "))
            }
        )
    }
}

fn number(value: Decimal) -> String {
    crate::money::format_decimal_de(value.normalize(), value.normalize().scale().min(3))
}

impl StructureReport {
    pub fn is_empty(&self) -> bool {
        self.foreign.is_empty()
            && self.assigned.is_empty()
            && self.conflicts.is_empty()
            && self.quantities.is_empty()
            && self.units.is_empty()
            && self.short_texts.is_empty()
            && self.long_texts.is_empty()
            && self.kinds.is_empty()
            && self.titles.is_empty()
    }

    /// The stored item for a proposal.
    pub fn item(&self, ordinal: &str) -> Option<&BidItem> {
        self.foreign
            .iter()
            .chain(self.conflicts.iter().map(|(_, item)| item))
            .find(|item| item.ordinal == ordinal)
    }

    /// „Strukturabweichungen: 6 umnummeriert, 2 Mengen, 1 Einheit …“, None
    /// without any; `missing` = LV positions not in the file.
    pub fn summary(&self, missing: usize) -> Option<String> {
        let count = crate::wording::count;
        let mut parts = Vec::new();
        let mut add = |number: usize, one: &str, many: &str| {
            if number > 0 {
                parts.push(count(number, one, many));
            }
        };
        add(
            self.assigned.len(),
            "umnummeriert (zugeordnet)",
            "umnummeriert (zugeordnet)",
        );
        add(
            self.foreign.len(),
            "Position nicht im LV",
            "Positionen nicht im LV",
        );
        add(missing, "LV-Position fehlt", "LV-Positionen fehlen");
        add(
            self.conflicts.len(),
            "OZ passt nicht zum Inhalt",
            "OZ passen nicht zum Inhalt",
        );
        add(self.quantities.len(), "Menge", "Mengen");
        add(self.units.len(), "Einheit", "Einheiten");
        add(self.short_texts.len(), "Kurztext", "Kurztexte");
        add(self.long_texts.len(), "Langtext", "Langtexte");
        add(self.kinds.len(), "Positionsart", "Positionsarten");
        add(self.titles.len(), "Titel", "Titel");
        (!parts.is_empty()).then(|| format!("Strukturabweichungen: {}", parts.join(", ")))
    }

    /// Every deviation as a line (for the bid view, MCP and the print).
    pub fn lines(&self, lv: &Lv, missing: &[RowId]) -> Vec<DeviationLine> {
        let oz = |row: RowId| {
            lv.position_by_id(row)
                .map(|position| position.ordinal().to_string())
                .unwrap_or_else(|| format!("#{}", row.0))
        };
        let position = |row: RowId| lv.position_by_id(row);
        let mut lines = Vec::new();
        for assignment in &self.assigned {
            lines.push(DeviationLine {
                what: "umnummeriert",
                bid_ordinal: assignment.item.ordinal.clone(),
                lv_ordinal: oz(assignment.row),
                detail: format!(
                    "{} zugeordnet ({} %)",
                    match assignment.by {
                        AssignedBy::Structure => "über Reihenfolge/Menge/Text",
                        AssignedBy::Hand => "von Hand",
                    },
                    assignment.similarity
                ),
            });
        }
        for item in &self.foreign {
            let total = item
                .total
                .map(|total| format!(", GP {}", Money::from_decimal(total)))
                .unwrap_or_default();
            lines.push(DeviationLine {
                what: "nicht im LV",
                bid_ordinal: item.ordinal.clone(),
                lv_ordinal: String::new(),
                detail: format!(
                    "{}{}{total} – nicht gewertet",
                    if item.short_text.is_empty() {
                        "ohne Text"
                    } else {
                        item.short_text.as_str()
                    },
                    item.unit_price
                        .map(|price| format!(", EP {}", number(price)))
                        .unwrap_or_default()
                ),
            });
        }
        for row in missing {
            lines.push(DeviationLine {
                what: "fehlt im Angebot",
                bid_ordinal: String::new(),
                lv_ordinal: oz(*row),
                detail: position(*row)
                    .map(|position| position.short_text().to_string())
                    .unwrap_or_default(),
            });
        }
        for (row, item) in &self.conflicts {
            let quantity = position(*row)
                .filter(|position| item.quantity_fits(position.quantity()) == Some(false))
                .and_then(|_| item.priced_quantity())
                .map(|quantity| format!("Menge im Angebot {}", number(quantity)))
                .unwrap_or_else(|| "Text passt nicht".to_string());
            lines.push(DeviationLine {
                what: "OZ passt nicht",
                bid_ordinal: item.ordinal.clone(),
                lv_ordinal: oz(*row),
                detail: format!("{quantity} – zugeordnet über die OZ, bitte prüfen"),
            });
        }
        for (row, quantity) in &self.quantities {
            lines.push(DeviationLine {
                what: "Menge",
                bid_ordinal: oz(*row),
                lv_ordinal: oz(*row),
                detail: format!(
                    "LV {}, Angebot {} – gewertet mit der LV-Menge",
                    position(*row)
                        .map(|position| number(position.quantity()))
                        .unwrap_or_default(),
                    number(*quantity)
                ),
            });
        }
        for (row, unit) in &self.units {
            lines.push(DeviationLine {
                what: "Einheit",
                bid_ordinal: oz(*row),
                lv_ordinal: oz(*row),
                detail: format!(
                    "LV „{}“, Angebot „{unit}“",
                    position(*row)
                        .map(|position| position.unit().to_string())
                        .unwrap_or_default()
                ),
            });
        }
        for (row, text) in &self.short_texts {
            lines.push(DeviationLine {
                what: "Kurztext",
                bid_ordinal: oz(*row),
                lv_ordinal: oz(*row),
                detail: format!(
                    "LV „{}“, Angebot „{text}“",
                    position(*row)
                        .map(|position| position.short_text().to_string())
                        .unwrap_or_default()
                ),
            });
        }
        for (row, similarity) in &self.long_texts {
            lines.push(DeviationLine {
                what: "Langtext",
                bid_ordinal: oz(*row),
                lv_ordinal: oz(*row),
                detail: format!("deutlich geändert (Ähnlichkeit {similarity} %)"),
            });
        }
        for (row, kind) in &self.kinds {
            lines.push(DeviationLine {
                what: "Positionsart",
                bid_ordinal: oz(*row),
                lv_ordinal: oz(*row),
                detail: format!(
                    "LV {}, Angebot {kind}",
                    position(*row).map(position_kind_label).unwrap_or_default()
                ),
            });
        }
        for title in &self.titles {
            lines.push(DeviationLine {
                what: "Titel",
                bid_ordinal: String::new(),
                lv_ordinal: String::new(),
                detail: title.clone(),
            });
        }
        lines
    }
}

/// [`DeviationLine`] as one line of text.
impl DeviationLine {
    pub fn text(&self) -> String {
        let text = self.raw_text();
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn raw_text(&self) -> String {
        let place = match (self.bid_ordinal.as_str(), self.lv_ordinal.as_str()) {
            ("", "") => String::new(),
            (bid, "") => format!("Angebot {bid}"),
            ("", lv) => format!("LV {lv}"),
            (bid, lv) if bid == lv => lv.to_string(),
            (bid, lv) => format!("Angebot {bid} → LV {lv}"),
        };
        if place.is_empty() {
            format!("{}: {}", self.what, self.detail)
        } else {
            format!("{}: {place} – {}", self.what, self.detail)
        }
    }
}

#[cfg(test)]
mod tests;

impl crate::Bid {
    /// LV positions the file does not contain at all (no entry; a position
    /// in the file without EP is no such case).
    pub fn absent_positions(&self, lv: &Lv) -> Vec<RowId> {
        lv.positions()
            .filter(|position| !self.prices.contains_key(&position.id()))
            .map(Position::id)
            .collect()
    }

    /// Why the sum of the file differs from the ranked sum; None without a
    /// file sum or when they agree to the cent.
    pub fn total_breakdown(&self, lv: &Lv) -> Option<TotalBreakdown> {
        let file = self.structure.file_total?;
        let ranked = self.net_total(lv);
        if (file - ranked).cents().abs() <= 1 {
            return None;
        }
        let foreign: Money = self
            .structure
            .foreign
            .iter()
            .map(
                |item| match (item.total, item.price(), item.priced_quantity()) {
                    (Some(total), _, _) => Money::from_decimal(total),
                    (None, Some(price), Some(quantity)) => Money::total(price, quantity),
                    _ => Money::ZERO,
                },
            )
            .sum();
        let (mut arithmetic, mut quantities) = (Money::ZERO, Money::ZERO);
        for position in lv.positions() {
            if !position.counts_in_total() || position.is_surcharge() {
                continue;
            }
            let Some(price) = self.price(position.id()) else {
                continue;
            };
            let (Some(stated), Some(unit_price)) = (price.stated_total, price.unit_price) else {
                continue;
            };
            let stated = Money::from_decimal(stated);
            let ranked_here = self.total_for(position);
            let bidder = self
                .structure
                .quantities
                .iter()
                .find(|(row, _)| *row == position.id())
                .map(|(_, quantity)| Money::total(unit_price, *quantity));
            match bidder {
                Some(bidder) => {
                    quantities += bidder - ranked_here;
                    arithmetic += stated - bidder;
                }
                None => arithmetic += stated - ranked_here,
            }
        }
        Some(TotalBreakdown {
            file,
            ranked,
            foreign,
            arithmetic,
            quantities,
            rest: file - ranked - foreign - arithmetic - quantities,
        })
    }

    /// Gives a stored file item (not ranked or assigned by its OZ against
    /// its content) to the LV position `row`: its price goes there, the
    /// protocol notes it. A row priced by another OZ pair that the file
    /// contradicts gives way; a row priced without contradiction is refused.
    pub fn assign_item(
        &mut self,
        lv: &Lv,
        ordinal: &str,
        row: RowId,
        by: AssignedBy,
        similarity: u8,
    ) -> Result<(), String> {
        let position = lv
            .position_by_id(row)
            .ok_or("Die LV-Position gibt es nicht (mehr).")?;
        let structure = &mut self.structure;
        let item = if let Some(index) = structure
            .foreign
            .iter()
            .position(|item| item.ordinal == ordinal)
        {
            structure.foreign.remove(index)
        } else if let Some(index) = structure
            .conflicts
            .iter()
            .position(|(_, item)| item.ordinal == ordinal)
        {
            let (old, item) = structure.conflicts.remove(index);
            if old == row {
                structure.conflicts.insert(index, (old, item));
                return Err(format!("{ordinal} steht schon bei {}.", position.ordinal()));
            }
            self.prices.remove(&old);
            self.offered_quantities.remove(&old);
            self.complements.remove(&old);
            self.structure.quantities.retain(|(other, _)| *other != old);
            item
        } else {
            return Err(format!(
                "Die Angebotsposition {ordinal} ist schon zugeordnet oder unbekannt."
            ));
        };
        let structure = &mut self.structure;
        if self.prices.contains_key(&row) {
            match structure
                .conflicts
                .iter()
                .position(|(other, _)| *other == row)
            {
                Some(index) => {
                    // The row's own pair was contradicted: that item is now foreign.
                    let (_, displaced) = structure.conflicts.remove(index);
                    structure.foreign.push(displaced);
                    structure.quantities.retain(|(other, _)| *other != row);
                }
                None => {
                    structure.foreign.push(item);
                    return Err(format!(
                        "{} hat schon einen Preis aus dem Angebot – erst dort lösen.",
                        position.ordinal()
                    ));
                }
            }
        }
        let structure = &mut self.structure;
        if let Some(quantity) = item
            .quantity
            .filter(|quantity| quantity.normalize() != position.quantity().normalize())
        {
            structure.quantities.push((row, quantity));
        }
        self.prices.insert(
            row,
            crate::BidPrice {
                unit_price: item.price(),
                stated_total: item.total,
                not_offered: item.not_offered,
            },
        );
        self.offered_quantities.insert(row, position.quantity());
        self.unknown_ordinals.retain(|known| known != ordinal);
        let structure = &mut self.structure;
        structure
            .suggestions
            .retain(|suggestion| suggestion.ordinal != ordinal && suggestion.row != row);
        structure.assigned.push(Assignment {
            item,
            row,
            by,
            similarity,
        });
        Ok(())
    }

    /// Takes over every sure proposal; returns how many.
    pub fn take_sure_suggestions(&mut self, lv: &Lv) -> usize {
        let sure: Vec<StoredSuggestion> = self
            .structure
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.sure)
            .cloned()
            .collect();
        sure.into_iter()
            .filter(|suggestion| {
                self.assign_item(
                    lv,
                    &suggestion.ordinal,
                    suggestion.row,
                    AssignedBy::Structure,
                    suggestion.similarity,
                )
                .is_ok()
            })
            .count()
    }
}
