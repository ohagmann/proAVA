//! Nachträge nach außen: Nachtrags-LV als GAEB (X83 Nachtragsangebots-
//! aufforderung, X86 Nachtragsauftrag, je mit Nachtragsnummer CONo und
//! Status COStatus), Nachtragsangebot der Firma einlesen, und die Daten der
//! PDFs „Prüfung Nachtragsangebot“ und „Nachtragsübersicht“.

use std::collections::HashSet;

use proava_application::{ChangeOrderExport, ChangeOrderPrint};
use proava_domain::{
    change_order_overview, deviation_percent, format_decimal_de, sums_by_status, Address,
    ChangeOrder, ChangeOrderId, ChangeOrderOffer, ChangeOrderStatus, Decimal, ItemChange, LvRow,
    Money, Project, ProjectName, RowId,
};
use proava_gaeb::{BoqNode, ChangeOrderTag, GaebDocument, Phase};
use proava_print::{
    ChangeOrderCheckLine, ChangeOrderOverviewLine, PrintChangeOrderCheck, PrintChangeOrderOverview,
};

use crate::text::address_to_gaeb;
use crate::{gaeb_to_project, lv_to_gaeb};

fn find(project: &Project, id: ChangeOrderId) -> Result<&ChangeOrder, String> {
    project
        .change_order(id)
        .ok_or_else(|| "Den Nachtrag gibt es nicht (mehr).".to_string())
}

/// The project with only the new positions of the change order (and the
/// titles and Ausführungsbeschreibungen they need); quantities and prices
/// as offered (X83: no prices) or as checked (X86).
fn project_of_change_order(
    project: &Project,
    order: &ChangeOrder,
    kind: ChangeOrderExport,
) -> (Project, Vec<ChangeOrderStatus>) {
    let contract = project.contract();
    let wanted: Vec<(RowId, Decimal, Option<Decimal>, ChangeOrderStatus)> = order
        .items
        .iter()
        .filter(|item| item.change == ItemChange::NewPosition)
        .map(|item| {
            let (quantity, price) = match kind {
                ChangeOrderExport::Request => (item.offered_quantity, None),
                ChangeOrderExport::Order => (item.checked_quantity(), item.checked_price(contract)),
            };
            (item.position, quantity, price, order.item_status(item))
        })
        .collect();
    let keep_positions: HashSet<RowId> = wanted.iter().map(|(row, ..)| *row).collect();
    let mut copy = project.clone();
    let lv = copy.lv_mut();
    // Rows to keep: the positions, their titles, their Ausführungsbeschreibungen.
    let mut keep = vec![false; lv.rows().len()];
    let mut performances = HashSet::new();
    for (index, row) in lv.rows().iter().enumerate() {
        let Some(position) = row
            .as_position()
            .filter(|p| keep_positions.contains(&p.id()))
        else {
            continue;
        };
        keep[index] = true;
        if let Some(performance) = position.performance() {
            performances.insert(performance);
        }
        let mut level = usize::MAX;
        for parent in (0..index).rev() {
            if let LvRow::Group(group) = &lv.rows()[parent] {
                if group.level() < level {
                    keep[parent] = true;
                    level = group.level();
                    if level == 0 {
                        break;
                    }
                }
            }
        }
    }
    for (index, row) in lv.rows().iter().enumerate() {
        if performances.contains(&row.id()) {
            keep[index] = true;
        }
    }
    for index in (0..keep.len()).rev() {
        if !keep[index] {
            lv.remove_row(index);
        }
    }
    let mut statuses = Vec::new();
    for index in 0..lv.rows().len() {
        let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) else {
            continue;
        };
        if let Some((_, quantity, price, status)) =
            wanted.iter().find(|(row, ..)| *row == position.id())
        {
            position.set_quantity(*quantity);
            position.set_unit_price(*price);
            // „entfällt“ gilt für den Hauptauftrag; im Nachtrag ist sie gefordert.
            position.set_not_applicable(false);
            position.set_price_components(Vec::new());
            statuses.push(*status);
        }
    }
    (copy, statuses)
}

