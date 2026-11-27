//! Workspace: the frame around one open project – header with navigation and
//! menus, status line, dialogs – and the views LV, Angebote, Preisspiegel.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Vec2};
use proava_application::{
    BidImport, ContractExport, CsvEncoding, GaebExport, Launcher, PanelSide, PdfLogo, Preferences,
    PrintJob, PrintKind, PrintOutcome, ThemeChoice,
};
use proava_domain::{BidId, EditStatus, LvRow, Project};

use crate::bidders_view::BiddersView;
use crate::bids_view::BidsView;
use crate::columns::Column;
use crate::comparison_view::ComparisonView;
use crate::file_dialog::{self, DialogResult, FileDialog};
use crate::history::{self, History, ProjectChange};
use crate::icons::Icon;
use crate::jobs::{self, Job, Message, Poll};
use crate::letters_view::{LettersDialog, LettersResult};
use crate::lv_check::{self, Finding, Severity};
use crate::lv_editor::{EditorOutput, LvAction, LvEditor, StructCommand};
use crate::lv_list::{self, LvListAction};
use crate::lv_ops::Command;
use crate::lv_properties::{self, LvPropertiesForm, LvTarget};
use crate::master_data_view::MasterDataForm;
use crate::oz_mask_dialog::{MaskDialogResult, OzMaskDialog};
use crate::print_dialog::{self, PrintDialog, PrintDialogResult};
use crate::replace_question::{ReplaceAnswer, ReplaceQuestion};
use crate::settings_dialog::{SettingsDialog, SettingsResult};
use crate::status_view::StatusFilter;
use crate::stlb_dialog::{self, StlbDialog, StlbResult};
use crate::stlb_update_dialog::{StlbUpdateDialog, StlbUpdateResult};
use crate::theme::{self, Palette};
use crate::view::{ViewRequest, ViewResponse};
use crate::{strings, widgets};
pub(crate) use cost_lv_shell::tender_print;
pub use cost_shell::CostCommand;
pub use tracking_shell::TrackingCommand;

mod bridge_shell;
mod change_order_shell;
mod change_question;
mod compare_shell;
mod context_print_shell;
mod correspondence_shell;
mod cost_lv_shell;
mod cost_shell;
mod drag_shell;
mod import_shell;
mod library_shell;
mod library_store;
mod menu_bar;
mod template_shell;
mod tracking_shell;
mod transfer_shell;

pub(crate) use library_shell::LibraryCommand;

const HEADER_HEIGHT: f32 = 84.0;
/// Extra height of the header when the tabs get their own row (narrow
/// windows, e.g. half a 1920 screen).
const TAB_ROW_HEIGHT: f32 = 36.0;
const STATUS_HEIGHT: f32 = 30.0;
const MESSAGE_SECONDS: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Back,
    /// Back to the start page and open the dialog „Projekt öffnen“ there.
    BackToOpen,
    /// „Neu starten“ after an update: the project is closed (saved or
    /// discarded); the host starts the new program with
    /// [`WorkspaceScreen::restart_arguments`] and closes this window.
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Lv,
    Bids,
    Comparison,
    Billing,
    /// Nachträge des aktiven LV.
    ChangeOrders,
    /// Kostenschätzung und Kostenberechnung (DIN 276) des Projekts.
    Costs,
    /// Kostenverfolgung über alle LVs und Kostenermittlungen (DIN 276, 4.4–4.6).
    Tracking,
}

/// Everything the frame itself can do (menus, command palette).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellAction {
    Save,
    /// Projekt ▸ Neues Fenster: a second proAVA (start screen).
    NewWindow,
    /// Projekt ▸ Projekt schreibgeschützt öffnen …: a second proAVA with
    /// that project read-only (to copy from it).
    OpenReadOnly,
    /// Projekt ▸ Projektstammdaten …
    MasterData,
    /// Projekt ▸ Projekt öffnen … (closes this one, asks for unsaved changes).
    OpenOther,
    /// LVs of the project: switch, new, properties, duplicate, move, delete.
    Lv(LvListAction),
    /// Extras ▸ Einstellungen …
    Settings,
    /// Angebote ▸ Bieterverzeichnis (Reiter Angebote).
    Bidders,
    /// Angebote ▸ Schriftverkehr …
    Letters,
    /// Angebote ▸ Schriftverkehr im Projekt … (abgelegte Schreiben).
    FiledLetters,
    /// Ansicht ▸ Eigenschaftenfenster ein-/ausblenden.
    ToggleDetail,
    /// Ansicht ▸ Eigenschaftenfenster links/rechts.
    DetailSide(PanelSide),
    Undo,
    Redo,
    Backup,
    /// Projekt ▸ Als Vorlage speichern …
    SaveAsTemplate,
    Close,
    /// Band „Bibliothek“: save the library, back to the project before
    /// (in this window, or close this one when the project is open in the other).
    BackToProject,
    Mask,
    Check,
    /// Prüfen ▸ Alle LVs prüfen …
    CheckAll,
    Export(GaebExport),
    /// GAEB ▸ Importieren … (LV aus GAEB als neues LV, Projekt, Übernahme, Bibliothek).
    ImportGaeb,
    /// GAEB ▸ CSV importieren … (derselbe Dialog für CSV-Tabellen).
    ImportCsv,
    /// GAEB ▸ Als CSV exportieren …
    ExportCsv,
    /// Positionsbibliothek: Seitenfenster, ablegen, bearbeiten, GAEB hinein.
    Library(LibraryCommand),
    ImportBids,
    /// Angebote ▸ Angebot von Hand anlegen …
    NewBid,
    PrintDialog,
    Award,
    /// Angebote ▸ Auftrag zurücknehmen … (only with an awarded bid).
    RevokeAward,
    Contract,
    AdoptPrices,
    /// Drucken ▸ <Dokumentart> …: the print dialog with this kind chosen
    /// (Enter prints right away – quick like a direct entry, with all options).
    Print(PrintKind),
    ShowTab(Tab),
    ToggleTheme,
    Editor(Command),
    Structure(StructCommand),
    /// Prüfen ▸ STLB-Bau-Positionen zum Prüfen ausgeben (X81).
    StlbCheckFile,
    /// Prüfen ▸ Aktualisierte STLB-Bau-Positionen einlesen.
    StlbUpdate,
    /// Menü Kosten: Kostenschätzung und Kostenberechnung.
    Costs(CostCommand),
    /// Menü Kosten ▸ Kostenverfolgung …
    Tracking(TrackingCommand),
}

impl ShellAction {
    pub(crate) fn label(self) -> String {
        match self {
            ShellAction::Save => strings::MENU_SAVE.into(),
            ShellAction::MasterData => "Projektstammdaten …".into(),
            ShellAction::OpenOther => strings::OPEN_PROJECT.into(),
            ShellAction::Lv(LvListAction::New) => "Neues LV …".into(),
            ShellAction::Lv(LvListAction::Properties(_)) => "LV-Eigenschaften …".into(),
            ShellAction::Lv(LvListAction::Duplicate(_)) => "LV duplizieren (ohne Angebote)".into(),
            ShellAction::Lv(LvListAction::Delete(_)) => "LV löschen …".into(),
            ShellAction::Lv(LvListAction::Move { up: true, .. }) => "LV nach oben".into(),
            ShellAction::Lv(LvListAction::Move { up: false, .. }) => "LV nach unten".into(),
            ShellAction::Lv(LvListAction::Switch(index)) => format!("Gehe zu LV {}", index + 1),
            ShellAction::Lv(LvListAction::Copy(_)) => "Ganzes LV kopieren".into(),
            ShellAction::Lv(LvListAction::Paste) => "LV aus Zwischenablage einfügen …".into(),
            ShellAction::Settings => "Einstellungen …".into(),
            ShellAction::ToggleDetail => "Eigenschaftenfenster ein-/ausblenden".into(),
            ShellAction::DetailSide(PanelSide::Left) => "Eigenschaftenfenster links".into(),
            ShellAction::DetailSide(PanelSide::Right) => "Eigenschaftenfenster rechts".into(),
            ShellAction::Undo => Command::Undo.label().into(),
            ShellAction::Redo => Command::Redo.label().into(),
            ShellAction::Backup => strings::MENU_BACKUP.into(),
            ShellAction::SaveAsTemplate => "Als Vorlage speichern …".into(),
            ShellAction::Close => "Projekt schließen".into(),
            ShellAction::BackToProject => "Zurück zum Projekt".into(),
            ShellAction::NewWindow => "Neues Fenster".into(),
            ShellAction::OpenReadOnly => "Projekt schreibgeschützt öffnen …".into(),
            ShellAction::Mask => strings::MENU_MASK.into(),
            ShellAction::Check => strings::MENU_CHECK.into(),
            ShellAction::CheckAll => strings::MENU_CHECK_ALL.into(),
            ShellAction::Export(kind) => format!("Export {} …", kind.label()),
            ShellAction::ImportGaeb => "GAEB importieren (X81–X86, D81–D86) …".into(),
            ShellAction::ImportCsv => "CSV importieren …".into(),
            ShellAction::ExportCsv => "Als CSV exportieren …".into(),
            ShellAction::Library(command) => command.label().into(),
            ShellAction::ImportBids => strings::MENU_IMPORT_BIDS.into(),
            ShellAction::NewBid => "Angebot von Hand anlegen …".into(),
            ShellAction::Bidders => "Bieterverzeichnis".into(),
            ShellAction::Letters => "Schriftverkehr (Anschreiben an Bieter) …".into(),
            ShellAction::FiledLetters => "Schriftverkehr im Projekt (abgelegte Schreiben) …".into(),
            ShellAction::Award => "Auftrag erteilen …".into(),
            ShellAction::RevokeAward => "Auftrag zurücknehmen …".into(),
            ShellAction::Contract => "Auftrags-LV exportieren (X86/D86/PDF) …".into(),
            ShellAction::AdoptPrices => "Preise als Planerpreise übernehmen …".into(),
            ShellAction::StlbCheckFile => "STLB-Bau-Positionen zum Prüfen ausgeben (X81) …".into(),
            ShellAction::StlbUpdate => "Aktualisierte STLB-Bau-Positionen einlesen …".into(),
            ShellAction::Print(kind) => format!("{} …", kind.label()),
            ShellAction::PrintDialog => "Drucken …".into(),
            ShellAction::ShowTab(Tab::Lv) => "Ansicht LV".into(),
            ShellAction::ShowTab(Tab::Bids) => "Ansicht Angebote".into(),
            ShellAction::ShowTab(Tab::Comparison) => "Ansicht Preisspiegel".into(),
            ShellAction::ShowTab(Tab::Billing) => "Ansicht Abrechnung".into(),
            ShellAction::ShowTab(Tab::ChangeOrders) => "Ansicht Nachträge".into(),
            ShellAction::ShowTab(Tab::Costs) => "Ansicht Kosten".into(),
            ShellAction::ShowTab(Tab::Tracking) => "Ansicht Kostenverfolgung".into(),
            ShellAction::Costs(command) => command.label().into(),
            ShellAction::Tracking(command) => command.label().into(),
            ShellAction::ToggleTheme => "Hell/Dunkel umschalten".into(),
            ShellAction::Editor(command) => command.label().into(),
            ShellAction::Structure(command) => command.label().into(),
        }
    }

    pub(crate) fn shortcut(self) -> &'static str {
        match self {
            ShellAction::Save => "Strg+S",
            ShellAction::PrintDialog => "Strg+P",
            ShellAction::Undo => "Strg+Z",
            ShellAction::Redo => "Strg+Y",
            ShellAction::ShowTab(Tab::Lv) => "Strg+1",
            ShellAction::ShowTab(Tab::Bids) => "Strg+2",
            ShellAction::ShowTab(Tab::Comparison) => "Strg+3",
            ShellAction::ShowTab(Tab::Billing) => "Strg+4",
            ShellAction::ShowTab(Tab::ChangeOrders) => "Strg+5",
            ShellAction::ShowTab(Tab::Costs) => "Strg+6",
            ShellAction::ShowTab(Tab::Tracking) => "Strg+7",
            ShellAction::ToggleDetail => "Strg+E",
            ShellAction::Editor(command) => command.shortcut(),
            ShellAction::Structure(command) => command.shortcut(),
            ShellAction::Library(command) => command.shortcut(),
            _ => "",
        }
    }
}

/// Everything the command palette (Strg+K) offers.
pub(crate) fn palette_actions() -> Vec<ShellAction> {
    let mut actions = vec![
        ShellAction::Save,
        ShellAction::Editor(Command::NewPosition),
        ShellAction::Editor(Command::NewTitle),
        ShellAction::Editor(Command::NewNote),
        ShellAction::Editor(Command::NewSpacer),
        ShellAction::Editor(Command::NewPointSection),
        ShellAction::Editor(Command::NewSplit),
        ShellAction::Editor(Command::NewPointsTemplate),
        ShellAction::Editor(Command::FromStlb),
        ShellAction::Editor(Command::FromAusschreiben),
        ShellAction::StlbCheckFile,
        ShellAction::StlbUpdate,
        ShellAction::Editor(Command::NewPositionBefore),
        ShellAction::Structure(StructCommand::NewSubtitle),
        ShellAction::Structure(StructCommand::NewNoteBefore),
        ShellAction::Structure(StructCommand::NewPerformance),
        ShellAction::Structure(StructCommand::EditOrdinal),
        ShellAction::Structure(StructCommand::ConvertTo(proava_domain::RowKind::Position)),
        ShellAction::Structure(StructCommand::ConvertTo(proava_domain::RowKind::Note)),
        ShellAction::Structure(StructCommand::ConvertTo(
            proava_domain::RowKind::Performance,
        )),
        ShellAction::Structure(StructCommand::ToFree),
        ShellAction::Editor(Command::Duplicate),
        ShellAction::Editor(Command::Delete),
        ShellAction::Undo,
        ShellAction::Redo,
        ShellAction::Editor(Command::Find),
        ShellAction::Editor(Command::CollapseAll),
        ShellAction::Editor(Command::ExpandAll),
        ShellAction::Editor(Command::Renumber),
        ShellAction::Editor(Command::EditLong),
        ShellAction::Editor(Command::Properties),
        ShellAction::Editor(Command::FindReplace),
        ShellAction::Editor(Command::SpellCheck),
        ShellAction::Editor(Command::ToggleDone),
        ShellAction::Editor(Command::SetStatus(EditStatus::Done)),
        ShellAction::Editor(Command::SetStatus(EditStatus::InProgress)),
        ShellAction::Editor(Command::SetStatus(EditStatus::Open)),
        ShellAction::Editor(Command::NextMissingQuantity),
        ShellAction::Editor(Command::ToggleGapMarks),
        ShellAction::Editor(Command::ConfirmPriceCheck),
        ShellAction::Editor(Command::AllBlankNotesToSpacers),
        ShellAction::Editor(Command::OpenPoints),
        ShellAction::Editor(Command::CostGroups),
        ShellAction::Editor(Command::ToggleCostGroupColumn),
        ShellAction::Editor(Command::ToggleWorkCategoryColumn),
        ShellAction::Editor(Command::ToggleTitleShare),
    ];
    actions.extend(StatusFilter::ALL.map(|filter| ShellAction::Editor(Command::Filter(filter))));
    actions.extend([
        ShellAction::Mask,
        ShellAction::Check,
        ShellAction::CheckAll,
        ShellAction::MasterData,
        ShellAction::Lv(LvListAction::New),
        ShellAction::Lv(LvListAction::Properties(usize::MAX)),
        ShellAction::Lv(LvListAction::Duplicate(usize::MAX)),
        ShellAction::Lv(LvListAction::Copy(usize::MAX)),
        ShellAction::Lv(LvListAction::Paste),
        ShellAction::Lv(LvListAction::Delete(usize::MAX)),
        ShellAction::Settings,
        ShellAction::ToggleDetail,
        ShellAction::DetailSide(PanelSide::Left),
        ShellAction::DetailSide(PanelSide::Right),
    ]);
    actions.extend(
        [
            GaebExport::RequestForQuotation,
            GaebExport::PricedLv,
            GaebExport::Description,
        ]
        .map(ShellAction::Export),
    );
    actions.push(ShellAction::ImportGaeb);
    actions.push(ShellAction::ImportCsv);
    actions.push(ShellAction::ExportCsv);
    actions.extend(
        [
            LibraryCommand::Toggle,
            LibraryCommand::Store,
            LibraryCommand::ImportGaeb,
            LibraryCommand::Edit,
            LibraryCommand::Sync,
        ]
        .map(ShellAction::Library),
    );
    actions.push(ShellAction::ImportBids);
    actions.push(ShellAction::NewBid);
    actions.push(ShellAction::Bidders);
    actions.push(ShellAction::Letters);
    actions.push(ShellAction::FiledLetters);
    actions.push(ShellAction::Award);
    actions.push(ShellAction::RevokeAward);
    actions.push(ShellAction::Contract);
    actions.push(ShellAction::AdoptPrices);
    actions.push(ShellAction::Structure(StructCommand::Lv(
        LvAction::Transfer,
    )));
    actions.push(ShellAction::Structure(StructCommand::Lv(LvAction::Compare)));
    actions.push(ShellAction::PrintDialog);
    actions.extend(PrintKind::ALL.map(ShellAction::Print));
    actions.extend(CostCommand::ALL.map(ShellAction::Costs));
    actions.extend(TrackingCommand::ALL.map(ShellAction::Tracking));
    actions.extend([
        ShellAction::ShowTab(Tab::Lv),
        ShellAction::ShowTab(Tab::Bids),
        ShellAction::ShowTab(Tab::Comparison),
        ShellAction::ShowTab(Tab::Billing),
        ShellAction::ShowTab(Tab::ChangeOrders),
        ShellAction::ShowTab(Tab::Costs),
        ShellAction::ShowTab(Tab::Tracking),
        ShellAction::Backup,
        ShellAction::SaveAsTemplate,
        ShellAction::ToggleTheme,
        ShellAction::OpenOther,
        ShellAction::OpenReadOnly,
        ShellAction::Close,
    ]);
    actions
}

/// Entries of the palette that contain every word of `query`.
fn filter_actions(actions: &[ShellAction], query: &str) -> Vec<ShellAction> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    actions
        .iter()
        .copied()
        .filter(|action| {
            let label = action.label().to_lowercase();
            words.iter().all(|word| label.contains(word))
        })
        .collect()
}

/// Results of the worker thread.
enum JobOutcome {
    Gaeb {
        kind: GaebExport,
        file: PathBuf,
        result: Result<Vec<String>, String>,
        /// Index of the exported LV.
        lv: usize,
    },
    /// CSV written; the notes name what fell away.
    Csv {
        file: PathBuf,
        result: Result<Vec<String>, String>,
    },
    Bids {
        results: Vec<(String, Result<BidImport, String>)>,
    },
    Contract {
        kind: ContractExport,
        file: PathBuf,
        result: Result<Vec<String>, String>,
    },
    Print {
        label: &'static str,
        file: PathBuf,
        open: bool,
        result: Result<PrintOutcome, String>,
    },
    PrintPages(Result<usize, String>),
    /// Abrechnung: Auftrag aus X86/D86.
    ContractRead {
        file: PathBuf,
        result: Result<Box<proava_application::ContractImport>, String>,
    },
    /// Abrechnung: Rechnung (X89/X89B) oder Aufmaß (X31); `target` = the
    /// invoice an X31 belongs to.
    InvoiceRead {
        file: PathBuf,
        target: Option<proava_domain::InvoiceId>,
        result: Result<Box<proava_application::InvoiceImport>, String>,
    },
    /// Nachträge: PDF, GAEB, Nachtragsangebot.
    ChangeOrder(change_order_shell::ChangeOrderDone),
    Letters {
        kind: proava_domain::LetterKind,
        recipients: Vec<proava_domain::InviteeId>,
        /// Files written (one, or one per bidder).
        files: Vec<PathBuf>,
        /// „Im Projekt ablegen“: one entry per recipient.
        filed: Vec<proava_domain::CorrespondenceEntry>,
        result: Result<usize, String>,
    },
}

#[derive(Debug, Clone, Copy)]
enum FilePurpose {
    /// Letters (Schriftverkehr): file or folder for `pending_letters`.
    Letters,
    ExportGaeb(GaebExport),
    /// GAEB ▸ Als CSV exportieren … (the check box chooses Windows-1252).
    ExportCsv(CsvEncoding),
    /// Kostenverfolgung as shown (CSV).
    ExportTrackingCsv(CsvEncoding),
    ImportBids,
    InsertImages,
    /// Images for the letter template; the dialog waits in `parked_letters`.
    LetterImages,
    /// A project to open read-only in a new window.
    ReadOnlyProject,
    ExportContract(BidId, ContractExport),
    /// The job waits in `Workspace::pending_print`.
    Print,
    /// Quelle für „Aus anderem Projekt / GAEB übernehmen“.
    TransferSource,
    /// Ordner für „In allen Projekten unter … suchen“ (Übernehmen-Fenster).
    TransferSearchFolder,
    /// Anderer Stand für „Mit anderem Stand vergleichen“.
    CompareSource,
    /// Ziel der Änderungsliste (PDF).
    ComparePdf,
    /// Library entries as GAEB X81.
    ExportLibrary,
    /// STLB-Bau-Positionen zum Prüfen in STLB-Bau (X81).
    StlbCheckFile,
    /// Datei von STLB-Bau mit aktualisierten Positionen.
    StlbUpdateSource,
    /// Abrechnung: Auftrags-LV (X86/D86) als Auftrag.
    ImportContract,
    /// Abrechnung: Rechnung (X89/X89B) oder Mengenermittlung (X31); X31 gehört
    /// zur genannten Rechnung.
    ImportInvoice(Option<proava_domain::InvoiceId>),
    /// Nachträge: PDF, Nachtrags-LV, Nachtragsangebot.
    ChangeOrder(change_order_shell::ChangeOrderFile),
    /// Schriftverkehr im Projekt: „Speichern unter …“ of a filed letter.
    SaveFiledLetter(proava_domain::CorrespondenceId),
    /// Schriftverkehr im Projekt: own PDFs to file.
    AddFiledLetters,
}

enum Dialog {
    /// „Projekt verlassen“ / „proAVA beenden“ with unsaved changes; `error`
    /// tells why saving failed (the question stays open then).
    Leave {
        error: Option<String>,
    },
    /// A recovery copy newer than the project was found when opening.
    Recover {
        file: PathBuf,
        time: String,
    },
    Mask(OzMaskDialog),
    MasterData(Box<MasterDataForm>),
    /// LV properties; for a new LV optionally with the Gliederungs-Assistent on top.
    LvProperties(Box<LvPropertiesForm>, Option<Box<OzMaskDialog>>),
    DeleteLv {
        index: usize,
        question: String,
    },
    /// Angebote ▸ Auftrag zurücknehmen …: `blocked` explains why it cannot
    /// be done now; `with_contract` = the Festschreibung of the Abrechnung
    /// goes along.
    RevokeAward {
        question: String,
        blocked: bool,
        with_contract: bool,
    },
    /// Druckdialog for a document of a window (LV-Vergleich, Nachträge, Schreiben).
    ContextPrint(
        Box<crate::context_print::ContextPrintDialog>,
        context_print_shell::ContextTarget,
    ),
    /// An LV from the clipboard: as a new LV (LV list) or its content into
    /// the table; asks whether the planner's prices come along.
    PasteLv {
        lv: Box<proava_domain::Lv>,
        images: Vec<proava_domain::ImageAsset>,
        summary: String,
        into_table: bool,
    },
    Settings(Box<SettingsDialog>),
    Print(Box<PrintDialog>),
    Letters(Box<LettersDialog>),
    /// Schriftverkehr im Projekt: the filed letters.
    Correspondence(Box<crate::correspondence_view::CorrespondenceDialog>),
    Stlb(Box<StlbDialog>),
    StlbUpdate(Box<StlbUpdateDialog>),
    Check {
        findings: Vec<Finding>,
        export: Option<GaebExport>,
    },
    /// „Alle LVs prüfen“: the findings per LV.
    CheckAll {
        groups: Vec<lv_check::LvFindings>,
    },
    /// Hints after an export or when opening (one line per fact, the hints
    /// as a list), with „Kopieren“.
    Messages {
        protocol: proava_application::Protocol,
        copied: bool,
    },
    /// Order numbers changed after the LV was sent (Anwender 18.11.), or
    /// positions came or went in an LV with bids or an Auftrag (Anwender
    /// 19.11.): keep the change or take it back.
    ChangeQuestion(Box<change_question::ChangeQuestion>),
    /// After reading bids: rename bidders (D84 files often carry no name) and read the warnings.
    BidsRead {
        names: Vec<(BidId, String)>,
        /// Invited bidder for each bid (same order as `names`), proposed by name.
        links: Vec<Option<proava_domain::InviteeId>>,
        /// What was read, one list per bid (same order as `names`) …
        reports: Vec<proava_application::Protocol>,
        /// … and per file that could not be read.
        failures: Vec<proava_application::Protocol>,
        copied: bool,
        focus: bool,
        /// Put the bidders into the Bieterverzeichnis (Vorgabe an).
        adopt: bool,
    },
    File {
        dialog: FileDialog,
        purpose: FilePurpose,
    },
    /// „Dateien ersetzen?“ before one PDF per bidder is written into `target`.
    ReplaceLetters {
        question: Box<ReplaceQuestion>,
        target: PathBuf,
    },
    Palette {
        query: String,
        focus: bool,
        index: usize,
    },
    /// GAEB ▸ Importieren … and its protocol.
    Import(Box<crate::import_dialog::ImportDialog>),
    /// Projekt ▸ Als Vorlage speichern …
    SaveTemplate(Box<template_shell::TemplateForm>),
}

