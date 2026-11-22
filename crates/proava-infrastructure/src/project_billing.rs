//! Auftrag, Nachträge and Abrechnung of an LV in `projekt.xml` (schema 2,
//! additive: files without these elements read as before).
//!
//! ```xml
//! <lv …>
//!   <contract contractor="…" bid="3" date="…" discount="2" total="1234.50">
//!     <terms …/>
//!     <item row="12" oz="01.0010" short="…" unit="St" quantity="10" price="12.5" kind="normal"/>
//!   </contract>
//!   <change-orders next-id="2">
//!     <change-order id="1" number="1" title="…" status="approved" basis="additional" group="38" …>
//!       <step status="offered" date="10.03.2026" note="…"/>
//!       <item row="40" change="new" offered-quantity="1" offered-price="1947" checked-price="1697" note="…"/>
//!     </change-order>
//!   </change-orders>
//!   <billing next-id="3">
//!     <invoice id="1" number="R1" kind="partial" quantities="increment" release="cumulative" vat="reverse-charge" …>
//!       <item row="12" claimed-quantity="40" checked-quantity="38" progress="0.375" progress-note="…"/>
//!     </invoice>
//!     <takeoff row="12"><line label="EG" formula="2*12,5" invoice="1"/></takeoff>
//!   </billing>
//! </lv>
//! ```

use std::collections::BTreeMap;

use proava_domain::{
    Billing, ChangeBasis, ChangeOrder, ChangeOrderId, ChangeOrderItem, ChangeOrderStatus,
    CheckBasis, Contract, ContractItem, Correction, Decimal, Invoice, InvoiceId, InvoiceItem,
    InvoiceKind, ItemChange, Mode, Money, Progress, Project, RowId, StatusChange, TakeoffLine,
    VatRule,
};

use crate::project_xml::{
    decimal, kind_attributes, kind_of, number, optional_decimal, terms_element, terms_of,
};
use crate::xml_tree::Element;

/// Adds contract, change orders and billing of the LV at `index`.
pub(crate) fn write(project: &Project, index: usize, element: &mut Element) {
    if let Some(contract) = project.contract_of(index) {
        element.push(contract_element(contract));
    }
    let orders = project.change_orders_of(index);
    if !orders.is_empty() {
        let next = orders.iter().map(|order| order.id.0 + 1).max().unwrap_or(1);
        element.push(
            Element::new("change-orders")
                .attr("next-id", next.to_string())
                .children(orders.iter().map(change_order_element)),
        );
    }
    if let Some(billing) = project
        .billing_of(index)
        .filter(|billing| !billing.is_empty())
    {
        element.push(billing_element(billing));
    }
}

/// Reads them back for the LV at `index`.
pub(crate) fn read(project: &mut Project, index: usize, node: &Element) -> Result<(), String> {
    if let Some(contract) = node.first("contract") {
        project.set_contract_of(index, Some(contract_of(contract)?));
    }
    if let Some(orders) = node.first("change-orders") {
        for order in orders.all("change-order") {
            project.add_change_order_to(index, change_order_of(order)?);
        }
    }
    if let Some(billing) = node.first("billing") {
        let read = billing_of(billing)?;
        if let Some(target) = project.billing_of_mut(index) {
            *target = read;
        }
    }
    Ok(())
}

fn text(value: &Decimal) -> String {
    value.normalize().to_string()
}

