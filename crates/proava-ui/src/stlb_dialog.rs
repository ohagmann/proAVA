//! „Aus STLB-Bau übernehmen“: positions built in STLB-Bau come in as a GAEB
//! file (STLB-Bau Online: „Leistungsverzeichnisse → GAEB exportieren“, or the
//! local installation). The dialog opens STLB-Bau Online in the browser and
//! watches the Downloads folder. As soon as exactly one new GAEB file has been
//! written completely, it is read and – with „sofort übernehmen“ – taken into
//! the LV without a further click: the selected placeholder is filled, the
//! other positions follow, and the selection moves on to the next open
//! placeholder. The dialog stays open for the next position. The STLB-Bau key
//! of each position is kept.
//!
//! The direct call of STLB-Bau from an AVA needs a partner interface of
//! DBD/DIN Bauportal; proAVA uses the GAEB file (`docs/FACHLICHES.md`).
//!
//! The same dialog serves every text source that hands over GAEB files
//! ([`TextSource`]): AUSSCHREIBEN.DE (manufacturer texts, export in the
//! browser) and a GAEB file dropped on the LV
//! table. Each taken position remembers its Herkunft (source, manufacturer,
//! day, file).
//!
//! Where it is installed, the web pages open in proAVA's own browser window
//! ([`crate::web_window`], helper `proava-web`): a GAEB file exported there
//! is reported directly and taken like a new file in the Downloads folder.
//!
//! Taken once (Anwender 19.11.): after a take-over – automatic or by the
//! button – the entries of the file count as taken; „Übernehmen“ is grey
//! („Übernommen“) until a new file comes or a new choice is made. Entries
//! taken earlier in this dialog (the same file again, the same positions in
//! a new export) are not taken a second time without the question „Diese
//! Positionen wurden bereits übernommen – noch einmal?“. Strg+Z in the
//! dialog takes the last take-over back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use eframe::egui::{self, RichText};
use proava_application::{CatalogPositions, Launcher};
use proava_domain::stlb::{adopt_catalog_cost_group, catalog_cost_group, fill_from_catalog};
use proava_domain::text_origin::{AUSSCHREIBEN_DE, STLB_BAU};
use proava_domain::RowKind;
use proava_domain::{
    format_decimal_de, Decimal, EditStatus, Lv, LvRow, Position, PositionKind, TextOrigin,
};

use crate::file_dialog::{self, DialogResult, FileDialog};
use crate::lv_ops;
use crate::tender_search::TenderSearch;
use crate::theme::Palette;
use crate::web_window::{self, WebWindow};
use crate::{jobs, strings, widgets};
use proava_web_protocol::{FromHelper, ToHelper};

pub const STLB_ONLINE_URL: &str = "https://www.stlb-bau-online.de";
pub const AUSSCHREIBEN_URL: &str = "https://ausschreiben.de";

/// Where the texts come from: decides title, help text, the web page to
/// open and the source noted in the Herkunft of each position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    /// STLB-Bau Online or the local STLB-Bau (STLB-Bau key, placeholders).
    StlbBau,
    /// Manufacturer texts from AUSSCHREIBEN.DE (export in the browser).
    AusschreibenDe,
    /// Any GAEB file, e.g. dropped on the LV table.
    GaebFile,
}

impl TextSource {
    /// Name of the source in messages and in the Herkunft.
    pub fn name(self) -> &'static str {
        match self {
            TextSource::StlbBau => STLB_BAU,
            TextSource::AusschreibenDe => AUSSCHREIBEN_DE,
            TextSource::GaebFile => "GAEB-Datei",
        }
    }

    fn title(self) -> &'static str {
        match self {
            TextSource::StlbBau => strings::STLB_DIALOG_TITLE,
            TextSource::AusschreibenDe => strings::AUSSCHREIBEN_DIALOG_TITLE,
            TextSource::GaebFile => strings::GAEB_DROP_DIALOG_TITLE,
        }
    }

    fn hint(self) -> &'static str {
        match self {
            TextSource::StlbBau => strings::STLB_DIALOG_HINT,
            TextSource::AusschreibenDe => strings::AUSSCHREIBEN_DIALOG_HINT,
            TextSource::GaebFile => strings::GAEB_DROP_DIALOG_HINT,
        }
    }

    /// Button text and address of the web page with the texts.
    fn web(self) -> Option<(&'static str, &'static str)> {
        match self {
            TextSource::StlbBau => Some(("STLB-Bau Online öffnen", STLB_ONLINE_URL)),
            TextSource::AusschreibenDe => Some(("AUSSCHREIBEN.DE öffnen", AUSSCHREIBEN_URL)),
            TextSource::GaebFile => None,
        }
    }

    fn picker(self) -> (&'static str, &'static str) {
        match self {
            TextSource::StlbBau => (
                "GAEB-Datei aus STLB-Bau",
                "Die Datei, die STLB-Bau (Online oder lokal) als GAEB gespeichert hat.",
            ),
            TextSource::AusschreibenDe => (
                "GAEB-Datei von AUSSCHREIBEN.DE",
                "Die Datei, die AUSSCHREIBEN.DE beim Export (Text oder Merkliste) gespeichert hat.",
            ),
            TextSource::GaebFile => (
                "GAEB-Datei",
                "Eine GAEB-Datei (X81–X86, D81–D86) oder ein ZIP.",
            ),
        }
    }
}

/// Notes the source of the dialog in the Herkunft of read positions (see
/// [`proava_domain::text_origin::adopt_source`]): a named source replaces
/// one the file names but proAVA does not know; a dropped file keeps it.
pub fn adopt_source(positions: &mut [Position], source: TextSource, today: &str) {
    proava_domain::text_origin::adopt_source(
        positions,
        source.name(),
        source != TextSource::GaebFile,
        today,
    );
}
/// How often the Downloads folder is looked at while the dialog is open.
const WATCH_SECONDS: f64 = 2.0;
/// Take-overs listed in the dialog.
const LOG_LINES: usize = 6;

/// What the dialog asks the workspace to do.
pub enum StlbResult {
    Open,
    Cancelled,
    /// Take these entries into the LV (see [`take_over`]) – each as the
    /// chosen kind (Position, Hinweis, Ausführungsbeschreibung); the dialog
    /// stays open, the workspace reports back with [`StlbDialog::note`].
    Apply(Vec<(Position, RowKind)>),
    /// Put the whole file into the own library (form of the workspace); the
    /// Herkunft of the read positions comes along (see [`StlbDialog::origins`]).
    ToLibrary(PathBuf),
}

/// Where the next positions go, from the selection in the LV (see [`target_of`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StlbTarget {
    pub label: String,
    pub fills_placeholder: bool,
}

/// One take-over asked of the workspace by this dialog.
#[derive(Debug, Clone)]
struct TakeOver {
    /// Identity of each entry taken (see [`entry_key`]).
    keys: Vec<String>,
    /// File read (generation) the entries came from, and their indices.
    generation: u64,
    indices: Vec<usize>,
    /// „3 Positionen“ / „2 Einträge“.
    label: String,
    /// Line of the workspace (log, „Zurückgenommen: …“).
    line: String,
}

/// The question before taking entries a second time.
#[derive(Debug, Clone)]
struct Repeat {
    /// The entries asked for.
    indices: Vec<usize>,
    /// Those of them taken earlier in this dialog.
    repeated: Vec<usize>,
}

