//! „GAEB ▸ Importieren …“ and „CSV importieren …“: choose a GAEB file (X81 …
//! X86, D81 … D86, also in a zip) or a CSV table in the proAVA CSV format, see what
//! is in it, choose where it goes – a new LV of the open project, a new
//! project under a free name, the takeover window for the active LV or the
//! own library – and read the protocol afterwards. For a CSV table the
//! Währung is chosen here; positions whose GP did not fit are listed.
//!
//! The dialog only collects; the workspace does the import (with undo) and
//! hands the protocol back ([`ImportDialog::finished`]).

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, RichText};
use proava_application::gaeb_import::{self, GaebImport, ImportPreview, ImportProtocol};
use proava_application::Launcher;
use proava_domain::Project;

use crate::file_dialog::{DialogResult, FileDialog};
use crate::jobs::{Job, Poll};
use crate::theme::Palette;
use crate::{form_kit, format, widgets};

/// Files the dialog offers: every GAEB LV phase in XML and GAEB 90, a zip,
/// and CSV tables.
pub const IMPORT_EXTENSIONS: [&str; 17] = [
    "x81", "x82", "x83", "x84", "x85", "x86", "d81", "d82", "d83", "d84", "d85", "d86", "xml",
    "zip", "csv", "tsv", "txt",
];

/// Files of „CSV importieren …“.
pub const CSV_EXTENSIONS: [&str; 3] = ["csv", "tsv", "txt"];

pub const TITLE: &str = "GAEB importieren";
pub const CSV_TITLE: &str = "CSV importieren";
const HINT: &str = "LV aus einer GAEB-Datei übernehmen – auch Zulieferungen wie Vortexte oder \
                    Herstellertexte. Die Datei wird nur gelesen.";
const CSV_HINT: &str = "LV aus einer CSV-Tabelle übernehmen (Spalten OZ, Positionstext, Einheit, Menge, \
                        EP, GP, KG, Positionsart; Langtext in den Zeilen darunter). Die Datei wird nur gelesen.";

/// Where the import goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportChoice {
    NewLv,
    NewProject,
    ActiveLv,
    Library,
}

/// What the workspace should do now.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportRequest {
    AddLv {
        number: String,
        title: String,
        prices: bool,
    },
    NewProject {
        name: String,
        folder: PathBuf,
        open: bool,
        prices: bool,
    },
    Transfer {
        prices: bool,
    },
    Library {
        prices: bool,
    },
    /// The file is a bid: read it as one (Angebote einlesen).
    ReadBid(PathBuf),
}

pub enum ImportResult {
    Open,
    Closed,
    Do(ImportRequest),
}

enum Picking {
    File,
    Folder,
}

pub struct ImportDialog {
    file: String,
    import: Option<Box<GaebImport>>,
    preview: Option<ImportPreview>,
    reading: Option<Job<Result<GaebImport, String>>>,
    /// File of the running or last read (to read again after a change).
    read_file: String,
    error: Option<String>,
    pub choice: ImportChoice,
    pub lv_number: String,
    pub lv_title: String,
    pub project_name: String,
    pub project_folder: String,
    pub open_after: bool,
    pub prices: bool,
    picker: Option<(FileDialog, Picking)>,
    protocol: Option<ImportProtocol>,
    copied: bool,
    focus_file: bool,
    /// The open project has unsaved changes (shown at „danach öffnen“).
    unsaved: bool,
    /// Opened as „CSV importieren …“ (title, hint, file filter).
    csv: bool,
}

impl ImportDialog {
    pub fn new(launcher: &Launcher, unsaved: bool) -> Self {
        let folder = launcher
            .suggested_folder()
            .unwrap_or_else(|| launcher.home_folder());
        Self {
            file: String::new(),
            import: None,
            preview: None,
            reading: None,
            read_file: String::new(),
            error: None,
            choice: ImportChoice::NewLv,
            lv_number: String::new(),
            lv_title: String::new(),
            project_name: String::new(),
            project_folder: folder.to_string_lossy().to_string(),
            open_after: true,
            prices: true,
            picker: None,
            protocol: None,
            copied: false,
            focus_file: true,
            unsaved,
            csv: false,
        }
    }

    /// „CSV importieren …“: the same dialog for CSV tables.
    pub fn new_csv(launcher: &Launcher, unsaved: bool) -> Self {
        Self {
            csv: true,
            ..Self::new(launcher, unsaved)
        }
    }

