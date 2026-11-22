use std::path::Path;

use proava_domain::{
    Bid, BidId, ChangeOrderId, ChangeOrderOffer, ComparisonOptions, Project, ProjectName,
};

/// Which letters of the Schriftverkehr to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LetterRequest {
    /// Letters of `kind` to invited bidders of the active LV, in this order.
    Bidders {
        kind: proava_domain::LetterKind,
        recipients: Vec<proava_domain::InviteeId>,
    },
    /// The letter to the client about the release of `invoice` (None = the
    /// last one) of the active LV.
    Release {
        invoice: Option<proava_domain::InvoiceId>,
    },
}

impl LetterRequest {
    pub fn kind(&self) -> proava_domain::LetterKind {
        match self {
            LetterRequest::Bidders { kind, .. } => *kind,
            LetterRequest::Release { .. } => proava_domain::LetterKind::Release,
        }
    }
}

/// Printouts of the Nachträge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOrderPrint {
    /// Prüfung Nachtragsangebot of one change order of the active LV.
    Check(ChangeOrderId),
    /// Nachtragsübersicht of the active LV.
    Overview,
}

impl ChangeOrderPrint {
    pub fn label(self) -> &'static str {
        match self {
            ChangeOrderPrint::Check(_) => "Prüfung Nachtragsangebot (PDF)",
            ChangeOrderPrint::Overview => "Nachtragsübersicht (PDF)",
        }
    }

    /// Word for the file name.
    pub fn file_label(self) -> &'static str {
        match self {
            ChangeOrderPrint::Check(_) => "Pruefung_Nachtrag",
            ChangeOrderPrint::Overview => "Nachtragsuebersicht",
        }
    }
}

/// GAEB form of a Nachtrags-LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOrderExport {
    /// X83 – Nachtragsangebotsaufforderung (positions without prices).
    Request,
    /// X86 – Nachtragsauftrag (checked prices, status per position).
    Order,
}

impl ChangeOrderExport {
    pub fn extension(self) -> &'static str {
        match self {
            ChangeOrderExport::Request => "X83",
            ChangeOrderExport::Order => "X86",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ChangeOrderExport::Request => "Nachtrags-LV als X83 (Angebotsaufforderung)",
            ChangeOrderExport::Order => "Nachtrags-LV als X86 (Nachtragsauftrag)",
        }
    }
}

/// Which GAEB file to write for the LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaebExport {
    /// X81 – Leistungsbeschreibung (ohne Preise).
    Description,
    /// X83 – Angebotsaufforderung (Mengen, keine Preise).
    RequestForQuotation,
    /// X82 – bepreistes LV mit Planerpreisen (GAEB: Kostenanschlag; nach DIN 276
    /// der Kostenvoranschlag).
    PricedLv,
}

/// Choices for writing a GAEB file (Exportdialog, MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaebExportOptions {
    /// Bilder der Langtexte mitgeben – nur GAEB DA XML (nie X84); GAEB 90
    /// kann keine Bilder.
    pub images: bool,
}

impl Default for GaebExportOptions {
    fn default() -> Self {
        Self { images: true }
    }
}

impl GaebExport {
    pub fn extension(self) -> &'static str {
        match self {
            GaebExport::Description => "X81",
            GaebExport::RequestForQuotation => "X83",
            GaebExport::PricedLv => "X82",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GaebExport::Description => "Leistungsbeschreibung (X81)",
            GaebExport::RequestForQuotation => "Angebotsaufforderung (X83)",
            GaebExport::PricedLv => "Bepreistes LV (X82, GAEB: Kostenanschlag)",
        }
    }
}

/// Encoding of a CSV export: UTF-8 with BOM (Excel and LibreOffice
/// recognise it) or Windows-1252 for older programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CsvEncoding {
    #[default]
    Utf8Bom,
    Windows1252,
}

impl CsvEncoding {
    pub fn label(self) -> &'static str {
        match self {
            CsvEncoding::Utf8Bom => "UTF-8 (Excel, LibreOffice)",
            CsvEncoding::Windows1252 => "Windows-1252 (ältere Programme)",
        }
    }
}

/// Which PDF to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfDocument {
    /// LV ohne Preise mit Feldern zum Ausfüllen.
    RequestForQuotation,
    /// LV mit Planerpreisen und Summen.
    PricedLv,
    /// Preisspiegel aller Angebote.
    PriceComparison,
    /// Kosten nach Kostengruppen (DIN 276), Planer und Bieter.
    CostGroups,
}