impl Dialog {
    /// Hints after an export, a reading or when opening: the file (if any)
    /// and the hints as a list under one line.
    fn hints(title: impl Into<String>, file: Option<&Path>, lines: Vec<String>) -> Self {
        Dialog::Messages {
            protocol: proava_application::Protocol::messages(title, file, lines),
            copied: false,
        }
    }
}

/// Numbers the workspaces of a run (see [`WorkspaceScreen::session`]).
static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct WorkspaceScreen {
    /// Which opening of a project this is (tests: same session or a new one).
    session: u64,
    launcher: Arc<Launcher>,
    project_file: PathBuf,
    project: Project,
    theme_choice: ThemeChoice,
    palette: Palette,
    tab: Tab,
    editor: LvEditor,
    bids: BidsView,
    /// Reiter „Abrechnung“.
    billing: crate::billing_view::BillingView,
    /// Bieterverzeichnis above the bids.
    bidders: BiddersView,
    /// Print job waiting for its file name, and whether to open the PDF.
    pending_print: Option<(PrintJob, bool)>,
    /// Stil and Seite chosen for the next document of a window (LV-Vergleich, Nachträge, Schreiben).
    context_look: Option<context_print_shell::ContextLook>,
    /// Letters waiting for their file or folder: kind, recipients, one PDF per bidder.
    pending_letters: Option<(
        proava_domain::LetterKind,
        Vec<proava_domain::InviteeId>,
        bool,
    )>,
    /// Invoice of a pending letter to the client (Freigabe an den Bauherrn).
    pending_release_letter: Option<proava_domain::InvoiceId>,
    /// Schriftverkehr dialog while its image file dialog is open.
    parked_letters: Option<Box<LettersDialog>>,
    /// „Im Projekt ablegen“ for the pending letters.
    pending_file_letters: bool,
    /// The list „Schriftverkehr im Projekt“ while its file dialog is open.
    parked_correspondence: Option<Box<crate::correspondence_view::CorrespondenceDialog>>,
    /// Data of the proAVA content this window last put into the system
    /// clipboard (to recognise its own rows on Strg+V).
    clip_data: Option<String>,
    /// LV (session key) the rows of `clip_data` were copied from: Strg+V in
    /// that LV keeps their references, in another LV they come over completely.
    clip_lv: Option<u64>,
    /// What lies in the clipboard, for the status bar („3 Positionen“).
    clip_summary: Option<String>,
    /// Window had the focus last frame (clipboard looked at on return).
    had_focus: bool,
    comparison: ComparisonView,
    /// Reiter „Nachträge“.
    change_orders: crate::change_orders_view::ChangeOrdersView,
    /// Reiter Kosten.
    costs: crate::costs_view::CostsView,
    /// Kostenermittlungen before the last change (undo).
    costs_baseline: Option<Vec<proava_domain::CostEstimate>>,
    /// Reiter Kostenverfolgung.
    tracking: crate::tracking_view::TrackingView,
    /// Kostenverfolgung data before the last change (undo).
    tracking_baseline: Option<proava_domain::CostTracking>,
    dirty: bool,
    leave: bool,
    message: Option<Message>,
    job: Option<Job<JobOutcome>>,
    dialog: Option<Dialog>,
    window_title: String,
    /// Opened read-only: no saving, no recovery copies, no lock of its own.
    read_only: bool,
    /// Time (egui seconds) of the last automatic recovery copy.
    last_autosave: f64,
    /// The recovery copy being written on a worker thread.
    autosave: crate::autosave::BackgroundAutosave,
    /// Since when there are unsaved changes.
    dirty_since: Option<f64>,
    /// The window close button was pressed with unsaved changes: quit after
    /// the „Projekt verlassen“ dialog instead of going back to the start screen.
    quit_after_leave: bool,
    /// „Projekt öffnen …“: after leaving, the start page opens its dialog.
    open_after_leave: bool,
    /// „Neu starten“ after an update: after leaving, the new program starts
    /// with this project (`update_notice`).
    restart_after_leave: bool,
    /// The program file was replaced while it runs: notice in the status bar.
    program_updated: bool,
    /// Undo order over all LVs and project steps (Stammdaten, LV-Liste).
    history: History,
    /// Bids and Bieterverzeichnis of the active LV as last recorded, so a
    /// change in the views can go into the undo history (key of the LV).
    offers_baseline: Option<(u64, proava_domain::OffersSnapshot)>,
    /// Auftrag, Nachträge and Abrechnung of the active LV as last recorded (undo).
    contract_baseline: Option<(u64, proava_domain::ContractSnapshot)>,
    /// LVs (keys) where „Trotzdem ändern“ was chosen after sending: no more
    /// questions until the next X83 export (Anwender 18.11.).
    sent_confirmed: HashSet<u64>,
    /// Records of the LV editor last looked at (key, count) for the question
    /// before order numbers change after sending.
    sent_guard: Option<(u64, u64)>,
    /// LVs (keys) where a change was kept although it leaves bids or the
    /// Auftrag incomplete, with their state then: no more questions until
    /// bids or Auftrag change (Anwender 19.11.).
    bids_confirmed: std::collections::HashMap<u64, proava_domain::BidsOrOrder>,
    /// „Seit dem Versand geändert“ per (revision, LV) – the fingerprint is not
    /// computed every frame.
    sent_cache: Option<((u64, usize), bool)>,
    /// Extras ▸ Einstellungen, read when opening and after changes.
    preferences: Preferences,
    /// Window „Übernehmen aus Projekt, GAEB oder Bibliothek“.
    transfer: Option<crate::transfer_view::TransferView>,
    /// Side window „Bibliothek“ (created when first shown).
    library: Option<crate::library_view::LibraryPanel>,
    /// Entries waiting for the file name of „Auswahl als GAEB X81 …“.
    pending_library_export: Option<Box<Project>>,
    /// Window „Mit Bibliothek abgleichen“.
    library_sync: Option<crate::library_sync::LibrarySync>,
    /// Dialog „In Bibliothek übernehmen“.
    library_form: Option<library_store::LibraryForm>,
    /// „Rückgängig“ of the message after „In Bibliothek übernehmen“.
    library_undo: Option<library_store::LibraryUndo>,
    /// While a library is edited: the project to go back to (file, name) –
    /// the one opened last before it.
    back_to: Option<(PathBuf, String)>,
    /// The open file is one of the libraries of the settings (band
    /// „Bibliothek“, window title); read once when it opens.
    library_source: Option<proava_application::library::LibrarySource>,
    /// Window „Mit anderem Stand vergleichen“.
    compare: Option<crate::compare_view::CompareView>,
    /// Undo pushes of the editor at the last comparison.
    compare_seen: u64,
    /// What an assistant does in this window (local bridge): KI-Markierung,
    /// line in the status bar, a waiting insert.
    ai: crate::bridge::AiActivity,
    /// The local bridge (dragging rows into other windows); None without one.
    bridge: Option<proava_application::window_bridge::BridgeHandle>,
    /// Rows of this window dragged into another one.
    drag_out: Option<drag_shell::DragOut>,
    /// Rows of another window dragged over this one.
    drag_in: Option<drag_shell::DragIn>,
    /// A drop from another window whose answer waits for a question.
    drag_pending: Option<crate::bridge::PendingInsert>,
    /// „Zwischen Fenstern ziehen: Strg+C/Strg+V“ (no window places known) at
    /// this point since this time.
    drag_hint: Option<(egui::Pos2, f64)>,
}

impl WorkspaceScreen {
    pub fn new(launcher: Arc<Launcher>, project_file: PathBuf, project: Project) -> Self {
        let theme_choice = launcher.theme();
        let back_to = library_shell::back_target(&launcher, &project_file);
        let library_source = launcher.library_at(&project_file);
        let mut editor = LvEditor::new(
            project.lv(),
            launcher.detail_width(),
            &launcher.column_widths(),
        );
        // A library opened with „Bibliothek bearbeiten …“ shows Stichwörter, Stand, Preisdatum.
        editor.set_library_mode(library_source.is_some());
        let preferences = launcher.preferences();
        let comparison = new_comparison(&preferences);
        // Early phases (Kostenschätzung/-berechnung, noch keine Positionen):
        // start in the Reiter Kosten.
        let tab = if !project.cost_estimates().is_empty()
            && project.lvs().all(|lv| lv.position_count() == 0)
        {
            Tab::Costs
        } else {
            Tab::Lv
        };
        Self {
            session: NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            launcher,
            project_file,
            project,
            theme_choice,
            palette: Palette::of(theme_choice).with_title_rows(preferences.title_rows_filled),
            tab,
            editor,
            bids: BidsView::new(),
            billing: crate::billing_view::BillingView::new(),
            bidders: BiddersView::new(),
            pending_print: None,
            context_look: None,
            pending_letters: None,
            pending_release_letter: None,
            parked_letters: None,
            pending_file_letters: true,
            parked_correspondence: None,
            clip_data: None,
            clip_lv: None,
            clip_summary: None,
            had_focus: true,
            comparison,
            change_orders: crate::change_orders_view::ChangeOrdersView::new(),
            costs: crate::costs_view::CostsView::new(),
            costs_baseline: None,
            tracking: crate::tracking_view::TrackingView::default(),
            tracking_baseline: None,
            dirty: false,
            leave: false,
            message: None,
            job: None,
            dialog: None,
            window_title: String::new(),
            read_only: false,
            last_autosave: 0.0,
            autosave: Default::default(),
            dirty_since: None,
            quit_after_leave: false,
            open_after_leave: false,
            restart_after_leave: false,
            program_updated: false,
            history: History::default(),
            offers_baseline: None,
            contract_baseline: None,
            sent_confirmed: HashSet::new(),
            sent_guard: None,
            bids_confirmed: std::collections::HashMap::new(),
            sent_cache: None,
            preferences,
            transfer: None,
            library: None,
            pending_library_export: None,
            library_sync: None,
            library_form: None,
            library_undo: None,
            back_to,
            library_source,
            compare: None,
            compare_seen: 0,
            ai: crate::bridge::AiActivity::default(),
            bridge: None,
            drag_out: None,
            drag_in: None,
            drag_pending: None,
            drag_hint: None,
        }
        .offer_recovery()
    }

    /// Offers a recovery copy that is newer than the project file.
    fn offer_recovery(mut self) -> Self {
        if let Some((file, time)) = self.launcher.newer_autosave(&self.project_file) {
            self.dialog = Some(Dialog::Recover { file, time });
        }
        self
    }

    /// Every few minutes with unsaved changes: a recovery copy next to the
    /// project, written on a worker thread (only the snapshot is taken here).
    fn autosave_if_due(&mut self, ctx: &egui::Context, now: f64) {
        if let Some(result) = self.autosave.finished() {
            self.autosave_result(ctx, result);
        }
        if self.autosave.is_running() {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if !self.dirty || !self.preferences.autosave || self.read_only {
            self.dirty_since = None;
            return;
        }
        let since = *self.dirty_since.get_or_insert(now);
        let due = since.max(self.last_autosave) + self.preferences.autosave_minutes as f64 * 60.0;
        if now < due {
            ctx.request_repaint_after(Duration::from_secs_f64(due - now + 0.5));
            return;
        }
        self.last_autosave = now;
        if let Some((file, storage)) = self.launcher.background_autosave(&self.project_file) {
            self.autosave.start(storage, file, self.project.clone());
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let mut copy = self.project.clone();
        copy.remove_unused_images();
        let result = self.launcher.autosave(&self.project_file, &copy);
        self.autosave_result(ctx, result);
    }

    fn autosave_result(&mut self, ctx: &egui::Context, result: Result<PathBuf, String>) {
        if let Err(error) = result {
            self.say(
                ctx,
                format!("Automatische Sicherung fehlgeschlagen: {error}"),
            );
        }
    }

    /// Lets a recovery copy that is being written finish (before saving or
    /// discarding: it must not land after them).
    fn wait_for_autosave(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.autosave.wait() {
            self.autosave_result(ctx, result);
        }
    }

    /// Shows messages right after opening (e.g. warnings of a GAEB import).
    /// Colours of the appearance with the title rows as set (Einstellungen).
    fn palette_of_settings(&self) -> Palette {
        Palette::of(self.theme_choice).with_title_rows(self.launcher.title_rows_filled())
    }

    /// The project as it is in memory (with unsaved changes).
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Index of the selected row in the active LV (for tests and hosts).
    pub fn selected_index(&self) -> Option<usize> {
        self.editor.selected_row(self.project.lv())
    }

    /// All selected rows of the LV table (multiple selection).
    pub fn selection_ids(&self) -> std::collections::HashSet<proava_domain::RowId> {
        self.editor.selection_ids()
    }

    /// Whether a cell of the LV table is being edited (for tests).
    pub fn cell_editing(&self) -> bool {
        self.editor.cell_editing()
    }

    /// The cell of the LV table being edited (for tests).
    pub fn editing_cell(&self) -> Option<crate::cell_edit::Cell> {
        self.editor.editing_cell()
    }

    /// Row (index in the active LV) of the table cell being edited (for tests).
    pub fn editing_index(&self) -> Option<usize> {
        self.editor
            .editing_row()
            .and_then(|row| self.project.lv().index_of(row))
    }

    /// A dialog of the shell or of the LV editor is open (for tests).
    pub fn any_dialog_open(&self) -> bool {
        self.dialog.is_some() || self.editor.has_dialog()
    }

    /// Column positions of the LV table as last drawn (for tests).
    pub fn table_layout(&self) -> Option<crate::columns::Columns> {
        self.editor.table_layout()
    }

    /// Whether a column of the LV table is switched on (KG, LB; for tests).
    pub fn column_shown(&self, column: Column) -> bool {
        self.editor.column_shown(column)
    }

    /// Whether the title rows show their share of the LV total (Ansicht; for tests).
    pub fn title_share_shown(&self) -> bool {
        self.editor.title_share_shown()
    }

    /// Whether the Eigenschaftenfenster is shown (Ansicht, Strg+E; for tests).
    pub fn detail_visible(&self) -> bool {
        self.preferences.detail_visible
    }

    /// Row shown in the window „Eigenschaften“, if it is open (for tests and hosts).
    pub fn position_window_row(&self) -> Option<proava_domain::RowId> {
        self.editor.position_window_row()
    }

    /// Datenpunkt-Abschnitt whose plant list is open (for tests).
    pub fn point_window_row(&self) -> Option<proava_domain::RowId> {
        self.editor.point_window_row()
    }

    /// Row shown in the window „Langtext“ (Umschalt+F2), if it is open.
    pub fn long_text_window_row(&self) -> Option<proava_domain::RowId> {
        self.editor.long_text_window_row()
    }

    /// Rows the LV table shows completely (places in the shown order; for tests).
    pub fn table_rows_shown(&self) -> std::ops::Range<usize> {
        self.editor.fully_shown_rows()
    }

    /// The tab in front (LV, Angebote, Preisspiegel).
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Whether a dialog of the workspace is open.
    pub fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// Hands a message of the browser window to the open take-over dialog
    /// (the window does it itself; tests simulate a download with it).
    /// False when no such dialog is open.
    pub fn browser_event(&mut self, event: proava_web_protocol::FromHelper) -> bool {
        match &mut self.dialog {
            Some(Dialog::Stlb(dialog)) => {
                dialog.web_event(event);
                true
            }
            _ => false,
        }
    }

    /// Number of this opening of a project, unique in the run: a project
    /// closed and opened again is a new session (new undo history).
    pub fn session(&self) -> u64 {
        self.session
    }

    /// Whether a long piece of work (export, import, PDF) is still running.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether the question „Projekt verlassen“ / „proAVA beenden“ is open.
    pub fn leave_question_open(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Leave { .. }))
    }

    /// An update waits for a restart (`update_notice`): the status bar says so.
    pub fn set_program_updated(&mut self, updated: bool) {
        self.program_updated = updated;
    }

    /// „Neu starten“: like quitting (with unsaved changes the question
    /// „proAVA neu starten“ first), then the host starts the new program
    /// with this project ([`Outcome::Restart`]).
    pub fn request_restart(&mut self, ctx: &egui::Context) {
        self.restart_after_leave = true;
        self.quit_after_leave = true;
        self.open_after_leave = false;
        if self.dirty {
            if !self.leave_question_open() {
                self.dialog = Some(Dialog::Leave { error: None });
            }
        } else {
            self.leave_now(ctx);
        }
    }

    /// Arguments for the new program after „Neu starten“: the same project,
    /// read-only again if it was.
    pub fn restart_arguments(&self) -> Vec<std::ffi::OsString> {
        let mut arguments: Vec<std::ffi::OsString> = Vec::new();
        if self.read_only {
            arguments.push("--schreibgeschuetzt".into());
        }
        arguments.push(self.project_file.clone().into_os_string());
        arguments
    }

