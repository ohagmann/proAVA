use std::collections::HashMap;

use rust_decimal::Decimal;

use crate::lv::{Lv, RowId};
use crate::money::Money;
use crate::position::Position;

/// Identity of a bid inside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BidId(pub u64);

/// An address with the fields of GAEB DA XML 3.3 (tgAddress).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Address {
    pub name1: String,
    pub name2: String,
    pub name3: String,
    pub name4: String,
    pub street: String,
    pub postal_code: String,
    pub city: String,
    /// Land (GAEB Country).
    pub country: String,
    /// International Location Number (ILN/GLN, GAEB ILN).
    pub iln: String,
    /// Ansprechpartner (GAEB Contact).
    pub contact: String,
    pub phone: String,
    pub fax: String,
    pub email: String,
    /// Umsatzsteuer-Identifikationsnummer (GAEB VATID).
    pub vat_id: String,
}

impl Address {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name1: name.into(),
            ..Self::default()
        }
    }

    /// Every filled field in one line: names, street, PLZ Ort, Land, then
    /// „Ansprechpartner …“, „Tel. …“, „Fax …“, „E-Mail …“, „USt-IdNr. …“,
    /// „ILN …“.
    pub fn details(&self) -> String {
        let city = format!("{} {}", self.postal_code, self.city);
        let mut pieces: Vec<String> = [
            self.name1.as_str(),
            self.name2.as_str(),
            self.name3.as_str(),
            self.name4.as_str(),
            self.street.as_str(),
            city.trim(),
            self.country.as_str(),
        ]
        .into_iter()
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(str::to_string)
        .collect();
        for (label, value) in [
            ("Ansprechpartner", &self.contact),
            ("Tel.", &self.phone),
            ("Fax", &self.fax),
            ("E-Mail", &self.email),
            ("USt-IdNr.", &self.vat_id),
            ("ILN", &self.iln),
        ] {
            if !value.trim().is_empty() {
                pieces.push(format!("{label} {}", value.trim()));
            }
        }
        pieces.join(", ")
    }

    pub fn one_line(&self) -> String {
        let city = format!("{} {}", self.postal_code, self.city);
        [self.name1.as_str(), self.street.as_str(), city.trim()]
            .into_iter()
            .filter(|piece| !piece.trim().is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A bidder's price for one position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BidPrice {
    pub unit_price: Option<Decimal>,
    /// Total price as written by the bidder (for the arithmetic check).
    pub stated_total: Option<Decimal>,
    pub not_offered: bool,
}

/// The award of a bid (Auftragserteilung / Zuschlag).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Award {
    /// Date of the award, TT.MM.JJJJ as the user writes it.
    pub date: String,
    /// Contract sum net after discount (Auftragssumme) at the time of award.
    pub total: Money,
}

/// What the bidder filled into a Textergänzung (Fabrikat, Typ …).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BidComplement {
    /// Marker of the gap in the LV position, e.g. "01".
    pub marker: String,
    pub text: String,
}

/// Discount granted on the whole bid (Nachlass).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Discount {
    #[default]
    None,
    Percent(Decimal),
    Amount(Money),
}

/// One bid (Angebot) of one bidder for the project's LV, usually read from
/// an X84/D84 file. Prices are linked to LV positions by row id.
#[derive(Debug, Clone, PartialEq)]
pub struct Bid {
    id: BidId,
    bidder: Address,
    /// Where it came from (file name), for the user.
    source: String,
    bid_date: Option<String>,
    /// Datenaustauschnummer of the bidder (GAEB CTR/DPNo).
    exchange_number: String,
    pub(crate) prices: HashMap<RowId, BidPrice>,
    discount: Discount,
    /// Excluded from the comparison (e.g. formally excluded bid).
    excluded: bool,
    /// Order numbers in the bid file that the LV does not know.
    pub(crate) unknown_ordinals: Vec<String>,
    /// Textergänzungen filled in by the bidder, per position.
    pub(crate) complements: HashMap<RowId, Vec<BidComplement>>,
    /// Set when this bid got the contract; at most one per LV
    /// (see [`crate::Project::award_bid`]).
    award: Option<Award>,
    /// LV quantity of each priced position when the bid came in
    /// (Angebotsmenge, the Vordersatz the bidder priced). Taken by
    /// [`crate::Project::add_bid`]; bids saved before this existed have none
    /// and are computed with the current LV quantity.
    pub(crate) offered_quantities: HashMap<RowId, Decimal>,
    /// Abweichungsprotokoll: what the file changed against the LV.
    pub(crate) structure: crate::bid_structure::StructureReport,
}