/// Identity of an entry for „schon übernommen“: catalogue key with its
/// characteristics, texts, unit and the values of the Textergänzungen – the
/// same file read again or the same positions in a new export match, a
/// different position (or a different value) does not.
fn entry_key(position: &Position) -> String {
    let stlb = position
        .stlb_reference()
        .map(|stlb| {
            format!(
                "{}|{:?}|{}|{}|{:?}",
                stlb.catalog, stlb.version, stlb.work_category, stlb.group, stlb.characteristics
            )
        })
        .unwrap_or_default();
    let values: Vec<String> = position
        .complements()
        .iter()
        .map(|complement| format!("{}={}", complement.marker.trim(), complement.body.trim()))
        .collect();
    format!(
        "{stlb}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        position.standard_number(),
        position.short_text().trim(),
        position.long_text().to_plain(),
        position.unit().trim(),
        values.join(";")
    )
}

/// A GAEB file in a Downloads folder: size and time when it was seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: SystemTime,
    len: u64,
}

pub struct StlbDialog {
    source: TextSource,
    started: SystemTime,
    /// Download folders that are watched (XDG, Windows, macOS, `~/Downloads`).
    downloads: Vec<PathBuf>,
    last_scan: f64,
    /// New files with their stamp at the last scan (written after the
    /// dialog was opened); a file counts as complete when its stamp did not
    /// change between two scans.
    seen: HashMap<PathBuf, Stamp>,
    /// Files already read or offered, with the stamp they had then.
    handled: HashMap<PathBuf, Stamp>,
    /// Complete new files that were not taken automatically (several at once).
    offered: Vec<PathBuf>,
    /// Take a single new file at once, without a click.
    pub auto: bool,
    /// File that was read last.
    file: Option<PathBuf>,
    read: Option<CatalogPositions>,
    chosen: Vec<bool>,
    /// „Übernehmen als“ per entry, first the suggestion of the file.
    kinds: Vec<RowKind>,
    error: Option<String>,
    picker: Option<FileDialog>,
    /// What was taken over, newest first.
    log: Vec<String>,
    /// „In AUSSCHREIBEN.DE suchen“ (only for that source).
    search: TenderSearch,
    /// Pages open in proAVA's browser window (installed, not switched off).
    own_window: bool,
    /// Why the pages open in the system browser instead (`None` with the
    /// own window or for a GAEB file); shown in the dialog.
    web_fallback: Option<String>,
    /// The browser window while it is open.
    web: Option<WebWindow>,
    /// Messages of the window not handled yet (tests hand in their own).
    web_events: Vec<FromHelper>,
    /// „Ziel im LV“ last sent to the window.
    sent_target: String,
    /// What the window last did, for the line in the dialog.
    web_note: Option<String>,
    /// +1 for every file read; a take-over belongs to one of them.
    generation: u64,
    /// Identity of each entry of the file read last ([`entry_key`]).
    keys: Vec<String>,
    /// Entries of the file read last that were taken over from it.
    taken: Vec<bool>,
    /// The take-over just asked of the workspace (confirmed by [`Self::note`]).
    pending: Option<TakeOver>,
    /// Take-overs of this dialog, oldest first: the question before a repeat
    /// and Strg+Z in the dialog.
    done: Vec<TakeOver>,
    /// „Diese Positionen wurden bereits übernommen – noch einmal?“
    repeat: Option<Repeat>,
    /// „3 Positionen übernommen – Strg+Z nimmt es zurück.“
    taken_note: Option<String>,
}

impl StlbDialog {
    pub fn new(launcher: &Launcher) -> Self {
        Self::for_source(launcher, TextSource::StlbBau)
    }

    /// The dialog for `source` (STLB-Bau, AUSSCHREIBEN.DE, GAEB file).
    pub fn for_source(launcher: &Launcher, source: TextSource) -> Self {
        // Why the pages open in the system browser (shown in the dialog).
        let web_fallback = web_window::site_of(source).and_then(|_| {
            web_window::own_window(
                launcher.web_pages_in_system_browser(),
                jobs::helper_missing().map_or(Ok(()), Err),
            )
            .err()
        });
        let downloads: Vec<PathBuf> = system_download_folders(launcher)
            .into_iter()
            .filter(|folder| launcher.is_folder(folder))
            .collect();
        Self {
            source,
            started: SystemTime::now(),
            downloads,
            last_scan: f64::NEG_INFINITY,
            seen: HashMap::new(),
            handled: HashMap::new(),
            offered: Vec::new(),
            auto: true,
            file: None,
            read: None,
            chosen: Vec::new(),
            kinds: Vec::new(),
            error: None,
            picker: None,
            log: Vec::new(),
            search: TenderSearch::default(),
            own_window: web_fallback.is_none() && web_window::site_of(source).is_some(),
            web_fallback,
            web: None,
            web_events: Vec::new(),
            sent_target: String::new(),
            web_note: None,
            generation: 0,
            keys: Vec::new(),
            taken: Vec::new(),
            pending: None,
            done: Vec::new(),
            repeat: None,
            taken_note: None,
        }
    }

    /// Whether the pages open in proAVA's browser window.
    pub fn uses_own_window(&self) -> bool {
        self.own_window
    }

    /// A message of the browser window (from the window, or handed in by a
    /// test); handled in the next frame.
    pub fn web_event(&mut self, event: FromHelper) {
        self.web_events.push(event);
    }

    /// Opens the site of the source – in the own window (a page of the site
    /// or its start page) or in the system browser.
    fn open_web(&mut self, ctx: &egui::Context, url: Option<String>, target: &str) {
        let Some(site) = web_window::site_of(self.source) else {
            return;
        };
        if self.own_window {
            if let Some(web) = self.web.as_mut().filter(|web| web.is_open()) {
                match &url {
                    Some(url) => web.open_page(url),
                    None => web.send(&ToHelper::Raise),
                }
                return;
            }
            match WebWindow::start(ctx, site, url.clone(), target) {
                Ok(Some(web)) => {
                    self.web = Some(web);
                    self.sent_target = target.to_string();
                    self.web_note = Some(format!(
                        "{} ist im Browserfenster von proAVA offen – dort als GAEB exportieren, die Datei kommt direkt hierher.",
                        site.name()
                    ));
                    return;
                }
                // Tests: nothing is started.
                Ok(None) => return,
                Err(error) => {
                    self.own_window = false;
                    self.web_fallback = Some(error);
                }
            }
        }
        let url = url.unwrap_or_else(|| site.start_url().to_string());
        if let Err(error) = jobs::open_with_system(Path::new(&url)) {
            self.error = Some(error);
        }
    }

    /// Handles the messages of the browser window: a GAEB file exported
    /// there is read (and with „sofort übernehmen“ taken at once).
    fn take_web_events(&mut self, launcher: &Launcher) -> Option<StlbResult> {
        let mut events = std::mem::take(&mut self.web_events);
        if let Some(web) = &mut self.web {
            events.extend(web.poll());
        }
        let mut result = None;
        for event in events {
            match event {
                FromHelper::Downloaded { file, .. } => {
                    if !proava_web_protocol::is_catalog_file(&file) {
                        continue;
                    }
                    self.load(launcher, &file);
                    self.web_note = Some(format!(
                        "Aus dem Browserfenster: {}",
                        file_dialog::file_name(&file)
                    ));
                    if self.auto {
                        result = self.request(self.chosen_indices()).or(result);
                    }
                }
                FromHelper::DownloadFailed { .. } => {
                    self.error = Some("Der Download im Browserfenster ist abgebrochen.".into());
                }
                FromHelper::Error { message } => self.error = Some(message),
                FromHelper::Closed => {
                    self.web = None;
                    self.web_note = Some("Das Browserfenster ist geschlossen.".into());
                }
                FromHelper::Ready | FromHelper::Page { .. } => {}
            }
        }
        if self.web.as_ref().is_some_and(|web| !web.is_open()) {
            self.web = None;
        }
        result
    }

