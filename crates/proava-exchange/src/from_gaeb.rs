use std::collections::HashMap;

use proava_domain::{
    ComplementKind, Decimal, Lv, LvGroup, LvRow, Note, OzMask, PerformanceDescription, Position,
    PositionKind, PriceComponent, Project, ProjectName, RichText, RowId, Run, SubDescription,
    Surcharge, SurchargeKind, SurchargeReference, TextBlock, TextComplement,
};
use proava_gaeb::{BoqNode, GaebDocument, Item, PerfDescr, Phase, Provisional};

use crate::oz::{detect_step, labels_from_breakdown, mask_from_breakdown};
use crate::text::{
    address_from_gaeb, format_deadline, procedure_label, rich_from_gaeb, short_list, ImportedImages,
};

/// GAEB exchange model → new project. Order numbers are taken as written
/// (never renumbered); the position step is derived from them.
pub fn gaeb_to_project(
    doc: &GaebDocument,
    name: ProjectName,
    created_iso: &str,
) -> (Project, Vec<String>) {
    let mut warnings = Vec::new();
    let boq = &doc.award.boq;

    // Some files give items an index without an index level in the
    // breakdown; the mask gets one so the order numbers stay as they are.
    let mut breakdown = boq.breakdown.clone();
    let index_width = boq
        .items()
        .iter()
        .map(|(_, item)| item.rno_index.trim().chars().count())
        .max()
        .unwrap_or(0);
    let declared: usize = breakdown
        .iter()
        .filter(|level| level.kind == proava_gaeb::BreakdownKind::Index)
        .map(|level| usize::from(level.length))
        .sum();
    if index_width > declared {
        breakdown.retain(|level| level.kind != proava_gaeb::BreakdownKind::Index);
        breakdown.push(proava_gaeb::BreakdownLevel {
            kind: proava_gaeb::BreakdownKind::Index,
            label: String::new(),
            length: index_width.min(9) as u8,
            numeric: false,
        });
        warnings.push(
            "Die Datei hat Positionen mit Index, aber keine Indexstelle in der OZ-Maske – sie wurde ergänzt."
                .to_string(),
        );
    }
    let mask = mask_from_breakdown(&breakdown).unwrap_or_else(|reason| {
        warnings.push(format!(
            "{reason} Die OZ-Maske {} wird angenommen.",
            OzMask::DEFAULT_PATTERN
        ));
        OzMask::parse(OzMask::DEFAULT_PATTERN).expect("default mask is valid")
    });
    let mut alphanumeric = false;
    if boq
        .breakdown
        .iter()
        .any(|level| level.kind == proava_gaeb::BreakdownKind::Item && !level.numeric)
    {
        alphanumeric = true;
        warnings.push(
            "Die Positionsnummern der Datei sind alphanumerisch; sie bleiben als Text erhalten."
                .to_string(),
        );
    }

    let title = [
        boq.label.as_str(),
        boq.name.as_str(),
        doc.project.label.as_str(),
        name.as_str(),
    ]
    .into_iter()
    .find(|text| !text.trim().is_empty())
    .unwrap_or_default();
    // The LV title is one line; some files (e.g. manufacturer texts in GAEB 90)
    // put a whole note into the label – it stays in the project label.
    let title = title
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(one_line)
        .unwrap_or_default();
    let mut lv = Lv::new(title, mask);
    if lv.level_count() == labels_from_breakdown(&boq.breakdown).len() {
        lv.set_level_labels(labels_from_breakdown(&boq.breakdown));
    }
    lv.set_number(boq.name.trim());
    lv.set_alphanumeric_positions(alphanumeric);
    lv.set_time_label(boq.time_label.trim());
    // Kostengruppen: the DIN 276 catalog of the file, the current edition
    // (2018) first – STLB-Bau Online assigns both 2018 and 2008-12.
    let cost_catalog = boq
        .catalogs
        .iter()
        .find(|catalog| catalog.is_cost_groups() && catalog.kind.contains("2018"))
        .or_else(|| boq.catalogs.iter().find(|catalog| catalog.is_cost_groups()));
    if let Some(catalog) = cost_catalog {
        lv.set_cost_catalog(proava_domain::CostCatalog {
            id: catalog.id.trim().to_string(),
            kind: catalog.kind.trim().to_string(),
            name: catalog.name.trim().to_string(),
        });
    }
    let cost_catalog = cost_catalog.map(|catalog| catalog.id.trim().to_string());
    // Leistungsbereiche: the first catalog of the type „work category“.
    let work_catalog = boq
        .catalogs
        .iter()
        .find(|catalog| catalog.is_work_categories());
    if let Some(catalog) = work_catalog {
        lv.set_work_catalog(proava_domain::WorkCatalog {
            id: catalog.id.trim().to_string(),
            kind: catalog.kind.trim().to_string(),
            name: catalog.name.trim().to_string(),
        });
    }
    let work_catalog = work_catalog.map(|catalog| catalog.id.trim().to_string());
    // EP-Anteile auch aus X81/X83: die Angebotsaufforderung fragt die
    // Aufgliederung schon ab (UPBkdn), ohne Bezeichnungen fehlte sie im Export.
    if doc.phase.has_prices() || !boq.up_component_labels.is_empty() {
        lv.set_price_components(
            boq.up_component_labels
                .iter()
                .enumerate()
                .map(|(index, label)| PriceComponent {
                    label: label.trim().to_string(),
                    kind: boq
                        .up_component_kinds
                        .get(index)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
        );
    }
    if !doc.project.currency.trim().is_empty() {
        lv.set_currency(doc.project.currency.trim());
    }
    if let Some(vat) = boq.totals.as_ref().and_then(|totals| totals.vat_percent) {
        lv.set_vat_percent(vat);
    }

    let priced = doc.phase.has_prices();
    // Pauschalsumme of the whole LV (TotalLSUM) replaces the sum of the
    // positions as the LV total.
    if priced {
        if let Some(lump) = boq.totals.as_ref().and_then(|totals| totals.lump_sum) {
            lv.set_lump_sum_total(Some(proava_domain::Money::from_decimal(lump)));
        }
    }
    if matches!(doc.phase, Phase::Dp84 | Phase::Dp85 | Phase::Dp86) {
        warnings.push(format!(
            "Die Datei ist Phase {} (Angebot/Auftrag); ihre Preise werden als Planerpreise übernommen.",
            doc.phase.number()
        ));
    }

    if !doc.award.contract_text.is_empty() {
        let text = RichText::new(
            doc.award
                .contract_text
                .iter()
                .map(|line| TextBlock::paragraph(vec![Run::plain(line.clone())]))
                .collect(),
        );
        let mut note = Note::rich(text);
        note.set_heading(crate::files::CONTRACT_HEADING);
        note.set_preliminary(true);
        lv.push_note(note);
        warnings.push(
            "Vertragliche Regelungen der Datei als erste Vorbemerkung übernommen.".to_string(),
        );
    }

    let mut preliminary_images = ImportedImages::default();
    for add_text in &doc.award.add_texts {
        let mut note = Note::rich(rich_from_gaeb(&add_text.detail, &mut preliminary_images));
        note.set_heading(one_line(&add_text.outline));
        note.set_preliminary(true);
        if !note.text().is_empty() || !note.heading().is_empty() {
            lv.push_note(note);
        }
    }

    let mut walker = Walker {
        lv: &mut lv,
        level_count: 0,
        priced,
        numbers: Vec::new(),
        found: Found::default(),
        alternative_groups: HashMap::new(),
        images: preliminary_images,
        performance_links: Vec::new(),
        repetition_links: Vec::new(),
        surcharge_links: Vec::new(),
        cost_catalog,
        work_catalog,
    };
    walker.level_count = walker.lv.level_count();
    walker.walk(&boq.body, &mut Vec::new());
    let numbers = std::mem::take(&mut walker.numbers);
    let mut found = std::mem::take(&mut walker.found);
    let mut images = std::mem::take(&mut walker.images);
    let links = std::mem::take(&mut walker.performance_links);
    let repetitions = std::mem::take(&mut walker.repetition_links);
    let surcharge_links = std::mem::take(&mut walker.surcharge_links);
    // Descriptions may stand after the positions that use them.
    for (position, number, ordinal) in links {
        match lv
            .performance_by_number(&number)
            .map(PerformanceDescription::id)
        {
            Some(performance) => lv
                .link_performance(position, Some(performance))
                .expect("both rows exist"),
            None => found.unknown_performance.push(format!(
                "{ordinal} → {}",
                proava_domain::performance_mark(&number)
            )),
        }
    }
    // Zuschläge on listed positions (MarkupSubQty), by ordinal.
    for (position, entries, ordinal) in surcharge_links {
        let mut references = Vec::new();
        for (base, quantity) in entries {
            match lv.position_by_ordinal(&base).map(Position::id) {
                Some(id) if id != position => references.push(SurchargeReference {
                    position: id,
                    quantity,
                }),
                _ => found.surcharges.push(format!("{ordinal} → {base}")),
            }
        }
        if let Some(row) = lv
            .index_of(position)
            .and_then(|index| lv.row_mut(index))
            .and_then(LvRow::as_position_mut)
        {
            if let Some(mut surcharge) = row.surcharge().cloned() {
                surcharge.references = references;
                row.set_surcharge(Some(surcharge));
            }
        }
    }
    for (position, base, ordinal) in repetitions {
        match lv.position_by_ordinal(&base).map(Position::id) {
            Some(base) if base != position => lv
                .link_repetition(position, Some(base))
                .expect("both positions exist"),
            _ => found.references.push(format!("{ordinal} → {base}")),
        }
    }
    lv.set_position_step(detect_step(&numbers));
    // Schlussbemerkungen (AddText after the LV, e.g. a „Schlussblatt“) stay
    // where they stand in the file: at the end.
    for add_text in &doc.award.closing_texts {
        let mut note = Note::rich(rich_from_gaeb(&add_text.detail, &mut images));
        note.set_heading(one_line(&add_text.outline));
        note.set_closing(true);
        if !note.text().is_empty() || !note.heading().is_empty() {
            lv.push_note(note);
        }
    }
    warnings.extend(found.into_warnings());

    let project_name = name.as_str().to_string();
    let mut project = Project::new(name, created_iso, lv);
    if !images.images.is_empty() {
        warnings.push(format!(
            "{} aus den Langtexten übernommen.",
            proava_domain::wording::count(images.images.len(), "Bild", "Bilder")
        ));
    }
    for (file, bytes) in images.images {
        let stored = project.add_image(&file, bytes);
        debug_assert_eq!(stored, file, "names are cleaned and unique already");
    }
    let info = project.info_mut();
    // Same as the project name: the export falls back to it anyway.
    let short_name = doc.project.name.trim();
    if short_name != project_name {
        info.short_name = short_name.to_string();
    }
    info.label = if doc.project.label.trim().is_empty() {
        doc.project.name.clone()
    } else {
        doc.project.label.clone()
    };
    info.award_number = doc.award.award_no.trim().to_string();
    // Datenaustauschnummer as in the file, also when it equals the
    // Vergabenummer (some programs write both): the export gives it back.
    info.exchange_number = doc.award.owner_dp_no.trim().to_string();
    if let Some(owner) = &doc.award.owner {
        info.owner = address_from_gaeb(owner);
    }
    // Bedarfsträger, Ausführungs-/Benachrichtigungsorte, Debitorennummer, Staat.
    crate::text::places_from_gaeb(&doc.award, info);
    let submission = &doc.award.submission;
    info.submission_place = submission.location.clone();
    let date = |iso: &Option<String>| format_deadline(iso.as_deref(), None);
    info.execution_start = date(&submission.construction_start);
    info.execution_end = date(&submission.construction_end);
    info.binding_period_end = date(&submission.award_deadline);
    info.award_procedure = procedure_label(&doc.award.category);
    info.submission_deadline = format_deadline(
        submission.open_date.as_deref(),
        submission.open_time.as_deref(),
    );
    // Taken over as delivered (e.g. an Ausführungsende before the Beginn);
    // the protocol and „LV prüfen“ say so.
    warnings.extend(project.info().problems_as_delivered());
    (project, warnings)
}

#[derive(Default)]
struct Found {
    too_deep: Vec<String>,
    alternative_titles: Vec<String>,
    kind_conflicts: Vec<String>,
    references: Vec<String>,
    unknown_performance: Vec<String>,
    renumbered_performance: Vec<String>,
    short_from_long: Vec<String>,
    odd_alternatives: Vec<String>,
    surcharges: Vec<String>,
}

impl Found {
    fn into_warnings(self) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut list = |entries: &[String], text: &str| {
            if !entries.is_empty() {
                warnings.push(format!("{text}: {}", short_list(entries, 10)));
            }
        };
        list(&self.too_deep, "Titel tiefer als die Gliederung der Datei");
        list(
            &self.alternative_titles,
            "Grund-/Wahlbereiche (Titel) als normale Titel übernommen – Wahltitel zählen jetzt in die Summe, bitte prüfen",
        );
        list(
            &self.surcharges,
            "Zuschlagspositionen mit Bezug auf unbekannte Positionen (ohne diesen Bezug übernommen)",
        );
        list(
            &self.kind_conflicts,
            "Positionen mit mehreren Positionsarten (nur eine übernommen: Wahl vor Bedarf)",
        );
        list(
            &self.references,
            "Wiederholungsbeschreibungen mit unbekannter Bezugsposition (ohne Bezug übernommen)",
        );
        list(
            &self.unknown_performance,
            "Bezug auf eine Ausführungsbeschreibung, die es in der Datei nicht gibt (ohne Bezug übernommen)",
        );
        list(
            &self.renumbered_performance,
            "Ausführungsbeschreibungen mit doppelter oder fehlender Nummer (neu nummeriert)",
        );
        list(
            &self.short_from_long,
            "Kurztext fehlte, erste Langtextzeile übernommen",
        );
        list(
            &self.odd_alternatives,
            "Wahlgruppe/-nummer nicht numerisch (fortlaufend nummeriert)",
        );
        warnings
    }
}

/// A Zuschlagsposition (row), its listed positions (ordinal in the file,
/// part quantity) and its own ordinal, linked once all rows are in.
type SurchargeLink = (RowId, Vec<(String, Option<Decimal>)>, String);

struct Walker<'a> {
    lv: &'a mut Lv,
    level_count: usize,
    priced: bool,
    numbers: Vec<u32>,
    found: Found,
    alternative_groups: HashMap<String, u32>,
    images: ImportedImages,
    /// Position, number of the Ausführungsbeschreibung, OZ (for warnings).
    performance_links: Vec<(RowId, String, String)>,
    /// Position, OZ of the repeated base position, own OZ.
    repetition_links: Vec<(RowId, String, String)>,
    /// Zuschlagsposition → listed positions (ordinal, part quantity), own ordinal.
    surcharge_links: Vec<SurchargeLink>,
    /// CtlgID of the DIN 276 catalog, if the file has one.
    cost_catalog: Option<String>,
    /// CtlgID of the Leistungsbereich catalog, if the file has one.
    work_catalog: Option<String>,
}

