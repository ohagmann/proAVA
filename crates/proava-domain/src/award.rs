//! Vergabe-Abschluss: awarding a bid and carrying prices from the bids
//! into the planner's prices (Kostenvoranschlag fortschreiben).

use rust_decimal::Decimal;

use crate::bid::{Award, Bid, BidId};
use crate::money::Money;
use crate::project::Project;

/// Where adopted planner prices come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    /// The unit prices of one bid.
    Bid(BidId),
    /// Mean of all bids that are not excluded and priced the position.
    Mean,
    /// Median of those bids.
    Median,
    /// Lowest of those bids.
    Minimum,
}

impl PriceSource {
    pub fn label(self) -> &'static str {
        match self {
            PriceSource::Bid(_) => "aus einem Angebot",
            PriceSource::Mean => "Mittelwert",
            PriceSource::Median => "Median",
            PriceSource::Minimum => "Minimum",
        }
    }
}

/// Result of [`Project::adopt_prices`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PriceAdoption {
    /// Positions whose planner price changed.
    pub changed: usize,
    /// Positions left alone because they already had a price.
    pub kept: usize,
    /// Order numbers without any price in the source.
    pub without_price: Vec<String>,
}

impl Project {
    /// The awarded bid of the active LV, if any.
    pub fn awarded_bid(&self) -> Option<&Bid> {
        self.bids().iter().find(|bid| bid.is_awarded())
    }

    /// Awards the contract to a bid of the active LV on `date` (TT.MM.JJJJ):
    /// the contract sum is the bid's net after discount. Another awarded
    /// bid of the same LV loses its award. Returns the contract sum.
    pub fn award_bid(&mut self, id: BidId, date: &str) -> Result<Money, String> {
        let lv = self.lv().clone();
        let bid = self
            .bids()
            .iter()
            .find(|bid| bid.id() == id)
            .ok_or("Das Angebot gibt es nicht (mehr).")?;
        if bid.is_excluded() {
            return Err(format!(
                "Das Angebot von {} ist ausgeschlossen – erst wieder werten, dann beauftragen.",
                bid.name()
            ));
        }
        let total = bid.total_after_discount(&lv);
        let others: Vec<BidId> = self
            .bids()
            .iter()
            .filter(|other| other.id() != id && other.is_awarded())
            .map(Bid::id)
            .collect();
        for other in others {
            if let Some(other) = self.bid_mut(other) {
                other.set_award(None);
            }
        }
        let bid = self.bid_mut(id).expect("checked above");
        bid.set_award(Some(Award {
            date: date.trim().to_string(),
            total,
        }));
        Ok(total)
    }

    /// Takes the award back; returns the bid that had it.
    pub fn revoke_award(&mut self) -> Option<BidId> {
        let id = self.awarded_bid()?.id();
        self.bid_mut(id)?.set_award(None);
        Some(id)
    }

    /// Carries unit prices from the bids into the planner's prices of the
    /// active LV. `overwrite` = also replace prices already set. Positions
    /// without a price in the source stay as they are. One call is one
    /// change, so the caller records one undo step before it.
    pub fn adopt_prices(
        &mut self,
        source: PriceSource,
        overwrite: bool,
    ) -> Result<PriceAdoption, String> {
        let bids: Vec<Bid> = match source {
            PriceSource::Bid(id) => vec![self
                .bids()
                .iter()
                .find(|bid| bid.id() == id)
                .cloned()
                .ok_or("Das Angebot gibt es nicht (mehr).")?],
            _ => self
                .bids()
                .iter()
                .filter(|bid| !bid.is_excluded())
                .cloned()
                .collect(),
        };
        if bids.is_empty() {
            return Err("Es gibt kein gewertetes Angebot.".to_string());
        }
        let mut result = PriceAdoption::default();
        let ids: Vec<_> = self
            .lv()
            .positions()
            .map(|position| position.id())
            .collect();
        for id in ids {
            let mut prices: Vec<Decimal> =
                bids.iter().filter_map(|bid| bid.unit_price(id)).collect();
            prices.sort();
            let price = match source {
                PriceSource::Bid(_) | PriceSource::Minimum => prices.first().copied(),
                PriceSource::Mean => (!prices.is_empty()).then(|| {
                    (prices.iter().sum::<Decimal>() / Decimal::from(prices.len())).round_dp(2)
                }),
                PriceSource::Median => median(&prices),
            };
            let lv = self.lv_mut();
            let index = lv.index_of(id).expect("position exists");
            let position = lv
                .row_mut(index)
                .and_then(|row| row.as_position_mut())
                .expect("position row");
            let Some(price) = price else {
                result.without_price.push(position.ordinal().to_string());
                continue;
            };
            if position.unit_price().is_some() && !overwrite {
                result.kept += 1;
                continue;
            }
            if position.unit_price() != Some(price) {
                position.set_unit_price(Some(price));
                result.changed += 1;
            }
        }
        Ok(result)
    }
}

