use std::path::Path;

use proava_application::{
    BidImport, CatalogPositions, ComparisonPrint, ContractExport, CsvEncoding, ExchangeFiles,
    GaebExport, GaebExportOptions, GaebRuleFinding, GaebRuleSeverity, LvImport, PdfDocument,
    PdfLogo, PrintJob, PrintKind, PrintOutcome,
};
use proava_domain::{BidId, Project, ProjectName};
use proava_gaeb::GaebDocument;

use proava_print::{render_lv_pdf, render_price_comparison_pdf, LvPrintMode};

use crate::{
    contract_to_gaeb, contract_to_print, gaeb_to_project, lv_to_gaeb,
    price_comparison_to_print_with, render_print,
};

/// Heading of the note holding the contract text of a GAEB 90 file.
pub(crate) const CONTRACT_HEADING: &str = "Vertragliche Regelungen";

/// GAEB file format, chosen by the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaebFormat {
    /// GAEB DA XML 3.x: .X81 … .X86 (also .xml).
    Xml,
    /// GAEB 90: .D81 … .D86.
    Datan,
}

impl GaebFormat {
    pub fn of(file: &Path) -> Result<Self, String> {
        let extension = file
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let numbered = |prefix: char| {
            extension.len() == 3
                && extension.starts_with(prefix)
                && extension[1..]
                    .chars()
                    .all(|character| character.is_ascii_digit())
        };
        if extension == "xml" || numbered('x') {
            Ok(GaebFormat::Xml)
        } else if numbered('d') {
            Ok(GaebFormat::Datan)
        } else if numbered('p') {
            Err("GAEB 2000 (.P8x) wird nicht unterstützt – bitte als GAEB DA XML (.X8x) oder GAEB 90 (.D8x) anfordern.".to_string())
        } else {
            Err(format!(
                "„{}“ ist keine GAEB-Datei (erwartet .X8x oder .D8x).",
                file.display()
            ))
        }
    }
}

/// The real exchange adapter: GAEB DA XML, GAEB 90 and PDF.
#[derive(Debug, Clone, Copy, Default)]
pub struct GaebPdfExchange;

impl ExchangeFiles for GaebPdfExchange {
    fn export_gaeb(
        &self,
        project: &Project,
        kind: GaebExport,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        self.export_gaeb_with(project, kind, file, GaebExportOptions::default())
    }

    fn export_gaeb_with(
        &self,
        project: &Project,
        kind: GaebExport,
        file: &Path,
        options: GaebExportOptions,
    ) -> Result<Vec<String>, String> {
        let phase = crate::export_phase(kind);
        let (mut document, mut warnings) = lv_to_gaeb(project, phase);
        let format = GaebFormat::of(file)?;
        if format == GaebFormat::Datan {
            warnings.extend(datan_losses(project, false));
        }
        let (images, mut bytes) = project.lv_images();
        if images > 0 {
            // PDF clips go as PNG (300 dpi): their real size.
            if format == GaebFormat::Xml && options.images {
                bytes = crate::clip::gaeb_pictures(project)
                    .iter()
                    .map(|picture| picture.bytes)
                    .sum();
            }
            let size = proava_domain::size_text(bytes);
            let pictures = proava_domain::wording::count(images, "Bild", "Bilder");
            warnings.push(match (format, options.images) {
                (GaebFormat::Datan, _) => format!(
                    "GAEB 90 kann keine Bilder: {pictures} aus den Langtexten ({size}) fehlen in der Datei – mit GAEB DA XML (.{}) gehen sie mit.",
                    kind.extension()
                ),
                (GaebFormat::Xml, false) => format!(
                    "{pictures} aus den Langtexten ({size}) nicht mitgegeben – „Bilder mitgeben“ ist aus."
                ),
                (GaebFormat::Xml, true) => {
                    format!("{pictures} aus den Langtexten mitgegeben ({size}).")
                }
            });
        }
        document.date = Some(chrono::Local::now().format("%Y-%m-%d").to_string());
        let bytes = encode_with(format, &document, options.images)?;
        write_export(file, &bytes)?;
        // Fachregeln of the file just written; hints stood in the check before.
        warnings.extend(
            crate::rule_check::file_findings(format, &bytes, phase)
                .iter()
                .filter(|finding| finding.severity != crate::RuleSeverity::Hint)
                .map(crate::rule_line),
        );
        Ok(warnings)
    }

