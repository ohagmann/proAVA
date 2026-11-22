//! GAEB DA XML 3.x (3.0 to 3.3) reader for the phases 81 to 86.

use std::collections::HashMap;
use std::str::FromStr;

use rust_decimal::Decimal;

use super::dom::{self, Element};
use super::hard_wrap;
use super::rich_text::{read_plain, read_rich, take_anchors};
use super::standard;
use super::{calc, GaebXmlError, XmlReadReport};
use crate::model::*;

struct Reader {
    warnings: Vec<String>,
    /// XML ID of an item → its ordinal.
    item_ids: HashMap<String, String>,
    /// XML ID of a PerfDescr → its number.
    perf_ids: HashMap<String, String>,
    /// Lengths of the title levels (lot first) and of the item part.
    level_lengths: Vec<usize>,
    item_length: usize,
    dropped_images: usize,
}

pub fn read(bytes: &[u8]) -> Result<(GaebDocument, XmlReadReport), GaebXmlError> {
    let text = dom::decode(bytes);
    let root = dom::parse(&text)?;
    if root.name != "GAEB" {
        return Err(GaebXmlError::NotGaeb(format!(
            "das Wurzelelement heißt <{}> statt <GAEB>",
            root.name
        )));
    }
    let namespace = root.attr("xmlns").unwrap_or_default().to_string();
    let info = root.child("GAEBInfo");
    let version = info
        .and_then(|info| info.child_text("Version"))
        .unwrap_or_default();
    let award = root.child("Award").ok_or_else(|| {
        GaebXmlError::NotGaeb(
            "die Datei enthält keine Vergabe (Award) – sie ist kein Leistungsverzeichnis (z. B. eine \
             Mengenermittlung X31)"
                .into(),
        )
    })?;
    let phase_number = award
        .child_text("DP")
        .or_else(|| phase_from_namespace(&namespace))
        .ok_or_else(|| GaebXmlError::NotGaeb("die Austauschphase (DP) fehlt".into()))?;
    let phase = phase_number
        .trim()
        .parse::<u8>()
        .ok()
        .and_then(Phase::from_number)
        .ok_or_else(|| GaebXmlError::UnsupportedPhase(phase_number.trim().to_string()))?;

    let mut reader = Reader {
        warnings: Vec::new(),
        item_ids: HashMap::new(),
        perf_ids: HashMap::new(),
        level_lengths: Vec::new(),
        item_length: 0,
        dropped_images: 0,
    };

    let prj = root.child("PrjInfo");
    let award_info = award.child("AwardInfo");
    let text_of = |element: Option<&Element>, name: &str| {
        element.and_then(|element| element.child_text(name))
    };
    let project = ProjectInfo {
        name: text_of(prj, "NamePrj").unwrap_or_default(),
        label: text_of(prj, "LblPrj").unwrap_or_default(),
        currency: text_of(prj, "Cur")
            .or_else(|| text_of(award_info, "Cur"))
            .unwrap_or_default(),
        currency_label: text_of(prj, "CurLbl")
            .or_else(|| text_of(award_info, "CurLbl"))
            .unwrap_or_default(),
        up_fraction_digits: text_of(prj, "UPFracDig").and_then(|digits| digits.parse().ok()),
    };

    let mut add_texts = Vec::new();
    // AddText at the root stands after the Award: a closing text
    // (Schlussbemerkung, e.g. a „Schlussblatt“ for the bidder) – it stays at the end.
    let mut closing_texts = Vec::new();
    for (owner, closing) in [(Some(award), false), (prj, false), (Some(&root), true)] {
        let Some(owner) = owner else {
            continue;
        };
        for element in owner.children_named("AddText") {
            let texts = if closing {
                &mut closing_texts
            } else {
                &mut add_texts
            };
            texts.push(AddText {
                outline: element
                    .child("OutlineAddText")
                    .map(read_plain)
                    .unwrap_or_default(),
                detail: element
                    .child("DetailAddText")
                    .map(|detail| {
                        let mut read = read_rich(detail);
                        reader.dropped_images += read.dropped_images;
                        hard_wrap::unwrap(&mut read.text);
                        take_anchors(&mut read.text);
                        read.text
                    })
                    .unwrap_or_default(),
            });
        }
    }

    let mut boq = match award.child("BoQ") {
        Some(boq) => reader.boq(boq),
        None => {
            reader
                .warnings
                .push("Die Datei enthält kein Leistungsverzeichnis (BoQ).".into());
            Boq::default()
        }
    };
    reader.apply(&mut boq.body);
    if phase == Phase::Dp84 {
        mark_free_quantities(&mut boq.body);
    }
    if reader.dropped_images > 0 {
        reader.warnings.push(format!(
            "{} {} im Langtext ohne lesbaren Inhalt (Base64) übergangen.",
            reader.dropped_images,
            if reader.dropped_images == 1 {
                "Bild"
            } else {
                "Bilder"
            }
        ));
    }

    let award_no = text_of(award.child("OWN"), "AwardNo")
        .or_else(|| text_of(award.child("CTR"), "AwardNo"))
        .unwrap_or_default();
    let owner_dp_no = text_of(award.child("OWN"), "DPNo").unwrap_or_default();
    // Requester and CnstSite/NotifSite (a NotifSite belongs to the CnstSite before it).
    let place = |element: &Element, prefix: &str| Place {
        address: element.child("Address").map(address),
        id_no: element
            .child_text(&format!("{prefix}IDNo"))
            .unwrap_or_default(),
        name: element
            .child_text(&format!("{prefix}Name"))
            .unwrap_or_default(),
    };
    let mut requesters = Vec::new();
    let mut sites: Vec<Site> = Vec::new();
    for element in award.elements() {
        match element.name.as_str() {
            "Requester" => requesters.push(place(element, "Request")),
            "CnstSite" => sites.push(Site {
                site: place(element, "CnstSite"),
                notification: None,
            }),
            "NotifSite" => match sites.last_mut() {
                Some(site) if site.notification.is_none() => {
                    site.notification = Some(place(element, "NotifSite"))
                }
                _ => reader.warnings.push(
                    "Benachrichtigungsort (NotifSite) ohne Ausführungsort übergangen.".into(),
                ),
            },
            _ => {}
        }
    }
    let country = |party: Option<&Element>| Country {
        kind: text_of(party, "CntryType").unwrap_or_default(),
        name: text_of(party, "CntryName").unwrap_or_default(),
    };
    let document = GaebDocument {
        phase,
        program: info
            .and_then(|info| {
                info.child_text("ProgName")
                    .or_else(|| info.child_text("ProgSystem"))
            })
            .unwrap_or_default(),
        date: info.and_then(|info| info.child_text("Date")),
        project,
        award: Award {
            owner: award.path(&["OWN", "Address"]).map(address),
            bidder: award.path(&["CTR", "Address"]).map(address),
            bid_date: text_of(award_info, "BidDate"),
            submission: Submission {
                location: text_of(award_info, "SubmLoc").unwrap_or_default(),
                open_date: text_of(award_info, "OpenDate"),
                open_time: text_of(award_info, "OpenTime"),
                construction_start: text_of(award_info, "CnstStart"),
                construction_end: text_of(award_info, "CnstEnd"),
                award_deadline: text_of(award_info, "EvalEnd"),
            },
            boq,
            award_no,
            owner_dp_no,
            owner_account: text_of(award.child("OWN"), "AcctRecNo").unwrap_or_default(),
            owner_country: country(award.child("OWN")),
            bidder_dp_no: text_of(award.child("CTR"), "DPNo").unwrap_or_default(),
            requesters,
            sites,
            contract_text: Vec::new(),
            category: text_of(award_info, "Cat").unwrap_or_default(),
            add_texts,
            closing_texts,
        },
    };
    Ok((
        document,
        XmlReadReport {
            version,
            namespace,
            warnings: reader.warnings,
        },
    ))
}