fn median(sorted: &[Decimal]) -> Option<Decimal> {
    let count = sorted.len();
    if count == 0 {
        return None;
    }
    Some(if count % 2 == 1 {
        sorted[count / 2]
    } else {
        ((sorted[count / 2 - 1] + sorted[count / 2]) / Decimal::TWO).round_dp(2)
    })
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::PriceSource;
    use crate::{
        Address, Bid, BidId, Discount, Lv, LvGroup, Money, OzMask, Position, Project, ProjectName,
    };

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    /// Three positions, three bids; position 3 is priced by one bid only.
    fn project() -> (Project, [BidId; 3]) {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        let a = lv.push_position(Position::new(
            "01.0010",
            "A",
            dec("2"),
            "St",
            Some(dec("5")),
        ));
        let b = lv.push_position(Position::new("01.0020", "B", dec("1"), "St", None));
        let c = lv.push_position(Position::new("01.0030", "C", dec("1"), "St", None));
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
        let mut ids = [BidId::default(); 3];
        for (index, (name, pa, pb)) in [("X", "10", "1"), ("Y", "20", "2"), ("Z", "40", "4")]
            .into_iter()
            .enumerate()
        {
            let mut bid = Bid::new(Address::named(name), format!("{name}.x84"));
            bid.set_unit_price(a, Some(dec(pa)));
            bid.set_unit_price(b, Some(dec(pb)));
            if index == 0 {
                bid.set_unit_price(c, Some(dec("7")));
                bid.set_discount(Discount::Percent(dec("10")));
            }
            ids[index] = project.add_bid(bid);
        }
        (project, ids)
    }

    #[test]
    fn only_one_bid_is_awarded_with_its_sum_after_discount() {
        let (mut project, [x, y, z]) = project();
        // X: 2×10 + 1 + 7 = 28, minus 10 % = 25,20
        assert_eq!(
            project.award_bid(x, "01.12.2026").unwrap(),
            Money::from_cents(2520)
        );
        assert_eq!(project.awarded_bid().unwrap().id(), x);
        assert_eq!(
            project.award_bid(y, "02.12.2026").unwrap(),
            Money::from_cents(4200)
        );
        assert_eq!(project.awarded_bid().unwrap().id(), y);
        assert_eq!(
            project.bids().iter().filter(|bid| bid.is_awarded()).count(),
            1
        );
        assert_eq!(
            project.awarded_bid().unwrap().award().unwrap().date,
            "02.12.2026"
        );
        project.bid_mut(z).unwrap().set_excluded(true);
        assert!(project.award_bid(z, "").is_err());
        assert!(project.award_bid(BidId(99), "").is_err());
        assert_eq!(project.revoke_award(), Some(y));
        assert!(project.awarded_bid().is_none());
    }

    #[test]
    fn prices_from_a_bid_mean_median_minimum() {
        let (base, [x, _, z]) = project();
        let prices = |project: &Project| -> Vec<Option<Decimal>> {
            project
                .lv()
                .positions()
                .map(|position| position.unit_price())
                .collect()
        };

        let mut project = base.clone();
        let result = project.adopt_prices(PriceSource::Bid(x), false).unwrap();
        assert_eq!(
            prices(&project),
            [Some(dec("5")), Some(dec("1")), Some(dec("7"))]
        );
        assert_eq!((result.changed, result.kept), (2, 1));

        let mut project = base.clone();
        project.adopt_prices(PriceSource::Mean, true).unwrap();
        assert_eq!(
            prices(&project),
            [Some(dec("23.33")), Some(dec("2.33")), Some(dec("7"))]
        );

        let mut project = base.clone();
        project.adopt_prices(PriceSource::Median, true).unwrap();
        assert_eq!(
            prices(&project),
            [Some(dec("20")), Some(dec("2")), Some(dec("7"))]
        );

        // Excluded bids do not count; a bid without price leaves the position.
        let mut project = base.clone();
        project.bid_mut(x).unwrap().set_excluded(true);
        let result = project.adopt_prices(PriceSource::Minimum, true).unwrap();
        assert_eq!(prices(&project), [Some(dec("20")), Some(dec("2")), None]);
        assert_eq!(result.without_price, ["01.0030"]);
        let result = project.adopt_prices(PriceSource::Bid(z), true).unwrap();
        assert_eq!(result.changed, 2);
        assert!(project
            .adopt_prices(PriceSource::Bid(BidId(99)), true)
            .is_err());
    }
}
