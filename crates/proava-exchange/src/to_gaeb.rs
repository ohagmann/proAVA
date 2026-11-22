use std::collections::BTreeSet;

use proava_domain::{
    ComplementKind, ImageAsset, Lv, LvRow, Money, Position, PositionKind, Project,
};
use proava_gaeb::{
    AddText, Alternative, Award, Boq, BoqNode, Category, GaebDocument, Item, ItemKind, PerfDescr,
    Phase, Provisional, Remark, RichText, Submission, SurchargeItem, Totals,
};

use crate::oz::{breakdown_with_labels, ordinal_pieces, MaskLayout, OzParts};
use crate::text::{
    address_to_gaeb, is_empty_address, iso_date, parse_deadline, procedure_code, rich_to_gaeb,
    short_list,
};

/// Longest short text (Kurztext) GAEB carries: fixed in GAEB 90, checked by
/// the GAEB DA XML test programs.
pub(crate) const SHORT_TEXT_MAX: usize = 70;

/// Domain → GAEB exchange model for phase 81, 82 or 83 (84–86 behave like 82).
/// Prices and totals only in phases with prices; sums count only positions
/// that count in the LV total. `date` stays empty for the caller to fill.
pub fn lv_to_gaeb(project: &Project, phase: Phase) -> (GaebDocument, Vec<String>) {
    let lv = project.lv();
    let layout = MaskLayout::of(lv.oz_mask());
    let numbered = {
        let mut copy = lv.clone();
        copy.renumber();
        copy
    };
    let priced = phase.has_prices();
    let mut checks = Checks::default();
    let mut tree = TreeBuilder::new(priced, lv.vat_percent());
    let mut images = 0;
    let mut dotted_by_id = std::collections::HashMap::new();
    let mut add_texts = Vec::new();
    let mut closing_texts = Vec::new();
    let mut repeats = Vec::new();
    // Zuschlagspositionen: base and amount (planner's prices), listed positions.
    let surcharge_values = lv.surcharge_values();
    let mut surcharge_lists = Vec::new();
    // The title of the rows that follow has subtitles (its positions then
    // need an empty intermediate category, see below).
    let mut title_has_subtitles = false;

    for (index, row) in lv.rows().iter().enumerate() {
        match row {
            LvRow::Group(group) => {
                let level_count = layout.level_count();
                if level_count == 0 {
                    checks.notes.push(format!(
                        "Titel „{}“: die OZ-Maske hat keine Titelstufe – als Hinweis übertragen.",
                        group.short_text()
                    ));
                    tree.add(BoqNode::Remark(Remark {
                        text: RichText::plain(group.short_text()),
                        ..Remark::default()
                    }));
                    continue;
                }
                let mut level = group.level();
                if level >= level_count {
                    checks.notes.push(format!(
                        "Titel {}: Ebene {} liegt außerhalb der OZ-Maske – als Ebene {} übertragen.",
                        group.ordinal(),
                        level + 1,
                        level_count
                    ));
                    level = level_count - 1;
                }
                tree.close_to(level);
                title_has_subtitles = lv.rows()[index + 1..]
                    .iter()
                    .find_map(LvRow::as_group)
                    .is_some_and(|next| next.level() > group.level());
                let pieces = valid_group_pieces(group.ordinal(), level)
                    .or_else(|| valid_group_pieces(numbered.rows()[index].ordinal(), level))
                    .unwrap_or_default();
                if tree.depth() < level {
                    checks.notes.push(format!(
                        "Titel {}: übersprungene Gliederungsebene – Zwischenstufe ohne Text eingefügt.",
                        group.ordinal()
                    ));
                }
                while tree.depth() < level {
                    let depth = tree.depth();
                    tree.open(
                        pieces
                            .get(depth)
                            .cloned()
                            .unwrap_or_else(|| layout.zeros(depth)),
                        String::new(),
                    );
                }
                let rno = pieces
                    .get(level)
                    .cloned()
                    .unwrap_or_else(|| layout.zeros(level));
                if pieces.is_empty() {
                    checks.notes.push(format!(
                        "Titel „{}“ hat keine gültige OZ – als {} übertragen.",
                        group.short_text(),
                        rno
                    ));
                }
                tree.open(rno, group.short_text().to_string());
                tree.lump_open(group.lump_sum_total());
                let mut assigns = cost_assign(lv, group.cost_group());
                assigns.extend(work_assign(lv, group.work_category()));
                tree.assign_open(assigns);
            }
            LvRow::Position(position) => {
                // Imported LVs may hold positions above the deepest level
                // (01.01.0010 in an 11.22.33.PPPP mask); they keep their OZ.
                let below = || layout.split_position_below(position.ordinal(), &tree.path());
                let full = || layout.split_position(position.ordinal());
                let split = if tree.depth() < layout.level_count() {
                    below().or_else(full)
                } else {
                    full()
                };
                let mut parts = match split {
                    Some(parts) => parts,
                    None => {
                        let fallback = numbered.rows()[index].ordinal();
                        checks.notes.push(format!(
                            "Position „{}“: OZ „{}“ passt nicht zur OZ-Maske – als {} übertragen.",
                            position.short_text(),
                            position.ordinal(),
                            fallback
                        ));
                        match layout.split_position(fallback) {
                            Some(parts) => parts,
                            None => continue,
                        }
                    }
                };
                if tree.depth() < layout.level_count() && parts.levels.len() == layout.level_count()
                {
                    // GAEB DA XML 3.3, Fachdokumentation 4.3.4 „Unbelegte
                    // Hierarchiestufen“: lower levels may stay unused – a
                    // position under a higher title („01.001.00.00.0010“)
                    // goes straight into that LV-Bereich, as long as it has
                    // no sub-areas (a Bereich holds either positions or
                    // Bereiche). Only then an empty intermediate category.
                    let depth = tree.depth();
                    let unused = parts.levels[depth..]
                        .iter()
                        .all(|piece| crate::oz::unused_piece(piece));
                    if depth > 0
                        && unused
                        && !title_has_subtitles
                        && parts.levels[..depth] == tree.path()[..]
                    {
                        parts.levels.truncate(depth);
                    } else {
                        checks.not_deepest.push(position.ordinal().to_string());
                        while tree.depth() < layout.level_count() {
                            let depth = tree.depth();
                            tree.open(parts.levels[depth].clone(), String::new());
                        }
                    }
                }
                checks.position(position, priced);
                let dotted = parts.dotted();
                dotted_by_id.insert(position.id(), dotted.clone());
                if let Some(base) = position.repeats() {
                    repeats.push((dotted.clone(), base));
                }
                let (mut item, image_count) = item_of(position, parts, priced, project.images());
                let mut counted = if priced {
                    position.counted_total()
                } else {
                    Money::ZERO
                };
                if let Some(surcharge) = position.surcharge() {
                    let value = surcharge_values
                        .get(&position.id())
                        .copied()
                        .unwrap_or_default();
                    surcharge_to_gaeb(&mut item, position, surcharge, value, priced);
                    counted = if priced && position.counts_in_total() {
                        value.amount
                    } else {
                        Money::ZERO
                    };
                    if !surcharge.references.is_empty() {
                        surcharge_lists.push((dotted.clone(), surcharge.references.clone()));
                    }
                }
                item.catalog_assigns = cost_assign(lv, position.cost_group());
                item.catalog_assigns
                    .extend(work_assign(lv, position.work_category()));
                item.kind.reference_description = lv
                    .performance_of(position)
                    .map(|performance| performance.number().to_string());
                images += image_count;
                tree.add_item(item, counted);
            }
            LvRow::Performance(performance) => {
                checks.performance(performance);
                let (long_text, image_count) =
                    rich_to_gaeb(performance.long_text(), project.images());
                images += image_count;
                let (complements, stlb, standard_number) =
                    crate::stlb::text_to_gaeb(performance.complements(), performance.catalog());
                tree.add(BoqNode::PerfDescr(PerfDescr {
                    number: performance.number().to_string(),
                    label: performance.label().to_string(),
                    short_text: performance.short_text().to_string(),
                    long_text,
                    stlb,
                    standard_number,
                    complements,
                }));
            }
            LvRow::Note(note) if note.is_preliminary() || note.is_closing() => {
                let (detail, image_count) = rich_to_gaeb(note.text(), project.images());
                images += image_count;
                let text = AddText {
                    outline: note.heading().to_string(),
                    detail,
                };
                if note.is_closing() {
                    closing_texts.push(text);
                } else {
                    add_texts.push(text);
                }
            }
            LvRow::Note(note) => {
                let (text, image_count) = rich_to_gaeb(note.text(), project.images());
                images += image_count;
                let mut remark = Remark {
                    text,
                    short_text: note.heading().to_string(),
                    ..Remark::default()
                };
                crate::stlb::remark_from_note(note, &mut remark);
                tree.add(BoqNode::Remark(remark));
            }
            // GAEB has no empty line: the Abstandshalter stays at home (the
            // export check counts them as a hint).
            LvRow::Spacer(_) => {}
        }
    }

    let (mut body, net) = tree.finish();
    let repeats: std::collections::HashMap<String, String> = repeats
        .into_iter()
        .filter_map(|(own, base)| Some((own, dotted_by_id.get(&base)?.clone())))
        .collect();
    if !repeats.is_empty() {
        set_repetitions(&mut body, &mut Vec::new(), &repeats);
    }
    // Listed positions of Zuschläge (MarkupSubQty) by their GAEB ordinal.
    let surcharge_lists: std::collections::HashMap<String, Vec<SurchargeItem>> = surcharge_lists
        .into_iter()
        .map(|(own, references)| {
            let items = references
                .iter()
                .filter_map(|reference| {
                    Some(SurchargeItem {
                        ordinal: dotted_by_id.get(&reference.position)?.clone(),
                        sub_qty: reference.quantity,
                    })
                })
                .collect();
            (own, items)
        })
        .collect();
    if !surcharge_lists.is_empty() {
        set_surcharge_items(&mut body, &mut Vec::new(), &surcharge_lists);
    }
    // Whether the images go into the file is decided when writing it
    // (Exportdialog „Bilder mitgeben“); the writer reports them there.
    let _ = images;
    let mut warnings = checks.finish(lv);

    let info = project.info();
    let (open_date, open_time) = parse_deadline(&info.submission_deadline);
    for (value, what) in [
        (&info.execution_start, "Ausführungsbeginn"),
        (&info.execution_end, "Ausführungsende"),
        (&info.binding_period_end, "Bindefrist"),
    ] {
        if !value.trim().is_empty() && iso_date(value).is_none() {
            warnings.push(format!(
                "{what} „{value}“ ist kein Datum (TT.MM.JJJJ) – fehlt im GAEB."
            ));
        }
    }
    if !info.award_procedure.trim().is_empty() && procedure_code(&info.award_procedure).is_empty() {
        warnings.push(format!(
            "Vergabeart „{}“ kennt GAEB nicht – fehlt im GAEB.",
            info.award_procedure
        ));
    }
    if !info.submission_deadline.trim().is_empty() && open_date.is_none() {
        warnings.push(format!(
            "Abgabetermin „{}“ ist kein Datum (TT.MM.JJJJ) – im GAEB fehlt der Eröffnungstermin.",
            info.submission_deadline
        ));
    }

    let totals = priced.then(|| {
        // A Pauschalsumme (TotalLSUM) replaces the sum of the positions for
        // VAT and gross; Total stays the sum of the positions.
        let lump = lv.lump_sum_total();
        let base = lump.unwrap_or(net);
        let vat = lv.vat(base);
        Totals {
            total: Some(net.to_decimal()),
            lump_sum: lump.map(|lump| lump.to_decimal()),
            vat_percent: Some(lv.vat_percent()),
            total_gross: Some((base + vat).to_decimal()),
            ..Totals::default()
        }
    });

    let document = GaebDocument {
        phase,
        program: format!("proAVA {}", env!("CARGO_PKG_VERSION")),
        date: None,
        project: proava_gaeb::ProjectInfo {
            name: if info.short_name.trim().is_empty() {
                project.name().as_str().to_string()
            } else {
                info.short_name.clone()
            },
            label: info.label.clone(),
            currency: lv.currency().to_string(),
            currency_label: currency_label(lv.currency()).to_string(),
            ..proava_gaeb::ProjectInfo::default()
        },
        award: Award {
            // GAEB OWN is the Auftraggeber (contracting authority): the own
            // one, else the Bauherr.
            owner: (!is_empty_address(info.client())).then(|| address_to_gaeb(info.client())),
            owner_dp_no: if info.exchange_number.trim().is_empty() {
                String::new()
            } else {
                info.exchange_number.trim().to_string()
            },
            owner_account: String::new(),
            owner_country: Default::default(),
            bidder_dp_no: String::new(),
            requesters: Vec::new(),
            sites: Vec::new(),
            bidder: None,
            bid_date: None,
            submission: Submission {
                location: info.submission_place.clone(),
                open_date,
                open_time,
                construction_start: iso_date(&info.execution_start),
                construction_end: iso_date(&info.execution_end),
                award_deadline: iso_date(&info.binding_period_end),
            },
            boq: Boq {
                name: if lv.number().trim().is_empty() {
                    "1".to_string()
                } else {
                    lv.number().to_string()
                },
                label: lv.title().to_string(),
                breakdown: {
                    let mut breakdown = breakdown_with_labels(lv.oz_mask(), lv.level_labels());
                    // GAEB: Num = No when position numbers may hold letters.
                    let letters = lv.has_alphanumeric_positions()
                        || lv.positions().any(|position| {
                            layout
                                .split_position(position.ordinal())
                                .is_some_and(|parts| {
                                    !parts.position.bytes().all(|byte| byte.is_ascii_digit())
                                })
                        });
                    for level in &mut breakdown {
                        if level.kind == proava_gaeb::BreakdownKind::Item {
                            level.numeric = !letters;
                        }
                    }
                    breakdown
                },
                // The EP shares belong to the description of the LV: the X83
                // asks for them (the bidder fills them in), and like other AVA
                // programs the X81 already names them (Anwender 18.11.).
                up_component_labels: lv
                    .price_components()
                    .iter()
                    .map(|component| component.label.clone())
                    .collect(),
                up_component_kinds: lv
                    .price_components()
                    .iter()
                    .map(|component| component.kind.clone())
                    .collect(),
                body,
                totals,
                time_label: lv.time_label().to_string(),
                catalogs: lv_catalogs(lv),
                ..Boq::default()
            },
            award_no: info.award_number.clone(),
            contract_text: Vec::new(),
            category: procedure_code(&info.award_procedure),
            add_texts,
            closing_texts,
        },
    };
    let mut document = document;
    crate::text::places_to_gaeb(info, &mut document.award);
    (document, warnings)
}

