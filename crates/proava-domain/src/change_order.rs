//! Nachtragsmanagement: change orders to a contract. A change order
//! (Nachtrag) has a number (GAEB CONo 1, 2 …), a status, a reason, and items:
//! new positions (written in the LV like any other position) or a changed
//! quantity of a contract position (Mehr-/Mindermenge). Each item carries the
//! contractor's offer and the planner's checked quantity and price with a
//! note. Approved items extend the contract and become billable
//! ([`billable_items`]).

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use crate::contract::Contract;
use crate::lv::{Lv, LvRow, RowId};
use crate::money::Money;
use crate::position::PositionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChangeOrderId(pub u64);

/// Where a change order stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChangeOrderStatus {
    /// Announced by the contractor or recognised by the planner, no offer yet.
    #[default]
    Announced,
    /// Offer received.
    Offered,
    /// The planner is checking it (also: in negotiation).
    InReview,
    /// Checked by the planner, waiting for the client's decision.
    Reviewed,
    /// Ordered by the client – billable.
    Approved,
    Rejected,
    Withdrawn,
}

impl ChangeOrderStatus {
    pub const ALL: [ChangeOrderStatus; 7] = [
        ChangeOrderStatus::Announced,
        ChangeOrderStatus::Offered,
        ChangeOrderStatus::InReview,
        ChangeOrderStatus::Reviewed,
        ChangeOrderStatus::Approved,
        ChangeOrderStatus::Rejected,
        ChangeOrderStatus::Withdrawn,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChangeOrderStatus::Announced => "angekündigt",
            ChangeOrderStatus::Offered => "angeboten",
            ChangeOrderStatus::InReview => "in Prüfung",
            ChangeOrderStatus::Reviewed => "geprüft",
            ChangeOrderStatus::Approved => "beauftragt",
            ChangeOrderStatus::Rejected => "abgelehnt",
            ChangeOrderStatus::Withdrawn => "zurückgezogen",
        }
    }

    /// Value in the project file.
    pub fn key(self) -> &'static str {
        match self {
            ChangeOrderStatus::Announced => "announced",
            ChangeOrderStatus::Offered => "offered",
            ChangeOrderStatus::InReview => "in-review",
            ChangeOrderStatus::Reviewed => "reviewed",
            ChangeOrderStatus::Approved => "approved",
            ChangeOrderStatus::Rejected => "rejected",
            ChangeOrderStatus::Withdrawn => "withdrawn",
        }
    }

    /// Reads the project-file value and the German words (for tools).
    pub fn parse(text: &str) -> Result<Self, String> {
        let wanted = text.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|status| status.key() == wanted || status.label() == wanted)
            .or(match wanted.as_str() {
                "verhandlung" | "in verhandlung" => Some(ChangeOrderStatus::InReview),
                "genehmigt" => Some(ChangeOrderStatus::Approved),
                "zurückgewiesen" => Some(ChangeOrderStatus::Rejected),
                "angemeldet" | "erkannt" => Some(ChangeOrderStatus::Announced),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "Nachtragsstatus „{}“ gibt es nicht – angekündigt, angeboten, in Prüfung, geprüft, beauftragt, abgelehnt oder zurückgezogen.",
                    text.trim()
                )
            })
    }

    /// GAEB DA XML COStatus (tgCOStatus).
    pub fn gaeb(self) -> &'static str {
        match self {
            ChangeOrderStatus::Announced => "Filed",
            ChangeOrderStatus::Offered | ChangeOrderStatus::InReview => "Offered",
            ChangeOrderStatus::Reviewed => "FormAckn",
            ChangeOrderStatus::Approved => "Approved",
            ChangeOrderStatus::Rejected => "Rejected",
            ChangeOrderStatus::Withdrawn => "Withdrawn",
        }
    }

    pub fn from_gaeb(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "Recog" | "Filed" => ChangeOrderStatus::Announced,
            "Offered" => ChangeOrderStatus::Offered,
            "ObjToRecj" => ChangeOrderStatus::InReview,
            "FormAckn" => ChangeOrderStatus::Reviewed,
            "Approved" => ChangeOrderStatus::Approved,
            "Rejected" => ChangeOrderStatus::Rejected,
            "Withdrawn" => ChangeOrderStatus::Withdrawn,
            _ => return None,
        })
    }

    /// Only ordered change orders may be billed (GAEB DA XML 3.3, 8.1.3.8).
    pub fn is_billable(self) -> bool {
        self == ChangeOrderStatus::Approved
    }

    /// Still open (neither decided nor gone).
    pub fn is_open(self) -> bool {
        matches!(
            self,
            ChangeOrderStatus::Announced
                | ChangeOrderStatus::Offered
                | ChangeOrderStatus::InReview
                | ChangeOrderStatus::Reviewed
        )
    }
}