/// Sets CONo/COStatus on every item in document order; with `dropped` the
/// item is not part of the order (entfällt, without total).
fn tag_items(nodes: &mut [BoqNode], tags: &mut impl Iterator<Item = (ChangeOrderTag, bool)>) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => tag_items(&mut category.body, tags),
            BoqNode::Item(item) => {
                if let Some((tag, dropped)) = tags.next() {
                    item.change_order = Some(tag);
                    item.not_applicable |= dropped;
                }
            }
            _ => {}
        }
    }
}

/// Nachtrags-LV of one change order of the active LV as GAEB document:
/// X83 (positions as offered quantities, no prices, status „angemeldet“)
/// or X86 (checked quantities and prices, status per position). Mehr-/
/// Mindermengen of contract positions are not part of it (GAEB has no
/// Teilmengen) – a warning says so.
pub fn change_order_to_gaeb(
    project: &Project,
    id: ChangeOrderId,
    kind: ChangeOrderExport,
) -> Result<(GaebDocument, Vec<String>), String> {
    let order = find(project, id)?;
    let (copy, statuses) = project_of_change_order(project, order, kind);
    if statuses.is_empty() {
        return Err(format!(
            "{} hat keine neuen Positionen im LV – Mehr-/Mindermengen allein lassen sich in GAEB nicht übergeben.",
            order.label()
        ));
    }
    let phase = match kind {
        ChangeOrderExport::Request => Phase::Dp83,
        ChangeOrderExport::Order => Phase::Dp86,
    };
    let (mut doc, export_warnings) = lv_to_gaeb(&copy, phase);
    let mut warnings: Vec<String> = export_warnings
        .into_iter()
        .filter(|warning| {
            kind == ChangeOrderExport::Order
                || !warning.starts_with("Positionen ohne Einheitspreis")
        })
        .collect();
    let quantity_changes = order
        .items
        .iter()
        .filter(|item| item.change == ItemChange::Quantity)
        .count();
    if quantity_changes > 0 {
        warnings.push(if quantity_changes == 1 {
            "1 Mehr- oder Mindermenge einer Auftragsposition steht nicht in der Datei (GAEB kennt keine Teilmengen) – sie steht im PDF „Prüfung Nachtragsangebot“.".to_string()
        } else {
            format!(
                "{quantity_changes} Mehr- oder Mindermengen von Auftragspositionen stehen nicht in der Datei (GAEB kennt keine Teilmengen) – sie stehen im PDF „Prüfung Nachtragsangebot“."
            )
        });
    }
    let boq = &mut doc.award.boq;
    boq.label = format!("{} {}", order.label(), order.title.trim())
        .trim()
        .to_string();
    let status = match kind {
        ChangeOrderExport::Request => "Filed".to_string(),
        ChangeOrderExport::Order => order.status.gaeb().to_string(),
    };
    boq.change_order = Some(ChangeOrderTag {
        number: order.number,
        status: status.clone(),
    });
    // X86: rejected and withdrawn positions stay visible with their status
    // but „entfallen“ (NotAppl), so the totals are the ordered amount.
    let mut tags = statuses.into_iter().map(|item_status| {
        let tag = ChangeOrderTag {
            number: order.number,
            status: match kind {
                ChangeOrderExport::Request => status.clone(),
                ChangeOrderExport::Order => item_status.gaeb().to_string(),
            },
        };
        let dropped = kind == ChangeOrderExport::Order
            && matches!(
                item_status,
                ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
            );
        (tag, dropped)
    });
    tag_items(&mut boq.body, &mut tags);
    if kind == ChangeOrderExport::Order {
        // Totals again without the dropped positions (the writer fills them in).
        fn clear(nodes: &mut [BoqNode]) {
            for node in nodes {
                match node {
                    BoqNode::Category(category) => {
                        category.totals = None;
                        clear(&mut category.body);
                    }
                    BoqNode::Item(item) if item.not_applicable => item.total = None,
                    _ => {}
                }
            }
        }
        clear(&mut boq.body);
        boq.totals = None;
    }
    if kind == ChangeOrderExport::Order {
        if order.status != ChangeOrderStatus::Approved {
            warnings.push(format!(
                "{} ist {} – ein Nachtragsauftrag (X86) setzt üblicherweise „beauftragt“ voraus.",
                order.label(),
                order.status.label()
            ));
        }
        let contract = project.contract();
        let bidder = contract
            .and_then(|contract| contract.bid)
            .and_then(|bid| project.bids().iter().find(|known| known.id() == bid))
            .map(|bid| bid.bidder().clone())
            .unwrap_or_else(|| Address::named(order.firm(contract)));
        let mut bidder = bidder;
        if !order.contractor.trim().is_empty() {
            bidder = Address::named(order.contractor.trim());
        }
        doc.award.bidder = Some(address_to_gaeb(&bidder));
    }
    Ok((doc, warnings))
}

