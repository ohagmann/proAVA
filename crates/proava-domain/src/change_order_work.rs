//! Working on Nachträge in the LV: the Nachtragsbereich (titles at the end
//! of the LV that hold the new positions of a change order), assigning
//! positions, price hints for the check, sums by status, and taking over
//! the contractor's offer read from a file.
//!
//! New rows only ever go to the end of a title block (or of the LV), so
//! the order numbers of the contract positions never change.

use rust_decimal::{Decimal, RoundingStrategy};

use crate::change_order::{
    ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus, ItemChange,
};
use crate::contract::Contract;
use crate::invitation::name_similarity;
use crate::lv::{normalize_ordinal, Lv, LvGroup, LvRow, RowId};
use crate::money::Money;
use crate::position::Position;
use crate::project::Project;
use crate::transfer::{self, TransferOptions};

/// Short text of the collecting title for Nachträge (masks with two or more
/// title levels): below it one title per Nachtrag.
pub const COLLECTOR_TITLE: &str = "Nachträge";

/// Where the next new position of a change order goes and which order
/// number it would get.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaProposal {
    /// Title of the change order (existing or to be created), e.g. „90.01 N01 Wetterstation“.
    pub title: String,
    pub ordinal: String,
}

fn group_level_zero_collector(lv: &Lv) -> Option<usize> {
    lv.rows().iter().position(|row| {
        matches!(row, LvRow::Group(group)
            if group.level() == 0 && group.short_text().trim().eq_ignore_ascii_case(COLLECTOR_TITLE))
    })
}

fn insert_group(lv: &mut Lv, index: usize, level: usize, text: &str) -> Result<usize, String> {
    let probe = LvRow::Group(LvGroup::new(level, "", text));
    let ordinal = lv.free_ordinal_at(index, &probe).ok_or_else(|| {
        "Für den Nachtragsbereich ist am Ende des LV keine Ordnungszahl frei.".to_string()
    })?;
    lv.insert_row(index, LvRow::Group(LvGroup::new(level, ordinal, text)));
    Ok(index)
}

/// Title text of a change order in the LV: „N01 Wetterstation“.
pub fn area_title(order: &ChangeOrder) -> String {
    format!("{} {}", order.label(), order.title.trim())
        .trim()
        .to_string()
}

/// Creates the titles of the Nachtragsbereich at the end of `lv` (with two
/// or more title levels inside the collecting title „Nachträge“) and
/// returns the index of the deepest one; None for masks without titles.
fn create_area(lv: &mut Lv, order: &ChangeOrder) -> Result<Option<usize>, String> {
    let levels = lv.level_count();
    if levels == 0 {
        return Ok(None);
    }
    let title = area_title(order);
    let (mut parent, first) = if levels >= 2 {
        let collector = match group_level_zero_collector(lv) {
            Some(index) => index,
            None => insert_group(lv, lv.rows().len(), 0, COLLECTOR_TITLE)?,
        };
        (Some(collector), 1)
    } else {
        (None, 0)
    };
    for level in first..levels {
        let at = match parent {
            Some(index) => lv.block_range(index).end,
            None => lv.rows().len(),
        };
        parent = Some(insert_group(lv, at, level, &title)?);
    }
    Ok(parent)
}

fn probe_position() -> LvRow {
    LvRow::Position(Position::new("", "", Decimal::ZERO, "", None))
}

/// Order number for a new position at `index` of `lv`.
fn free_position_ordinal(lv: &Lv, index: usize) -> Result<String, String> {
    lv.free_ordinal_at(index, &probe_position()).ok_or_else(|| {
        "Im Nachtragsbereich ist keine Ordnungszahl mehr frei – Positionen im LV-Editor einfügen und dem Nachtrag zuordnen."
            .to_string()
    })
}