/// Legal basis of a change order (Anlass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChangeBasis {
    #[default]
    Unspecified,
    /// Geänderte Leistung – § 1 Abs. 3, § 2 Abs. 5 VOB/B.
    ChangedPerformance,
    /// Zusätzliche Leistung – § 1 Abs. 4, § 2 Abs. 6 VOB/B.
    AdditionalPerformance,
    /// Mengenänderung über 10 % – § 2 Abs. 3 VOB/B.
    QuantityChange,
    /// Leistung ohne Auftrag – § 2 Abs. 8 VOB/B.
    WithoutOrder,
    /// Behinderung, Beschleunigung – § 6 VOB/B.
    Hindrance,
    Other,
}

impl ChangeBasis {
    pub const ALL: [ChangeBasis; 7] = [
        ChangeBasis::Unspecified,
        ChangeBasis::ChangedPerformance,
        ChangeBasis::AdditionalPerformance,
        ChangeBasis::QuantityChange,
        ChangeBasis::WithoutOrder,
        ChangeBasis::Hindrance,
        ChangeBasis::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChangeBasis::Unspecified => "ohne Angabe",
            ChangeBasis::ChangedPerformance => "geänderte Leistung (§ 1 Abs. 3, § 2 Abs. 5 VOB/B)",
            ChangeBasis::AdditionalPerformance => {
                "zusätzliche Leistung (§ 1 Abs. 4, § 2 Abs. 6 VOB/B)"
            }
            ChangeBasis::QuantityChange => "Mengenänderung (§ 2 Abs. 3 VOB/B)",
            ChangeBasis::WithoutOrder => "Leistung ohne Auftrag (§ 2 Abs. 8 VOB/B)",
            ChangeBasis::Hindrance => "Behinderung, Beschleunigung (§ 6 VOB/B)",
            ChangeBasis::Other => "sonstiger Anlass",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ChangeBasis::Unspecified => "",
            ChangeBasis::ChangedPerformance => "changed",
            ChangeBasis::AdditionalPerformance => "additional",
            ChangeBasis::QuantityChange => "quantity",
            ChangeBasis::WithoutOrder => "without-order",
            ChangeBasis::Hindrance => "hindrance",
            ChangeBasis::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|basis| basis.key() == key.trim())
            .unwrap_or(ChangeBasis::Unspecified)
    }
}

/// What a change-order item changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ItemChange {
    /// A new position (Nachtragsposition) – its row in the LV.
    #[default]
    NewPosition,
    /// More or less of a contract position (Mehr-/Mindermenge); the quantity
    /// is the difference, negative = Minderung.
    Quantity,
}

/// One item of a change order: offer and check.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeOrderItem {
    /// LV row: the new position, or the contract position whose quantity
    /// changes.
    pub position: RowId,
    pub change: ItemChange,
    /// Offered quantity (for [`ItemChange::Quantity`] the difference).
    pub offered_quantity: Decimal,
    /// Offered unit price; for a quantity change None = contract price.
    pub offered_unit_price: Option<Decimal>,
    /// Checked quantity; None = as offered.
    pub checked_quantity: Option<Decimal>,
    /// Checked unit price; None = as offered.
    pub checked_unit_price: Option<Decimal>,
    /// Planner's note (Prüfvermerk), e.g. „EK 102 €, Zeitansatz in Ordnung“.
    pub note: String,
    /// Status of this item when it differs from the change order (GAEB: the
    /// item's status has priority), e.g. one item rejected.
    pub status: Option<ChangeOrderStatus>,
}

impl ChangeOrderItem {
    pub fn checked_quantity(&self) -> Decimal {
        self.checked_quantity.unwrap_or(self.offered_quantity)
    }

    /// Offered unit price, for a quantity change falling back to the
    /// contract price.
    pub fn offered_price(&self, contract: Option<&Contract>) -> Option<Decimal> {
        self.offered_unit_price
            .or_else(|| self.contract_price(contract))
    }