/// A position whose LV quantity was changed after the bid came in
/// (Massenmehrung/-minderung nach Angebotseingang).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantityChange {
    pub position: RowId,
    /// Quantity the bid was priced with.
    pub offered: Decimal,
    /// Quantity in the LV now.
    pub current: Decimal,
}

impl QuantityChange {
    /// Positive = Mehrung, negative = Minderung.
    pub fn difference(&self) -> Decimal {
        self.current - self.offered
    }

    pub fn is_increase(&self) -> bool {
        self.current > self.offered
    }

    /// „Massenmehrung“ or „Massenminderung“.
    pub fn label(&self) -> &'static str {
        if self.is_increase() {
            "Massenmehrung"
        } else {
            "Massenminderung"
        }
    }

    /// „Achtung Massenmehrung: LV-Menge 12 statt 10 St (+2)“ (Anwender 18.11.).
    pub fn hint(&self, unit: &str) -> String {
        let number = |value: Decimal| {
            crate::money::format_decimal_de(value.normalize(), value.normalize().scale())
        };
        let difference = self.difference();
        let sign = if difference.is_sign_negative() {
            ""
        } else {
            "+"
        };
        format!(
            "Achtung {}: LV-Menge {} statt {} {} ({sign}{})",
            self.label(),
            number(self.current),
            number(self.offered),
            unit.trim(),
            number(difference)
        )
        .replace("  (", " (")
    }
}

impl Bid {
    pub fn new(bidder: Address, source: impl Into<String>) -> Self {
        Self {
            id: BidId::default(),
            bidder,
            source: source.into(),
            bid_date: None,
            exchange_number: String::new(),
            prices: HashMap::new(),
            discount: Discount::None,
            excluded: false,
            unknown_ordinals: Vec::new(),
            complements: HashMap::new(),
            award: None,
            offered_quantities: HashMap::new(),
            structure: crate::bid_structure::StructureReport::default(),
        }
    }

    /// Abweichungsprotokoll (Strukturvergleich beim Einlesen).
    pub fn structure(&self) -> &crate::bid_structure::StructureReport {
        &self.structure
    }

    pub fn set_structure(&mut self, structure: crate::bid_structure::StructureReport) {
        self.structure = structure;
    }

    /// Quantity this bid was priced with for `position` (see
    /// [`Bid::offered_quantities`]); the LV quantity when none was taken.
    pub fn offered_quantity(&self, position: &Position) -> Decimal {
        self.offered_quantities
            .get(&position.id())
            .copied()
            .unwrap_or_else(|| position.quantity())
    }

    /// Angebotsmengen per position (saved with the project).
    pub fn offered_quantities(&self) -> &HashMap<RowId, Decimal> {
        &self.offered_quantities
    }

    pub fn set_offered_quantity(&mut self, position: RowId, quantity: Option<Decimal>) {
        match quantity {
            Some(quantity) => {
                self.offered_quantities.insert(position, quantity);
            }
            None => {
                self.offered_quantities.remove(&position);
            }
        }
    }

    /// Takes the current LV quantity as Angebotsmenge for every priced
    /// position that has none yet (when the bid comes in).
    pub fn take_offered_quantities(&mut self, lv: &Lv) {
        for position in lv.positions() {
            if self.prices.contains_key(&position.id()) {
                self.offered_quantities
                    .entry(position.id())
                    .or_insert_with(|| position.quantity());
            }
        }
    }

    /// Priced positions whose LV quantity differs from the Angebotsmenge
    /// (changed after the bid came in), in LV order.
    pub fn quantity_changes(&self, lv: &Lv) -> Vec<QuantityChange> {
        lv.positions()
            .filter(|position| self.unit_price(position.id()).is_some())
            .filter_map(|position| {
                let offered = *self.offered_quantities.get(&position.id())?;
                (offered != position.quantity()).then(|| QuantityChange {
                    position: position.id(),
                    offered,
                    current: position.quantity(),
                })
            })
            .collect()
    }

    /// Net sum with the current LV quantities and this bid's unit prices –
    /// for information when quantities changed after the bid came in (the
    /// Wertung uses the Angebotsmengen, see [`Bid::total_for`]).
    pub fn net_total_at_lv_quantities(&self, lv: &Lv) -> Money {
        lv.row_totals_with(
            |position| {
                if position.counts_in_total() {
                    position.total_for(self.unit_price(position.id()))
                } else {
                    Money::ZERO
                }
            },
            |position| self.unit_price(position.id()),
        )
        .into_iter()
        .sum()
    }

