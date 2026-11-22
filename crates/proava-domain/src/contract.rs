//! Auftrag (contract) of one LV: the state agreed with the contractor when
//! the bid was awarded – positions with contract quantity and unit price,
//! discount and contract terms. It is a snapshot, so later edits of the LV
//! or the bid do not change what was agreed. Change orders (Nachträge) and
//! billing (Abrechnung) build on it.

use std::collections::BTreeMap;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::bid::{Bid, BidId, Discount};
use crate::lv::{Lv, RowId};
use crate::money::Money;
use crate::position::PositionKind;
use crate::project::{ContractTerms, Project};

/// One position of the contract (Auftragsposition).
#[derive(Debug, Clone, PartialEq)]
pub struct ContractItem {
    /// Order number and short text at the time of the snapshot (for print
    /// and checks when the LV row changed or is gone).
    pub ordinal: String,
    pub short_text: String,
    pub unit: String,
    /// Contract quantity (beauftragte Menge); 1 for lump sums.
    pub quantity: Decimal,
    /// Contract unit price (EP des Auftrags), net before discount.
    pub unit_price: Decimal,
    pub kind: PositionKind,
    pub lump_sum: bool,
    pub hourly: bool,
}

impl ContractItem {
    /// GP of the contract quantity, rounded to cents.
    pub fn total(&self) -> Money {
        Money::total(self.unit_price, self.quantity)
    }

    /// Whether it counts in the contract sum (Bedarfspositionen ohne GB and
    /// Wahlpositionen do not).
    pub fn counts(&self) -> bool {
        self.kind.counts_in_total()
    }
}

/// The contract of one LV with the awarded contractor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contract {
    /// Contractor (Auftragnehmer), as named in the bid.
    pub contractor: String,
    /// Bid the contract came from, if it still exists.
    pub bid: Option<BidId>,
    /// Date of award (TT.MM.JJJJ).
    pub date: String,
    /// Contract number of the client (Auftrags-Nr.), free text.
    pub number: String,
    /// Positions by LV row id.
    pub items: BTreeMap<RowId, ContractItem>,
    /// Discount (Nachlass) in percent, applied to every billed amount. An
    /// amount discount of the bid is converted to percent (GAEB DA XML 3.3,
    /// 8.1.3.6), six decimals.
    pub discount_percent: Decimal,
    /// Contract sum net after discount as awarded (Auftragssumme).
    pub total: Money,
    /// Terms for this contract (security, deductions, cash discount);
    /// copied from the project when the contract is taken over.
    pub terms: ContractTerms,
}

impl Contract {
    /// Snapshot of the awarded bid: every position the bid priced, except
    /// positions marked „entfällt“ (they are not part of the contract and
    /// not in the award sum either).
    pub fn from_award(lv: &Lv, bid: &Bid, terms: &ContractTerms) -> Result<Self, String> {
        let award = bid
            .award()
            .ok_or_else(|| format!("Das Angebot von {} ist nicht beauftragt.", bid.name()))?;
        let mut items = BTreeMap::new();
        for position in lv
            .positions()
            .filter(|position| !position.is_not_applicable())
        {
            let Some(unit_price) = bid.unit_price(position.id()) else {
                continue;
            };
            items.insert(
                position.id(),
                ContractItem {
                    ordinal: position.ordinal().to_string(),
                    short_text: position.short_text().to_string(),
                    unit: position.unit().to_string(),
                    // Auftrag = Angebot: a quantity changed in the LV after the
                    // bid came in is a Mehr-/Mindermenge (Nachtrag).
                    quantity: bid.offered_quantity(position),
                    unit_price,
                    kind: position.kind(),
                    lump_sum: position.is_lump_sum(),
                    hourly: position.is_hourly(),
                },
            );
        }
        if items.is_empty() {
            return Err("Das beauftragte Angebot hat keine Preise.".to_string());
        }
        let net = bid.net_total(lv);
        Ok(Self {
            contractor: bid.name().to_string(),
            bid: Some(bid.id()),
            date: award.date.clone(),
            number: String::new(),
            items,
            discount_percent: discount_percent(bid.discount(), net),
            total: award.total,
            terms: terms.clone(),
        })
    }

    /// Net sum of the counted positions before discount.
    pub fn net_before_discount(&self) -> Money {
        self.items
            .values()
            .filter(|item| item.counts())
            .map(ContractItem::total)
            .sum()
    }