    pub fn checked_price(&self, contract: Option<&Contract>) -> Option<Decimal> {
        self.checked_unit_price
            .or_else(|| self.offered_price(contract))
    }

    fn contract_price(&self, contract: Option<&Contract>) -> Option<Decimal> {
        (self.change == ItemChange::Quantity)
            .then(|| contract.and_then(|contract| contract.item(self.position)))
            .flatten()
            .map(|item| item.unit_price)
    }

    pub fn offered_total(&self, contract: Option<&Contract>) -> Money {
        self.offered_price(contract)
            .map(|price| Money::total(price, self.offered_quantity))
            .unwrap_or(Money::ZERO)
    }

    pub fn checked_total(&self, contract: Option<&Contract>) -> Money {
        self.checked_price(contract)
            .map(|price| Money::total(price, self.checked_quantity()))
            .unwrap_or(Money::ZERO)
    }

    /// Whether the planner changed quantity or price.
    pub fn is_corrected(&self, contract: Option<&Contract>) -> bool {
        self.checked_quantity() != self.offered_quantity
            || self.checked_price(contract) != self.offered_price(contract)
    }
}

/// One step of the status history (Statusverlauf): when the change order
/// got which status, with a note („Angebot per Mail“, „Bauherr per Schreiben
/// vom …“).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusChange {
    pub status: ChangeOrderStatus,
    /// TT.MM.JJJJ.
    pub date: String,
    pub note: String,
}

/// A change order (Nachtrag).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeOrder {
    pub id: ChangeOrderId,
    /// Number 1, 2 … (GAEB CONo; 0 is the main contract).
    pub number: u32,
    /// Content / purpose (Inhalt, Nachtragszweck).
    pub title: String,
    pub basis: ChangeBasis,
    /// Reason in words (Begründung der Erfordernis).
    pub reason: String,
    pub status: ChangeOrderStatus,
    /// Contractor's offer number and date.
    pub offer_number: String,
    pub offer_date: String,
    /// Check by the planner: date, by whom, note for the whole offer
    /// („fachtechnisch und rechnerisch geprüft“).
    pub checked_date: String,
    pub checked_by: String,
    pub check_note: String,
    /// Date of the client's order or decision.
    pub decision_date: String,
    /// The discount of the main contract also applies (Nachlass gilt).
    pub discount_applies: bool,
    /// While open: counts in the forecast (Wirksamkeit). Approved change
    /// orders always count, rejected and withdrawn ones never.
    pub in_forecast: bool,
    pub items: Vec<ChangeOrderItem>,
    /// Firm when it differs from the contractor of the contract (empty =
    /// the contract's).
    pub contractor: String,
    /// Date the change order was announced or recognised (Anzeige).
    pub announced_date: String,
    /// Deadline for the contractor's offer (Angebotsfrist).
    pub offer_due: String,
    /// Deadline for the check / the client's decision.
    pub decision_due: String,
    /// Recommendation to the client (Empfehlung), printed in the check.
    pub recommendation: String,
    /// Title in the LV that holds the new positions (Nachtragsbereich).
    pub group: Option<RowId>,
    /// Status history, oldest first.
    pub history: Vec<StatusChange>,
}

impl ChangeOrder {
    pub fn new(id: ChangeOrderId, number: u32, title: impl Into<String>) -> Self {
        Self {
            id,
            number,
            title: title.into(),
            discount_applies: true,
            in_forecast: true,
            ..Self::default()
        }
    }

    /// „N04“.
    pub fn label(&self) -> String {
        format!("N{:02}", self.number)
    }

    /// Firm of the change order: its own, else the contractor of the contract.
    pub fn firm<'a>(&'a self, contract: Option<&'a Contract>) -> &'a str {
        if self.contractor.trim().is_empty() {
            contract
                .map(|contract| contract.contractor.as_str())
                .unwrap_or("")
        } else {
            self.contractor.as_str()
        }
    }