    /// The window is to close (X, Alt+F4, window list – also while it is
    /// minimized): with unsaved changes the close is cancelled and the
    /// question „proAVA beenden“ comes. Returns whether it was cancelled.
    pub fn ask_before_quit(&mut self, ctx: &egui::Context) -> bool {
        if !self.dirty {
            return false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.quit_after_leave = true;
        self.open_after_leave = false;
        self.restart_after_leave = false;
        if !self.leave_question_open() {
            self.dialog = Some(Dialog::Leave { error: None });
        }
        true
    }

    /// Read-only (another window has the project open, or only to copy from).
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        if read_only && matches!(self.dialog, Some(Dialog::Recover { .. })) {
            // The recovery copy belongs to the window that edits the project.
            self.dialog = None;
        }
        self
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn with_messages(mut self, title: &str, lines: Vec<String>) -> Self {
        if !lines.is_empty() {
            self.dialog = Some(Dialog::Messages {
                protocol: proava_application::Protocol::messages(title, None, lines),
                copied: false,
            });
        }
        self
    }

    /// The protocol of the import that created the project (GAEB or CSV
    /// from the start screen): shown first; messages of opening it become one
    /// more line of it.
    pub fn with_protocol(mut self, protocol: Option<proava_application::ImportProtocol>) -> Self {
        let Some(mut protocol) = protocol else {
            return self;
        };
        if let Some(Dialog::Messages {
            protocol: messages, ..
        }) = self.dialog.take()
        {
            protocol.entries.extend(messages.entries);
        }
        self.dialog = Some(Dialog::Import(Box::new(
            crate::import_dialog::ImportDialog::showing(protocol),
        )));
        self
    }

    /// Topic of F1 for the view in front.
    pub fn help_topic(&self) -> crate::help::Topic {
        match self.tab {
            Tab::Lv => crate::help::Topic::Lv,
            Tab::Bids => crate::help::Topic::Bids,
            Tab::Comparison => crate::help::Topic::Comparison,
            Tab::Billing => crate::help::Topic::Billing,
            Tab::ChangeOrders => crate::help::Topic::ChangeOrders,
            Tab::Costs | Tab::Tracking => crate::help::Topic::Costs,
        }
    }

    /// PDF clips in long texts: the preview service with the renderer of the
    /// exchange adapter, the setting „alle ausblenden“, and the preview
    /// choices (einfrieren, ausblenden) the editors asked for.
    fn clip_previews(&mut self, ctx: &egui::Context) {
        let mut previews = crate::pdf_preview::ClipPreviews::of(ctx);
        if !previews.has_renderer() {
            let exchange = self.launcher.shared_exchange();
            let renderer: crate::pdf_preview::Renderer =
                std::sync::Arc::new(move |pdf, page, rect, width| {
                    exchange.render_pdf_clip(pdf, page, rect, width)
                });
            previews = crate::pdf_preview::ClipPreviews::install(ctx, Some(renderer));
        }
        if let Some(hidden) = previews.take_all_hidden_request() {
            self.launcher.set_clip_previews_hidden(hidden);
        }
        previews.set_all_hidden(self.launcher.clip_previews_hidden());
        if crate::pdf_preview::apply_updates(ctx, &mut self.project) {
            self.dirty = true;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Outcome {
        let ctx = ui.ctx().clone();
        theme::apply(&ctx, self.theme_choice);
        self.update_title(&ctx);

        let now = ctx.input(|input| input.time);
        if self
            .message
            .as_ref()
            .is_some_and(|message| message.expired(now))
        {
            self.message = None;
        }
        self.poll_job(&ctx);
        self.notice_clipboard(&ctx);
        self.clip_previews(&ctx);
        self.autosave_if_due(&ctx, now);
        // Fenster schließen (X, Alt+F4, Fensterliste): `ask_before_quit`,
        // aufgerufen aus `App::logic` – das läuft auch bei minimiertem Fenster.

        let mut actions: Vec<ShellAction> = Vec::new();
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
            actions.push(ShellAction::Save);
        }
        if self.dialog.is_none()
            && !self.editor.has_dialog()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::P))
        {
            actions.push(ShellAction::PrintDialog);
        }
        if self.dialog.is_none() && !self.editor.has_dialog() {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
                self.open_palette();
            }
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F8)) {
                actions.push(ShellAction::Library(LibraryCommand::Toggle));
            }
            // Strg+E: Eigenschaftenfenster ein/aus (Anwender 18.11.); exact
            // modifiers – Strg+Umschalt+E sets „in Bearbeitung“.
            if ctx.input_mut(|input| {
                let at = input.events.iter().position(|event| {
                    matches!(event, egui::Event::Key { key: egui::Key::E, pressed: true, modifiers, .. }
                        if modifiers.matches_exact(egui::Modifiers::COMMAND))
                });
                at.map(|at| input.events.remove(at)).is_some()
            }) {
                actions.push(ShellAction::ToggleDetail);
            }
            for (key, tab) in [
                (egui::Key::Num1, Tab::Lv),
                (egui::Key::Num2, Tab::Bids),
                (egui::Key::Num3, Tab::Comparison),
                (egui::Key::Num4, Tab::Billing),
                (egui::Key::Num5, Tab::ChangeOrders),
                (egui::Key::Num6, Tab::Costs),
                (egui::Key::Num7, Tab::Tracking),
            ] {
                if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, key)) {
                    actions.push(ShellAction::ShowTab(tab));
                }
            }
        }

        // Strg+Z/Y in der Reihenfolge des ganzen Projekts (auch Stammdaten,
        // Angebote, LV-Liste und andere LVs). In einem Textfeld bleibt es beim
        // Editor, solange der neueste Schritt eine Änderung dieses LVs ist –
        // sonst erreichte Strg+Z im Feld eine spätere Angebotsänderung nie
        // (Affentest).
        if self.dialog.is_none() && !self.editor.has_dialog() {
            let in_field = ctx.text_edit_focused();
            if in_field {
                self.note_editor();
            }
            let key = self.active_key();
            let editor_undo =
                in_field && self.history.next_is_lv(key, false) && self.editor.can_undo();
            let editor_redo =
                in_field && self.history.next_is_lv(key, true) && self.editor.can_redo();
            let (undo, redo) = ctx.input_mut(|input| {
                let command = egui::Modifiers::COMMAND;
                let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
                let redo = !editor_redo
                    && (input.consume_key(shift, egui::Key::Z)
                        || input.consume_key(command, egui::Key::Y));
                let undo = !editor_undo && input.consume_key(command, egui::Key::Z);
                (undo, redo)
            });
            if undo {
                actions.push(ShellAction::Undo);
            }
            if redo {
                actions.push(ShellAction::Redo);
            }
        }

        self.bridge_frame_start(&ctx);
        self.drag_frame_start(&ctx);
        self.library_band(ui, &mut actions);
        self.header(ui, &mut actions);
        self.status_bar(ui);

        // The views of one LV get a small LV choice when there are several
        // LVs of the tender (the LV table shows them one below the other) or
        // the LV of the Schreibmarke is a Kostenermittlung (no bids there).
        if lv_list::has_view_choice(&self.project)
            && matches!(
                self.tab,
                Tab::Bids | Tab::Comparison | Tab::Billing | Tab::ChangeOrders
            )
        {
            let palette = self.palette;
            let picked = egui::Panel::top("lv_view_choice")
                .frame(
                    egui::Frame::new()
                        .fill(palette.background)
                        .inner_margin(egui::Margin {
                            left: 14,
                            right: 14,
                            top: 8,
                            bottom: 0,
                        }),
                )
                .show(ui, |ui| lv_list::view_choice(ui, &palette, &self.project))
                .inner;
            if let Some(index) = picked {
                self.activate_lv(index);
                let label = lv_properties::lv_label(self.project.lv());
                self.say(&ctx, format!("{label} gewählt."));
            }
        }

        // After the LV choice: a Kosten-LV shows a note instead of a tab of the tender.
        let notice = self.cost_lv_notice(ui);
        match self.tab {
            _ if notice => {}
            Tab::Lv => {
                self.cost_lv_banner(ui);
                self.library_panel_ui(&ctx, ui);
                let keys = self.dialog.is_none();
                // Strg+Z/Y the editor handles itself (in a text field) move the
                // project order too.
                self.note_editor();
                let moves = self.editor.undo_moves();
                let output =
                    self.editor
                        .ui(ui, &self.palette, &mut self.project, &self.launcher, keys);
                let after = self.editor.undo_moves();
                let key = self.active_key();
                for _ in moves.0..after.0 {
                    self.history.editor_moved(key, false);
                }
                for _ in moves.1..after.1 {
                    self.history.editor_moved(key, true);
                }
                self.take_editor_output(&ctx, output);
            }
            Tab::Billing => {
                self.ensure_contract_baseline();
                let palette = self.palette;
                self.billing.set_today(&self.launcher.now_iso());
                let response = egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(palette.background)
                            .inner_margin(egui::Margin::symmetric(14, 12)),
                    )
                    .show(ui, |ui| self.billing.ui(ui, &palette, &mut self.project))
                    .inner;
                if response.dirty {
                    self.contract_changed(&ctx, "Abrechnung");
                }
                self.handle_view_response(&ctx, response);
            }
            Tab::ChangeOrders => self.change_orders_tab(&ctx, ui),
            Tab::Costs => self.costs_tab(&ctx, ui),
            Tab::Tracking => self.tracking_tab(&ctx, ui),
            Tab::Bids | Tab::Comparison => {
                self.ensure_offers_baseline();
                let palette = self.palette;
                let response = egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(palette.background)
                            .inner_margin(egui::Margin::symmetric(14, 12)),
                    )
                    .show(ui, |ui| match self.tab {
                        Tab::Bids => {
                            // Oben das Bieterverzeichnis, darunter die Angebote.
                            let mut top = ViewResponse::default();
                            let height = self.bidders.desired_height(
                                &self.project,
                                ui.available_height(),
                                0.55,
                            );
                            egui::Panel::top("bidders_panel")
                                .exact_size(height)
                                .frame(egui::Frame::new().inner_margin(egui::Margin {
                                    left: 0,
                                    right: 0,
                                    top: 0,
                                    bottom: 10,
                                }))
                                .show(ui, |ui| {
                                    // Heading and tool row stay in view, the list scrolls.
                                    top = self.bidders.ui(ui, &palette, &mut self.project);
                                });
                            self.bids.set_today(&self.launcher.now_iso());
                            let mut response = self.bids.ui(ui, &palette, &mut self.project);
                            response.merge(top);
                            response
                        }
                        _ => self.comparison.ui(ui, &palette, &mut self.project),
                    })
                    .inner;
                if response.dirty {
                    self.offers_changed(&ctx, response.label.unwrap_or("Angebote"));
                }
                self.handle_view_response(&ctx, response);
            }
        }

        for action in actions {
            self.apply(&ctx, action);
        }
        self.dialogs(ui);
        self.transfer_ui(&ctx, ui);
        self.compare_ui(&ctx);
        self.library_sync_ui(&ctx);
        self.note_editor();
        self.guard_sent_ordinals(&ctx);
        self.bridge_frame_end(&ctx);
        self.drag_frame_end(&ctx);

        if self.leave {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                strings::WINDOW_TITLE.to_string(),
            ));
            if self.restart_after_leave {
                // The host closes the window once the new program is started.
                Outcome::Restart
            } else if self.quit_after_leave {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                Outcome::Back
            } else if self.open_after_leave {
                Outcome::BackToOpen
            } else {
                Outcome::Back
            }
        } else {
            Outcome::Stay
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        // A library says so (two windows: project and library side by side).
        let name = if self.library_source.is_some() {
            format!(
                "Bibliothek: {}",
                proava_application::library::library_name(&self.project_file)
            )
        } else {
            self.project.name().as_str().to_string()
        };
        let title = format!(
            "proAVA – {}{}{}",
            name,
            if self.read_only {
                " [schreibgeschützt]"
            } else {
                ""
            },
            if self.dirty { " *" } else { "" }
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
    }

    fn take_editor_output(&mut self, ctx: &egui::Context, output: EditorOutput) {
        self.dirty |= output.dirty;
        if let Some(jump) = output.jump {
            // Into another LV of the table: it becomes the LV of the Schreibmarke.
            if jump.lv != self.project.active_lv() {
                self.activate_lv(jump.lv);
            }
            self.editor.land(&self.project, ctx, jump);
        }
        if let Some(action) = output.lv_list {
            self.lv_action(ctx, action);
        }
        if let Some(drop) = output.cross_drop.clone() {
            self.cross_drop(ctx, drop);
        }
        if let Some((find, replacement, options)) = output.replace_all.clone() {
            self.replace_in_all_lvs(ctx, &find, &replacement, &options);
        }
        if let Some((word, with)) = output.spell_replace.clone() {
            self.replace_word_in_all_lvs(ctx, &word, &with);
        }
        if let Some(text) = output.toast {
            self.say(ctx, text);
        }
        if output.pick_images {
            self.open_file_dialog(FilePurpose::InsertImages);
        }
        if let Some(copied) = output.copied {
            self.copy_rows_to_system(ctx, copied);
        }
        if let Some(text) = output.paste {
            self.paste_from_system(ctx, &text);
        }
        if output.open_palette {
            self.open_palette();
        }
        if output.open_stlb {
            self.open_stlb();
        }
        if output.open_ausschreiben {
            self.open_text_source(stlb_dialog::TextSource::AusschreibenDe);
        }
        if let Some(file) = output.dropped_catalog {
            self.tab = Tab::Lv;
            // An open take-over dialog reads the file itself (its source stays).
            match &mut self.dialog {
                Some(Dialog::Stlb(stlb)) => stlb.load_file(&self.launcher, &file),
                _ => {
                    self.dialog = Some(Dialog::Stlb(Box::new(StlbDialog::with_file(
                        &self.launcher,
                        stlb_dialog::TextSource::GaebFile,
                        &file,
                    ))))
                }
            }
        }
        if let Some(document) = output.pdf {
            self.apply(ctx, ShellAction::Print(document.print_kind()));
        }
        if output.save {
            self.apply(ctx, ShellAction::Save);
        }
        match output.lv_action {
            Some(LvAction::Outline) => self.apply(ctx, ShellAction::Mask),
            Some(LvAction::Check) => self.apply(ctx, ShellAction::Check),
            Some(LvAction::NewLv) => self.lv_action(ctx, LvListAction::New),
            Some(LvAction::LvProperties) => {
                let active = self.project.active_lv();
                self.lv_action(ctx, LvListAction::Properties(active))
            }
            Some(LvAction::Transfer) => self.open_transfer(),
            Some(LvAction::ToLibrary) => self.open_library_form(ctx),
            Some(LvAction::Compare) => self.open_compare(),
            None => {}
        }
    }

    /// The single place where answers of the sub views (Angebote, Preisspiegel) are handled.
    pub fn handle_view_response(&mut self, ctx: &egui::Context, response: ViewResponse) {
        self.dirty |= response.dirty;
        if let Some(text) = response.toast {
            self.say(ctx, text);
        }
        for request in response.requests {
            match request {
                ViewRequest::ImportBids => self.open_file_dialog(FilePurpose::ImportBids),
                ViewRequest::ExportSubmission => self.open_print_dialog(PrintKind::Submission),
                ViewRequest::Letters => self.open_letters(),
                ViewRequest::FiledLetters(recipient) => self.open_filed_letters(recipient),
                ViewRequest::ReleaseLetter => {
                    let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
                    let mut letters = LettersDialog::new(
                        &self.project,
                        proava_domain::LetterKind::Release,
                        &today,
                    );
                    letters.set_invoice(self.billing.selected_invoice());
                    self.dialog = Some(Dialog::Letters(Box::new(letters)));
                }
                ViewRequest::ExportPdf(document) => self.open_print_dialog(document.print_kind()),
                ViewRequest::ShowInLv(id) => {
                    self.tab = Tab::Lv;
                    self.editor.select(self.project.lv(), id);
                }
                ViewRequest::ImportContract => self.open_file_dialog(FilePurpose::ImportContract),
                ViewRequest::ImportInvoice(target) => {
                    self.open_file_dialog(FilePurpose::ImportInvoice(target))
                }
                ViewRequest::BillingPdf(release) => {
                    let kind = if release {
                        PrintKind::Release
                    } else {
                        PrintKind::BillingCheck
                    };
                    self.open_print_dialog(kind);
                    if let Some(Dialog::Print(print)) = &mut self.dialog {
                        print.set_invoice(self.billing.selected_invoice());
                    }
                }
                // The Auftrags-LV as PDF gets the options of the print dialog.
                ViewRequest::ExportContract(bid, ContractExport::Pdf) => {
                    self.open_print_dialog(PrintKind::Contract);
                    if let Some(Dialog::Print(print)) = &mut self.dialog {
                        print.set_bid(bid);
                    }
                }
                ViewRequest::ExportContract(bid, kind) => {
                    self.open_file_dialog(FilePurpose::ExportContract(bid, kind))
                }
                ViewRequest::QuantityChangeOrder => self.quantity_change_order(ctx),
                ViewRequest::AdoptPrices { source, overwrite } => {
                    self.editor.record(self.project.lv());
                    match self.project.adopt_prices(source, overwrite) {
                        Ok(result) => {
                            self.editor.reload();
                            self.dirty |= result.changed > 0;
                            let mut text = format!(
                                "Planerpreise ({}): {} übernommen",
                                source.label(),
                                result.changed
                            );
                            if result.kept > 0 {
                                text.push_str(&format!(", {} vorhandene behalten", result.kept));
                            }
                            if !result.without_price.is_empty() {
                                text.push_str(&format!(
                                    ", {} ohne Preis in der Quelle",
                                    result.without_price.len()
                                ));
                            }
                            self.say(ctx, text);
                        }
                        Err(error) => self.say(ctx, error),
                    }
                }
            }
        }
    }

    fn say(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.say_with_file(ctx, text, None);
    }

    /// Whether the active LV went to the bidders (X83 export, fingerprint or a
    /// Versanddatum in the Bieterverzeichnis).
    fn lv_sent(&self) -> bool {
        let lv = self.project.lv();
        lv.request_exported().is_some()
            || lv.sent_fingerprint().is_some()
            || self
                .project
                .invitees()
                .iter()
                .any(|invitee| !invitee.requested.trim().is_empty())
    }

    /// „Seit dem Versand geändert“ (cached per revision and LV).
    fn changed_since_sent(&mut self) -> bool {
        let key = (self.project.revision(), self.project.active_lv());
        if let Some((known, changed)) = self.sent_cache {
            if known == key {
                return changed;
            }
        }
        let changed = self.project.lv().changed_since_sent();
        self.sent_cache = Some((key, changed));
        changed
    }

    /// Opens the list of changes against the sent file (or lets the user
    /// pick the other state when the file is gone).
    fn show_changes_since_sent(&mut self) {
        let file = self
            .project
            .lv()
            .sent_file()
            .map(PathBuf::from)
            .filter(|file| file.is_file());
        self.open_compare();
        if let Some(file) = file {
            self.load_compare_source(file);
        }
    }

    /// Quantities changed after the bids came in → new Nachtrag with the
    /// Mehr-/Mindermengen (Auftrag first taken over if needed), one undo step.
    fn quantity_change_order(&mut self, ctx: &egui::Context) {
        self.ensure_contract_baseline();
        let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
        let had_contract = self.project.contract().is_some();
        match self.project.quantity_changes_to_change_order(&today) {
            Ok((id, count)) => {
                self.dirty = true;
                self.contract_changed(ctx, "Mehr-/Mindermengen in Nachtrag");
                let label = self
                    .project
                    .change_order(id)
                    .map(|order| order.label())
                    .unwrap_or_default();
                let contract = if had_contract {
                    ""
                } else {
                    " (Auftrag aus dem beauftragten Angebot übernommen)"
                };
                self.say(
                    ctx,
                    format!(
                        "{label}: {} als Mehr-/Mindermenge angelegt{contract}.",
                        crate::format::count(count, "Position", "Positionen")
                    ),
                );
                self.change_orders.open(id);
                self.tab = Tab::ChangeOrders;
            }
            Err(error) => self.say(ctx, error),
        }
    }

    fn say_with_file(
        &mut self,
        ctx: &egui::Context,
        text: impl Into<String>,
        file: Option<PathBuf>,
    ) {
        let message = Message {
            text: text.into(),
            started: ctx.input(|input| input.time),
            file,
            undo: false,
        };
        let seconds = if message.file.is_some() {
            15.2
        } else {
            MESSAGE_SECONDS + 0.2
        };
        self.message = Some(message);
        ctx.request_repaint_after(Duration::from_secs_f64(seconds));
    }

    /// A message with „Rückgängig“ (for the last change of a library file).
    fn say_with_undo(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.say_with_file(ctx, text, None);
        if let Some(message) = self.message.as_mut() {
            message.undo = true;
        }
    }

    /// Starts work on a worker thread unless something is still running.
    fn start_job(
        &mut self,
        ctx: &egui::Context,
        label: String,
        work: impl FnOnce() -> JobOutcome + Send + 'static,
    ) {
        if let Some(job) = &self.job {
            let text = format!("Bitte warten: {} läuft noch.", job.label());
            self.say(ctx, text);
            return;
        }
        self.job = Some(Job::spawn(label, work));
        ctx.request_repaint();
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        match job.poll() {
            Poll::Running => ctx.request_repaint_after(Duration::from_millis(100)),
            Poll::Lost => {
                let text = format!("{} ist abgebrochen (interner Fehler).", job.label());
                self.job = None;
                self.say(ctx, text);
            }
            Poll::Done(outcome) => {
                self.job = None;
                self.finish_job(ctx, outcome);
            }
        }
    }

    fn finish_job(&mut self, ctx: &egui::Context, outcome: JobOutcome) {
        match outcome {
            JobOutcome::Gaeb {
                kind,
                file,
                result,
                lv,
            } => match result {
                Ok(warnings) => {
                    if kind == GaebExport::RequestForQuotation {
                        self.mark_request_exported(lv, &file);
                    }
                    if !warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(
                            format!("Export {}", kind.extension()),
                            Some(&file),
                            warnings,
                        ));
                    }
                    self.say_with_file(
                        ctx,
                        format!("Exportiert: {}", file_dialog::file_name(&file)),
                        Some(file),
                    );
                }
                Err(error) => self.say(ctx, format!("Export fehlgeschlagen: {error}")),
            },
            JobOutcome::ContractRead { file, result } => match result {
                Ok(import) => {
                    self.ensure_contract_baseline();
                    let text = format!(
                        "Auftrag aus {} übernommen: {} – {}, Auftragssumme {}. Strg+Z macht es rückgängig.",
                        file_dialog::file_name(&file),
                        import.contract.contractor,
                        crate::format::count(import.contract.items.len(), "Position", "Positionen"),
                        import.contract.total
                    );
                    self.project.set_contract(Some(import.contract));
                    self.dirty = true;
                    self.contract_changed(ctx, "Auftrag aus GAEB");
                    self.tab = Tab::Billing;
                    self.say(ctx, text);
                    if !import.warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(
                            "Auftrag aus GAEB",
                            Some(&file),
                            import.warnings,
                        ));
                    }
                }
                Err(error) => self.say(ctx, format!("Auftrag aus GAEB: {error}")),
            },
            JobOutcome::InvoiceRead {
                file,
                target,
                result,
            } => match result {
                Ok(import) => {
                    self.ensure_contract_baseline();
                    let name = file_dialog::file_name(&file);
                    let positions = import.takeoff.len();
                    let text = match import.invoice {
                        Some(invoice) => {
                            let number = invoice.number.clone();
                            let items = invoice.items.len();
                            let billing = self.project.billing_mut();
                            let id = billing.add_invoice(invoice);
                            billing.add_takeoff(import.takeoff, Some(id));
                            self.billing.select_invoice(id);
                            format!(
                                "Rechnung {number} aus {name} ({}) angelegt: {}{}.",
                                import.format,
                                crate::format::count(items, "Position", "Positionen"),
                                if positions > 0 {
                                    format!(
                                        ", Aufmaß zu {}",
                                        crate::format::count(positions, "Position", "Positionen")
                                    )
                                } else {
                                    String::new()
                                }
                            )
                        }
                        None => {
                            let label = target
                                .and_then(|id| self.project.billing().invoice(id))
                                .map(|invoice| format!("der Rechnung {}", invoice.number))
                                .unwrap_or_else(|| "ohne Rechnung (nicht zugeordnet)".to_string());
                            let lines = self
                                .project
                                .billing_mut()
                                .add_takeoff(import.takeoff, target);
                            format!(
                                "Aufmaß aus {name} ({}): {} zu {} {label}.",
                                import.format,
                                crate::format::count(lines, "Zeile", "Zeilen"),
                                crate::format::count(positions, "Position", "Positionen")
                            )
                        }
                    };
                    self.dirty = true;
                    self.contract_changed(ctx, "Rechnung aus GAEB");
                    self.tab = Tab::Billing;
                    self.say(ctx, text);
                    if !import.warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(
                            format!("Abrechnung aus GAEB ({})", import.format),
                            Some(&file),
                            import.warnings,
                        ));
                    }
                }
                Err(error) => self.say(ctx, format!("Rechnung aus GAEB: {error}")),
            },
            JobOutcome::Contract { kind, file, result } => match result {
                Ok(warnings) => {
                    if !warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(kind.label(), Some(&file), warnings));
                    }
                    self.say_with_file(
                        ctx,
                        format!("Gespeichert: {}", file_dialog::file_name(&file)),
                        Some(file),
                    );
                }
                Err(error) => self.say(ctx, format!("{} fehlgeschlagen: {error}", kind.label())),
            },
            JobOutcome::Print {
                label,
                file,
                open,
                result,
            } => match result {
                Ok(outcome) => {
                    self.say_with_file(
                        ctx,
                        format!(
                            "PDF gespeichert: {} ({})",
                            file_dialog::file_name(&file),
                            crate::format::count(outcome.pages, "Seite", "Seiten")
                        ),
                        Some(file.clone()),
                    );
                    if open {
                        if let Err(error) = crate::jobs::open_with_system(&file) {
                            self.say(ctx, error);
                        }
                    }
                    if !outcome.warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(label, Some(&file), outcome.warnings));
                    }
                }
                // Too big for the free memory: the message is long, a window keeps it readable.
                Err(error)
                    if proava_application::is_memory_refusal(&error) && self.dialog.is_none() =>
                {
                    self.say(
                        ctx,
                        format!("{label}: nicht gedruckt (zu wenig Arbeitsspeicher)."),
                    );
                    self.dialog = Some(Dialog::hints(label, None, vec![error]));
                }
                Err(error) => self.say(ctx, format!("{label}: {error}")),
            },
            JobOutcome::Letters {
                kind,
                recipients,
                files,
                filed,
                result,
            } => match result {
                Ok(pages) => {
                    // Versanddatum im Bieterverzeichnis und Ablage: ein Rückgängig-Schritt.
                    let filed = self.letters_written(kind, &recipients, filed);
                    let mut text = if files.len() == 1 {
                        format!(
                            "{} gespeichert: {} ({})",
                            kind.label(),
                            file_dialog::file_name(&files[0]),
                            crate::format::count(pages, "Seite", "Seiten")
                        )
                    } else {
                        format!(
                            "{} Schreiben gespeichert ({}).",
                            files.len(),
                            crate::format::count(pages, "Seite", "Seiten")
                        )
                    };
                    if filed > 0 {
                        text.push_str(&format!(
                            " {} im Projekt abgelegt.",
                            crate::format::count(filed, "Schreiben", "Schreiben")
                        ));
                    }
                    if let Some(hint) = self.project.correspondence_hint() {
                        text.push_str(&format!(" {hint}"));
                    }
                    let first = if files.len() == 1 {
                        files.first().cloned()
                    } else {
                        None
                    };
                    self.say_with_file(ctx, text, first);
                }
                Err(error) => self.say(ctx, format!("{}: {error}", kind.label())),
            },
            JobOutcome::ChangeOrder(done) => self.finish_change_order_job(ctx, done),
            JobOutcome::PrintPages(result) => {
                if let Some(Dialog::Print(dialog)) = self.dialog.as_mut() {
                    dialog.pages(result);
                }
            }
            JobOutcome::Csv { file, result } => match result {
                Ok(notes) => {
                    if !notes.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints("Export CSV", Some(&file), notes));
                    }
                    self.say_with_file(
                        ctx,
                        format!("Exportiert: {}", file_dialog::file_name(&file)),
                        Some(file),
                    );
                }
                Err(error) => self.say(ctx, format!("Export fehlgeschlagen: {error}")),
            },
            JobOutcome::Bids { results } => {
                self.ensure_offers_baseline();
                let total = results.len();
                let mut reports = Vec::new();
                let mut failures = Vec::new();
                let mut names = Vec::new();
                for (name, result) in results {
                    match result {
                        Ok(import) => {
                            let bidder = import.bid.name().to_string();
                            let id = self.project.add_bid(import.bid);
                            names.push((id, bidder));
                            if let Some(bid) = self.project.bids().iter().find(|bid| bid.id() == id)
                            {
                                reports.push(proava_application::protocol::bid_protocol(
                                    &self.project,
                                    bid,
                                    &import.warnings,
                                ));
                            }
                        }
                        Err(error) => {
                            failures.push(proava_application::protocol::bid_failure(&name, &error))
                        }
                    }
                }
                if !names.is_empty() {
                    self.dirty = true;
                    self.offers_changed(ctx, "Angebote einlesen");
                }
                self.say(
                    ctx,
                    format!("{} von {total} Angeboten eingelesen.", names.len()),
                );
                if (!names.is_empty() || !failures.is_empty()) && self.dialog.is_none() {
                    let links = crate::bidders_view::proposed_links(&self.project, &names);
                    self.dialog = Some(Dialog::BidsRead {
                        names,
                        links,
                        reports,
                        failures,
                        copied: false,
                        focus: true,
                        adopt: true,
                    });
                }
            }
        }
    }

    // ----- Kopf -----

    fn header(&mut self, ui: &mut egui::Ui, actions: &mut Vec<ShellAction>) {
        let palette = self.palette;
        let dark = self.theme_choice == ThemeChoice::Dark;
        let bids = self.project.bids().len();
        let change_orders = self.project.change_orders().len();
        let tab = self.tab;
        let dirty = self.dirty;
        let name = self.project.name().as_str().to_string();
        let label = self.project.info().label.clone();

        // Kosten as a tab once the project has a Kostenermittlung (or it
        // is shown); otherwise Menü Kosten / Strg+6 – the header of
        // pure LV projects stays as narrow as before.
        let costs_tab = tab == Tab::Costs || !self.project.cost_estimates().is_empty();
        // Kostenverfolgung once there is something to follow: a
        // Kostenermittlung, a Kosten-LV, a contract or a budget.
        let tracking_tab =
            tab == Tab::Tracking || costs_tab || crate::tracking_view::has_tracking(&self.project);
        // Width inside the frame: margin 16 and the line on each side.
        let inner_width = ui.available_width() - 34.0;
        let extra_tabs: Vec<&str> = [
            costs_tab.then_some("Kosten"),
            tracking_tab.then_some("Kostenverfolgung"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let room = header_room(
            ui,
            inner_width,
            &self.project,
            dirty,
            bids,
            change_orders,
            &extra_tabs,
        );
        let height = if room.tabs_below {
            HEADER_HEIGHT + TAB_ROW_HEIGHT
        } else {
            HEADER_HEIGHT
        };

        egui::Panel::top("ws_header")
            .exact_size(height)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.line))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                let mut tabs = vec![
                    (Tab::Lv, strings::NAV_LV.to_string()),
                    (Tab::Bids, room.tab_label(strings::NAV_BIDS, bids)),
                    (Tab::Comparison, strings::NAV_COMPARISON.to_string()),
                    (Tab::Billing, strings::NAV_BILLING.to_string()),
                    (
                        Tab::ChangeOrders,
                        room.tab_label("Nachträge", change_orders),
                    ),
                ];
                if costs_tab {
                    tabs.push((Tab::Costs, "Kosten".to_string()));
                }
                if tracking_tab {
                    tabs.push((Tab::Tracking, "Kostenverfolgung".to_string()));
                }
                // Fixed areas: the buttons on the right never cover the tabs;
                // the project name gets what is left (shortened with „…“).
                let full = ui.available_rect_before_wrap();
                let full = egui::Rect::from_min_size(full.min, Vec2::new(full.width(), 32.0));
                let right_rect = egui::Rect::from_min_max(
                    egui::pos2(full.right() - room.right, full.top()),
                    full.max,
                );
                let left_rect = egui::Rect::from_min_max(
                    full.min,
                    egui::pos2(right_rect.left() - 8.0, full.bottom()),
                );
                ui.allocate_rect(full, Sense::hover());
                let clip = ui.clip_rect();
                let mut left = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(left_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                left.set_clip_rect(left_rect.intersect(clip));
                // „Start“ (back to the start page) leads the tabs, so the
                // project name stands first and gets the room (Anwender 18.11.).
                let start_button = |ui: &mut egui::Ui, actions: &mut Vec<ShellAction>| {
                    if widgets::icon_label_button(ui, &palette, Icon::Back, strings::WS_BACK)
                        .on_hover_text(strings::WS_BACK_HINT)
                        .clicked()
                    {
                        actions.push(ShellAction::Close);
                    }
                    ui.add_space(4.0);
                };
                {
                    let ui = &mut left;
                    if room.title > 0.0 {
                        let title = ui
                            .scope(|ui| {
                                ui.set_max_width(room.title);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "{name}{}",
                                            if dirty { " *" } else { "" }
                                        ))
                                        .size(16.0)
                                        .strong()
                                        .color(palette.text),
                                    )
                                    .truncate(),
                                )
                            })
                            .inner;
                        if !label.is_empty() {
                            title.on_hover_text(format!("{name}\n{label}"));
                        }
                    }
                    // No LV choice here: all LVs stand one below the other in
                    // the table (Anwender 18.11. 22:16).
                    if !room.tabs_below {
                        ui.add_space(16.0);
                        start_button(ui, actions);
                        for (which, text) in &tabs {
                            if segment(ui, &palette, text, tab == *which, room.narrow_tabs)
                                .clicked()
                            {
                                actions.push(ShellAction::ShowTab(*which));
                            }
                        }
                    }
                }
                let mut right = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right_rect)
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                );
                right.set_clip_rect(right_rect.intersect(clip));
                {
                    let ui = &mut right;
                    let read_only = self.read_only;
                    let hint = if read_only {
                        "Schreibgeschützt geöffnet – Speichern ist gesperrt"
                    } else {
                        "Speichern (Strg+S)"
                    };
                    let save = ui
                        .add_enabled_ui(!read_only, |ui| {
                            if room.level >= 3 {
                                widgets::square_icon_button(ui, &palette, Icon::Save, hint)
                            } else {
                                widgets::button(ui, &palette, strings::WS_SAVE, true, 104.0)
                            }
                        })
                        .inner;
                    if save
                        .on_hover_text(hint)
                        .on_disabled_hover_text(hint)
                        .clicked()
                    {
                        actions.push(ShellAction::Save);
                    }
                    ui.add_space(6.0);
                    let (icon, hint) = if dark {
                        (Icon::Sun, "Helles Aussehen")
                    } else {
                        (Icon::Moon, "Dunkles Aussehen")
                    };
                    if widgets::square_icon_button(ui, &palette, icon, hint).clicked() {
                        actions.push(ShellAction::ToggleTheme);
                    }
                    ui.add_space(6.0);
                    if widgets::square_icon_button(ui, &palette, Icon::Gear, "Einstellungen")
                        .clicked()
                    {
                        actions.push(ShellAction::Settings);
                    }
                    ui.add_space(6.0);
                    let palette_button = if room.level >= 1 {
                        widgets::square_icon_button(ui, &palette, Icon::Search, "Befehl …")
                            .on_hover_text("Befehlspalette (Strg+K)")
                    } else {
                        widgets::icon_label_button(ui, &palette, Icon::Search, "Befehl …")
                            .on_hover_text("Befehlspalette (Strg+K)")
                    };
                    if palette_button.clicked() {
                        actions.push(ShellAction::Editor(Command::Palette));
                    }
                }
                if room.tabs_below {
                    // Narrow window: the tabs in their own row, so the project
                    // name and every tab stay readable.
                    ui.add_space(4.0);
                    let row = ui.available_rect_before_wrap();
                    let row = egui::Rect::from_min_size(row.min, Vec2::new(row.width(), 32.0));
                    ui.allocate_rect(row, Sense::hover());
                    let mut below = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(row)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    below.set_clip_rect(row.intersect(clip));
                    start_button(&mut below, actions);
                    for (which, text) in &tabs {
                        if segment(&mut below, &palette, text, tab == *which, room.narrow_tabs)
                            .clicked()
                        {
                            actions.push(ShellAction::ShowTab(*which));
                        }
                    }
                }
                ui.add_space(4.0);
                self.menu_bar(ui, actions);
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        let message = self.message.clone();
        let running = self.job.as_ref().map(|job| job.label().to_string());
        let mut open: Option<PathBuf> = None;
        let mut undo_library = false;
        let mut clip_summary = self.clip_summary.clone();
        let sent_changed = self.changed_since_sent();
        let mut show_changes = false;
        // An update waits for a restart: notice and „Neu starten“ for good.
        let program_updated = self.program_updated;
        let mut restart = false;
        let selection = match self.tab {
            Tab::Lv => self.editor.selection_info(self.project.lv()),
            Tab::Bids => crate::format::count(self.project.bids().len(), "Angebot", "Angebote"),
            Tab::Comparison => {
                crate::format::count(self.project.bids().len(), "Angebot", "Angebote")
            }
            Tab::Billing => crate::format::count(
                self.project.billing().invoices().len(),
                "Rechnung",
                "Rechnungen",
            ),
            Tab::ChangeOrders => self.change_orders.status_text(&self.project),
            Tab::Costs => self.costs.status_text(&self.project),
            Tab::Tracking => self.tracking.status_text(&self.project),
        };
        // Several LVs: which one the Schreibmarke (and every LV command) is in.
        let selection = if self.project.lv_count() > 1 && self.tab != Tab::Costs {
            format!(
                "{} · {selection}",
                lv_properties::lv_label(self.project.lv())
            )
        } else {
            selection
        };
        let now = ui.ctx().input(|input| input.time);
        let ai_text = self.ai.text.clone();
        let ai_connected = self.ai.connected(now);
        egui::Panel::bottom("ws_status")
            .exact_size(STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(Stroke::new(1.0, palette.line))
                    .inner_margin(egui::Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(selection).size(12.5).color(palette.text_dim));
                    // Room on the right for „Öffnen“, „gespeichert“, the note
                    // on the sent LV and the clipboard; in a narrow window the
                    // clipboard note goes first.
                    let sent_width = if sent_changed {
                        strings::SENT_CHANGED.chars().count() as f32 * 6.5 + 30.0
                    } else {
                        0.0
                    };
                    let update_width = if program_updated {
                        crate::update_notice::TEXT.chars().count() as f32 * 6.8 + 110.0
                    } else {
                        0.0
                    };
                    let ai_reserve = 190.0
                        + sent_width
                        + update_width
                        + if ai_connected { 110.0 } else { 0.0 };
                    if let Some(text) = &ai_text {
                        // What the assistant does (local bridge) – its own colour.
                        ui.add_space(16.0);
                        let room = (ui.available_width() - ai_reserve).max(120.0);
                        let line = format!("KI: {text}");
                        ui.scope(|ui| {
                            ui.set_max_width(room);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(line.as_str())
                                        .size(12.5)
                                        .strong()
                                        .color(palette.ai),
                                )
                                .truncate(),
                            )
                            .on_hover_text(format!(
                                "{line}\nEsc bricht ab, Strg+Z nimmt Änderungen zurück."
                            ));
                        });
                    }
                    if let Some(label) = &running {
                        ui.add_space(16.0);
                        ui.add(egui::Spinner::new().size(14.0).color(palette.accent));
                        ui.label(
                            RichText::new(label.as_str())
                                .size(12.5)
                                .color(palette.accent),
                        );
                    }
                    let mut clip_width = clip_summary
                        .as_ref()
                        .map_or(0.0, |summary| summary.chars().count() as f32 * 6.8 + 110.0);
                    let reserve = ai_reserve;
                    let wanted = if message.is_some() { 136.0 } else { 0.0 };
                    if ui.available_width() - reserve - clip_width < wanted {
                        clip_summary = None;
                        clip_width = 0.0;
                    }
                    let message_room = ui.available_width() - reserve - clip_width - 16.0;
                    // Beside the line of the assistant only when it fits.
                    let message = message.filter(|_| ai_text.is_none() || message_room >= 100.0);
                    if let Some(message) = &message {
                        ui.add_space(16.0);
                        let room = message_room.max(120.0);
                        ui.scope(|ui| {
                            ui.set_max_width(room);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(message.text.as_str())
                                        .size(12.5)
                                        .color(palette.accent),
                                )
                                .truncate(),
                            )
                            .on_hover_text(message.text.as_str());
                        });
                        if let Some(file) = &message.file {
                            if ui
                                .add(egui::Button::new(RichText::new("Öffnen").size(12.5)).small())
                                .on_hover_text(file.display().to_string())
                                .clicked()
                            {
                                open = Some(file.clone());
                            }
                        }
                        if message.undo
                            && ui
                                .add(
                                    egui::Button::new(RichText::new("Rückgängig").size(12.5))
                                        .small(),
                                )
                                .on_hover_text("Die Bibliotheksdatei wieder auf den Stand davor")
                                .clicked()
                        {
                            undo_library = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (text, color) = if self.read_only {
                            ("schreibgeschützt", palette.warn)
                        } else if self.dirty {
                            ("ungespeichert", palette.warn)
                        } else {
                            ("gespeichert", palette.text_dim)
                        };
                        ui.label(RichText::new(text).size(12.5).color(color));
                        if program_updated {
                            ui.add_space(16.0);
                            restart = ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(crate::update_notice::RESTART).size(12.5),
                                    )
                                    .small(),
                                )
                                .on_hover_text(crate::update_notice::RESTART_HOVER)
                                .clicked();
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(crate::update_notice::TEXT)
                                    .size(12.5)
                                    .strong()
                                    .color(palette.warn),
                            );
                        }
                        if ai_connected {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new("KI verbunden")
                                    .size(12.5)
                                    .color(palette.text_dim),
                            )
                            .on_hover_text(
                                "Ein KI-Assistent hat in den letzten Minuten in diesem Fenster gefragt \
                                 (Extras › Einstellungen › Darstellung › KI-Assistenten).",
                            );
                        }
                        if sent_changed {
                            ui.add_space(16.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(strings::SENT_CHANGED)
                                            .size(12.5)
                                            .color(palette.warn),
                                    )
                                    .small(),
                                )
                                .on_hover_text(crate::lv_check::sent_change_note(&self.project))
                                .clicked()
                            {
                                show_changes = true;
                            }
                        }
                        if let Some(summary) = &clip_summary {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new(format!("Zwischenablage: {summary}"))
                                    .size(12.5)
                                    .color(palette.text_dim),
                            )
                            .on_hover_text(
                                "Mit Strg+V einfügen – auch in einem anderen proAVA-Fenster",
                            );
                        }
                    });
                });
            });
        if show_changes {
            self.show_changes_since_sent();
        }
        if restart {
            self.request_restart(ui.ctx());
        }
        if let Some(file) = open {
            if let Err(error) = jobs::open_with_system(&file) {
                self.say(ui.ctx(), error);
            }
        }
        if undo_library {
            self.undo_library_store(ui.ctx());
        }
    }

    // ----- Aktionen -----

    fn apply(&mut self, ctx: &egui::Context, action: ShellAction) {
        // A Kosten-LV never goes to bidders (menu, palette and keys alike).
        if let Some(message) = self.cost_lv_block(action) {
            self.say(ctx, message);
            return;
        }
        match action {
            ShellAction::Save => self.save_now(ctx),
            ShellAction::MasterData => {
                let mut form = MasterDataForm::of(&self.project);
                form.frame_default = self.launcher.preferences().page_frame;
                self.dialog = Some(Dialog::MasterData(Box::new(form)))
            }
            ShellAction::Lv(action) => self.lv_action(ctx, action),
            ShellAction::Settings => {
                self.dialog = Some(Dialog::Settings(Box::new(SettingsDialog::of(
                    &self.launcher,
                ))))
            }
            ShellAction::ToggleDetail => {
                let visible = !self.launcher.detail_visible();
                self.launcher.set_detail_visible(visible);
                self.preferences.detail_visible = visible;
            }
            ShellAction::DetailSide(side) => {
                self.launcher.set_detail_side(side);
                if !self.preferences.detail_visible {
                    self.launcher.set_detail_visible(true);
                    self.preferences.detail_visible = true;
                }
            }
            ShellAction::Undo | ShellAction::Editor(Command::Undo) => self.undo(ctx),
            ShellAction::Redo | ShellAction::Editor(Command::Redo) => self.redo(ctx),
            ShellAction::Backup => self.backup(ctx),
            ShellAction::SaveAsTemplate => self.open_save_template(),
            ShellAction::NewWindow => {
                if let Err(error) = jobs::new_window(&[]) {
                    self.say(ctx, error);
                } else {
                    self.say(ctx, "Neues proAVA-Fenster wird geöffnet …");
                }
            }
            ShellAction::OpenReadOnly => self.open_file_dialog(FilePurpose::ReadOnlyProject),
            ShellAction::BackToProject => self.back_to_project(ctx),
            ShellAction::OpenOther | ShellAction::Close => {
                self.open_after_leave = action == ShellAction::OpenOther;
                self.restart_after_leave = false;
                if self.dirty {
                    self.quit_after_leave = false;
                    self.dialog = Some(Dialog::Leave { error: None });
                } else {
                    self.leave_now(ctx);
                }
            }
            ShellAction::Mask => {
                self.dialog = Some(Dialog::Mask(OzMaskDialog::of(
                    self.project.lv(),
                    self.project.bids().len(),
                )))
            }
            ShellAction::Check => {
                self.dialog = Some(Dialog::Check {
                    findings: lv_check::check_with_rules(
                        &self.project,
                        None,
                        self.launcher.exchange(),
                    ),
                    export: None,
                })
            }
            ShellAction::CheckAll => {
                self.dialog = Some(Dialog::CheckAll {
                    groups: lv_check::check_all(&self.project, self.launcher.exchange()),
                })
            }
            ShellAction::Export(kind) => {
                let findings =
                    lv_check::check_with_rules(&self.project, Some(kind), self.launcher.exchange());
                if !lv_check::worth_a_stop(&findings) {
                    self.open_file_dialog(FilePurpose::ExportGaeb(kind));
                } else {
                    self.dialog = Some(Dialog::Check {
                        findings,
                        export: Some(kind),
                    });
                }
            }
            ShellAction::ImportGaeb => self.open_import(),
            ShellAction::ImportCsv => self.open_import_csv(),
            ShellAction::ExportCsv => {
                self.open_file_dialog(FilePurpose::ExportCsv(CsvEncoding::Utf8Bom))
            }
            ShellAction::Library(command) => self.library_command(ctx, command),
            ShellAction::ImportBids => self.open_file_dialog(FilePurpose::ImportBids),
            ShellAction::StlbCheckFile => {
                let count = self
                    .project
                    .lv()
                    .positions()
                    .filter(|position| position.stlb_reference().is_some())
                    .count();
                if count == 0 {
                    self.say(ctx, "Das LV hat keine Positionen mit STLB-Bau-Kennung.");
                } else {
                    self.open_file_dialog(FilePurpose::StlbCheckFile);
                }
            }
            ShellAction::StlbUpdate => self.open_file_dialog(FilePurpose::StlbUpdateSource),
            ShellAction::Bidders => self.tab = Tab::Bids,
            ShellAction::NewBid => {
                self.tab = Tab::Bids;
                self.bids.open_new_bid();
            }
            ShellAction::Letters => self.open_letters(),
            ShellAction::FiledLetters => self.open_filed_letters(None),
            ShellAction::PrintDialog
                if self.tab == Tab::Costs && !self.project.cost_estimates().is_empty() =>
            {
                if let Some(id) = self.costs.selected(&self.project) {
                    self.open_cost_print(id);
                }
            }
            ShellAction::PrintDialog => {
                let kind = match self.tab {
                    _ if self.active_cost_stage().is_some() => PrintKind::CostLv,
                    Tab::Comparison => PrintKind::PriceComparison,
                    Tab::Bids => PrintKind::BidPrices,
                    _ => PrintKind::PricedLv,
                };
                let kind = if kind == PrintKind::BidPrices && self.project.bids().is_empty() {
                    PrintKind::PricedLv
                } else {
                    kind
                };
                self.open_print_dialog(kind);
            }
            ShellAction::RevokeAward => self.ask_revoke_award(ctx),
            ShellAction::Award | ShellAction::Contract | ShellAction::AdoptPrices => {
                self.tab = Tab::Bids;
                let opened = match action {
                    ShellAction::Award => self.bids.open_award(&self.project),
                    ShellAction::Contract => self.bids.open_contract(&self.project),
                    _ => self.bids.open_prices(&self.project),
                };
                if !opened {
                    self.say(ctx, "Noch keine Angebote – erst Angebote einlesen.");
                }
            }
            ShellAction::Print(PrintKind::CostEstimate) => match self.costs.selected(&self.project)
            {
                Some(id) => self.open_cost_print(id),
                None => self.open_print_dialog(PrintKind::CostEstimate),
            },
            ShellAction::Print(kind) => self.open_print_dialog(kind),
            ShellAction::ShowTab(tab) => self.tab = tab,
            ShellAction::Costs(command) => self.cost_command(ctx, command),
            ShellAction::Tracking(command) => self.tracking_command(command),
            ShellAction::ToggleTheme => {
                self.theme_choice = self.theme_choice.flipped();
                self.palette = self.palette_of_settings();
                theme::apply(ctx, self.theme_choice);
                self.launcher.set_theme(self.theme_choice);
            }
            ShellAction::Editor(Command::Palette) => self.open_palette(),
            ShellAction::Editor(command) => {
                self.tab = Tab::Lv;
                let output = self.editor.run(command, &mut self.project, ctx);
                self.take_editor_output(ctx, output);
            }
            ShellAction::Structure(command) => {
                self.tab = Tab::Lv;
                let output = self.editor.run_struct(command, &mut self.project, ctx);
                self.take_editor_output(ctx, output);
            }
        }
    }

    fn open_stlb(&mut self) {
        self.open_text_source(stlb_dialog::TextSource::StlbBau);
    }

    /// The take-over dialog for a text source (STLB-Bau, AUSSCHREIBEN.DE).
    fn open_text_source(&mut self, source: stlb_dialog::TextSource) {
        self.tab = Tab::Lv;
        self.dialog = Some(Dialog::Stlb(Box::new(StlbDialog::for_source(
            &self.launcher,
            source,
        ))));
    }

    /// „Ganze Datei in die Bibliothek …“ of the take-over dialog: reads the
    /// file like GAEB ▸ Importieren, keeps the Herkunft the dialog noted and
    /// opens the library form.
    fn text_source_to_library(
        &mut self,
        ctx: &egui::Context,
        file: &Path,
        origins: Vec<Option<proava_domain::TextOrigin>>,
    ) {
        let now = self.launcher.now_iso();
        match proava_application::gaeb_import::read(self.launcher.exchange(), file, &now) {
            Ok(mut import) => {
                let lv = import.read.project.lv_mut();
                let indices: Vec<usize> = (0..lv.rows().len())
                    .filter(|&index| lv.rows()[index].is_position())
                    .collect();
                if indices.len() == origins.len() {
                    for (index, origin) in indices.into_iter().zip(origins) {
                        if let Some(position) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
                            position.set_text_origin(origin);
                        }
                    }
                }
                self.open_library_for_import(ctx, import, false);
            }
            Err(error) => self.say(ctx, error),
        }
    }

    /// Where the STLB-Bau dialog puts the next positions (from the selection).
    fn stlb_target(&self) -> stlb_dialog::StlbTarget {
        let lv = self.project.lv();
        stlb_dialog::target_of(lv, self.editor.selected_row(lv))
    }

    /// Takes positions from STLB-Bau into the LV and selects the next open
    /// placeholder (or the last new position), so that the next file from
    /// the Downloads folder goes there. Returns the line for the dialog.
    fn take_over_stlb(
        &mut self,
        ctx: &egui::Context,
        source: stlb_dialog::TextSource,
        entries: Vec<(proava_domain::Position, proava_domain::RowKind)>,
    ) -> Result<String, String> {
        let selected = self.editor.selected_row(self.project.lv());
        let before = self.project.lv().clone();
        let titles = if self.library_source.is_some() {
            crate::lv_ops::NewTitles::Never
        } else {
            crate::lv_ops::NewTitles::Fill
        };
        match stlb_dialog::take_over_with(self.project.lv_mut(), selected, entries, titles) {
            Ok(taken) => {
                self.editor.record(&before);
                self.editor.reload();
                let lv = self.project.lv();
                let ordinals: Vec<String> = (taken.first..=taken.last)
                    .filter_map(|index| lv.rows().get(index)?.as_position())
                    .map(|position| position.ordinal().to_string())
                    .collect();
                // STLB-Bau goes on with the next placeholder, other sources
                // insert the next file after the positions just taken.
                let next = if source == stlb_dialog::TextSource::StlbBau || taken.filled_placeholder
                {
                    stlb_dialog::next_placeholder(lv, taken.last).unwrap_or(taken.last)
                } else {
                    taken.last
                };
                if let Some(id) = lv.rows().get(next).map(|row| row.id()) {
                    self.editor.select(lv, id);
                }
                self.dirty = true;
                let texts = match taken.texts {
                    0 => String::new(),
                    count if count == taken.count => " als Text".to_string(),
                    count => format!(", davon {count} als Text"),
                };
                let message = format!(
                    "{} aus {} übernommen{texts}{}{}",
                    if taken.texts == 0 {
                        crate::format::count(taken.count, "Position", "Positionen")
                    } else {
                        crate::format::count(taken.count, "Eintrag", "Einträge")
                    },
                    source.name(),
                    if taken.filled_placeholder {
                        ", Platzhalter gefüllt"
                    } else {
                        ""
                    },
                    if ordinals.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", ordinals.join(", "))
                    }
                );
                // The dialog stays open: bids or Auftrag as a note, no question.
                let message = match self.uncovered_hint(&before) {
                    Some(hint) => format!("{message} – {hint}"),
                    None => message,
                };
                self.say(ctx, message.clone());
                Ok(message)
            }
            Err(error) => {
                self.say(ctx, error.clone());
                Err(error)
            }
        }
    }

    // ----- Mehrere LVs, Rückgängig über das ganze Projekt -----

    fn active_key(&self) -> u64 {
        self.project
            .lv_key(self.project.active_lv())
            .unwrap_or_default()
    }

    /// Takes note of steps the LV editor added (for the undo order).
    fn note_editor(&mut self) {
        let key = self.active_key();
        self.history.note_editor(key, self.editor.undo_pushes());
    }

    /// Records a project step (Stammdaten, LV-Liste) for Strg+Z.
    fn record_project(&mut self, before: ProjectChange, label: &'static str) {
        self.note_editor();
        self.history.record_project(before, label);
    }

    /// Shows another LV of the project.
    fn activate_lv(&mut self, index: usize) {
        self.note_editor();
        let previous = self.active_key();
        if self.project.set_active_lv(index) {
            self.follow_active_lv(previous);
            // Unless a click or key goes on with a row (`LvEditor::land`):
            // the LV's head row at the top of the table.
            if self.project.lv_count() > 1 {
                self.editor.show_head_of(index);
            }
        }
    }

    /// The active LV may have changed (switch, undo): hand the editor the
    /// new LV's undo stack and view, start fresh offer/comparison views.
    /// Remembers bids and Bieterverzeichnis of the active LV (if not yet).
    fn ensure_offers_baseline(&mut self) {
        let key = self.active_key();
        if self
            .offers_baseline
            .as_ref()
            .is_none_or(|(known, _)| *known != key)
        {
            self.offers_baseline = self
                .project
                .offers_snapshot(self.project.active_lv())
                .map(|snapshot| (key, snapshot));
        }
    }

    fn ensure_contract_baseline(&mut self) {
        let key = self.active_key();
        if self
            .contract_baseline
            .as_ref()
            .is_none_or(|(known, _)| *known != key)
        {
            self.contract_baseline = self
                .project
                .contract_snapshot(self.project.active_lv())
                .map(|snapshot| (key, snapshot));
        }
    }

    /// Auftrag, Nachträge or Abrechnung may have changed: one undo step with
    /// the state before (typing in a row joins one step).
    fn contract_changed(&mut self, ctx: &egui::Context, label: &'static str) {
        let key = self.active_key();
        let Some(current) = self.project.contract_snapshot(self.project.active_lv()) else {
            return;
        };
        if let Some((known, before)) = self.contract_baseline.take() {
            if known == key && before != current {
                self.note_editor();
                let now = ctx.input(|input| input.time);
                let join = before.shape() == current.shape();
                self.history.record_contract(key, before, label, now, join);
            }
        }
        self.contract_baseline = Some((key, current));
    }

    /// Angebote ▸ Auftrag zurücknehmen …: the question depends on what hangs
    /// on the award – nothing (plain question), only the Festschreibung of the
    /// Abrechnung (it goes along, red warning) or invoices/Nachträge (blocked).
    fn ask_revoke_award(&mut self, _ctx: &egui::Context) {
        let Some(bid) = self.project.awarded_bid() else {
            return;
        };
        let award = bid.award().cloned().unwrap_or_default();
        let symbol = proava_domain::price_check::currency_symbol(self.project.lv().currency());
        let what = format!(
            "Auftrag an {} vom {} über {} {} (netto nach Nachlass)",
            bid.name(),
            award.date,
            award.total.plain(),
            symbol
        );
        let invoices = self.project.billing().invoices().len();
        let orders = self.project.change_orders().len();
        let contract = self.project.contract().is_some();
        let (question, blocked) = if invoices > 0 || orders > 0 {
            let parts: Vec<String> = [
                (invoices > 0).then(|| crate::format::count(invoices, "Rechnung", "Rechnungen")),
                (orders > 0).then(|| crate::format::count(orders, "Nachtrag", "Nachträge")),
            ]
            .into_iter()
            .flatten()
            .collect();
            (
                format!(
                    "{what}: Auf ihm stehen schon {} (Reiter Abrechnung bzw. Nachträge). Sie gehören \
                     zum festgeschriebenen Auftrag und gingen sonst ins Leere. Erst dort löschen, dann den \
                     Auftrag zurücknehmen.",
                    parts.join(" und ")
                ),
                true,
            )
        } else if contract {
            (
                format!(
                    "{what}. Er ist in der Abrechnung festgeschrieben – die Festschreibung wird mit \
                     zurückgenommen. Das Angebot bleibt im Preisspiegel, ein abgelegtes Zuschlagsschreiben \
                     bleibt in der Ablage. Strg+Z holt beides zurück."
                ),
                false,
            )
        } else {
            (
                format!(
                    "{what}. Das Angebot bleibt im Preisspiegel, ein abgelegtes Zuschlagsschreiben bleibt \
                     in der Ablage. Strg+Z holt den Auftrag zurück."
                ),
                false,
            )
        };
        self.dialog = Some(Dialog::RevokeAward {
            question,
            blocked,
            with_contract: contract,
        });
    }

    /// Takes the award back (and the Festschreibung with it) – one undo step.
    fn revoke_award(&mut self, ctx: &egui::Context, with_contract: bool) {
        let index = self.project.active_lv();
        let key = self.active_key();
        let (Some(offers), Some(contract)) = (
            self.project.offers_snapshot(index),
            self.project.contract_snapshot(index),
        ) else {
            return;
        };
        let Some(id) = self.project.revoke_award() else {
            return;
        };
        if with_contract {
            self.project.set_contract(None);
        }
        let mut steps = vec![ProjectChange::Offers(key, Box::new(offers))];
        if with_contract {
            steps.push(ProjectChange::Contract(key, Box::new(contract)));
        }
        let change = if steps.len() == 1 {
            steps.remove(0)
        } else {
            ProjectChange::Several(steps)
        };
        self.record_project(change, "Auftrag zurücknehmen");
        // The new state is the base for the next change (no second step).
        self.offers_baseline = self
            .project
            .offers_snapshot(index)
            .map(|snapshot| (key, snapshot));
        self.contract_baseline = self
            .project
            .contract_snapshot(index)
            .map(|snapshot| (key, snapshot));
        self.dirty = true;
        let name = self
            .project
            .bids()
            .iter()
            .find(|bid| bid.id() == id)
            .map(|bid| bid.name().to_string())
            .unwrap_or_default();
        self.say(
            ctx,
            format!("Auftrag an {name} zurückgenommen – Strg+Z holt ihn zurück."),
        );
    }

    /// Bids or invited bidders may have changed: one undo step with the state before.
    fn offers_changed(&mut self, ctx: &egui::Context, label: &'static str) {
        let key = self.active_key();
        let Some(current) = self.project.offers_snapshot(self.project.active_lv()) else {
            return;
        };
        if let Some((known, before)) = self.offers_baseline.take() {
            if known == key && before != current {
                self.note_editor();
                let now = ctx.input(|input| input.time);
                let join = before.shape() == current.shape();
                self.history.record_offers(key, before, label, now, join);
            }
        }
        self.offers_baseline = Some((key, current));
    }

    fn follow_active_lv(&mut self, previous: u64) {
        self.offers_baseline = None;
        let current = self.active_key();
        if current == previous {
            self.editor.reload();
            return;
        }
        let view = self.editor.view_state();
        let mut stack = LvEditor::new_undo_stack();
        self.editor.swap_undo(&mut stack);
        let saved_view = self.history.switch(previous, current, &mut stack, view);
        self.editor.swap_undo(&mut stack);
        // Both in one table (LVs of the tender): the table keeps its place.
        let shown = self.project.lv_indices_shown_with(self.project.active_lv());
        let several = shown.len() > 1
            && shown
                .iter()
                .any(|index| self.project.lv_key(*index) == Some(previous));
        self.editor
            .switch_lv(previous, current, self.project.lv(), saved_view, several);
        self.bids = BidsView::new();
        self.bidders = BiddersView::new();
        self.billing = crate::billing_view::BillingView::new();
        self.contract_baseline = None;
        self.comparison = new_comparison(&self.preferences);
        self.change_orders.show_list();
    }

    fn undo(&mut self, ctx: &egui::Context) {
        self.note_editor();
        let key = self.active_key();
        let costs = self.history.costs_next(false);
        let tracking = self.history.tracking_next(false);
        match self
            .history
            .undo(&mut self.project, key, self.editor.can_undo())
        {
            history::Action::Lv(target) => {
                if target != key {
                    if let Some(index) = self.project.lv_index_of_key(target) {
                        self.activate_lv(index);
                    }
                }
                self.tab = Tab::Lv;
                let output = self.editor.run(Command::Undo, &mut self.project, ctx);
                self.take_editor_output(ctx, output);
            }
            history::Action::Done(label) => {
                self.follow_active_lv(key);
                self.after_costs_step(costs);
                self.after_tracking_step(tracking);
                self.dirty = true;
                self.say(ctx, format!("Rückgängig: {label}."));
            }
            history::Action::Nothing => self.say(ctx, "Nichts mehr rückgängig zu machen."),
        }
    }

    fn redo(&mut self, ctx: &egui::Context) {
        self.note_editor();
        let key = self.active_key();
        let costs = self.history.costs_next(true);
        let tracking = self.history.tracking_next(true);
        match self
            .history
            .redo(&mut self.project, key, self.editor.can_redo())
        {
            history::Action::Lv(target) => {
                if target != key {
                    if let Some(index) = self.project.lv_index_of_key(target) {
                        self.activate_lv(index);
                    }
                }
                self.tab = Tab::Lv;
                let output = self.editor.run(Command::Redo, &mut self.project, ctx);
                self.take_editor_output(ctx, output);
            }
            history::Action::Done(label) => {
                self.follow_active_lv(key);
                self.after_costs_step(costs);
                self.after_tracking_step(tracking);
                self.dirty = true;
                self.say(ctx, format!("Wiederholt: {label}."));
            }
            history::Action::Nothing => self.say(ctx, "Nichts zu wiederholen."),
        }
    }

    /// After an undo/redo step: the Kosten view starts fresh; a step of the
    /// Kostenermittlung shows the Reiter Kosten.
    fn after_costs_step(&mut self, costs: bool) {
        self.costs.reset_editing();
        self.costs_baseline = None;
        if costs {
            self.tab = Tab::Costs;
        }
    }

    /// Rows of the active LV dragged onto another LV of the table: inserted
    /// there (OZ after its mask, Ausführungsbeschreibungen and images along,
    /// like Strg+C/Strg+V), removed here unless copied (Strg). One undo step
    /// for both LVs; the target LV becomes the LV of the Schreibmarke.
    fn cross_drop(&mut self, ctx: &egui::Context, drop: crate::lv_editor::CrossDrop) {
        let source = self.project.active_lv();
        if drop.to_lv == source || drop.to_lv >= self.project.lv_count() || drop.ids.is_empty() {
            return;
        }
        if !drop.copy {
            if let Err(message) = self.project.lv().ensure_editable(&drop.ids) {
                self.say(ctx, message);
                return;
            }
        }
        self.note_editor();
        let options = proava_domain::transfer::TransferOptions {
            quantities: true,
            prices: true,
        };
        let transfer =
            proava_domain::transfer::collect(&self.project, self.project.lv(), &drop.ids, options);
        let before = self.project.lv_list_snapshot();
        let Some(target_before) = self.project.lv_at(drop.to_lv).cloned() else {
            return;
        };
        let summary = proava_application::clipboard::describe_rows(&transfer.rows);
        // Moved rows leave the source LV: its state before for the question.
        let source_before = if drop.copy {
            None
        } else {
            self.project.lv_at(source).cloned()
        };
        self.activate_lv(drop.to_lv);
        match self
            .editor
            .place_transfer(&mut self.project, &transfer, drop.before)
        {
            Ok(_) => {
                if !drop.copy {
                    if let Some(lv) = self.project.lv_at_mut(source) {
                        let ids: HashSet<proava_domain::RowId> = drop.ids.iter().copied().collect();
                        let roots = crate::lv_ops::selection_roots(lv, &ids);
                        crate::lv_ops::delete_roots(lv, &roots);
                    }
                }
                let label = if drop.copy {
                    "Zeilen in anderes LV kopieren"
                } else {
                    "Zeilen in anderes LV verschieben"
                };
                self.history
                    .record_project(ProjectChange::LvList(Box::new(before)), label);
                self.dirty = true;
                let target = lv_properties::lv_label(self.project.lv());
                let verb = if drop.copy { "kopiert" } else { "verschoben" };
                self.say(
                    ctx,
                    format!("{summary} nach {target} {verb} – Strg+Z macht es rückgängig."),
                );
                let source = source_before.as_ref().map(|lv| (source, lv));
                self.ask_after_drop(ctx, &target_before, source);
            }
            Err(error) => {
                if let Some(index) = self.project.lv_index_of_key(before.keys()[source]) {
                    self.activate_lv(index);
                }
                self.say(ctx, error);
            }
        }
    }

    /// „Alle ersetzen“ with several LVs in the table: in each of them, one
    /// undo step for all; the message names the LVs with replacements.
    fn replace_in_all_lvs(
        &mut self,
        ctx: &egui::Context,
        find: &str,
        replacement: &str,
        options: &crate::search_replace::SearchOptions,
    ) {
        self.note_editor();
        let before = self.project.lv_list_snapshot();
        let mut total = crate::search_replace::ReplaceReport::default();
        let mut per_lv = Vec::new();
        for index in self.project.lv_indices_shown_with(self.project.active_lv()) {
            let Some(lv) = self.project.lv_at_mut(index) else {
                continue;
            };
            let report = crate::search_replace::replace_all(lv, find, replacement, options);
            if report.replaced > 0 {
                per_lv.push(format!(
                    "{}: {}",
                    lv_properties::lv_label(lv),
                    report.replaced
                ));
            }
            total.replaced += report.replaced;
            total.rows += report.rows;
            total.locked_hits += report.locked_hits;
            total.locked_rows += report.locked_rows;
            total.stlb_hits += report.stlb_hits;
        }
        let mut message = total.message(find);
        if total.replaced > 0 {
            self.history
                .record_project(ProjectChange::LvList(Box::new(before)), "Alle ersetzen");
            self.editor.reload();
            self.dirty = true;
            message.push_str(&format!(" ({})", per_lv.join(" · ")));
        }
        self.say(ctx, message);
    }

    /// Rechtschreibung „Überall ersetzen“ with several LVs in the table: in
    /// each of them, one undo step for all.
    fn replace_word_in_all_lvs(&mut self, ctx: &egui::Context, word: &str, with: &str) {
        self.note_editor();
        let before = self.project.lv_list_snapshot();
        let (mut replaced, mut skipped) = (0, 0);
        for index in self.project.lv_indices_shown_with(self.project.active_lv()) {
            if let Some(lv) = self.project.lv_at_mut(index) {
                let (done, left) = proava_application::spelling::replace_word(lv, word, with);
                replaced += done;
                skipped += left;
            }
        }
        if replaced > 0 {
            self.history.record_project(
                ProjectChange::LvList(Box::new(before)),
                "Rechtschreibung ersetzen",
            );
            self.dirty = true;
        }
        let mut message = format!("„{word}“ {replaced}× durch „{with}“ ersetzt.");
        if skipped > 0 {
            message.push_str(&format!(
                " {skipped}× in fertigen Positionen oder STLB-Bau-Langtexten nicht geändert."
            ));
        }
        self.editor.spelling_replaced(ctx, &self.project, message);
    }

    fn lv_action(&mut self, ctx: &egui::Context, action: LvListAction) {
        let active = self.project.active_lv();
        // Aus der Befehlspalette kommt „das aktive LV“ als usize::MAX.
        let index_of = |index: usize| if index == usize::MAX { active } else { index };
        match action {
            LvListAction::Switch(index) => {
                // Gehe zu LV: it becomes the LV of the Schreibmarke, the table
                // shows its head row.
                self.activate_lv(index);
                self.tab = Tab::Lv;
                self.editor.land(
                    &self.project,
                    ctx,
                    crate::lv_editor::LvJump {
                        lv: index,
                        row: None,
                        then: crate::lv_editor::JumpThen::Head,
                    },
                );
            }
            LvListAction::New => {
                self.dialog = Some(Dialog::LvProperties(
                    Box::new(LvPropertiesForm::new_lv(&self.project)),
                    None,
                ))
            }
            LvListAction::Properties(index) => {
                if let Some(form) = LvPropertiesForm::existing(&self.project, index_of(index)) {
                    self.dialog = Some(Dialog::LvProperties(Box::new(form), None));
                }
            }
            LvListAction::Duplicate(index) => {
                let index = index_of(index);
                let before = self.project.lv_list_snapshot();
                let number = lv_properties::next_lv_number(&self.project);
                if let Some(copy) = self.project.duplicate_lv(index) {
                    if let Some(lv) = self.project.lv_at_mut(copy) {
                        lv.set_number(number);
                    }
                    self.record_project(ProjectChange::LvList(Box::new(before)), "LV duplizieren");
                    self.activate_lv(copy);
                    self.dirty = true;
                    let label = lv_properties::lv_label(self.project.lv());
                    self.say(ctx, format!("{label} angelegt (ohne Angebote)."));
                }
            }
            LvListAction::Copy(index) => self.copy_lv_to_system(ctx, index_of(index)),
            LvListAction::Paste => self.paste_lv_from_system(ctx),
            LvListAction::Move { index, up } => {
                let index = index_of(index);
                let before = self.project.lv_list_snapshot();
                if self.project.move_lv(index, up).is_some() {
                    self.record_project(ProjectChange::LvList(Box::new(before)), "LV verschieben");
                    self.dirty = true;
                }
            }
            LvListAction::Delete(index) => {
                let index = index_of(index);
                if self.project.lv_count() <= 1 {
                    self.say(ctx, "Das letzte LV eines Projekts bleibt.");
                } else if let Some(question) = lv_list::delete_question(&self.project, index) {
                    self.dialog = Some(Dialog::DeleteLv { index, question });
                }
            }
        }
    }

    // ----- Zwischenablage zwischen proAVA-Fenstern -----

    /// Puts `html` (with the proAVA header line) and `plain` into the system
    /// clipboard; without one (or on an error) only the text.
    fn put_into_clipboard(
        &mut self,
        ctx: &egui::Context,
        clip: &proava_application::Clip,
        mask: &proava_domain::OzMask,
        html_table: &str,
        plain: &str,
    ) {
        match self.launcher.clipboard_header(clip, mask) {
            Ok((header, without_images)) => {
                let html = format!("{html_table}<!--{}--></body></html>", header.line());
                if crate::system_clipboard::set_html(&html, plain) {
                    self.clip_data = Some(header.data.clone());
                    self.clip_summary = Some(header.summary.clone());
                    if without_images {
                        self.say(
                            ctx,
                            format!(
                                "{} kopiert – ohne Bilder (zu groß für die Zwischenablage).",
                                header.summary
                            ),
                        );
                    }
                    return;
                }
                self.clip_summary = Some(header.summary);
            }
            Err(error) => self.say(ctx, format!("Nur als Text kopiert: {error}")),
        }
        self.clip_data = None;
        ctx.copy_text(plain.to_string());
    }

    fn copy_rows_to_system(&mut self, ctx: &egui::Context, copied: crate::lv_editor::Copied) {
        self.clip_lv = Some(self.active_key());
        let clip = proava_application::Clip::Rows(copied.transfer);
        self.put_into_clipboard(ctx, &clip, &copied.mask, &copied.html, &copied.plain);
    }

    /// The proAVA header line in the system clipboard (HTML, else text).
    fn clipboard_header(text: &str) -> Option<proava_application::ClipHeader> {
        crate::system_clipboard::html()
            .as_deref()
            .and_then(proava_application::ClipHeader::find)
            .or_else(|| proava_application::ClipHeader::find(text))
    }

    /// Strg+V in the table: rows of another proAVA window (or project)
    /// completely, our own rows as before, foreign text is refused.
    fn paste_from_system(&mut self, ctx: &egui::Context, text: &str) {
        self.note_editor();
        let output = match Self::clipboard_header(text) {
            Some(header)
                if self.clip_data.as_deref() == Some(header.data.as_str())
                    && self.editor.has_internal("")
                    && self.clip_lv == Some(self.active_key()) =>
            {
                self.editor.paste_internal(&mut self.project)
            }
            Some(header) => match self.launcher.clipboard_content(&header) {
                Ok(proava_application::Clip::Rows(transfer)) => {
                    self.editor.paste_transfer(&mut self.project, &transfer)
                }
                Ok(proava_application::Clip::Lv { lv, images }) => {
                    self.dialog = Some(Dialog::PasteLv {
                        lv,
                        images,
                        summary: header.summary,
                        into_table: true,
                    });
                    return;
                }
                Err(error) => {
                    self.say(ctx, error);
                    return;
                }
            },
            None if self.editor.has_internal(text) && self.clip_lv == Some(self.active_key()) => {
                self.editor.paste_internal(&mut self.project)
            }
            None => {
                self.say(
                    ctx,
                    "Die Zwischenablage enthält keine LV-Zeilen aus proAVA.",
                );
                return;
            }
        };
        self.take_editor_output(ctx, output);
    }

    /// LV-Liste ▸ Kopieren: the whole LV (without bids) for any proAVA window.
    fn copy_lv_to_system(&mut self, ctx: &egui::Context, index: usize) {
        let Some(lv) = self.project.lv_at(index).cloned() else {
            return;
        };
        let images = proava_domain::transfer::images_of(&self.project, lv.rows());
        let plain = crate::lv_ops::rows_as_text(lv.rows());
        let html = crate::lv_ops::rows_as_html(lv.rows());
        let mask = lv.oz_mask().clone();
        let clip = proava_application::Clip::Lv {
            lv: Box::new(lv),
            images,
        };
        self.put_into_clipboard(ctx, &clip, &mask, &html, &plain);
        if let Some(summary) = self.clip_summary.clone() {
            self.say(
                ctx,
                format!("{summary} kopiert – mit Strg+V in der LV-Liste als neues LV einfügen."),
            );
        }
    }

    /// LV-Liste ▸ Einfügen: an LV from the clipboard as a new LV (asks about prices).
    fn paste_lv_from_system(&mut self, ctx: &egui::Context) {
        let Some(header) = Self::clipboard_header("") else {
            self.say(ctx, "Die Zwischenablage enthält kein LV aus proAVA.");
            return;
        };
        match self.launcher.clipboard_content(&header) {
            Ok(proava_application::Clip::Lv { lv, images }) => {
                self.dialog = Some(Dialog::PasteLv {
                    lv,
                    images,
                    summary: header.summary,
                    into_table: false,
                });
            }
            Ok(proava_application::Clip::Rows(_)) => self.say(
                ctx,
                format!(
                    "In der Zwischenablage liegen Zeilen ({}), kein ganzes LV – in der Tabelle mit Strg+V einfügen.",
                    header.summary
                ),
            ),
            Err(error) => self.say(ctx, error),
        }
    }

    /// Answer of the LV question: as a new LV or its rows into the table.
    fn paste_lv(
        &mut self,
        ctx: &egui::Context,
        lv: proava_domain::Lv,
        images: Vec<proava_domain::ImageAsset>,
        into_table: bool,
        prices: bool,
    ) {
        if into_table {
            let mut rows = lv.rows().to_vec();
            for row in &mut rows {
                if let proava_domain::LvRow::Position(position) = row {
                    position.set_status(proava_domain::EditStatus::Open);
                    if !prices {
                        position.set_unit_price(None);
                        position.set_price_components(Vec::new());
                    }
                }
            }
            self.note_editor();
            let transfer = proava_domain::transfer::Transfer { rows, images };
            let output = self.editor.paste_transfer(&mut self.project, &transfer);
            self.take_editor_output(ctx, output);
            return;
        }
        let before = self.project.lv_list_snapshot();
        let number = lv_properties::next_lv_number(&self.project);
        let index =
            proava_domain::transfer::add_lv(&mut self.project, lv, &images, &number, prices);
        self.record_project(ProjectChange::LvList(Box::new(before)), "LV einfügen");
        self.activate_lv(index);
        self.tab = Tab::Lv;
        self.dirty = true;
        let label = lv_properties::lv_label(self.project.lv());
        self.say(ctx, format!("{label} eingefügt (ohne Angebote)."));
    }

    /// Back in the window: look what another proAVA put into the clipboard.
    fn notice_clipboard(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|input| input.focused);
        if focused && !self.had_focus {
            if let Some(header) = Self::clipboard_header("") {
                if self.clip_data.as_deref() != Some(header.data.as_str()) {
                    self.clip_data = None;
                    self.clip_summary = Some(header.summary);
                }
            } else if self.clip_data.is_some() || self.clip_summary.is_some() {
                // Something else was copied meanwhile (text from another program).
                self.clip_data = None;
                self.clip_summary = None;
            }
        }
        self.had_focus = focused;
    }

    /// „Anlegen“/„Übernehmen“ in the LV properties.
    fn take_lv_properties(
        &mut self,
        ctx: &egui::Context,
        form: &LvPropertiesForm,
    ) -> Result<(), String> {
        match form.target {
            LvTarget::New => {
                let lv = form.build()?;
                let before = self.project.lv_list_snapshot();
                let index = self.project.add_lv(lv);
                self.record_project(ProjectChange::LvList(Box::new(before)), "Neues LV");
                self.activate_lv(index);
                self.tab = Tab::Lv;
                self.dirty = true;
                let label = lv_properties::lv_label(self.project.lv());
                self.say(ctx, format!("{label} angelegt."));
            }
            LvTarget::Existing(index) => {
                self.note_editor();
                let key = self.project.lv_key(index).unwrap_or_default();
                let before = self
                    .project
                    .lv_at(index)
                    .cloned()
                    .ok_or_else(|| "Dieses LV gibt es nicht mehr.".to_string())?;
                let mut changed = before.clone();
                if form.write(&mut changed)? {
                    if index == self.project.active_lv() {
                        self.editor.record(&before);
                        *self.project.lv_mut() = changed;
                        self.editor.reload();
                    } else {
                        self.history.record_other_lv(key, &before);
                        if let Some(lv) = self.project.lv_at_mut(index) {
                            *lv = changed;
                        }
                    }
                    self.dirty = true;
                    self.say(ctx, "LV-Eigenschaften übernommen.");
                }
            }
        }
        Ok(())
    }

    /// Logo for every printout, from Extras ▸ Einstellungen ▸ Drucken.
    fn print_logo(&mut self, ctx: &egui::Context) -> Option<PdfLogo> {
        let logo = self.preferences.logo_file.clone().and_then(|file| {
            match self.launcher.read_file(&file) {
                Ok(bytes) => Some(PdfLogo {
                    file_name: file_dialog::file_name(&file),
                    bytes,
                }),
                Err(error) => {
                    self.say(ctx, format!("Logo nicht gedruckt: {error}"));
                    None
                }
            }
        });
        logo
    }

    /// Back to the start screen; with the setting, a backup (Zip) first.
    fn leave_now(&mut self, ctx: &egui::Context) {
        if self.preferences.backup_on_close && !self.dirty && !self.read_only {
            match self
                .launcher
                .backup(&self.project_file, &self.project_folder())
            {
                Ok(_) => {}
                Err(error) => self.say(
                    ctx,
                    format!("Sicherung beim Schließen fehlgeschlagen: {error}"),
                ),
            }
        }
        self.leave = true;
    }

    /// Schriftverkehr for the active LV.
    fn open_letters(&mut self) {
        self.tab = Tab::Bids;
        let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
        self.dialog = Some(Dialog::Letters(Box::new(LettersDialog::new(
            &self.project,
            proava_domain::LetterKind::Invitation,
            &today,
        ))));
    }

    /// Image files for the letter template: into the project's images, into
    /// the text at the caret; then the Schriftverkehr dialog comes back.
    fn insert_letter_images(&mut self, ctx: &egui::Context, files: &[PathBuf]) {
        let Some(mut letters) = self.parked_letters.take() else {
            return;
        };
        let before = self.project.letter_templates().to_vec();
        let mut added = 0;
        let mut refused = Vec::new();
        for path in files {
            let name = file_dialog::file_name(path);
            match self.launcher.read_file(path) {
                Ok(bytes) if crate::images::is_supported_image(&bytes) => {
                    let file = self.project.add_image(&name, bytes);
                    letters.insert_image(&mut self.project, file);
                    added += 1;
                }
                Ok(_) => refused.push(name),
                Err(error) => refused.push(format!("{name} ({error})")),
            }
        }
        if added > 0 {
            self.dirty = true;
            self.record_project(ProjectChange::Letters(before), "Bild in Briefvorlage");
        }
        let text = match (added, refused.is_empty()) {
            (0, _) => format!("Kein Bild eingefügt: {}", refused.join(", ")),
            (count, true) => format!(
                "{} eingefügt.",
                crate::format::count(count, "Bild", "Bilder")
            ),
            (count, false) => {
                format!(
                    "{} eingefügt; übergangen: {}",
                    crate::format::count(count, "Bild", "Bilder"),
                    refused.join(", ")
                )
            }
        };
        self.say(ctx, text);
        self.dialog = Some(Dialog::Letters(letters));
    }

    /// The print dialog with a kind chosen (Niederschrift from the Angebote …).
    fn open_print_dialog(&mut self, kind: PrintKind) {
        let selected: Vec<_> = self.editor.selection_ids().into_iter().collect();
        let marked = print_dialog::titles_of_selection(&self.project, &selected);
        let mut dialog = PrintDialog::new(
            kind,
            self.launcher.print_settings(kind),
            &self.project,
            marked,
        );
        dialog.default_style = self.launcher.preferences().print_look.style;
        dialog.default_border = self.launcher.preferences().print_look.border;
        dialog.default_grid = self.launcher.preferences().print_look.table_grid;
        dialog.set_tracking(self.tracking.options.clone());
        self.dialog = Some(Dialog::Print(Box::new(dialog)));
    }

    fn open_palette(&mut self) {
        self.dialog = Some(Dialog::Palette {
            query: String::new(),
            focus: true,
            index: 0,
        });
    }

    fn project_folder(&self) -> PathBuf {
        self.project_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.launcher.home_folder())
    }

    fn open_file_dialog(&mut self, purpose: FilePurpose) {
        let folder = self.project_folder();
        let name = self.project.name().as_str().to_string();
        let today = self.launcher.now_iso();
        let dialog = match purpose {
            FilePurpose::ExportGaeb(kind) => {
                let document = match kind {
                    GaebExport::RequestForQuotation => "Angebotsaufforderung",
                    GaebExport::PricedLv => "Kostenvoranschlag",
                    GaebExport::Description => "Leistungsbeschreibung",
                };
                let (images, bytes) = self.project.lv_images();
                let label = if images > 0 {
                    format!(
                        "Bilder mitgeben ({images} im LV, {}; nur GAEB DA XML, GAEB 90 kann keine)",
                        proava_domain::size_text(bytes)
                    )
                } else {
                    "Bilder mitgeben (im LV sind keine; nur GAEB DA XML)".to_string()
                };
                FileDialog::save(
                    &self.launcher,
                    &format!("Export {}", kind.label()),
                    "Datei im Projektordner vorgeschlagen; Name und Ordner sind frei.",
                    folder,
                    &file_dialog::suggested_name(&name, document, &today, kind.extension()),
                    kind.extension(),
                )
                .with_option(&label, self.launcher.gaeb_images())
            }
            FilePurpose::ExportCsv(_) => FileDialog::save(
                &self.launcher,
                "Als CSV exportieren",
                "Format wie beim CSV-Import (OZ, Positionstext, Einheit, Menge, EP, GP, KG, Positionsart; Langtext \
                 in den Zeilen darunter). UTF-8 mit BOM öffnen Excel und LibreOffice direkt.",
                folder,
                &file_dialog::suggested_name(&name, &csv_document(self.project.lv()), &today, "csv"),
                "csv",
            )
            .with_option("Windows-1252 statt UTF-8 (für ältere Programme)", self.launcher.csv_windows_1252()),
            FilePurpose::ExportTrackingCsv(_) => FileDialog::save(
                &self.launcher,
                "Kostenverfolgung als CSV",
                "Die Tabelle wie gezeigt (Ansicht, Ebene, netto/brutto) mit den Abweichungen der Prognose und den \
                 Erläuterungen. UTF-8 mit BOM öffnen Excel und LibreOffice direkt.",
                folder,
                &file_dialog::suggested_name(&name, "Kostenverfolgung", &today, "csv"),
                "csv",
            )
            .with_option("Windows-1252 statt UTF-8 (für ältere Programme)", self.launcher.csv_windows_1252()),
            FilePurpose::ImportContract => FileDialog::open(
                &self.launcher,
                "Auftrag aus GAEB einlesen",
                "Auftrags-LV der Vergabe (X86, D86): Positionen, Mengen und Preise werden der Auftrag dieses LV.",
                folder,
                &strings::CONTRACT_EXTENSIONS,
                false,
            ),
            FilePurpose::ImportInvoice(_) => FileDialog::open(
                &self.launcher,
                "Rechnung oder Aufmaß aus GAEB einlesen",
                "Rechnung der Firma (X89, X89B) als neue Rechnung, Mengenermittlung (X31) als Aufmaß der gewählten Rechnung.",
                folder,
                &strings::INVOICE_EXTENSIONS,
                false,
            ),
            FilePurpose::ImportBids => FileDialog::open(
                &self.launcher,
                "Angebote einlesen",
                "GAEB-Rückläufer der Bieter (X84, D84) – mit Strg+Klick mehrere wählen.",
                folder,
                &strings::BID_EXTENSIONS,
                true,
            ),
            FilePurpose::ExportContract(_, kind) => FileDialog::save(
                &self.launcher,
                kind.label(),
                "LV mit den Preisen des beauftragten Bieters.",
                folder,
                &file_dialog::suggested_name(&name, "Auftrags-LV", &today, kind.extension()),
                kind.extension(),
            ),
            FilePurpose::Letters => {
                let Some((kind, _, separate)) = &self.pending_letters else {
                    return;
                };
                if kind.to_client() {
                    let number = self
                        .pending_release_letter
                        .and_then(|id| self.project.billing().invoice(id))
                        .map(|invoice| invoice.number.trim().replace(['/', '\\', ' '], "-"))
                        .filter(|number| !number.is_empty());
                    let label = match number {
                        Some(number) => format!("{}_{number}", kind.file_label()),
                        None => kind.file_label().to_string(),
                    };
                    FileDialog::save(
                        &self.launcher,
                        kind.label(),
                        "Anschreiben an den Auftraggeber bzw. Bauherrn mit dem Freigabebetrag.",
                        folder,
                        &file_dialog::suggested_name(&name, &label, &today, "pdf"),
                        "pdf",
                    )
                } else if *separate {
                    FileDialog::folder(&self.launcher, "Ordner für die Schreiben", folder)
                } else {
                    FileDialog::save(
                        &self.launcher,
                        kind.label(),
                        "Alle Schreiben in einer PDF (eine Seite je Bieter und mehr).",
                        folder,
                        &file_dialog::suggested_name(&name, kind.file_label(), &today, "pdf"),
                        "pdf",
                    )
                }
            }
            FilePurpose::Print => {
                let kind = self
                    .pending_print
                    .as_ref()
                    .map(|(job, _)| job.kind)
                    .unwrap_or(PrintKind::PricedLv);
                // Abrechnung: the invoice number belongs into the name.
                let number = self
                    .pending_print
                    .as_ref()
                    .and_then(|(job, _)| job.invoice)
                    .and_then(|id| self.project.billing().invoice(id))
                    .map(|invoice| invoice.number.trim().replace(['/', '\\', ' '], "-"))
                    .filter(|number| !number.is_empty());
                // Kostenermittlung: Stufe und Bezeichnung („Kostenberechnung_LPH-3“).
                let costs = self
                    .pending_print
                    .as_ref()
                    .filter(|(job, _)| job.kind == PrintKind::CostEstimate)
                    .and_then(|(job, _)| job.cost_estimate)
                    .and_then(|id| self.project.cost_estimate(id))
                    .map(|estimate| {
                        let title = estimate.title.trim().replace(['/', '\\', ' '], "-");
                        if title.is_empty() { estimate.stage.label().to_string() } else { title }
                    });
                let label = match (number, costs) {
                    (_, Some(costs)) => costs,
                    (Some(number), None) => format!("{}_{number}", kind.file_label()),
                    (None, None) => kind.file_label().to_string(),
                };
                FileDialog::save(
                    &self.launcher,
                    kind.label(),
                    "PDF zum Drucken oder Versenden.",
                    folder,
                    &file_dialog::suggested_name(&name, &label, &today, "pdf"),
                    "pdf",
                )
            }
            FilePurpose::InsertImages | FilePurpose::LetterImages => FileDialog::open(
                &self.launcher,
                "Bild einfügen",
                "PNG oder JPEG – mit Strg+Klick mehrere wählen.",
                folder,
                &strings::IMAGE_EXTENSIONS,
                true,
            ),
            FilePurpose::ReadOnlyProject => FileDialog::open(
                &self.launcher,
                "Projekt schreibgeschützt öffnen",
                "Öffnet in einem neuen Fenster, ohne Speichern – zum Nachsehen und Kopieren (Strg+C hier, Strg+V dort).",
                folder,
                &["proava"],
                false,
            ),
            FilePurpose::TransferSource => self.pick_transfer_source(),
            FilePurpose::TransferSearchFolder => FileDialog::folder(
                &self.launcher,
                "Projekte unter diesem Ordner durchsuchen",
                folder,
            ),
            FilePurpose::CompareSource => self.pick_compare_source(),
            FilePurpose::ComparePdf => self.compare_pdf_dialog(),
            FilePurpose::ExportLibrary => FileDialog::save(
                &self.launcher,
                "Bibliothek als GAEB X81",
                "Leistungsbeschreibung ohne Mengen und Preise.",
                folder,
                &file_dialog::suggested_name(&name, "Bibliothek", &today, "X81"),
                "X81",
            ),
            FilePurpose::StlbCheckFile => FileDialog::save(
                &self.launcher,
                "STLB-Bau-Positionen zum Prüfen",
                "Nur die Positionen mit STLB-Bau-Kennung (mit OZ und Titeln) als X81 – in STLB-Bau einlesen, prüfen oder aktualisieren und wieder als GAEB speichern.",
                folder,
                &file_dialog::suggested_name(&name, "STLB-Bau_Pruefen", &today, "X81"),
                "X81",
            ),
            FilePurpose::StlbUpdateSource => FileDialog::open(
                &self.launcher,
                "Aktualisierte Positionen aus STLB-Bau",
                "Die GAEB-Datei, die STLB-Bau nach dem Prüfen oder Aktualisieren gespeichert hat.",
                stlb_dialog::system_download_folders(&self.launcher)
                    .into_iter()
                    .find(|candidate| self.launcher.is_folder(candidate))
                    .unwrap_or(folder),
                &stlb_dialog::CATALOG_EXTENSIONS,
                false,
            ),
            FilePurpose::ChangeOrder(file) => self.change_order_dialog(file),
            FilePurpose::SaveFiledLetter(_) | FilePurpose::AddFiledLetters => self.filed_letter_dialog(purpose),
        };
        let dialog = dialog.with_place("Projektordner", &self.project_folder());
        self.dialog = Some(Dialog::File { dialog, purpose });
    }

    /// The PDF files of „ein PDF je Bieter“ in `folder`, in the pattern of
    /// the single PDF: <Projekt>_<Art>_<JJJJ-MM-TT>_<Bieter>.pdf.
    fn letter_files(
        &self,
        kind: proava_domain::LetterKind,
        recipients: &[proava_domain::InviteeId],
        folder: &Path,
    ) -> Vec<(proava_domain::InviteeId, PathBuf)> {
        let stem = file_dialog::suggested_name(
            self.project.name().as_str(),
            kind.file_label(),
            &self.launcher.now_iso(),
            "",
        );
        letter_files_in(&self.project, &stem, recipients, folder)
    }

    fn file_chosen(&mut self, ctx: &egui::Context, purpose: FilePurpose, files: Vec<PathBuf>) {
        match purpose {
            FilePurpose::ChangeOrder(file) => self.change_order_file_chosen(ctx, file, files),
            FilePurpose::SaveFiledLetter(_) | FilePurpose::AddFiledLetters => {
                self.filed_letter_file_chosen(ctx, purpose, files)
            }
            FilePurpose::ExportGaeb(kind) => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                let lv = self.project.active_lv();
                let options = proava_application::GaebExportOptions {
                    images: self.launcher.gaeb_images(),
                };
                self.start_job(
                    ctx,
                    format!("{} wird erzeugt …", kind.extension()),
                    move || {
                        let result = exchange.export_gaeb_with(&project, kind, &file, options);
                        JobOutcome::Gaeb {
                            kind,
                            file,
                            result,
                            lv,
                        }
                    },
                );
            }
            FilePurpose::ExportCsv(encoding) => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                self.start_job(ctx, "CSV wird geschrieben …".to_string(), move || {
                    let result = exchange.export_csv(&project, &file, encoding);
                    JobOutcome::Csv { file, result }
                });
            }
            FilePurpose::ExportTrackingCsv(encoding) => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let report = self.tracking.report(&self.project).clone();
                self.start_job(ctx, "CSV wird geschrieben …".to_string(), move || {
                    let result = exchange.export_tracking_csv(&report, &file, encoding);
                    JobOutcome::Csv { file, result }
                });
            }
            FilePurpose::ImportBids => {
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                let label = format!(
                    "{} werden eingelesen …",
                    crate::format::count(files.len(), "Angebot", "Angebote")
                );
                self.start_job(ctx, label, move || JobOutcome::Bids {
                    results: files
                        .iter()
                        .map(|file| {
                            (
                                file_dialog::file_name(file),
                                exchange.import_bid(&project, file),
                            )
                        })
                        .collect(),
                });
            }
            FilePurpose::ImportContract => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                self.start_job(ctx, "Auftrag wird eingelesen …".to_string(), move || {
                    let result = exchange.import_contract(&project, &file).map(Box::new);
                    JobOutcome::ContractRead { file, result }
                });
            }
            FilePurpose::ImportInvoice(target) => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                self.start_job(ctx, "Rechnung wird eingelesen …".to_string(), move || {
                    let result = exchange.import_invoice(&project, &file).map(Box::new);
                    JobOutcome::InvoiceRead {
                        file,
                        target,
                        result,
                    }
                });
            }
            FilePurpose::ExportContract(bid, kind) => {
                let Some(file) = files.first().cloned() else {
                    return;
                };
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                self.start_job(
                    ctx,
                    format!("{} wird erzeugt …", kind.label()),
                    move || {
                        let result = exchange.export_contract(&project, bid, kind, &file);
                        JobOutcome::Contract { kind, file, result }
                    },
                );
            }
            FilePurpose::Letters => {
                let Some(target) = files.first().cloned() else {
                    return;
                };
                // One PDF per bidder into a folder: ask once for all files
                // that are there already (Befund 6).
                if let Some((kind, recipients, true)) = &self.pending_letters {
                    let existing: Vec<PathBuf> = self
                        .letter_files(*kind, recipients, &target)
                        .into_iter()
                        .map(|(_, file)| file)
                        .filter(|file| self.launcher.is_file(file))
                        .collect();
                    if !existing.is_empty() {
                        self.dialog = Some(Dialog::ReplaceLetters {
                            question: Box::new(ReplaceQuestion::new(&self.launcher, existing)),
                            target,
                        });
                        return;
                    }
                }
                self.write_letters(ctx, target);
            }
            FilePurpose::Print => {
                let (Some(file), Some((mut job, open))) =
                    (files.first().cloned(), self.pending_print.take())
                else {
                    return;
                };
                job.file_name = file_dialog::file_name(&file);
                let exchange = self.launcher.shared_exchange();
                let project = self.project.clone();
                let label = job.kind.label();
                self.start_job(ctx, format!("{label} wird erzeugt …"), move || {
                    let result = exchange.export_print(&project, &job, &file);
                    JobOutcome::Print {
                        label,
                        file,
                        open,
                        result,
                    }
                });
            }
            FilePurpose::LetterImages => self.insert_letter_images(ctx, &files),
            FilePurpose::InsertImages => {
                let output =
                    self.editor
                        .insert_image_files(&mut self.project, &self.launcher, &files);
                self.take_editor_output(ctx, output);
            }
            FilePurpose::ReadOnlyProject => {
                if let Some(path) = files.into_iter().next() {
                    let arguments = [
                        std::ffi::OsString::from("--schreibgeschuetzt"),
                        path.clone().into_os_string(),
                    ];
                    match jobs::new_window(&arguments) {
                        Ok(()) => self.say(
                            ctx,
                            format!(
                                "{} wird schreibgeschützt in einem neuen Fenster geöffnet …",
                                file_dialog::file_name(&path)
                            ),
                        ),
                        Err(error) => self.say(ctx, error),
                    }
                }
            }
            FilePurpose::TransferSource => {
                if let Some(path) = files.into_iter().next() {
                    self.load_transfer_source(path, false);
                }
            }
            FilePurpose::TransferSearchFolder => {
                if let Some(folder) = files.into_iter().next() {
                    self.start_transfer_search(ctx, folder);
                }
            }
            FilePurpose::CompareSource => {
                if let Some(path) = files.into_iter().next() {
                    self.load_compare_source(path);
                }
            }
            FilePurpose::ComparePdf => {
                if let Some(file) = files.into_iter().next() {
                    self.export_compare_pdf(ctx, file);
                }
            }
            FilePurpose::ExportLibrary => {
                if let Some(file) = files.into_iter().next() {
                    self.write_library_export(ctx, file);
                }
            }
            FilePurpose::StlbCheckFile => {
                if let Some(file) = files.into_iter().next() {
                    self.export_stlb_check(ctx, file);
                }
            }
            FilePurpose::StlbUpdateSource => {
                if let Some(file) = files.into_iter().next() {
                    self.read_stlb_update(ctx, &file);
                }
            }
        }
    }

    /// Writes the STLB-Bau positions of the LV (with titles, OZ as in the LV)
    /// as X81 for checking or updating in STLB-Bau.
    fn export_stlb_check(&mut self, ctx: &egui::Context, file: PathBuf) {
        let mut project = self.project.clone();
        let reduced = proava_domain::stlb_update::positions_for_check(self.project.lv());
        let count = reduced.positions().count();
        *project.lv_mut() = reduced;
        match self
            .launcher
            .shared_exchange()
            .export_gaeb(&project, GaebExport::Description, &file)
        {
            Ok(_) => self.say_with_file(
                ctx,
                format!(
                    "{} zum Prüfen gespeichert: {}",
                    crate::format::count(count, "STLB-Bau-Position", "STLB-Bau-Positionen"),
                    file_dialog::file_name(&file)
                ),
                Some(file),
            ),
            Err(error) => self.say(ctx, error),
        }
    }

    /// Reads the file STLB-Bau wrote after checking/updating and shows what
    /// would change.
    fn read_stlb_update(&mut self, ctx: &egui::Context, file: &Path) {
        let read = match self.launcher.exchange().read_catalog_positions(file) {
            Ok(read) => read,
            Err(error) => {
                self.say(ctx, error);
                return;
            }
        };
        // Only what the file delivered as positions (texts are not updated).
        let delivered = read.delivered_positions();
        let plan = proava_domain::stlb_update::plan_update(self.project.lv(), &delivered);
        if plan.updates.is_empty() {
            self.say(
                ctx,
                format!(
                    "Keine Änderungen aus STLB-Bau: {} unverändert, {} ohne Partner im LV.",
                    crate::format::count(plan.unchanged, "Position", "Positionen"),
                    plan.unmatched.len()
                ),
            );
            return;
        }
        self.tab = Tab::Lv;
        self.dialog = Some(Dialog::StlbUpdate(Box::new(StlbUpdateDialog::new(
            file_dialog::file_name(file),
            plan,
            read.warnings,
        ))));
    }

    /// Takes the chosen updates (from the dialog) into the LV.
    fn apply_stlb_update(
        &mut self,
        ctx: &egui::Context,
        dialog: &StlbUpdateDialog,
        chosen: &[usize],
    ) {
        let before = self.project.lv().clone();
        let updates = chosen
            .iter()
            .filter_map(|index| dialog.plan().updates.get(*index));
        let count = proava_domain::stlb_update::apply_updates(self.project.lv_mut(), updates);
        if count > 0 {
            self.editor.record(&before);
            self.editor.reload();
            self.dirty = true;
        }
        self.say(
            ctx,
            format!(
                "{} aktualisiert, Status offen.",
                crate::format::count(count, "STLB-Bau-Position", "STLB-Bau-Positionen")
            ),
        );
    }

    /// Remembers in the LV that its Angebotsaufforderung went out (warning
    /// before renumbering).
    /// A saved project stays saved: the date is written to the file at once
    /// (the export changed nothing the user would have to save).
    fn mark_request_exported(&mut self, lv: usize, file: &std::path::Path) {
        let now = self.launcher.now_iso();
        let active = self.project.active_lv();
        if self.project.set_active_lv(lv) {
            // Sent again: the state of now is what the bidders have.
            self.project
                .lv_mut()
                .mark_sent(now, Some(file.display().to_string()));
            self.sent_confirmed.remove(&self.active_key());
            self.project.set_active_lv(active);
            if self.dirty {
                return;
            }
            let mut copy = self.project.clone();
            copy.remove_unused_images();
            if self
                .launcher
                .save_project(&self.project_file, &copy)
                .is_err()
            {
                self.dirty = true;
            }
        }
    }

    fn save_now(&mut self, ctx: &egui::Context) {
        if let Err(problem) = self.try_save(ctx) {
            self.say(ctx, problem);
        }
    }

    /// Saves the project; on failure the reason (status bar or dialog).
    fn try_save(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.read_only {
            return Err(
                "Schreibgeschützt geöffnet – Speichern ist gesperrt. Zeilen mit Strg+C kopieren und im anderen Fenster einfügen."
                    .to_string(),
            );
        }
        self.wait_for_autosave(ctx);
        let mut copy = self.project.clone();
        copy.remove_unused_images();
        match self.launcher.save_project(&self.project_file, &copy) {
            Ok(()) => {
                self.dirty = false;
                self.dirty_since = None;
                let _ = self.launcher.discard_autosave(&self.project_file);
                self.say(ctx, "Gespeichert.");
                // A library edited here: the side window shows the new state.
                if self.launcher.library_at(&self.project_file).is_some() {
                    self.reload_library();
                }
                Ok(())
            }
            Err(error) => Err(format!("Speichern fehlgeschlagen: {error}")),
        }
    }

    fn backup(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.save_now(ctx);
            if self.dirty {
                return;
            }
        }
        match self
            .launcher
            .backup(&self.project_file, &self.project_folder())
        {
            Ok(file) => self.say(
                ctx,
                format!("Sicherung angelegt: {}", file_dialog::file_name(&file)),
            ),
            Err(error) => self.say(ctx, format!("Sicherung fehlgeschlagen: {error}")),
        }
    }

    // ----- Dialoge -----

    fn dialogs(&mut self, ui: &mut egui::Ui) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let ctx = ui.ctx().clone();
        let palette = self.palette;
        let keep = match &mut dialog {
            Dialog::File {
                dialog: files,
                purpose,
            } => match files.show(ui, &palette, &self.launcher) {
                DialogResult::Open => true,
                DialogResult::Cancelled => {
                    if matches!(
                        purpose,
                        FilePurpose::SaveFiledLetter(_) | FilePurpose::AddFiledLetters
                    ) {
                        self.filed_letter_cancelled();
                        return;
                    }
                    if matches!(purpose, FilePurpose::LetterImages | FilePurpose::Letters) {
                        // Back to the Schriftverkehr dialog.
                        self.pending_letters = None;
                        self.dialog = self.parked_letters.take().map(Dialog::Letters);
                        return;
                    }
                    false
                }
                DialogResult::Chosen(chosen) => {
                    let purpose = *purpose;
                    if let (FilePurpose::ExportGaeb(_), Some(images)) = (purpose, files.option()) {
                        // Gemerkt für den nächsten Export.
                        self.launcher.set_gaeb_images(images);
                    }
                    let purpose = match (purpose, files.option()) {
                        (FilePurpose::ExportCsv(_), Some(windows)) => {
                            // Gemerkt für den nächsten Export.
                            self.launcher.set_csv_windows_1252(windows);
                            FilePurpose::ExportCsv(if windows {
                                CsvEncoding::Windows1252
                            } else {
                                CsvEncoding::Utf8Bom
                            })
                        }
                        (FilePurpose::ExportTrackingCsv(_), Some(windows)) => {
                            self.launcher.set_csv_windows_1252(windows);
                            FilePurpose::ExportTrackingCsv(if windows {
                                CsvEncoding::Windows1252
                            } else {
                                CsvEncoding::Utf8Bom
                            })
                        }
                        (other, _) => other,
                    };
                    self.file_chosen(&ctx, purpose, chosen);
                    return;
                }
            },
            Dialog::ReplaceLetters { question, target } => match question.show(ui, &palette) {
                ReplaceAnswer::Open => true,
                ReplaceAnswer::Replace => {
                    let target = target.clone();
                    self.write_letters(&ctx, target);
                    return;
                }
                ReplaceAnswer::Cancel => {
                    // Back to the Schriftverkehr dialog, nothing written.
                    self.pending_letters = None;
                    self.dialog = self.parked_letters.take().map(Dialog::Letters);
                    return;
                }
            },
            Dialog::Leave { error } => self.leave_dialog(ui, &ctx, error),
            Dialog::Import(import) => self.import_step(ui, &ctx, import),
            Dialog::SaveTemplate(form) => self.save_template_step(ui, &ctx, form),
            Dialog::Recover { file, time } => {
                let mut choice = None;
                let (_, close) = widgets::modal(ui, &palette, 540.0, |ui| {
                    widgets::title(
                        ui,
                        &palette,
                        "Wiederherstellungsdatei gefunden",
                        &format!(
                            "Automatisch gesichert am {} – neuer als das gespeicherte Projekt. \
                             Vermutlich wurde proAVA nicht regulär beendet.",
                            crate::format::iso_to_german_date(time)
                                + " "
                                + time.get(11..16).unwrap_or_default()
                        ),
                    );
                    widgets::button_row(ui, |ui| {
                        if widgets::button(ui, &palette, "Wiederherstellen", true, 150.0).clicked()
                        {
                            choice = Some(true);
                        }
                        ui.add_space(8.0);
                        if widgets::danger_button(ui, &palette, "Verwerfen", 120.0).clicked() {
                            choice = Some(false);
                        }
                    });
                });
                match choice {
                    Some(true) => {
                        match self.launcher.load_autosave(file) {
                            Ok(project) => {
                                self.project = project;
                                self.history = History::default();
                                self.offers_baseline = None;
                                let mut fresh = LvEditor::new_undo_stack();
                                self.editor.swap_undo(&mut fresh);
                                let lv = self.project.lv().clone();
                                self.editor.show_lv(&lv, None);
                                self.dirty = true;
                                self.say(&ctx, "Stand aus der Wiederherstellungsdatei geladen – bitte speichern.");
                            }
                            Err(error) => {
                                self.say(&ctx, format!("Wiederherstellen fehlgeschlagen: {error}"))
                            }
                        }
                        false
                    }
                    Some(false) => {
                        self.wait_for_autosave(&ctx);
                        let _ = self.launcher.discard_autosave(&self.project_file);
                        false
                    }
                    None => !close,
                }
            }
            Dialog::Letters(letters) => {
                // Strg+Z/Y in the dialog: only template steps (also typing in the text).
                let (undo, redo) = ctx.input_mut(|input| {
                    let command = egui::Modifiers::COMMAND;
                    let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
                    let redo = self.history.letters_next(true)
                        && (input.consume_key(shift, egui::Key::Z)
                            || input.consume_key(command, egui::Key::Y));
                    let undo = self.history.letters_next(false)
                        && input.consume_key(command, egui::Key::Z);
                    (undo, redo)
                });
                if undo {
                    self.undo(&ctx);
                }
                if redo {
                    self.redo(&ctx);
                }
                let mut dirty = false;
                let before = self.project.letter_templates().to_vec();
                let result = letters.show(ui, &palette, &mut self.project, &mut dirty);
                if dirty {
                    self.dirty = true;
                }
                if self.project.letter_templates() != before.as_slice() {
                    self.note_editor();
                    let now = ctx.input(|input| input.time);
                    self.history.record_letters(before, now);
                }
                match result {
                    LettersResult::Open => true,
                    LettersResult::Closed => false,
                    LettersResult::ShowFiled => {
                        self.open_filed_letters(None);
                        return;
                    }
                    LettersResult::ExportRelease {
                        invoice,
                        file_in_project,
                    } => {
                        self.pending_letters =
                            Some((proava_domain::LetterKind::Release, Vec::new(), false));
                        self.pending_release_letter = Some(invoice);
                        self.pending_file_letters = file_in_project;
                        if let Dialog::Letters(letters) = dialog {
                            self.parked_letters = Some(letters);
                        }
                        self.open_letters_print(None, file_in_project);
                        return;
                    }
                    LettersResult::Export {
                        kind,
                        recipients,
                        separate,
                        file_in_project,
                    } => {
                        self.pending_release_letter = None;
                        self.pending_letters = Some((kind, recipients, separate));
                        self.pending_file_letters = file_in_project;
                        if let Dialog::Letters(letters) = dialog {
                            self.parked_letters = Some(letters);
                        }
                        self.open_letters_print(Some(separate), file_in_project);
                        return;
                    }
                    LettersResult::PickImage => {
                        if let Dialog::Letters(letters) = dialog {
                            self.parked_letters = Some(letters);
                        }
                        self.open_file_dialog(FilePurpose::LetterImages);
                        return;
                    }
                }
            }
            Dialog::Correspondence(list) => self.correspondence_step(ui, &ctx, list),
            Dialog::ContextPrint(print, target) => {
                let target = *target;
                self.context_print_step(ui, &palette, print, target)
            }
            Dialog::Print(print) => {
                let comparison = self.comparison.print_settings(&self.project);
                match print.show(ui, &palette, &self.project, &comparison) {
                    PrintDialogResult::Open => true,
                    PrintDialogResult::Cancelled => {
                        self.launcher
                            .set_print_settings(print.kind(), print.settings());
                        false
                    }
                    PrintDialogResult::KindChanged(old, new) => {
                        self.launcher.set_print_settings(old, print.settings());
                        print.load(self.launcher.print_settings(new));
                        true
                    }
                    PrintDialogResult::CountPages(mut job) => {
                        job.logo = self.print_logo(&ctx);
                        job.frame = self.launcher.preferences().page_frame;
                        job.cover_blocks = self.launcher.preferences().cover_blocks;
                        job.look = self.launcher.preferences().print_look;
                        job.author = self.launcher.preferences().print_author;
                        let exchange = self.launcher.shared_exchange();
                        let project = self.project.clone();
                        self.start_job(&ctx, "Seiten werden gezählt …".to_string(), move || {
                            JobOutcome::PrintPages(exchange.print_page_count(&project, &job))
                        });
                        true
                    }
                    PrintDialogResult::EditFrame => {
                        self.launcher
                            .set_print_settings(print.kind(), print.settings());
                        let mut form = MasterDataForm::of(&self.project);
                        form.frame_default = self.launcher.preferences().page_frame;
                        form.tab = crate::master_data_view::MasterTab::PageFrame;
                        self.dialog = Some(Dialog::MasterData(Box::new(form)));
                        false
                    }
                    PrintDialogResult::Print(mut job) => {
                        job.logo = self.print_logo(&ctx);
                        job.frame = self.launcher.preferences().page_frame;
                        job.cover_blocks = self.launcher.preferences().cover_blocks;
                        job.look = self.launcher.preferences().print_look;
                        job.author = self.launcher.preferences().print_author;
                        self.launcher.set_print_settings(job.kind, &job.settings);
                        self.pending_print = Some((job, print.open_after));
                        self.open_file_dialog(FilePurpose::Print);
                        false
                    }
                }
            }
            Dialog::Mask(mask_dialog) => {
                let lv = self.project.lv().clone();
                let mut changed: Option<proava_domain::Lv> = None;
                let result = mask_dialog.show(ui, &palette, &lv, |spec, step, numbering| {
                    let mut copy = lv.clone();
                    let message = crate::struct_ops::apply_mask(&mut copy, spec, step, numbering)?;
                    changed = Some(copy);
                    Ok(message)
                });
                match result {
                    MaskDialogResult::Open => true,
                    MaskDialogResult::Cancelled => false,
                    MaskDialogResult::Applied(message) => {
                        if let Some(new_lv) = changed {
                            if new_lv != lv {
                                if lv.request_exported().is_some() {
                                    // The dialog warned about the sent X83 already.
                                    self.sent_confirmed.insert(self.active_key());
                                }
                                self.editor.record(self.project.lv());
                                *self.project.lv_mut() = new_lv;
                                self.editor.reload();
                                self.dirty = true;
                            }
                        }
                        self.say(&ctx, message);
                        false
                    }
                }
            }
            Dialog::StlbUpdate(update) => match update.show(ui, &palette) {
                StlbUpdateResult::Open => true,
                StlbUpdateResult::Cancelled => false,
                StlbUpdateResult::Apply(chosen) => {
                    self.apply_stlb_update(&ctx, update, &chosen);
                    false
                }
            },
            Dialog::Stlb(stlb) => {
                // Strg+Z in the dialog takes back its last take-over (the
                // dialog says so after each one); other steps wait until it
                // is closed.
                if stlb.can_undo()
                    && !ctx.text_edit_focused()
                    && ctx.input_mut(|input| {
                        input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
                    })
                {
                    self.undo(&ctx);
                    stlb.undone();
                }
                let target = self.stlb_target();
                match stlb.show(ui, &palette, &self.launcher, &target) {
                    StlbResult::Open => true,
                    StlbResult::Cancelled => false,
                    StlbResult::Apply(positions) => {
                        // The dialog stays open for the next position.
                        let outcome = self.take_over_stlb(&ctx, stlb.source(), positions);
                        stlb.note(outcome);
                        true
                    }
                    StlbResult::ToLibrary(file) => {
                        let origins = stlb.origins();
                        self.text_source_to_library(&ctx, &file, origins);
                        false
                    }
                }
            }
            Dialog::MasterData(form) => {
                let ((cancelled, confirmed), close) =
                    widgets::modal(ui, &palette, 760.0, |ui| form.show(ui, &palette));
                if close || cancelled {
                    false
                } else if confirmed {
                    match form.to_info() {
                        Ok(info) => {
                            if info != *self.project.info() {
                                let before = self.project.info().clone();
                                self.record_project(
                                    ProjectChange::Info(Box::new(before)),
                                    "Projektstammdaten",
                                );
                                *self.project.info_mut() = info;
                                self.dirty = true;
                                self.say(&ctx, "Projektstammdaten übernommen.");
                            }
                            false
                        }
                        Err(problem) => {
                            if let Some(tab) = form.errors().first_tab() {
                                form.tab = tab;
                            }
                            form.error = Some(problem);
                            true
                        }
                    }
                } else {
                    true
                }
            }
            Dialog::LvProperties(form, assistant) => {
                if std::mem::take(&mut form.wants_assistant) {
                    let sample = form.sample_lv();
                    *assistant = Some(Box::new(OzMaskDialog::of(&sample, 0)));
                }
                if let Some(outline) = assistant {
                    let sample = form.sample_lv();
                    let mut shaped: Option<proava_domain::Lv> = None;
                    let result = outline.show(ui, &palette, &sample, |spec, step, numbering| {
                        let mut copy = sample.clone();
                        let message =
                            crate::struct_ops::apply_mask(&mut copy, spec, step, numbering)?;
                        shaped = Some(copy);
                        Ok(message)
                    });
                    match result {
                        MaskDialogResult::Open => {}
                        MaskDialogResult::Cancelled => *assistant = None,
                        MaskDialogResult::Applied(_) => {
                            if let Some(lv) = shaped {
                                form.take_outline(&lv);
                            }
                            *assistant = None;
                        }
                    }
                    if self.dialog.is_none() {
                        self.dialog = Some(dialog);
                    }
                    return;
                }
                let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                let ((cancelled, confirmed), close) =
                    widgets::modal(ui, &palette, 560.0, |ui| form.show(ui, &palette));
                if close || cancelled {
                    false
                } else if confirmed || enter {
                    match self.take_lv_properties(&ctx, form) {
                        Ok(()) => false,
                        Err(problem) => {
                            form.error = Some(problem);
                            true
                        }
                    }
                } else {
                    true
                }
            }
            Dialog::RevokeAward {
                question,
                blocked,
                with_contract,
            } => {
                let (blocked, with_contract) = (*blocked, *with_contract);
                let ((cancelled, confirmed), close) = widgets::modal(ui, &palette, 560.0, |ui| {
                    if blocked {
                        widgets::title(
                            ui,
                            &palette,
                            "Auftrag lässt sich nicht zurücknehmen",
                            question,
                        );
                        ui.add_space(8.0);
                        let close = ui
                            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                widgets::accent_button(ui, &palette, "Schließen")
                            })
                            .inner;
                        (close, false)
                    } else {
                        widgets::title(ui, &palette, "Auftrag zurücknehmen?", question);
                        widgets::danger_dialog_buttons(ui, &palette, "Zurücknehmen")
                    }
                });
                if confirmed {
                    self.revoke_award(&ctx, with_contract);
                    false
                } else {
                    !(cancelled || close)
                }
            }
            Dialog::DeleteLv { index, question } => {
                let index = *index;
                let ((cancelled, confirmed), close) = widgets::modal(ui, &palette, 520.0, |ui| {
                    widgets::title(ui, &palette, "LV löschen?", question);
                    widgets::danger_dialog_buttons(ui, &palette, "Löschen")
                });
                if confirmed {
                    let before = self.project.lv_list_snapshot();
                    self.note_editor();
                    let previous = self.active_key();
                    match self.project.remove_lv(index) {
                        Ok((lv, _)) => {
                            self.history.record_project(
                                ProjectChange::LvList(Box::new(before)),
                                "LV löschen",
                            );
                            self.follow_active_lv(previous);
                            self.dirty = true;
                            self.say(&ctx, format!("{} gelöscht.", lv_properties::lv_label(&lv)));
                        }
                        Err(problem) => self.say(&ctx, problem),
                    }
                    false
                } else {
                    !(cancelled || close)
                }
            }
            Dialog::PasteLv {
                lv,
                images,
                summary,
                into_table,
            } => {
                let into_table = *into_table;
                let mut choice: Option<bool> = None;
                let (cancel, close) = widgets::modal(ui, &palette, 560.0, |ui| {
                    let (title, hint) = if into_table {
                        (
                            "Inhalt eines LV einfügen?",
                            format!(
                                "In der Zwischenablage liegt {summary}. Seine Titel und Positionen kommen unter die gewählte Zeile; Angebote nie."
                            ),
                        )
                    } else {
                        (
                            "LV als neues LV einfügen?",
                            format!(
                                "In der Zwischenablage liegt {summary}. Es wird als neues LV mit der nächsten freien LV-Nummer angelegt (Maske, USt und Texte mit, ohne Angebote)."
                            ),
                        )
                    };
                    widgets::title(ui, &palette, title, &hint);
                    ui.add_space(8.0);
                    let mut cancel = false;
                    widgets::button_row(ui, |ui| {
                        if widgets::button(ui, &palette, "Mit Planerpreisen", true, 180.0).clicked()
                        {
                            choice = Some(true);
                        }
                        ui.add_space(8.0);
                        if widgets::button(ui, &palette, "Ohne Preise", false, 140.0).clicked() {
                            choice = Some(false);
                        }
                        ui.add_space(8.0);
                        if widgets::button(ui, &palette, strings::CANCEL, false, 120.0).clicked() {
                            cancel = true;
                        }
                    });
                    cancel
                });
                if let Some(prices) = choice {
                    let lv = (**lv).clone();
                    let images = std::mem::take(images);
                    self.paste_lv(&ctx, lv, images, into_table, prices);
                    false
                } else {
                    !(cancel || close)
                }
            }
            Dialog::Settings(settings) => match settings.show(ui, &palette, &self.launcher) {
                SettingsResult::Open => true,
                SettingsResult::Cancelled => false,
                SettingsResult::Applied => {
                    self.preferences = self.launcher.preferences();
                    self.theme_choice = self.launcher.theme();
                    self.palette = self.palette_of_settings();
                    theme::apply(&ctx, self.theme_choice);
                    self.comparison = new_comparison(&self.preferences);
                    self.say(&ctx, "Einstellungen übernommen.");
                    false
                }
            },
            Dialog::Check { findings, export } => {
                let export = *export;
                let mut jump = None;
                let mut go_on = false;
                let mut to_spacers = false;
                let mut to_free = false;
                let blank_notes = self.project.lv().blank_notes().len();
                let changed_stlb =
                    crate::lv_editor::changed_stlb_positions(self.project.lv()).len();
                let (cancelled, close) = widgets::modal(ui, &palette, 620.0, |ui| {
                    let title = if export.is_some() {
                        strings::CHECK_TITLE
                    } else {
                        strings::CHECK_TITLE_LV
                    };
                    let summary = format!(
                        "{} Fehler · {} · {} – Klick auf eine Zeile zeigt sie im LV.",
                        lv_check::count(findings, Severity::Error),
                        crate::format::count(
                            lv_check::count(findings, Severity::Warning),
                            "Warnung",
                            "Warnungen"
                        ),
                        crate::format::count(
                            lv_check::count(findings, Severity::Hint),
                            "Hinweis",
                            "Hinweise"
                        )
                    );
                    widgets::title(ui, &palette, title, &summary);
                    egui::ScrollArea::vertical()
                        .max_height(340.0)
                        .show(ui, |ui| {
                            if findings.is_empty() {
                                ui.label(
                                    RichText::new(strings::CHECK_OK)
                                        .size(13.5)
                                        .color(palette.text),
                                );
                            }
                            for finding in findings.iter() {
                                if finding_row(ui, &palette, finding).clicked() {
                                    jump = finding.row;
                                }
                            }
                        });
                    // Hints without text are mostly empty lines (GAEB imports
                    // from other programs): one click turns them into Abstandshalter.
                    if blank_notes > 0 {
                        ui.add_space(8.0);
                        if ui
                            .button(format!(
                                "{} ({blank_notes})",
                                Command::AllBlankNotesToSpacers.label()
                            ))
                            .on_hover_text(
                                "Leerzeile nur für die Bearbeitung, ohne Warnung – erscheint weder im Druck noch in GAEB (Strg+Z macht es rückgängig)",
                            )
                            .clicked()
                        {
                            to_spacers = true;
                        }
                    }
                    // STLB-Bau positions whose Langtext was changed: one click
                    // removes the key (Strg+Z takes it back).
                    if changed_stlb > 0 {
                        ui.add_space(8.0);
                        if ui
                            .button(format!(
                                "STLB-Bau* in freie Positionen umwandeln ({changed_stlb})"
                            ))
                            .on_hover_text(proava_domain::stlb::STLB_CONVERT_QUESTION)
                            .clicked()
                        {
                            to_free = true;
                        }
                    }
                    ui.add_space(12.0);
                    match export {
                        Some(_) => {
                            let (no, yes) =
                                widgets::dialog_buttons(ui, &palette, strings::CHECK_ANYWAY);
                            go_on = yes;
                            no
                        }
                        None => widgets::accent_button(ui, &palette, strings::DONE),
                    }
                });
                if to_spacers {
                    let output =
                        self.editor
                            .run(Command::AllBlankNotesToSpacers, &mut self.project, &ctx);
                    self.take_editor_output(&ctx, output);
                    *findings =
                        lv_check::check_with_rules(&self.project, export, self.launcher.exchange());
                    true
                } else if to_free {
                    let today = crate::date_picker::today(&ctx).to_string();
                    let output = self.editor.changed_stlb_to_free(&mut self.project, &today);
                    self.take_editor_output(&ctx, output);
                    *findings =
                        lv_check::check_with_rules(&self.project, export, self.launcher.exchange());
                    true
                } else if let Some(row) = jump {
                    self.tab = Tab::Lv;
                    self.editor.select(self.project.lv(), row);
                    false
                } else if go_on {
                    if let Some(kind) = export {
                        self.open_file_dialog(FilePurpose::ExportGaeb(kind));
                    }
                    return;
                } else {
                    !(cancelled || close)
                }
            }
            Dialog::CheckAll { groups } => {
                let mut jump = None;
                let all: Vec<&Finding> = groups.iter().flat_map(|group| &group.findings).collect();
                let count =
                    |severity: Severity| all.iter().filter(|f| f.severity == severity).count();
                let (done, close) = widgets::modal(ui, &palette, 680.0, |ui| {
                    let summary = format!(
                        "{} · {} Fehler · {} · {} – Klick auf eine Zeile zeigt sie im LV.",
                        crate::format::count(groups.len(), "LV", "LVs"),
                        count(Severity::Error),
                        crate::format::count(count(Severity::Warning), "Warnung", "Warnungen"),
                        crate::format::count(count(Severity::Hint), "Hinweis", "Hinweise")
                    );
                    widgets::title(ui, &palette, strings::CHECK_TITLE_ALL, &summary);
                    egui::ScrollArea::vertical()
                        .max_height(420.0)
                        .show(ui, |ui| {
                            for group in groups.iter() {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(&group.label)
                                        .size(13.5)
                                        .strong()
                                        .color(palette.text),
                                );
                                if group.findings.is_empty() {
                                    ui.label(
                                        RichText::new(strings::CHECK_OK)
                                            .size(13.0)
                                            .color(palette.text_dim),
                                    );
                                }
                                for finding in &group.findings {
                                    if finding_row(ui, &palette, finding).clicked() {
                                        jump = finding.row.map(|row| (group.index, row));
                                    }
                                }
                            }
                        });
                    ui.add_space(12.0);
                    widgets::accent_button(ui, &palette, strings::DONE)
                });
                if let Some((index, row)) = jump {
                    // Into the right LV (also one shown below another).
                    self.activate_lv(index);
                    self.tab = Tab::Lv;
                    self.editor.select(self.project.lv(), row);
                    self.editor.scroll_to_row(self.project.lv(), row);
                    false
                } else {
                    !(done || close)
                }
            }
            Dialog::Messages { protocol, copied } => {
                crate::protocol_view::dialog(ui, &palette, protocol, copied)
            }
            Dialog::ChangeQuestion(question) => self.change_question_dialog(ui, &ctx, question),
            Dialog::BidsRead {
                names,
                links,
                reports,
                failures,
                copied,
                focus,
                adopt,
            } => {
                let invitees: Vec<(proava_domain::InviteeId, String)> = self
                    .project
                    .invitees()
                    .iter()
                    .map(|invitee| (invitee.id, invitee.name().to_string()))
                    .collect();
                let width = crate::protocol_view::dialog_width(
                    ui,
                    reports
                        .iter()
                        .chain(failures.iter())
                        .flat_map(|report| report.entries.iter()),
                    640.0,
                );
                let max_height = (ctx.content_rect().height() * 0.6).max(220.0);
                let (done, close) = widgets::modal(ui, &palette, width, |ui| {
                    widgets::title(
                        ui,
                        &palette,
                        "Angebote eingelesen",
                        "Bieternamen hier anpassen – manche D84 nennen keinen Namen.",
                    );
                    egui::ScrollArea::vertical()
                        .id_salt("bids_read")
                        .max_height(max_height)
                        .show(ui, |ui| {
                            for (position, (_, name)) in names.iter_mut().enumerate() {
                                let mut first = *focus && position == 0;
                                widgets::focus_field(
                                    ui,
                                    &palette,
                                    &format!("Bieter {}", position + 1),
                                    name,
                                    &mut first,
                                );
                                if !invitees.is_empty() {
                                    if let Some(link) = links.get_mut(position) {
                                        crate::bidders_view::link_choice(
                                            ui, &palette, position, link, &invitees,
                                        );
                                    }
                                }
                                // What was read; the name is the field above.
                                if let Some(report) = reports.get(position) {
                                    ui.add_space(4.0);
                                    let entries: Vec<_> = report
                                        .entries
                                        .iter()
                                        .filter(|entry| entry.label != "Bieter")
                                        .cloned()
                                        .collect();
                                    crate::protocol_view::show(ui, &palette, &entries);
                                }
                                ui.add_space(8.0);
                            }
                            *focus = false;
                            for failure in failures.iter() {
                                crate::protocol_view::show(ui, &palette, &failure.entries);
                                ui.add_space(8.0);
                            }
                        });
                    ui.add_space(8.0);
                    ui.checkbox(adopt, strings::ADOPT_BIDDERS)
                        .on_hover_text(strings::ADOPT_BIDDERS_HINT);
                    ui.add_space(12.0);
                    let text = reports
                        .iter()
                        .chain(failures.iter())
                        .map(proava_application::Protocol::text)
                        .collect::<Vec<_>>()
                        .join("\n");
                    crate::protocol_view::buttons(ui, &palette, &text, copied)
                });
                let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
                if done || close || enter {
                    for ((id, _), link) in names.iter().zip(links.iter()) {
                        if let Some(invitee) = link {
                            if self.project.link_bid(*id, Some(*invitee)).is_ok() {
                                self.dirty = true;
                            }
                        }
                    }
                    for (id, name) in names.iter() {
                        let name = name.trim();
                        if let Some(bid) = self.project.bid_mut(*id) {
                            if !name.is_empty() && bid.name() != name {
                                bid.bidder_mut().name1 = name.to_string();
                                self.dirty = true;
                            }
                        }
                    }
                    if *adopt {
                        let (mut created, mut linked) = (0, 0);
                        for (id, _) in names.iter() {
                            match self.project.adopt_bidder(*id) {
                                Some((_, true)) => created += 1,
                                Some((_, false)) => linked += 1,
                                None => {}
                            }
                        }
                        if created + linked > 0 {
                            self.dirty = true;
                            let text = match (created, linked) {
                                (0, linked) => format!(
                                    "{} im Bieterverzeichnis zugeordnet.",
                                    crate::format::count(linked, "Angebot", "Angebote")
                                ),
                                (created, 0) => format!(
                                    "{} ins Bieterverzeichnis aufgenommen.",
                                    crate::format::count(created, "Bieter", "Bieter")
                                ),
                                (created, linked) => format!(
                                    "{} ins Bieterverzeichnis aufgenommen, {} zugeordnet.",
                                    crate::format::count(created, "Bieter", "Bieter"),
                                    crate::format::count(linked, "Angebot", "Angebote")
                                ),
                            };
                            self.say(&ctx, text);
                        }
                    }
                    self.offers_changed(&ctx, "Angebote einlesen");
                    false
                } else {
                    true
                }
            }
            Dialog::Palette {
                query,
                focus,
                index,
            } => {
                let actions = filter_actions(&palette_actions(), query);
                let (up, down, enter) = ui.input_mut(|input| {
                    (
                        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                        input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                        input.key_pressed(egui::Key::Enter),
                    )
                });
                if up {
                    *index = index.saturating_sub(1);
                }
                if down {
                    *index = (*index + 1).min(actions.len().saturating_sub(1));
                }
                *index = (*index).min(actions.len().saturating_sub(1));
                let mut chosen = None;
                let (_, close) = widgets::modal(ui, &palette, 560.0, |ui| {
                    widgets::title(
                        ui,
                        &palette,
                        "Befehl",
                        "Tippen filtert · ↑/↓ wählt · Enter führt aus · Esc schließt",
                    );
                    let response = ui.add(
                        egui::TextEdit::singleline(query)
                            .hint_text("z. B. „x83“, „titel“, „drucken“")
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(15.0)),
                    );
                    if std::mem::take(focus) {
                        response.request_focus();
                    }
                    if response.changed() {
                        *index = 0;
                    }
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            for (position, action) in actions.iter().enumerate() {
                                let row = palette_row(ui, &palette, *action, position == *index);
                                if position == *index && (up || down) {
                                    row.scroll_to_me(None);
                                }
                                if row.clicked() {
                                    chosen = Some(*action);
                                }
                            }
                        });
                });
                if enter {
                    chosen = actions.get(*index).copied();
                }
                if let Some(action) = chosen {
                    self.apply(&ctx, action);
                    return;
                }
                !close
            }
        };
        if keep && self.dialog.is_none() {
            self.dialog = Some(dialog);
        }
    }

    /// „Projekt verlassen“ / „proAVA beenden“: every answer closes the
    /// question – except a failed save, which shows its reason in it.
    fn leave_dialog(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        error: &mut Option<String>,
    ) -> bool {
        let palette = self.palette;
        let mut choice = None;
        let (title, save_label) = if self.restart_after_leave {
            (strings::WS_RESTART_TITLE, strings::WS_SAVE_AND_RESTART)
        } else if self.quit_after_leave {
            (strings::WS_QUIT_TITLE, strings::WS_SAVE_AND_QUIT)
        } else {
            (strings::WS_LEAVE_TITLE, strings::WS_SAVE_AND_LEAVE)
        };
        let read_only = self.read_only;
        // Own id: the window's X can bring it on top of another dialog.
        let id = egui::Id::unique("proava_leave_question");
        let (_, close) = widgets::modal_with_id(ui, &palette, id, 520.0, |ui| {
            let hint = if read_only {
                "Schreibgeschützt geöffnet – die Änderungen in diesem Fenster lassen sich nicht speichern."
            } else {
                strings::WS_LEAVE_HINT
            };
            widgets::title(ui, &palette, title, hint);
            widgets::error_line(ui, &palette, error.as_deref());
            widgets::button_row(ui, |ui| {
                if !read_only && widgets::button(ui, &palette, save_label, true, 170.0).clicked() {
                    choice = Some(LeaveChoice::Save);
                }
                ui.add_space(8.0);
                if widgets::danger_button(ui, &palette, strings::WS_DISCARD, 120.0).clicked() {
                    choice = Some(LeaveChoice::Discard);
                }
                ui.add_space(8.0);
                if widgets::button(ui, &palette, strings::CANCEL, false, 120.0).clicked() {
                    choice = Some(LeaveChoice::Cancel);
                }
            });
        });
        if close && choice.is_none() {
            // Esc or a click beside the question: like „Abbrechen“.
            choice = Some(LeaveChoice::Cancel);
        }
        match choice {
            None => true,
            Some(LeaveChoice::Cancel) => {
                // Stay in the project; the next request asks anew.
                self.quit_after_leave = false;
                self.open_after_leave = false;
                self.restart_after_leave = false;
                false
            }
            Some(LeaveChoice::Save) => match self.try_save(ctx) {
                Ok(()) => {
                    self.leave_now(ctx);
                    false
                }
                Err(problem) => {
                    *error = Some(problem);
                    true
                }
            },
            Some(LeaveChoice::Discard) => {
                if !self.read_only {
                    self.wait_for_autosave(ctx);
                    let _ = self.launcher.discard_autosave(&self.project_file);
                }
                self.leave = true;
                false
            }
        }
    }
}

