//! The part of a document that survives writing it in a given phase.

use std::collections::HashSet;

use super::calc;
use super::image;
use super::rich_text::complement_marker;
use super::standard;
use super::writer::{images_allowed, up_component_kind, WriteOptions};
use crate::model::*;

fn cut(text: &mut String, max: usize) {
    let cleaned: String = text
        .replace(['\n', '\r', '\t'], " ")
        .chars()
        .take(max)
        .collect();
    *text = cleaned;
}

fn number_in(text: &str, max: u32) -> String {
    text.trim().parse::<u32>().unwrap_or(0).min(max).to_string()
}

fn totals_as_written(totals: &mut Totals) {
    match totals.total_after_discount {
        Some(_) if totals.discount_percent.is_some() => totals.discount_amount = None,
        Some(_) if totals.discount_amount.is_some() => {}
        _ => {
            totals.total_after_discount = None;
            totals.discount_percent = None;
            totals.discount_amount = None;
        }
    }
    if totals.total_after_discount.is_some() {
        totals.lump_sum = None;
    }
}

/// What reading the file written by [`super::write`] in `phase` gives back:
/// the document without everything GAEB DA XML 3.3 cannot carry in that
/// phase (prices outside 82/84–86, NotAppl in X83, texts and kinds in X84,
/// labels of non-title OZ levels …), with missing totals filled in and the
/// values normalised as the writer does. For documents that came from the
/// reader, `read(write(doc, phase)) == as_written(doc, phase)`.
pub fn as_written(doc: &GaebDocument, phase: Phase) -> GaebDocument {
    as_written_with(doc, phase, &WriteOptions::default())
}