fn german_date(iso: &str) -> String {
    let iso = iso.trim();
    match (iso.get(0..4), iso.get(5..7), iso.get(8..10)) {
        (Some(year), Some(month), Some(day)) if iso.len() >= 10 => format!("{day}.{month}.{year}"),
        _ => iso.to_string(),
    }
}

/// A contractor's change-order offer from a GAEB document: the LV of the
/// file as project, Nachtragsnummer, date and firm.
pub fn change_order_offer_from_gaeb(
    doc: &GaebDocument,
    file: &str,
) -> (ChangeOrderOffer, Vec<String>) {
    let name = ProjectName::parse("Nachtragsangebot").expect("valid name");
    let (source, mut warnings) = gaeb_to_project(doc, name, "");
    // The prices are the offer here, not planner prices.
    warnings.retain(|warning| !warning.contains("als Planerpreise übernommen"));
    if !doc.phase.has_prices() {
        warnings.push(format!(
            "Die Datei ist eine X{} ohne Preise – nur Mengen und Texte werden übernommen.",
            doc.phase.number()
        ));
    }
    let number = doc
        .award
        .boq
        .change_order
        .as_ref()
        .map(|tag| tag.number)
        .or_else(|| {
            doc.award
                .boq
                .items()
                .iter()
                .find_map(|(_, item)| item.change_order.as_ref().map(|tag| tag.number))
        });
    let date = doc
        .award
        .bid_date
        .as_deref()
        .or(doc.date.as_deref())
        .map(german_date)
        .unwrap_or_default();
    let firm = doc
        .award
        .bidder
        .as_ref()
        .map(|bidder| bidder.name1.trim().to_string())
        .unwrap_or_default();
    (
        ChangeOrderOffer {
            source,
            number,
            date,
            firm,
            file: file.to_string(),
        },
        warnings,
    )
}

/// „1.234,56 €“ in the Währung of the LV.
fn money_in(value: Money, currency: &str) -> String {
    proava_domain::price_check::amount(value, currency)
}

fn price(value: Option<Decimal>) -> String {
    value
        .map(|value| format_decimal_de(value, 2))
        .unwrap_or_else(|| "–".into())
}

fn quantity(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    if text.contains(',') {
        text.trim_end_matches('0').trim_end_matches(',').to_string()
    } else {
        text
    }
}

fn percent(value: Option<Decimal>) -> String {
    value
        .filter(|value| !value.is_zero())
        .map(|value| format_decimal_de(value, 1))
        .unwrap_or_default()
}