impl Project {
    /// LV quantities changed after the bids came in (Anwender 18.11.: „eher ein
    /// Nachtrag oder eine Massenmehrung“) as a new Nachtrag of the awarded
    /// bid: one Mehr-/Mindermenge (difference) per contract position,
    /// Grundlage Mengenänderung (§ 2 Abs. 3 VOB/B). Without a contract the
    /// awarded bid is taken over first (Auftrag = Angebot, Angebotsmengen).
    /// Positions that already stand as a quantity change in a Nachtrag are
    /// left out. Returns the Nachtrag and the number of items.
    pub fn quantity_changes_to_change_order(
        &mut self,
        today: &str,
    ) -> Result<(ChangeOrderId, usize), String> {
        let bid = self
            .awarded_bid()
            .ok_or("Im LV ist noch kein Angebot beauftragt – Mehr-/Mindermengen gehören zu einem Auftrag.")?
            .clone();
        let changes = bid.quantity_changes(self.lv());
        if changes.is_empty() {
            return Err("Nach dem Angebotseingang wurde keine Menge geändert.".to_string());
        }
        if self.contract().is_none() {
            self.take_over_contract()?;
        }
        let contract = self.contract().expect("taken over");
        let in_change_order = |position: RowId| {
            self.change_orders().iter().any(|order| {
                order
                    .items
                    .iter()
                    .any(|item| item.position == position && item.change == ItemChange::Quantity)
            })
        };
        let lv = self.lv();
        let mut lines = Vec::new();
        let items: Vec<ChangeOrderItem> = changes
            .iter()
            .filter(|change| {
                contract.item(change.position).is_some() && !in_change_order(change.position)
            })
            .map(|change| {
                let (ordinal, unit) = lv
                    .index_of(change.position)
                    .and_then(|index| lv.rows()[index].as_position())
                    .map(|position| (position.ordinal().to_string(), position.unit().to_string()))
                    .unwrap_or_default();
                lines.push(format!(
                    "{ordinal}: {}",
                    change.hint(&unit).trim_start_matches("Achtung ")
                ));
                ChangeOrderItem {
                    position: change.position,
                    change: ItemChange::Quantity,
                    offered_quantity: change.difference(),
                    note: "LV-Menge nach dem Angebotseingang geändert".to_string(),
                    ..ChangeOrderItem::default()
                }
            })
            .collect();
        if items.is_empty() {
            return Err("Die geänderten Mengen stehen schon in einem Nachtrag.".to_string());
        }
        let increase = changes.iter().any(|change| change.is_increase());
        let decrease = changes.iter().any(|change| !change.is_increase());
        let title = match (increase, decrease) {
            (true, false) => "Massenmehrung nach Angebotseingang",
            (false, true) => "Massenminderung nach Angebotseingang",
            _ => "Mengenänderungen nach Angebotseingang",
        };
        let mut order = ChangeOrder::new(ChangeOrderId(0), 0, title);
        order.basis = ChangeBasis::QuantityChange;
        order.announced_date = today.to_string();
        order.reason = format!(
            "LV-Mengen nach dem Angebotseingang geändert (Auftrag nach Angebotsmengen):\n{}",
            lines.join("\n")
        );
        let count = items.len();
        order.items = items;
        Ok((self.add_change_order(order), count))
    }
}

impl Project {
    /// Index of the change order's title in the active LV, if it still exists.
    pub fn change_order_area(&self, id: ChangeOrderId) -> Option<usize> {
        let group = self.change_order(id)?.group?;
        let index = self.lv().index_of(group)?;
        self.lv().rows()[index].is_group().then_some(index)
    }

    /// The title for the new positions of the change order; created at the
    /// end of the LV when missing. None for masks without title levels (new
    /// positions then go to the end of the LV).
    pub fn ensure_change_order_area(&mut self, id: ChangeOrderId) -> Result<Option<usize>, String> {
        if let Some(index) = self.change_order_area(id) {
            return Ok(Some(index));
        }
        let order = self
            .change_order(id)
            .ok_or("Den Nachtrag gibt es nicht.")?
            .clone();
        let index = create_area(self.lv_mut(), &order)?;
        let row = index.map(|index| self.lv().rows()[index].id());
        if let Some(order) = self.change_order_mut(id) {
            order.group = row;
        }
        Ok(index)
    }