    /// The dialog with a file already read, e.g. dropped on the LV table.
    pub fn with_file(launcher: &Launcher, source: TextSource, file: &Path) -> Self {
        let mut dialog = Self::for_source(launcher, source);
        dialog.load(launcher, file);
        dialog
    }

    pub fn source(&self) -> TextSource {
        self.source
    }

    /// Reads a file given from outside (dropped on the LV table).
    pub fn load_file(&mut self, launcher: &Launcher, file: &Path) {
        self.load(launcher, file);
    }

    /// Herkunft of the positions read last, in file order (for the library).
    pub fn origins(&self) -> Vec<Option<TextOrigin>> {
        self.read
            .as_ref()
            .map(|read| {
                read.positions
                    .iter()
                    .map(|p| p.text_origin().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The workspace reports the result of an [`StlbResult::Apply`]. A
    /// take-over that went in marks its entries as taken: the button turns
    /// grey until a new file or a new choice comes.
    pub fn note(&mut self, outcome: Result<String, String>) {
        let pending = self.pending.take();
        match outcome {
            Ok(line) => {
                self.error = None;
                // The browser window shows it in its toolbar too.
                if let Some(web) = &mut self.web {
                    web.send(&ToHelper::Status { text: line.clone() });
                }
                if let Some(mut taken) = pending {
                    if taken.generation == self.generation {
                        for &index in &taken.indices {
                            if let Some(flag) = self.taken.get_mut(index) {
                                *flag = true;
                            }
                            if let Some(chosen) = self.chosen.get_mut(index) {
                                *chosen = false;
                            }
                        }
                    }
                    self.taken_note = Some(format!(
                        "{} übernommen – Strg+Z nimmt es zurück.",
                        taken.label
                    ));
                    taken.line = line.clone();
                    self.done.push(taken);
                }
                self.log.insert(0, line);
                self.log.truncate(LOG_LINES);
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Whether Strg+Z in the dialog has a take-over of it to take back.
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// The workspace took the last take-over back (Strg+Z in the dialog):
    /// its entries count as not taken, the button is active again.
    pub fn undone(&mut self) {
        let Some(taken) = self.done.pop() else {
            return;
        };
        if taken.generation == self.generation {
            for &index in &taken.indices {
                if let Some(flag) = self.taken.get_mut(index) {
                    *flag = false;
                }
                if let Some(chosen) = self.chosen.get_mut(index) {
                    *chosen = true;
                }
            }
        }
        self.repeat = None;
        self.taken_note = Some(format!(
            "{} zurückgenommen – „Übernehmen“ fügt sie wieder ein.",
            taken.label
        ));
        self.log
            .insert(0, format!("Zurückgenommen: {}", taken.line));
        self.log.truncate(LOG_LINES);
    }

    /// Indices of the chosen entries of the file read last.
    fn chosen_indices(&self) -> Vec<usize> {
        if self.read.is_none() {
            return Vec::new();
        }
        self.chosen
            .iter()
            .enumerate()
            .filter(|(_, chosen)| **chosen)
            .map(|(index, _)| index)
            .collect()
    }

    /// Whether entry `index` of the file read last was taken earlier in
    /// this dialog (the same file again or the same position).
    fn was_taken(&self, index: usize) -> bool {
        self.taken.get(index).copied().unwrap_or(false)
            || self.keys.get(index).is_some_and(|key| {
                self.done
                    .iter()
                    .any(|taken| taken.keys.iter().any(|known| known == key))
            })
    }

    /// „Übernehmen“ for `indices` (automatic or by the button): entries
    /// taken before ask first, the others go in.
    fn request(&mut self, indices: Vec<usize>) -> Option<StlbResult> {
        if indices.is_empty() {
            return None;
        }
        let repeated: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&index| self.was_taken(index))
            .collect();
        if !repeated.is_empty() {
            self.repeat = Some(Repeat { indices, repeated });
            return None;
        }
        self.apply(indices)
    }

    /// Asks the workspace to take `indices` (no question).
    fn apply(&mut self, indices: Vec<usize>) -> Option<StlbResult> {
        let read = self.read.as_ref()?;
        let entries: Vec<(Position, RowKind)> = indices
            .iter()
            .filter_map(|&index| {
                let position = read.positions.get(index)?;
                let kind = self
                    .kinds
                    .get(index)
                    .copied()
                    .unwrap_or(read.kind(index).suggested);
                Some((position.clone(), kind))
            })
            .collect();
        if entries.is_empty() {
            return None;
        }
        let label = if entries.iter().all(|(_, kind)| *kind == RowKind::Position) {
            crate::format::count(entries.len(), "Position", "Positionen")
        } else {
            crate::format::count(entries.len(), "Eintrag", "Einträge")
        };
        self.repeat = None;
        self.pending = Some(TakeOver {
            keys: indices
                .iter()
                .filter_map(|&index| self.keys.get(index).cloned())
                .collect(),
            generation: self.generation,
            indices,
            label,
            line: String::new(),
        });
        Some(StlbResult::Apply(entries))
    }

    /// The entries of the last take-over from the file read last
    /// („Noch einmal übernehmen“), if there is one.
    fn last_of_this_file(&self) -> Option<Vec<usize>> {
        self.done
            .last()
            .filter(|taken| taken.generation == self.generation)
            .map(|taken| taken.indices.clone())
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
        target: &StlbTarget,
    ) -> StlbResult {
        let mut result = StlbResult::Open;
        if let Some(picker) = &mut self.picker {
            // Own id scope: in the frame the picker closes, this dialog's
            // modal is drawn too (same `ui`, otherwise the same id).
            let shown = ui
                .push_id("stlb_picker", |ui| picker.show(ui, palette, launcher))
                .inner;
            match shown {
                DialogResult::Open => return StlbResult::Open,
                DialogResult::Cancelled => self.picker = None,
                DialogResult::Chosen(files) => {
                    self.picker = None;
                    if let Some(file) = files.into_iter().next() {
                        self.load(launcher, &file);
                    }
                }
            }
        }
        if let Some(taken) = self.take_web_events(launcher) {
            result = taken;
        }
        if let Some(web) = &mut self.web {
            if self.sent_target != target.label {
                web.send(&ToHelper::Target {
                    text: target.label.clone(),
                });
                self.sent_target = target.label.clone();
            }
        }
        let now = ui.input(|input| input.time);
        if now - self.last_scan >= WATCH_SECONDS {
            self.last_scan = now;
            if let Some(file) = self.scan() {
                self.load(launcher, &file);
                if let Some(taken) = self.request(self.chosen_indices()) {
                    result = taken;
                }
            }
        }
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f64(WATCH_SECONDS));

        let mut pick = false;
        let mut load: Option<PathBuf> = None;
        let mut open_web: Option<Option<String>> = None;
        let (_, close) = widgets::modal(ui, palette, 720.0, |ui| {
            widgets::title(ui, palette, self.source.title(), self.source.hint());
            // Search, file list and log can be taller than the window: the
            // middle scrolls, title and buttons stay.
            let middle = (ui.ctx().content_rect().height() - 300.0).clamp(240.0, 900.0);
            egui::ScrollArea::vertical()
                .id_salt("text_source_body")
                .max_height(middle)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some((label, url)) = self.source.web() {
                        let hover = if self.own_window {
                            format!("{url} im Browserfenster von proAVA")
                        } else {
                            format!("{url} im System-Browser")
                        };
                        if ui.button(label).on_hover_text(hover).clicked() {
                            open_web = Some(None);
                        }
                    }
                    if ui.button("GAEB-Datei wählen …").clicked() {
                        pick = true;
                    }
                    if self.read.is_some()
                        && self.source != TextSource::StlbBau
                        && ui
                            .button("Ganze Datei in die Bibliothek …")
                            .on_hover_text("Alle Zeilen der Datei (mit Titeln) in die eigene Bibliothek – Titel dort wählen.")
                            .clicked()
                    {
                        if let Some(file) = self.file.clone() {
                            result = StlbResult::ToLibrary(file);
                        }
                    }
                });
                // Never silently: why the pages open in the system browser.
                if let Some(reason) = &self.web_fallback {
                    ui.label(
                        RichText::new(format!("Öffnet im System-Browser: {reason}"))
                            .size(12.0)
                            .color(palette.warn),
                    );
                }
                if self.source == TextSource::AusschreibenDe {
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(RichText::new("In AUSSCHREIBEN.DE suchen").size(13.0))
                        .id_salt("tender_search")
                        .default_open(true)
                        .show(ui, |ui| {
                            if let Some(url) =
                                self.search.show(ui, palette, launcher, self.own_window)
                            {
                                open_web = Some(Some(url));
                            }
                        });
                }
                ui.add_space(6.0);
                let auto_label = if self.own_window {
                    "Neue Datei aus dem Browserfenster oder Download-Ordner sofort übernehmen"
                } else {
                    "Neue Datei aus dem Download-Ordner sofort übernehmen"
                };
                ui.checkbox(&mut self.auto, auto_label).on_hover_text(
                    "Kommt eine GAEB-Datei aus dem Browserfenster oder liegt genau eine neue GAEB-Datei vollständig im Download-Ordner, wird sie ohne weiteren Klick eingelesen und übernommen.",
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Ziel: {}", target.label))
                        .size(12.5)
                        .color(palette.text),
                );
                ui.add_space(4.0);
                self.watch_line(ui, palette, &mut load);
                ui.add_space(6.0);
                widgets::error_line(ui, palette, self.error.as_deref());
                if let Some(read) = &self.read {
                    self.positions_list(ui, palette, read.clone());
                }
                if !self.log.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Bisher übernommen")
                            .size(12.0)
                            .strong()
                            .color(palette.text_dim),
                    );
                    for line in &self.log {
                        ui.label(RichText::new(line).size(12.0).color(palette.text));
                    }
                }
                });
            ui.add_space(10.0);
            if let Some(again) = self.repeat_question(ui, palette) {
                result = again;
            }
            if let Some(note) = &self.taken_note {
                ui.label(
                    RichText::new(note)
                        .size(13.0)
                        .strong()
                        .color(palette.accent),
                );
                ui.add_space(4.0);
            }
            let count = self.chosen_indices().len();
            // Everything of this file is in, nothing new chosen: grey.
            let all_taken = count == 0 && self.taken.iter().any(|taken| *taken);
            if self.read.is_some() {
                let hint = if target.fills_placeholder {
                    "Die erste gewählte Position füllt den Platzhalter (seine Menge bleibt), die weiteren folgen dahinter. Danach ist der nächste offene Platzhalter das Ziel."
                } else {
                    "Die gewählten Positionen werden hinter der Auswahl eingefügt."
                };
                ui.label(RichText::new(hint).size(12.0).color(palette.text_dim));
                ui.add_space(4.0);
            }
            if self.auto {
                ui.label(
                    RichText::new("Neue Dateien werden sofort übernommen.")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.add_space(4.0);
            }
            let label = if all_taken {
                TAKEN_LABEL.to_string()
            } else {
                format!("Übernehmen ({count})")
            };
            let again = all_taken && self.repeat.is_none() && self.last_of_this_file().is_some();
            let pressed = buttons(
                ui,
                palette,
                &label,
                !all_taken && self.repeat.is_none(),
                again,
            );
            if pressed.closed {
                result = StlbResult::Cancelled;
            } else if pressed.again {
                // Explicit wish: the same entries once more, no question.
                if let Some(taken) = self
                    .last_of_this_file()
                    .and_then(|indices| self.apply(indices))
                {
                    result = taken;
                }
            } else if pressed.confirmed {
                let indices = self.chosen_indices();
                if indices.is_empty() {
                    self.error = Some("Bitte zuerst Positionen einlesen und wählen.".into());
                } else if let Some(taken) = self.request(indices) {
                    result = taken;
                }
            }
        });
        if pick {
            let start = self
                .downloads
                .first()
                .cloned()
                .unwrap_or_else(|| launcher.home_folder());
            let (title, hint) = self.source.picker();
            self.picker = Some(FileDialog::open(
                launcher,
                title,
                hint,
                start,
                &CATALOG_EXTENSIONS,
                false,
            ));
        }
        if let Some(file) = load {
            self.load(launcher, &file);
        }
        if let Some(url) = open_web {
            self.open_web(ui.ctx(), url, &target.label);
        }
        if close {
            StlbResult::Cancelled
        } else {
            result
        }
    }