/// Answer to „Projekt verlassen“ / „proAVA beenden“.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaveChoice {
    Save,
    Discard,
    Cancel,
}

impl Drop for WorkspaceScreen {
    /// Closing the project (back to the start screen, end of the program):
    /// the own lock goes, a foreign one stays.
    fn drop(&mut self) {
        if !self.read_only {
            self.launcher.unlock_project(&self.project_file);
        }
    }
}

/// A Preisspiegel view with the defaults from the settings.
fn new_comparison(preferences: &Preferences) -> ComparisonView {
    ComparisonView::with_defaults(
        preferences.outlier_percent,
        preferences.deviation_reference,
        preferences.undercut_percent,
    )
}

/// Width of a tab of the header ([`segment`]).
fn segment_width(text: &str, narrow: bool) -> f32 {
    if narrow {
        text.chars().count() as f32 * 7.6 + 14.0
    } else {
        text.chars().count() as f32 * 8.2 + 28.0
    }
}

/// How the header fits into the window: width for the project name and
/// how far it is compacted (0: everything; 1: „Befehl …“ only as symbol;
/// 2: tabs without counts; 3: „Speichern“ as symbol; 4: narrower tabs).
/// Step 1 is taken as soon as the name does not fit whole, the others when
/// it has less than a readable piece (100 px). When even that leaves no
/// room for the name, the tabs get a
/// row of their own under the first (`tabs_below`, e.g. half a 1920 screen):
/// the name and every tab stay, nothing is cut off.
struct HeaderRoom {
    title: f32,
    level: u8,
    /// Width of the area with the buttons on the right.
    right: f32,
    /// The tabs in their own row (the header gets [`TAB_ROW_HEIGHT`] higher).
    tabs_below: bool,
    /// Tabs „Angebote (n)“ and „Nachträge (n)“ with their counts.
    tab_counts: bool,
    narrow_tabs: bool,
}

