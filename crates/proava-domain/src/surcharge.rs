//! Zuschlagspositionen (GAEB DA XML `MarkupItem`, GAEB 90 Satz 23 mit
//! Zuschlag): a percentage on the totals of other positions.
//!
//! The percentage stands in the unit price field of the position (for a
//! bidder: the bidder's unit price of that position), the Bezugssumme
//! (GAEB `ITMarkup`) is always computed from the LV, never stored. The rules
//! are those of GAEB DA XML 3.3 as `proava_gaeb::xml::calc` applies them:
//!
//! - **alle Positionen** (`AllInCat`): the counted positions of the same list
//!   (the positions under the same lowest title) since the previous
//!   Zuschlagsblock;
//! - **gekennzeichnete Positionen** (`IdentAsMark`): only those marked
//!   „zu bezuschlagen“ (`MarkupIt`) in that range;
//! - **aufgezählte Positionen** (`ListInSubQty`): the listed positions, each
//!   with its total or a part quantity × unit price (`MarkupSubQty`).
//!
//! **Aufteilungszeilen** (Kosten-LV, Anwender 19.11.: „nur die Prozentzeilen
//! anlegen“): a Zuschlag with [`Surcharge::split`] splits its Bezugssumme
//! onto its own Kostengruppe – e.g. the Datenpunkte of the GA as Kurztext
//! positions, below them KG 481 40 %, 482 12 % … The positions it refers to
//! are then only the *Rechengrundlage*: they count nothing themselves (their
//! GP stays visible), the Aufteilungszeilen carry the money. Consecutive
//! Aufteilungszeilen on the same base form a block; when its percentages make
//! 100 %, the rounding rest goes to the line with the largest share, so the
//! block adds up to its base exactly.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::{Lv, LvRow, Money, Position, RowId};

/// Which positions a Zuschlag applies to (GAEB MarkupType).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurchargeKind {
    /// All positions of the list since the previous Zuschlag (AllInCat).
    #[default]
    AllInList,
    /// The positions marked „zu bezuschlagen“ in that range (IdentAsMark).
    Marked,
    /// The listed positions, optionally with a part quantity (ListInSubQty).
    Listed,
}

impl SurchargeKind {
    pub const ALL: [SurchargeKind; 3] = [
        SurchargeKind::AllInList,
        SurchargeKind::Marked,
        SurchargeKind::Listed,
    ];

    /// GAEB DA XML MarkupType.
    pub fn gaeb(self) -> &'static str {
        match self {
            SurchargeKind::AllInList => "AllInCat",
            SurchargeKind::Marked => "IdentAsMark",
            SurchargeKind::Listed => "ListInSubQty",
        }
    }

    /// From GAEB MarkupType; unknown or empty (GAEB 90) = all positions.
    pub fn from_gaeb(value: &str) -> Self {
        match value.trim() {
            "IdentAsMark" => SurchargeKind::Marked,
            "ListInSubQty" => SurchargeKind::Listed,
            _ => SurchargeKind::AllInList,
        }
    }

    /// Words for tables, print and MCP.
    pub fn label(self) -> &'static str {
        match self {
            SurchargeKind::AllInList => "auf alle Positionen davor",
            SurchargeKind::Marked => "auf die gekennzeichneten Positionen",
            SurchargeKind::Listed => "auf die genannten Positionen",
        }
    }

    /// Short key for the project file and the MCP (`alle`, `gekennzeichnet`, `liste`).
    pub fn key(self) -> &'static str {
        match self {
            SurchargeKind::AllInList => "alle",
            SurchargeKind::Marked => "gekennzeichnet",
            SurchargeKind::Listed => "liste",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_lowercase().as_str() {
            "alle" | "allincat" | "" => Some(SurchargeKind::AllInList),
            "gekennzeichnet" | "gekennzeichnete" | "markiert" | "identasmark" => {
                Some(SurchargeKind::Marked)
            }
            "liste" | "aufgezählt" | "aufgezaehlt" | "genannt" | "listinsubqty" => {
                Some(SurchargeKind::Listed)
            }
            _ => None,
        }
    }
}

