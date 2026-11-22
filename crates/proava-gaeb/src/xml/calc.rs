//! Totals as GAEB computes them (Fachdokumentation 3.3, 4.6.17 Rundung).

use rust_decimal::{Decimal, RoundingStrategy};

use crate::model::{BoqNode, Category, Item, Provisional, Totals};

fn round2(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Full ordinal number of an item: level pieces, item and index joined with ".".
pub fn ordinal(path: &[String], item: &Item) -> String {
    let mut pieces: Vec<&str> = path.iter().map(String::as_str).collect();
    pieces.push(&item.rno_part);
    if !item.rno_index.is_empty() {
        pieces.push(&item.rno_index);
    }
    pieces.join(".")
}

/// Whether the item is a Zuschlagsposition (MarkupItem in DA XML, record 23
/// with ZUSCHPR/ZUSCHSU in GAEB 90).
pub fn is_surcharge(item: &Item) -> bool {
    item.surcharge.is_some() || item.markup_percent.is_some() || item.markup_base.is_some()
}

/// Wahlbereich (title with ALNBSerNo > 0): not part of the totals.
pub fn is_alternative_area(category: &Category) -> bool {
    category
        .alternative
        .as_ref()
        .is_some_and(|alternative| !alternative.serial.trim().trim_start_matches('0').is_empty())
}

/// Whether the item shows a total price at all (not "Nur Einh.-Preis").
pub fn shows_total(item: &Item) -> bool {
    let alternative =
        item.kind.alternative.as_ref().is_some_and(|alternative| {
            !alternative.serial.trim().trim_start_matches('0').is_empty()
        });
    !alternative
        && item.kind.provisional != Some(Provisional::WithoutTotal)
        && !(item.kind.provisional == Some(Provisional::WithTotal)
            && item.kind.alternative.is_some())
        && !item.not_applicable
}

/// Whether the item's total is part of the title and LV totals
/// (Bedarfspositionen mit GB count, as in the GAEB examples).
pub fn counts_in_total(item: &Item) -> bool {
    shows_total(item)
}

/// Fills missing totals as the writer computes them: item totals (IT) where
/// a total is shown, title and LV totals. Values already set stay.
pub fn fill_totals(boq: &mut crate::model::Boq) {
    fn fill(nodes: &mut [BoqNode], path: &mut Vec<String>) {
        for node in nodes.iter_mut() {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    fill(&mut category.body, path);
                    let total = nodes_total(&category.body, path);
                    path.pop();
                    let totals = category.totals.get_or_insert_with(Totals::default);
                    totals.total.get_or_insert(total);
                }
                BoqNode::Item(item)
                    if item.total.is_none() && (shows_total(item) || is_surcharge(item)) =>
                {
                    item.total = item_total(item);
                }
                _ => {}
            }
        }
    }
    fill(&mut boq.body, &mut Vec::new());
    let total = nodes_total(&boq.body, &mut Vec::new());
    boq.totals
        .get_or_insert_with(Totals::default)
        .total
        .get_or_insert(total);
}

/// Total price of an item: quantity × unit price, rounded to cents (lump
/// sum items without quantity count once), less the item discount, rounded
/// again. Surcharge items: base × percentage when both are known.
pub fn item_total(item: &Item) -> Option<Decimal> {
    if is_surcharge(item) {
        let (Some(base), Some(percent)) = (item.markup_base, item.markup_percent) else {
            return None;
        };
        return Some(round2(base * percent / Decimal::ONE_HUNDRED));
    }
    let price = item.unit_price?;
    let qty = match item.qty {
        Some(qty) => qty,
        None if item.kind.lump_sum => Decimal::ONE,
        None => return None,
    };
    let total = round2(qty * price);
    Some(match item.discount_percent {
        Some(percent) => round2(total - total * percent / Decimal::ONE_HUNDRED),
        None => total,
    })
}

/// Total after a discount given in the file's totals (percentage or amount).
pub fn after_discount(total: Decimal, totals: Option<&Totals>) -> Decimal {
    let Some(totals) = totals else { return total };
    if let Some(percent) = totals.discount_percent {
        return round2(total - total * percent / Decimal::ONE_HUNDRED);
    }
    if let Some(amount) = totals.discount_amount {
        return round2(total - amount);
    }
    total
}

/// Sum of the nodes computed from quantities and unit prices, ignoring the
/// totals written in the file (discounts of titles apply). Items without
/// quantity (X84 often has none) count with their total price. `path` holds
/// the ordinal pieces of the titles above the nodes (empty for the LV).
pub fn computed_total(nodes: &[BoqNode], path: &[String]) -> Decimal {
    sum(nodes, &mut path.to_vec(), false)
}