/// PDF data of the Prüfung Nachtragsangebot.
pub fn change_order_check_to_print(
    project: &Project,
    id: ChangeOrderId,
    date: &str,
) -> Result<PrintChangeOrderCheck, String> {
    let currency = project.lv().currency().to_string();
    let money = |value: Money| money_in(value, &currency);
    let order = find(project, id)?;
    let contract = project.contract();
    let lv = project.lv();
    let mut print = PrintChangeOrderCheck {
        header: crate::to_print::header(project, date),
        title: format!("{} {}", order.label(), order.title.trim())
            .trim()
            .to_string(),
        reason: order.reason.trim().to_string(),
        check_note: order.check_note.trim().to_string(),
        recommendation: order.recommendation.trim().to_string(),
        signatures: vec![
            "Ort, Datum, Unterschrift Planer".to_string(),
            "Ort, Datum, Unterschrift Auftraggeber".to_string(),
        ],
        ..PrintChangeOrderCheck::default()
    };
    let mut fact = |label: &str, value: String| {
        if !value.trim().is_empty() {
            print
                .facts
                .push((label.to_string(), value.trim().to_string()));
        }
    };
    fact("Auftragnehmer", order.firm(contract).to_string());
    if let Some(contract) = contract {
        let mut text = contract.number.clone();
        if !contract.date.is_empty() {
            text = format!("{text} vom {}", contract.date).trim().to_string();
        }
        fact("Auftrag", text);
    }
    fact("Anlass", order.basis.label().to_string());
    let mut offer = order.offer_number.clone();
    if !order.offer_date.is_empty() {
        offer = format!("{offer} vom {}", order.offer_date)
            .trim()
            .to_string();
    }
    fact("Nachtragsangebot", offer);
    fact("Angekündigt am", order.announced_date.clone());
    let status = match order.status_date() {
        "" => order.status.label().to_string(),
        date => format!("{} seit {date}", order.status.label()),
    };
    fact("Status", status);

    for item in &order.items {
        let (ordinal, short_text, unit) = match lv.position_by_id(item.position) {
            Some(position) => (
                position.ordinal().to_string(),
                position.short_text().to_string(),
                position.unit().to_string(),
            ),
            None => contract
                .and_then(|contract| contract.item(item.position))
                .map(|c| (c.ordinal.clone(), c.short_text.clone(), c.unit.clone()))
                .unwrap_or_default(),
        };
        let item_status = order.item_status(item);
        let dropped = matches!(
            item_status,
            ChangeOrderStatus::Rejected | ChangeOrderStatus::Withdrawn
        );
        let offered = item.offered_price(contract);
        let checked = item.checked_price(contract);
        print.lines.push(ChangeOrderCheckLine {
            ordinal,
            short_text,
            kind: match item.change {
                ItemChange::NewPosition => "neu".to_string(),
                ItemChange::Quantity if item.offered_quantity.is_sign_negative() => {
                    "Mindermenge".to_string()
                }
                ItemChange::Quantity => "Mehrmenge".to_string(),
            },
            unit,
            offered_quantity: quantity(item.offered_quantity),
            offered_price: price(offered),
            offered_total: money(item.offered_total(contract)),
            checked_quantity: quantity(item.checked_quantity()),
            checked_price: price(checked),
            checked_total: money(item.checked_total(contract)),
            deviation: match (checked, offered) {
                (Some(checked), Some(offered)) => percent(deviation_percent(checked, offered)),
                _ => String::new(),
            },
            status: item
                .status
                .filter(|status| *status != order.status)
                .map(|status| status.label().to_string())
                .unwrap_or_default(),
            note: item.note.trim().to_string(),
            dropped,
        });
    }
    let offered = order.offered_total(contract);
    let checked = order.checked_total(contract);
    let mut sums = vec![
        ("Angebotssumme netto".to_string(), money(offered), false),
        (
            "Geprüfte Nachtragssumme netto".to_string(),
            money(checked),
            true,
        ),
    ];
    let discount = order.discount_on(checked, contract);
    if !discount.is_zero() {
        let percent = contract
            .map(|contract| contract.discount_percent)
            .unwrap_or_default();
        sums.push((
            format!(
                "Nachlass des Auftrags {} %",
                format_decimal_de(percent, percent.normalize().scale().min(6))
            ),
            money(Money::ZERO - discount),
            false,
        ));
        sums.push((
            "Geprüft nach Nachlass netto".to_string(),
            money(checked - discount),
            true,
        ));
    }
    if let Some(value) = deviation_percent(checked.to_decimal(), offered.to_decimal())
        .filter(|value| !value.is_zero())
    {
        sums.push((
            "Abweichung geprüft gegen angeboten".to_string(),
            format!("{} %", format_decimal_de(value, 1)),
            false,
        ));
    }
    let credit = order.credit_total(contract);
    if credit.cents() < 0 {
        sums.push((
            "davon Minderungen / Gutschrift".to_string(),
            money(credit),
            false,
        ));
    }
    print.sums = sums;
    let by = order.checked_by.trim();
    let on = order.checked_date.trim();
    if !by.is_empty() || !on.is_empty() {
        let mut text = "Fachtechnisch und rechnerisch geprüft".to_string();
        if !on.is_empty() {
            text.push_str(&format!(" am {on}"));
        }
        if !by.is_empty() {
            text.push_str(&format!(" von {by}"));
        }
        text.push_str(&format!(
            "; geprüfte Nachtragssumme {} netto.",
            money(checked - discount)
        ));
        print.checked = text;
    }
    print.history = order
        .history
        .iter()
        .map(|step| {
            let mut text = format!("{} {}", step.date, step.status.label())
                .trim()
                .to_string();
            if !step.note.trim().is_empty() {
                text.push_str(&format!(" – {}", step.note.trim()));
            }
            text
        })
        .collect();
    print.notes = order.problems(contract, lv);
    Ok(print)
}