impl PdfDocument {
    /// The kind of the print dialog that prints this document.
    pub fn print_kind(self) -> crate::PrintKind {
        match self {
            PdfDocument::RequestForQuotation => crate::PrintKind::RequestForQuotation,
            PdfDocument::PricedLv => crate::PrintKind::PricedLv,
            PdfDocument::PriceComparison => crate::PrintKind::PriceComparison,
            PdfDocument::CostGroups => crate::PrintKind::CostGroups,
        }
    }

    pub fn label(self) -> &'static str {
        self.print_kind().label()
    }
}

/// Form of the Auftrags-LV (LV with the prices of the awarded bid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractExport {
    /// GAEB DA XML, phase 86 (Auftragserteilung).
    Xml,
    /// GAEB 90, phase 86.
    Datan,
    Pdf,
}

impl ContractExport {
    pub fn extension(self) -> &'static str {
        match self {
            ContractExport::Xml => "X86",
            ContractExport::Datan => "D86",
            ContractExport::Pdf => "pdf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ContractExport::Xml => "Auftrags-LV GAEB XML (X86)",
            ContractExport::Datan => "Auftrags-LV GAEB 90 (D86)",
            ContractExport::Pdf => "Auftrags-LV (PDF)",
        }
    }
}

/// Image for the title block of every printout, e.g. the planner's logo
/// (Extras ▸ Einstellungen ▸ Drucken); travels with the [`crate::PrintJob`].
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLogo {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

/// Severity of a GAEB-Fachregel finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaebRuleSeverity {
    /// A rule of the GAEB Fachdokumentation is violated.
    Error,
    Warning,
    Hint,
}

/// One finding of the GAEB-Fachregeln R1–R13 (docs/FACHLICHES.md)
/// on a file proAVA would write.
#[derive(Debug, Clone, PartialEq)]
pub struct GaebRuleFinding {
    /// Single check, e.g. "R9.unit"; the part before the dot is the rule.
    pub id: String,
    pub severity: GaebRuleSeverity,
    /// OZ of the title or position concerned, as written in the file.
    pub ordinal: Option<String>,
    pub text: String,
    /// Where the rule stands, e.g. "FD 3.3 11.2.3 Item 7".
    pub reference: String,
}

impl GaebRuleFinding {
    /// "R9" for "R9.unit".
    pub fn rule(&self) -> &str {
        self.id.split('.').next().unwrap_or(&self.id)
    }
}

/// A bid read from X84/D84, already matched against the project's LV.
#[derive(Debug, Clone)]
pub struct BidImport {
    pub bid: Bid,
    /// Findings for the user: missing positions, unknown OZ, changed
    /// quantities, arithmetic differences …
    pub warnings: Vec<String>,
}

/// How bid positions are matched with the LV (Anwender 20.11.).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BidMatchOptions {
    /// Take over the sure proposals of the structure comparison (order,
    /// quantities, texts) – else they are only proposed.
    pub by_structure: bool,
    /// By hand: OZ in the file → OZ in the LV.
    pub manual: Vec<(String, String)>,
}

/// Abrechnung: a contract read from X86/D86 (Auftrags-LV), matched with the
/// LV; quantities and prices are those of the file.
#[derive(Debug, Clone)]
pub struct ContractImport {
    pub contract: proava_domain::Contract,
    pub warnings: Vec<String>,
}

/// Abrechnung: an invoice (X89/X89B) or a quantity takeoff (X31) read from
/// GAEB and matched with the LV.
#[derive(Debug, Clone)]
pub struct InvoiceImport {
    /// „X89“, „X89B“ or „X31“.
    pub format: String,
    /// X89/X89B: the invoice, not yet in the project; X31: None.
    pub invoice: Option<proava_domain::Invoice>,
    /// Takeoff lines per position. X89: meant for the new invoice (Zuwachs
    /// already worked out for cumulative invoices); X31: for the invoice the
    /// user chooses.
    pub takeoff: Vec<(proava_domain::RowId, Vec<proava_domain::TakeoffLine>)>,
    pub warnings: Vec<String>,
}