/// Code of the first assignment to the catalog `catalog`.
fn assigned_code(catalog: &str, assigns: &[proava_gaeb::CatalogAssign]) -> Option<String> {
    assigns
        .iter()
        .find(|assign| assign.catalog.trim() == catalog)
        .map(|assign| assign.code.trim().to_string())
        .filter(|code| !code.is_empty())
}

impl Walker<'_> {
    /// Code of the assignment to the DIN 276 catalog.
    fn cost_group(&self, assigns: &[proava_gaeb::CatalogAssign]) -> Option<String> {
        assigned_code(self.cost_catalog.as_deref()?, assigns)
    }

    /// Code of the assignment to the Leistungsbereich catalog.
    fn work_category(&self, assigns: &[proava_gaeb::CatalogAssign]) -> Option<String> {
        assigned_code(self.work_catalog.as_deref()?, assigns)
    }

    fn walk(&mut self, nodes: &[BoqNode], path: &mut Vec<String>) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.trim().to_string());
                    let ordinal = path.join(".");
                    if path.len() > self.level_count {
                        self.found.too_deep.push(ordinal.clone());
                    }
                    if category.alternative.is_some() {
                        self.found.alternative_titles.push(ordinal.clone());
                    }
                    let mut group =
                        LvGroup::new(path.len() - 1, ordinal, one_line(&category.label));
                    group.set_cost_group(self.cost_group(&category.catalog_assigns));
                    group.set_work_category(self.work_category(&category.catalog_assigns));
                    // Pauschalsumme of the title (TotalLSUM).
                    if self.priced {
                        group.set_lump_sum_total(
                            category
                                .totals
                                .as_ref()
                                .and_then(|totals| totals.lump_sum)
                                .map(proava_domain::Money::from_decimal),
                        );
                    }
                    self.lv.push_group(group);
                    self.walk(&category.body, path);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    let mut position = self.position(item, path);
                    position.set_cost_group(self.cost_group(&item.catalog_assigns));
                    position.set_work_category(self.work_category(&item.catalog_assigns));
                    let ordinal = position.ordinal().to_string();
                    let id = self.lv.push_position(position);
                    if let Some(number) = &item.kind.reference_description {
                        self.performance_links.push((
                            id,
                            number.trim().to_string(),
                            ordinal.clone(),
                        ));
                    }
                    if let Some(surcharge) = item
                        .surcharge
                        .as_ref()
                        .filter(|surcharge| !surcharge.items.is_empty())
                    {
                        self.surcharge_links.push((
                            id,
                            surcharge
                                .items
                                .iter()
                                .map(|entry| (entry.ordinal.trim().to_string(), entry.sub_qty))
                                .collect(),
                            ordinal.clone(),
                        ));
                    }
                    if let Some(base) = &item.kind.repetition_of {
                        self.repetition_links
                            .push((id, base.trim().to_string(), ordinal));
                    }
                }
                BoqNode::Remark(remark) => {
                    let mut note = Note::rich(rich_from_gaeb(&remark.text, &mut self.images));
                    note.set_heading(one_line(&remark.short_text));
                    crate::stlb::note_from_remark(remark, &mut note);
                    self.lv.push_note(note);
                }
                BoqNode::PerfDescr(description) => {
                    let performance = self.performance(description);
                    let wanted = performance.number().to_string();
                    let id = self.lv.push_row(LvRow::Performance(performance));
                    let number = self
                        .lv
                        .performance_by_id(id)
                        .map(|p| p.number().to_string());
                    if number.as_deref() != Some(wanted.as_str()) {
                        self.found
                            .renumbered_performance
                            .push(format!("„{wanted}“ → {}", number.unwrap_or_default()));
                    }
                }
            }
        }
    }

    fn position(&mut self, item: &Item, path: &[String]) -> Position {
        let mut pieces = path.to_vec();
        pieces.push(item.rno_part.trim().to_string());
        if !item.rno_index.trim().is_empty() {
            pieces.push(item.rno_index.trim().to_string());
        }
        let ordinal = pieces.join(".");
        if let Ok(number) = item.rno_part.trim().parse::<u32>() {
            self.numbers.push(number);
        }

        let mut short_text = item.short_text.trim().to_string();
        if short_text.is_empty() {
            if let Some(line) = item
                .long_text
                .to_plain()
                .lines()
                .find(|line| !line.trim().is_empty())
            {
                short_text = line.trim().to_string();
                self.found.short_from_long.push(ordinal.clone());
            }
        }
        let unit_price = if self.priced { item.unit_price } else { None };
        let mut position = Position::new(
            ordinal.clone(),
            short_text,
            item.qty.unwrap_or(Decimal::ZERO),
            item.unit.trim(),
            unit_price,
        );
        position.set_long_text(rich_from_gaeb(&item.long_text, &mut self.images));
        let sub_descriptions = item
            .sub_descriptions
            .iter()
            .map(|sub| SubDescription {
                number: sub.number.trim().to_string(),
                short_text: sub.short_text.clone(),
                long_text: rich_from_gaeb(&sub.long_text, &mut self.images),
                quantity: sub.qty,
                unit: sub.unit.trim().to_string(),
                catalog: crate::stlb::catalog_from_gaeb(
                    sub.stlb.as_ref(),
                    sub.standard_number.as_ref(),
                ),
            })
            .collect();
        position.set_sub_descriptions(sub_descriptions);
        if self.priced {
            position.set_price_components(item.up_components.clone());
        }
        position.set_time_quantity(item.time_quantity);
        position.set_lump_sum(item.kind.lump_sum);
        position.set_hourly(item.kind.hourly);
        position.set_kind(self.kind(item, &ordinal));

        position.set_quantity_open(item.qty_to_be_determined);
        position.set_price_breakdown(item.up_breakdown);
        position.set_not_applicable(item.not_applicable);
        position.set_key_item(item.key_item);
        position.set_complements(
            item.complements
                .iter()
                .map(|complement| TextComplement {
                    marker: complement.marker.trim().to_string(),
                    kind: if complement.kind == "Owner" {
                        ComplementKind::Owner
                    } else {
                        ComplementKind::Bidder
                    },
                    caption: complement.caption.clone(),
                    body: complement.body.clone(),
                    tail: complement.tail.clone(),
                    anchor: complement.anchor.map(|anchor| proava_domain::TextAnchor {
                        block: anchor.block,
                        offset: anchor.offset,
                        own_line: anchor.own_line,
                    }),
                })
                .collect(),
        );
        crate::stlb::keys_from_gaeb(item, &mut position);
        if proava_gaeb::xml::is_surcharge(item) {
            // Zuschlagsposition: percentage in the unit price field, the kind
            // from MarkupType (GAEB 90: on all positions before); listed
            // positions are linked once all rows are in.
            let kind = item
                .surcharge
                .as_ref()
                .map(|surcharge| SurchargeKind::from_gaeb(&surcharge.markup_type))
                .unwrap_or_default();
            position.set_surcharge(Some(Surcharge {
                split: false,
                kind,
                references: Vec::new(),
            }));
            position.set_unit_price(if self.priced {
                item.markup_percent
            } else {
                None
            });
            position.set_price_components(Vec::new());
        } else {
            position.set_marked_for_surcharge(item.kind.markup);
        }
        position
    }

    fn kind(&mut self, item: &Item, ordinal: &str) -> PositionKind {
        let kind = &item.kind;
        let count = [kind.alternative.is_some(), kind.provisional.is_some()]
            .into_iter()
            .filter(|set| *set)
            .count();
        if count > 1 {
            self.found.kind_conflicts.push(ordinal.to_string());
        }
        if let Some(alternative) = &kind.alternative {
            let group = self.number(&alternative.group, ordinal);
            let serial = self.number(&alternative.serial, ordinal);
            return if serial == 0 {
                PositionKind::Base { group }
            } else {
                PositionKind::Alternative { group, serial }
            };
        }
        if let Some(provisional) = kind.provisional {
            return PositionKind::Provisional {
                with_total: provisional == Provisional::WithTotal,
            };
        }
        PositionKind::Normal
    }

    fn performance(&mut self, description: &PerfDescr) -> PerformanceDescription {
        // GAEB writes the number as an integer; "01" and "1" are the same.
        let raw = description.number.trim();
        let number = match raw.parse::<u32>() {
            Ok(number) => number.to_string(),
            Err(_) => raw.to_string(),
        };
        let mut performance = PerformanceDescription::new(
            number,
            one_line(&description.label),
            rich_from_gaeb(&description.long_text, &mut self.images),
        );
        performance.set_short_text(one_line(&description.short_text));
        let (complements, keys) = crate::stlb::text_from_gaeb(
            &description.complements,
            description.stlb.as_ref(),
            description.standard_number.as_ref(),
        );
        performance.set_complements(complements);
        performance.set_catalog(keys);
        performance
    }

    fn number(&mut self, text: &str, ordinal: &str) -> u32 {
        if let Ok(number) = text.trim().parse::<u32>() {
            return number;
        }
        self.found.odd_alternatives.push(ordinal.to_string());
        let next = 1000 + self.alternative_groups.len() as u32;
        *self
            .alternative_groups
            .entry(text.trim().to_string())
            .or_insert(next)
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