    /// Changes the status with date and note and keeps the step in the
    /// history; the matching date field is filled (offer, check, decision).
    /// Returns false when nothing changed.
    pub fn set_status(&mut self, status: ChangeOrderStatus, date: &str, note: &str) -> bool {
        let date = date.trim();
        let note = note.trim();
        if status == self.status && note.is_empty() && date.is_empty() {
            return false;
        }
        if self.history.is_empty() && status != self.status && !self.announced_date.is_empty() {
            // The start is known: keep it as first step.
            self.history.push(StatusChange {
                status: self.status,
                date: self.announced_date.clone(),
                note: String::new(),
            });
        }
        self.history.push(StatusChange {
            status,
            date: date.to_string(),
            note: note.to_string(),
        });
        if !date.is_empty() {
            let field = match status {
                ChangeOrderStatus::Announced => &mut self.announced_date,
                ChangeOrderStatus::Offered => &mut self.offer_date,
                ChangeOrderStatus::InReview => &mut self.offer_date,
                ChangeOrderStatus::Reviewed => &mut self.checked_date,
                ChangeOrderStatus::Approved
                | ChangeOrderStatus::Rejected
                | ChangeOrderStatus::Withdrawn => &mut self.decision_date,
            };
            let always = matches!(
                status,
                ChangeOrderStatus::Approved
                    | ChangeOrderStatus::Rejected
                    | ChangeOrderStatus::Withdrawn
            );
            if always || field.is_empty() {
                *field = date.to_string();
            }
        }
        self.status = status;
        true
    }

    /// Date belonging to the current status (last history step, else the
    /// matching date field).
    pub fn status_date(&self) -> &str {
        if let Some(step) = self
            .history
            .iter()
            .rev()
            .find(|step| step.status == self.status && !step.date.is_empty())
        {
            return &step.date;
        }
        match self.status {
            ChangeOrderStatus::Announced => &self.announced_date,
            ChangeOrderStatus::Offered | ChangeOrderStatus::InReview => &self.offer_date,
            ChangeOrderStatus::Reviewed => &self.checked_date,
            _ => &self.decision_date,
        }
    }

    pub fn item(&self, position: RowId) -> Option<&ChangeOrderItem> {
        self.items.iter().find(|item| item.position == position)
    }

    pub fn item_mut(&mut self, position: RowId) -> Option<&mut ChangeOrderItem> {
        self.items.iter_mut().find(|item| item.position == position)
    }

    /// Status of an item: its own, else the change order's.
    pub fn item_status(&self, item: &ChangeOrderItem) -> ChangeOrderStatus {
        item.status.unwrap_or(self.status)
    }

    pub fn offered_total(&self, contract: Option<&Contract>) -> Money {
        self.items
            .iter()
            .map(|item| item.offered_total(contract))
            .sum()
    }

    /// Checked sum of all items that are not rejected or withdrawn.
    pub fn checked_total(&self, contract: Option<&Contract>) -> Money {
        self.items
            .iter()
            .filter(|item| {
                !matches!(
                    self.item_status(item),
                    ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
                )
            })
            .map(|item| item.checked_total(contract))
            .sum()
    }

    /// Checked sum of the approved items (net before discount).
    pub fn approved_total(&self, contract: Option<&Contract>) -> Money {
        self.items
            .iter()
            .filter(|item| self.item_status(item).is_billable())
            .map(|item| item.checked_total(contract))
            .sum()
    }

    /// Discount on an amount of this change order.
    pub fn discount_on(&self, amount: Money, contract: Option<&Contract>) -> Money {
        match contract {
            Some(contract) if self.discount_applies => amount.percent(contract.discount_percent),
            _ => Money::ZERO,
        }
    }

    /// Credits within the change order (sum of the negative items, checked;
    /// rejected and withdrawn items do not count).
    pub fn credit_total(&self, contract: Option<&Contract>) -> Money {
        self.items
            .iter()
            .filter(|item| {
                !matches!(
                    self.item_status(item),
                    ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
                )
            })
            .map(|item| item.checked_total(contract))
            .filter(|total| total.cents() < 0)
            .sum()
    }