/// An LV read from a GAEB file into a project in memory (see
/// [`ExchangeFiles::read_lv`]), with what the file says about itself.
#[derive(Debug, Clone)]
pub struct LvImport {
    pub project: Project,
    /// Findings of the codec and of the conversion, for the user.
    pub warnings: Vec<String>,
    /// Exchange phase of the file (81 … 86), when known.
    pub phase: Option<u8>,
    /// „GAEB DA XML 3.2“, „GAEB 90“; empty when unknown.
    pub format: String,
    /// Program that wrote the file; empty when not given.
    pub program: String,
    /// CSV: the columns as recognised, in the order of the file (empty for GAEB).
    pub columns: Vec<String>,
    /// CSV: the currency named in the file (code), if any.
    pub currency: Option<String>,
    /// Further facts of the file for the protocol, one each (CSV: Trenner,
    /// Zeichensatz); empty for GAEB.
    pub facts: Vec<(String, String)>,
}

/// Phase from the file name: `.X83` / `.d81` → 83 / 81.
pub fn phase_of_file(file: &Path) -> Option<u8> {
    let extension = file.extension()?.to_str()?.to_ascii_lowercase();
    let (kind, digits) = extension.split_at_checked(1)?;
    if !matches!(kind, "x" | "d" | "p") || digits.len() != 2 {
        return None;
    }
    digits
        .parse::<u8>()
        .ok()
        .filter(|phase| (81..=86).contains(phase))
}

/// How an entry of a catalogue file came and what it should become in the
/// LV. STLB-Bau Online delivers a Standardbeschreibung as an Item without
/// unit and quantity (Anwender 18.11.) – that is a text, the suggestion says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogKind {
    /// Item, Remark or PerfDescr in the file.
    pub delivered: proava_domain::RowKind,
    /// What the take-over inserts unless the planner chooses otherwise.
    pub suggested: proava_domain::RowKind,
}

impl CatalogKind {
    /// Why the suggestion differs from the file, for a tooltip.
    pub fn reason(&self) -> Option<&'static str> {
        use proava_domain::RowKind;
        match (self.delivered, self.suggested) {
            (RowKind::Position, RowKind::Note) => Some(
                "In der Datei eine Position ohne Einheit und Menge – vermutlich ein Text \
                 (Standardbeschreibung), deshalb als Hinweis vorgeschlagen.",
            ),
            (RowKind::Note, RowKind::Note) => Some("In der Datei ein Hinweistext."),
            (RowKind::Performance, RowKind::Performance) => {
                Some("In der Datei eine Ausführungsbeschreibung.")
            }
            _ => None,
        }
    }
}

/// Positions read from a catalogue file (see [`ExchangeFiles::read_catalog_positions`]).
/// Hinweistexte and Ausführungsbeschreibungen of the file that carry a
/// catalogue key (STLB-Bau Standardbeschreibungen) come along in position
/// form; [`CatalogPositions::kinds`] tells what each entry was and becomes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CatalogPositions {
    pub positions: Vec<proava_domain::Position>,
    /// One per entry of `positions`, same order.
    pub kinds: Vec<CatalogKind>,
    /// How many of them carry an STLB-Bau key.
    pub with_stlb_key: usize,
    /// File the positions came from inside a zip archive, empty otherwise.
    pub source_in_archive: String,
    pub warnings: Vec<String>,
}

impl CatalogPositions {
    /// Kind of entry `index` (a position when the list is shorter).
    pub fn kind(&self, index: usize) -> CatalogKind {
        self.kinds.get(index).copied().unwrap_or(CatalogKind {
            delivered: proava_domain::RowKind::Position,
            suggested: proava_domain::RowKind::Position,
        })
    }

    /// The entries the file delivered as positions (for „Aktualisierte
    /// STLB-Bau-Positionen einlesen“, which matches positions only).
    pub fn delivered_positions(&self) -> Vec<proava_domain::Position> {
        self.positions
            .iter()
            .enumerate()
            .filter(|(index, _)| self.kind(*index).delivered == proava_domain::RowKind::Position)
            .map(|(_, position)| position.clone())
            .collect()
    }
}

/// How the price comparison is printed – what the user set up on screen.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComparisonPrint {
    pub options: ComparisonOptions,
    /// Bids to print in this order; empty = all that are not excluded.
    pub bids: Vec<BidId>,
    /// Names to print instead of the bidders' (same order as `bids`), e.g.
    /// „Bieter 1“ for an anonymous comparison; empty = real names.
    pub names: Vec<String>,
    /// Only headings with their sums, no positions.
    pub totals_only: bool,
}