    /// Bezugssumme and Zuschlag of every Zuschlagsposition with this bid's
    /// prices; the bidder's „unit price“ of a Zuschlagsposition is the
    /// percentage (GAEB X84 Markup).
    pub fn surcharge_values(&self, lv: &Lv) -> HashMap<RowId, crate::surcharge::SurchargeValue> {
        crate::surcharge::values_with(
            lv,
            &|position: &Position| self.counted_total_for(position),
            &|position: &Position| self.unit_price(position.id()),
        )
    }

    pub fn id(&self) -> BidId {
        self.id
    }

    /// Same bid asking for `id`; [`crate::Project::add_bid`] keeps it when it
    /// is free (used when loading a saved project).
    pub fn with_id(mut self, id: BidId) -> Self {
        self.id = id;
        self
    }

    pub(crate) fn set_id(&mut self, id: BidId) {
        self.id = id;
    }

    pub fn bidder(&self) -> &Address {
        &self.bidder
    }

    pub fn bidder_mut(&mut self) -> &mut Address {
        &mut self.bidder
    }

    pub fn name(&self) -> &str {
        &self.bidder.name1
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn bid_date(&self) -> Option<&str> {
        self.bid_date.as_deref()
    }

    pub fn set_bid_date(&mut self, date: Option<String>) {
        self.bid_date = date;
    }

    /// Datenaustauschnummer of the bidder (GAEB CTR/DPNo).
    pub fn exchange_number(&self) -> &str {
        &self.exchange_number
    }

    pub fn set_exchange_number(&mut self, number: impl Into<String>) {
        self.exchange_number = number.into();
    }

    pub fn price(&self, position: RowId) -> Option<&BidPrice> {
        self.prices.get(&position)
    }

    pub fn prices(&self) -> &HashMap<RowId, BidPrice> {
        &self.prices
    }

    pub fn set_price(&mut self, position: RowId, price: BidPrice) {
        self.prices.insert(position, price);
    }

    /// Sets or clears the unit price of one position by hand (Aufklärung,
    /// Nachverhandlung). The total stated in the bid file no longer applies,
    /// so the arithmetic check ends for this position; "not offered" ends too.
    pub fn set_unit_price(&mut self, position: RowId, unit_price: Option<Decimal>) {
        let entry = self.prices.entry(position).or_default();
        entry.unit_price = unit_price;
        entry.stated_total = None;
        entry.not_offered = false;
    }

    pub fn discount(&self) -> Discount {
        self.discount
    }

    pub fn set_discount(&mut self, discount: Discount) {
        self.discount = discount;
    }

    pub fn is_excluded(&self) -> bool {
        self.excluded
    }

    pub fn set_excluded(&mut self, excluded: bool) {
        self.excluded = excluded;
    }

    pub fn unknown_ordinals(&self) -> &[String] {
        &self.unknown_ordinals
    }

    pub fn set_unknown_ordinals(&mut self, ordinals: Vec<String>) {
        self.unknown_ordinals = ordinals;
    }

    /// The bidder's Textergänzungen of one position (empty when none).
    pub fn complements_of(&self, position: RowId) -> &[BidComplement] {
        self.complements
            .get(&position)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn complements(&self) -> &HashMap<RowId, Vec<BidComplement>> {
        &self.complements
    }

    /// Replaces the Textergänzungen of one position; empty texts are dropped.
    pub fn set_complements(&mut self, position: RowId, complements: Vec<BidComplement>) {
        let kept: Vec<BidComplement> = complements
            .into_iter()
            .filter(|complement| !complement.text.trim().is_empty())
            .collect();
        if kept.is_empty() {
            self.complements.remove(&position);
        } else {
            self.complements.insert(position, kept);
        }
    }

    /// One line per complement, "Fabrikat: Siemens", using the captions of
    /// the LV position where the markers match.
    pub fn complement_lines(&self, position: &Position) -> Vec<String> {
        self.complements_of(position.id())
            .iter()
            .map(|complement| {
                let caption = position
                    .complements()
                    .iter()
                    .find(|gap| gap.marker.trim() == complement.marker.trim())
                    .map(|gap| gap.caption.trim().to_string())
                    .filter(|caption| !caption.is_empty());
                match caption {
                    Some(caption) => format!("{caption} {}", complement.text.trim()),
                    None => complement.text.trim().to_string(),
                }
            })
            .collect()
    }

    pub fn award(&self) -> Option<&Award> {
        self.award.as_ref()
    }

    pub fn is_awarded(&self) -> bool {
        self.award.is_some()
    }

    /// Sets the award without the one-per-LV check; inside a project use
    /// [`crate::Project::award_bid`].
    pub fn set_award(&mut self, award: Option<Award>) {
        self.award = award;
    }

    /// Net sum after discount (what the contract sum is computed from).
    pub fn total_after_discount(&self, lv: &Lv) -> Money {
        let net = self.net_total(lv);
        net - self.discount_amount(net)
    }

    pub fn unit_price(&self, position: RowId) -> Option<Decimal> {
        self.prices
            .get(&position)
            .filter(|price| !price.not_offered)
            .and_then(|price| price.unit_price)
    }

    /// GP of one position with this bidder's unit price (recomputed, not
    /// stated) and the quantity the bid was priced with. The Wertung (sums,
    /// rank, Auftragssumme) stays with the offer as submitted when the LV
    /// quantity changes later (Anwender 18.11.: that is a Mehr-/Mindermenge for
    /// a Nachtrag, not a different offer; it also cannot shift the ranking).
    pub fn total_for(&self, position: &Position) -> Money {
        self.unit_price(position.id())
            .map(|price| Money::total(price, self.offered_quantity(position)))
            .unwrap_or(Money::ZERO)
    }

    pub fn counted_total_for(&self, position: &Position) -> Money {
        if position.counts_in_total() {
            self.total_for(position)
        } else {
            Money::ZERO
        }
    }

    /// Net sum before discount, recomputed from the unit prices (Zuschläge
    /// with the bidder's percentages on the bidder's totals).
    pub fn net_total(&self, lv: &Lv) -> Money {
        lv.row_totals_with(
            |position| self.counted_total_for(position),
            |position| self.unit_price(position.id()),
        )
        .into_iter()
        .sum()
    }

    pub fn discount_amount(&self, net: Money) -> Money {
        match self.discount {
            Discount::None => Money::ZERO,
            Discount::Percent(percent) => net.percent(percent),
            Discount::Amount(amount) => amount,
        }
    }

    /// Positions of the LV without a price in this bid.
    pub fn missing_positions<'a>(&self, lv: &'a Lv) -> Vec<&'a Position> {
        lv.positions()
            .filter(|position| self.unit_price(position.id()).is_none())
            .collect()
    }