/// Name of the currency for GAEB CurLbl (Euro, US-Dollar, Schweizer
/// Franken – the ones proAVA offers; else empty).
fn currency_label(code: &str) -> &'static str {
    proava_domain::price_check::currency_name(code)
}

/// Wiederholungsbeschreibungen point to their base by its GAEB ordinal.
fn set_repetitions(
    nodes: &mut [BoqNode],
    path: &mut Vec<String>,
    repeats: &std::collections::HashMap<String, String>,
) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                path.push(category.rno_part.clone());
                set_repetitions(&mut category.body, path, repeats);
                path.pop();
            }
            BoqNode::Item(item) => {
                let mut pieces = path.clone();
                pieces.push(item.rno_part.clone());
                if !item.rno_index.is_empty() {
                    pieces.push(item.rno_index.clone());
                }
                item.kind.repetition_of = repeats.get(&pieces.join(".")).cloned();
            }
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
        }
    }
}

fn set_surcharge_items(
    nodes: &mut [BoqNode],
    path: &mut Vec<String>,
    lists: &std::collections::HashMap<String, Vec<SurchargeItem>>,
) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                path.push(category.rno_part.clone());
                set_surcharge_items(&mut category.body, path, lists);
                path.pop();
            }
            BoqNode::Item(item) => {
                let mut pieces = path.clone();
                pieces.push(item.rno_part.clone());
                if !item.rno_index.is_empty() {
                    pieces.push(item.rno_index.clone());
                }
                if let (Some(surcharge), Some(items)) =
                    (&mut item.surcharge, lists.get(&pieces.join(".")))
                {
                    surcharge.items = items.clone();
                }
            }
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => {}
        }
    }
}

