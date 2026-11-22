use std::collections::{HashMap, HashSet};

use proava_application::{BidImport, BidMatchOptions};
use proava_domain::{
    format_decimal_de, Address, AssignedBy, Assignment, Bid, BidComplement, BidItem, BidPrice,
    Decimal, Discount, Money, PositionKind, Project, RowId, StoredSuggestion, StructureReport,
};
use proava_gaeb::{BoqNode, GaebDocument, Item, Phase, Provisional};

use crate::oz::{match_key, position_keys, MaskLayout};
use crate::text::{address_from_gaeb, short_list};

const LIST_LIMIT: usize = 15;

/// Reads a bidder's GAEB document into a [`Bid`] for the project's LV.
/// Positions are matched by order number, compared without leading zeros;
/// when the full number fails, the number without index is used if it is
/// unique on both sides. Everything odd ends up in the warnings.
pub fn match_bid(project: &Project, doc: &GaebDocument, source: &str) -> BidImport {
    match_bid_with(project, doc, source, &BidMatchOptions::default())
}

/// The file item as the structure comparison sees it.
fn bid_item(pieces: &[String], item: &Item) -> BidItem {
    let mut long_text: String = item.long_text.to_plain();
    if long_text.chars().count() > proava_domain::bid_structure::LONG_TEXT_KEPT {
        long_text = long_text
            .chars()
            .take(proava_domain::bid_structure::LONG_TEXT_KEPT)
            .collect();
    }
    let kind = if let Some(alternative) = &item.kind.alternative {
        if alternative.serial.trim().trim_start_matches('0').is_empty() {
            PositionKind::Base { group: 0 }
        } else {
            PositionKind::Alternative {
                group: 0,
                serial: 1,
            }
        }
    } else {
        match item.kind.provisional {
            Some(Provisional::WithTotal) => PositionKind::Provisional { with_total: true },
            Some(Provisional::WithoutTotal) => PositionKind::Provisional { with_total: false },
            None if item.kind.markup => PositionKind::Supplement,
            None => PositionKind::Normal,
        }
    };
    let mut ordinal = pieces.join(".");
    if !item.rno_index.trim().is_empty()
        && !pieces
            .last()
            .is_some_and(|last| last == item.rno_index.trim())
    {
        ordinal = format!("{ordinal}.{}", item.rno_index.trim());
    }
    BidItem {
        ordinal: ordinal.split_whitespace().collect::<Vec<_>>().join(""),
        short_text: item.short_text.trim().to_string(),
        long_text: long_text.trim().to_string(),
        unit: item.unit.trim().to_string(),
        quantity: item.qty,
        unit_price: item.unit_price,
        total: item.total,
        not_offered: item.not_offered,
        kind: proava_domain::kind_label(kind, item.kind.lump_sum),
    }
}