    /// What keeps this change order from being consistent: items twice,
    /// quantity changes of positions outside the contract, new positions
    /// that are contract positions, missing prices.
    pub fn problems(&self, contract: Option<&Contract>, lv: &Lv) -> Vec<String> {
        let mut problems = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for item in &self.items {
            let name = position_name(lv, contract, item.position);
            if !seen.insert(item.position) {
                problems.push(format!(
                    "{}: {name} steht zweimal im Nachtrag.",
                    self.label()
                ));
            }
            let in_contract =
                contract.is_some_and(|contract| contract.item(item.position).is_some());
            match item.change {
                ItemChange::Quantity if !in_contract => problems.push(format!(
                    "{}: {name} ist keine Auftragsposition – Mengenänderung nicht möglich.",
                    self.label()
                )),
                ItemChange::NewPosition if in_contract => problems.push(format!(
                    "{}: {name} ist schon Auftragsposition – als Mengenänderung erfassen.",
                    self.label()
                )),
                ItemChange::NewPosition if lv.row_by_id(item.position).is_none() => {
                    problems.push(format!(
                        "{}: Die Nachtragsposition gibt es im LV nicht mehr.",
                        self.label()
                    ))
                }
                _ => {}
            }
            if item.offered_price(contract).is_none() && item.checked_unit_price.is_none() {
                problems.push(format!("{}: {name} hat keinen Preis.", self.label()));
            }
        }
        problems
    }
}

fn position_name(lv: &Lv, contract: Option<&Contract>, position: RowId) -> String {
    match lv.row_by_id(position) {
        Some(row) => format!("{} {}", row.ordinal(), row.short_text())
            .trim()
            .to_string(),
        None => contract
            .and_then(|contract| contract.item(position))
            .map(|item| format!("{} {}", item.ordinal, item.short_text))
            .unwrap_or_else(|| "Position".to_string()),
    }
}

/// Where a billable position comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillableSource {
    Contract,
    ChangeOrder(ChangeOrderId),
}

/// A position that may be billed: contract or approved change order,
/// with the ordered quantity (incl. approved Mehr-/Mindermengen) and price.
#[derive(Debug, Clone, PartialEq)]
pub struct BillableItem {
    pub position: RowId,
    pub ordinal: String,
    pub short_text: String,
    pub unit: String,
    /// Ordered quantity: contract quantity plus approved quantity changes.
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub kind: PositionKind,
    pub lump_sum: bool,
    pub hourly: bool,
    /// The contract discount applies to billed amounts.
    pub discount: bool,
    pub source: BillableSource,
}

/// Contract positions plus the approved change-order items. Approved
/// quantity changes add to the ordered quantity at the contract price.
pub fn billable_items(
    contract: &Contract,
    orders: &[ChangeOrder],
    lv: &Lv,
) -> BTreeMap<RowId, BillableItem> {
    let mut items: BTreeMap<RowId, BillableItem> = contract
        .items
        .iter()
        .map(|(id, item)| {
            (
                *id,
                BillableItem {
                    position: *id,
                    ordinal: item.ordinal.clone(),
                    short_text: item.short_text.clone(),
                    unit: item.unit.clone(),
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    kind: item.kind,
                    lump_sum: item.lump_sum,
                    hourly: item.hourly,
                    discount: true,
                    source: BillableSource::Contract,
                },
            )
        })
        .collect();
    for order in orders {
        for item in order
            .items
            .iter()
            .filter(|item| order.item_status(item).is_billable())
        {
            match item.change {
                ItemChange::Quantity => {
                    if let Some(billable) = items.get_mut(&item.position) {
                        billable.quantity += item.checked_quantity();
                    }
                }
                ItemChange::NewPosition => {
                    if items.contains_key(&item.position) {
                        continue;
                    }
                    let Some(price) = item.checked_price(Some(contract)) else {
                        continue;
                    };
                    let position = lv.row_by_id(item.position).and_then(LvRow::as_position);
                    items.insert(
                        item.position,
                        BillableItem {
                            position: item.position,
                            ordinal: position
                                .map(|p| p.ordinal().to_string())
                                .unwrap_or_default(),
                            short_text: position
                                .map(|p| p.short_text().to_string())
                                .unwrap_or_else(|| order.title.clone()),
                            unit: position.map(|p| p.unit().to_string()).unwrap_or_default(),
                            quantity: item.checked_quantity(),
                            unit_price: price,
                            kind: position.map(|p| p.kind()).unwrap_or_default(),
                            lump_sum: position.is_some_and(|p| p.is_lump_sum()),
                            hourly: position.is_some_and(|p| p.is_hourly()),
                            discount: order.discount_applies,
                            source: BillableSource::ChangeOrder(order.id),
                        },
                    );
                }
            }
        }
    }
    items
}