/// A Zuschlagsposition as GAEB MarkupItem: no quantity, unit or unit price;
/// MarkupType, the percentage (Markup), the base (ITMarkup) and the total
/// (IT) in phases with prices. Listed positions follow once all ordinals are
/// known ([`set_surcharge_items`]).
fn surcharge_to_gaeb(
    item: &mut Item,
    position: &Position,
    surcharge: &proava_domain::Surcharge,
    value: proava_domain::SurchargeValue,
    priced: bool,
) {
    item.surcharge = Some(proava_gaeb::Surcharge {
        markup_type: surcharge.kind.gaeb().to_string(),
        items: Vec::new(),
    });
    item.qty = None;
    item.qty_to_be_determined = false;
    item.unit.clear();
    item.unit_price = None;
    item.up_components.clear();
    item.up_breakdown = false;
    item.time_quantity = None;
    item.kind.lump_sum = false;
    item.kind.hourly = false;
    item.total = None;
    if priced {
        item.markup_percent = position.surcharge_percent();
        item.markup_base = Some(value.base.to_decimal());
        if position.surcharge_percent().is_some() && proava_gaeb::xml::shows_total(item) {
            item.total = Some(value.amount.to_decimal());
        }
    }
}

fn valid_group_pieces(ordinal: &str, level: usize) -> Option<Vec<String>> {
    let pieces = ordinal_pieces(ordinal);
    (pieces.len() == level + 1).then_some(pieces)
}