/// One position a listed Zuschlag applies to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurchargeReference {
    pub position: RowId,
    /// Part quantity (GAEB SubQty); None = the whole position.
    pub quantity: Option<Decimal>,
}

/// What makes a position a Zuschlagsposition.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Surcharge {
    pub kind: SurchargeKind,
    /// Only for [`SurchargeKind::Listed`].
    pub references: Vec<SurchargeReference>,
    /// Aufteilungszeile (only in a Kosten-LV): the positions referred to are
    /// the Rechengrundlage and count nothing themselves.
    pub split: bool,
}

impl Surcharge {
    /// An Aufteilungszeile on the positions before it (as „alle“).
    pub fn split_on_all() -> Self {
        Self {
            kind: SurchargeKind::AllInList,
            references: Vec::new(),
            split: true,
        }
    }
}

/// Consecutive Aufteilungszeilen on the same base (planner's prices).
#[derive(Debug, Clone, PartialEq)]
pub struct SplitBlock {
    /// The Aufteilungszeilen in their order.
    pub rows: Vec<RowId>,
    /// The Rechengrundlage: the positions the block refers to.
    pub bases: Vec<RowId>,
    pub base: Money,
    /// Sum of the percentages (lines without one count 0).
    pub percent: Decimal,
    /// Sum of the amounts (with the rounding rest when the block makes 100 %).
    pub amount: Money,
}

impl SplitBlock {
    /// The percentages make 100 %.
    pub fn is_complete(&self) -> bool {
        self.percent == Decimal::ONE_HUNDRED
    }
}

/// Everything the Zuschläge of an LV give for one price source.
#[derive(Debug, Clone, Default)]
pub struct SurchargeComputation {
    pub values: HashMap<RowId, SurchargeValue>,
    /// Rechengrundlage: the positions in the base of an Aufteilungszeile.
    pub bases: HashSet<RowId>,
    pub blocks: Vec<SplitBlock>,
}

/// Bezugssumme and Zuschlag of one Zuschlagsposition for a price source.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SurchargeValue {
    pub base: Money,
    /// Base × percentage, rounded to cents; zero without a percentage.
    pub amount: Money,
}

/// Values of all Zuschlagspositionen of `lv` for a price source:
/// `counted_of` gives the counted total of an ordinary position (zero when it
/// does not count), `price_of` the unit price of a position – for a
/// Zuschlagsposition its percentage.
pub fn values_with(
    lv: &Lv,
    counted_of: &dyn Fn(&Position) -> Money,
    price_of: &dyn Fn(&Position) -> Option<Decimal>,
) -> HashMap<RowId, SurchargeValue> {
    compute_with(lv, counted_of, price_of).values
}