/// In a bid (X84) an item carries a quantity only as the bidder's free
/// quantity (Regeln für DA84: „Qty nur für die Angabe einer Freien Menge“),
/// even though X84 has no QtyTBD: mark it, so that writing keeps it.
fn mark_free_quantities(nodes: &mut [BoqNode]) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => mark_free_quantities(&mut category.body),
            BoqNode::Item(item) if item.qty.is_some() && !calc::is_surcharge(item) => {
                item.qty_to_be_determined = true;
            }
            _ => {}
        }
    }
}

fn phase_from_namespace(namespace: &str) -> Option<String> {
    let start = namespace.find("/DA")? + 3;
    let digits: String = namespace[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (digits.len() == 2).then_some(digits)
}

pub(super) fn address(element: &Element) -> Address {
    let text = |name: &str| element.child_text(name).unwrap_or_default();
    Address {
        name1: text("Name1"),
        name2: text("Name2"),
        name3: text("Name3"),
        name4: text("Name4"),
        street: text("Street"),
        postal_code: text("PCode"),
        city: text("City"),
        phone: text("Phone"),
        email: text("Email"),
        country: text("Country"),
        iln: text("ILN"),
        contact: text("Contact"),
        fax: text("Fax"),
        vat_id: text("VATID"),
    }
}

fn yes(element: &Element, name: &str) -> bool {
    element
        .child_text(name)
        .is_some_and(|value| value.eq_ignore_ascii_case("yes"))
}

/// Numeric ordinal parts are padded with zeros to the mask length
/// (`RNoPart="1"` → "01"), as GAEB 90 always writes them.
fn pad(part: &str, length: usize) -> String {
    let part = part.trim();
    if !part.is_empty() && part.len() < length && part.chars().all(|ch| ch.is_ascii_digit()) {
        format!("{part:0>length$}")
    } else {
        part.to_string()
    }
}

/// Kurztext, Langtext, text complements and unreadable images of a
/// Description element.
pub(super) fn description(
    element: Option<&Element>,
) -> (String, RichText, Vec<TextComplement>, usize) {
    let Some(element) = element else {
        return (String::new(), RichText::default(), Vec::new(), 0);
    };
    let mut short = String::new();
    let mut long = RichText::default();
    let mut complements = Vec::new();
    let mut dropped = 0;
    if let Some(complete) = element.child("CompleteText") {
        if let Some(detail) = complete.child("DetailTxt") {
            let read = read_rich(detail);
            (long, complements, dropped) = (read.text, read.complements, read.dropped_images);
            hard_wrap::unwrap(&mut long);
            let anchors = take_anchors(&mut long);
            if anchors.len() == complements.len() {
                for (complement, anchor) in complements.iter_mut().zip(anchors) {
                    complement.anchor = Some(anchor);
                }
            }
        }
        if let Some(outline) = complete.path(&["OutlineText", "OutlTxt"]) {
            short = read_plain(outline);
        }
    }
    if short.is_empty() {
        if let Some(outline) = element.path(&["OutlineText", "OutlTxt"]) {
            short = read_plain(outline);
        }
    }
    (short, long, complements, dropped)
}

/// CtlgAssign children (catalog id and code, both required).
fn catalog_assigns(element: &Element) -> Vec<CatalogAssign> {
    element
        .children_named("CtlgAssign")
        .map(|assign| CatalogAssign {
            catalog: assign.child_text("CtlgID").unwrap_or_default(),
            code: assign.child_text("CtlgCode").unwrap_or_default(),
        })
        .filter(|assign| !assign.catalog.is_empty() && !assign.code.is_empty())
        .collect()
}

impl Reader {
    fn decimal(&mut self, element: &Element, name: &str) -> Option<Decimal> {
        let raw = element.child_text(name)?;
        let cleaned = raw.replace(',', ".");
        match Decimal::from_str(&cleaned).or_else(|_| Decimal::from_scientific(&cleaned)) {
            Ok(value) => Some(value),
            Err(_) => {
                self.warnings.push(format!(
                    "Wert „{raw}“ in <{name}> ist keine Zahl und wurde übergangen."
                ));
                None
            }
        }
    }

    fn boq(&mut self, element: &Element) -> Boq {
        let mut boq = Boq::default();
        if let Some(info) = element.child("BoQInfo") {
            boq.name = info.child_text("Name").unwrap_or_default();
            boq.label = info.child_text("LblBoQ").unwrap_or_default();
            boq.change_order = change_order_tag(info);
            boq.date = info.child_text("Date");
            for level in info.children_named("BoQBkdn") {
                let kind = match level.child_text("Type").as_deref() {
                    Some("Lot") => BreakdownKind::Lot,
                    Some("Item") => BreakdownKind::Item,
                    Some("Index") => BreakdownKind::Index,
                    _ => BreakdownKind::Level,
                };
                boq.breakdown.push(BreakdownLevel {
                    kind,
                    label: level.child_text("LblBoQBkdn").unwrap_or_default(),
                    length: level
                        .child_text("Length")
                        .and_then(|length| length.parse().ok())
                        .unwrap_or(1),
                    numeric: !level
                        .child_text("Num")
                        .is_some_and(|num| num.eq_ignore_ascii_case("no")),
                });
            }
            let count: usize = info
                .child_text("NoUPComps")
                .and_then(|count| count.parse().ok())
                .unwrap_or(0);
            for index in 1..=6 {
                if let Some(label) = info.child(&format!("LblUPComp{index}")) {
                    boq.up_component_labels
                        .push(label.text().trim().to_string());
                    boq.up_component_kinds
                        .push(label.attr("Type").unwrap_or_default().to_string());
                } else if index <= count {
                    boq.up_component_labels.push(String::new());
                    boq.up_component_kinds.push(String::new());
                }
            }
            boq.time_label = info.child_text("LblTime").unwrap_or_default();
            boq.totals = info.child("Totals").map(|totals| self.totals(totals));
            boq.catalogs = info
                .children_named("Ctlg")
                .map(|catalog| Catalog {
                    id: catalog.child_text("CtlgID").unwrap_or_default(),
                    kind: catalog.child_text("CtlgType").unwrap_or_default(),
                    name: catalog.child_text("CtlgName").unwrap_or_default(),
                })
                .filter(|catalog| !catalog.id.is_empty())
                .collect();
        }
        self.level_lengths = boq
            .breakdown
            .iter()
            .filter(|level| matches!(level.kind, BreakdownKind::Lot | BreakdownKind::Level))
            .map(|level| level.length as usize)
            .collect();
        self.item_length = boq
            .breakdown
            .iter()
            .find(|level| level.kind == BreakdownKind::Item)
            .map(|level| level.length as usize)
            .unwrap_or(0);
        if let Some(body) = element.child("BoQBody") {
            boq.body = self.body(body, &mut Vec::new());
        }
        boq
    }

    fn totals(&mut self, element: &Element) -> Totals {
        Totals {
            total: self.decimal(element, "Total"),
            discount_percent: self.decimal(element, "DiscountPcnt"),
            total_after_discount: self.decimal(element, "TotAfterDisc"),
            vat_percent: self.decimal(element, "VAT"),
            total_gross: self.decimal(element, "TotalGross"),
            discount_amount: self.decimal(element, "DiscountAmt"),
            lump_sum: self.decimal(element, "TotalLSUM"),
            total_net: self.decimal(element, "TotalNet"),
            vat_amount: self.decimal(element, "VATAmount"),
        }
    }

    fn body(&mut self, element: &Element, path: &mut Vec<String>) -> Vec<BoqNode> {
        let mut nodes = Vec::new();
        for child in element.elements() {
            match child.name.as_str() {
                "BoQCtgy" => nodes.push(BoqNode::Category(self.category(child, path))),
                "Itemlist" => {
                    for entry in child.elements() {
                        if let Some(node) = self.list_entry(entry, path) {
                            nodes.push(node);
                        }
                    }
                }
                _ => {
                    if let Some(node) = self.list_entry(child, path) {
                        nodes.push(node);
                    }
                }
            }
        }
        nodes
    }

    fn list_entry(&mut self, element: &Element, path: &[String]) -> Option<BoqNode> {
        Some(match element.name.as_str() {
            "Remark" => {
                let (short_text, text, complements, dropped) =
                    description(element.child("Description"));
                self.dropped_images += dropped;
                let (stlb, standard_number) = match element.child("Description") {
                    Some(first) => standard::read(first, "Hinweistext", &mut self.warnings),
                    None => (None, None),
                };
                BoqNode::Remark(Remark {
                    text,
                    short_text,
                    complements,
                    stlb,
                    standard_number,
                })
            }
            "PerfDescr" => {
                let number = element.child_text("PerfNo").unwrap_or_default();
                if let Some(id) = element.attr("ID") {
                    self.perf_ids.insert(id.to_string(), number.clone());
                }
                let mut short_text = String::new();
                let mut long_text = RichText::default();
                let mut keys = (None, None);
                if let Some(first) = element.child("Description") {
                    let context = format!("Ausführungsbeschreibung {}", crate::perf_mark(&number));
                    keys = standard::read(first, &context, &mut self.warnings);
                }
                let mut complements = Vec::new();
                for part in element.children_named("Description") {
                    let (short, long, part_complements, dropped) = description(Some(part));
                    self.dropped_images += dropped;
                    if short_text.is_empty() {
                        short_text = short;
                    }
                    // Places of later parts count after the blocks before them.
                    let offset = long_text.blocks.len();
                    complements.extend(part_complements.into_iter().map(|mut complement| {
                        if let Some(anchor) = &mut complement.anchor {
                            anchor.block += offset;
                        }
                        complement
                    }));
                    long_text.blocks.extend(long.blocks);
                }
                BoqNode::PerfDescr(PerfDescr {
                    number,
                    label: element.child_text("PerfLbl").unwrap_or_default(),
                    short_text,
                    long_text,
                    stlb: keys.0,
                    standard_number: keys.1,
                    complements,
                })
            }
            "Item" | "MarkupItem" => BoqNode::Item(self.item(element, path)),
            _ => return None,
        })
    }

    fn category(&mut self, element: &Element, path: &mut Vec<String>) -> Category {
        let length = self.level_lengths.get(path.len()).copied().unwrap_or(0);
        let rno_part = pad(element.attr("RNoPart").unwrap_or_default(), length);
        path.push(rno_part.clone());
        let body = element
            .child("BoQBody")
            .map(|body| self.body(body, path))
            .unwrap_or_default();
        path.pop();
        let alternative = match (
            element.child_text("ALNBGroupNo"),
            element.child_text("ALNBSerNo"),
        ) {
            (Some(group), Some(serial)) => Some(Alternative { group, serial }),
            _ => None,
        };
        Category {
            rno_part,
            label: element.child("LblTx").map(read_plain).unwrap_or_default(),
            body,
            totals: element.child("Totals").map(|totals| self.totals(totals)),
            alternative,
            catalog_assigns: catalog_assigns(element),
        }
    }

    fn item(&mut self, element: &Element, path: &[String]) -> Item {
        let mut item = Item {
            rno_part: pad(
                element.attr("RNoPart").unwrap_or_default(),
                self.item_length,
            ),
            rno_index: element
                .attr("RNoIndex")
                .unwrap_or_default()
                .trim()
                .to_uppercase(),
            ..Item::default()
        };
        if let Some(id) = element.attr("ID") {
            self.item_ids
                .insert(id.to_string(), calc::ordinal(path, &item));
        }
        if let (Some(group), Some(serial)) = (
            element.child_text("ALNGroupNo"),
            element.child_text("ALNSerNo"),
        ) {
            item.kind.alternative = Some(Alternative { group, serial });
        }
        item.kind.provisional = match element.child_text("Provis").as_deref() {
            Some("WithoutTotal") => Some(Provisional::WithoutTotal),
            Some("WithTotal") => Some(Provisional::WithTotal),
            _ => None,
        };
        item.kind.lump_sum = yes(element, "LumpSumItem");
        item.kind.hourly = yes(element, "HourIt");
        item.kind.markup = yes(element, "MarkupIt");
        item.not_applicable = yes(element, "NotAppl");
        item.not_offered = yes(element, "NotOffered");
        item.key_item = yes(element, "KeyIt");
        item.change_order = change_order_tag(element);
        item.up_breakdown = yes(element, "UPBkdn");
        item.qty_to_be_determined = yes(element, "QtyTBD");
        // XML IDs for now, replaced by ordinal / PerfNo in `apply`.
        item.kind.repetition_of = element
            .child("RefRNo")
            .and_then(|reference| reference.attr("IDRef"))
            .map(str::to_string);
        item.kind.reference_description = element
            .child("RefPerfNo")
            .and_then(|reference| reference.attr("IDRef"))
            .map(str::to_string);
        item.qty = self.decimal(element, "Qty");
        item.unit = element.child_text("QU").unwrap_or_default();
        item.catalog_assigns = catalog_assigns(element);
        item.unit_price = self.decimal(element, "UP");
        let mut components = Vec::new();
        for index in 1..=6 {
            components.push(self.decimal(element, &format!("UPComp{index}")));
        }
        while components.last().is_some_and(Option::is_none) {
            components.pop();
        }
        item.up_components = components;
        item.discount_percent = self.decimal(element, "DiscountPcnt");
        item.total = self.decimal(element, "IT");
        item.time_quantity = self.decimal(element, "TimeQu");
        let dropped;
        (item.short_text, item.long_text, item.complements, dropped) =
            description(element.child("Description"));
        self.dropped_images += dropped;
        if let Some(description) = element.child("Description") {
            let context = format!("Position {}", calc::ordinal(path, &item));
            (item.stlb, item.standard_number) =
                standard::read(description, &context, &mut self.warnings);
        }
        for sub in element.children_named("SubDescr") {
            let (short_text, long_text, _, dropped) = description(sub.child("Description"));
            self.dropped_images += dropped;
            let number = sub.child_text("SubDNo").unwrap_or_default();
            let (stlb, standard_number) = match sub.child("Description") {
                Some(description) => {
                    let context = format!(
                        "Position {}, Unterbeschreibung {number}",
                        calc::ordinal(path, &item)
                    );
                    standard::read(description, &context, &mut self.warnings)
                }
                None => (None, None),
            };
            item.sub_descriptions.push(SubDescription {
                number,
                qty: self.decimal(sub, "Qty"),
                unit: sub.child_text("QU").unwrap_or_default(),
                short_text,
                long_text,
                stlb,
                standard_number,
            });
        }
        if element.name == "MarkupItem" {
            // As GAEB 90 reads a Zuschlagsposition.
            item.kind.markup = true;
            item.markup_percent = self.decimal(element, "Markup");
            item.markup_base = self.decimal(element, "ITMarkup");
            let mut surcharge = Surcharge {
                markup_type: element.child_text("MarkupType").unwrap_or_default(),
                items: Vec::new(),
            };
            for sub in element.children_named("MarkupSubQty") {
                if let Some(reference) = sub
                    .child("RefItem")
                    .and_then(|reference| reference.attr("IDRef"))
                {
                    surcharge.items.push(SurchargeItem {
                        ordinal: reference.to_string(),
                        sub_qty: self.decimal(sub, "SubQty"),
                    });
                }
            }
            item.surcharge = Some(surcharge);
        }
        item
    }

    /// Replaces the XML IDs collected while reading by ordinal and PerfNo.
    fn apply(&mut self, nodes: &mut [BoqNode]) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => self.apply(&mut category.body),
                BoqNode::Item(item) => {
                    if let Some(reference) = item.kind.repetition_of.take() {
                        match self.item_ids.get(&reference) {
                            Some(ordinal) => item.kind.repetition_of = Some(ordinal.clone()),
                            None => self.warnings.push(format!(
                                "Position {}: Bezug auf unbekannte Position „{reference}“.",
                                item.rno_part
                            )),
                        }
                    }
                    if let Some(reference) = item.kind.reference_description.take() {
                        match self.perf_ids.get(&reference) {
                            Some(number) => {
                                item.kind.reference_description = Some(number.clone())
                            }
                            None => self.warnings.push(format!(
                                "Position {}: Bezug auf unbekannte Ausführungsbeschreibung „{reference}“.",
                                item.rno_part
                            )),
                        }
                    }
                    if let Some(surcharge) = &mut item.surcharge {
                        let mut resolved = Vec::new();
                        for entry in surcharge.items.drain(..) {
                            match self.item_ids.get(&entry.ordinal) {
                                Some(ordinal) => resolved.push(SurchargeItem {
                                    ordinal: ordinal.clone(),
                                    sub_qty: entry.sub_qty,
                                }),
                                None => self.warnings.push(format!(
                                    "Zuschlagsposition {}: Bezug auf unbekannte Position „{}“.",
                                    item.rno_part, entry.ordinal
                                )),
                            }
                        }
                        surcharge.items = resolved;
                    }
                }
                BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
            }
        }
    }
}

/// CONo/COStatus of a BoQInfo or item, if the element has a Nachtragsnummer.
fn change_order_tag(element: &Element) -> Option<ChangeOrderTag> {
    let number = element.child_text("CONo")?.trim().parse().ok()?;
    Some(ChangeOrderTag {
        number,
        status: element
            .child_text("COStatus")
            .unwrap_or_default()
            .trim()
            .to_string(),
    })
}