fn item_of(
    position: &Position,
    parts: OzParts,
    priced: bool,
    assets: &[ImageAsset],
) -> (Item, usize) {
    let (long_text, images) = rich_to_gaeb(position.long_text(), assets);
    let complements = position
        .complements()
        .iter()
        .map(|complement| proava_gaeb::TextComplement {
            marker: complement.marker.clone(),
            kind: match complement.kind {
                ComplementKind::Owner => "Owner",
                ComplementKind::Bidder => "Bidder",
            }
            .to_string(),
            caption: complement.caption.clone(),
            body: complement.body.clone(),
            tail: complement.tail.clone(),
            stlb_characteristic: None,
            number: None,
            anchor: complement.anchor.map(|anchor| proava_gaeb::TextAnchor {
                block: anchor.block,
                offset: anchor.offset,
                own_line: anchor.own_line,
            }),
        })
        .collect();
    let mut item = Item {
        rno_part: parts.position,
        rno_index: parts.index,
        kind: kind_of(position),
        qty: Some(position.quantity()),
        qty_to_be_determined: position.is_quantity_open(),
        unit: position.unit().to_string(),
        short_text: position.short_text().to_string(),
        long_text,
        complements,
        not_applicable: position.is_not_applicable(),
        key_item: position.is_key_item(),
        up_breakdown: position.has_price_breakdown(),
        sub_descriptions: position
            .sub_descriptions()
            .iter()
            .map(|sub| proava_gaeb::SubDescription {
                number: sub.number.clone(),
                qty: sub.quantity,
                unit: sub.unit.clone(),
                short_text: sub.short_text.clone(),
                long_text: rich_to_gaeb(&sub.long_text, assets).0,
                stlb: crate::stlb::catalog_to_gaeb(&sub.catalog).0,
                standard_number: crate::stlb::catalog_to_gaeb(&sub.catalog).1,
            })
            .collect(),
        ..Item::default()
    };
    crate::stlb::keys_to_gaeb(position, &mut item);
    if priced {
        item.unit_price = position.unit_price();
        item.up_components = position.price_components().to_vec();
        item.time_quantity = position.time_quantity();
        // Wahlpositionen, Bedarfspositionen ohne GB and entfallende
        // Positionen carry „Nur Einh.-Preis“, no total (FD 3.3 4.5.2, 4.5.3).
        if proava_gaeb::xml::shows_total(&item) && position.unit_price().is_some() {
            item.total = Some(position.total_price().to_decimal());
        }
    }
    (item, images)
}