/// The change order a position belongs to that is not (yet) billable.
pub fn pending_change_order(
    orders: &[ChangeOrder],
    position: RowId,
) -> Option<(&ChangeOrder, ChangeOrderStatus)> {
    orders.iter().find_map(|order| {
        order
            .items
            .iter()
            .find(|item| item.position == position && item.change == ItemChange::NewPosition)
            .map(|item| (order, order.item_status(item)))
            .filter(|(_, status)| !status.is_billable())
    })
}

/// One line of the change-order overview (Nachtragsübersicht).
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeOrderLine {
    pub id: ChangeOrderId,
    pub label: String,
    pub title: String,
    pub basis: ChangeBasis,
    pub reason: String,
    pub status: ChangeOrderStatus,
    pub offered: Money,
    pub checked: Money,
    /// Approved, after the contract discount where it applies.
    pub approved: Money,
    /// Negative items (Gutschrift), checked.
    pub credit: Money,
}

/// Change-order overview with its effect on the contract sum.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeOrderOverview {
    pub lines: Vec<ChangeOrderLine>,
    pub offered: Money,
    pub checked: Money,
    /// Approved change orders after discount.
    pub approved: Money,
    /// Still open (announced … reviewed): checked, or offered while unchecked.
    pub open: Money,
    /// Approved quantity changes of contract positions: more and less
    /// (Mehrungen / Minderungen), before discount.
    pub increases: Money,
    pub decreases: Money,
    /// Contract sum as awarded.
    pub contract_total: Money,
    /// Contract sum plus approved change orders.
    pub total_with_change_orders: Money,
    /// Plus the open change orders that count in the forecast (checked,
    /// after discount): what the contract will probably cost.
    pub forecast: Money,
}