    fn title(&self) -> &'static str {
        if self.csv {
            CSV_TITLE
        } else {
            TITLE
        }
    }

    /// Währung of the read CSV table (code); the protocol and the new LV
    /// follow it.
    pub fn set_currency(&mut self, code: &str) {
        if let Some(import) = self.import.as_mut() {
            import.set_currency(code);
            self.preview = Some(ImportPreview::of(import));
        }
    }

    /// Only the protocol of an import that happened elsewhere (start screen).
    pub fn showing(protocol: ImportProtocol) -> Self {
        Self {
            file: String::new(),
            import: None,
            preview: None,
            reading: None,
            read_file: String::new(),
            error: None,
            choice: ImportChoice::NewLv,
            lv_number: String::new(),
            lv_title: String::new(),
            project_name: String::new(),
            project_folder: String::new(),
            open_after: false,
            prices: true,
            picker: None,
            protocol: Some(protocol),
            copied: false,
            focus_file: false,
            unsaved: false,
            csv: false,
        }
    }

    /// The import read so far (for the workspace to carry out the request).
    pub fn import(&self) -> Option<&GaebImport> {
        self.import.as_deref()
    }

    /// Takes the read import out (transfer window, library).
    pub fn take_import(&mut self) -> Option<GaebImport> {
        self.import.take().map(|import| *import)
    }

    pub fn preview(&self) -> Option<&ImportPreview> {
        self.preview.as_ref()
    }

    /// The import is done: show its protocol.
    pub fn finished(&mut self, protocol: ImportProtocol) {
        self.protocol = Some(protocol);
        self.copied = false;
        self.error = None;
    }

    /// The import failed: show why and stay.
    pub fn failed(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Reads `file` on a worker thread (also from the start: a file given).
    pub fn read(&mut self, launcher: &Launcher, file: &Path) {
        self.file = file.to_string_lossy().to_string();
        self.start_reading(launcher);
    }

    fn start_reading(&mut self, launcher: &Launcher) {
        let text = self.file.trim().to_string();
        if text.is_empty() {
            self.error = Some(
                if self.csv {
                    "Bitte eine CSV-Datei wählen."
                } else {
                    "Bitte eine GAEB-Datei wählen."
                }
                .to_string(),
            );
            return;
        }
        let file = PathBuf::from(&text);
        if !launcher.is_file(&file) {
            self.error = Some(format!("Die Datei „{text}“ gibt es nicht."));
            return;
        }
        let exchange = launcher.shared_exchange();
        let now = launcher.now_iso();
        self.error = None;
        self.import = None;
        self.preview = None;
        self.read_file = text;
        self.reading = Some(Job::spawn(
            format!("{} wird gelesen …", crate::file_dialog::file_name(&file)),
            move || gaeb_import::read(exchange.as_ref(), &file, &now),
        ));
    }

    /// The read file arrived: preview and proposals.
    fn take_read(&mut self, project: &Project, result: Result<GaebImport, String>) {
        match result {
            Ok(import) => {
                let preview = ImportPreview::of(&import);
                self.lv_number = preview.suggested_number(project);
                self.lv_title = preview.suggested_title();
                self.project_name = Path::new(&preview.file_name)
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().trim().to_string())
                    .unwrap_or_default();
                self.prices = preview.has_prices();
                self.import = Some(Box::new(import));
                self.preview = Some(preview);
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Problems of the choice that keep „Importieren“ from working.
    fn problem(&self, project: &Project, launcher: &Launcher) -> Option<String> {
        self.preview.as_ref()?;
        match self.choice {
            ImportChoice::NewLv => {
                let number = self.lv_number.trim();
                let fresh = project.lv_count() == 1 && project.lv().rows().is_empty();
                if self.lv_title.trim().is_empty() {
                    Some("Bitte eine Bezeichnung für das LV angeben.".to_string())
                } else if !fresh
                    && !number.is_empty()
                    && project.lvs().any(|lv| lv.number().trim() == number)
                {
                    Some(format!(
                        "Die LV-Nummer „{number}“ gibt es im Projekt schon (frei ist z. B. {}).",
                        gaeb_import::next_free_number(project)
                    ))
                } else {
                    None
                }
            }
            ImportChoice::NewProject => {
                if let Err(error) = proava_domain::ProjectName::parse(&self.project_name) {
                    return Some(error.to_string());
                }
                let folder = PathBuf::from(self.project_folder.trim());
                if self.project_folder.trim().is_empty() || !launcher.is_folder(&folder) {
                    return Some(format!(
                        "Den Ordner „{}“ gibt es nicht.",
                        self.project_folder.trim()
                    ));
                }
                let name = self.project_name.trim();
                let file = folder.join(name).join(format!("{name}.proava"));
                launcher
                    .is_file(&file)
                    .then(|| format!("Dort gibt es schon ein Projekt: {}", file.display()))
            }
            ImportChoice::ActiveLv | ImportChoice::Library => None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
        project: &Project,
    ) -> ImportResult {
        if let Some(result) = self.picker_step(ui, palette, launcher) {
            return result;
        }
        if let Some(job) = &self.reading {
            match job.poll() {
                Poll::Running => ui.ctx().request_repaint_after(Duration::from_millis(100)),
                Poll::Lost => {
                    self.reading = None;
                    self.error = Some("Das Lesen ist abgebrochen (interner Fehler).".to_string());
                }
                Poll::Done(result) => {
                    self.reading = None;
                    self.take_read(project, result);
                }
            }
        }
        if self.protocol.is_some() {
            return self.protocol_step(ui, palette);
        }

        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let mut result = ImportResult::Open;
        let mut read_now = false;
        let mut pick = None;
        let problem = self.problem(project, launcher);
        let preview_protocol = self.preview.as_ref().map(ImportPreview::protocol);
        let width = preview_protocol.as_ref().map_or(680.0, |protocol| {
            crate::protocol_view::dialog_width(ui, protocol.entries.iter(), 680.0)
        });
        let (_, close) = widgets::modal(ui, palette, width, |ui| {
            widgets::title(
                ui,
                palette,
                self.title(),
                if self.csv { CSV_HINT } else { HINT },
            );
            // Datei
            let caption = ui.label(
                RichText::new(if self.csv { "CSV-Datei" } else { "GAEB-Datei" })
                    .size(12.0)
                    .color(palette.text_dim),
            );
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 2.0 * 128.0 - 16.0).max(160.0);
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.file)
                            .desired_width(width)
                            .margin(egui::Margin::symmetric(8, 6))
                            .hint_text(if self.csv {
                                "Pfad der .csv-Datei"
                            } else {
                                "Pfad der .X8x-, .D8x-, .zip- oder .csv-Datei"
                            }),
                    )
                    .labelled_by(caption.id);
                if std::mem::take(&mut self.focus_file) {
                    response.request_focus();
                }
                if response.lost_focus() && enter && self.file.trim() != self.read_file {
                    read_now = true;
                }
                if ui
                    .add(egui::Button::new("Datei wählen …").min_size(egui::vec2(124.0, 28.0)))
                    .clicked()
                {
                    pick = Some(Picking::File);
                }
                if ui
                    .add_enabled(
                        self.reading.is_none(),
                        egui::Button::new("Lesen").min_size(egui::vec2(124.0, 28.0)),
                    )
                    .on_hover_text("Datei lesen und Inhalt zeigen (auch Enter im Feld)")
                    .clicked()
                {
                    read_now = true;
                }
            });
            ui.add_space(10.0);
            if let Some(job) = &self.reading {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0).color(palette.accent));
                    ui.label(RichText::new(job.label()).size(12.5).color(palette.accent));
                });
                ui.add_space(8.0);
            }
            if let (Some(preview), Some(protocol)) = (self.preview.clone(), &preview_protocol) {
                preview_block(ui, palette, protocol);
                ui.add_space(6.0);
                if preview.is_bid() {
                    ui.label(
                        RichText::new(
                            "Die Datei ist ein Angebot. Angebote liest „Angebote einlesen …“ im Menü Angebote ein \
                             und ordnet sie dem LV zu; als LV übernommen gehen Bieter und Nachlass verloren.",
                        )
                        .size(12.5)
                        .color(palette.warn),
                    );
                    if ui.button("Als Angebot einlesen …").clicked() {
                        if let Some(import) = &self.import {
                            result = ImportResult::Do(ImportRequest::ReadBid(import.file.clone()));
                        }
                    }
                    ui.add_space(8.0);
                }
                self.target_block(ui, palette, &preview, &mut pick);
            }
            widgets::error_line(ui, palette, self.error.as_deref().or(problem.as_deref()));
            let can = self.preview.is_some() && problem.is_none() && self.reading.is_none();
            let (cancelled, confirmed) = widgets::button_row(ui, |ui| {
                let confirmed = ui
                    .add_enabled(
                        can,
                        egui::Button::new(RichText::new("Importieren").color(palette.on_accent))
                            .fill(palette.accent)
                            .min_size(egui::vec2(120.0, 32.0)),
                    )
                    .clicked();
                ui.add_space(8.0);
                let cancelled = ui
                    .add(
                        egui::Button::new(
                            RichText::new(crate::strings::CANCEL).color(palette.text),
                        )
                        .min_size(egui::vec2(120.0, 32.0)),
                    )
                    .clicked();
                (cancelled, confirmed)
            });
            if cancelled {
                result = ImportResult::Closed;
            } else if confirmed {
                result = ImportResult::Do(self.request());
            }
        });
        if read_now {
            self.start_reading(launcher);
        }
        match pick {
            Some(Picking::File) => {
                let start = PathBuf::from(self.file.trim())
                    .parent()
                    .filter(|parent| launcher.is_folder(parent))
                    .map(Path::to_path_buf)
                    .or_else(|| launcher.suggested_folder())
                    .unwrap_or_else(|| launcher.home_folder());
                let dialog = if self.csv {
                    FileDialog::open(
                        launcher,
                        CSV_TITLE,
                        "CSV-Tabelle (Trenner Semikolon, Komma oder Tabulator; UTF-8 oder Windows-1252) – wird nur gelesen.",
                        start,
                        &CSV_EXTENSIONS,
                        false,
                    )
                } else {
                    FileDialog::open(
                        launcher,
                        TITLE,
                        "GAEB-LV (X81 … X86, D81 … D86), eine .zip, die eine enthält, oder eine CSV-Tabelle – wird nur gelesen.",
                        start,
                        &IMPORT_EXTENSIONS,
                        false,
                    )
                };
                self.picker = Some((dialog, Picking::File));
            }
            Some(Picking::Folder) => {
                let typed = PathBuf::from(self.project_folder.trim());
                let start = if launcher.is_folder(&typed) {
                    typed
                } else {
                    launcher.home_folder()
                };
                self.picker = Some((
                    FileDialog::folder(launcher, "Ordner für das neue Projekt", start),
                    Picking::Folder,
                ));
            }
            None => {}
        }
        if close && matches!(result, ImportResult::Open) {
            return ImportResult::Closed;
        }
        result
    }

    fn picker_step(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
    ) -> Option<ImportResult> {
        let (dialog, picking) = self.picker.as_mut()?;
        match dialog.show(ui, palette, launcher) {
            DialogResult::Open => {}
            DialogResult::Cancelled => self.picker = None,
            DialogResult::Chosen(paths) => {
                let folder = matches!(picking, Picking::Folder);
                self.picker = None;
                if let Some(path) = paths.into_iter().next() {
                    if folder {
                        self.project_folder = path.to_string_lossy().to_string();
                    } else {
                        self.read(launcher, &path);
                    }
                }
            }
        }
        Some(ImportResult::Open)
    }

    fn target_block(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        preview: &ImportPreview,
        pick: &mut Option<Picking>,
    ) {
        form_kit::section(ui, palette, "Ziel");
        ui.radio_value(
            &mut self.choice,
            ImportChoice::NewLv,
            "Als neues LV in dieses Projekt",
        );
        if self.choice == ImportChoice::NewLv {
            ui.indent("import_lv", |ui| {
                form_kit::split(
                    ui,
                    0.25,
                    |ui| {
                        form_kit::field(ui, palette, "LV-Nr.", &mut self.lv_number, None);
                    },
                    |ui| {
                        form_kit::field(ui, palette, "Bezeichnung", &mut self.lv_title, None);
                    },
                );
            });
        }
        ui.radio_value(
            &mut self.choice,
            ImportChoice::NewProject,
            "Als neues Projekt (eigener Name und Ordner)",
        );
        if self.choice == ImportChoice::NewProject {
            ui.indent("import_project", |ui| {
                form_kit::field(ui, palette, "Projektname", &mut self.project_name, None);
                let caption = ui.label(RichText::new("Ordner").size(12.0).color(palette.text_dim));
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 136.0).max(160.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_folder)
                            .desired_width(width)
                            .margin(egui::Margin::symmetric(8, 6)),
                    )
                    .labelled_by(caption.id);
                    if ui
                        .add(egui::Button::new("Ordner wählen …").min_size(egui::vec2(128.0, 28.0)))
                        .clicked()
                    {
                        *pick = Some(Picking::Folder);
                    }
                });
                ui.add_space(6.0);
                let label = if self.unsaved {
                    "Danach öffnen (das offene Projekt wird vorher gespeichert)"
                } else {
                    "Danach öffnen"
                };
                ui.checkbox(&mut self.open_after, label);
                ui.add_space(4.0);
            });
        }
        ui.radio_value(
            &mut self.choice,
            ImportChoice::ActiveLv,
            "In das aktive LV übernehmen (Auswahl im Übernahme-Fenster)",
        );
        ui.radio_value(
            &mut self.choice,
            ImportChoice::Library,
            "In die eigene Bibliothek übernehmen (unter einen Titel)",
        );
        ui.add_space(6.0);
        if !preview.columns.is_empty() {
            let mut chosen = preview.currency.to_uppercase();
            crate::widgets::row(ui, |ui| {
                let caption = ui.label(RichText::new("Währung").size(12.5).color(palette.text_dim));
                egui::ComboBox::from_id_salt("import_currency")
                    .selected_text(match proava_domain::price_check::currency_name(&chosen) {
                        "" => chosen.clone(),
                        name => format!("{name} ({chosen})"),
                    })
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (code, name) in proava_domain::price_check::CURRENCIES {
                            ui.selectable_value(
                                &mut chosen,
                                code.to_string(),
                                format!("{name} ({code})"),
                            );
                        }
                    })
                    .response
                    .labelled_by(caption.id)
                    .on_hover_text("Währung des neuen LV (GAEB, Druck, Summen)");
            });
            if chosen != preview.currency.to_uppercase() {
                self.set_currency(&chosen);
            }
            ui.add_space(4.0);
        }
        if preview.has_prices() {
            ui.checkbox(
                &mut self.prices,
                format!(
                    "Preise übernehmen ({} als Planerpreise)",
                    format::count(
                        preview.priced_positions(),
                        "Einheitspreis",
                        "Einheitspreise"
                    )
                ),
            );
        } else if preview.positions > 0 {
            form_kit::hint(ui, palette, "Die Datei hat keine Einheitspreise.");
        }
        ui.add_space(8.0);
    }

    fn request(&self) -> ImportRequest {
        let prices = self.prices && self.preview.as_ref().is_some_and(ImportPreview::has_prices);
        match self.choice {
            ImportChoice::NewLv => ImportRequest::AddLv {
                number: self.lv_number.trim().to_string(),
                title: self.lv_title.trim().to_string(),
                prices,
            },
            ImportChoice::NewProject => ImportRequest::NewProject {
                name: self.project_name.trim().to_string(),
                folder: PathBuf::from(self.project_folder.trim()),
                open: self.open_after,
                prices,
            },
            ImportChoice::ActiveLv => ImportRequest::Transfer { prices },
            ImportChoice::Library => ImportRequest::Library { prices },
        }
    }

    fn protocol_step(&mut self, ui: &mut egui::Ui, palette: &Palette) -> ImportResult {
        let Some(protocol) = self.protocol.clone() else {
            return ImportResult::Closed;
        };
        if crate::protocol_view::dialog(ui, palette, &protocol, &mut self.copied) {
            ImportResult::Open
        } else {
            ImportResult::Closed
        }
    }
}