pub(crate) fn kind_of(position: &Position) -> ItemKind {
    let mut kind = ItemKind {
        lump_sum: position.is_lump_sum(),
        hourly: position.is_hourly(),
        // „zu bezuschlagen“ (MarkupIt) – for Zuschläge on marked positions
        markup: position.is_marked_for_surcharge() && !position.is_surcharge(),
        ..ItemKind::default()
    };
    match position.kind() {
        PositionKind::Normal => {}
        PositionKind::Provisional { with_total } => {
            kind.provisional = Some(if with_total {
                Provisional::WithTotal
            } else {
                Provisional::WithoutTotal
            });
        }
        PositionKind::Base { group } => {
            kind.alternative = Some(Alternative {
                group: group.to_string(),
                serial: "0".to_string(),
            });
        }
        PositionKind::Alternative { group, serial } => {
            kind.alternative = Some(Alternative {
                group: group.to_string(),
                serial: serial.to_string(),
            });
        }
        // A Zulageposition is an ordinary priced item in GAEB; MarkupIt marks
        // an item „zu bezuschlagen“ (see `markup` above), something else.
        PositionKind::Supplement => {}
    }
    kind
}

/// Catalogs of the LV that are in use: DIN 276 and Leistungsbereiche.
fn lv_catalogs(lv: &Lv) -> Vec<proava_gaeb::Catalog> {
    let mut catalogs = Vec::new();
    if lv.has_cost_groups() {
        let catalog = lv.cost_catalog();
        catalogs.push(proava_gaeb::Catalog {
            id: catalog.id.clone(),
            kind: catalog.kind.clone(),
            name: catalog.name.clone(),
        });
    }
    if lv.has_work_categories() {
        let catalog = lv.work_catalog();
        // Two catalogs need two ids (a file could bring the same one).
        if !catalogs.iter().any(|other| other.id == catalog.id) {
            catalogs.push(proava_gaeb::Catalog {
                id: catalog.id.clone(),
                kind: catalog.kind.clone(),
                name: catalog.name.clone(),
            });
        }
    }
    catalogs
}

