//! Use cases of proAVA.
//!
//! Knows *what* the program does (create, open, save, back up, restore),
//! but not *how* it is stored or shown: storage and screen are ports.

pub mod clipboard;
pub mod exchange;
pub mod gaeb_import;
pub mod launcher;
pub mod letter_archive;
pub mod library;
pub mod lv_search;
pub mod pdf_clip;
pub mod ports;
pub mod print;
pub mod program_update;
pub mod protocol;
pub mod settings;
pub mod spelling;
pub mod tender_index;
pub mod window_bridge;

pub use clipboard::{Clip, ClipHeader, ClipKind};
pub use exchange::{
    BidImport, BidMatchOptions, CatalogKind, CatalogPositions, ChangeOrderExport, ChangeOrderPrint,
    ComparisonPrint, ContractExport, ContractImport, CsvEncoding, ExchangeFiles, GaebExport,
    GaebExportOptions, GaebRuleFinding, GaebRuleSeverity, InvoiceImport, LetterRequest, LvImport,
    NoExchange, PdfDocument, PdfLogo,
};
pub use gaeb_import::{GaebImport, ImportPreview, ImportProtocol, ImportTarget};
pub use launcher::{Launcher, ProjectPreview, TemplateEntry, TemplateSource, TEMPLATE_FOLDER_NAME};
pub use letter_archive::{write_letters, LetterJob, LetterOutput, LetterTarget};
pub use library::LibrarySource;
pub use pdf_clip::{prepare_clip, GaebPicture, GaebPictureInfo, PdfSource, Pixels, PreparedClip};
pub use ports::{
    FileDetails, Folders, NoWeb, ProjectLock, ProjectRepository, SettingsFiles, UserFolder,
    WebTexts,
};
pub use print::{
    is_memory_refusal, BorderWidth, CoverBlocks, CoverForm, LineSpacing, ParagraphGap, PrintJob,
    PrintKind, PrintLook, PrintOutcome, PrintSettings, PrintStyle, TextSpacing, MEMORY_REFUSAL,
};
pub use protocol::{Protocol, ProtocolEntry, Tone, ValueKind};
pub use settings::{
    caption_list, default_bidder_captions, default_owner_captions, reference_from_key,
    reference_key, LibrarySetting, PanelSide, Preferences, ProjectKind, RecentProject, Settings,
    ThemeChoice, WindowPlace, AUTOSAVE_MINUTES_RANGE, ZOOM_RANGE,
};