    /// State of the Downloads watch: waiting, several new files, last file.
    fn watch_line(&mut self, ui: &mut egui::Ui, palette: &Palette, load: &mut Option<PathBuf>) {
        if let Some(note) = &self.web_note {
            ui.label(RichText::new(note).size(12.0).color(palette.accent));
        }
        if !self.offered.is_empty() {
            ui.label(
                RichText::new("Mehrere neue Dateien in Downloads – bitte eine wählen:")
                    .size(12.5)
                    .color(palette.accent),
            );
            for file in &self.offered {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(file_dialog::file_name(file)).color(palette.text))
                        .on_hover_text(file.display().to_string());
                    if ui.small_button("Einlesen").clicked() {
                        *load = Some(file.clone());
                    }
                });
            }
            return;
        }
        if let Some(file) = &self.file {
            let source = self
                .read
                .as_ref()
                .map(|read| read.source_in_archive.as_str())
                .filter(|source| !source.is_empty())
                .map(|source| format!(" → {source}"))
                .unwrap_or_default();
            ui.label(
                RichText::new(format!(
                    "Zuletzt eingelesen: {}{source}",
                    file_dialog::file_name(file)
                ))
                .size(12.0)
                .color(palette.text_dim),
            )
            .on_hover_text(file.display().to_string());
        }
        match self.downloads.first() {
            Some(folder) => ui.label(
                RichText::new(format!(
                    "Download-Ordner {} wird beobachtet (GAEB X81–X86, D81–D86, ZIP).",
                    folder.display()
                ))
                .size(12.0)
                .color(palette.text_dim),
            ),
            None => ui.label(
                RichText::new("Kein Download-Ordner gefunden – bitte die Datei wählen.")
                    .size(12.0)
                    .color(palette.text_dim),
            ),
        };
    }

    fn positions_list(&mut self, ui: &mut egui::Ui, palette: &Palette, read: CatalogPositions) {
        let count = if read.with_stlb_key > 0 || self.source == TextSource::StlbBau {
            format!(
                "{}, davon {} mit STLB-Bau-Kennung",
                crate::format::count(read.positions.len(), "Position", "Positionen"),
                read.with_stlb_key
            )
        } else {
            crate::format::count(read.positions.len(), "Position", "Positionen")
        };
        let texts = self
            .kinds
            .iter()
            .filter(|kind| **kind != RowKind::Position)
            .count();
        let count = if texts > 0 {
            format!(
                "{count} – {} als Text (Hinweis oder Ausführungsbeschreibung)",
                crate::format::count(texts, "Eintrag", "Einträge")
            )
        } else {
            count
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(count).size(12.5).color(palette.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Keine").clicked() {
                    self.chosen.iter_mut().for_each(|chosen| *chosen = false);
                }
                if ui.small_button("Alle").clicked() {
                    self.chosen.iter_mut().for_each(|chosen| *chosen = true);
                }
            });
        });
        for warning in &read.warnings {
            ui.label(RichText::new(warning).size(12.0).color(palette.warn));
        }
        let taken = self.taken.iter().filter(|taken| **taken).count();
        if taken > 0 {
            ui.label(
                RichText::new(format!(
                    "bereits übernommen ({})",
                    crate::format::count(taken, "Position", "Positionen")
                ))
                .size(12.5)
                .color(palette.accent),
            );
        }
        ui.add_space(4.0);
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.0, palette.line))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("stlb_positions")
                            .num_columns(8)
                            .spacing([12.0, 6.0])
                            .striped(false)
                            .show(ui, |ui| {
                                let head = |text: &str| {
                                    RichText::new(text)
                                        .size(12.0)
                                        .strong()
                                        .color(palette.text_dim)
                                };
                                ui.label("");
                                ui.label(head("OZ (Datei)"));
                                ui.label(head("Kurztext"));
                                ui.label(head("Menge"));
                                ui.label(head("Einheit"));
                                ui.label(head("KG"));
                                ui.label(head("Katalog / Hersteller"));
                                ui.label(head("Übernehmen als"));
                                ui.end_row();
                                for (index, position) in read.positions.iter().enumerate() {
                                    let taken = self.taken.get(index).copied().unwrap_or(false);
                                    if let Some(chosen) = self.chosen.get_mut(index) {
                                        let response = ui.checkbox(chosen, "");
                                        if taken {
                                            response.on_hover_text(
                                                "Schon übernommen – erneut wählen fragt vorher.",
                                            );
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                    ui.label(
                                        RichText::new(position.ordinal())
                                            .monospace()
                                            .color(palette.text_dim),
                                    );
                                    if taken {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} (übernommen)",
                                                position.short_text()
                                            ))
                                            .color(palette.text_dim),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(position.short_text())
                                                .color(palette.text),
                                        );
                                    }
                                    let quantity = quantity_text(position.quantity());
                                    ui.label(RichText::new(quantity).color(palette.text_dim));
                                    ui.label(
                                        RichText::new(position.unit()).color(palette.text_dim),
                                    );
                                    ui.label(
                                        RichText::new(
                                            catalog_cost_group(position).unwrap_or_default(),
                                        )
                                        .color(palette.text_dim),
                                    );
                                    let supplier = position
                                        .text_origin()
                                        .map(|origin| origin.supplier.trim())
                                        .filter(|supplier| !supplier.is_empty());
                                    match (position.catalog_line(), supplier) {
                                        (Some(line), _) => ui.label(
                                            RichText::new(line).size(12.0).color(palette.accent),
                                        ),
                                        (None, Some(supplier)) => ui.label(
                                            RichText::new(supplier).size(12.0).color(palette.text),
                                        ),
                                        (None, None) => ui.label(
                                            RichText::new("frei")
                                                .size(12.0)
                                                .color(palette.text_dim),
                                        ),
                                    };
                                    if let Some(kind) = self.kinds.get_mut(index) {
                                        let reason = read.kind(index).reason();
                                        let response =
                                            egui::ComboBox::from_id_salt(("stlb_kind", index))
                                                .selected_text(kind.label())
                                                .show_ui(ui, |ui| {
                                                    for option in RowKind::ALL {
                                                        ui.selectable_value(
                                                            kind,
                                                            option,
                                                            option.label(),
                                                        );
                                                    }
                                                })
                                                .response;
                                        if let Some(reason) = reason {
                                            response.on_hover_text(reason);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
    }

    /// The question before taking entries a second time; the result when
    /// answered with a take-over.
    fn repeat_question(&mut self, ui: &mut egui::Ui, palette: &Palette) -> Option<StlbResult> {
        let repeat = self.repeat.clone()?;
        let read = self.read.as_ref()?;
        let names: Vec<String> = repeat
            .repeated
            .iter()
            .filter_map(|&index| read.positions.get(index))
            .take(5)
            .map(|position| format!("• {} {}", position.ordinal(), position.short_text()))
            .collect();
        let new: Vec<usize> = repeat
            .indices
            .iter()
            .copied()
            .filter(|index| !repeat.repeated.contains(index))
            .collect();
        let mut answer = None;
        egui::Frame::new()
            .fill(palette.warn.gamma_multiply(0.12))
            .stroke(egui::Stroke::new(1.0, palette.warn))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new(REPEAT_QUESTION)
                        .size(13.0)
                        .strong()
                        .color(palette.text),
                );
                for name in &names {
                    ui.label(RichText::new(name).size(12.5).color(palette.text));
                }
                if repeat.repeated.len() > names.len() {
                    ui.label(format!(
                        "… und {} weitere",
                        repeat.repeated.len() - names.len()
                    ));
                }
                if !new.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "{} der Auswahl {} neu.",
                            crate::format::count(new.len(), "Eintrag", "Einträge"),
                            if new.len() == 1 { "ist" } else { "sind" }
                        ))
                        .size(12.5)
                        .color(palette.text_dim),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button(AGAIN_LABEL).clicked() {
                        answer = Some(repeat.indices.clone());
                    }
                    if !new.is_empty()
                        && ui
                            .button(format!("Nur neue übernehmen ({})", new.len()))
                            .clicked()
                    {
                        answer = Some(new.clone());
                    }
                    if ui.button("Nicht übernehmen").clicked() {
                        self.repeat = None;
                    }
                });
            });
        ui.add_space(6.0);
        answer.and_then(|indices| self.apply(indices))
    }

    fn load(&mut self, launcher: &Launcher, file: &Path) {
        self.file = Some(file.to_path_buf());
        self.offered.clear();
        // A file read anew: nothing of it is taken yet (a repeat of earlier
        // entries asks before it goes in).
        self.generation += 1;
        self.taken.clear();
        self.keys.clear();
        self.repeat = None;
        self.taken_note = None;
        if let Some(stamp) = stamp_of(file) {
            self.handled.insert(file.to_path_buf(), stamp);
        }
        match launcher.exchange().read_catalog_positions(file) {
            Ok(mut read) => {
                adopt_source(&mut read.positions, self.source, &launcher.now_iso());
                self.chosen = vec![true; read.positions.len()];
                self.taken = vec![false; read.positions.len()];
                self.keys = read.positions.iter().map(entry_key).collect();
                self.kinds = (0..read.positions.len())
                    .map(|index| read.kind(index).suggested)
                    .collect();
                self.error = None;
                self.read = Some(read);
            }
            Err(error) => {
                self.read = None;
                self.chosen.clear();
                self.kinds.clear();
                self.error = Some(error);
            }
        }
    }

    /// Looks at the Downloads folders; returns the file to read at once
    /// (exactly one new complete file and `auto` on). Several new files are
    /// offered as a list instead.
    fn scan(&mut self) -> Option<PathBuf> {
        let now: HashMap<PathBuf, Stamp> = self
            .downloads
            .iter()
            .flat_map(|folder| gaeb_files_since(folder, self.started))
            .filter_map(|path| stamp_of(&path).map(|stamp| (path, stamp)))
            .collect();
        let complete = settled(&self.seen, &now, &self.handled);
        self.seen = now;
        match complete.len() {
            0 => None,
            1 if self.auto => complete.into_iter().next(),
            _ => {
                for file in complete {
                    if let Some(stamp) = self.seen.get(&file) {
                        self.handled.insert(file.clone(), *stamp);
                    }
                    if !self.offered.contains(&file) {
                        self.offered.push(file);
                    }
                }
                None
            }
        }
    }
}