    fn check_gaeb_export(&self, project: &Project, kind: GaebExport) -> Vec<GaebRuleFinding> {
        crate::export_rule_findings(project, kind)
            .into_iter()
            .map(|finding| GaebRuleFinding {
                id: finding.id.to_string(),
                severity: match finding.severity {
                    crate::RuleSeverity::Error => GaebRuleSeverity::Error,
                    crate::RuleSeverity::Warning => GaebRuleSeverity::Warning,
                    crate::RuleSeverity::Hint => GaebRuleSeverity::Hint,
                },
                ordinal: finding.ordinal,
                text: finding.text,
                reference: finding.reference.to_string(),
            })
            .collect()
    }

    fn import_bid(&self, project: &Project, file: &Path) -> Result<BidImport, String> {
        self.import_bid_with(
            project,
            file,
            &proava_application::BidMatchOptions::default(),
        )
    }

    fn import_bid_with(
        &self,
        project: &Project,
        file: &Path,
        options: &proava_application::BidMatchOptions,
    ) -> Result<BidImport, String> {
        let (document, read_warnings) = read_gaeb(file)?;
        let mut import =
            crate::bid_match::match_bid_with(project, &document, &file_name(file), options);
        import.warnings.extend(
            crate::rule_check::bid_rule_findings(project, &document)
                .iter()
                .map(crate::rule_line),
        );
        let unnamed = document
            .award
            .bidder
            .as_ref()
            .map(|bidder| crate::bid_match::is_placeholder_name(&bidder.name1))
            .unwrap_or(true);
        if unnamed {
            let folder = file
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().into_owned());
            if let Some(name) = folder
                .as_deref()
                .and_then(crate::bid_match::name_from_folder)
            {
                import.warnings.push(format!(
                    "Bietername aus dem Ordner übernommen: „{name}“ – bitte prüfen."
                ));
                import.bid.bidder_mut().name1 = name;
            }
        }
        // A warning of the reader that starts with ACHTUNG (cut position numbers)
        // explains the many unknown positions: it goes to the front.
        let (urgent, rest): (Vec<String>, Vec<String>) = read_warnings
            .into_iter()
            .partition(|warning| warning.contains("ACHTUNG"));
        import.warnings.splice(0..0, urgent);
        import.warnings.extend(rest);
        Ok(import)
    }

    fn import_lv(
        &self,
        file: &Path,
        name: &ProjectName,
        created_iso: &str,
    ) -> Result<(Project, Vec<String>), String> {
        let (document, read_warnings) = read_gaeb(file)?;
        let (project, mut warnings) = gaeb_to_project(&document, name.clone(), created_iso);
        warnings.extend(read_warnings);
        Ok((project, warnings))
    }

    fn read_lv(
        &self,
        file: &Path,
        name: &ProjectName,
        created_iso: &str,
    ) -> Result<LvImport, String> {
        if crate::csv_import::is_csv(file) {
            let bytes = std::fs::read(file)
                .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", file.display()))?;
            let read = crate::csv_import::read_csv(&bytes, name.clone(), created_iso)?;
            return Ok(LvImport {
                project: read.project,
                warnings: read.warnings,
                phase: None,
                format: read.format,
                program: String::new(),
                columns: read.columns,
                facts: read.facts,
                currency: read.currency,
            });
        }
        let mut inner = None;
        let (format, bytes) = if is_zip(file) {
            let (entry, format, bytes) = gaeb_in_zip(file)?;
            inner = Some(entry);
            (format, bytes)
        } else {
            let format = GaebFormat::of(file)?;
            let bytes = std::fs::read(file)
                .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", file.display()))?;
            (format, bytes)
        };
        let (document, read_warnings, version) = decode_described(format, &bytes)?;
        let (project, mut warnings) = gaeb_to_project(&document, name.clone(), created_iso);
        warnings.extend(read_warnings);
        // From a zip: which file inside it (a fact, no finding).
        let facts = inner
            .map(|entry| vec![("Im Archiv".to_string(), entry)])
            .unwrap_or_default();
        Ok(LvImport {
            project,
            warnings,
            phase: Some(document.phase.number()),
            format: version,
            program: document.program.trim().to_string(),
            columns: Vec::new(),
            facts,
            currency: None,
        })
    }

    fn export_csv(
        &self,
        project: &Project,
        file: &Path,
        encoding: CsvEncoding,
    ) -> Result<Vec<String>, String> {
        let export = crate::csv_export::lv_to_csv(project.lv(), encoding);
        write_export(file, &export.bytes)?;
        Ok(export.notes)
    }

    fn export_tracking_csv(
        &self,
        report: &proava_domain::TrackReport,
        file: &Path,
        encoding: CsvEncoding,
    ) -> Result<Vec<String>, String> {
        let export = crate::tracking_csv::tracking_to_csv(report, encoding);
        write_export(file, &export.bytes)?;
        Ok(export.notes)
    }

    fn read_catalog_positions(&self, file: &Path) -> Result<CatalogPositions, String> {
        let mut source_in_archive = String::new();
        let (document, read_warnings) = if is_zip(file) {
            let (name, format, bytes) = gaeb_in_zip(file)?;
            source_in_archive = name;
            decode(format, &bytes)?
        } else {
            read_gaeb(file)?
        };
        let name = ProjectName::parse("Katalog").map_err(|error| error.to_string())?;
        let (project, _) = gaeb_to_project(&document, name, "");
        let mut read = crate::stlb::catalog_positions(&project, read_warnings);
        let delivered = crate::stlb::Delivered {
            program: &document.program,
            file_name: &crate::stlb::file_name_of(file, &source_in_archive),
            lv_names: [
                document.award.boq.label.as_str(),
                document.project.label.as_str(),
                document.award.boq.name.as_str(),
                document.project.name.as_str(),
            ],
            retrieved: &crate::stlb::modified_day(file),
        };
        crate::stlb::mark_origin(&mut read.positions, &delivered);
        read.source_in_archive = source_in_archive;
        Ok(read)
    }

    fn export_pdf(
        &self,
        project: &Project,
        document: PdfDocument,
        file: &Path,
    ) -> Result<(), String> {
        render_pdf(project, document, file)
    }

    fn export_print(
        &self,
        project: &Project,
        job: &PrintJob,
        file: &Path,
    ) -> Result<PrintOutcome, String> {
        write_print(project, job, file)
    }

    fn print_page_count(&self, project: &Project, job: &PrintJob) -> Result<usize, String> {
        render_print(project, job, &today()).map(|(_, outcome)| outcome.pages)
    }

    fn export_contract(
        &self,
        project: &Project,
        bid: BidId,
        kind: ContractExport,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        match kind {
            ContractExport::Pdf => {
                let today = chrono::Local::now().format("%d.%m.%Y").to_string();
                let (mut print, warnings) = contract_to_print(project, bid, &today)?;
                // Without print options: no Mengenermittlung (as the dialog's default).
                crate::print_job::strip_takeoff(&mut print.items);
                let bytes = render_lv_pdf(&print, LvPrintMode::Priced)
                    .map_err(|error| format!("{}: {error}", kind.label()))?;
                write_export(file, bytes)?;
                Ok(warnings)
            }
            ContractExport::Xml | ContractExport::Datan => {
                let (mut document, mut warnings) = contract_to_gaeb(project, bid)?;
                if kind == ContractExport::Datan {
                    warnings.extend(datan_losses(project, true));
                }
                document.date = Some(chrono::Local::now().format("%Y-%m-%d").to_string());
                let format = if kind == ContractExport::Xml {
                    GaebFormat::Xml
                } else {
                    GaebFormat::Datan
                };
                let bytes = encode(format, &document)?;
                write_export(file, bytes)?;
                Ok(warnings)
            }
        }
    }

    fn export_change_list_pdf(
        &self,
        project: &Project,
        against: &str,
        diff: &proava_domain::lv_diff::LvDiff,
        file: &Path,
    ) -> Result<(), String> {
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let list = crate::to_print::change_list_to_print(project, against, diff, &today);
        let bytes = proava_print::render_change_list_pdf(&list)
            .map_err(|error| format!("Änderungsliste: {error}"))?;
        write_export(file, bytes)
    }

    fn export_change_list_pdf_styled(
        &self,
        project: &Project,
        against: &str,
        diff: &proava_domain::lv_diff::LvDiff,
        logo: Option<&PdfLogo>,
        look: &proava_application::PrintLook,
        frame: &proava_domain::PageFrame,
        landscape: bool,
        file: &Path,
    ) -> Result<(), String> {
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let mut list = crate::to_print::change_list_to_print(project, against, diff, &today);
        list.landscape = landscape;
        list.header.frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name(file))]);
        list.header.logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        list.header.look = crate::print_job::print_look(look, None);
        let bytes = proava_print::render_change_list_pdf(&list)
            .map_err(|error| format!("Änderungsliste: {error}"))?;
        write_export(file, bytes)
    }

    fn export_price_comparison_pdf(
        &self,
        project: &Project,
        print: &ComparisonPrint,
        file: &Path,
    ) -> Result<(), String> {
        if project.bids().iter().all(|bid| bid.is_excluded()) {
            return Err("Für den Preisspiegel fehlt ein gewertetes Angebot.".to_string());
        }
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let bytes =
            render_price_comparison_pdf(&price_comparison_to_print_with(project, &today, print))
                .map_err(|error| format!("{}: {error}", PdfDocument::PriceComparison.label()))?;
        write_export(file, bytes)
    }

    fn export_letters_pdf(
        &self,
        project: &Project,
        kind: proava_domain::LetterKind,
        recipients: &[proava_domain::InviteeId],
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &proava_application::PrintLook,
        file: &Path,
    ) -> Result<usize, String> {
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let mut letters = crate::letters_to_print(project, kind, recipients, &today)?;
        letters.frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name(file))]);
        letters.logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        letters.look = crate::print_job::print_look(look, None);
        let (bytes, pages) = proava_print::render_letters_pdf(&letters)
            .map_err(|error| format!("{}: {error}", kind.label()))?;
        write_export(file, bytes)?;
        Ok(pages)
    }

    fn letters_pdf(
        &self,
        project: &Project,
        request: &proava_application::LetterRequest,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &proava_application::PrintLook,
        file_name: &str,
    ) -> Result<(Vec<u8>, usize), String> {
        use proava_application::LetterRequest;
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let mut letters = match request {
            LetterRequest::Bidders { kind, recipients } => {
                crate::letters_to_print(project, *kind, recipients, &today)?
            }
            LetterRequest::Release { invoice } => {
                crate::release_letter_to_print(project, *invoice, &today)?
            }
        };
        letters.frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name.to_string())]);
        letters.logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        letters.look = crate::print_job::print_look(look, None);
        proava_print::render_letters_pdf(&letters)
            .map_err(|error| format!("{}: {error}", request.kind().label()))
    }

    fn write_file(&self, file: &Path, bytes: &[u8]) -> Result<(), String> {
        write_export(file, bytes)
    }

    fn export_change_order_pdf(
        &self,
        project: &Project,
        print: proava_application::ChangeOrderPrint,
        logo: Option<&PdfLogo>,
        look: &proava_application::PrintLook,
        frame: &proava_domain::PageFrame,
        file: &Path,
    ) -> Result<usize, String> {
        use proava_application::ChangeOrderPrint;
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let kind = crate::change_order_print_kind(print);
        let frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name(file))]);
        let logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        // Style, accent colour and Blattrahmen from the settings.
        let look = crate::print_job::print_look(look, None);
        let (bytes, pages) = match print {
            ChangeOrderPrint::Check(id) => {
                let mut check = crate::change_order_check_to_print(project, id, &today)?;
                check.header.frame = frame;
                check.header.logo = logo;
                check.header.look = look;
                proava_print::render_change_order_check_pdf(&check)
            }
            ChangeOrderPrint::Overview => {
                let mut overview = crate::change_order_overview_to_print(project, &today);
                overview.header.frame = frame;
                overview.header.logo = logo;
                overview.header.look = look;
                proava_print::render_change_order_overview_pdf(&overview)
            }
        }
        .map_err(|error| format!("{kind}: {error}"))?;
        write_export(file, bytes)?;
        Ok(pages)
    }

    fn import_contract(
        &self,
        project: &Project,
        file: &Path,
    ) -> Result<proava_application::ContractImport, String> {
        let (document, read_warnings) = read_gaeb(file)?;
        let mut import = crate::contract_from_gaeb(project, &document, &file_name(file))?;
        import.warnings.extend(read_warnings);
        Ok(import)
    }

    fn import_invoice(
        &self,
        project: &Project,
        file: &Path,
    ) -> Result<proava_application::InvoiceImport, String> {
        let bytes = std::fs::read(file)
            .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", file.display()))?;
        let (billing, report) =
            proava_gaeb::xml::read_billing(&bytes).map_err(|error| error.to_string())?;
        let mut import = crate::invoice_from_gaeb(project, &billing, &file_name(file))?;
        import.warnings.extend(report.warnings);
        Ok(import)
    }

    fn export_release_letter_pdf(
        &self,
        project: &Project,
        invoice: Option<proava_domain::InvoiceId>,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        file: &Path,
    ) -> Result<usize, String> {
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let mut letters = crate::release_letter_to_print(project, invoice, &today)?;
        letters.frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name(file))]);
        letters.logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        let (bytes, pages) = proava_print::render_letters_pdf(&letters)
            .map_err(|error| format!("{}: {error}", proava_domain::LetterKind::Release.label()))?;
        std::fs::write(file, bytes)
            .map_err(|error| format!("„{}“ lässt sich nicht schreiben: {error}", file.display()))?;
        Ok(pages)
    }

    fn export_change_order_gaeb(
        &self,
        project: &Project,
        change_order: proava_domain::ChangeOrderId,
        kind: proava_application::ChangeOrderExport,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        let (mut document, mut warnings) =
            crate::change_order_to_gaeb(project, change_order, kind)?;
        document.date = Some(chrono::Local::now().format("%Y-%m-%d").to_string());
        let format = GaebFormat::of(file)?;
        let bytes = encode(format, &document)?;
        write_export(file, &bytes)?;
        // Fachregeln of the file just written, like the LV export.
        warnings.extend(
            crate::rule_check::file_findings(format, &bytes, document.phase)
                .iter()
                .filter(|finding| finding.severity != crate::RuleSeverity::Hint)
                .map(crate::rule_line),
        );
        Ok(warnings)
    }

    fn read_change_order_offer(
        &self,
        file: &Path,
    ) -> Result<(proava_domain::ChangeOrderOffer, Vec<String>), String> {
        let (document, mut warnings) = read_gaeb(file)?;
        let (offer, more) = crate::change_order_offer_from_gaeb(&document, &file_name(file));
        warnings.extend(more);
        Ok((offer, warnings))
    }

    fn export_submission_pdf(
        &self,
        project: &Project,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &proava_application::PrintLook,
        file: &Path,
    ) -> Result<(), String> {
        let today = chrono::Local::now().format("%d.%m.%Y").to_string();
        let mut submission = crate::submission_to_print(project, &today);
        submission.header.frame =
            crate::page_frame_to_print(project, frame, &today, &[("datei", file_name(file))]);
        submission.header.logo = logo
            .map(|logo| proava_print::PrintImage::new(logo.bytes.clone(), logo.file_name.clone()));
        submission.header.look = crate::print_job::print_look(look, None);
        let bytes = proava_print::render_submission_pdf(&submission)
            .map_err(|error| format!("Niederschrift: {error}"))?;
        write_export(file, bytes)
    }

    fn pdf_pages(&self, pdf: &[u8]) -> Result<Vec<(f32, f32)>, String> {
        proava_print::pdf_pages(pdf).map_err(|error| error.to_string())
    }

    fn pdf_single_page(&self, pdf: &[u8], page: u32) -> Result<Vec<u8>, String> {
        proava_print::pdf_single_page(pdf, page).map_err(|error| error.to_string())
    }

    fn render_pdf_clip(
        &self,
        pdf: &[u8],
        page: u32,
        rect: proava_domain::ClipRect,
        width_px: u32,
    ) -> Result<proava_application::Pixels, String> {
        crate::clip::render(pdf, page, rect, width_px)
    }

    fn gaeb_pictures(&self, project: &Project) -> Vec<proava_application::GaebPictureInfo> {
        crate::clip::gaeb_pictures(project)
    }

    fn gaeb_clip_png(
        &self,
        asset: &proava_domain::ImageAsset,
        width_mm: f32,
    ) -> Result<proava_application::GaebPicture, String> {
        crate::clip::gaeb_picture(asset, width_mm)
    }
}

