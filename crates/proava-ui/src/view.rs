//! Contract between the workspace shell and its sub views (LV, Angebote,
//! Preisspiegel): views change the project and ask the shell for the rest.

use proava_application::{ContractExport, PdfDocument};
use proava_domain::{BidId, InvoiceId, PriceSource, RowId};

/// Something only the shell can do (file dialogs, switching views, saving).
#[derive(Debug, Clone, PartialEq)]
pub enum ViewRequest {
    /// Pick one or more X84/D84 files and import them as bids.
    ImportBids,
    /// Pick a target file and print this PDF.
    ExportPdf(PdfDocument),
    /// Switch to the LV view and select this row.
    ShowInLv(RowId),
    /// Pick a target file and print the Niederschrift über die Angebotsöffnung.
    ExportSubmission,
    /// Open the Schriftverkehr (letters to the invited bidders).
    Letters,
    /// Show the filed letters (Schriftverkehr im Projekt), optionally only
    /// those to this recipient.
    FiledLetters(Option<String>),
    /// Pick a target file and write the Auftrags-LV of this bid.
    ExportContract(BidId, ContractExport),
    /// Carry prices from the bids into the planner's prices (one undo step).
    AdoptPrices {
        source: PriceSource,
        overwrite: bool,
    },
    /// Abrechnung: read a GAEB X86 file as the contract.
    ImportContract,
    /// Abrechnung: read a GAEB X89/X89B/X31 file into an invoice (a new one,
    /// or the takeoff of the given invoice for X31).
    ImportInvoice(Option<InvoiceId>),
    /// Abrechnung: PDF of the selected invoice – release sheet (true) or
    /// takeoff check (false).
    BillingPdf(bool),
    /// Abrechnung: Schriftverkehr with the letter to the client about the
    /// selected invoice.
    ReleaseLetter,
    /// LV quantities changed after the bids came in: Mehr-/Mindermengen of
    /// the awarded bid into a new Nachtrag (Anwender 18.11.).
    QuantityChangeOrder,
}

/// What happened in a view during one frame.
#[derive(Debug, Default)]
pub struct ViewResponse {
    /// The project was changed (enables saving).
    pub dirty: bool,
    /// Short message for the status line.
    pub toast: Option<String>,
    pub requests: Vec<ViewRequest>,
    /// Name of the undo step for a change (`dirty`), e.g. „Bieter entfernen“;
    /// none = the view's general name.
    pub label: Option<&'static str>,
}

impl ViewResponse {
    pub fn merge(&mut self, other: ViewResponse) {
        self.dirty |= other.dirty;
        if other.toast.is_some() {
            self.toast = other.toast;
        }
        if other.label.is_some() {
            self.label = other.label;
        }
        self.requests.extend(other.requests);
    }
}