/// [`as_written`] for [`super::write_with`] with these options.
pub fn as_written_with(doc: &GaebDocument, phase: Phase, options: &WriteOptions) -> GaebDocument {
    let mut doc = doc.clone();
    let images = images_allowed(phase, options);
    for text in doc
        .award
        .add_texts
        .iter_mut()
        .chain(doc.award.closing_texts.iter_mut())
        .map(|text| &mut text.detail)
    {
        images_as_written(text, images);
    }
    texts_as_written(&mut doc.award.boq.body, images);
    catalogs_as_written(&mut doc.award.boq, phase);
    doc.phase = phase;
    doc.program = "proAVA".into();
    if !doc.date.as_ref().is_some_and(|date| date.len() == 10) {
        doc.date = Some(super::writer::today());
    }
    let bid = phase == Phase::Dp84;
    let tender = phase == Phase::Dp83;
    let prices = phase.has_prices();

    let project = &mut doc.project;
    cut(&mut project.name, 60);
    cut(&mut project.label, 100);
    cut(&mut project.currency, 4);
    if project.currency.is_empty() {
        project.currency_label.clear();
    }
    cut(&mut project.currency_label, 60);
    if bid
        || !project
            .up_fraction_digits
            .is_some_and(|digits| (2..=3).contains(&digits))
    {
        project.up_fraction_digits = None;
    }

    let award = &mut doc.award;
    let place_addresses = award
        .requesters
        .iter_mut()
        .chain(
            award
                .sites
                .iter_mut()
                .flat_map(|site| std::iter::once(&mut site.site).chain(site.notification.as_mut())),
        )
        .filter_map(|place| {
            cut(&mut place.id_no, 20);
            cut(&mut place.name, 60);
            place.address.as_mut()
        })
        .collect::<Vec<_>>();
    for address in [&mut award.owner, &mut award.bidder]
        .into_iter()
        .flatten()
        .chain(place_addresses)
    {
        cut(&mut address.name1, 40);
        cut(&mut address.name2, 40);
        cut(&mut address.name3, 40);
        cut(&mut address.name4, 40);
        cut(&mut address.street, 40);
        cut(&mut address.postal_code, 20);
        cut(&mut address.city, 40);
        cut(&mut address.country, 40);
        cut(&mut address.iln, 20);
        cut(&mut address.contact, 40);
        cut(&mut address.phone, 20);
        cut(&mut address.fax, 20);
        cut(&mut address.email, 256);
        cut(&mut address.vat_id, 80);
    }
    cut(&mut award.submission.location, 60);
    cut(&mut award.award_no, 60);
    cut(&mut award.owner_dp_no, 60);
    cut(&mut award.bidder_dp_no, 60);
    cut(&mut award.owner_account, 20);
    let country = &mut award.owner_country;
    country.kind = country.kind.trim().to_string();
    if country.kind == "EEA" || country.kind == "Other" {
        cut(&mut country.name, 3);
    } else {
        *country = Country::default();
    }
    if !matches!(
        award.category.as_str(),
        "OpenProc"
            | "ClosedProc"
            | "NegProc"
            | "OpenCall"
            | "SelectCall"
            | "NegCont"
            | "IntNATO"
            | "SelectCallPostOpen"
            | "NegProcOpen"
            | "CompetDialog"
            | "InnovationPartnership"
    ) {
        award.category.clear();
    }
    if award.submission.open_date.is_none() {
        award.submission.open_time = None;
    }
    if let Some(time) = &mut award.submission.open_time {
        if time.len() == 5 && time.as_bytes()[2] == b':' {
            time.push_str(":00");
        }
    }
    // Vertragliche Regelungen have no element in DA XML.
    award.contract_text.clear();
    if tender {
        award.bidder = None;
        award.bid_date = None;
    }
    if phase == Phase::Dp85 {
        award.requesters.clear();
        award.sites.clear();
    }
    // CTR/DPNo only where CTR is written.
    if award.bidder.is_none() && !matches!(phase, Phase::Dp84 | Phase::Dp85 | Phase::Dp86) {
        award.bidder_dp_no.clear();
    }
    if phase == Phase::Dp86 {
        award.category.clear();
        let submission = &mut award.submission;
        submission.location.clear();
        submission.open_date = None;
        submission.open_time = None;
        submission.award_deadline = None;
        award.owner.get_or_insert_with(Address::default);
        award.bidder.get_or_insert_with(Address::default);
    }
    if bid {
        award.owner = None;
        award.add_texts.clear();
        award.closing_texts.clear();
        award.category.clear();
        award.award_no.clear();
        award.owner_dp_no.clear();
        award.owner_account.clear();
        award.owner_country = Country::default();
        award.requesters.clear();
        award.sites.clear();
        award.submission = Submission::default();
        award.bidder.get_or_insert_with(Address::default);
    }

    let boq = &mut award.boq;
    cut(&mut boq.name, 20);
    cut(&mut boq.label, 60);
    for level in &mut boq.breakdown {
        if level.kind == BreakdownKind::Level {
            cut(&mut level.label, 40);
        } else {
            level.label.clear();
        }
        level.length = if level.kind == BreakdownKind::Index {
            1
        } else {
            level.length.clamp(1, 14)
        };
    }
    change_order_as_written(&mut boq.change_order, phase);
    if bid {
        boq.label.clear();
        boq.date = None;
        boq.up_component_labels.clear();
        boq.up_component_kinds.clear();
        boq.time_label.clear();
    } else if !boq.up_component_labels.is_empty() {
        let count = boq.up_component_labels.len().clamp(2, 6);
        boq.up_component_labels.resize(count, "Sonstiges".into());
        boq.up_component_labels.truncate(count);
        for label in &mut boq.up_component_labels {
            cut(label, 20);
        }
        boq.up_component_kinds.resize(count, String::new());
        boq.up_component_kinds.truncate(count);
        for (index, kind) in boq.up_component_kinds.iter_mut().enumerate() {
            *kind = up_component_kind(&boq.up_component_labels[index], kind, index).to_string();
        }
    }
    cut(&mut boq.time_label, 20);
    if prices {
        calc::fill_totals(boq);
        if let Some(totals) = &mut boq.totals {
            totals_as_written(totals);
        }
    } else {
        boq.totals = None;
    }

    let mut ordinals = HashSet::new();
    let mut perf_numbers = HashSet::new();
    collect(&boq.body, &mut Vec::new(), &mut ordinals, &mut perf_numbers);
    let context = Context {
        phase,
        ordinals,
        perf_numbers,
    };
    strip(&mut boq.body, &context);
    doc
}