/// Files whose stamp did not change since the last scan (written completely)
/// and that were not handled in this state yet, newest last.
fn settled(
    before: &HashMap<PathBuf, Stamp>,
    now: &HashMap<PathBuf, Stamp>,
    handled: &HashMap<PathBuf, Stamp>,
) -> Vec<PathBuf> {
    let mut files: Vec<(&PathBuf, &Stamp)> = now
        .iter()
        .filter(|(path, stamp)| before.get(*path) == Some(*stamp))
        .filter(|(path, stamp)| handled.get(*path) != Some(*stamp))
        .collect();
    files.sort_by_key(|(path, stamp)| (stamp.modified, (*path).clone()));
    files.into_iter().map(|(path, _)| path.clone()).collect()
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        modified: meta.modified().ok()?,
        len: meta.len(),
    })
}

/// Main button after everything chosen of the file went in (grey; no
/// symbol characters in texts, see `tests/ui_symbols.rs`).
pub const TAKEN_LABEL: &str = "Übernommen";
/// Taking the same entries once more – only on explicit wish.
pub const AGAIN_LABEL: &str = "Noch einmal übernehmen";
/// Question before entries taken in this dialog go in a second time.
pub const REPEAT_QUESTION: &str = "Diese Positionen wurden bereits übernommen – noch einmal?";