/// CtlgAssign for a Leistungsbereich in the LV's catalog.
fn work_assign(lv: &Lv, code: Option<&str>) -> Vec<proava_gaeb::CatalogAssign> {
    code.map(|code| proava_gaeb::CatalogAssign {
        catalog: lv.work_catalog().id.clone(),
        code: code.to_string(),
    })
    .into_iter()
    .collect()
}

/// CtlgAssign for a cost group in the LV's catalog.
fn cost_assign(lv: &Lv, code: Option<&str>) -> Vec<proava_gaeb::CatalogAssign> {
    code.map(|code| proava_gaeb::CatalogAssign {
        catalog: lv.cost_catalog().id.clone(),
        code: code.to_string(),
    })
    .into_iter()
    .collect()
}

struct OpenCategory {
    category: Category,
    total: Money,
    /// Pauschalsumme of the title (TotalLSUM): counts above instead of `total`.
    lump: Option<Money>,
    /// Whether positions stand in it (a title with nothing but hints gets
    /// no gross sum, as in AVA programs).
    has_items: bool,
}

/// Turns the flat outline into nested categories.
struct TreeBuilder {
    root: Vec<BoqNode>,
    open: Vec<OpenCategory>,
    total: Money,
    priced: bool,
    /// Umsatzsteuer in %: the top-level titles also carry their gross sum,
    /// as AVA programs write it (Los/Titel brutto).
    vat_percent: proava_domain::Decimal,
}