/// The preview of the read file: one fact per line, in a scroll area so the
/// targets below stay visible with long lists.
fn preview_block(ui: &mut egui::Ui, palette: &Palette, protocol: &proava_application::Protocol) {
    // The rest of the dialog (file field, targets, buttons) needs about
    // 600 px; the preview scrolls in what is left.
    let window = ui.ctx().content_rect().height();
    let max_height = (window - 600.0).clamp(140.0, window * 0.45);
    egui::ScrollArea::vertical()
        .id_salt("import_preview")
        .max_height(max_height)
        .show(ui, |ui| {
            crate::protocol_view::show(ui, palette, &protocol.entries)
        });
    ui.add_space(4.0);
    ui.separator();
}

/// Open a project after the import (the host picks it up each frame).
pub(crate) fn request_open(ctx: &egui::Context, request: crate::OpenRequest) {
    ctx.data_mut(|data| data.insert_temp(open_id(), Some(request)));
}

pub(crate) fn take_open_request(ctx: &egui::Context) -> Option<crate::OpenRequest> {
    ctx.data_mut(|data| data.remove_temp::<Option<crate::OpenRequest>>(open_id()))
        .flatten()
}

fn open_id() -> egui::Id {
    egui::Id::unique("proava_open_after_import")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_filter_takes_every_lv_phase_and_zip() {
        for extension in [
            "x81", "x82", "x83", "d81", "d82", "d83", "x84", "d86", "zip",
        ] {
            assert!(IMPORT_EXTENSIONS.contains(&extension), "{extension}");
        }
    }
}