/// Writes an exported file. A file another program holds open – on Windows
/// typically the last PDF in the PDF viewer – gets a hint what to do.
fn write_export(file: &Path, bytes: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(file, bytes).map_err(|error| export_error(file, &error, cfg!(windows)))
}

fn export_error(file: &Path, error: &std::io::Error, windows: bool) -> String {
    let mut text = format!("„{}“ lässt sich nicht schreiben: {error}", file.display());
    // Windows: sharing violation (32), lock violation (33).
    if windows && matches!(error.raw_os_error(), Some(32 | 33)) {
        text.push_str(
            " – Die Datei ist in einem anderen Programm geöffnet (z. B. in der PDF-Anzeige). \
             Dort schließen und noch einmal exportieren.",
        );
    }
    text
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string())
}

/// The document and the codec's own findings (prefixed with the line).
fn is_zip(file: &Path) -> bool {
    file.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

/// The first GAEB file (X8x/D8x) inside a zip archive, read in memory –
/// nothing is unpacked to disk.
fn gaeb_in_zip(file: &Path) -> Result<(String, GaebFormat, Vec<u8>), String> {
    use std::io::Read;
    let cannot = |error: &dyn std::fmt::Display| {
        format!(
            "„{}“ lässt sich nicht als ZIP lesen: {error}",
            file.display()
        )
    };
    let handle = std::fs::File::open(file).map_err(|error| cannot(&error))?;
    let mut archive = zip::ZipArchive::new(handle).map_err(|error| cannot(&error))?;
    let mut names: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.ok().map(|name| name.to_string()))
        .collect();
    names.sort();
    for name in names {
        let Ok(format) = GaebFormat::of(Path::new(&name)) else {
            continue;
        };
        let mut entry = archive.by_name(&name).map_err(|error| cannot(&error))?;
        if entry.size() > 200 * 1024 * 1024 {
            return Err(format!("„{name}“ im Archiv ist zu groß."));
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| cannot(&error))?;
        return Ok((name, format, bytes));
    }
    Err(format!(
        "Im Archiv „{}“ ist keine GAEB-Datei (.X8x/.D8x).",
        file.display()
    ))
}