impl HeaderRoom {
    fn tab_label(&self, name: &str, count: usize) -> String {
        if self.tab_counts {
            format!("{name} ({count})")
        } else {
            name.to_string()
        }
    }
}

/// The header's fixed parts measured (back button, tabs, buttons on the
/// right) for the width `width` inside its frame; the project name
/// gets the rest.
fn header_room(
    ui: &egui::Ui,
    width: f32,
    project: &Project,
    dirty: bool,
    bids: usize,
    change_orders: usize,
    // Labels of the tabs shown only when there is something (Kosten,
    // Kostenverfolgung).
    extra_tabs: &[&str],
) -> HeaderRoom {
    let spacing = ui.spacing().item_spacing.x;
    let text = |label: &str| {
        ui.painter()
            .layout_no_wrap(
                label.to_string(),
                FontId::proportional(12.5),
                egui::Color32::WHITE,
            )
            .size()
            .x
    };
    // Space after the name and the gap to „Start“ (no LV choice any more:
    // all LVs stand in the table; the spacing before „Start“ is the one
    // counted behind the last tab).
    let left = spacing + 16.0;
    // „Start“ before the tabs.
    let start = text(strings::WS_BACK) + 40.0 + spacing + 4.0;
    let tabs = |counts: bool, narrow: bool| -> f32 {
        let count = |name: &str, n: usize| {
            if counts {
                format!("{name} ({n})")
            } else {
                name.to_string()
            }
        };
        [
            strings::NAV_LV.to_string(),
            count(strings::NAV_BIDS, bids),
            strings::NAV_COMPARISON.to_string(),
            strings::NAV_BILLING.to_string(),
            count("Nachträge", change_orders),
        ]
        .iter()
        .map(String::as_str)
        .chain(extra_tabs.iter().copied())
        .map(|label| segment_width(label, narrow) + spacing)
        .sum::<f32>()
            + start
    };
    let right = |level: u8| -> f32 {
        let save = if level >= 3 { 30.0 } else { 104.0 };
        let search = if level >= 1 {
            30.0
        } else {
            text("Befehl …") + 40.0
        };
        // Four buttons with 6 px and the spacing between them.
        save + 30.0 + 30.0 + search + 3.0 * (6.0 + spacing)
    };
    let available = width - 8.0;
    // The name as shown (with „ *“ while unsaved) gets at least a readable
    // piece before the next step.
    let full = ui
        .painter()
        .layout_no_wrap(
            format!(
                "{}{}",
                project.name().as_str(),
                if dirty { " *" } else { "" }
            ),
            FontId::proportional(16.0),
            egui::Color32::WHITE,
        )
        .size()
        .x;
    let wanted = full.min(100.0);
    // `left` already holds the space behind the name.
    let title_at = |level: u8| available - left - tabs(level < 2, level >= 4) - right(level);
    for level in 0..=4u8 {
        let narrow = level >= 4;
        let title = title_at(level);
        // „Befehl …“ as a symbol (step 1) costs nothing but its text: it
        // keeps its text only while the whole name fits (Anwender 18.11.).
        let enough = if level == 0 {
            title >= full
        } else {
            title >= wanted
        };
        if enough {
            return HeaderRoom {
                title,
                level,
                right: right(level),
                tabs_below: false,
                tab_counts: level < 2,
                narrow_tabs: narrow,
            };
        }
    }
    // Tabs below: the first row has the back button, the name and the
    // buttons (as symbols).
    let right = right(3);
    let title = available - left + 16.0 - right;
    let (tab_counts, narrow_tabs) = if tabs(true, false) <= available {
        (true, false)
    } else if tabs(false, false) <= available {
        (false, false)
    } else {
        (false, true)
    };
    HeaderRoom {
        title: if title >= 40.0 { title } else { 0.0 },
        level: 3,
        right,
        tabs_below: true,
        tab_counts,
        narrow_tabs,
    }
}