    /// Where the next new position of the change order would go and its
    /// order number (OZ-Vorschlag); nothing is changed.
    pub fn change_order_proposal(&self, id: ChangeOrderId) -> Result<AreaProposal, String> {
        let order = self.change_order(id).ok_or("Den Nachtrag gibt es nicht.")?;
        let (lv, index) = match self.change_order_area(id) {
            Some(index) => (std::borrow::Cow::Borrowed(self.lv()), Some(index)),
            None => {
                let mut copy = self.lv().clone();
                let index = create_area(&mut copy, order)?;
                (std::borrow::Cow::Owned(copy), index)
            }
        };
        let at = index.map_or(lv.rows().len(), |index| lv.block_range(index).end);
        let title = index
            .map(|index| {
                let row = &lv.rows()[index];
                format!("{} {}", row.ordinal(), row.short_text())
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| "Ende des LV".to_string());
        Ok(AreaProposal {
            title,
            ordinal: free_position_ordinal(&lv, at)?,
        })
    }

    /// Adds a new position to the Nachtragsbereich of the change order and
    /// as its item (offered quantity = the position's quantity). An empty
    /// or taken order number is replaced by the next free one.
    pub fn add_change_order_position(
        &mut self,
        id: ChangeOrderId,
        mut position: Position,
        offered_price: Option<Decimal>,
    ) -> Result<RowId, String> {
        if self.change_order(id).is_none() {
            return Err("Den Nachtrag gibt es nicht.".to_string());
        }
        let area = self.ensure_change_order_area(id)?;
        let lv = self.lv_mut();
        let at = area.map_or(lv.rows().len(), |index| lv.block_range(index).end);
        let wanted = position.ordinal().trim().to_string();
        let taken = wanted.is_empty() || lv.position_by_ordinal(&wanted).is_some();
        if taken {
            position.set_ordinal(free_position_ordinal(lv, at)?);
        }
        let quantity = position.quantity();
        let row = lv.insert_row(at, LvRow::Position(position));
        if let Some(order) = self.change_order_mut(id) {
            order.items.push(ChangeOrderItem {
                position: row,
                change: ItemChange::NewPosition,
                offered_quantity: quantity,
                offered_unit_price: offered_price,
                ..ChangeOrderItem::default()
            });
        }
        Ok(row)
    }

    /// The change order a position belongs to (as new position or quantity
    /// change), first match.
    pub fn change_order_of_position(&self, position: RowId) -> Option<(&ChangeOrder, ItemChange)> {
        self.change_orders().iter().find_map(|order| {
            order
                .items
                .iter()
                .find(|item| item.position == position)
                .map(|item| (order, item.change))
        })
    }

    /// Assigns an LV row to the change order: a contract position becomes a
    /// quantity change (difference 0, to be entered), any other position a
    /// new position (offered quantity = LV quantity); a title brings all its
    /// positions that are neither in the contract nor in a change order.
    /// Returns the number of items added.
    pub fn assign_to_change_order(
        &mut self,
        id: ChangeOrderId,
        row: RowId,
    ) -> Result<usize, String> {
        let order = self.change_order(id).ok_or("Den Nachtrag gibt es nicht.")?;
        let label = order.label();
        let lv = self.lv();
        let index = lv
            .index_of(row)
            .ok_or("Die Zeile gibt es im LV nicht mehr.")?;
        let contract = self.contract();
        let in_contract =
            |position: RowId| contract.is_some_and(|contract| contract.item(position).is_some());
        let mut items = Vec::new();
        match &lv.rows()[index] {
            LvRow::Position(position) => {
                // A contract position may change in several change orders;
                // a new position belongs to one.
                if let Some((other, _)) = self
                    .change_order_of_position(position.id())
                    .filter(|_| !in_contract(position.id()))
                {
                    return Err(format!(
                        "{} {} gehört schon zu {}.",
                        position.ordinal(),
                        position.short_text(),
                        other.label()
                    ));
                }
                if in_contract(position.id()) {
                    if order.item(position.id()).is_some() {
                        return Err(format!(
                            "{} steht schon als Mengenänderung in {label}.",
                            position.ordinal()
                        ));
                    }
                    items.push(ChangeOrderItem {
                        position: position.id(),
                        change: ItemChange::Quantity,
                        ..ChangeOrderItem::default()
                    });
                } else {
                    items.push(ChangeOrderItem {
                        position: position.id(),
                        change: ItemChange::NewPosition,
                        offered_quantity: position.quantity(),
                        ..ChangeOrderItem::default()
                    });
                }
            }
            LvRow::Group(_) => {
                for row in &lv.rows()[lv.block_range(index)] {
                    let Some(position) = row.as_position() else {
                        continue;
                    };
                    if in_contract(position.id())
                        || self.change_order_of_position(position.id()).is_some()
                    {
                        continue;
                    }
                    items.push(ChangeOrderItem {
                        position: position.id(),
                        change: ItemChange::NewPosition,
                        offered_quantity: position.quantity(),
                        ..ChangeOrderItem::default()
                    });
                }
                if items.is_empty() {
                    return Err(
                        "Unter diesem Titel gibt es keine Position, die weder im Auftrag noch in einem Nachtrag steht."
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err(
                    "Nur Positionen und Titel lassen sich einem Nachtrag zuordnen.".to_string(),
                )
            }
        }
        let count = items.len();
        if let Some(order) = self.change_order_mut(id) {
            order.items.extend(items);
        }
        Ok(count)
    }

    /// Takes an item out of the change order. With `delete_row`, a new
    /// position is also deleted from the LV (only when no other change
    /// order and not the contract use it).
    pub fn remove_change_order_item(
        &mut self,
        id: ChangeOrderId,
        position: RowId,
        delete_row: bool,
    ) -> Result<(), String> {
        let order = self
            .change_order_mut(id)
            .ok_or("Den Nachtrag gibt es nicht.")?;
        let index = order
            .items
            .iter()
            .position(|item| item.position == position)
            .ok_or("Die Position gehört nicht zu diesem Nachtrag.")?;
        let item = order.items.remove(index);
        if delete_row && item.change == ItemChange::NewPosition {
            let used = self
                .contract()
                .is_some_and(|contract| contract.item(position).is_some())
                || self.change_order_of_position(position).is_some();
            if !used {
                if let Some(index) = self.lv().index_of(position) {
                    self.lv_mut().remove_row(index);
                }
            }
        }
        Ok(())
    }

    /// Removes a change order; with `delete_rows` also its new positions and
    /// its (then empty) titles from the LV.
    pub fn delete_change_order(
        &mut self,
        id: ChangeOrderId,
        delete_rows: bool,
    ) -> Option<ChangeOrder> {
        let order = self.remove_change_order(id)?;
        if delete_rows {
            let contract_has = |project: &Project, row: RowId| {
                project
                    .contract()
                    .is_some_and(|contract| contract.item(row).is_some())
            };
            for item in order
                .items
                .iter()
                .filter(|item| item.change == ItemChange::NewPosition)
            {
                if contract_has(self, item.position)
                    || self.change_order_of_position(item.position).is_some()
                {
                    continue;
                }
                if let Some(index) = self.lv().index_of(item.position) {
                    self.lv_mut().remove_row(index);
                }
            }
            // Empty titles of the area, innermost first.
            if let Some(group) = order.group {
                let mut current = self.lv().index_of(group);
                while let Some(index) = current {
                    let range = self.lv().block_range(index);
                    if range.len() > 1 || !self.lv().rows()[index].is_group() {
                        break;
                    }
                    let level = self.lv().rows()[index]
                        .as_group()
                        .map(LvGroup::level)
                        .unwrap_or(0);
                    let parent = self.lv().rows()[..index].iter().rposition(
                        |row| matches!(row, LvRow::Group(group) if group.level() < level),
                    );
                    self.lv_mut().remove_row(index);
                    current = parent;
                }
            }
        }
        Some(order)
    }
}

/// A price to compare the offered unit price with.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceHint {
    /// „EP Auftrag 01.0010“, „Planerpreis“, „Preisspiegel Mittel“, „ähnlich: 01.0020 Kabel …“.
    pub label: String,
    pub price: Decimal,
    /// Offered EP against this price in percent (+ = higher), one decimal.
    pub deviation: Option<Decimal>,
}

/// Deviation of `value` from `base` in percent, one decimal.
pub fn deviation_percent(value: Decimal, base: Decimal) -> Option<Decimal> {
    if base.is_zero() {
        return None;
    }
    Some(
        ((value - base) / base * Decimal::ONE_HUNDRED)
            .round_dp_with_strategy(1, RoundingStrategy::MidpointAwayFromZero),
    )
}

fn mean(values: &[Decimal]) -> Option<Decimal> {
    if values.is_empty() {
        return None;
    }
    let sum: Decimal = values.iter().copied().sum();
    Some(
        (sum / Decimal::from(values.len()))
            .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
    )
}

/// Prices to check an item's offered EP against (Prüfhilfe): contract EP of
/// the position or of the most similar contract position, the planner's
/// price, and the bid prices of the tender (Preisspiegel).
pub fn price_hints(project: &Project, item: &ChangeOrderItem) -> Vec<PriceHint> {
    let contract = project.contract();
    let offered = item.offered_price(contract);
    let mut hints = Vec::new();
    let mut push = |label: String, price: Decimal| {
        hints.push(PriceHint {
            label,
            deviation: offered.and_then(|offered| deviation_percent(offered, price)),
            price,
        });
    };
    let lv = project.lv();
    let position = lv.position_by_id(item.position);
    let bid_prices = |row: RowId| -> Vec<Decimal> {
        project
            .bids()
            .iter()
            .filter_map(|bid| bid.unit_price(row))
            .collect()
    };
    if let Some(contract_item) = contract.and_then(|contract| contract.item(item.position)) {
        push(
            format!("EP Auftrag {}", contract_item.ordinal),
            contract_item.unit_price,
        );
    } else if let (Some(contract), Some(position)) = (contract, position) {
        // Most similar contract position with the same unit.
        let unit = position.unit().trim().to_lowercase();
        let best = contract
            .items
            .iter()
            .filter(|(row, other)| {
                **row != item.position && other.unit.trim().to_lowercase() == unit
            })
            .map(|(row, other)| {
                (
                    name_similarity(position.short_text(), &other.short_text),
                    *row,
                    other,
                )
            })
            .filter(|(score, _, _)| *score >= 50)
            .max_by_key(|(score, row, _)| (*score, std::cmp::Reverse(*row)));
        if let Some((_, row, other)) = best {
            push(
                format!("ähnlich: {} {}", other.ordinal, other.short_text),
                other.unit_price,
            );
            if let Some(average) = mean(&bid_prices(row)) {
                push(format!("Preisspiegel Mittel zu {}", other.ordinal), average);
            }
        }
    }
    if let Some(price) = position.and_then(Position::unit_price) {
        push("Planerpreis".to_string(), price);
    }
    let prices = bid_prices(item.position);
    if let (Some(low), Some(average)) = (prices.iter().min().copied(), mean(&prices)) {
        push("Preisspiegel günstigster".to_string(), low);
        push("Preisspiegel Mittel".to_string(), average);
    }
    hints
}

/// Number and sums of the change orders in one status.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusSum {
    pub status: ChangeOrderStatus,
    pub count: usize,
    pub offered: Money,
    pub checked: Money,
    /// Approved after the contract discount where it applies.
    pub approved: Money,
}

/// Sums for every status, in the order of [`ChangeOrderStatus::ALL`].
pub fn sums_by_status(contract: Option<&Contract>, orders: &[ChangeOrder]) -> Vec<StatusSum> {
    ChangeOrderStatus::ALL
        .into_iter()
        .map(|status| {
            let mut sum = StatusSum {
                status,
                ..StatusSum::default()
            };
            for order in orders.iter().filter(|order| order.status == status) {
                sum.count += 1;
                sum.offered += order.offered_total(contract);
                sum.checked += order.checked_total(contract);
                let approved = order.approved_total(contract);
                sum.approved += approved - order.discount_on(approved, contract);
            }
            sum
        })
        .collect()
}

/// The contractor's offer for a change order as read from a file: the file
/// as a project (its first LV with texts, quantities and offered unit
/// prices as planner prices) and what the file says about itself.
#[derive(Debug, Clone)]
pub struct ChangeOrderOffer {
    pub source: Project,
    /// Change order number in the file (GAEB CONo), if any.
    pub number: Option<u32>,
    /// Date of the offer (TT.MM.JJJJ), if the file has one.
    pub date: String,
    /// Firm named in the file.
    pub firm: String,
    /// File name, for notes.
    pub file: String,
}

/// What taking over an offer did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OfferResult {
    /// Items whose offered quantity and price were set.
    pub updated: usize,
    /// New positions written into the Nachtragsbereich.
    pub created: usize,
    pub warnings: Vec<String>,
}