/// [`match_bid`] with the way of matching: sure proposals of the structure
/// comparison taken over, assignments by hand.
pub fn match_bid_with(
    project: &Project,
    doc: &GaebDocument,
    source: &str,
    options: &BidMatchOptions,
) -> BidImport {
    let lv = project.lv();
    let layout = MaskLayout::of(lv.oz_mask());
    let mut warnings = Vec::new();

    if !matches!(doc.phase, Phase::Dp84 | Phase::Dp85) {
        warnings.push(format!(
            "Die Datei ist Phase {}, kein Angebot (84/85) – sie wird trotzdem gelesen.",
            doc.phase.number()
        ));
    }
    // proAVA writes the Kurzname (if set) as the project name of the X83:
    // both count as „ours“.
    let file_name = doc.project.name.trim();
    let short_name = project.info().short_name.trim();
    let ours = file_name.eq_ignore_ascii_case(project.name().as_str().trim())
        || (!short_name.is_empty() && file_name.eq_ignore_ascii_case(short_name));
    if !file_name.is_empty() && !ours {
        warnings.push(format!(
            "Projektname in der Datei „{file_name}“ weicht ab vom Projekt „{}“.",
            project.name()
        ));
    }

    let bidder = match &doc.award.bidder {
        Some(address) if !is_placeholder_name(&address.name1) => address_from_gaeb(address),
        _ => {
            warnings.push("Die Datei nennt keinen Bieter – bitte den Namen eintragen.".to_string());
            Address::named(format!("Bieter aus {source}"))
        }
    };
    let mut bid = Bid::new(bidder, source);
    bid.set_bid_date(doc.award.bid_date.clone().or_else(|| doc.date.clone()));
    bid.set_exchange_number(doc.award.bidder_dp_no.trim());

    let mut full: HashMap<String, RowId> = HashMap::new();
    let mut base: HashMap<String, Vec<RowId>> = HashMap::new();
    for position in lv.positions() {
        for (full_key, base_key) in position_keys(&layout, position.ordinal()) {
            full.entry(full_key).or_insert(position.id());
            base.entry(base_key).or_default().push(position.id());
        }
    }

    let mut unknown = Vec::new();
    let mut by_base = Vec::new();
    let mut twice = Vec::new();
    let mut arithmetic = Vec::new();
    let mut not_offered = Vec::new();
    let mut any_price = false;
    let mut markups = Vec::new();
    let mut markup_ids = Vec::new();
    let mut item_discounts = Vec::new();
    let mut derived = Vec::new();
    let mut filled_in = 0;

    let items = doc.award.boq.items();
    let base_keys: Vec<String> = items
        .iter()
        .map(|(pieces, item)| {
            if item.rno_index.trim().is_empty() {
                match_key(pieces)
            } else {
                match_key(&pieces[..pieces.len() - 1])
            }
        })
        .collect();
    let mut file_base_count: HashMap<&str, usize> = HashMap::new();
    for key in &base_keys {
        *file_base_count.entry(key.as_str()).or_default() += 1;
    }

    // 1. By OZ (the first of two equal ones counts).
    let mut by_ordinal: Vec<Option<RowId>> = Vec::with_capacity(items.len());
    let mut used: HashSet<RowId> = HashSet::new();
    let mut duplicate: HashSet<usize> = HashSet::new();
    for (index, ((pieces, _), base_key)) in items.iter().zip(&base_keys).enumerate() {
        let shown = pieces.join(".");
        let id = full.get(&match_key(pieces)).copied().or_else(|| {
            let candidates = base.get(base_key)?;
            (candidates.len() == 1 && file_base_count[base_key.as_str()] == 1).then(|| {
                by_base.push(shown.clone());
                candidates[0]
            })
        });
        match id {
            Some(id) if !used.insert(id) => {
                twice.push(shown);
                duplicate.insert(index);
                by_ordinal.push(None);
            }
            other => by_ordinal.push(other),
        }
    }

    // 2. Structure: the file items without duplicates, by hand first.
    let considered: Vec<usize> = (0..items.len())
        .filter(|index| !duplicate.contains(index))
        .collect();
    let file_items: Vec<BidItem> = considered
        .iter()
        .map(|&index| bid_item(&items[index].0, items[index].1))
        .collect();
    let mut manual = Vec::new();
    let mut manual_failed = Vec::new();
    for (file_ordinal, lv_ordinal) in &options.manual {
        let wanted = match_key(&file_ordinal.split('.').map(str::trim).collect::<Vec<_>>());
        let item = considered
            .iter()
            .position(|&index| match_key(&items[index].0) == wanted);
        let row = lv
            .position_by_ordinal(lv_ordinal)
            .map(|position| position.id())
            .or_else(|| {
                full.get(&match_key(
                    &lv_ordinal.split('.').map(str::trim).collect::<Vec<_>>(),
                ))
                .copied()
            });
        match (item, row) {
            (Some(item), Some(row)) => manual.push((item, row)),
            (None, _) => manual_failed.push(format!("{file_ordinal} (nicht in der Datei)")),
            (_, None) => manual_failed.push(format!("{lv_ordinal} (nicht im LV)")),
        }
    }
    let considered_by_ordinal: Vec<Option<RowId>> =
        considered.iter().map(|&index| by_ordinal[index]).collect();
    let alignment = proava_domain::align(
        lv,
        &file_items,
        &considered_by_ordinal,
        &manual,
        options.by_structure,
    );
    let mut rows: Vec<Option<RowId>> = vec![None; items.len()];
    for (sub, &index) in considered.iter().enumerate() {
        rows[index] = alignment.rows[sub];
    }
    let mut report = StructureReport::default();
    for (sub, similarity) in &alignment.by_structure {
        if let Some(row) = alignment.rows[*sub] {
            report.assigned.push(Assignment {
                item: file_items[*sub].clone(),
                row,
                by: AssignedBy::Structure,
                similarity: *similarity,
            });
        }
    }
    for (sub, row) in &manual {
        report.assigned.push(Assignment {
            item: file_items[*sub].clone(),
            row: *row,
            by: AssignedBy::Hand,
            similarity: proava_domain::check_pair(
                &file_items[*sub],
                lv.position_by_id(*row).expect("row of the LV"),
            )
            .similarity,
        });
    }
    for &sub in &alignment.conflicts {
        if let Some(row) = alignment.rows[sub] {
            report.conflicts.push((row, file_items[sub].clone()));
        }
    }
    report.suggestions = alignment
        .suggestions
        .iter()
        .map(|suggestion| StoredSuggestion {
            ordinal: file_items[suggestion.item].ordinal.clone(),
            row: suggestion.row,
            similarity: suggestion.similarity,
            sure: suggestion.sure,
        })
        .collect();
    for (sub, row) in alignment.rows.iter().enumerate() {
        if row.is_none() {
            report.foreign.push(file_items[sub].clone());
        }
    }
    // Only a file that names kinds at all is compared (GAEB 90 files of
    // some programs carry none).
    let plain_kind = proava_domain::kind_label(PositionKind::Normal, false);
    let file_kinds = file_items.iter().any(|item| item.kind != plain_kind);
    let lv_kinds = lv
        .positions()
        .any(|position| proava_domain::position_kind_label(position) != plain_kind);
    if !file_kinds && lv_kinds {
        warnings.push(
            "Die Datei nennt keine Positionsarten (Bedarf, Wahl, pauschal) – sie sind nicht verglichen.".to_string(),
        );
    }
    let item_of: HashMap<usize, usize> = considered
        .iter()
        .enumerate()
        .map(|(sub, &index)| (index, sub))
        .collect();

    for (index, (pieces, item)) in items.iter().enumerate() {
        let shown = pieces.join(".");
        if duplicate.contains(&index) {
            continue;
        }
        let Some(id) = rows[index] else {
            unknown.push(shown);
            continue;
        };
        let Some(position) = lv.position_by_id(id) else {
            continue;
        };
        // What the file changed at this position (the LV counts).
        let file_item = &file_items[item_of[&index]];
        // A conflict names its quantity itself.
        let conflict = alignment.conflicts.contains(&item_of[&index]);
        // Without a quantity in the file: GP ÷ EP when it comes out even
        // (GP = that quantity × EP to the cent) – a GP for another quantity
        // is a Mengenabweichung, no Rechenfehler.
        let derived_qty = file_item
            .priced_quantity()
            .filter(|_| item.qty.is_none() && !position.is_surcharge())
            .filter(|quantity| {
                let (Some(price), Some(total)) = (file_item.unit_price, file_item.total) else {
                    return false;
                };
                (Money::total(price, *quantity) - Money::from_decimal(total))
                    .cents()
                    .abs()
                    <= 1
            });
        if let Some(qty) = item.qty.or(derived_qty).filter(|_| !conflict) {
            if qty.normalize() != position.quantity().normalize() {
                report.quantities.push((id, qty));
            }
        }
        if !file_item.unit.is_empty()
            && !position.unit().trim().is_empty()
            && !proava_domain::same_unit(&file_item.unit, position.unit())
        {
            report.units.push((id, file_item.unit.clone()));
        }
        if !file_item.short_text.is_empty()
            && !position.short_text().trim().is_empty()
            && proava_domain::normalized(&file_item.short_text)
                != proava_domain::normalized(position.short_text())
        {
            report.short_texts.push((id, file_item.short_text.clone()));
        }
        let long_plain = position.long_text().to_plain();
        if !file_item.long_text.is_empty() && !long_plain.trim().is_empty() {
            let similarity = proava_domain::text_similarity(&file_item.long_text, &long_plain);
            if similarity < 0.7 {
                report
                    .long_texts
                    .push((id, (similarity * 100.0).round() as u8));
            }
        }
        if file_kinds
            && !position.is_surcharge()
            && file_item.kind != proava_domain::position_kind_label(position)
        {
            report.kinds.push((id, file_item.kind.clone()));
        }

        // Zuschlagsposition of the LV: the bidder's percentage is its price
        // (X84 Markup), the stated amount is checked like a GP.
        if position.is_surcharge() {
            if item.not_offered {
                not_offered.push(position.ordinal().to_string());
            }
            let percent = item.markup_percent.or(item.unit_price);
            any_price |= percent.is_some();
            bid.set_price(
                id,
                BidPrice {
                    unit_price: percent,
                    stated_total: item.total,
                    not_offered: item.not_offered,
                },
            );
            continue;
        }
        let mut unit_price = item.unit_price;
        let qty = item.qty.or(derived_qty).unwrap_or(position.quantity());
        if let (None, Some(stated)) = (unit_price, item.total) {
            if !qty.is_zero() && item.markup_percent.is_none() {
                unit_price = Some((stated / qty).round_dp(3));
                derived.push(position.ordinal().to_string());
            }
        }
        if let (Some(unit_price), Some(stated), None) =
            (item.unit_price, item.total, item.markup_percent)
        {
            let computed = Money::total(unit_price, qty);
            let stated_money = Money::from_decimal(stated);
            if (stated_money - computed).cents().abs() > 1 {
                arithmetic.push(format!(
                    "{} (Datei {}, gerechnet {})",
                    position.ordinal(),
                    stated_money,
                    computed
                ));
            }
        }
        if item.not_offered {
            not_offered.push(position.ordinal().to_string());
        }
        if let (Some(percent), None) = (item.markup_percent, unit_price) {
            markups.push(format!(
                "{} ({} %)",
                position.ordinal(),
                plain_number(percent)
            ));
            markup_ids.push(id);
        }
        if let Some(percent) = item.discount_percent.filter(|percent| !percent.is_zero()) {
            item_discounts.push(format!(
                "{} ({} %)",
                position.ordinal(),
                plain_number(percent)
            ));
        }
        any_price |= unit_price.is_some();
        let texts: Vec<BidComplement> = item
            .complements
            .iter()
            .filter(|complement| complement.kind != "Owner")
            .map(|complement| BidComplement {
                marker: complement.marker.trim().to_string(),
                text: complement.body.trim().to_string(),
            })
            .collect();
        if texts.iter().any(|text| !text.text.is_empty()) {
            filled_in += 1;
        }
        bid.set_complements(id, texts);
        bid.set_price(
            id,
            BidPrice {
                unit_price,
                stated_total: item.total,
                not_offered: item.not_offered,
            },
        );
    }

    let missing: Vec<String> = lv
        .positions()
        .filter(|position| !markup_ids.contains(&position.id()))
        .filter(|position| match bid.price(position.id()) {
            None => true,
            Some(price) => price.unit_price.is_none() && !price.not_offered,
        })
        .map(|position| position.ordinal().to_string())
        .collect();

    if !any_price {
        warnings.push("Die Datei enthält keine Einheitspreise.".to_string());
    }
    // Not in the file at all (a position without a price is no sign of
    // another LV version, it is listed on its own below).
    let absent = lv
        .positions()
        .filter(|position| !markup_ids.contains(&position.id()))
        .filter(|position| bid.price(position.id()).is_none())
        .count();
    if let Some(warning) = version_mismatch(
        lv.position_count(),
        unknown.len(),
        absent,
        report.quantities.len(),
    ) {
        warnings.insert(0, warning);
    }
    if let Some(warning) = repeated_ordinals(&items) {
        warnings.insert(0, warning);
    }
    let mut list = |entries: &[String], text: &str| {
        if !entries.is_empty() {
            warnings.push(format!(
                "{text} ({}): {}",
                entries.len(),
                short_list(entries, LIST_LIMIT)
            ));
        }
    };
    list(&missing, "Positionen ohne Preis im Angebot");
    list(&manual_failed, "Zuordnung von Hand nicht möglich");
    list(
        &twice,
        "Ordnungszahlen doppelt in der Datei (nur die erste zählt)",
    );
    list(&by_base, "Ohne Index zugeordnet");
    list(&arithmetic, "Rechenfehler GP ≠ Menge × EP");
    list(&not_offered, "Nicht angeboten");
    list(
        &derived,
        "Nur Gesamtpreis angegeben, EP = GP ÷ Menge übernommen",
    );
    list(
        &markups,
        "Prozentsatz an einer Position, die im LV keine Zuschlagsposition ist (nicht gewertet, bitte prüfen)",
    );
    list(
        &item_discounts,
        "Nachlass auf einzelne Positionen (nicht übernommen, bitte prüfen)",
    );

    if let Some(totals) = &doc.award.boq.totals {
        let net = bid.net_total(lv);
        let nonzero = |value: Option<Decimal>| value.filter(|value| !value.is_zero());
        let discount = if let Some(percent) = nonzero(totals.discount_percent) {
            Discount::Percent(percent)
        } else if let Some(amount) = nonzero(totals.discount_amount) {
            Discount::Amount(Money::from_decimal(amount))
        } else if let Some(lump_sum) = nonzero(totals.lump_sum) {
            let amount = net - Money::from_decimal(lump_sum);
            warnings.push(format!(
                "Pauschalsumme {} angeboten – als Nachlass {amount} auf die gerechnete Summe übernommen.",
                Money::from_decimal(lump_sum)
            ));
            Discount::Amount(amount)
        } else {
            match (totals.total, totals.total_after_discount) {
                (Some(total), Some(after)) if total != after => {
                    Discount::Amount(Money::from_decimal(total - after))
                }
                _ => Discount::None,
            }
        };
        bid.set_discount(discount);
        report.file_total = totals.total.map(Money::from_decimal);

        if let Some(total) = totals.total {
            let stated = Money::from_decimal(total);
            if (stated - net).cents().abs() > 1 {
                // GAEB 90 has no field for the amount of a Zuschlag; some
                // programs leave it out of their sums.
                let surcharges: Money = bid
                    .surcharge_values(lv)
                    .values()
                    .map(|value| value.amount)
                    .sum();
                let without = net - surcharges;
                let note = if !surcharges.is_zero() && (stated - without).cents().abs() <= 1 {
                    format!(" – die Datei zählt die Zuschläge ({surcharges}) nicht mit, proAVA wertet sie")
                } else {
                    String::new()
                };
                // Where the difference comes from (Abweichungsprotokoll).
                let mut check = bid.clone();
                check.set_structure(report.clone());
                let parts = check
                    .total_breakdown(lv)
                    .map(|breakdown| breakdown_text(&breakdown))
                    .unwrap_or_default();
                warnings.push(format!(
                    "Angebotssumme der Datei {stated} weicht ab von der gerechneten Summe {net} (Differenz {}){parts}{note}.",
                    stated - net
                ));
            }
        }
        if let Some(after) = totals.total_after_discount {
            let stated = Money::from_decimal(after);
            let computed = net - bid.discount_amount(net);
            if (stated - computed).cents().abs() > 1 {
                warnings.push(format!(
                    "Summe nach Nachlass der Datei {stated} weicht ab von der gerechneten {computed}."
                ));
            }
        }
    }
    if categories_with_discount(&doc.award.boq.body) {
        warnings.push(
            "Die Datei enthält Nachlässe auf Titel – sie werden nicht übernommen, nur der Nachlass auf das ganze Angebot."
                .to_string(),
        );
    }

    if filled_in > 0 {
        warnings.push(format!(
            "Bieterangaben (Fabrikat/Typ) zu {filled_in} Position(en) übernommen."
        ));
    }
    report.titles = title_lines(lv, &doc.award.boq.body);
    // The protocol at the front, right after an ACHTUNG.
    let absent_rows = bid.absent_positions(lv);
    if let Some(summary) = report.summary(absent_rows.len()) {
        let lines: Vec<String> = report
            .lines(lv, &absent_rows)
            .iter()
            .map(proava_domain::DeviationLine::text)
            .collect();
        let mut text = format!("{summary} – gewertet wird mit Menge und Text des LV:");
        for line in lines.iter().take(PROTOCOL_LIMIT) {
            text.push_str("\n- ");
            text.push_str(line);
        }
        if lines.len() > PROTOCOL_LIMIT {
            text.push_str(&format!(
                "\n- … und {} weitere (Abweichungsprotokoll am Angebot)",
                lines.len() - PROTOCOL_LIMIT
            ));
        }
        let at = warnings
            .iter()
            .take_while(|warning| warning.starts_with("ACHTUNG"))
            .count();
        warnings.insert(at, text);
    }
    if report.suggestions.iter().any(|suggestion| suggestion.sure) {
        let sure = report
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.sure)
            .count();
        warnings.push(format!(
            "{} nicht übernommen (Reiter Angebote › Positionen zuordnen …, MCP zuordnung: text).",
            proava_domain::wording::count(
                sure,
                "sicherer Zuordnungsvorschlag",
                "sichere Zuordnungsvorschläge"
            )
        ));
    }
    bid.set_structure(report);
    bid.set_unknown_ordinals(unknown);
    BidImport { bid, warnings }
}