/// What GAEB 90 cannot carry from the active LV, one warning per kind.
/// „Position entfällt“ only matters where it is written (contract, 86).
fn datan_losses(project: &Project, not_applicable: bool) -> Vec<String> {
    let ordinals = |keep: &dyn Fn(&proava_domain::Position) -> bool| -> Vec<String> {
        project
            .lv()
            .positions()
            .filter(|position| keep(position))
            .map(|position| position.ordinal().to_string())
            .collect()
    };
    let mut warnings = Vec::new();
    if not_applicable {
        let gone = ordinals(&|position| position.is_not_applicable());
        if !gone.is_empty() {
            warnings.push(format!(
                "GAEB 90 kennt „Position entfällt“ nicht – diese Positionen zählen dort mit: {}",
                gone.join(", ")
            ));
        }
    }
    let lump = ordinals(&|position| {
        position.is_lump_sum() && !position.unit().trim().eq_ignore_ascii_case("psch")
    });
    if !lump.is_empty() {
        warnings.push(format!(
            "GAEB 90 erkennt „pauschal“ nur an der Einheit „psch“ – bei diesen Positionen geht die Kennung verloren: {}",
            crate::text::short_list(&lump, 10)
        ));
    }
    warnings
}

fn read_gaeb(file: &Path) -> Result<(GaebDocument, Vec<String>), String> {
    let format = GaebFormat::of(file)?;
    let bytes = std::fs::read(file)
        .map_err(|error| format!("„{}“ lässt sich nicht lesen: {error}", file.display()))?;
    decode(format, &bytes)
}