impl Project {
    /// Takes the contractor's offer into the change order: positions with
    /// the order number of an item get the offered quantity and price
    /// (checked values stay), contract positions become quantity changes,
    /// unknown positions are written into the Nachtragsbereich with their
    /// texts. An announced change order becomes „angeboten“ (`today`).
    pub fn take_change_order_offer(
        &mut self,
        id: ChangeOrderId,
        offer: &ChangeOrderOffer,
        today: &str,
    ) -> Result<OfferResult, String> {
        let order = self
            .change_order(id)
            .ok_or("Den Nachtrag gibt es nicht.")?
            .clone();
        let mut result = OfferResult::default();
        if let Some(number) = offer
            .number
            .filter(|number| *number != order.number && *number != 0)
        {
            result.warnings.push(format!(
                "Die Datei gehört zu Nachtrag N{number:02}, eingelesen in {}.",
                order.label()
            ));
        }
        let contract = self.contract().cloned();
        let mut new_rows: Vec<RowId> = Vec::new();
        let mut items = order.items.clone();
        for line in offer.source.lv().positions() {
            let quantity = line.quantity();
            let price = line.unit_price();
            let target = self
                .lv()
                .position_by_ordinal(line.ordinal())
                .map(Position::id)
                .or_else(|| {
                    // Second version of an offer with the firm's own numbers:
                    // a new position of this change order with the same short
                    // text and unit.
                    let key = (
                        line.short_text().trim().to_lowercase(),
                        line.unit().trim().to_lowercase(),
                    );
                    items
                        .iter()
                        .filter(|item| item.change == ItemChange::NewPosition)
                        .filter_map(|item| self.lv().position_by_id(item.position))
                        .find(|position| {
                            (
                                position.short_text().trim().to_lowercase(),
                                position.unit().trim().to_lowercase(),
                            ) == key
                        })
                        .map(Position::id)
                });
            let Some(target) = target else {
                new_rows.push(line.id());
                continue;
            };
            let contract_price = contract
                .as_ref()
                .and_then(|contract| contract.item(target))
                .map(|item| item.unit_price);
            if let Some(item) = items.iter_mut().find(|item| item.position == target) {
                item.offered_quantity = quantity;
                // A file without prices (X83, X81) keeps the offered price.
                if price.is_some() {
                    item.offered_unit_price = match item.change {
                        ItemChange::Quantity if price == contract_price => None,
                        _ => price,
                    };
                }
                result.updated += 1;
                continue;
            }
            if let Some((other, _)) = self
                .change_order_of_position(target)
                .filter(|(other, _)| other.id != id)
            {
                result.warnings.push(format!(
                    "{} gehört zu {} – nicht übernommen.",
                    line.ordinal(),
                    other.label()
                ));
                continue;
            }
            if contract_price.is_some() {
                items.push(ChangeOrderItem {
                    position: target,
                    change: ItemChange::Quantity,
                    offered_quantity: quantity,
                    offered_unit_price: price.filter(|price| Some(*price) != contract_price),
                    ..ChangeOrderItem::default()
                });
                result.warnings.push(format!(
                    "{} ist Auftragsposition – als Mengenänderung übernommen (Menge {} = Mehrmenge, bitte prüfen).",
                    line.ordinal(),
                    crate::format_decimal_de(quantity, 3)
                ));
            } else {
                items.push(ChangeOrderItem {
                    position: target,
                    change: ItemChange::NewPosition,
                    offered_quantity: quantity,
                    offered_unit_price: price,
                    ..ChangeOrderItem::default()
                });
            }
            result.updated += 1;
        }
        if let Some(target) = self.change_order_mut(id) {
            target.items = items;
        }
        if !new_rows.is_empty() {
            let transfer = transfer::collect(
                &offer.source,
                offer.source.lv(),
                &new_rows,
                TransferOptions {
                    quantities: true,
                    prices: false,
                },
            );
            let area = self.ensure_change_order_area(id)?;
            let range = match area {
                Some(index) => transfer::insert_under(self, index, &transfer)?,
                None => transfer::insert(self, None, &transfer)?,
            };
            let inserted: Vec<RowId> = self.lv().rows()[range]
                .iter()
                .filter(|row| row.is_position())
                .map(LvRow::id)
                .collect();
            let sources: Vec<&Position> = new_rows
                .iter()
                .filter_map(|row| offer.source.lv().position_by_id(*row))
                .collect();
            let mut renamed = Vec::new();
            for (row, source) in inserted.iter().zip(sources) {
                let ordinal = self
                    .lv()
                    .position_by_id(*row)
                    .map(|p| p.ordinal().to_string())
                    .unwrap_or_default();
                if normalize_ordinal(&ordinal) != normalize_ordinal(source.ordinal()) {
                    renamed.push(format!("{} → {ordinal}", source.ordinal()));
                }
                if let Some(order) = self.change_order_mut(id) {
                    order.items.push(ChangeOrderItem {
                        position: *row,
                        change: ItemChange::NewPosition,
                        offered_quantity: source.quantity(),
                        offered_unit_price: source.unit_price(),
                        ..ChangeOrderItem::default()
                    });
                }
                result.created += 1;
            }
            if !renamed.is_empty() {
                result.warnings.push(format!(
                    "Neue Positionen im Nachtragsbereich mit eigener OZ: {}",
                    short_join(&renamed, 12)
                ));
            }
        }
        if let Some(order) = self.change_order_mut(id) {
            if order.offer_date.trim().is_empty() && !offer.date.trim().is_empty() {
                order.offer_date = offer.date.trim().to_string();
            }
            let firm = offer.firm.trim();
            let contractor = contract
                .as_ref()
                .map(|contract| contract.contractor.trim())
                .unwrap_or("");
            if !firm.is_empty()
                && order.contractor.trim().is_empty()
                && name_similarity(firm, contractor) < 60
            {
                order.contractor = firm.to_string();
            }
            if order.status == ChangeOrderStatus::Announced {
                let date = if offer.date.trim().is_empty() {
                    today
                } else {
                    offer.date.trim()
                };
                order.set_status(
                    ChangeOrderStatus::Offered,
                    date,
                    &format!("Nachtragsangebot eingelesen ({})", offer.file),
                );
            }
        }
        Ok(result)
    }
}

fn short_join(items: &[String], max: usize) -> String {
    let mut text = items
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > max {
        text.push_str(&format!(" und {} weitere", items.len() - max));
    }
    text
}

#[cfg(test)]
mod tests;