/// Files that leave or enter proAVA: GAEB and PDF.
/// `Send + Sync`, so exports and imports can run on a worker thread.
pub trait ExchangeFiles: Send + Sync {
    /// Writes a GAEB file and returns warnings (e.g. images left out).
    fn export_gaeb(
        &self,
        project: &Project,
        kind: GaebExport,
        file: &Path,
    ) -> Result<Vec<String>, String>;
    /// Like [`ExchangeFiles::export_gaeb`] with the choices of the export
    /// dialog; adapters without own handling ignore them.
    fn export_gaeb_with(
        &self,
        project: &Project,
        kind: GaebExport,
        file: &Path,
        options: GaebExportOptions,
    ) -> Result<Vec<String>, String> {
        let _ = options;
        self.export_gaeb(project, kind, file)
    }

    /// GAEB-Fachregeln R1–R13 of the GAEB DA XML file `export_gaeb` would
    /// write for `kind` – the check before the export.
    fn check_gaeb_export(&self, project: &Project, kind: GaebExport) -> Vec<GaebRuleFinding> {
        let _ = (project, kind);
        Vec::new()
    }
    /// Writes the active LV as CSV (proAVA CSV format) and returns what fell
    /// away (formatting, images …).
    fn export_csv(
        &self,
        project: &Project,
        file: &Path,
        encoding: CsvEncoding,
    ) -> Result<Vec<String>, String> {
        let _ = (project, file, encoding);
        Err("CSV-Export ist hier nicht eingebaut.".to_string())
    }
    /// Writes the Kostenverfolgung as shown (CSV) and returns what fell away.
    fn export_tracking_csv(
        &self,
        report: &proava_domain::TrackReport,
        file: &Path,
        encoding: CsvEncoding,
    ) -> Result<Vec<String>, String> {
        let _ = (report, file, encoding);
        Err("CSV-Export ist hier nicht eingebaut.".to_string())
    }
    /// Reads a bidder's X84/D84 and matches it with the LV.
    fn import_bid(&self, project: &Project, file: &Path) -> Result<BidImport, String>;
    /// [`ExchangeFiles::import_bid`] with the way of matching.
    fn import_bid_with(
        &self,
        project: &Project,
        file: &Path,
        options: &BidMatchOptions,
    ) -> Result<BidImport, String> {
        let _ = options;
        self.import_bid(project, file)
    }
    /// Builds a new project from any GAEB LV (X81/X82/X83/D8x).
    fn import_lv(
        &self,
        file: &Path,
        name: &ProjectName,
        created_iso: &str,
    ) -> Result<(Project, Vec<String>), String>;
    fn export_pdf(
        &self,
        project: &Project,
        document: PdfDocument,
        file: &Path,
    ) -> Result<(), String>;
    /// Like [`ExchangeFiles::import_lv`], with phase, format and program of
    /// the file (GAEB-Import mit Vorschau und Protokoll).
    fn read_lv(
        &self,
        file: &Path,
        name: &ProjectName,
        created_iso: &str,
    ) -> Result<LvImport, String> {
        let (project, warnings) = self.import_lv(file, name, created_iso)?;
        Ok(LvImport {
            project,
            warnings,
            phase: phase_of_file(file),
            format: String::new(),
            program: String::new(),
            columns: Vec::new(),
            facts: Vec::new(),
            currency: None,
        })
    }
    /// Positions of a GAEB file written by STLB-Bau (or any catalogue), to be
    /// taken into the open LV: texts, units, keys; order numbers as in the file.
    /// Images are left out (they would have no place in the project yet).
    fn read_catalog_positions(&self, file: &Path) -> Result<CatalogPositions, String> {
        let _ = file;
        Err(NOT_YET.to_string())
    }
    /// Auftrags-LV for one bid of the active LV (usually the awarded one);
    /// returns warnings (bid not awarded, positions without price …).
    fn export_contract(
        &self,
        project: &Project,
        bid: BidId,
        kind: ContractExport,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        let _ = (project, bid, kind, file);
        Err(NOT_YET.to_string())
    }
    /// Any printout of the print dialog with its options; returns the page
    /// count and warnings. Without an own implementation the three fixed
    /// PDFs are printed with their defaults.
    fn export_print(
        &self,
        project: &Project,
        job: &crate::PrintJob,
        file: &Path,
    ) -> Result<crate::PrintOutcome, String> {
        let document = match job.kind {
            crate::PrintKind::RequestForQuotation => PdfDocument::RequestForQuotation,
            crate::PrintKind::PricedLv => PdfDocument::PricedLv,
            crate::PrintKind::PriceComparison => PdfDocument::PriceComparison,
            crate::PrintKind::CostGroups => PdfDocument::CostGroups,
            _ => return Err(NOT_YET.to_string()),
        };
        self.export_pdf(project, document, file)?;
        Ok(crate::PrintOutcome::default())
    }
    /// Pages the printout would have (preview in the dialog).
    fn print_page_count(&self, project: &Project, job: &crate::PrintJob) -> Result<usize, String> {
        let _ = (project, job);
        Err(NOT_YET.to_string())
    }
    /// Änderungsliste of the active LV against another state (`against`
    /// names it in the title block).
    fn export_change_list_pdf(
        &self,
        project: &Project,
        against: &str,
        diff: &proava_domain::lv_diff::LvDiff,
        file: &Path,
    ) -> Result<(), String> {
        let _ = (project, against, diff, file);
        Err(NOT_YET.to_string())
    }
    /// Änderungsliste as printed from the print dialog: logo, Kopf-/Fußzeile,
    /// Stil and Blattrahmen as chosen (`look`), A4 quer on request.
    #[allow(clippy::too_many_arguments)]
    fn export_change_list_pdf_styled(
        &self,
        project: &Project,
        against: &str,
        diff: &proava_domain::lv_diff::LvDiff,
        logo: Option<&PdfLogo>,
        look: &crate::PrintLook,
        frame: &proava_domain::PageFrame,
        landscape: bool,
        file: &Path,
    ) -> Result<(), String> {
        let _ = (logo, look, frame, landscape);
        self.export_change_list_pdf(project, against, diff, file)
    }
    /// Price comparison with the on-screen settings (order, anonymous, variants).
    fn export_price_comparison_pdf(
        &self,
        project: &Project,
        print: &ComparisonPrint,
        file: &Path,
    ) -> Result<(), String> {
        let _ = print;
        self.export_pdf(project, PdfDocument::PriceComparison, file)
    }
    /// Letters (Schriftverkehr) of `kind` to the invited bidders `recipients`
    /// of the active LV, all in one PDF; returns the page count.
    // Logo, Kopf-/Fußzeile and look are the print settings of the caller.
    #[allow(clippy::too_many_arguments)]
    fn export_letters_pdf(
        &self,
        project: &Project,
        kind: proava_domain::LetterKind,
        recipients: &[proava_domain::InviteeId],
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &crate::PrintLook,
        file: &Path,
    ) -> Result<usize, String> {
        let _ = (project, kind, recipients, logo, frame, look, file);
        Err(NOT_YET.to_string())
    }
    /// Letters as a PDF in memory (nothing written) with its page count –
    /// for writing and for the Schriftverkehr archive. `file_name` is the
    /// name shown by the footer field {datei}.
    #[allow(clippy::too_many_arguments)]
    fn letters_pdf(
        &self,
        project: &Project,
        request: &LetterRequest,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &crate::PrintLook,
        file_name: &str,
    ) -> Result<(Vec<u8>, usize), String> {
        let _ = (project, request, logo, frame, look, file_name);
        Err(NOT_YET.to_string())
    }
    /// Writes `bytes` to a file the user chose (e.g. a filed letter).
    fn write_file(&self, file: &Path, bytes: &[u8]) -> Result<(), String> {
        let _ = (file, bytes);
        Err(NOT_YET.to_string())
    }
    /// Prüfung Nachtragsangebot or Nachtragsübersicht of the active LV;
    /// returns the page count.
    fn export_change_order_pdf(
        &self,
        project: &Project,
        print: ChangeOrderPrint,
        logo: Option<&PdfLogo>,
        look: &crate::PrintLook,
        frame: &proava_domain::PageFrame,
        file: &Path,
    ) -> Result<usize, String> {
        let _ = (project, print, logo, look, frame, file);
        Err(NOT_YET.to_string())
    }
    /// Nachtrags-LV of one change order of the active LV as GAEB file
    /// (Nachtragsnummer CONo, status COStatus); returns warnings.
    fn export_change_order_gaeb(
        &self,
        project: &Project,
        change_order: ChangeOrderId,
        kind: ChangeOrderExport,
        file: &Path,
    ) -> Result<Vec<String>, String> {
        let _ = (project, change_order, kind, file);
        Err(NOT_YET.to_string())
    }
    /// Reads a contractor's change-order offer (any GAEB file with prices)
    /// for [`Project::take_change_order_offer`]; returns reading warnings.
    fn read_change_order_offer(
        &self,
        file: &Path,
    ) -> Result<(ChangeOrderOffer, Vec<String>), String> {
        let _ = file;
        Err(NOT_YET.to_string())
    }
    /// Abrechnung: reads an Auftrags-LV (X86/D86) as the contract of the
    /// active LV (not stored yet).
    fn import_contract(&self, project: &Project, file: &Path) -> Result<ContractImport, String> {
        let _ = (project, file);
        Err(NOT_YET.to_string())
    }
    /// Abrechnung: reads an invoice (X89/X89B) or a quantity takeoff (X31)
    /// of the contractor for the active LV (not stored yet).
    fn import_invoice(&self, project: &Project, file: &Path) -> Result<InvoiceImport, String> {
        let _ = (project, file);
        Err(NOT_YET.to_string())
    }
    /// Abrechnung: the letter to the client (Auftraggeber/Bauherr) about the
    /// release of `invoice` (None = the last one) of the active LV; returns
    /// the page count.
    fn export_release_letter_pdf(
        &self,
        project: &Project,
        invoice: Option<proava_domain::InvoiceId>,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        file: &Path,
    ) -> Result<usize, String> {
        let _ = (project, invoice, logo, frame, file);
        Err(NOT_YET.to_string())
    }
    /// Niederschrift über die Angebotsöffnung (Submissionsprotokoll) of the
    /// active LV: invited bidders, bids received with their sums, signatures.
    fn export_submission_pdf(
        &self,
        project: &Project,
        logo: Option<&PdfLogo>,
        frame: &proava_domain::PageFrame,
        look: &crate::PrintLook,
        file: &Path,
    ) -> Result<(), String> {
        let _ = (project, logo, frame, look, file);
        Err(NOT_YET.to_string())
    }