fn decode(format: GaebFormat, bytes: &[u8]) -> Result<(GaebDocument, Vec<String>), String> {
    decode_described(format, bytes).map(|(document, warnings, _)| (document, warnings))
}

/// Like [`decode`], with the format as the user knows it („GAEB DA XML 3.2“).
fn decode_described(
    format: GaebFormat,
    bytes: &[u8],
) -> Result<(GaebDocument, Vec<String>, String), String> {
    match format {
        GaebFormat::Xml => {
            let (document, report) =
                proava_gaeb::xml::read_with_report(bytes).map_err(|error| error.to_string())?;
            let warnings = report
                .warnings
                .iter()
                .map(|warning| format!("GAEB XML: {warning}"))
                .collect();
            let version = report.version.trim();
            let label = if version.is_empty() {
                "GAEB DA XML".to_string()
            } else {
                format!("GAEB DA XML {version}")
            };
            Ok((document, warnings, label))
        }
        GaebFormat::Datan => {
            let report = proava_gaeb::datan::read(bytes).map_err(|error| error.to_string())?;
            let warnings = datan_warnings(&report.warnings);
            Ok((report.document, warnings, "GAEB 90".to_string()))
        }
    }
}

/// GAEB 90 reading notes: the same note on many records once, with its count
/// and the first lines (a file with 145 skipped records gave 145 lines).
pub(crate) fn datan_warnings(warnings: &[proava_gaeb::datan::Warning]) -> Vec<String> {
    let mut order: Vec<&str> = Vec::new();
    let mut lines: std::collections::HashMap<&str, Vec<usize>> = Default::default();
    for warning in warnings {
        let entry = lines.entry(warning.message.as_str()).or_default();
        if entry.is_empty() {
            order.push(warning.message.as_str());
        }
        entry.push(warning.line);
    }
    order
        .into_iter()
        .flat_map(|message| {
            let lines = &lines[message];
            if lines.len() <= 3 {
                lines
                    .iter()
                    .map(|line| format!("GAEB 90, Zeile {line}: {message}"))
                    .collect()
            } else {
                let first: Vec<String> = lines.iter().take(5).map(usize::to_string).collect();
                vec![format!(
                    "GAEB 90: {message} ({}×, Zeilen {} …)",
                    lines.len(),
                    first.join(", ")
                )]
            }
        })
        .collect()
}

