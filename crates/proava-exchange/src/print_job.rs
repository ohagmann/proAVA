//! Printouts of the print dialog: chooses the content (range, texts,
//! images, hints) on a copy of the project and the layout options, then
//! builds the matching print model.

use std::collections::HashSet;

use proava_application::{PrintJob, PrintKind, PrintOutcome};
use proava_domain::{LvRow, Project, RichText};
use proava_print::{
    render_lv_pdf_counted, render_price_comparison_pdf_counted, LvPrintMode, PrintOptions,
};

use crate::{
    bid_prices_to_print, contract_to_print, lv_to_print, price_comparison_to_print_with,
    project_cover,
};

/// Kopf-/Fußzeile of the job: the project's own one or the job's (settings).
fn frame(project: &Project, job: &PrintJob, date: &str) -> proava_print::PrintFrame {
    let file = if job.file_name.trim().is_empty() {
        format!("{}.pdf", project.name().as_str())
    } else {
        job.file_name.trim().to_string()
    };
    crate::page_frame_to_print(
        project,
        &job.frame,
        date,
        &[
            ("datei", file),
            // Bearbeiter from the settings (else the planner, see page_frame_to_print).
            ("ersteller", job.author.trim().to_string()),
            ("bearbeiter", job.author.trim().to_string()),
            ("kopftext", job.settings.header_text.trim().to_string()),
            ("fusstext", job.settings.footer_text.trim().to_string()),
        ],
    )
}

/// What every printout of `job` takes from the settings: logo, Kopf-/Fußzeile,
/// Druckstil and Bearbeiter.
fn dress(header: &mut proava_print::PrintHeader, project: &Project, job: &PrintJob, date: &str) {
    header.logo = logo(job);
    header.frame = frame(project, job, date);
    header.look = look(job);
    if !job.author.trim().is_empty() {
        header.author = job.author.trim().to_string();
    }
}

/// Druckstil of this kind (or the default from the settings) and accent colour.
fn look(job: &PrintJob) -> proava_print::PrintLook {
    let look = proava_application::PrintLook {
        // Blattrahmen of this kind (or the default from the settings).
        border: job.settings.border.unwrap_or(job.look.border),
        // Gitter of this kind (or the default from the settings).
        table_grid: job.settings.grid.unwrap_or(job.look.table_grid),
        ..job.look.clone()
    };
    print_look(&look, job.settings.style)
}

/// The settings' look for the print crate; `style` overrides the default.
pub(crate) fn print_look(
    look: &proava_application::PrintLook,
    style: Option<proava_application::PrintStyle>,
) -> proava_print::PrintLook {
    proava_print::PrintLook {
        style: match style.unwrap_or(look.style) {
            proava_application::PrintStyle::Classic => proava_print::PrintStyle::Classic,
            proava_application::PrintStyle::ProAva => proava_print::PrintStyle::ProAva,
        },
        accent: look.accent.clone(),
        border: match (look.border, look.border_width) {
            (false, _) => proava_print::PageBorder::None,
            (true, proava_application::BorderWidth::Thin) => proava_print::PageBorder::Thin,
            (true, proava_application::BorderWidth::Medium) => proava_print::PageBorder::Medium,
        },
        line_spacing_percent: (look.text.line.factor() * 100.0).round() as u16,
        table_grid: look.table_grid,
        paragraph_gap: match look.text.gap {
            proava_application::ParagraphGap::None => proava_print::ParagraphGap::Points(0),
            proava_application::ParagraphGap::Points3 => proava_print::ParagraphGap::Points(3),
            proava_application::ParagraphGap::Points6 => proava_print::ParagraphGap::Points(6),
            proava_application::ParagraphGap::HalfLine => proava_print::ParagraphGap::HalfLine,
        },
    }
}

fn logo(job: &PrintJob) -> Option<proava_print::PrintImage> {
    job.logo
        .as_ref()
        .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()))
}