    pub fn item(&self, position: RowId) -> Option<&ContractItem> {
        self.items.get(&position)
    }
}

/// Discount of a bid as percent of its net sum (six decimals).
pub fn discount_percent(discount: Discount, net: Money) -> Decimal {
    match discount {
        Discount::None => Decimal::ZERO,
        Discount::Percent(percent) => percent,
        Discount::Amount(amount) if !net.is_zero() => (amount.to_decimal() / net.to_decimal()
            * Decimal::ONE_HUNDRED)
            .round_dp_with_strategy(6, RoundingStrategy::MidpointAwayFromZero),
        Discount::Amount(_) => Decimal::ZERO,
    }
}

impl Project {
    /// Takes the awarded bid of the active LV over as its contract (with the
    /// project's contract terms); replaces an earlier contract snapshot.
    pub fn take_over_contract(&mut self) -> Result<&Contract, String> {
        let bid = self
            .awarded_bid()
            .ok_or("Im LV ist noch kein Angebot beauftragt.")?
            .clone();
        let contract = Contract::from_award(self.lv(), &bid, &self.info().terms)?;
        self.set_contract(Some(contract));
        Ok(self.contract().expect("just set"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Address, LvGroup, OzMask, Position, ProjectName};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    #[test]
    fn snapshot_of_the_awarded_bid() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        let a = lv.push_position(Position::new("01.0010", "A", dec("10"), "St", None));
        let b = lv.push_position(Position::new("01.0020", "B", dec("2"), "m", None));
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
        project.info_mut().terms.performance_security_percent = Some(dec("10"));
        let mut bid = Bid::new(Address::named("Firma"), "f.x84");
        bid.set_unit_price(a, Some(dec("12.50")));
        bid.set_unit_price(b, Some(dec("100")));
        bid.set_discount(Discount::Amount(Money::from_cents(1_000)));
        let id = project.add_bid(bid);
        assert!(project.take_over_contract().is_err(), "noch kein Auftrag");
        project.award_bid(id, "01.12.2026").unwrap();
        let contract = project.take_over_contract().unwrap().clone();
        assert_eq!(contract.contractor, "Firma");
        assert_eq!(contract.items.len(), 2);
        assert_eq!(contract.item(a).unwrap().unit_price, dec("12.50"));
        // 10 € of 325 € = 3,076923 %
        assert_eq!(contract.discount_percent, dec("3.076923"));
        assert_eq!(contract.net_before_discount(), Money::from_cents(32_500));
        assert_eq!(contract.total, Money::from_cents(31_500));
        assert_eq!(contract.terms.performance_security_percent, Some(dec("10")));
        // The snapshot stays when the LV changes.
        let index = project.lv().index_of(a).unwrap();
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(|row| row.as_position_mut())
        {
            position.set_quantity(dec("99"));
        }
        assert_eq!(
            project.contract().unwrap().item(a).unwrap().quantity,
            dec("10")
        );
    }

    #[test]
    fn positions_marked_entfaellt_are_not_in_the_contract() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        let a = lv.push_position(Position::new("01.0010", "A", dec("10"), "St", None));
        let mut gone = Position::new("01.0020", "B", dec("1"), "St", None);
        gone.set_not_applicable(true);
        let b = lv.push_position(gone);
        let mut project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
        let mut bid = Bid::new(Address::named("Firma"), "f.x84");
        bid.set_unit_price(a, Some(dec("10")));
        bid.set_unit_price(b, Some(dec("50")));
        let id = project.add_bid(bid);
        project.award_bid(id, "01.12.2026").unwrap();
        let contract = project.take_over_contract().unwrap().clone();
        assert!(contract.item(b).is_none(), "entfällt");
        assert_eq!(
            contract.net_before_discount(),
            contract.total,
            "Auftragssumme = Summe der Positionen"
        );
    }

    #[test]
    fn discount_in_percent() {
        assert_eq!(
            discount_percent(Discount::None, Money::from_cents(100)),
            Decimal::ZERO
        );
        assert_eq!(
            discount_percent(Discount::Percent(dec("2.5")), Money::from_cents(100)),
            dec("2.5")
        );
        assert_eq!(
            discount_percent(Discount::Amount(Money::from_cents(100)), Money::ZERO),
            Decimal::ZERO
        );
    }
}