impl TreeBuilder {
    fn new(priced: bool, vat_percent: proava_domain::Decimal) -> Self {
        Self {
            root: Vec::new(),
            open: Vec::new(),
            total: Money::ZERO,
            priced,
            vat_percent,
        }
    }

    fn depth(&self) -> usize {
        self.open.len()
    }

    /// Order number pieces of the open categories.
    fn path(&self) -> Vec<String> {
        self.open
            .iter()
            .map(|open| open.category.rno_part.clone())
            .collect()
    }

    fn body(&mut self) -> &mut Vec<BoqNode> {
        match self.open.last_mut() {
            Some(open) => &mut open.category.body,
            None => &mut self.root,
        }
    }

    fn open(&mut self, rno_part: String, label: String) {
        self.open.push(OpenCategory {
            category: Category {
                rno_part,
                label,
                ..Category::default()
            },
            total: Money::ZERO,
            lump: None,
            has_items: false,
        });
    }

    /// Pauschalsumme of the category opened last.
    fn lump_open(&mut self, lump: Option<Money>) {
        if let Some(open) = self.open.last_mut() {
            open.lump = lump;
        }
    }

    /// Catalog assignments of the category opened last.
    fn assign_open(&mut self, assigns: Vec<proava_gaeb::CatalogAssign>) {
        if let Some(open) = self.open.last_mut() {
            open.category.catalog_assigns = assigns;
        }
    }

    fn close_to(&mut self, depth: usize) {
        while self.open.len() > depth {
            let Some(open) = self.open.pop() else { break };
            let mut category = open.category;
            // A Pauschalsumme counts above instead of the sum of the block.
            let lump = open.lump.filter(|_| self.priced);
            if let Some(lump) = lump {
                let delta = lump - open.total;
                for parent in &mut self.open {
                    parent.total += delta;
                }
                self.total += delta;
            }
            if self.priced {
                let effective = lump.unwrap_or(open.total);
                let top_level = self.open.is_empty() && (open.has_items || lump.is_some());
                category.totals = Some(Totals {
                    total: Some(open.total.to_decimal()),
                    lump_sum: lump.map(|lump| lump.to_decimal()),
                    total_gross: top_level
                        .then(|| (effective + effective.percent(self.vat_percent)).to_decimal()),
                    ..Totals::default()
                });
            }
            self.body().push(BoqNode::Category(category));
        }
    }

    fn add(&mut self, node: BoqNode) {
        self.body().push(node);
    }

    fn add_item(&mut self, item: Item, counted: Money) {
        for open in &mut self.open {
            open.total += counted;
            open.has_items = true;
        }
        self.total += counted;
        self.add(BoqNode::Item(item));
    }

    fn finish(mut self) -> (Vec<BoqNode>, Money) {
        self.close_to(0);
        (self.root, self.total)
    }
}

/// Collects findings per kind so the user gets one line each.
#[derive(Default)]
struct Checks {
    notes: Vec<String>,
    not_deepest: Vec<String>,
    no_short_text: Vec<String>,
    long_short_text: Vec<String>,
    no_quantity: Vec<String>,
    no_unit: Vec<String>,
    no_price: Vec<String>,
    stlb: Vec<String>,
    /// STLB-Bau positions whose Langtext was changed: written without key.
    stlb_changed: Vec<String>,
    bases: BTreeSet<u32>,
    alternatives: BTreeSet<u32>,
    serial_zero: Vec<String>,
    performance_numbers: Vec<String>,
    performance_labels: Vec<String>,
    both_references: Vec<String>,
}

/// Longest label (PerfLbl) of an Ausführungsbeschreibung in GAEB.
const PERFORMANCE_LABEL_MAX: usize = 55;

impl Checks {
    fn performance(&mut self, performance: &proava_domain::PerformanceDescription) {
        let number = performance.number();
        if !number
            .parse::<u32>()
            .is_ok_and(|value| (1..=9999).contains(&value))
        {
            self.performance_numbers.push(format!("„{number}“"));
        }
        let length = performance.label().chars().count();
        if length > PERFORMANCE_LABEL_MAX {
            self.performance_labels.push(format!("{number} ({length})"));
        }
    }