fn collect(
    nodes: &[BoqNode],
    path: &mut Vec<String>,
    ordinals: &mut HashSet<String>,
    perfs: &mut HashSet<String>,
) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                path.push(category.rno_part.clone());
                collect(&category.body, path, ordinals, perfs);
                path.pop();
            }
            BoqNode::Item(item) => {
                ordinals.insert(calc::ordinal(path, item));
            }
            BoqNode::PerfDescr(description) => {
                perfs.insert(description.number.clone());
            }
            BoqNode::Remark(_) => {}
        }
    }
}

struct Context {
    phase: Phase,
    ordinals: HashSet<String>,
    perf_numbers: HashSet<String>,
}

fn strip(nodes: &mut Vec<BoqNode>, context: &Context) {
    let phase = context.phase;
    let bid = phase == Phase::Dp84;
    if bid {
        nodes.retain(|node| matches!(node, BoqNode::Category(_) | BoqNode::Item(_)));
    }
    for node in nodes.iter_mut() {
        match node {
            BoqNode::Category(category) => {
                if phase.has_prices() {
                    if let Some(totals) = &mut category.totals {
                        totals_as_written(totals);
                    }
                } else {
                    category.totals = None;
                }
                if bid {
                    category.label.clear();
                    category.alternative = None;
                }
                if let Some(alternative) = &mut category.alternative {
                    alternative.group = number_in(&alternative.group, 999);
                    alternative.serial = number_in(&alternative.serial, 9);
                }
                strip(&mut category.body, context);
            }
            BoqNode::Item(item) => strip_item(item, context),
            BoqNode::PerfDescr(description) => {
                description.number = number_in(&description.number, 9999);
                cut(&mut description.label, 55);
                markers_as_written(&mut description.complements);
                standard::as_written(&mut description.stlb, &mut description.standard_number);
            }
            BoqNode::Remark(remark) => {
                markers_as_written(&mut remark.complements);
                standard::as_written(&mut remark.stlb, &mut remark.standard_number);
            }
        }
    }
}

/// Marker and kind of Textergänzungen in a Hinweistext or an
/// Ausführungsbeschreibung as the writer puts them.
fn markers_as_written(complements: &mut [TextComplement]) {
    for complement in complements {
        complement.marker = complement_marker(&complement.marker);
        if complement.kind != "Owner" {
            complement.kind = "Bidder".into();
        }
    }
}

/// CONo/COStatus: not in phase 84 and not at Zuschlagspositionen; number at
/// most 999, an empty status is written as „Filed“.
fn change_order_as_written(tag: &mut Option<ChangeOrderTag>, phase: Phase) {
    if phase == Phase::Dp84 {
        *tag = None;
    }
    if let Some(tag) = tag {
        tag.number = tag.number.min(999);
        tag.status = tag.status.trim().to_string();
        if tag.status.is_empty() {
            tag.status = "Filed".into();
        }
    }
}