/// Copy of the project with only what the job prints in the active LV:
/// the chosen titles (with the headings above them), with or without long
/// texts, images, hints/Vorbemerkungen and Ausführungsbeschreibungen.
pub fn print_selection(project: &Project, job: &PrintJob) -> Result<Project, String> {
    let settings = &job.settings;
    let mut copy = project.clone();
    let lv = copy.lv_mut();

    if !job.titles.is_empty() {
        let mut keep: HashSet<usize> = HashSet::new();
        let mut found = 0;
        for id in &job.titles {
            let Some(index) = lv
                .index_of(*id)
                .filter(|index| lv.rows()[*index].is_group())
            else {
                continue;
            };
            found += 1;
            keep.extend(lv.block_range(index));
            // Headings above, so the title sits at its place in the outline.
            let mut level = lv.rows()[index]
                .as_group()
                .map(|group| group.level())
                .unwrap_or(0);
            for above in (0..index).rev() {
                if let Some(group) = lv.rows()[above].as_group() {
                    if group.level() < level {
                        keep.insert(above);
                        level = group.level();
                    }
                }
            }
        }
        if found == 0 {
            return Err("Die gewählten Titel gibt es im LV nicht (mehr).".to_string());
        }
        for index in (0..lv.rows().len()).rev() {
            if !keep.contains(&index) {
                lv.remove_row(index);
            }
        }
    }

    for index in (0..lv.rows().len()).rev() {
        let drop = match &lv.rows()[index] {
            LvRow::Note(_) => !settings.notes,
            LvRow::Performance(_) => !settings.performance,
            _ => false,
        };
        if drop {
            lv.remove_row(index);
            continue;
        }
        let Some(row) = lv.row_mut(index) else {
            continue;
        };
        let strip = |text: &RichText| {
            if settings.images {
                text.clone()
            } else {
                text.without_images()
            }
        };
        match row {
            LvRow::Position(position) => {
                if settings.long_texts {
                    let text = strip(position.long_text());
                    position.set_long_text(text);
                    let mut subs = position.sub_descriptions().to_vec();
                    for sub in &mut subs {
                        sub.long_text = strip(&sub.long_text);
                    }
                    position.set_sub_descriptions(subs);
                } else {
                    // Unterbeschreibungen and the Auftraggeber's Textergänzungen
                    // belong to the long text; the Bieterangaben stay as lines
                    // under the short text (the bidder fills them in).
                    position.set_long_text(RichText::default());
                    position.set_sub_descriptions(Vec::new());
                    let entries = position
                        .complements()
                        .iter()
                        .filter(|complement| {
                            complement.kind == proava_domain::ComplementKind::Bidder
                        })
                        .map(|complement| proava_domain::TextComplement {
                            anchor: None,
                            ..complement.clone()
                        })
                        .collect();
                    position.set_complements(entries);
                }
            }
            LvRow::Note(note) => {
                let text = strip(note.text());
                note.set_text(text);
            }
            LvRow::Performance(performance) => {
                let text = strip(performance.long_text());
                performance.set_long_text(text);
            }
            // Abstandshalter are left out by lv_to_print.
            LvRow::Group(_) | LvRow::Spacer(_) => {}
        }
    }
    Ok(copy)
}

fn options(job: &PrintJob) -> PrintOptions {
    let settings = &job.settings;
    PrintOptions {
        landscape: settings.landscape,
        font_size_pt: settings.font_size_pt,
        title_page_break: settings.title_page_break,
        summary: settings.summary,
        summary_only: job.kind == PrintKind::Summary,
        summary_share: settings.summary_share,
        header_text: settings.header_text.clone(),
        footer_text: settings.footer_text.clone(),
        contents: settings.contents && job.kind != PrintKind::Summary,
        carry: settings.carry && job.kind != PrintKind::Summary,
        bookmark_positions: settings.bookmark_positions,
    }
}

/// Kostengruppe/Leistungsbereich at the positions only when asked for.
pub(crate) fn keep_codes(
    items: &mut [proava_print::PrintItem],
    cost_group: bool,
    work_category: bool,
) {
    for item in items {
        match item {
            proava_print::PrintItem::Title(title) => {
                keep_codes(&mut title.items, cost_group, work_category)
            }
            proava_print::PrintItem::Position(position) => {
                if !cost_group {
                    position.cost_group = None;
                }
                if !work_category {
                    position.work_category = None;
                }
            }
            _ => {}
        }
    }
}

/// Without the option „mit Mengenermittlung“ (or for kinds without it) the
/// positions print no takeoff lines.
pub(crate) fn strip_takeoff(items: &mut [proava_print::PrintItem]) {
    for item in items {
        match item {
            proava_print::PrintItem::Title(title) => strip_takeoff(&mut title.items),
            proava_print::PrintItem::Position(position) => position.takeoff.clear(),
            _ => {}
        }
    }
}

pub(crate) fn strip_long_texts(items: &mut [proava_print::PrintItem]) {
    for item in items {
        match item {
            proava_print::PrintItem::Title(title) => strip_long_texts(&mut title.items),
            // Only the lines of the Bieterangaben stay (see print_selection).
            proava_print::PrintItem::Position(position) => position.long_text.retain(|block| {
                matches!(block, proava_print::TextBlock::Paragraph { runs, .. } if runs.iter().any(|run| run.entry))
            }),
            // Ausführungsbeschreibung: mark and heading stay (Kurztext-LV).
            proava_print::PrintItem::Performance(_, text) => text.truncate(1),
            _ => {}
        }
    }
}