/// Which of the buttons below the dialog was pressed.
struct Pressed {
    closed: bool,
    confirmed: bool,
    again: bool,
}

/// „Übernehmen“ (grey when `enabled` is false), „Schließen“ and – with
/// `again` – the small „Noch einmal übernehmen“ beside them (the dialog
/// stays open after a take-over).
fn buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    confirm_label: &str,
    enabled: bool,
    again: bool,
) -> Pressed {
    let mut pressed = Pressed {
        closed: false,
        confirmed: false,
        again: false,
    };
    widgets::button_row(ui, |ui| {
        let button = if enabled {
            egui::Button::new(RichText::new(confirm_label).color(palette.on_accent))
                .fill(palette.accent)
        } else {
            egui::Button::new(RichText::new(confirm_label).color(palette.text_dim))
        };
        if ui
            .add_enabled(enabled, button.min_size(egui::vec2(120.0, 32.0)))
            .clicked()
        {
            pressed.confirmed = true;
        }
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(RichText::new("Schließen").color(palette.text))
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            pressed.closed = true;
        }
        if again {
            ui.add_space(8.0);
            if ui
                .small_button(AGAIN_LABEL)
                .on_hover_text("Dieselben Positionen ein weiteres Mal ins LV – nur, wenn sie wirklich zweimal gebraucht werden.")
                .clicked()
            {
                pressed.again = true;
            }
        }
    });
    pressed
}

/// „24“, „1,5“ – without trailing zeros, empty for none.
fn quantity_text(quantity: Decimal) -> String {
    if quantity == Decimal::ZERO {
        return String::new();
    }
    let quantity = quantity.normalize();
    format_decimal_de(quantity, quantity.scale().min(3))
}

/// Where the next positions go, from the selected row.
pub fn target_of(lv: &Lv, selected: Option<usize>) -> StlbTarget {
    let row = selected.and_then(|index| lv.rows().get(index));
    match row {
        Some(LvRow::Position(position)) if position.is_stlb_placeholder() => {
            let quantity = if position.quantity() == Decimal::ZERO {
                String::new()
            } else {
                format!(
                    ", Menge {} {}",
                    quantity_text(position.quantity()),
                    position.unit()
                )
                .trim_end()
                .to_string()
            };
            StlbTarget {
                label: format!(
                    "Platzhalter {} „{}“{quantity} wird gefüllt",
                    position.ordinal(),
                    position.short_text()
                ),
                fills_placeholder: true,
            }
        }
        Some(LvRow::Position(position)) => StlbTarget {
            label: format!(
                "einfügen hinter {} „{}“",
                position.ordinal(),
                position.short_text()
            ),
            fills_placeholder: false,
        },
        Some(LvRow::Group(group)) => StlbTarget {
            label: format!(
                "einfügen am Anfang von Titel {} „{}“",
                group.ordinal(),
                group.short_text()
            ),
            fills_placeholder: false,
        },
        Some(_) => StlbTarget {
            label: "einfügen hinter der Auswahl".into(),
            fills_placeholder: false,
        },
        None => StlbTarget {
            label: "einfügen am Ende des LV".into(),
            fills_placeholder: false,
        },
    }
}

/// Result of [`take_over`]: rows that were changed or added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakenOver {
    pub first: usize,
    pub last: usize,
    pub count: usize,
    pub filled_placeholder: bool,
    /// How many entries went in as Hinweis or Ausführungsbeschreibung.
    pub texts: usize,
}

/// The next STLB placeholder after row `after` (the target of the next file).
pub fn next_placeholder(lv: &Lv, after: usize) -> Option<usize> {
    lv.rows()
        .iter()
        .enumerate()
        .skip(after + 1)
        .find(|(_, row)| {
            row.as_position()
                .is_some_and(|position| position.is_stlb_placeholder())
        })
        .map(|(index, _)| index)
}

/// GAEB files and zip archives (STLB-Bau Online may deliver a zip).
pub const CATALOG_EXTENSIONS: [&str; 11] = [
    "x81", "x82", "x83", "x84", "x86", "d81", "d82", "d83", "d84", "d86", "zip",
];
/// How deep below a download folder new files are looked for.
const SCAN_DEPTH: usize = 2;
/// Upper bound of directory entries looked at per scan.
const SCAN_LIMIT: usize = 5000;

/// Candidate download folders, most specific first, without duplicates:
/// XDG `user-dirs.dirs` (`XDG_DOWNLOAD_DIR`), Windows `%USERPROFILE%\Downloads`,
/// then `~/Downloads` and `~/Download` (also macOS).
pub fn download_folders(home: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        if let Some(folder) = xdg_download_dir(&text, home) {
            folders.push(folder);
        }
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        folders.push(PathBuf::from(profile).join("Downloads"));
    }
    folders.push(home.join("Downloads"));
    folders.push(home.join("Download"));
    let mut unique = Vec::new();
    for folder in folders {
        if !unique.contains(&folder) {
            unique.push(folder);
        }
    }
    unique
}

/// The download folder the system names first (Windows: the Known Folder
/// „Downloads“, also when it is moved to another drive or OneDrive), then
/// [`download_folders`].
pub fn system_download_folders(launcher: &Launcher) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = launcher
        .user_folders()
        .into_iter()
        .filter(|(kind, _)| *kind == proava_application::UserFolder::Downloads)
        .map(|(_, folder)| folder)
        .collect();
    for folder in download_folders(&launcher.home_folder()) {
        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

/// `XDG_DOWNLOAD_DIR="$HOME/Downloads"` from `user-dirs.dirs`.
fn xdg_download_dir(text: &str, home: &Path) -> Option<PathBuf> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("XDG_DOWNLOAD_DIR="))?;
    let value = line["XDG_DOWNLOAD_DIR=".len()..].trim().trim_matches('"');
    let path = if let Some(rest) = value.strip_prefix("$HOME") {
        home.join(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(value)
    };
    (path.is_absolute() && path != home).then_some(path)
}

/// GAEB files and zips in `folder` (and up to two levels of subfolders)
/// changed at or after `since`.
pub(crate) fn gaeb_files_since(folder: &Path, since: SystemTime) -> Vec<PathBuf> {
    let extensions: Vec<String> = CATALOG_EXTENSIONS
        .iter()
        .map(|extension| extension.to_string())
        .collect();
    let mut found = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), 0)];
    let mut seen = 0;
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > SCAN_LIMIT {
                return found;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                let hidden = path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'));
                if depth < SCAN_DEPTH && !hidden {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if !kind.is_file() || !file_dialog::matches_extension(&path, &extensions) {
                continue;
            }
            let Some(modified) = entry.metadata().ok().and_then(|meta| meta.modified().ok()) else {
                continue;
            };
            if modified >= since {
                found.push(path);
            }
        }
    }
    found
}