    // ---------- Ausschnitte aus PDF-Seiten ----------

    /// Size of every page of a PDF in points (as shown).
    fn pdf_pages(&self, pdf: &[u8]) -> Result<Vec<(f32, f32)>, String> {
        let _ = pdf;
        Err(NOT_YET.to_string())
    }
    /// Page `page` (from 1) as a PDF of its own; a one-page PDF comes back
    /// unchanged.
    fn pdf_single_page(&self, pdf: &[u8], page: u32) -> Result<Vec<u8>, String> {
        let _ = (pdf, page);
        Err(NOT_YET.to_string())
    }
    /// Renders `rect` of page `page` with `width_px` pixels across (height by
    /// the aspect, within the adapter's limits) on white paper.
    fn render_pdf_clip(
        &self,
        pdf: &[u8],
        page: u32,
        rect: proava_domain::ClipRect,
        width_px: u32,
    ) -> Result<crate::pdf_clip::Pixels, String> {
        let _ = (pdf, page, rect, width_px);
        Err(NOT_YET.to_string())
    }
    /// The pictures of the active LV's long texts as GAEB DA XML carries them
    /// (PDF clips as PNG, 300 dpi) – for the export check.
    fn gaeb_pictures(&self, project: &Project) -> Vec<crate::pdf_clip::GaebPictureInfo> {
        let _ = project;
        Vec::new()
    }
    /// The PNG a PDF clip becomes in GAEB DA XML (300 dpi at its width in
    /// the long text); the adapter may cache it.
    fn gaeb_clip_png(
        &self,
        asset: &proava_domain::ImageAsset,
        width_mm: f32,
    ) -> Result<crate::pdf_clip::GaebPicture, String> {
        let _ = (asset, width_mm);
        Err(NOT_YET.to_string())
    }
}

/// Placeholder until the real adapter is wired: every call says so.
pub struct NoExchange;

const NOT_YET: &str = "Diese Funktion ist noch nicht eingebaut.";

impl ExchangeFiles for NoExchange {
    fn export_gaeb(&self, _: &Project, _: GaebExport, _: &Path) -> Result<Vec<String>, String> {
        Err(NOT_YET.to_string())
    }

    fn import_bid(&self, _: &Project, _: &Path) -> Result<BidImport, String> {
        Err(NOT_YET.to_string())
    }

    fn import_lv(
        &self,
        _: &Path,
        _: &ProjectName,
        _: &str,
    ) -> Result<(Project, Vec<String>), String> {
        Err(NOT_YET.to_string())
    }

    fn export_pdf(&self, _: &Project, _: PdfDocument, _: &Path) -> Result<(), String> {
        Err(NOT_YET.to_string())
    }
}