fn money_text(value: Money) -> String {
    value.to_decimal().to_string()
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn optional_money(node: &Element, key: &str, what: &str) -> Result<Option<Money>, String> {
    Ok(optional_decimal(node, key, what)?.map(Money::from_decimal))
}

// ---------- contract ----------

fn contract_element(contract: &Contract) -> Element {
    Element::new("contract")
        .attr("contractor", contract.contractor.as_str())
        .attr_opt("bid", contract.bid.map(|bid| bid.0.to_string()))
        .attr_opt("date", non_empty(&contract.date))
        .attr_opt("number", non_empty(&contract.number))
        .attr("discount", text(&contract.discount_percent))
        .attr("total", money_text(contract.total))
        .child(terms_element(&contract.terms))
        .children(contract.items.iter().map(|(row, item)| {
            kind_attributes(
                Element::new("item")
                    .attr("row", row.0.to_string())
                    .attr("oz", item.ordinal.as_str())
                    .attr("short", item.short_text.as_str())
                    .attr("unit", item.unit.as_str())
                    .attr("quantity", text(&item.quantity))
                    .attr("price", text(&item.unit_price)),
                item.kind,
            )
            .flag("lump-sum", item.lump_sum)
            .flag("hourly", item.hourly)
        }))
}

fn contract_of(node: &Element) -> Result<Contract, String> {
    let mut items = BTreeMap::new();
    for item in node.all("item") {
        let Some(row) = number::<u64>(item, "row")? else {
            continue;
        };
        items.insert(
            RowId(row),
            ContractItem {
                ordinal: item.text("oz"),
                short_text: item.text("short"),
                unit: item.text("unit"),
                quantity: optional_decimal(item, "quantity", "Auftragsmenge")?.unwrap_or_default(),
                unit_price: optional_decimal(item, "price", "Einheitspreis des Auftrags")?
                    .unwrap_or_default(),
                kind: kind_of(item)?,
                lump_sum: item.is_set("lump-sum"),
                hourly: item.is_set("hourly"),
            },
        );
    }
    Ok(Contract {
        contractor: node.text("contractor"),
        bid: number::<u64>(node, "bid")?.map(proava_domain::BidId),
        date: node.text("date"),
        number: node.text("number"),
        items,
        discount_percent: optional_decimal(node, "discount", "Nachlass")?.unwrap_or_default(),
        total: optional_money(node, "total", "Auftragssumme")?.unwrap_or_default(),
        terms: node
            .first("terms")
            .map(terms_of)
            .transpose()?
            .unwrap_or_default(),
    })
}

// ---------- change orders ----------

fn change_order_element(order: &ChangeOrder) -> Element {
    Element::new("change-order")
        .attr("id", order.id.0.to_string())
        .attr("number", order.number.to_string())
        .attr("title", order.title.as_str())
        .attr("status", order.status.key())
        .attr_opt("basis", non_empty(order.basis.key()))
        .attr_opt("reason", non_empty(&order.reason))
        .attr_opt("offer-number", non_empty(&order.offer_number))
        .attr_opt("offer-date", non_empty(&order.offer_date))
        .attr_opt("checked-date", non_empty(&order.checked_date))
        .attr_opt("checked-by", non_empty(&order.checked_by))
        .attr_opt("check-note", non_empty(&order.check_note))
        .attr_opt("decision-date", non_empty(&order.decision_date))
        .attr("discount", if order.discount_applies { "1" } else { "0" })
        .attr("forecast", if order.in_forecast { "1" } else { "0" })
        .attr_opt("contractor", non_empty(&order.contractor))
        .attr_opt("announced", non_empty(&order.announced_date))
        .attr_opt("offer-due", non_empty(&order.offer_due))
        .attr_opt("decision-due", non_empty(&order.decision_due))
        .attr_opt("recommendation", non_empty(&order.recommendation))
        .attr_opt("group", order.group.map(|row| row.0.to_string()))
        .children(order.history.iter().map(|step| {
            Element::new("step")
                .attr("status", step.status.key())
                .attr_opt("date", non_empty(&step.date))
                .attr_opt("note", non_empty(&step.note))
        }))
        .children(order.items.iter().map(|item| {
            Element::new("item")
                .attr("row", item.position.0.to_string())
                .attr(
                    "change",
                    match item.change {
                        ItemChange::NewPosition => "new",
                        ItemChange::Quantity => "quantity",
                    },
                )
                .attr("offered-quantity", text(&item.offered_quantity))
                .attr_opt("offered-price", item.offered_unit_price.as_ref().map(text))
                .attr_opt("checked-quantity", item.checked_quantity.as_ref().map(text))
                .attr_opt("checked-price", item.checked_unit_price.as_ref().map(text))
                .attr_opt("note", non_empty(&item.note))
                .attr_opt("status", item.status.map(ChangeOrderStatus::key))
        }))
}

fn change_order_of(node: &Element) -> Result<ChangeOrder, String> {
    let mut items = Vec::new();
    for item in node.all("item") {
        let Some(row) = number::<u64>(item, "row")? else {
            continue;
        };
        items.push(ChangeOrderItem {
            position: RowId(row),
            change: match item.get("change") {
                Some("quantity") => ItemChange::Quantity,
                _ => ItemChange::NewPosition,
            },
            offered_quantity: optional_decimal(item, "offered-quantity", "Nachtragsmenge")?
                .unwrap_or_default(),
            offered_unit_price: optional_decimal(item, "offered-price", "angebotener EP")?,
            checked_quantity: optional_decimal(item, "checked-quantity", "geprüfte Menge")?,
            checked_unit_price: optional_decimal(item, "checked-price", "geprüfter EP")?,
            note: item.text("note"),
            status: item
                .get("status")
                .map(ChangeOrderStatus::parse)
                .transpose()?,
        });
    }
    Ok(ChangeOrder {
        id: ChangeOrderId(number::<u64>(node, "id")?.unwrap_or(0)),
        number: number::<u32>(node, "number")?.unwrap_or(0),
        title: node.text("title"),
        basis: ChangeBasis::from_key(&node.text("basis")),
        reason: node.text("reason"),
        status: node
            .get("status")
            .map(ChangeOrderStatus::parse)
            .transpose()?
            .unwrap_or_default(),
        offer_number: node.text("offer-number"),
        offer_date: node.text("offer-date"),
        checked_date: node.text("checked-date"),
        checked_by: node.text("checked-by"),
        check_note: node.text("check-note"),
        decision_date: node.text("decision-date"),
        discount_applies: node.get("discount") != Some("0"),
        in_forecast: node.get("forecast") != Some("0"),
        items,
        contractor: node.text("contractor"),
        announced_date: node.text("announced"),
        offer_due: node.text("offer-due"),
        decision_due: node.text("decision-due"),
        recommendation: node.text("recommendation"),
        group: number::<u64>(node, "group")?.map(RowId),
        history: node
            .all("step")
            .map(|step| {
                Ok(StatusChange {
                    status: ChangeOrderStatus::parse(&step.text("status"))?,
                    date: step.text("date"),
                    note: step.text("note"),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

// ---------- billing ----------

/// One takeoff line `<line label=… formula=… invoice=…/>` – the Aufmaß of the
/// Abrechnung and the Mengenermittlung of a position (without invoice).
pub(crate) fn takeoff_line_element(line: &TakeoffLine) -> Element {
    Element::new("line")
        .attr("label", line.label.as_str())
        .attr("formula", line.formula.as_str())
        .attr_opt("invoice", line.invoice.map(|id| id.0.to_string()))
}

pub(crate) fn takeoff_line_of(line: &Element) -> Result<TakeoffLine, String> {
    Ok(TakeoffLine {
        label: line.text("label"),
        formula: line.text("formula"),
        invoice: number::<u64>(line, "invoice")?.map(InvoiceId),
    })
}

fn billing_element(billing: &Billing) -> Element {
    let mut element =
        Element::new("billing").attr("next-id", billing.next_invoice_id().to_string());
    for invoice in billing.invoices() {
        element.push(invoice_element(invoice));
    }
    for (row, lines) in billing
        .takeoff()
        .iter()
        .filter(|(_, lines)| !lines.is_empty())
    {
        element.push(
            Element::new("takeoff")
                .attr("row", row.0.to_string())
                .children(lines.iter().map(takeoff_line_element)),
        );
    }
    element
}

fn invoice_element(invoice: &Invoice) -> Element {
    let mut element = Element::new("invoice")
        .attr("id", invoice.id.0.to_string())
        .attr("number", invoice.number.as_str())
        .attr("kind", invoice.kind.key())
        .attr("quantities", invoice.quantities.key())
        .attr("release", invoice.release.key())
        .attr(
            "vat",
            match invoice.vat {
                VatRule::Rate(percent) => text(&percent),
                VatRule::ReverseCharge => "reverse-charge".to_string(),
            },
        )
        .attr_opt("date", non_empty(&invoice.date))
        .attr_opt("received", non_empty(&invoice.received))
        .attr_opt("due", non_empty(&invoice.due))
        .attr_opt("period-start", non_empty(&invoice.period_start))
        .attr_opt("period-end", non_empty(&invoice.period_end))
        .attr_opt("claimed-net", invoice.claimed_net.map(money_text))
        .attr_opt("claimed-lump", invoice.claimed_lump.map(money_text))
        .attr_opt("checked-lump", invoice.checked_lump.map(money_text))
        .attr_opt("security", invoice.security_percent.as_ref().map(text))
        .attr_opt("credited", invoice.credited.map(money_text))
        .attr_opt(
            "correction",
            invoice.correction.as_ref().map(|c| money_text(c.amount)),
        )
        .attr_opt(
            "correction-text",
            invoice.correction.as_ref().and_then(|c| non_empty(&c.text)),
        )
        .attr_opt("security-release", invoice.security_release.map(money_text))
        .attr_opt("note", non_empty(&invoice.note))
        .attr_opt("checked-date", non_empty(&invoice.checked_date))
        .attr_opt("checked-by", non_empty(&invoice.checked_by));
    if let Some(deductions) = &invoice.deductions {
        element.push(
            Element::new("deductions").children(deductions.iter().map(|deduction| {
                Element::new("deduction")
                    .attr("label", deduction.label.as_str())
                    .attr("percent", text(&deduction.percent))
            })),
        );
    }
    for (row, item) in &invoice.items {
        element.push(
            Element::new("item")
                .attr("row", row.0.to_string())
                .attr_opt("claimed-quantity", item.claimed_quantity.as_ref().map(text))
                .attr_opt("claimed-price", item.claimed_unit_price.as_ref().map(text))
                .attr_opt("claimed-total", item.claimed_total.as_ref().map(text))
                .attr_opt("checked-quantity", item.checked_quantity.as_ref().map(text))
                .attr_opt("progress", item.progress.as_ref().map(|p| text(&p.factor)))
                .attr_opt(
                    "progress-note",
                    item.progress.as_ref().and_then(|p| non_empty(&p.note)),
                )
                .attr_opt("basis", item.basis.map(CheckBasis::key))
                .attr_opt("note", non_empty(&item.note))
                .flag("evidence", item.evidence),
        );
    }
    element
}

fn billing_of(node: &Element) -> Result<Billing, String> {
    let mut billing = Billing::default();
    for invoice in node.all("invoice") {
        billing.add_invoice(invoice_of(invoice)?);
    }
    for takeoff in node.all("takeoff") {
        let Some(row) = number::<u64>(takeoff, "row")? else {
            continue;
        };
        let lines = billing.takeoff_mut(RowId(row));
        for line in takeoff.all("line") {
            lines.push(takeoff_line_of(line)?);
        }
    }
    if let Some(next) = number::<u64>(node, "next-id")? {
        billing.reserve_invoice_ids(next);
    }
    Ok(billing)
}

fn invoice_of(node: &Element) -> Result<Invoice, String> {
    let kind_text = node.text("kind");
    let kind = InvoiceKind::from_key(&kind_text)
        .ok_or_else(|| format!("Rechnungsart „{kind_text}“ ist unbekannt."))?;
    let mode = |key: &str| Mode::from_key(&node.text(key)).unwrap_or_default();
    let vat = match node.get("vat") {
        Some("reverse-charge") => VatRule::ReverseCharge,
        Some(percent) => VatRule::Rate(decimal(percent, "USt-Satz")?),
        None => VatRule::default(),
    };
    let correction = optional_money(node, "correction", "Korrektur")?.map(|amount| Correction {
        amount,
        text: node.text("correction-text"),
    });
    let deductions = node
        .first("deductions")
        .map(|list| {
            list.all("deduction")
                .map(|deduction| {
                    Ok(proava_domain::Deduction {
                        label: deduction.text("label"),
                        percent: decimal(deduction.get("percent").unwrap_or("0"), "Abzug")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    let mut items = BTreeMap::new();
    for item in node.all("item") {
        let Some(row) = number::<u64>(item, "row")? else {
            continue;
        };
        let progress =
            optional_decimal(item, "progress", "Leistungsfortschritt")?.map(|factor| Progress {
                factor,
                note: item.text("progress-note"),
            });
        items.insert(
            RowId(row),
            InvoiceItem {
                claimed_quantity: optional_decimal(item, "claimed-quantity", "Rechnungsmenge")?,
                claimed_unit_price: optional_decimal(item, "claimed-price", "EP der Rechnung")?,
                claimed_total: optional_decimal(item, "claimed-total", "GP der Rechnung")?,
                checked_quantity: optional_decimal(item, "checked-quantity", "geprüfte Menge")?,
                progress,
                basis: item.get("basis").and_then(CheckBasis::from_key),
                note: item.text("note"),
                evidence: item.is_set("evidence"),
            },
        );
    }
    Ok(Invoice {
        id: InvoiceId(number::<u64>(node, "id")?.unwrap_or(0)),
        number: node.text("number"),
        date: node.text("date"),
        received: node.text("received"),
        due: node.text("due"),
        period_start: node.text("period-start"),
        period_end: node.text("period-end"),
        kind,
        quantities: mode("quantities"),
        release: mode("release"),
        claimed_net: optional_money(node, "claimed-net", "Rechnungsbetrag")?,
        claimed_lump: optional_money(node, "claimed-lump", "pauschale Forderung")?,
        checked_lump: optional_money(node, "checked-lump", "pauschal geprüft")?,
        items,
        vat,
        security_percent: optional_decimal(node, "security", "Sicherheitseinbehalt")?,
        deductions,
        credited: optional_money(node, "credited", "angerechnete Freigaben")?,
        correction,
        security_release: optional_money(node, "security-release", "Sicherheitsfreigabe")?,
        note: node.text("note"),
        checked_date: node.text("checked-date"),
        checked_by: node.text("checked-by"),
    })
}