fn strip_item(item: &mut Item, context: &Context) {
    let phase = context.phase;
    let surcharge = calc::is_surcharge(item);
    change_order_as_written(&mut item.change_order, phase);
    if surcharge {
        item.change_order = None;
    }
    for complement in &mut item.complements {
        complement.marker = complement_marker(&complement.marker);
        if complement.kind != "Owner" {
            complement.kind = "Bidder".into();
        }
    }
    if phase == Phase::Dp84 {
        let complements: Vec<TextComplement> = item
            .complements
            .iter()
            .filter(|complement| complement.kind == "Bidder" && !surcharge)
            .map(|complement| TextComplement {
                caption: String::new(),
                tail: String::new(),
                // A bid carries no long text: the complement stands alone.
                anchor: Some(crate::model::TextAnchor {
                    block: 0,
                    offset: 0,
                    own_line: true,
                }),
                ..complement.clone()
            })
            .collect();
        *item = Item {
            rno_part: item.rno_part.clone(),
            rno_index: item.rno_index.clone(),
            qty: item.qty.filter(|_| item.qty_to_be_determined && !surcharge),
            // The reader marks a quantity in an X84 as the free quantity.
            qty_to_be_determined: item.qty_to_be_determined && item.qty.is_some() && !surcharge,
            unit_price: item.unit_price.filter(|_| !surcharge),
            up_components: if item.unit_price.is_some() && !surcharge {
                item.up_components.iter().take(6).cloned().collect()
            } else {
                Vec::new()
            },
            discount_percent: item
                .discount_percent
                .filter(|_| surcharge || item.unit_price.is_some()),
            total: item.total,
            time_quantity: item.time_quantity.filter(|_| !surcharge),
            not_offered: item.not_offered && !surcharge,
            complements,
            markup_percent: item.markup_percent,
            markup_base: item.markup_base,
            surcharge: surcharge.then(Surcharge::default),
            kind: ItemKind {
                markup: surcharge,
                ..ItemKind::default()
            },
            ..Item::default()
        };
        trim_components(item);
        return;
    }
    standard::as_written(&mut item.stlb, &mut item.standard_number);

    if !phase.has_prices() {
        item.unit_price = None;
        item.up_components.clear();
        item.discount_percent = None;
        item.total = None;
        item.time_quantity = None;
        item.markup_base = None;
        item.markup_percent = None;
    } else if item.unit_price.is_none() && !surcharge {
        item.up_components.clear();
        item.discount_percent = None;
    }
    item.up_components.truncate(6);
    trim_components(item);
    if let Some(alternative) = &mut item.kind.alternative {
        alternative.group = number_in(&alternative.group, 999);
        alternative.serial = number_in(&alternative.serial, 9);
    }
    if item
        .kind
        .repetition_of
        .as_ref()
        .is_some_and(|ordinal| !context.ordinals.contains(ordinal))
    {
        item.kind.repetition_of = None;
    }
    if item.kind.repetition_of.is_some()
        || item
            .kind
            .reference_description
            .as_ref()
            .is_some_and(|number| !context.perf_numbers.contains(number))
    {
        item.kind.reference_description = None;
    }
    if surcharge {
        // A Zuschlagsposition has no quantity, unit or price of its own.
        item.qty = None;
        item.qty_to_be_determined = false;
        item.unit.clear();
        item.unit_price = None;
        item.up_components.clear();
        item.kind.lump_sum = false;
        item.kind.markup = true;
        item.not_applicable = false;
        item.up_breakdown = false;
        item.time_quantity = None;
        item.sub_descriptions.clear();
        if phase == Phase::Dp86 {
            item.markup_base.get_or_insert(rust_decimal::Decimal::ZERO);
            item.markup_percent
                .get_or_insert(rust_decimal::Decimal::ZERO);
        }
        let entry = item.surcharge.get_or_insert_with(Surcharge::default);
        if !matches!(
            entry.markup_type.as_str(),
            "IdentAsMark" | "AllInCat" | "ListInSubQty"
        ) {
            entry.markup_type = "IdentAsMark".into();
        }
        if entry.markup_type == "ListInSubQty" {
            entry
                .items
                .retain(|reference| context.ordinals.contains(&reference.ordinal));
        } else {
            entry.items.clear();
        }
        return;
    }
    cut(&mut item.unit, 4);
    if phase == Phase::Dp83 {
        item.not_applicable = false;
        if item.qty_to_be_determined {
            item.qty = None;
        }
    }
    item.not_offered = false;
    for (index, sub) in item.sub_descriptions.iter_mut().enumerate() {
        sub.number = match sub.number.trim() {
            number if number.parse::<u32>().is_ok_and(|n| (1..=99).contains(&n)) => {
                number.to_string()
            }
            _ => (index + 1).to_string(),
        };
        if sub.qty.is_none() {
            sub.unit.clear();
        }
        cut(&mut sub.unit, 4);
        standard::as_written(&mut sub.stlb, &mut sub.standard_number);
    }
}