fn encode(format: GaebFormat, document: &GaebDocument) -> Result<Vec<u8>, String> {
    encode_with(format, document, false)
}

/// Like [`encode`]; `images` writes the images of the long texts (DA XML only).
fn encode_with(
    format: GaebFormat,
    document: &GaebDocument,
    images: bool,
) -> Result<Vec<u8>, String> {
    match format {
        GaebFormat::Xml => proava_gaeb::xml::write_with(
            document,
            document.phase,
            &proava_gaeb::xml::WriteOptions {
                include_images: images,
            },
        )
        .map(String::into_bytes)
        .map_err(|error| error.to_string()),
        GaebFormat::Datan => {
            // GAEB 90 has no Vorbemerkungen: they go in front as hints.
            let mut document = document.clone();
            let mut texts = std::mem::take(&mut document.award.add_texts);
            // The contract text of a GAEB 90 file (T1) goes back there.
            if document.award.contract_text.is_empty() {
                if let Some(index) = texts
                    .iter()
                    .position(|text| text.outline.trim() == CONTRACT_HEADING)
                {
                    let text = texts.remove(index);
                    document.award.contract_text =
                        text.detail.to_plain().lines().map(str::to_string).collect();
                }
            }
            let remark = |text: proava_gaeb::AddText| {
                proava_gaeb::BoqNode::Remark(proava_gaeb::Remark {
                    short_text: text.outline,
                    text: text.detail,
                    ..proava_gaeb::Remark::default()
                })
            };
            // Schlussbemerkungen likewise as hints, at the end of the LV.
            let closing: Vec<_> = std::mem::take(&mut document.award.closing_texts)
                .into_iter()
                .map(remark)
                .collect();
            let body = &mut document.award.boq.body;
            let rest = std::mem::take(body);
            body.extend(texts.into_iter().map(remark));
            body.extend(rest);
            body.extend(closing);
            proava_gaeb::datan::write(&document).map_err(|error| error.to_string())
        }
    }
}

