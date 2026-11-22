use proava_domain::{
    ComparisonLine, Discount, LvRow, Position, PositionKind, PriceComparison, Project, RichText,
    TextBlock,
};
use proava_print::{
    self as print, LvPrintMode, PcItem, PcPosition, PcTitle, PrintBidder, PrintHeader, PrintImage,
    PrintItem, PrintLv, PrintPosition, PrintPriceComparison, PrintTitle, TextRun,
};

use proava_application::ComparisonPrint;

use crate::text::format_deadline;

/// Domain → print model for the Angebotsaufforderung (`Blank`, no prices)
/// or the bepreistes LV (`Priced`). Title and LV sums are the domain's.
/// `date` is printed as given (TT.MM.JJJJ).
pub fn lv_to_print(project: &Project, mode: LvPrintMode, date: &str) -> PrintLv {
    let lv = project.lv();
    let priced = mode == LvPrintMode::Priced;
    let group_totals = lv.group_totals();
    // Kostengruppe and Leistungsbereich that apply (own or from the title).
    let cost_groups = lv.effective_cost_groups();
    let work_categories = lv.effective_work_categories();
    // Zuschläge and Aufteilungszeilen with their Rechengrundlage.
    let computation = lv.split_computation();
    let surcharges = &computation.values;
    let mut tree = Tree::<PrintTitle, PrintItem>::default();
    let mut group_number = 0;

    for (index, row) in lv.rows().iter().enumerate() {
        match row {
            LvRow::Group(group) => {
                tree.open(
                    group.level(),
                    PrintTitle {
                        lump: priced && group.lump_sum_total().is_some(),
                        oz: group.ordinal().to_string(),
                        label: lv.level_label(group.level()),
                        text: group.short_text().to_string(),
                        total: priced.then(|| group_totals[group_number].total.to_decimal()),
                        items: Vec::new(),
                    },
                );
                group_number += 1;
            }
            LvRow::Position(position) => {
                let mut long_text = Vec::new();
                // Catalogue line first (GAEB DA XML 3.3, 4.6.10), e.g. „STLB-Bau 4/2026 070 TA“.
                if let Some(line) = position.catalog_line() {
                    long_text.push(print::TextBlock::paragraph(vec![TextRun::plain(line)]));
                }
                // Under the short text, also in the Kurztext-LV (a bidder must
                // see that an Ausführungsbeschreibung applies).
                let mut remarks = Vec::new();
                if position.is_not_applicable() {
                    remarks.push("Position entfällt".to_string());
                }
                if let Some(performance) = lv.performance_of(position) {
                    remarks.push(format!("Ausführung nach {}", performance.heading()));
                }
                if let Some(base) = position.repeats().and_then(|id| lv.position_by_id(id)) {
                    remarks.push(format!("Wiederholung von Position {}", base.ordinal()));
                }
                if let Some(remark) = surcharge_remark(lv, position) {
                    remarks.push(remark);
                }
                // Textergänzungen mit Platz im Text stehen dort, die anderen danach.
                let with_places = proava_domain::complement_text::with_complements(
                    position.long_text(),
                    position.complements(),
                    |complement| proava_domain::Run::plain(complement.print_text_gap()),
                );
                long_text.extend(blocks(project, &with_places));
                for sub in position.sub_descriptions() {
                    let mut heading = format!("Unterbeschreibung {}", sub.number);
                    if !sub.short_text.trim().is_empty() {
                        heading.push_str(&format!(": {}", sub.short_text.trim()));
                    }
                    // Quantity 0 = none given (STLB-Bau Online writes 0.000 for a
                    // new Unterbeschreibung); no „– 0“ in the print.
                    if let Some(quantity) = sub.quantity.filter(|quantity| !quantity.is_zero()) {
                        heading.push_str(&format!(
                            " – {} {}",
                            proava_domain::format_decimal_de(
                                quantity,
                                quantity.normalize().scale()
                            ),
                            sub.unit
                        ));
                    }
                    long_text.push(print::TextBlock::paragraph(vec![TextRun::bold(heading)]));
                    if let Some(line) = sub.catalog.print_line() {
                        long_text.push(print::TextBlock::paragraph(vec![TextRun::plain(line)]));
                    }
                    long_text.extend(blocks(project, &sub.long_text));
                }
                for complement in position.complements().iter().filter(|c| c.anchor.is_none()) {
                    long_text.push(print::TextBlock::paragraph(entry_runs(complement)));
                }
                let surcharge = surcharges.get(&position.id()).map(|value| {
                    Box::new(print::PrintSurcharge {
                        base: priced.then(|| value.base.to_decimal()),
                        percent: if priced { position.unit_price() } else { None },
                    })
                });
                let total_price = match surcharges.get(&position.id()) {
                    Some(value) => (priced && position.unit_price().is_some())
                        .then(|| value.amount.to_decimal()),
                    None => (priced && position.unit_price().is_some())
                        .then(|| position.total_price().to_decimal()),
                };
                tree.push(PrintItem::Position(PrintPosition {
                    oz: position.ordinal().to_string(),
                    short_text: position.short_text().to_string(),
                    takeoff: takeoff_lines(position),
                    long_text,
                    quantity: (!position.is_surcharge()).then(|| position.quantity()),
                    unit: position.unit().to_string(),
                    unit_price: if priced && !position.is_surcharge() {
                        position.unit_price()
                    } else {
                        None
                    },
                    total_price,
                    surcharge,
                    kinds: kinds(position, computation.bases.contains(&position.id())),
                    // Rechengrundlage: its GP in brackets, the Aufteilung carries it.
                    in_total: position.counts_in_total()
                        && !computation.bases.contains(&position.id()),
                    remarks,
                    cost_group: cost_groups
                        .get(index)
                        .cloned()
                        .flatten()
                        .map(|(code, _)| code),
                    work_category: work_categories
                        .get(index)
                        .cloned()
                        .flatten()
                        .map(|(code, _)| code),
                    // EP-Aufgliederung: labels of the LV, values only when priced.
                    price_parts: if position.has_price_breakdown() {
                        lv.price_components()
                            .iter()
                            .enumerate()
                            .map(|(index, component)| {
                                let value = priced
                                    .then(|| {
                                        position.price_components().get(index).copied().flatten()
                                    })
                                    .flatten();
                                (component.label.trim().to_string(), value)
                            })
                            .filter(|(label, _)| !label.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    },
                }));
            }
            // An empty Hinweis prints nothing – no mark either.
            LvRow::Note(note)
                if note.is_blank()
                    && note.complements().is_empty()
                    && note.catalog_line().is_none() => {}
            LvRow::Note(note) => {
                let mut text = Vec::new();
                if !note.heading().trim().is_empty() {
                    text.push(print::TextBlock::paragraph(vec![TextRun::bold(
                        note.heading().trim(),
                    )]));
                }
                if let Some(line) = note.catalog_line() {
                    text.push(print::TextBlock::paragraph(vec![TextRun::plain(line)]));
                }
                let with_places = proava_domain::complement_text::with_complements(
                    note.text(),
                    note.complements(),
                    |complement| proava_domain::Run::plain(complement.print_text_gap()),
                );
                text.extend(blocks(project, &with_places));
                for complement in note.complements().iter().filter(|c| c.anchor.is_none()) {
                    text.push(print::TextBlock::paragraph(entry_runs(complement)));
                }
                tree.push(if note.is_preliminary() {
                    PrintItem::Preliminary(text)
                } else if note.is_closing() {
                    PrintItem::Closing(text)
                } else {
                    PrintItem::Note(text)
                });
            }
            LvRow::Performance(performance) => {
                // The OZ column carries „A0003“ and „Ausführungsbeschr.“,
                // the text column the label (bold), then catalogue line and text.
                let mut text = Vec::new();
                let title = performance.title();
                if !title.is_empty() {
                    text.push(print::TextBlock::paragraph(vec![TextRun::bold(title)]));
                }
                if let Some(line) = performance.catalog().print_line() {
                    text.push(print::TextBlock::paragraph(vec![TextRun::plain(line)]));
                }
                let with_places = proava_domain::complement_text::with_complements(
                    performance.long_text(),
                    performance.complements(),
                    |complement| proava_domain::Run::plain(complement.print_text_gap()),
                );
                text.extend(blocks(project, &with_places));
                for complement in performance
                    .complements()
                    .iter()
                    .filter(|c| c.anchor.is_none())
                {
                    text.push(print::TextBlock::paragraph(entry_runs(complement)));
                }
                // Same mark as on screen (PerformanceDescription::mark, „A0003“).
                tree.push(PrintItem::Performance(performance.mark(), text));
            }
            // Abstandshalter are only for editing: no print (Anwender 18.11.).
            LvRow::Spacer(_) => {}
        }
    }

    PrintLv {
        header: header(project, date),
        items: tree.finish(),
        net_total: priced.then(|| lv.net_total().to_decimal()),
        vat_percent: lv.vat_percent(),
        parties: Vec::new(),
        discount: None,
        cover: None,
        lump_sum: priced
            .then(|| lv.lump_sum_total().map(|lump| lump.to_decimal()))
            .flatten(),
    }
}

/// Domain → print model for the Preisspiegel: all bids that are not
/// excluded, in project order, with their discount; sums from
/// [`PriceComparison::build`].
pub fn price_comparison_to_print(project: &Project, date: &str) -> PrintPriceComparison {
    price_comparison_to_print_with(project, date, &ComparisonPrint::default())
}

/// Price comparison as set up on screen: variant options, bid order and
/// selection, anonymous names, headings only.
pub fn price_comparison_to_print_with(
    project: &Project,
    date: &str,
    print: &ComparisonPrint,
) -> PrintPriceComparison {
    let lv = project.lv();
    let comparison = PriceComparison::build(lv, project.bids(), print.options);
    let bids: Vec<_> = project
        .bids()
        .iter()
        .filter(|bid| !bid.is_excluded())
        .collect();

    let selected: Vec<usize> = if print.bids.is_empty() {
        (0..comparison.bids.len()).collect()
    } else {
        print
            .bids
            .iter()
            .filter_map(|id| {
                comparison
                    .bids
                    .iter()
                    .position(|summary| summary.bid == *id)
            })
            .collect()
    };
    let anonymous = !print.names.is_empty() && print.names.len() == selected.len();
    // Positions whose GP the bidder stated differently from quantity × EP („!“).
    let errors: Vec<std::collections::HashSet<proava_domain::RowId>> = selected
        .iter()
        .map(|&index| {
            bids[index]
                .arithmetic_errors(lv)
                .into_iter()
                .map(|(position, _, _)| position.id())
                .collect()
        })
        .collect();
    // Without planner prices the column would only show „–“ and 0,00.
    let with_estimate = print.options.with_estimate && lv.has_planner_prices();

    let bidders = selected
        .iter()
        .enumerate()
        .map(|(column, &index)| {
            let summary = &comparison.bids[index];
            let bid = bids[index];
            let (discount_percent, discount_amount) = match bid.discount() {
                Discount::None => (None, None),
                Discount::Percent(percent) => (Some(percent), None),
                Discount::Amount(_) => (None, Some(summary.discount.to_decimal())),
            };
            PrintBidder {
                name: if anonymous {
                    print.names[column].clone()
                } else {
                    summary.name.clone()
                },
                note: String::new(),
                // Anonymous: no date either (it could name the bidder).
                date: if anonymous {
                    String::new()
                } else {
                    bid.bid_date()
                        .map(|date| format_deadline(Some(date), None))
                        .unwrap_or_default()
                },
                net_total: Some(summary.net.to_decimal()),
                discount_percent,
                discount_amount,
                after_discount_at_lv_quantities: (summary.quantity_changes > 0)
                    .then(|| summary.after_discount_at_lv_quantities.to_decimal()),
                // The Abweichungsprotokoll names unknown OZ itself.
                unknown_ordinals: if bid.structure().foreign.is_empty() {
                    bid.unknown_ordinals().to_vec()
                } else {
                    Vec::new()
                },
                structure: {
                    let absent = bid.absent_positions(lv);
                    bid.structure()
                        .summary(absent.len())
                        .map(|summary| print::PcStructure {
                            summary,
                            sum_note: bid.total_breakdown(lv).map(|breakdown| breakdown.text()),
                            lines: bid
                                .structure()
                                .lines(lv, &absent)
                                .iter()
                                .map(proava_domain::DeviationLine::text)
                                .collect(),
                        })
                },
                arithmetic_errors: bid
                    .arithmetic_errors(lv)
                    .into_iter()
                    .map(|(position, stated, computed)| print::PcArithmeticError {
                        oz: position.ordinal().to_string(),
                        stated: stated.to_decimal(),
                        computed: computed.to_decimal(),
                    })
                    .collect(),
            }
        })
        .collect();

    let mut tree = Tree::<PcTitle, PcItem>::default();
    for line in &comparison.lines {
        match line {
            ComparisonLine::Group(group) => tree.open(
                group.level,
                PcTitle {
                    oz: group.ordinal.clone(),
                    text: group.short_text.clone(),
                    totals: selected
                        .iter()
                        .map(|&index| Some(group.totals[index].to_decimal()))
                        .collect(),
                    estimate: with_estimate.then(|| group.estimate_total.to_decimal()),
                    items: Vec::new(),
                },
            ),
            ComparisonLine::Position(_) if print.totals_only => {}
            ComparisonLine::Position(line) => {
                let position = lv.position_by_id(line.row);
                // Zuschlagsposition: the prices are percentages („%“ as unit).
                let is_surcharge = position.is_some_and(Position::is_surcharge);
                tree.push(PcItem::Position(PcPosition {
                    oz: line.ordinal.clone(),
                    short_text: line.short_text.clone(),
                    quantity: (!is_surcharge).then_some(line.quantity),
                    unit: if is_surcharge {
                        "%".to_string()
                    } else {
                        line.unit.clone()
                    },
                    kinds: position
                        .map(|position| kinds(position, false))
                        .unwrap_or_default(),
                    in_total: print.options.counts(line.kind),
                    prices: selected
                        .iter()
                        .enumerate()
                        .map(|(column, &index)| {
                            let cell = &line.cells[index];
                            print::BidPrice {
                                unit_price: cell.unit_price,
                                total_price: cell.unit_price.map(|_| cell.total.to_decimal()),
                                outlier: cell.outlier,
                                error: errors[column].contains(&line.row),
                            }
                        })
                        .collect(),
                    estimate: if with_estimate {
                        print::BidPrice {
                            unit_price: line.estimate_unit_price,
                            total_price: line
                                .estimate_unit_price
                                .map(|_| line.estimate_total.to_decimal()),
                            ..print::BidPrice::default()
                        }
                    } else {
                        print::BidPrice::default()
                    },
                    quantity_note: line
                        .quantity_change
                        .map(|change| change.hint(&line.unit))
                        .unwrap_or_default(),
                }));
            }
        }
    }

    let terms = &project.info().terms;
    let cash_discount_percent = terms
        .cash_discount_percent
        .filter(|percent| !percent.is_zero());
    PrintPriceComparison {
        header: header(project, date),
        bidders,
        items: tree.finish(),
        vat_percent: lv.vat_percent(),
        estimate: with_estimate,
        estimate_net: with_estimate.then(|| comparison.estimate_net.to_decimal()),
        outlier_note: format!(
            "EP weicht mehr als {} % vom {} ab",
            proava_domain::format_decimal_de(print.options.outlier_percent, 0),
            match print.options.reference {
                proava_domain::DeviationReference::Mean => "Mittelwert",
                proava_domain::DeviationReference::Lowest => "günstigsten EP",
                proava_domain::DeviationReference::Estimate => "Planerpreis",
            }
        ),
        cash_discount_percent,
        cash_discount_note: terms
            .cash_discount_days
            .filter(|_| cash_discount_percent.is_some())
            .map(|days| format!("bei Zahlung innerhalb von {days} Tagen"))
            .unwrap_or_default(),
        ..PrintPriceComparison::default()
    }
}

/// Kosten nach Kostengruppen for print (excluded bids left out).
pub fn cost_groups_to_print(project: &Project, date: &str) -> print::PrintCostGroups {
    let lv = project.lv();
    let report = proava_domain::cost_group_report(lv, project.bids());
    print::PrintCostGroups {
        header: PrintHeader {
            document_kind: Some("Kosten nach Kostengruppen".into()),
            ..header(project, date)
        },
        catalog: lv.cost_catalog().name.clone(),
        bidders: report.bidders.clone(),
        lines: report
            .lines
            .iter()
            .map(|line| print::PrintCostGroupLine {
                code: line.code.clone().unwrap_or_default(),
                label: line.label.clone(),
                subtotal: line.subtotal,
                positions: line.positions,
                planner: line.planner.to_decimal(),
                bidders: line
                    .bidders
                    .iter()
                    .map(|value| value.to_decimal())
                    .collect(),
            })
            .collect(),
        planner_total: report.planner_total.to_decimal(),
        bidder_totals: report
            .bidder_totals
            .iter()
            .map(|value| value.to_decimal())
            .collect(),
    }
}

/// Kopf-/Fußzeile for a printout of `project`: the project's own frame or
/// `default` (from the settings), with the values of all placeholders.
/// `extra` adds values of this printout, e.g. („datei“, „LV_480.pdf“).
pub fn page_frame_to_print(
    project: &Project,
    default: &proava_domain::PageFrame,
    date: &str,
    extra: &[(&str, String)],
) -> print::PrintFrame {
    let frame = project
        .info()
        .page_frame
        .clone()
        .unwrap_or_else(|| default.clone())
        .normalized();
    let info = project.info();
    let lv = project.lv();
    let mut values: Vec<(String, String)> = [
        ("projekt", project.name().as_str().to_string()),
        ("projektnummer", info.short_name.trim().to_string()),
        ("projektbezeichnung", info.label.trim().to_string()),
        (
            "baumassnahme",
            info.description
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string(),
        ),
        ("lv", lv.title().trim().to_string()),
        ("lv_nr", lv.number().trim().to_string()),
        ("lv_bezeichnung", lv.title().trim().to_string()),
        ("datum", date.to_string()),
        ("stand", date.to_string()),
        ("planer", info.planner.name1.trim().to_string()),
        ("bearbeiter", info.planner.name1.trim().to_string()),
        ("ersteller", info.planner.name1.trim().to_string()),
        ("bauherr", info.owner.name1.trim().to_string()),
        ("auftraggeber", info.client().name1.trim().to_string()),
        ("vergabenummer", info.award_number.trim().to_string()),
        ("datei", format!("{}.proava", project.name().as_str())),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    // Empty extras keep the project value (e.g. no Bearbeiter in the settings).
    for (key, value) in extra.iter().filter(|(key, value)| {
        !value.trim().is_empty() || !matches!(*key, "ersteller" | "bearbeiter")
    }) {
        values.retain(|(known, _)| known != key);
        values.push((key.to_string(), value.clone()));
    }
    print::PrintFrame {
        header: frame.header,
        footer: frame.footer,
        header_rule: frame.header_rule,
        footer_rule: frame.footer_rule,
        logo: match frame.logo {
            proava_domain::FrameLogoPlace::Off => print::FrameLogo::Off,
            proava_domain::FrameLogoPlace::Left => print::FrameLogo::Left,
            proava_domain::FrameLogoPlace::Right => print::FrameLogo::Right,
        },
        font_size_pt: f32::from(frame.font_size_pt),
        values,
    }
}

pub(crate) fn header(project: &Project, date: &str) -> PrintHeader {
    let info = project.info();
    PrintHeader {
        project_name: project.name().as_str().to_string(),
        project_title: info.label.clone(),
        client: info.client().one_line(),
        lv_name: project.lv().title().to_string(),
        date: date.to_string(),
        author: info.planner.name1.clone(),
        document_kind: None,
        logo: None,
        frame: Default::default(),
        look: Default::default(),
        currency: proava_domain::price_check::currency_symbol(project.lv().currency()),
    }
}

/// „Zuschlag auf die Positionen davor“, „Zuschlag auf 01.0010 (4 m), 01.0020“.
pub(crate) fn surcharge_remark(lv: &proava_domain::Lv, position: &Position) -> Option<String> {
    let surcharge = position.surcharge()?;
    Some(match surcharge.kind {
        proava_domain::SurchargeKind::Listed => {
            let listed: Vec<String> = surcharge
                .references
                .iter()
                .filter_map(|reference| {
                    let item = lv.position_by_id(reference.position)?;
                    Some(match reference.quantity {
                        Some(quantity) => format!(
                            "{} ({} {})",
                            item.ordinal(),
                            proava_domain::format_decimal_de(
                                quantity,
                                quantity.normalize().scale()
                            ),
                            item.unit()
                        ),
                        None => item.ordinal().to_string(),
                    })
                })
                .collect();
            if listed.is_empty() {
                "Zuschlag auf die genannten Positionen (keine angegeben)".to_string()
            } else {
                format!("Zuschlag auf {}", listed.join(", "))
            }
        }
        kind => format!("Zuschlag {}", kind.label()),
    })
}

fn kinds(position: &Position, basis: bool) -> Vec<print::PositionKind> {
    let mut kinds = Vec::new();
    match position.surcharge() {
        Some(surcharge) if surcharge.split => {
            kinds.push(print::PositionKind::Other("Aufteilung nach KG".to_string()))
        }
        Some(_) => kinds.push(print::PositionKind::Other("Zuschlagsposition".to_string())),
        None => {}
    }
    if basis {
        kinds.push(print::PositionKind::Other(
            "Rechengrundlage, aufgeteilt".to_string(),
        ));
    }
    if position.is_marked_for_surcharge() {
        kinds.push(print::PositionKind::Other("zu bezuschlagen".to_string()));
    }
    match position.kind() {
        PositionKind::Normal => {}
        PositionKind::Provisional { with_total: false } => {
            kinds.push(print::PositionKind::Optional)
        }
        PositionKind::Provisional { with_total: true } => kinds.push(print::PositionKind::Other(
            "Bedarfsposition mit GB".to_string(),
        )),
        PositionKind::Base { group } => kinds.push(print::PositionKind::Other(format!(
            "Grundposition (Gruppe {group})"
        ))),
        PositionKind::Alternative { group, serial } => kinds.push(print::PositionKind::Other(
            format!("Wahlposition {serial} zu Gruppe {group}"),
        )),
        PositionKind::Supplement => kinds.push(print::PositionKind::Surcharge),
    }
    if position.is_lump_sum() {
        kinds.push(print::PositionKind::LumpSum);
    }
    if position.is_hourly() {
        kinds.push(print::PositionKind::Hourly);
    }
    if position.is_stlb_placeholder() {
        kinds.push(print::PositionKind::Other("STLB-Platzhalter".to_string()));
    }
    kinds
}

/// Long text with images taken from the project; list items with their
/// level, numbered ones with their number („1.“, „a)“, „i.“ by level).
pub(crate) fn blocks(project: &Project, text: &RichText) -> Vec<print::TextBlock> {
    let markers = text.list_markers();
    text.blocks()
        .iter()
        .zip(markers)
        .map(|(block, marker)| match block {
            TextBlock::Paragraph { runs, align } => print::TextBlock::Paragraph {
                runs: print_runs(runs),
                align: print_align(*align),
            },
            TextBlock::ListItem {
                ordered,
                level,
                runs,
                align,
            } => print::TextBlock::ListItem {
                level: *level,
                marker: marker.filter(|_| *ordered),
                runs: print_runs(runs),
                align: print_align(*align),
            },
            TextBlock::Table(table) => print::TextBlock::Table {
                header_rows: table.header_rows,
                rows: table
                    .rows
                    .iter()
                    .map(|cells| {
                        cells
                            .iter()
                            .map(|cell| print::TableCell {
                                runs: print_runs(&cell.runs),
                                align: print_align(cell.align),
                            })
                            .collect()
                    })
                    .collect(),
                widths_mm: table.column_widths_mm.clone(),
            },
            TextBlock::Image { file, width_mm } => match project.image(file) {
                Some(image) => print::TextBlock::Image(PrintImage {
                    data: image.bytes.clone(),
                    file_name: image.file.clone(),
                    width_mm: *width_mm,
                    caption: None,
                    clip: image.clip.as_ref().map(|clip| print::PrintClip {
                        page: clip.page,
                        page_width: clip.page_width,
                        page_height: clip.page_height,
                        area: crate::clip::area_of(&clip.rect),
                    }),
                }),
                None => print::TextBlock::text(format!("[Bild fehlt: {file}]")),
            },
        })
        .collect()
}

/// The LV body prints at 9 pt where the editor shows the standard 10 pt;
/// chosen sizes keep that proportion.
const PRINT_SCALE: f32 = 0.9;

fn print_runs(runs: &[proava_domain::Run]) -> Vec<TextRun> {
    runs.iter()
        .flat_map(|run| {
            let format = TextRun {
                text: String::new(),
                bold: run.bold,
                italic: run.italic,
                underline: run.underline,
                font: run
                    .font
                    .as_ref()
                    .map(|_| run.font_class().liberation_name().to_string()),
                size_pt: run
                    .size_pt
                    .map(|size| ((size * PRINT_SCALE) * 10.0).round() / 10.0),
                ..TextRun::default()
            };
            with_gaps(&run.text, &format)
        })
        .collect()
}

/// The runs of `text` in `format`; every [`proava_domain::PRINT_GAP`]
/// becomes a gap run of 52 dots (one piece in the PDF, not underlined).
fn with_gaps(text: &str, format: &TextRun) -> Vec<TextRun> {
    if !text.contains(proava_domain::PRINT_GAP) {
        return vec![TextRun {
            text: text.to_string(),
            ..format.clone()
        }];
    }
    let mut out = Vec::new();
    for (index, piece) in text.split(proava_domain::PRINT_GAP).enumerate() {
        if index > 0 {
            out.push(TextRun {
                text: ".".repeat(proava_domain::PRINT_GAP_DOTS),
                underline: false,
                gap: true,
                ..format.clone()
            });
        }
        if !piece.is_empty() {
            out.push(TextRun {
                text: piece.to_string(),
                ..format.clone()
            });
        }
    }
    out
}

/// A Textergänzung on a line of its own (after the long text): its runs,
/// marked so that the Kurztext-LV keeps them.
fn entry_runs(complement: &proava_domain::TextComplement) -> Vec<TextRun> {
    let format = TextRun {
        entry: true,
        ..TextRun::default()
    };
    with_gaps(complement.print_text_gap().trim_end(), &format)
}

fn print_align(align: proava_domain::Align) -> print::TextAlign {
    match align {
        proava_domain::Align::Left => print::TextAlign::Left,
        proava_domain::Align::Center => print::TextAlign::Center,
        proava_domain::Align::Right => print::TextAlign::Right,
        proava_domain::Align::Justify => print::TextAlign::Justify,
    }
}

/// Nests flat rows with levels into titles.
trait Node: Sized {
    type Item;
    fn wrap(self) -> Self::Item;
    fn items(&mut self) -> &mut Vec<Self::Item>;
}

impl Node for PrintTitle {
    type Item = PrintItem;
    fn wrap(self) -> PrintItem {
        PrintItem::Title(self)
    }
    fn items(&mut self) -> &mut Vec<PrintItem> {
        &mut self.items
    }
}

impl Node for PcTitle {
    type Item = PcItem;
    fn wrap(self) -> PcItem {
        PcItem::Title(self)
    }
    fn items(&mut self) -> &mut Vec<PcItem> {
        &mut self.items
    }
}

struct Tree<T: Node<Item = I>, I> {
    root: Vec<I>,
    open: Vec<(usize, T)>,
}

impl<T: Node<Item = I>, I> Default for Tree<T, I> {
    fn default() -> Self {
        Self {
            root: Vec::new(),
            open: Vec::new(),
        }
    }
}

impl<T: Node<Item = I>, I> Tree<T, I> {
    fn close_while(&mut self, mut close: impl FnMut(usize) -> bool) {
        while self.open.last().is_some_and(|(level, _)| close(*level)) {
            let Some((_, title)) = self.open.pop() else {
                break;
            };
            self.push(title.wrap());
        }
    }

    fn open(&mut self, level: usize, title: T) {
        self.close_while(|open_level| open_level >= level);
        self.open.push((level, title));
    }

    fn push(&mut self, item: I) {
        match self.open.last_mut() {
            Some((_, title)) => title.items().push(item),
            None => self.root.push(item),
        }
    }

    fn finish(mut self) -> Vec<I> {
        self.close_while(|_| true);
        self.root
    }
}

/// Änderungsliste for the print crate.
pub fn change_list_to_print(
    project: &Project,
    against: &str,
    diff: &proava_domain::lv_diff::LvDiff,
    date: &str,
) -> print::PrintChangeList {
    use proava_domain::lv_diff::{Aspect, ChangeKind};
    print::PrintChangeList {
        header: PrintHeader {
            document_kind: Some("Änderungsliste".to_string()),
            ..header(project, date)
        },
        landscape: false,
        against: against.to_string(),
        summary: diff.summary(),
        entries: diff
            .changes
            .iter()
            .map(|change| print::PrintChange {
                kind: change.kind.label().to_string(),
                row_kind: change.row_kind.label().to_string(),
                ordinal: change.ordinal().to_string(),
                old_ordinal: if change.kind == ChangeKind::Moved {
                    change.old_ordinal.clone()
                } else {
                    String::new()
                },
                text: change.text.clone(),
                details: change
                    .details
                    .iter()
                    .filter(|detail| detail.aspect != Aspect::Ordinal)
                    .map(|detail| detail.line())
                    .collect(),
            })
            .collect(),
    }
}

/// Mengenermittlung of a position for the printout (the print option decides
/// whether it is shown, see `print_job`).
fn takeoff_lines(position: &proava_domain::Position) -> Vec<print::PrintTakeoffLine> {
    position
        .takeoff()
        .iter()
        .map(|line| print::PrintTakeoffLine {
            label: line.label.trim().to_string(),
            formula: line.formula.trim().to_string(),
            result: line.result().ok(),
        })
        .collect()
}