fn trim_components(item: &mut Item) {
    while item.up_components.last().is_some_and(Option::is_none) {
        item.up_components.pop();
    }
}

/// Images as the writer leaves them, then what the reader does with the
/// text (a hard-wrapped text without its image is joined into paragraphs).
fn images_as_written(text: &mut RichText, keep: bool) {
    text.blocks = std::mem::take(&mut text.blocks)
        .into_iter()
        .filter_map(|block| match block {
            TextBlock::Image { .. } if !keep => None,
            TextBlock::Image { .. } => image::as_written(&block),
            other => Some(other),
        })
        .collect();
    super::hard_wrap::unwrap(text);
}

/// Long text and complements as the reader gets them back: inside the
/// text at their anchors (see `complement_place`), otherwise after it.
fn complements_as_written(item: &mut Item, keep: bool) {
    text_with_complements_as_written(&mut item.long_text, &mut item.complements, keep);
}

/// [`complements_as_written`] for any text with complements (positions,
/// Hinweistexte, Ausführungsbeschreibungen).
fn text_with_complements_as_written(
    long_text: &mut RichText,
    complements: &mut Vec<TextComplement>,
    keep: bool,
) {
    use super::complement_place;
    use super::rich_text::{take_anchors, ANCHOR_MARK};
    match complement_place::segments(long_text, complements) {
        Some(segments) => {
            let mut text = complement_place::as_read(long_text, &segments, keep, ANCHOR_MARK);
            images_as_written(&mut text, keep);
            let anchors = take_anchors(&mut text);
            let order = complement_place::order(&segments);
            let placed: Vec<TextComplement> = order
                .iter()
                .zip(anchors)
                .map(|(&index, anchor)| TextComplement {
                    anchor: Some(anchor),
                    ..complements[index].clone()
                })
                .collect();
            *long_text = text;
            *complements = placed;
        }
        None => {
            images_as_written(long_text, keep);
            let end = long_text.blocks.len();
            for complement in complements.iter_mut() {
                complement.anchor = Some(crate::model::TextAnchor {
                    block: end,
                    offset: 0,
                    own_line: true,
                });
            }
        }
    }
}

/// Catalogs and assignments as written: none in a bid (X84), otherwise
/// only catalogs with id and type and assignments to them.
fn catalogs_as_written(boq: &mut Boq, phase: Phase) {
    fn walk(nodes: &mut [BoqNode], keep: &dyn Fn(&CatalogAssign) -> bool) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    category.catalog_assigns.retain(|assign| keep(assign));
                    walk(&mut category.body, keep);
                }
                BoqNode::Item(item) => item.catalog_assigns.retain(|assign| keep(assign)),
                _ => {}
            }
        }
    }
    if phase == Phase::Dp84 {
        boq.catalogs.clear();
    }
    boq.catalogs
        .retain(|catalog| !catalog.id.trim().is_empty() && !catalog.kind.trim().is_empty());
    let ids: HashSet<String> = boq
        .catalogs
        .iter()
        .map(|catalog| catalog.id.clone())
        .collect();
    walk(&mut boq.body, &|assign: &CatalogAssign| {
        ids.contains(&assign.catalog) && !assign.code.trim().is_empty()
    });
}

fn texts_as_written(nodes: &mut [BoqNode], keep: bool) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => texts_as_written(&mut category.body, keep),
            BoqNode::Item(item) => {
                complements_as_written(item, keep);
                for sub in &mut item.sub_descriptions {
                    images_as_written(&mut sub.long_text, keep);
                }
            }
            BoqNode::Remark(remark) => {
                text_with_complements_as_written(&mut remark.text, &mut remark.complements, keep)
            }
            BoqNode::PerfDescr(description) => text_with_complements_as_written(
                &mut description.long_text,
                &mut description.complements,
                keep,
            ),
        }
    }
}