    /// Positions where the stated GP differs from quantity × EP by more than one cent.
    pub fn arithmetic_errors<'a>(&self, lv: &'a Lv) -> Vec<(&'a Position, Money, Money)> {
        let surcharges = self.surcharge_values(lv);
        lv.positions()
            .filter_map(|position| {
                let price = self.prices.get(&position.id())?;
                let stated = Money::from_decimal(price.stated_total?);
                // A quantity of the bidder other than the LV's is no
                // Rechenfehler: his GP is checked against his quantity.
                let bidder_quantity = self
                    .structure
                    .quantities
                    .iter()
                    .find(|(row, _)| *row == position.id())
                    .map(|(_, quantity)| *quantity);
                let computed = match (surcharges.get(&position.id()), bidder_quantity) {
                    (Some(value), _) => value.amount,
                    (None, Some(quantity)) => self
                        .unit_price(position.id())
                        .map(|price| Money::total(price, quantity))
                        .unwrap_or(Money::ZERO),
                    (None, None) => self.total_for(position),
                };
                ((stated - computed).cents().abs() > 1).then_some((position, stated, computed))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Address, Bid, BidPrice};
    use crate::lv::RowId;

    #[test]
    fn unit_price_by_hand_replaces_the_file_values() {
        let mut bid = Bid::new(Address::named("Firma"), "manuell");
        let row = RowId(7);
        bid.set_price(
            row,
            BidPrice {
                unit_price: None,
                stated_total: Some(Decimal::from(20)),
                not_offered: true,
            },
        );
        bid.set_unit_price(row, Some(Decimal::from(10)));
        assert_eq!(bid.unit_price(row), Some(Decimal::from(10)));
        assert_eq!(bid.price(row).unwrap().stated_total, None);
        assert!(!bid.price(row).unwrap().not_offered);

        bid.set_unit_price(RowId(8), Some(Decimal::ONE));
        assert_eq!(bid.unit_price(RowId(8)), Some(Decimal::ONE));
        bid.set_unit_price(row, None);
        assert_eq!(bid.unit_price(row), None);
    }
}