/// PDF data of the Nachtragsübersicht of the active LV.
pub fn change_order_overview_to_print(project: &Project, date: &str) -> PrintChangeOrderOverview {
    let currency = project.lv().currency().to_string();
    let money = |value: Money| money_in(value, &currency);
    let contract = project.contract();
    let overview = change_order_overview(contract, project.change_orders());
    let mut print = PrintChangeOrderOverview {
        header: crate::to_print::header(project, date),
        ..PrintChangeOrderOverview::default()
    };
    if let Some(contract) = contract {
        print
            .facts
            .push(("Auftragnehmer".to_string(), contract.contractor.clone()));
        let mut text = contract.number.clone();
        if !contract.date.is_empty() {
            text = format!("{text} vom {}", contract.date).trim().to_string();
        }
        print.facts.push(("Auftrag".to_string(), text));
    }
    for line in &overview.lines {
        let order = project.change_order(line.id);
        let forecast = order.is_some_and(|order| {
            order.status == ChangeOrderStatus::Approved
                || (order.status.is_open() && order.in_forecast)
        });
        let basis = line.basis.label();
        print.lines.push(ChangeOrderOverviewLine {
            label: line.label.clone(),
            title: line.title.clone(),
            basis: basis.split(" (").next().unwrap_or(basis).to_string(),
            reason: line.reason.trim().to_string(),
            status: line.status.label().to_string(),
            date: order
                .map(|order| order.status_date().to_string())
                .unwrap_or_default(),
            offered: money(line.offered),
            checked: money(line.checked),
            approved: money(line.approved),
            credit: if line.credit.is_zero() {
                String::new()
            } else {
                money(line.credit)
            },
            forecast: if forecast { "ja" } else { "nein" }.to_string(),
        });
    }
    let credit: Money = overview.lines.iter().map(|line| line.credit).sum();
    print.total = [
        money(overview.offered),
        money(overview.checked),
        money(overview.approved),
        money(credit),
    ];
    print.by_status = sums_by_status(contract, project.change_orders())
        .into_iter()
        .filter(|sum| sum.count > 0)
        .map(|sum| {
            [
                sum.status.label().to_string(),
                sum.count.to_string(),
                money(sum.offered),
                money(sum.checked),
                money(sum.approved),
            ]
        })
        .collect();
    print.contract = vec![
        (
            "Auftragssumme alt (netto nach Nachlass)".to_string(),
            money(overview.contract_total),
            false,
        ),
        (
            "+ beauftragte Nachträge".to_string(),
            money(overview.approved),
            false,
        ),
        (
            "= Auftragssumme neu".to_string(),
            money(overview.total_with_change_orders),
            true,
        ),
        (
            "offene Nachträge (geprüft bzw. angeboten)".to_string(),
            money(overview.open),
            false,
        ),
        (
            "beauftragte Mehrungen / Minderungen".to_string(),
            format!(
                "{} / {}",
                money(overview.increases),
                money(overview.decreases)
            ),
            false,
        ),
        (
            "Prognose (mit offenen Nachträgen, die eingehen)".to_string(),
            money(overview.forecast),
            true,
        ),
    ];
    if contract.is_none() {
        print.notes.push(format!(
            "Noch kein Auftrag übernommen – Auftragssumme {}.",
            money(Money::ZERO)
        ));
    }
    print.notes.push(
        "Beträge netto. Beauftragt nach dem Nachlass des Auftrags, wo er gilt; Prognose mit offenen Nachträgen, die als „geht in die Prognose ein“ markiert sind."
            .to_string(),
    );
    print
}

/// What a printout of the Nachträge is called in the page header.
pub fn change_order_print_kind(print: ChangeOrderPrint) -> &'static str {
    match print {
        ChangeOrderPrint::Check(_) => "Prüfung Nachtragsangebot",
        ChangeOrderPrint::Overview => "Nachtragsübersicht",
    }
}

#[cfg(test)]
mod tests;