/// Newest GAEB file or zip in `folder` changed at or after `since`.
#[cfg(test)]
fn newest_gaeb_since(folder: &Path, since: SystemTime) -> Option<(SystemTime, PathBuf)> {
    gaeb_files_since(folder, since)
        .into_iter()
        .filter_map(|path| stamp_of(&path).map(|stamp| (stamp.modified, path)))
        .max_by_key(|(modified, _)| *modified)
}

/// Takes catalogue positions into the LV: a selected STLB placeholder is
/// filled with the first one (its quantity stays when set), the others follow
/// after it; without a placeholder all are inserted after the selection.
/// Order numbers follow the rules of the LV (renumbered or free numbers).
/// Every changed position gets the status „offen“ (it still has to be
/// checked) and the cost group from the STLB-Bau key when it has none and the
/// title above has another one. An entry chosen as Hinweis or
/// Ausführungsbeschreibung is converted after filling (a placeholder becomes
/// that kind, its quantity goes).
///
/// Public so that other menus (e.g. the context menu of the LV table) can use
/// the same action: they only need to open the dialog with
/// `Command::FromStlb`.
pub fn take_over(
    lv: &mut Lv,
    selected: Option<usize>,
    entries: Vec<(Position, RowKind)>,
) -> Result<TakenOver, String> {
    take_over_with(lv, selected, entries, lv_ops::NewTitles::Fill)
}

/// [`take_over`] with the rule for titles of new positions (a library:
/// [`lv_ops::NewTitles::Never`]).
pub fn take_over_with(
    lv: &mut Lv,
    selected: Option<usize>,
    entries: Vec<(Position, RowKind)>,
    titles: lv_ops::NewTitles,
) -> Result<TakenOver, String> {
    if entries.is_empty() {
        return Err("Keine Position gewählt.".to_string());
    }
    let before = lv.clone();
    let mut sources = entries;
    let mut first = None;
    let mut last = selected;
    let mut count = 0;
    let mut texts = 0;
    let placeholder = selected.filter(|&index| {
        lv.rows()
            .get(index)
            .and_then(LvRow::as_position)
            .is_some_and(Position::is_stlb_placeholder)
    });
    // Converts the position just filled at `index` into the chosen kind
    // (a Standardbeschreibung is a text, Anwender 18.11.).
    let convert = |lv: &mut Lv, index: usize, kind: RowKind| -> Result<(), String> {
        if kind != RowKind::Position {
            lv.convert_row(index, kind)?;
        }
        Ok(())
    };
    if let Some(index) = placeholder {
        let locked = lv
            .rows()
            .get(index)
            .and_then(LvRow::as_position)
            .is_some_and(|position| position.status() == EditStatus::Done);
        if locked {
            return Err(format!(
                "Der Platzhalter ist fertig und damit gesperrt – {}.",
                proava_domain::LOCKED_HINT
            ));
        }
        let (source, kind) = sources.remove(0);
        if let Some(target) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
            fill_from_catalog(target, &source);
        }
        if let Err(error) = convert(lv, index, kind) {
            *lv = before;
            return Err(error);
        }
        texts += usize::from(kind != RowKind::Position);
        first = Some(index);
        count += 1;
    }
    for (source, kind) in sources {
        let index = match lv_ops::insert_position_with(lv, last, false, titles) {
            Ok(index) => index,
            Err(error) => {
                *lv = before;
                return Err(error);
            }
        };
        if let Some(target) = lv.row_mut(index).and_then(LvRow::as_position_mut) {
            target.set_quantity(Decimal::ZERO);
            target.set_kind(PositionKind::Normal);
            fill_from_catalog(target, &source);
        }
        if let Err(error) = convert(lv, index, kind) {
            *lv = before;
            return Err(error);
        }
        texts += usize::from(kind != RowKind::Position);
        first.get_or_insert(index);
        last = Some(index);
        count += 1;
    }
    let (Some(first), Some(last)) = (first, last) else {
        return Err("Keine Position gewählt.".to_string());
    };
    for index in first..=last {
        adopt_catalog_cost_group(lv, index);
    }
    Ok(TakenOver {
        first,
        last,
        count,
        filled_placeholder: placeholder.is_some(),
        texts,
    })
}

#[cfg(test)]
mod tests {
    use proava_domain::{OzMask, RichText, StlbReference};

    use super::*;

    fn catalog(text: &str, qty: i64) -> Position {
        let mut position = Position::new("01.0010", text, Decimal::from(qty), "St", None);
        position.set_long_text(RichText::plain(&format!("{text} Langtext")));
        position.attach_stlb(StlbReference {
            catalog: "STLB-Bau".into(),
            group: 9001,
            ..StlbReference::default()
        });
        position
    }

    fn as_positions(list: Vec<Position>) -> Vec<(Position, RowKind)> {
        list.into_iter()
            .map(|position| (position, RowKind::Position))
            .collect()
    }