fn segment(
    ui: &mut egui::Ui,
    palette: &Palette,
    text: &str,
    active: bool,
    narrow: bool,
) -> egui::Response {
    let width = segment_width(text, narrow);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, active, text));
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, egui::CornerRadius::same(6), palette.accent_soft);
        painter.line_segment(
            [
                rect.left_bottom() + Vec2::new(6.0, -1.0),
                rect.right_bottom() + Vec2::new(-6.0, -1.0),
            ],
            Stroke::new(2.5, palette.accent),
        );
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(6), palette.button_hover);
    }
    let font = FontId::proportional(14.0);
    let color = if active { palette.accent } else { palette.text };
    if active {
        widgets::bold_text(
            painter,
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            font,
            color,
        );
    } else {
        painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn finding_row(ui: &mut egui::Ui, palette: &Palette, finding: &Finding) -> egui::Response {
    const TEXT_LEFT: f32 = 84.0;
    const LINE: f32 = 28.0;
    let width = ui.available_width();
    // Long findings (Fachregeln with source) wrap inside the dialog instead
    // of running past its right edge (Abnahme qa3, 18.11.).
    let galley = ui.painter().layout(
        finding.text.clone(),
        FontId::proportional(13.0),
        palette.text,
        (width - TEXT_LEFT - 6.0).max(80.0),
    );
    let height = (galley.size().y + 10.0).max(LINE);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    // Named for screen readers and the UI tests.
    widgets::name_row(&response, &finding.text, false);
    let painter = ui.painter();
    if response.hovered() && finding.row.is_some() {
        painter.rect_filled(rect, egui::CornerRadius::same(5), palette.row_hover);
    }
    let color = match finding.severity {
        Severity::Error => palette.error,
        Severity::Warning => palette.warn,
        Severity::Hint => palette.accent,
    };
    // The badge stays at the first line.
    widgets::badge(
        painter,
        egui::pos2(rect.left() + 6.0, rect.top() + LINE / 2.0),
        finding.severity.label(),
        color.gamma_multiply(0.15),
        color,
    );
    let top = rect.top() + (height - galley.size().y) / 2.0;
    painter.galley(
        egui::pos2(rect.left() + TEXT_LEFT, top),
        galley,
        palette.text,
    );
    if finding.row.is_some() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn palette_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    action: ShellAction,
    active: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, egui::CornerRadius::same(5), palette.selected);
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(5), palette.row_hover);
    }
    painter.text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        Align2::LEFT_CENTER,
        action.label(),
        FontId::proportional(14.0),
        palette.text,
    );
    painter.text(
        rect.right_center() - Vec2::new(10.0, 0.0),
        Align2::RIGHT_CENTER,
        action.shortcut(),
        FontId::proportional(12.5),
        palette.text_dim,
    );
    response
}