fn render_pdf(project: &Project, document: PdfDocument, file: &Path) -> Result<(), String> {
    let kind = match document {
        PdfDocument::RequestForQuotation => PrintKind::RequestForQuotation,
        PdfDocument::PricedLv => PrintKind::PricedLv,
        PdfDocument::PriceComparison => PrintKind::PriceComparison,
        PdfDocument::CostGroups => PrintKind::CostGroups,
    };
    write_print(project, &PrintJob::new(kind), file).map(|_| ())
}

fn today() -> String {
    chrono::Local::now().format("%d.%m.%Y").to_string()
}

fn write_print(project: &Project, job: &PrintJob, file: &Path) -> Result<PrintOutcome, String> {
    let (bytes, outcome) = render_print(project, job, &today())?;
    write_export(file, bytes)?;
    Ok(outcome)
}

#[cfg(test)]
mod write_tests {
    use std::path::Path;

    use super::export_error;

    #[test]
    fn a_file_held_open_on_windows_gets_a_hint() {
        let file = Path::new("Preisspiegel.pdf");
        // Raw code 32 = ERROR_SHARING_VIOLATION on Windows.
        let held = std::io::Error::from_raw_os_error(32);
        assert!(export_error(file, &held, true).contains("in einem anderen Programm geöffnet"));
        assert!(!export_error(file, &held, false).contains("anderen Programm"));
        let other = std::io::Error::from_raw_os_error(2);
        assert!(!export_error(file, &other, true).contains("anderen Programm"));
        assert!(export_error(file, &other, true)
            .starts_with("„Preisspiegel.pdf“ lässt sich nicht schreiben: "));
    }
}