/// [`values_with`] with the Rechengrundlage of the Aufteilungszeilen and their
/// blocks (rounding rest on the largest share of a block that makes 100 %).
pub fn compute_with(
    lv: &Lv,
    counted_of: &dyn Fn(&Position) -> Money,
    price_of: &dyn Fn(&Position) -> Option<Decimal>,
) -> SurchargeComputation {
    let mut values = HashMap::new();
    let mut bases: HashSet<RowId> = HashSet::new();
    let mut blocks: Vec<SplitBlock> = Vec::new();
    if !lv.positions().any(Position::is_surcharge) {
        return SurchargeComputation::default();
    }
    // Counted totals of all ordinary positions (listed references may point
    // anywhere in the LV).
    let counted: HashMap<RowId, Money> = lv
        .positions()
        .filter(|position| !position.is_surcharge())
        .map(|position| (position.id(), counted_of(position)))
        .collect();
    let rows = lv.rows();
    let mut start = 0;
    while start < rows.len() {
        // A list: the rows up to the next title.
        let end = rows[start..]
            .iter()
            .position(LvRow::is_group)
            .map_or(rows.len(), |offset| start + offset);
        let list: Vec<&Position> = rows[start..end]
            .iter()
            .filter_map(LvRow::as_position)
            .collect();
        let ordinary: Vec<&Position> = list
            .iter()
            .copied()
            .filter(|position| !position.is_surcharge())
            .collect();
        let mut seen = 0usize;
        let mut group_start = 0usize;
        let mut group_end = 0usize;
        let mut last_was_surcharge = false;
        // The Aufteilungszeilen in a row on the same base (with their key).
        let mut run: Option<(SplitKey, SplitBlock)> = None;
        for position in &list {
            let Some(surcharge) = position.surcharge() else {
                seen += 1;
                last_was_surcharge = false;
                finish_run(&mut run, &mut values, &mut blocks, lv, price_of);
                continue;
            };
            if !last_was_surcharge {
                group_start = group_end;
                group_end = seen;
            }
            last_was_surcharge = true;
            let value_of = |item: &Position| counted.get(&item.id()).copied().unwrap_or_default();
            let members: Vec<&Position> = match surcharge.kind {
                SurchargeKind::Listed => Vec::new(),
                SurchargeKind::Marked => ordinary[group_start..group_end]
                    .iter()
                    .copied()
                    .filter(|item| item.is_marked_for_surcharge())
                    .collect(),
                SurchargeKind::AllInList => ordinary[group_start..group_end].to_vec(),
            };
            let base: Money = match surcharge.kind {
                SurchargeKind::Listed => surcharge
                    .references
                    .iter()
                    .filter_map(|reference| {
                        let item = lv.position_by_id(reference.position)?;
                        if item.is_surcharge() {
                            return None;
                        }
                        Some(match (reference.quantity, price_of(item)) {
                            (Some(quantity), Some(price)) => Money::total(price, quantity),
                            _ => value_of(item),
                        })
                    })
                    .sum(),
                _ => members.iter().map(|item| value_of(item)).sum(),
            };
            let amount = price_of(position)
                .map(|percent| base.percent(percent))
                .unwrap_or_default();
            values.insert(position.id(), SurchargeValue { base, amount });
            if !surcharge.split {
                finish_run(&mut run, &mut values, &mut blocks, lv, price_of);
                continue;
            }
            let member_ids: Vec<RowId> = match surcharge.kind {
                SurchargeKind::Listed => surcharge
                    .references
                    .iter()
                    .map(|reference| reference.position)
                    .filter(|id| {
                        lv.position_by_id(*id)
                            .is_some_and(|item| !item.is_surcharge())
                    })
                    .collect(),
                _ => members.iter().map(|item| item.id()).collect(),
            };
            bases.extend(member_ids.iter().copied());
            let key = SplitKey {
                kind: surcharge.kind,
                references: surcharge.references.clone(),
            };
            if run.as_ref().is_some_and(|(other, _)| *other != key) {
                finish_run(&mut run, &mut values, &mut blocks, lv, price_of);
            }
            let block = &mut run
                .get_or_insert_with(|| {
                    (
                        key,
                        SplitBlock {
                            rows: Vec::new(),
                            bases: member_ids,
                            base,
                            percent: Decimal::ZERO,
                            amount: Money::ZERO,
                        },
                    )
                })
                .1;
            block.rows.push(position.id());
            block.percent += price_of(position).unwrap_or_default();
        }
        finish_run(&mut run, &mut values, &mut blocks, lv, price_of);
        start = end + 1;
    }
    SurchargeComputation {
        values,
        bases,
        blocks,
    }
}

/// What makes Aufteilungszeilen one block (besides standing in a row).
#[derive(Debug, Clone, PartialEq)]
struct SplitKey {
    kind: SurchargeKind,
    references: Vec<SurchargeReference>,
}

/// Closes a block of Aufteilungszeilen: a block that makes 100 % adds up to
/// its base exactly – the rounding rest goes to the line with the largest
/// share (the first of equal ones), as in the Datenpunktrechnung.
fn finish_run(
    run: &mut Option<(SplitKey, SplitBlock)>,
    values: &mut HashMap<RowId, SurchargeValue>,
    blocks: &mut Vec<SplitBlock>,
    lv: &Lv,
    price_of: &dyn Fn(&Position) -> Option<Decimal>,
) {
    let Some((_, mut block)) = run.take() else {
        return;
    };
    let percent_of = |id: RowId| lv.position_by_id(id).and_then(price_of).unwrap_or_default();
    let sum: Money = block
        .rows
        .iter()
        .filter_map(|id| values.get(id))
        .map(|value| value.amount)
        .sum();
    if block.is_complete() {
        let rest = block.base - sum;
        let largest = block.rows.iter().copied().reduce(|best, id| {
            if percent_of(id) > percent_of(best) {
                id
            } else {
                best
            }
        });
        if let Some(value) = largest.and_then(|id| values.get_mut(&id)) {
            value.amount += rest;
        }
        block.amount = block.base;
    } else {
        block.amount = sum;
    }
    blocks.push(block);
}