/// Sum of the nodes preferring the totals written in the model.
pub(crate) fn nodes_total(nodes: &[BoqNode], path: &mut Vec<String>) -> Decimal {
    sum(nodes, path, true)
}

fn value(item: &Item, prefer_given: bool) -> Option<Decimal> {
    if prefer_given {
        item.total.or_else(|| item_total(item))
    } else {
        item_total(item).or(item.total)
    }
}

struct Entry<'a> {
    ordinal: String,
    item: &'a Item,
    value: Decimal,
}

fn sum(nodes: &[BoqNode], path: &mut Vec<String>, prefer_given: bool) -> Decimal {
    // Values of the ordinary items of this list, for the surcharge bases.
    let entries: Vec<Entry> = nodes
        .iter()
        .filter_map(|node| match node {
            BoqNode::Item(item) if !is_surcharge(item) => Some(Entry {
                ordinal: ordinal(path, item),
                item,
                value: if counts_in_total(item) {
                    value(item, prefer_given).unwrap_or_default()
                } else {
                    Decimal::ZERO
                },
            }),
            _ => None,
        })
        .collect();
    let mut total = Decimal::ZERO;
    let mut seen = 0usize;
    let mut group_start = 0usize;
    let mut group_end = 0usize;
    let mut last_was_surcharge = false;
    for node in nodes {
        match node {
            BoqNode::Category(category) if is_alternative_area(category) => {}
            BoqNode::Category(category) => {
                path.push(category.rno_part.clone());
                let inner = sum(&category.body, path, prefer_given);
                path.pop();
                let given = category.totals.as_ref();
                // A Pauschalsumme (TotalLSUM) replaces the title total.
                if let Some(lump_sum) = given.and_then(|totals| totals.lump_sum) {
                    total += lump_sum;
                    continue;
                }
                total += match given {
                    Some(totals) if prefer_given => totals
                        .total_after_discount
                        .or(totals.total)
                        .unwrap_or_else(|| after_discount(inner, Some(totals))),
                    _ => after_discount(inner, given),
                };
            }
            BoqNode::Item(item) => {
                if !is_surcharge(item) {
                    total += entries[seen].value;
                    seen += 1;
                    last_was_surcharge = false;
                    continue;
                }
                if !last_was_surcharge {
                    group_start = group_end;
                    group_end = seen;
                }
                last_was_surcharge = true;
                // Without a markup type (X84 short form, GAEB 90) the file
                // gives the surcharge itself: IT, else ITMarkup × Markup.
                let (markup_type, references) = match &item.surcharge {
                    Some(surcharge) if !surcharge.markup_type.is_empty() => {
                        (surcharge.markup_type.as_str(), surcharge.items.as_slice())
                    }
                    _ => {
                        if counts_in_total(item) {
                            total += item.total.or_else(|| item_total(item)).unwrap_or_default();
                        }
                        continue;
                    }
                };
                let computed = || -> Decimal {
                    match markup_type {
                        "ListInSubQty" => references
                            .iter()
                            .filter_map(|reference| {
                                let entry = entries
                                    .iter()
                                    .find(|entry| entry.ordinal == reference.ordinal)?;
                                match (reference.sub_qty, entry.item.unit_price) {
                                    (Some(qty), Some(price)) => Some(round2(qty * price)),
                                    _ => Some(entry.value),
                                }
                            })
                            .sum(),
                        // Both apply to the items since the previous surcharge
                        // block (as BVBS 3.3 AVA 1.30.20.0070 computes).
                        "IdentAsMark" => entries[group_start..group_end]
                            .iter()
                            .filter(|entry| entry.item.kind.markup)
                            .map(|entry| entry.value)
                            .sum(),
                        _ => entries[group_start..group_end]
                            .iter()
                            .map(|entry| entry.value)
                            .sum(),
                    }
                };
                let base = match item.markup_base {
                    Some(base) if prefer_given => base,
                    given => {
                        let base = computed();
                        if base.is_zero() {
                            given.unwrap_or(base)
                        } else {
                            base
                        }
                    }
                };
                let value = match (prefer_given, item.total) {
                    (true, Some(given)) => given,
                    _ => item
                        .markup_percent
                        .map(|percent| round2(base * percent / Decimal::ONE_HUNDRED))
                        .unwrap_or_default(),
                };
                if counts_in_total(item) {
                    total += value;
                }
            }
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
        }
    }
    total
}