pub fn change_order_overview(
    contract: Option<&Contract>,
    orders: &[ChangeOrder],
) -> ChangeOrderOverview {
    let mut overview = ChangeOrderOverview {
        contract_total: contract.map(|contract| contract.total).unwrap_or_default(),
        ..ChangeOrderOverview::default()
    };
    let mut open_in_forecast = Money::ZERO;
    let mut sorted: Vec<&ChangeOrder> = orders.iter().collect();
    sorted.sort_by_key(|order| (order.number, order.id));
    for order in sorted {
        let approved_net = order.approved_total(contract);
        let approved = approved_net - order.discount_on(approved_net, contract);
        let line = ChangeOrderLine {
            id: order.id,
            label: order.label(),
            title: order.title.clone(),
            basis: order.basis,
            reason: order.reason.clone(),
            status: order.status,
            offered: order.offered_total(contract),
            checked: order.checked_total(contract),
            approved,
            credit: order.credit_total(contract),
        };
        overview.offered += line.offered;
        overview.checked += line.checked;
        overview.approved += line.approved;
        if order.status.is_open() {
            overview.open += line.checked;
            if order.in_forecast {
                let open = order.checked_total(contract);
                open_in_forecast += open - order.discount_on(open, contract);
            }
        }
        for item in order.items.iter().filter(|item| {
            item.change == ItemChange::Quantity && order.item_status(item).is_billable()
        }) {
            let total = item.checked_total(contract);
            if total.cents() >= 0 {
                overview.increases += total;
            } else {
                overview.decreases += total;
            }
        }
        overview.lines.push(line);
    }
    overview.total_with_change_orders = overview.contract_total + overview.approved;
    overview.forecast = overview.total_with_change_orders + open_in_forecast;
    overview
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractItem;
    use crate::{LvGroup, OzMask, Position};

    fn dec(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    /// LV with two contract positions (A, B) and a new Nachtragsposition N.
    fn setup() -> (Lv, Contract, [RowId; 3]) {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "01", "Titel"));
        let a = lv.push_position(Position::new("01.0010", "A", dec("10"), "St", None));
        let b = lv.push_position(Position::new("01.0020", "B", dec("100"), "m", None));
        lv.push_group(LvGroup::new(0, "02", "Nachtrag 1"));
        let n = lv.push_position(Position::new("02.0010", "Mast", dec("1"), "St", None));
        let item = |ordinal: &str, quantity: &str, price: &str| ContractItem {
            ordinal: ordinal.into(),
            short_text: String::new(),
            unit: "St".into(),
            quantity: dec(quantity),
            unit_price: dec(price),
            kind: PositionKind::Normal,
            lump_sum: false,
            hourly: false,
        };
        let contract = Contract {
            contractor: "Firma".into(),
            items: BTreeMap::from([
                (a, item("01.0010", "10", "50")),
                (b, item("01.0020", "100", "2.5")),
            ]),
            discount_percent: dec("2"),
            total: Money::from_cents(73_500),
            ..Contract::default()
        };
        (lv, contract, [a, b, n])
    }

    #[test]
    fn status_words_and_gaeb() {
        assert_eq!(
            ChangeOrderStatus::parse("Verhandlung"),
            Ok(ChangeOrderStatus::InReview)
        );
        assert_eq!(
            ChangeOrderStatus::parse("beauftragt"),
            Ok(ChangeOrderStatus::Approved)
        );
        assert_eq!(
            ChangeOrderStatus::parse("approved"),
            Ok(ChangeOrderStatus::Approved)
        );
        assert!(ChangeOrderStatus::parse("vielleicht").is_err());
        for status in ChangeOrderStatus::ALL {
            assert_eq!(ChangeOrderStatus::parse(status.key()), Ok(status));
            let back = ChangeOrderStatus::from_gaeb(status.gaeb()).unwrap();
            assert_eq!(back.is_billable(), status.is_billable());
        }
        assert!(ChangeOrderStatus::Approved.is_billable());
        assert!(!ChangeOrderStatus::Reviewed.is_billable());
        assert_eq!(
            ChangeBasis::from_key("additional"),
            ChangeBasis::AdditionalPerformance
        );
        assert_eq!(ChangeBasis::from_key("?"), ChangeBasis::Unspecified);
    }

    #[test]
    fn offered_checked_approved_and_credits() {
        let (lv, contract, [a, b, n]) = setup();
        let mut order = ChangeOrder::new(ChangeOrderId(1), 1, "Wetterstation");
        order.items = vec![
            ChangeOrderItem {
                position: n,
                offered_quantity: dec("1"),
                offered_unit_price: Some(dec("1947")),
                checked_unit_price: Some(dec("1697")),
                note: "Mast kürzer".into(),
                ..ChangeOrderItem::default()
            },
            // 20 St more of A at contract price, 30 m less of B.
            ChangeOrderItem {
                position: a,
                change: ItemChange::Quantity,
                offered_quantity: dec("20"),
                ..ChangeOrderItem::default()
            },
            ChangeOrderItem {
                position: b,
                change: ItemChange::Quantity,
                offered_quantity: dec("-30"),
                ..ChangeOrderItem::default()
            },
        ];
        assert!(order.problems(Some(&contract), &lv).is_empty());
        // 1947 + 20×50 − 30×2,5 = 2872
        assert_eq!(
            order.offered_total(Some(&contract)),
            Money::from_cents(287_200)
        );
        assert_eq!(
            order.checked_total(Some(&contract)),
            Money::from_cents(262_200)
        );
        assert_eq!(
            order.credit_total(Some(&contract)),
            Money::from_cents(-7_500)
        );
        assert!(order.items[0].is_corrected(Some(&contract)));
        assert!(!order.items[1].is_corrected(Some(&contract)));
        assert_eq!(order.approved_total(Some(&contract)), Money::ZERO);

        // Not approved: nothing billable beyond the contract.
        let items = billable_items(&contract, std::slice::from_ref(&order), &lv);
        assert_eq!(items.len(), 2);
        assert_eq!(items[&a].quantity, dec("10"));
        assert_eq!(
            pending_change_order(std::slice::from_ref(&order), n)
                .unwrap()
                .1,
            ChangeOrderStatus::Announced
        );

        order.status = ChangeOrderStatus::Approved;
        let items = billable_items(&contract, std::slice::from_ref(&order), &lv);
        assert_eq!(items.len(), 3);
        assert_eq!(items[&a].quantity, dec("30"));
        assert_eq!(items[&b].quantity, dec("70"));
        assert_eq!(items[&n].unit_price, dec("1697"));
        assert_eq!(items[&n].short_text, "Mast");
        assert_eq!(
            items[&n].source,
            BillableSource::ChangeOrder(ChangeOrderId(1))
        );
        assert!(pending_change_order(std::slice::from_ref(&order), n).is_none());

        // One item rejected: it drops out of checked and approved.
        order.items[0].status = Some(ChangeOrderStatus::Rejected);
        assert_eq!(
            order.checked_total(Some(&contract)),
            Money::from_cents(92_500)
        );
        assert!(!billable_items(&contract, std::slice::from_ref(&order), &lv).contains_key(&n));
    }

    #[test]
    fn overview_with_discount_and_quantity_changes() {
        let (lv, contract, [a, b, n]) = setup();
        let mut first = ChangeOrder::new(ChangeOrderId(1), 1, "Mast");
        first.status = ChangeOrderStatus::Approved;
        first.items.push(ChangeOrderItem {
            position: n,
            offered_quantity: dec("1"),
            offered_unit_price: Some(dec("1000")),
            ..ChangeOrderItem::default()
        });
        let mut second = ChangeOrder::new(ChangeOrderId(2), 2, "Mengen");
        second.status = ChangeOrderStatus::Approved;
        second.discount_applies = false;
        second.items = vec![
            ChangeOrderItem {
                position: a,
                change: ItemChange::Quantity,
                offered_quantity: dec("2"),
                ..ChangeOrderItem::default()
            },
            ChangeOrderItem {
                position: b,
                change: ItemChange::Quantity,
                offered_quantity: dec("-10"),
                ..ChangeOrderItem::default()
            },
        ];
        let mut third = ChangeOrder::new(ChangeOrderId(3), 3, "Beschleunigung");
        third.status = ChangeOrderStatus::InReview;
        third.basis = ChangeBasis::Hindrance;
        third.items.push(ChangeOrderItem {
            position: n,
            offered_quantity: dec("1"),
            offered_unit_price: Some(dec("500")),
            ..ChangeOrderItem::default()
        });
        let overview = change_order_overview(Some(&contract), &[third.clone(), first, second]);
        assert_eq!(
            overview
                .lines
                .iter()
                .map(|line| line.label.as_str())
                .collect::<Vec<_>>(),
            ["N01", "N02", "N03"]
        );
        // N01: 1000 − 2 % = 980; N02: 2×50 − 10×2,5 = 75 without discount.
        assert_eq!(overview.lines[0].approved, Money::from_cents(98_000));
        assert_eq!(overview.lines[1].approved, Money::from_cents(7_500));
        assert_eq!(overview.lines[2].approved, Money::ZERO);
        assert_eq!(overview.approved, Money::from_cents(105_500));
        assert_eq!(overview.open, Money::from_cents(50_000));
        assert_eq!(overview.increases, Money::from_cents(10_000));
        assert_eq!(overview.decreases, Money::from_cents(-2_500));
        assert_eq!(
            overview.total_with_change_orders,
            Money::from_cents(73_500 + 105_500)
        );
        // N03 is open and counts in the forecast: 500 − 2 % = 490.
        assert_eq!(
            overview.forecast,
            Money::from_cents(73_500 + 105_500 + 49_000)
        );
        let mut without = third.clone();
        without.in_forecast = false;
        let overview = change_order_overview(Some(&contract), &[without]);
        assert_eq!(overview.forecast, overview.total_with_change_orders);
        assert!(third.problems(Some(&contract), &lv).is_empty());
    }

    #[test]
    fn problems_are_named() {
        let (lv, contract, [a, _, n]) = setup();
        let mut order = ChangeOrder::new(ChangeOrderId(1), 4, "Fehler");
        order.items = vec![
            ChangeOrderItem {
                position: n,
                change: ItemChange::Quantity,
                offered_quantity: dec("1"),
                offered_unit_price: Some(dec("1")),
                ..ChangeOrderItem::default()
            },
            ChangeOrderItem {
                position: a,
                offered_quantity: dec("1"),
                offered_unit_price: Some(dec("1")),
                ..ChangeOrderItem::default()
            },
            ChangeOrderItem {
                position: a,
                change: ItemChange::Quantity,
                offered_quantity: dec("1"),
                ..ChangeOrderItem::default()
            },
            ChangeOrderItem {
                position: RowId(999),
                offered_quantity: dec("1"),
                ..ChangeOrderItem::default()
            },
        ];
        let problems = order.problems(Some(&contract), &lv).join("\n");
        assert!(
            problems.contains("N04: 02.0010 Mast ist keine Auftragsposition"),
            "{problems}"
        );
        assert!(
            problems.contains("01.0010 A ist schon Auftragsposition"),
            "{problems}"
        );
        assert!(problems.contains("steht zweimal"), "{problems}");
        assert!(problems.contains("gibt es im LV nicht mehr"), "{problems}");
        assert!(problems.contains("hat keinen Preis"), "{problems}");
    }
}