/// Lays out the printout of `job`; `date` is printed as given (TT.MM.JJJJ).
pub fn render_print(
    project: &Project,
    job: &PrintJob,
    date: &str,
) -> Result<(Vec<u8>, PrintOutcome), String> {
    render_print_within(project, job, date, crate::memory::available_memory())
}

/// [`render_print`] with the memory the printout may use (None = unknown):
/// an LV printout that clearly needs more is refused with a message.
pub fn render_print_within(
    project: &Project,
    job: &PrintJob,
    date: &str,
    available: Option<u64>,
) -> Result<(Vec<u8>, PrintOutcome), String> {
    let error = |error: proava_print::PrintError| format!("{}: {error}", job.kind.label());
    if job.kind == PrintKind::PriceComparison {
        if project.bids().iter().all(|bid| bid.is_excluded()) {
            return Err("Für den Preisspiegel fehlt ein gewertetes Angebot.".to_string());
        }
        let mut model = price_comparison_to_print_with(project, date, &job.comparison);
        dress(&mut model.header, project, job, date);
        let (bytes, pages) = render_price_comparison_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::Submission {
        if project.invitees().is_empty() && project.bids().is_empty() {
            return Err("Für die Niederschrift fehlen Bieter oder Angebote.".to_string());
        }
        let mut model = crate::submission_to_print(project, date);
        dress(&mut model.header, project, job, date);
        model.header_text = job.settings.header_text.clone();
        model.footer_text = job.settings.footer_text.clone();
        let (bytes, pages) = proava_print::render_submission_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::BillingCheck {
        let mut model = crate::billing_check_to_print(project, job.invoice, date)?;
        model.header.logo = logo(job);
        model.header.frame = frame(project, job, date);
        model.header.look = look(job);
        model.header_text = job.settings.header_text.clone();
        model.footer_text = job.settings.footer_text.clone();
        let (bytes, pages) =
            proava_print::render_billing_check_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::Release {
        let mut model = crate::release_to_print(project, job.invoice, date)?;
        model.header.logo = logo(job);
        model.header.frame = frame(project, job, date);
        model.header.look = look(job);
        model.header_text = job.settings.header_text.clone();
        model.footer_text = job.settings.footer_text.clone();
        let (bytes, pages) = proava_print::render_release_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::CostEstimate {
        let options = proava_print::PrintCostOptions {
            cover: job.settings.cover,
            landscape: job.settings.landscape,
            chart: job.settings.chart,
            plants: job.settings.plants,
        };
        let mut model = crate::cost_estimate_to_print(
            project,
            job.cost_estimate,
            date,
            options,
            job.settings.cost_control,
        )?;
        let (stand, author) = (model.header.date.clone(), model.header.author.clone());
        dress(&mut model.header, project, job, date);
        // Stand and Bearbeiter of the Kostenermittlung stay in the title block.
        model.header.date = stand;
        if !author.trim().is_empty() {
            model.header.author = author;
        }
        let (bytes, pages) =
            proava_print::render_cost_estimate_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::CostTracking {
        let options = proava_print::PrintTrackOptions {
            cover: job.settings.cover,
            chart: job.settings.chart,
        };
        let mut model = crate::cost_tracking_to_print(project, &job.tracking, date, options);
        dress(&mut model.header, project, job, date);
        // All LVs of the project: no LV in the Kopf-/Fußzeile.
        for (key, value) in &mut model.header.frame.values {
            if matches!(key.as_str(), "lv" | "lv_nr" | "lv_bezeichnung") {
                value.clear();
            }
        }
        let (bytes, pages) =
            proava_print::render_cost_tracking_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::CostLv {
        let options = crate::CostLvPrintOptions {
            notes: job.settings.notes,
            long_texts: job.settings.long_texts,
            list: job.settings.summary,
            landscape: job.settings.landscape,
        };
        let mut model = crate::cost_lv_to_print(project, date, options)?;
        dress(&mut model.header, project, job, date);
        let (bytes, pages) = proava_print::render_cost_lv_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    if job.kind == PrintKind::CostGroups {
        let mut model = crate::cost_groups_to_print(project, date);
        dress(&mut model.header, project, job, date);
        let (bytes, pages) = proava_print::render_cost_groups_pdf_counted(&model).map_err(error)?;
        return Ok((
            bytes,
            PrintOutcome {
                pages,
                warnings: Vec::new(),
            },
        ));
    }

    let selection = print_selection(project, job)?;
    let (mut model, mode, warnings) = match job.kind {
        PrintKind::ChangeList
        | PrintKind::ChangeOrderCheck
        | PrintKind::ChangeOrderOverview
        | PrintKind::Letters => {
            return Err(format!(
                "{} wird aus seinem Fenster gedruckt.",
                job.kind.label()
            ));
        }
        PrintKind::RequestForQuotation => (
            lv_to_print(&selection, LvPrintMode::Blank, date),
            LvPrintMode::Blank,
            Vec::new(),
        ),
        PrintKind::PricedLv | PrintKind::ShortTextLv | PrintKind::Summary => {
            // Kurztext-LV also blank (EP/GP to fill in) when asked for.
            let mode = if job.kind == PrintKind::ShortTextLv && !job.settings.prices {
                LvPrintMode::Blank
            } else {
                LvPrintMode::Priced
            };
            let mut model = lv_to_print(&selection, mode, date);
            model.header.document_kind = match job.kind {
                PrintKind::ShortTextLv => Some("Kurztext-LV".to_string()),
                PrintKind::Summary => Some("Zusammenstellung".to_string()),
                // The LV with prices is the Kostenvoranschlag (the template's
                // default); a Kosten-LV keeps the name of its Stufe.
                _ => project
                    .lv()
                    .purpose()
                    .cost_stage()
                    .map(|stage| stage.label().to_string()),
            };
            (model, mode, Vec::new())
        }
        PrintKind::Contract => {
            let bid = job
                .bid
                .or_else(|| project.awarded_bid().map(|bid| bid.id()))
                .ok_or("Kein Angebot ist beauftragt – erst den Auftrag erteilen.")?;
            let (model, warnings) = contract_to_print(&selection, bid, date)?;
            (model, LvPrintMode::Priced, warnings)
        }
        PrintKind::BidPrices => {
            let bid = job.bid.ok_or("Bitte das Angebot wählen.")?;
            let (model, warnings) = bid_prices_to_print(&selection, bid, date)?;
            (model, LvPrintMode::Priced, warnings)
        }
        PrintKind::PriceComparison
        | PrintKind::Submission
        | PrintKind::CostGroups
        | PrintKind::BillingCheck
        | PrintKind::Release
        | PrintKind::CostEstimate
        | PrintKind::CostTracking
        | PrintKind::CostLv => unreachable!("handled above"),
    };
    dress(&mut model.header, project, job, date);
    if !job.settings.long_texts {
        // Only the headings: no catalogue line, no „gem. Ausführungsbeschreibung“ either.
        strip_long_texts(&mut model.items);
    }
    if !(job.settings.takeoff && job.kind.offers_takeoff()) {
        strip_takeoff(&mut model.items);
    }
    keep_codes(
        &mut model.items,
        job.settings.cost_groups,
        job.settings.work_categories,
    );
    if job.settings.cover {
        let style = match job.settings.cover_form {
            proava_application::CoverForm::Standard => proava_print::CoverStyle::Standard,
            proava_application::CoverForm::Letterhead => proava_print::CoverStyle::Letterhead,
            proava_application::CoverForm::Table => proava_print::CoverStyle::Table,
            proava_application::CoverForm::FieldTable => proava_print::CoverStyle::FieldTable,
        };
        let blocks = proava_print::CoverBlocks {
            logo: job.cover_blocks.logo,
            addresses: job.cover_blocks.addresses,
            facts: job.cover_blocks.facts,
            bidder: job.cover_blocks.bidder,
        };
        model.cover = Some(project_cover(&selection, style, blocks));
    }
    let options = options(job);
    // A printout that clearly does not fit into the free memory gets a
    // message instead of making the system swap or the program crash.
    crate::memory::memory_check(proava_print::estimate_lv_print(&model, &options), available)?;
    let (bytes, pages) = render_lv_pdf_counted(&model, mode, &options).map_err(error)?;
    Ok((bytes, PrintOutcome { pages, warnings }))
}

#[cfg(test)]
mod tests {
    use proava_application::{PrintJob, PrintKind};

    /// Druckoption Gitter: the Preisspiegel has it from the start, the
    /// others follow the settings, the choice in the dialog wins.
    #[test]
    fn the_grid_comes_from_the_kind_the_settings_and_the_dialog() {
        let comparison = PrintJob::new(PrintKind::PriceComparison);
        assert!(super::look(&comparison).table_grid);
        let mut lv = PrintJob::new(PrintKind::PricedLv);
        assert!(!super::look(&lv).table_grid);
        lv.look.table_grid = true;
        assert!(super::look(&lv).table_grid, "Vorgabe aus den Einstellungen");
        lv.settings.grid = Some(false);
        assert!(!super::look(&lv).table_grid, "Wahl im Druckdialog");
        let mut comparison = PrintJob::new(PrintKind::PriceComparison);
        comparison.settings.grid = Some(false);
        assert!(!super::look(&comparison).table_grid);
    }
}