/// `<folder>/<stem>_<Bieter>.pdf` for each recipient (stem ends with „.“
/// from `suggested_name` with an empty extension).
fn letter_files_in(
    project: &proava_domain::Project,
    stem: &str,
    recipients: &[proava_domain::InviteeId],
    folder: &Path,
) -> Vec<(proava_domain::InviteeId, PathBuf)> {
    let stem = stem.trim_end_matches('.');
    recipients
        .iter()
        .map(|id| {
            let name = project
                .invitees()
                .iter()
                .find(|invitee| invitee.id == *id)
                .map(|invitee| crate::letters_view::file_part(invitee.name()))
                .unwrap_or_else(|| "Bieter".into());
            (*id, folder.join(format!("{stem}_{name}.pdf")))
        })
        .collect()
}

/// Document part of the suggested CSV file name: „LV“ or „LV_<Nummer>“.
fn csv_document(lv: &proava_domain::Lv) -> String {
    match lv.number().trim() {
        "" => "LV".to_string(),
        number => format!("LV_{number}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_actions, palette_actions, ShellAction};
    use crate::lv_ops::Command;
    use proava_application::GaebExport;

    /// Abnahme qa3 (18.11.): lange Befunde (Fachregeln mit Quelle) liefen in
    /// „LV prüfen“ über den rechten Dialogrand – sie brechen jetzt um.
    #[test]
    fn long_findings_wrap_inside_the_dialog_width() {
        use crate::lv_check::{Finding, Severity};
        let ctx = eframe::egui::Context::default();
        crate::fonts::install(&ctx);
        let palette = crate::theme::Palette::of(proava_application::ThemeChoice::Light);
        let short = Finding {
            severity: Severity::Warning,
            text: "Titel 03 enthält keine Positionen.".into(),
            row: None,
        };
        let long = Finding {
            severity: Severity::Warning,
            text: "Fachregel R13 (X83): OZ 01.01.0020 ohne Langtext – Vergabeplattformen lehnen das ab \
                   (VOB/A § 7). [RIB e-Vergabe GAEB-Prüfer]"
                .into(),
            row: None,
        };
        let mut rects = Vec::new();
        let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
            ui.set_width(560.0);
            ui.vertical(|ui| {
                ui.set_max_width(560.0);
                rects.push(super::finding_row(ui, &palette, &short).rect);
                rects.push(super::finding_row(ui, &palette, &long).rect);
            });
        });
        output.textures_delta.clear();
        assert_eq!(rects[0].height(), 28.0, "kurzer Befund: eine Zeile");
        assert!(
            rects[1].height() >= 38.0,
            "langer Befund bricht um (zwei Zeilen): {:?}",
            rects[1]
        );
        assert!(
            rects[1].width() <= 560.0 + 0.5,
            "nicht breiter als der Dialog: {:?}",
            rects[1]
        );
    }

    #[test]
    fn one_pdf_per_bidder_gets_its_name_in_the_folder() {
        use proava_domain::{Address, InvitedBidder, Lv, OzMask, Project, ProjectName};
        let mut project = Project::new(
            ProjectName::parse("Schule").unwrap(),
            "2026-11-17",
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        );
        let mueller =
            project.add_invitee(InvitedBidder::new(Address::named("Müller & Söhne GmbH")));
        let schmidt = project.add_invitee(InvitedBidder::new(Address::named("Schmidt")));
        let files = super::letter_files_in(
            &project,
            "Schule_Angebotsanforderung_2026-11-17.",
            &[mueller, schmidt],
            std::path::Path::new("/ziel"),
        );
        let names: Vec<String> = files
            .iter()
            .map(|(_, file)| file.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(files[0].0, mueller);
        assert!(
            names[0].starts_with("Schule_Angebotsanforderung_2026-11-17_M"),
            "{names:?}"
        );
        assert!(names[0].ends_with(".pdf"));
        assert_eq!(
            names[1],
            "Schule_Angebotsanforderung_2026-11-17_Schmidt.pdf"
        );
    }

    #[test]
    fn palette_filters_by_words() {
        let all = palette_actions();
        let found = filter_actions(&all, "x83");
        assert_eq!(
            found,
            vec![ShellAction::Export(GaebExport::RequestForQuotation)]
        );
        assert!(filter_actions(&all, "neue position")
            .contains(&ShellAction::Editor(Command::NewPosition)));
        assert_eq!(filter_actions(&all, "").len(), all.len());
        assert!(filter_actions(&all, "gibtsnicht").is_empty());
    }
}
