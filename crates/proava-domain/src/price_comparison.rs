//! Preisspiegel: the bids side by side, position by position.

use rust_decimal::Decimal;

use crate::bid::{Bid, BidId, QuantityChange};
use crate::lv::{Lv, LvRow, RowId};
use crate::money::Money;
use crate::position::{Position, PositionKind};

/// What a bidder's unit price is compared with (Bezugsgröße der Abweichung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviationReference {
    /// Mean of all offered unit prices of the line.
    #[default]
    Mean,
    /// Lowest offered unit price of the line.
    Lowest,
    /// The planner's unit price (Kostenvoranschlag).
    Estimate,
}

impl DeviationReference {
    pub fn label(self) -> &'static str {
        match self {
            DeviationReference::Mean => "Mittelwert",
            DeviationReference::Lowest => "Günstigster",
            DeviationReference::Estimate => "Planerpreis",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComparisonOptions {
    /// A unit price deviating more than this from the reference is an outlier.
    pub outlier_percent: Decimal,
    /// Show the planner's own prices (Kostenvoranschlag) as reference column.
    pub with_estimate: bool,
    /// What deviations and outliers are measured against.
    pub reference: DeviationReference,
    /// Variant: Bedarfspositionen ohne GB count with their quantity.
    pub include_provisional: bool,
}

impl Default for ComparisonOptions {
    fn default() -> Self {
        Self {
            outlier_percent: Decimal::from(20),
            with_estimate: true,
            reference: DeviationReference::default(),
            include_provisional: false,
        }
    }
}

impl ComparisonOptions {
    /// Whether a position of this kind counts in the sums under these options.
    pub fn counts(&self, kind: PositionKind) -> bool {
        kind.counts_in_total()
            || (self.include_provisional
                && matches!(kind, PositionKind::Provisional { with_total: false }))
    }

    fn counted(&self, position: &Position, total: Money) -> Money {
        if self.counts(position.kind()) && !position.is_not_applicable() {
            total
        } else {
            Money::ZERO
        }
    }
}

/// Totals and rank of one bid.
#[derive(Debug, Clone, PartialEq)]
pub struct BidSummary {
    pub bid: BidId,
    pub name: String,
    pub net: Money,
    pub discount: Money,
    pub net_after_discount: Money,
    pub vat: Money,
    pub gross: Money,
    /// 1 = cheapest (after discount).
    pub rank: usize,
    /// Percent above the cheapest bid (0 for rank 1).
    pub above_lowest_percent: Decimal,
    pub missing_prices: usize,
    pub arithmetic_errors: usize,
    /// Positions whose LV quantity changed after this bid came in.
    pub quantity_changes: usize,
    /// Net after discount with the current LV quantities – for information
    /// when quantities changed (the rank uses the Angebotsmengen).
    pub after_discount_at_lv_quantities: Money,
    /// Every position that counts in the total has a price. Incomplete bids
    /// rank after all complete ones (fehlende Preise, § 16 VOB/A).
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceMark {
    None,
    Lowest,
    Highest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceCell {
    pub unit_price: Option<Decimal>,
    pub total: Money,
    pub mark: PriceMark,
    /// Deviation of the unit price from the reference ([`ComparisonOptions::reference`]), in percent.
    pub deviation_percent: Option<Decimal>,
    /// The same deviation in euros per unit (EP − reference EP).
    pub deviation_amount: Option<Decimal>,
    pub outlier: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionLine {
    pub row: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub quantity: Decimal,
    pub unit: String,
    pub kind: PositionKind,
    pub estimate_unit_price: Option<Decimal>,
    pub estimate_total: Money,
    /// One cell per bid, in the order of [`PriceComparison::bids`].
    pub cells: Vec<PriceCell>,
    pub min_unit_price: Option<Decimal>,
    pub max_unit_price: Option<Decimal>,
    pub mean_unit_price: Option<Decimal>,
    /// The LV quantity changed after the bids came in (first bid that shows
    /// it): Massenmehrung/-minderung, no Rechenfehler of the bidders.
    pub quantity_change: Option<QuantityChange>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupLine {
    pub row: RowId,
    pub level: usize,
    pub ordinal: String,
    pub short_text: String,
    pub estimate_total: Money,
    /// Sum per bid, in the order of [`PriceComparison::bids`].
    pub totals: Vec<Money>,
    /// Index of the cheapest bid in this group.
    pub lowest: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonLine {
    Group(GroupLine),
    Position(PositionLine),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceComparison {
    pub bids: Vec<BidSummary>,
    pub lines: Vec<ComparisonLine>,
    pub estimate_net: Money,
    pub options: ComparisonOptions,
}

impl PriceComparison {
    /// Builds the comparison for all bids that are not excluded.
    pub fn build(lv: &Lv, bids: &[Bid], options: ComparisonOptions) -> Self {
        let bids: Vec<&Bid> = bids.iter().filter(|bid| !bid.is_excluded()).collect();
        let summaries = summarize(lv, &bids, &options);

        let group_totals_per_bid: Vec<Vec<Money>> = bids
            .iter()
            .map(|bid| {
                lv.group_totals_with(
                    |position| options.counted(position, bid.total_for(position)),
                    |position| bid.unit_price(position.id()),
                )
                .into_iter()
                .map(|total| total.total)
                .collect()
            })
            .collect();
        let estimate_group_totals: Vec<Money> = lv
            .group_totals_with(
                |position| options.counted(position, position.total_price()),
                Position::unit_price,
            )
            .into_iter()
            .map(|total| total.total)
            .collect();
        // Zuschläge: base × percentage of each price source.
        let bid_surcharges: Vec<_> = bids.iter().map(|bid| bid.surcharge_values(lv)).collect();
        let estimate_surcharges = lv.surcharge_values();

        let mut lines = Vec::new();
        let mut group_number = 0;
        for row in lv.rows() {
            match row {
                LvRow::Group(group) => {
                    let totals: Vec<Money> = group_totals_per_bid
                        .iter()
                        .map(|per_group| per_group[group_number])
                        .collect();
                    // Only bidders who priced every counted position of this
                    // title compete for the cheapest title sum.
                    let block = lv.block_range(lv.index_of(group.id()).unwrap_or(0));
                    let lowest = totals
                        .iter()
                        .enumerate()
                        .filter(|(_, total)| !total.is_zero())
                        .filter(|(index, _)| {
                            lv.rows()[block.clone()]
                                .iter()
                                .filter_map(LvRow::as_position)
                                .filter(|position| options.counts(position.kind()))
                                .all(|position| bids[*index].unit_price(position.id()).is_some())
                        })
                        .min_by_key(|(_, total)| **total)
                        .map(|(index, _)| index);
                    lines.push(ComparisonLine::Group(GroupLine {
                        row: group.id(),
                        level: group.level(),
                        ordinal: group.ordinal().to_string(),
                        short_text: group.short_text().to_string(),
                        estimate_total: estimate_group_totals[group_number],
                        totals,
                        lowest,
                    }));
                    group_number += 1;
                }
                LvRow::Position(position) => {
                    let prices: Vec<Option<Decimal>> = bids
                        .iter()
                        .map(|bid| bid.unit_price(position.id()))
                        .collect();
                    let offered: Vec<Decimal> = prices.iter().flatten().copied().collect();
                    let min = offered.iter().min().copied();
                    let max = offered.iter().max().copied();
                    let mean = (!offered.is_empty()).then(|| {
                        offered.iter().copied().sum::<Decimal>() / Decimal::from(offered.len())
                    });
                    let distinct = min != max;
                    let reference = match options.reference {
                        DeviationReference::Mean => mean,
                        DeviationReference::Lowest => min,
                        DeviationReference::Estimate => position.unit_price(),
                    };
                    let comparable = match options.reference {
                        DeviationReference::Estimate => !offered.is_empty(),
                        _ => offered.len() > 1,
                    };

                    let cells = bids
                        .iter()
                        .zip(&prices)
                        .enumerate()
                        .map(|(number, (bid, price))| {
                            let deviation = match (price, reference) {
                                (Some(price), Some(base)) if !base.is_zero() => Some(
                                    ((*price - base) / base * Decimal::ONE_HUNDRED).round_dp(1),
                                ),
                                _ => None,
                            };
                            let deviation_amount = match (price, reference) {
                                (Some(price), Some(base)) => Some((*price - base).round_dp(2)),
                                _ => None,
                            };
                            let mark = match price {
                                Some(price) if distinct && Some(*price) == min => PriceMark::Lowest,
                                Some(price) if distinct && Some(*price) == max => {
                                    PriceMark::Highest
                                }
                                _ => PriceMark::None,
                            };
                            PriceCell {
                                unit_price: *price,
                                total: bid_surcharges[number]
                                    .get(&position.id())
                                    .map(|value| value.amount)
                                    .unwrap_or_else(|| bid.total_for(position)),
                                mark,
                                outlier: comparable
                                    && deviation
                                        .map(|value| value.abs() > options.outlier_percent)
                                        .unwrap_or(false),
                                deviation_percent: deviation,
                                deviation_amount,
                            }
                        })
                        .collect();

                    lines.push(ComparisonLine::Position(PositionLine {
                        row: position.id(),
                        ordinal: position.ordinal().to_string(),
                        short_text: position.short_text().to_string(),
                        quantity: position.quantity(),
                        unit: position.unit().to_string(),
                        kind: position.kind(),
                        estimate_unit_price: position.unit_price(),
                        estimate_total: estimate_surcharges
                            .get(&position.id())
                            .map(|value| value.amount)
                            .unwrap_or_else(|| position.total_price()),
                        cells,
                        min_unit_price: min,
                        max_unit_price: max,
                        mean_unit_price: mean.map(|value| value.round_dp(3)),
                        quantity_change: bids.iter().find_map(|bid| {
                            let offered = bid.offered_quantity(position);
                            (bid.unit_price(position.id()).is_some()
                                && offered != position.quantity())
                            .then(|| QuantityChange {
                                position: position.id(),
                                offered,
                                current: position.quantity(),
                            })
                        }),
                    }));
                }
                LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => {}
            }
        }

        Self {
            bids: summaries,
            lines,
            estimate_net: lv
                .row_totals_with(
                    |position| options.counted(position, position.total_price()),
                    Position::unit_price,
                )
                .into_iter()
                .sum(),
            options,
        }
    }

    pub fn lowest_bid(&self) -> Option<&BidSummary> {
        self.bids.iter().find(|summary| summary.rank == 1)
    }

    /// Positions whose LV quantity changed after the bids came in.
    pub fn quantity_changes(&self) -> Vec<&PositionLine> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                ComparisonLine::Position(position) if position.quantity_change.is_some() => {
                    Some(position)
                }
                _ => None,
            })
            .collect()
    }
}

fn summarize(lv: &Lv, bids: &[&Bid], options: &ComparisonOptions) -> Vec<BidSummary> {
    let mut summaries: Vec<BidSummary> = bids
        .iter()
        .map(|bid| {
            let net: Money = lv
                .row_totals_with(
                    |position| options.counted(position, bid.total_for(position)),
                    |position| bid.unit_price(position.id()),
                )
                .into_iter()
                .sum();
            let discount = bid.discount_amount(net);
            let net_after_discount = net - discount;
            let vat = lv.vat(net_after_discount);
            let at_lv: Money = lv
                .row_totals_with(
                    |position| {
                        options.counted(position, position.total_for(bid.unit_price(position.id())))
                    },
                    |position| bid.unit_price(position.id()),
                )
                .into_iter()
                .sum();
            BidSummary {
                bid: bid.id(),
                name: bid.name().to_string(),
                net,
                discount,
                net_after_discount,
                vat,
                gross: net_after_discount + vat,
                rank: 0,
                above_lowest_percent: Decimal::ZERO,
                missing_prices: bid.missing_positions(lv).len(),
                arithmetic_errors: bid.arithmetic_errors(lv).len(),
                quantity_changes: bid.quantity_changes(lv).len(),
                after_discount_at_lv_quantities: at_lv - bid.discount_amount(at_lv),
                complete: lv
                    .positions()
                    .filter(|position| options.counts(position.kind()))
                    .all(|position| bid.unit_price(position.id()).is_some()),
            }
        })
        .collect();

    // Complete bids first; a bid with missing prices (or none at all, e.g.
    // created by hand with prices still to come) must not become the cheapest.
    let mut order: Vec<usize> = (0..summaries.len()).collect();
    order.sort_by_key(|&index| {
        let summary = &summaries[index];
        let total = summary.net_after_discount;
        (!summary.complete, total.is_zero(), total)
    });
    let lowest = order
        .first()
        .map(|&index| summaries[index].net_after_discount)
        .unwrap_or(Money::ZERO);
    for (rank, index) in order.into_iter().enumerate() {
        let summary = &mut summaries[index];
        summary.rank = rank + 1;
        if !lowest.is_zero() {
            summary.above_lowest_percent = ((summary.net_after_discount.to_decimal()
                - lowest.to_decimal())
                / lowest.to_decimal()
                * Decimal::ONE_HUNDRED)
                .round_dp(2);
        }
    }
    summaries
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{
        ComparisonLine, ComparisonOptions, DeviationReference, PriceComparison, PriceMark,
    };
    use crate::bid::{Address, Bid, BidPrice, Discount};
    use crate::lv::{Lv, LvGroup};
    use crate::money::Money;
    use crate::oz_mask::OzMask;
    use crate::position::{Position, PositionKind};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn bid(name: &str, lv: &Lv, prices: &[&str]) -> Bid {
        let mut bid = Bid::new(Address::named(name), format!("{name}.X84"));
        for (position, price) in lv.positions().zip(prices) {
            bid.set_price(
                position.id(),
                BidPrice {
                    unit_price: (!price.is_empty()).then(|| dec(price)),
                    ..BidPrice::default()
                },
            );
        }
        bid
    }

    /// Anwender 18.11.: LV-Menge nach Angebotseingang geändert – kein
    /// Rechenfehler der Bieter, sondern Massenmehrung; Rang nach Angebotsmengen.
    #[test]
    fn quantity_changed_after_the_bids_is_no_arithmetic_error() {
        use crate::{Project, ProjectName};
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "2026-11-18", lv());
        let b = project.lv().positions().nth(1).unwrap().id();
        let mut cheap = bid("Günstig", project.lv(), &["100", "5", "50"]);
        let mut dear = bid("Teuer", project.lv(), &["90", "8", "50"]);
        for (bid, ep) in [(&mut cheap, "5"), (&mut dear, "8")] {
            let mut price = *bid.price(b).unwrap();
            price.stated_total = Some(dec(ep) * dec("10"));
            bid.set_price(b, price);
        }
        let cheap = project.add_bid(cheap);
        project.add_bid(dear);
        // B: 10 m → 30 m nach dem Angebotseingang.
        let index = project.lv().index_of(b).unwrap();
        project
            .lv_mut()
            .row_mut(index)
            .unwrap()
            .as_position_mut()
            .unwrap()
            .set_quantity(dec("30"));

        let comparison =
            PriceComparison::build(project.lv(), project.bids(), ComparisonOptions::default());
        let cheapest = comparison
            .bids
            .iter()
            .find(|summary| summary.bid == cheap)
            .unwrap();
        assert_eq!(cheapest.arithmetic_errors, 0, "kein RF");
        assert_eq!(cheapest.quantity_changes, 1);
        // Wertung mit Angebotsmengen: 2×100 + 10×5 = 250 (Teuer: 180 + 80 = 260).
        assert_eq!(cheapest.net, Money::from_cents(25_000));
        assert_eq!(cheapest.rank, 1, "Rang bleibt nach Angebotsmengen");
        // Zur Information mit LV-Menge: 200 + 30×5 = 350 (Teuer 180 + 240 = 420).
        assert_eq!(
            cheapest.after_discount_at_lv_quantities,
            Money::from_cents(35_000)
        );
        let changed = comparison.quantity_changes();
        assert_eq!(changed.len(), 1);
        let change = changed[0].quantity_change.unwrap();
        assert_eq!((change.offered, change.current), (dec("10"), dec("30")));
        assert_eq!(
            change.hint("m"),
            "Achtung Massenmehrung: LV-Menge 30 statt 10 m (+20)"
        );
        let mut less = change;
        less.current = dec("7.5");
        assert_eq!(
            less.hint("m"),
            "Achtung Massenminderung: LV-Menge 7,5 statt 10 m (-2,5)"
        );

        // Auftrag = Angebot: Auftragsmenge 10, nicht 30.
        project.award_bid(cheap, "18.11.2026").unwrap();
        let contract = crate::Contract::from_award(
            project.lv(),
            project.awarded_bid().unwrap(),
            &crate::ContractTerms::default(),
        )
        .unwrap();
        assert_eq!(contract.item(b).unwrap().quantity, dec("10"));

        // Projekte ohne Angebotsmengen (vor 18.11.) rechnen mit der LV-Menge.
        let mut old = project.bids()[0].clone();
        old.set_offered_quantity(b, None);
        assert_eq!(old.quantity_changes(project.lv()).len(), 0);
        assert_eq!(old.net_total(project.lv()), Money::from_cents(35_000));
    }

    fn lv() -> Lv {
        let mut lv = Lv::new("GA", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Titel"));
        lv.push_position(Position::new("", "A", dec("2"), "St", Some(dec("100"))));
        lv.push_position(Position::new("", "B", dec("10"), "m", Some(dec("5"))));
        let mut provisional = Position::new("", "C", dec("1"), "St", Some(dec("50")));
        provisional.set_kind(PositionKind::Provisional { with_total: false });
        lv.push_position(provisional);
        lv.renumber();
        lv
    }

    #[test]
    fn ranks_marks_and_discounts() {
        let lv = lv();
        let cheap = bid("Firma A", &lv, &["90", "6", "40"]);
        let mut middle = bid("Firma B", &lv, &["110", "4", "60"]);
        middle.set_discount(Discount::Percent(dec("10")));
        let expensive = bid("Firma C", &lv, &["150", "", "70"]);

        let comparison = PriceComparison::build(
            &lv,
            &[cheap, middle, expensive],
            ComparisonOptions::default(),
        );

        assert_eq!(comparison.estimate_net, Money::from_cents(25000));
        let a = &comparison.bids[0];
        let b = &comparison.bids[1];
        let c = &comparison.bids[2];
        assert_eq!(a.net, Money::from_cents(24000));
        assert_eq!(b.net, Money::from_cents(26000));
        assert_eq!(b.discount, Money::from_cents(2600));
        assert_eq!(b.net_after_discount, Money::from_cents(23400));
        assert_eq!(b.rank, 1);
        assert_eq!(a.rank, 2);
        assert_eq!(c.rank, 3);
        assert_eq!(c.missing_prices, 1);
        assert_eq!(a.above_lowest_percent, dec("2.56"));
        assert_eq!(b.gross, Money::from_cents(27846));

        let ComparisonLine::Position(first) = &comparison.lines[1] else {
            panic!("expected position");
        };
        assert_eq!(first.cells[0].mark, PriceMark::Lowest);
        assert_eq!(first.cells[2].mark, PriceMark::Highest);
        assert_eq!(first.mean_unit_price, Some(dec("116.667")));
        assert!(first.cells[2].outlier);
        assert!(first.cells[0].outlier);
        assert!(!first.cells[1].outlier);

        let ComparisonLine::Group(group) = &comparison.lines[0] else {
            panic!("expected group");
        };
        assert_eq!(group.totals[0], Money::from_cents(24000));
        assert_eq!(group.lowest, Some(0));
    }

    #[test]
    fn empty_bids_rank_last() {
        let lv = lv();
        let empty = Bid::new(Address::named("Leer"), "manuell");
        let comparison = PriceComparison::build(
            &lv,
            &[empty, bid("Firma", &lv, &["90", "6", "40"])],
            ComparisonOptions::default(),
        );
        assert_eq!(comparison.bids[0].rank, 2);
        assert_eq!(comparison.bids[1].rank, 1);
        assert_eq!(comparison.bids[1].above_lowest_percent, Decimal::ZERO);
        assert_eq!(comparison.bids[0].above_lowest_percent, dec("-100"));
    }

    #[test]
    fn excluded_bids_are_left_out() {
        let lv = lv();
        let mut excluded = bid("Raus", &lv, &["1", "1", "1"]);
        excluded.set_excluded(true);
        let comparison = PriceComparison::build(
            &lv,
            &[excluded, bid("Drin", &lv, &["1", "1", "1"])],
            ComparisonOptions::default(),
        );
        assert_eq!(comparison.bids.len(), 1);
        assert_eq!(comparison.lowest_bid().unwrap().name, "Drin");
    }

    #[test]
    fn arithmetic_check_uses_stated_totals() {
        let lv = lv();
        let mut checked = bid("Firma", &lv, &["100", "5", "50"]);
        let first = lv.positions().next().unwrap().id();
        checked.set_price(
            first,
            BidPrice {
                unit_price: Some(dec("100")),
                stated_total: Some(dec("210")),
                not_offered: false,
            },
        );
        assert_eq!(checked.arithmetic_errors(&lv).len(), 1);
    }

    #[test]
    fn reference_and_provisional_variant() {
        let lv = lv();
        let bids = [
            bid("Firma A", &lv, &["90", "6", "40"]),
            bid("Firma B", &lv, &["110", "4", "60"]),
        ];

        let lowest = PriceComparison::build(
            &lv,
            &bids,
            ComparisonOptions {
                reference: DeviationReference::Lowest,
                ..ComparisonOptions::default()
            },
        );
        let ComparisonLine::Position(first) = &lowest.lines[1] else {
            panic!("expected position");
        };
        assert_eq!(first.cells[0].deviation_percent, Some(dec("0")));
        assert_eq!(first.cells[1].deviation_percent, Some(dec("22.2")));
        assert_eq!(first.cells[1].deviation_amount, Some(dec("20")));
        assert!(first.cells[1].outlier);

        let estimate = PriceComparison::build(
            &lv,
            &bids,
            ComparisonOptions {
                reference: DeviationReference::Estimate,
                ..ComparisonOptions::default()
            },
        );
        let ComparisonLine::Position(first) = &estimate.lines[1] else {
            panic!("expected position");
        };
        assert_eq!(first.cells[0].deviation_percent, Some(dec("-10")));
        assert_eq!(first.cells[0].deviation_amount, Some(dec("-10")));
        assert!(!first.cells[0].outlier);

        let with_provisional = PriceComparison::build(
            &lv,
            &bids,
            ComparisonOptions {
                include_provisional: true,
                ..ComparisonOptions::default()
            },
        );
        assert_eq!(with_provisional.bids[0].net, Money::from_cents(28000));
        assert_eq!(with_provisional.estimate_net, Money::from_cents(30000));
        let ComparisonLine::Group(group) = &with_provisional.lines[0] else {
            panic!("expected group");
        };
        assert_eq!(group.totals[1], Money::from_cents(32000));
    }

    #[test]
    fn incomplete_bids_rank_after_complete_ones() {
        let lv = lv();
        let full = bid("Voll", &lv, &["100", "5", "50"]);
        let one_price = bid("Ein Preis", &lv, &["1", "", ""]);
        let comparison =
            PriceComparison::build(&lv, &[one_price, full], ComparisonOptions::default());
        let voll = comparison
            .bids
            .iter()
            .find(|bid| bid.name == "Voll")
            .unwrap();
        let teil = comparison
            .bids
            .iter()
            .find(|bid| bid.name == "Ein Preis")
            .unwrap();
        assert!(voll.complete);
        assert!(!teil.complete);
        assert_eq!(voll.rank, 1);
        assert_eq!(teil.rank, 2);
        assert_eq!(comparison.lowest_bid().unwrap().name, "Voll");
    }

    #[test]
    fn title_lowest_only_among_bidders_who_priced_the_title() {
        let lv = lv();
        let full = bid("Voll", &lv, &["100", "5", "50"]);
        let partial = bid("Teil", &lv, &["1", "", ""]);
        let comparison =
            PriceComparison::build(&lv, &[partial, full], ComparisonOptions::default());
        let ComparisonLine::Group(group) = &comparison.lines[0] else {
            panic!("expected group");
        };
        assert_eq!(group.lowest, Some(1));
    }
}