/// Values for the planner's prices (Kostenvoranschlag).
pub fn planner_values(lv: &Lv) -> HashMap<RowId, SurchargeValue> {
    values_with(lv, &Position::counted_total, &Position::unit_price)
}

/// „40 % von 118.200,00 €“ – an Aufteilungs- or Zuschlagszeile in words.
pub fn share_text(percent: Option<Decimal>, base: Money) -> String {
    match percent {
        Some(percent) => format!("{} von {base}", percent_text(percent)),
        None => format!("– von {base}"),
    }
}

/// Percentage written as in the print and the tables: „5 %“, „3,5 %“.
pub fn percent_text(percent: Decimal) -> String {
    let places = percent.normalize().scale().min(6);
    format!("{} %", crate::format_decimal_de(percent, places))
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Surcharge, SurchargeKind, SurchargeReference};
    use crate::lv::{Lv, LvGroup, LvRow};
    use crate::money::Money;
    use crate::oz_mask::OzMask;
    use crate::position::{Position, PositionKind};
    use crate::{Address, Bid, BidPrice, RowId};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn position(ordinal: &str, quantity: &str, price: &str) -> LvRow {
        LvRow::Position(Position::new(
            ordinal,
            "Leistung",
            dec(quantity),
            "St",
            Some(dec(price)),
        ))
    }

    fn surcharge(kind: SurchargeKind, percent: &str) -> LvRow {
        LvRow::Position(Position::surcharge_position(
            "Zuschlag",
            Surcharge {
                split: false,
                kind,
                references: Vec::new(),
            },
            Some(dec(percent)),
        ))
    }

    fn amount(lv: &Lv, id: RowId) -> Money {
        lv.surcharge_values()[&id].amount
    }

    /// Alle Positionen davor: each Zuschlagsblock takes the positions since
    /// the previous one; two Zuschläge in a row share the same base.
    #[test]
    fn all_positions_since_the_previous_block() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Rohbau")));
        lv.push_row(position("01.0010", "2", "10")); // 20
        lv.push_row(position("01.0020", "1", "30")); // 30
        let first = lv.push_row(surcharge(SurchargeKind::AllInList, "5"));
        let second = lv.push_row(surcharge(SurchargeKind::AllInList, "3"));
        lv.push_row(position("01.0050", "1", "10"));
        let third = lv.push_row(surcharge(SurchargeKind::AllInList, "10"));
        lv.push_row(LvRow::Group(LvGroup::new(0, "02", "Ausbau")));
        lv.push_row(position("02.0010", "1", "100"));
        let values = lv.surcharge_values();
        assert_eq!(values[&first].base, Money::from_cents(5000));
        assert_eq!(amount(&lv, first), Money::from_cents(250));
        assert_eq!(values[&second].base, Money::from_cents(5000));
        assert_eq!(amount(&lv, second), Money::from_cents(150));
        assert_eq!(values[&third].base, Money::from_cents(1000), "nur 01.0050");
        assert_eq!(amount(&lv, third), Money::from_cents(100));
        // 20 + 30 + 2,50 + 1,50 + 10 + 1 = 65 in title 01, 100 in title 02
        let totals = lv.group_totals();
        assert_eq!(totals[0].total, Money::from_cents(6500));
        assert_eq!(totals[1].total, Money::from_cents(10000));
        assert_eq!(lv.net_total(), Money::from_cents(16500));
        let shown = lv.position_by_id(first).unwrap();
        assert_eq!(shown.total_price(), Money::ZERO, "nur über das LV");
        assert_eq!(lv.shown_total(shown), Money::from_cents(250));
    }

    /// Gekennzeichnete Positionen (IdentAsMark) and Positionen, die nicht
    /// zählen (Bedarf ohne GB), stay out of the base; a Zuschlag that does
    /// not count itself shows its amount but adds nothing.
    #[test]
    fn marked_positions_and_counting() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Titel")));
        let marked = lv.push_row(position("01.0010", "1", "200"));
        lv.push_row(position("01.0020", "1", "50"));
        let provisional = lv.push_row(position("01.0030", "1", "70"));
        let index = lv.index_of(marked).unwrap();
        lv.row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_marked_for_surcharge(true);
        let index = lv.index_of(provisional).unwrap();
        let row = lv.row_mut(index).unwrap().as_position_mut().unwrap();
        row.set_kind(PositionKind::Provisional { with_total: false });
        row.set_marked_for_surcharge(true);
        let on_marked = lv.push_row(surcharge(SurchargeKind::Marked, "10"));
        assert_eq!(
            lv.surcharge_values()[&on_marked].base,
            Money::from_cents(20000)
        );
        assert_eq!(amount(&lv, on_marked), Money::from_cents(2000));
        assert_eq!(lv.net_total(), Money::from_cents(27000));
        let index = lv.index_of(on_marked).unwrap();
        lv.row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_kind(PositionKind::Provisional { with_total: false });
        assert_eq!(amount(&lv, on_marked), Money::from_cents(2000));
        assert_eq!(lv.net_total(), Money::from_cents(25000));
    }

    /// Aufgezählte Positionen with a part quantity (ListInSubQty), also in
    /// another title; a removed position leaves the list.
    #[test]
    fn listed_positions_with_part_quantity() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Titel")));
        let a = lv.push_row(position("01.0010", "10", "12.5")); // 125
        let b = lv.push_row(position("01.0020", "1", "40"));
        lv.push_row(LvRow::Group(LvGroup::new(0, "02", "Zuschläge")));
        let listed = lv.push_row(LvRow::Position(Position::surcharge_position(
            "Zuschlag Nachtarbeit",
            Surcharge {
                split: false,
                kind: SurchargeKind::Listed,
                references: vec![
                    SurchargeReference {
                        position: a,
                        quantity: Some(dec("4")),
                    },
                    SurchargeReference {
                        position: b,
                        quantity: None,
                    },
                ],
            },
            Some(dec("25")),
        )));
        // 4 × 12,50 + 40 = 90 → 25 % = 22,50
        assert_eq!(lv.surcharge_values()[&listed].base, Money::from_cents(9000));
        assert_eq!(amount(&lv, listed), Money::from_cents(2250));
        assert_eq!(lv.net_total(), Money::from_cents(12500 + 4000 + 2250));
        let index = lv.index_of(b).unwrap();
        lv.remove_row(index);
        let surcharge = lv.position_by_id(listed).unwrap().surcharge().unwrap();
        assert_eq!(surcharge.references.len(), 1);
        assert_eq!(amount(&lv, listed), Money::from_cents(1250));
    }

    /// A bidder's Zuschlag: the bidder's percentage on the bidder's totals.
    #[test]
    fn bid_uses_its_own_percentage_and_totals() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_row(LvRow::Group(LvGroup::new(0, "01", "Titel")));
        let a = lv.push_row(position("01.0010", "2", "10"));
        let s = lv.push_row(surcharge(SurchargeKind::AllInList, "5"));
        let mut bid = Bid::new(Address::named("Firma"), "Datei");
        let price = |value: &str| BidPrice {
            unit_price: Some(dec(value)),
            stated_total: None,
            not_offered: false,
        };
        bid.set_price(a, price("15")); // 30
        bid.set_price(s, price("4")); // 4 % of 30 = 1,20
        assert_eq!(bid.surcharge_values(&lv)[&s].amount, Money::from_cents(120));
        assert_eq!(bid.net_total(&lv), Money::from_cents(3120));
        assert_eq!(lv.net_total(), Money::from_cents(2100));
    }

    #[test]
    fn percent_text_and_keys() {
        assert_eq!(super::percent_text(dec("5.000000")), "5 %");
        assert_eq!(super::percent_text(dec("3.5")), "3,5 %");
        for kind in SurchargeKind::ALL {
            assert_eq!(SurchargeKind::from_key(kind.key()), Some(kind));
            assert_eq!(SurchargeKind::from_gaeb(kind.gaeb()), kind);
        }
    }
}