    fn position(&mut self, position: &Position, priced: bool) {
        let ordinal = position.ordinal().to_string();
        if position.short_text().trim().is_empty() {
            self.no_short_text.push(ordinal.clone());
        }
        let length = position.short_text().chars().count();
        if length > SHORT_TEXT_MAX {
            self.long_short_text.push(format!("{ordinal} ({length})"));
        }
        // A Zuschlagsposition has no quantity and unit of its own.
        if position.quantity().is_zero() && !position.is_surcharge() {
            self.no_quantity.push(ordinal.clone());
        }
        if position.unit().trim().is_empty() && !position.is_surcharge() {
            self.no_unit.push(ordinal.clone());
        }
        if priced && position.unit_price().is_none() {
            self.no_price.push(ordinal.clone());
        }
        if position.is_stlb_placeholder() {
            self.stlb.push(ordinal.clone());
        }
        if position.stlb_text_changed() {
            self.stlb_changed.push(ordinal.clone());
        }
        if position.repeats().is_some() && position.performance().is_some() {
            self.both_references.push(ordinal.clone());
        }
        match position.kind() {
            PositionKind::Base { group } => {
                self.bases.insert(group);
            }
            PositionKind::Alternative { group, serial } => {
                self.alternatives.insert(group);
                if serial == 0 {
                    self.serial_zero.push(ordinal);
                }
            }
            _ => {}
        }
    }

    fn finish(self, lv: &Lv) -> Vec<String> {
        let mut warnings = self.notes;
        let mut list = |entries: &[String], text: &str| {
            if !entries.is_empty() {
                warnings.push(format!("{text}: {}", short_list(entries, 10)));
            }
        };
        list(
            &self.not_deepest,
            "Positionen neben Untertiteln im selben Titel oder vor dem ersten Titel (GAEB: ein LV-Bereich enthält entweder Positionen oder weitere LV-Bereiche – Zwischenstufe ohne Text eingefügt)",
        );
        list(&self.no_short_text, "Positionen ohne Kurztext");
        list(
            &self.long_short_text,
            &format!("Kurztext länger als {SHORT_TEXT_MAX} Zeichen (GAEB-Grenze, nicht gekürzt – bitte kürzen)"),
        );
        list(&self.no_quantity, "Positionen ohne Menge");
        list(&self.no_unit, "Positionen ohne Einheit");
        list(&self.no_price, "Positionen ohne Einheitspreis");
        list(&self.stlb, "STLB-Platzhalter ohne fertigen Text");
        list(
            &self.stlb_changed,
            "Langtext weicht von STLB-Bau ab – als freier Text ohne STLB-Bau-Kennung exportiert (umwandeln oder aus STLB-Bau aktualisieren)",
        );
        list(
            &self.serial_zero,
            "Wahlpositionen mit Wahlnummer 0 (GAEB: 0 = Grundposition)",
        );
        let orphans: Vec<String> = self
            .alternatives
            .difference(&self.bases)
            .map(|group| group.to_string())
            .collect();
        list(&orphans, "Wahlgruppen ohne Grundposition");
        let lonely: Vec<String> = self
            .bases
            .difference(&self.alternatives)
            .map(|group| group.to_string())
            .collect();
        list(&lonely, "Grundpositionen ohne Wahlposition (Gruppe)");
        list(&lv.duplicate_ordinals(), "Doppelte Ordnungszahlen");
        list(
            &self.performance_numbers,
            "Ausführungsbeschreibungen ohne Nummer 1–9999 (GAEB verlangt sie; Bezüge darauf gehen verloren)",
        );
        list(
            &self.performance_labels,
            &format!("Bezeichnung von Ausführungsbeschreibungen länger als {PERFORMANCE_LABEL_MAX} Zeichen (wird gekürzt)"),
        );
        list(
            &self.both_references,
            "Positionen mit Wiederholung und Ausführungsbeschreibung (GAEB trägt nur einen Bezug – die Wiederholung, der Bezug auf die AB fehlt)",
        );
        warnings
    }
}