    fn lv() -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(proava_domain::LvGroup::new(0, "", "GA"));
        lv.push_position(Position::new("", "Vorher", Decimal::ONE, "St", None));
        let mut placeholder = Position::stlb("Raumbediengerät");
        placeholder.set_quantity(Decimal::from(7));
        lv.push_position(placeholder);
        lv.push_position(Position::new("", "Nachher", Decimal::ONE, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn placeholder_is_filled_and_the_rest_follows() {
        let mut lv = lv();
        let taken = take_over(
            &mut lv,
            Some(2),
            as_positions(vec![catalog("RBG", 4), catalog("Fühler", 2)]),
        )
        .unwrap();
        assert_eq!(
            taken,
            TakenOver {
                first: 2,
                last: 3,
                count: 2,
                filled_placeholder: true,
                texts: 0,
            }
        );
        let positions: Vec<&Position> = lv.positions().collect();
        let texts: Vec<&str> = positions.iter().map(|p| p.short_text()).collect();
        assert_eq!(texts, vec!["Vorher", "RBG", "Fühler", "Nachher"]);
        assert_eq!(
            positions[1].quantity(),
            Decimal::from(7),
            "Menge des Platzhalters"
        );
        assert!(!positions[1].is_stlb_placeholder());
        assert_eq!(positions[2].quantity(), Decimal::from(2));
        assert!(positions
            .iter()
            .skip(1)
            .take(2)
            .all(|p| p.stlb_reference().is_some()));
        assert!(!positions[2].stlb_text_changed());
        let ordinals: Vec<&str> = positions.iter().map(|p| p.ordinal()).collect();
        assert_eq!(ordinals, vec!["01.0010", "01.0020", "01.0030", "01.0040"]);
    }

    #[test]
    fn without_placeholder_positions_are_inserted() {
        let mut lv = lv();
        take_over(&mut lv, Some(1), as_positions(vec![catalog("RBG", 4)])).unwrap();
        let texts: Vec<&str> = lv.positions().map(|p| p.short_text()).collect();
        assert_eq!(texts, vec!["Vorher", "RBG", "Raumbediengerät", "Nachher"]);
        assert!(take_over(&mut lv, Some(1), as_positions(Vec::new())).is_err());
    }

    #[test]
    fn taken_positions_are_open_get_the_catalog_cost_group_and_lead_to_the_next_placeholder() {
        let mut lv = lv();
        lv.push_position(Position::stlb("Fühler"));
        lv.renumber();
        // Titel mit KG 481: gleiche KG aus STLB-Bau wird geerbt, andere gesetzt.
        if let Some(LvRow::Group(group)) = lv.row_mut(0) {
            group.set_cost_group(Some("481".into()));
        }
        let mut same = catalog("RBG", 1);
        let mut other = catalog("Kabel", 1);
        if let Some(stlb) = same.stlb_reference().cloned() {
            same.attach_stlb(StlbReference {
                cost_group: "481".into(),
                ..stlb
            });
        }
        if let Some(stlb) = other.stlb_reference().cloned() {
            other.attach_stlb(StlbReference {
                cost_group: "444".into(),
                ..stlb
            });
        }
        let placeholder = 2;
        if let Some(position) = lv.row_mut(placeholder).and_then(LvRow::as_position_mut) {
            position.set_status(EditStatus::InProgress);
        }
        let taken = take_over(&mut lv, Some(placeholder), as_positions(vec![same, other])).unwrap();
        let rows = lv.rows();
        let filled = rows[taken.first].as_position().unwrap();
        let added = rows[taken.last].as_position().unwrap();
        assert_eq!(filled.status(), EditStatus::Open, "übernommen = offen");
        assert_eq!(added.status(), EditStatus::Open);
        assert_eq!(filled.cost_group(), None, "481 kommt schon vom Titel");
        assert_eq!(added.cost_group(), Some("444"));
        let next = next_placeholder(&lv, taken.last).unwrap();
        assert_eq!(rows[next].as_position().unwrap().short_text(), "Fühler");
        assert!(target_of(&lv, Some(next)).fills_placeholder);
        assert!(target_of(&lv, Some(next)).label.contains("„Fühler“"));
        assert_eq!(quantity_text(Decimal::new(24000, 3)), "24");
        assert_eq!(quantity_text(Decimal::new(15, 1)), "1,5");
        assert!(!target_of(&lv, Some(taken.last)).fills_placeholder);
        assert_eq!(target_of(&lv, None).label, "einfügen am Ende des LV");

        // Ein fertiger Platzhalter ist gesperrt.
        if let Some(position) = lv.row_mut(next).and_then(LvRow::as_position_mut) {
            position.set_status(EditStatus::Done);
        }
        let before = lv.clone();
        let error = take_over(
            &mut lv,
            Some(next),
            as_positions(vec![catalog("Fühler", 1)]),
        )
        .unwrap_err();
        assert!(error.contains("gesperrt"), "{error}");
        assert_eq!(lv, before);
    }

    #[test]
    fn source_of_the_dialog_goes_into_the_origin_and_travels_with_the_text() {
        let with = |source: &str, retrieved: &str| {
            let mut position = Position::new("01.0010", "Drehantrieb", Decimal::ONE, "St", None);
            position.set_text_origin(Some(TextOrigin {
                source: source.into(),
                supplier: "Muster GmbH".into(),
                retrieved: retrieved.into(),
                ..TextOrigin::default()
            }));
            position
        };
        let mut read = vec![
            with("Herstellerportal", "2026-11-16"),
            with("STLB-Bau", ""),
            Position::new("01.0030", "ohne", Decimal::ONE, "St", None),
        ];
        adopt_source(&mut read, TextSource::AusschreibenDe, "2026-11-17T23:05:00");
        let origins: Vec<(String, String)> = read
            .iter()
            .map(|p| {
                let origin = p.text_origin().unwrap();
                (origin.source.clone(), origin.retrieved.clone())
            })
            .collect();
        assert_eq!(
            origins,
            vec![
                ("AUSSCHREIBEN.DE".to_string(), "2026-11-16".to_string()),
                ("STLB-Bau".to_string(), "2026-11-17".to_string()),
                ("AUSSCHREIBEN.DE".to_string(), "2026-11-17".to_string()),
            ]
        );
        let mut dropped = vec![with("Herstellerportal", "2026-11-16"), with("", "")];
        adopt_source(&mut dropped, TextSource::GaebFile, "2026-11-17");
        assert_eq!(dropped[0].text_origin().unwrap().source, "Herstellerportal");
        assert_eq!(dropped[1].text_origin().unwrap().source, "GAEB-Datei");

        // Without a placeholder: inserted after the selection, Herkunft kept.
        let mut lv = lv();
        let taken = take_over(&mut lv, Some(1), as_positions(vec![read.remove(0)])).unwrap();
        assert!(!taken.filled_placeholder);
        let added = lv.rows()[taken.first].as_position().unwrap();
        assert_eq!(added.short_text(), "Drehantrieb");
        assert_eq!(added.text_origin().unwrap().source, "AUSSCHREIBEN.DE");
        assert_eq!(added.text_origin().unwrap().supplier, "Muster GmbH");
        assert_eq!(
            TextSource::AusschreibenDe.web().unwrap().1,
            AUSSCHREIBEN_URL
        );
        assert!(TextSource::GaebFile.web().is_none());
    }

    #[test]
    fn a_new_download_is_taken_only_when_it_is_complete_and_only_once() {
        let at = |seconds: u64, len: u64| Stamp {
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(seconds),
            len,
        };
        let file = PathBuf::from("/dl/LV.X83");
        let other = PathBuf::from("/dl/LV(1).X83");
        let none = HashMap::new();
        let writing = HashMap::from([(file.clone(), at(10, 100))]);
        let done = HashMap::from([(file.clone(), at(11, 400))]);
        assert!(
            settled(&none, &writing, &none).is_empty(),
            "eben erst gesehen"
        );
        assert!(settled(&writing, &done, &none).is_empty(), "wächst noch");
        assert_eq!(settled(&done, &done, &none), vec![file.clone()]);
        assert!(settled(&done, &done, &done).is_empty(), "schon übernommen");
        let rewritten = HashMap::from([(file.clone(), at(20, 400))]);
        assert_eq!(
            settled(&rewritten, &rewritten, &done),
            vec![file.clone()],
            "neu gespeichert = wieder neu"
        );
        let both = HashMap::from([(file.clone(), at(11, 400)), (other.clone(), at(12, 50))]);
        assert_eq!(
            settled(&both, &both, &none),
            vec![file, other],
            "älteste zuerst"
        );
    }

    #[test]
    fn finds_only_new_gaeb_files_also_in_subfolders_and_zips() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-stlb-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        std::fs::create_dir_all(folder.join("STLB/export/zu-tief")).unwrap();
        std::fs::write(folder.join("alt.X83"), b"x").unwrap();
        let since = SystemTime::now() + Duration::from_millis(5);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(newest_gaeb_since(&folder, since), None);
        std::fs::write(folder.join("notiz.txt"), b"x").unwrap();
        std::fs::write(folder.join("STLB.X81"), b"x").unwrap();
        let found = |folder: &Path| newest_gaeb_since(folder, since).map(|(_, path)| path);
        assert_eq!(found(&folder), Some(folder.join("STLB.X81")));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(folder.join("STLB/export/LV.zip"), b"x").unwrap();
        assert_eq!(found(&folder), Some(folder.join("STLB/export/LV.zip")));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(folder.join("STLB/export/zu-tief/neu.X83"), b"x").unwrap();
        assert_eq!(
            found(&folder),
            Some(folder.join("STLB/export/LV.zip")),
            "nur zwei Ebenen"
        );
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn download_folder_from_user_dirs() {
        let home = Path::new("/home/test");
        let text = "# Kommentar\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads/Browser\"\n";
        assert_eq!(
            xdg_download_dir(text, home),
            Some(PathBuf::from("/home/test/Downloads/Browser"))
        );
        assert_eq!(
            xdg_download_dir("XDG_DOWNLOAD_DIR=\"/daten/dl\"", home),
            Some(PathBuf::from("/daten/dl"))
        );
        assert_eq!(xdg_download_dir("XDG_DOWNLOAD_DIR=\"$HOME/\"", home), None);
        let folders = download_folders(home);
        assert!(folders.contains(&home.join("Downloads")));
        assert!(folders.contains(&home.join("Download")));
    }
}