/// Protocol lines in the import messages (the rest stays at the bid).
const PROTOCOL_LIMIT: usize = 40;

/// „: nicht zugeordnete Positionen 1.234,00 €, Rechenfehler …“
fn breakdown_text(breakdown: &proava_domain::TotalBreakdown) -> String {
    let mut parts = Vec::new();
    for (amount, label) in [
        (breakdown.foreign, "nicht zugeordnete Positionen"),
        (breakdown.arithmetic, "Rechenfehler"),
        (breakdown.quantities, "Mengenabweichungen"),
        (breakdown.rest, "Rest"),
    ] {
        if amount.cents().abs() > 1 {
            parts.push(format!("{label} {amount}"));
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(": davon {}", parts.join(", "))
    }
}

/// Titles of the file against the LV: only on one side, renamed, moved.
fn title_lines(lv: &proava_domain::Lv, body: &[BoqNode]) -> Vec<String> {
    fn collect(nodes: &[BoqNode], path: &mut Vec<String>, out: &mut Vec<(String, String, String)>) {
        for node in nodes {
            if let BoqNode::Category(category) = node {
                path.push(category.rno_part.trim().to_string());
                out.push((
                    match_key(path),
                    path.join("."),
                    category.label.trim().to_string(),
                ));
                collect(&category.body, path, out);
                path.pop();
            }
        }
    }
    let mut file = Vec::new();
    collect(body, &mut Vec::new(), &mut file);
    let ours: Vec<(String, String, String)> = lv
        .rows()
        .iter()
        .filter(|row| row.is_group())
        .map(|row| {
            let pieces: Vec<&str> = row.ordinal().split('.').map(str::trim).collect();
            (
                match_key(&pieces),
                row.ordinal().to_string(),
                row.short_text().trim().to_string(),
            )
        })
        .collect();
    let quoted = |label: &str| {
        if label.is_empty() {
            String::new()
        } else {
            format!(" „{label}“")
        }
    };
    let mut lines = Vec::new();
    for (key, shown, label) in &file {
        match ours.iter().find(|(other, _, _)| other == key) {
            None => lines.push(format!("Titel {shown}{} nur im Angebot", quoted(label))),
            Some((_, _, own)) => {
                if !label.is_empty()
                    && !own.is_empty()
                    && proava_domain::normalized(label) != proava_domain::normalized(own)
                {
                    lines.push(format!(
                        "Titel {shown} heißt im Angebot „{label}“ statt „{own}“"
                    ));
                }
            }
        }
    }
    for (key, shown, label) in &ours {
        if !file.iter().any(|(other, _, _)| other == key) {
            lines.push(format!("Titel {shown}{} fehlt im Angebot", quoted(label)));
        }
    }
    // Order: the common titles in file order must come in LV order.
    let places: Vec<(usize, &str)> = file
        .iter()
        .filter_map(|(key, shown, _)| {
            ours.iter()
                .position(|(other, _, _)| other == key)
                .map(|place| (place, shown.as_str()))
        })
        .collect();
    if let Some(pair) = places.windows(2).find(|pair| pair[1].0 < pair[0].0) {
        lines.push(format!(
            "Titelreihenfolge im Angebot umgestellt ({} steht vor {})",
            pair[0].1, pair[1].1
        ));
    }
    lines
}

fn categories_with_discount(nodes: &[BoqNode]) -> bool {
    nodes.iter().any(|node| match node {
        BoqNode::Category(category) => {
            category.totals.as_ref().is_some_and(|totals| {
                totals.discount_percent.is_some_and(|percent| !percent.is_zero())
                    || matches!((totals.total, totals.total_after_discount), (Some(a), Some(b)) if a != b)
            }) || categories_with_discount(&category.body)
        }
        _ => false,
    })
}

fn plain_number(value: Decimal) -> String {
    format_decimal_de(value, value.normalize().scale())
}

/// Many positions of the file under the same order number: the program of
/// the bidder cut the position numbers (GAEB 90 holds at most 9 digits, a
/// mask like 11.22.33.PPPP needs 10) – the prices cannot be assigned safely.
fn repeated_ordinals(items: &[(Vec<String>, &proava_gaeb::Item)]) -> Option<String> {
    let mut count: HashMap<String, (String, usize)> = HashMap::new();
    for (pieces, item) in items {
        let key = format!("{}|{}", match_key(pieces), item.rno_index.trim());
        count.entry(key).or_insert_with(|| (pieces.join("."), 0)).1 += 1;
    }
    let mut repeated: Vec<(String, usize)> = count.into_values().filter(|(_, n)| *n > 1).collect();
    let positions: usize = repeated.iter().map(|(_, n)| n).sum();
    // A single doubled position is a slip, listed below; many are a cut mask.
    if positions < 10 || positions * 20 < items.len() {
        return None;
    }
    repeated.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let examples: Vec<String> = repeated
        .iter()
        .take(3)
        .map(|(key, n)| format!("{key} {n}×"))
        .collect();
    Some(format!(
        "ACHTUNG: In der Datei tragen {positions} von {} Positionen dieselbe Ordnungszahl wie andere \
         ({} Ordnungszahlen mehrfach, z. B. {}) – die Positionsnummern sind vermutlich gekürzt \
         (GAEB 90 fasst höchstens 9 Stellen). Die Preise lassen sich so nicht sicher zuordnen; \
         bitte das Angebot als GAEB DA XML (X84) anfordern.",
        items.len(),
        repeated.len(),
        examples.join(", ")
    ))
}

/// Many unknown/missing order numbers or changed quantities usually mean the
/// bidder priced another version of the LV – say so before the details.
fn version_mismatch(
    positions: usize,
    unknown: usize,
    missing: usize,
    quantity: usize,
) -> Option<String> {
    if positions == 0 {
        return None;
    }
    let share = |count: usize| count * 100 / positions;
    let structural = unknown.max(missing);
    // A share and at least three cases: one gap in a small LV is no other version.
    let structure = structural >= 3 && share(structural) >= 5;
    let quantities = quantity >= 3 && share(quantity) >= 10;
    (structure || quantities).then(|| {
        format!(
            "ACHTUNG: Das Angebot passt vermutlich zu einer anderen LV-Fassung \
             ({unknown} unbekannte, {missing} fehlende Positionen, {quantity} abweichende Mengen \
             bei {positions} Positionen im LV). Bitte prüfen, ob das richtige LV geöffnet ist."
        )
    })
}

#[cfg(test)]
mod version_tests {
    use super::version_mismatch;

    #[test]
    fn flags_bids_for_another_lv_version() {
        assert!(version_mismatch(534, 69, 71, 117).is_some());
        assert!(version_mismatch(89, 1, 0, 1).is_none());
        assert!(version_mismatch(100, 0, 0, 12).is_some());
        assert!(version_mismatch(0, 0, 0, 0).is_none());
        assert!(
            version_mismatch(19, 0, 1, 0).is_none(),
            "eine Lücke in kleinem LV"
        );
        assert!(version_mismatch(19, 0, 3, 0).is_some());
    }
}

/// Names some programs write when they have none: "k.A.", "-", "n/a" …
pub(crate) fn is_placeholder_name(name: &str) -> bool {
    let compact: String = name
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    matches!(
        compact.as_str(),
        "" | "ka" | "na" | "nn" | "keineangabe" | "unbekannt" | "bieter" | "firma"
    )
}

/// A bidder name from the folder the bid lies in, e.g.
/// "Angebot E2 Muster Gebäudeautomation" → "Muster Gebäudeautomation",
/// "01- AN Beispiel Solutions" → "Beispiel Solutions".
pub(crate) fn name_from_folder(folder: &str) -> Option<String> {
    let mut rest = folder.trim();
    let digits = rest.trim_start_matches(|character: char| character.is_ascii_digit());
    if digits.len() < rest.len() {
        rest = digits.trim_start_matches(['-', '_', '.', ' ']);
    }
    for prefix in ["Angebot ", "Angebote ", "AN ", "Bieter "] {
        if rest.len() > prefix.len() && rest[..prefix.len()].eq_ignore_ascii_case(prefix) {
            rest = rest[prefix.len()..].trim_start();
        }
    }
    let mut words = rest.split_whitespace().peekable();
    if let Some(first) = words.peek() {
        let code = first.len() <= 3
            && first.starts_with(|character: char| character.is_ascii_uppercase())
            && first[1..]
                .chars()
                .all(|character| character.is_ascii_digit())
            && first.len() > 1;
        if code {
            words.next();
        }
    }
    let name = words.collect::<Vec<_>>().join(" ");
    let generic = [
        "angebote",
        "angebot",
        "rücklauf",
        "ruecklauf",
        "gaeb",
        "x84",
        "d84",
    ];
    (name
        .chars()
        .filter(|character| character.is_alphabetic())
        .count()
        >= 3
        && !generic.contains(&name.to_lowercase().as_str()))
    .then_some(name)
}

#[cfg(test)]
mod name_tests {
    use super::{is_placeholder_name, name_from_folder};

    #[test]
    fn placeholder_names_and_folder_names() {
        assert!(is_placeholder_name("k.A."));
        assert!(is_placeholder_name(" - "));
        assert!(!is_placeholder_name("Siemens AG"));
        assert_eq!(
            name_from_folder("Angebot E2 Muster Gebäudeautomation").as_deref(),
            Some("Muster Gebäudeautomation")
        );
        assert_eq!(
            name_from_folder("01- AN Beispiel Solutions").as_deref(),
            Some("Beispiel Solutions")
        );
        assert_eq!(name_from_folder("6 Angebote"), None);
        assert_eq!(name_from_folder("GAEB"), None);
    }
}
