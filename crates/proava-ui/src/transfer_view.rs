//! „Aus anderem Projekt / GAEB / Bibliothek übernehmen“: a second outline in
//! its own window – choose the source, tick titles or positions (search,
//! long text preview), take them over behind the selection of the open LV.
//! The source is only read, never saved.
//!
//! The search goes through Kurztext, Langtext, Bieterangaben, Hinweise and
//! Ausführungsbeschreibungen (`proava_domain::lv_search`, the same as the
//! MCP tool `lv_suchen`); „In allen Projekten unter … suchen“ runs it over
//! every project below a folder on a background thread and lists the hits –
//! a click loads that project as the source with its hit shown.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::{self, RichText};
use proava_application::lv_search::SearchOutcome;
use proava_domain::lv_search::{self, LvQuery};
use proava_domain::transfer::{self, Transfer, TransferOptions};
use proava_domain::{Lv, LvRow, Project, RowId};

use crate::jobs::{Job, Poll};
use crate::theme::Palette;

/// Payload while rows are dragged from the window (or the side window
/// „Bibliothek“) into the LV table.
#[derive(Clone, Copy, Debug)]
pub struct TransferDrag {
    pub from_library: bool,
}

pub struct TransferView {
    /// Closed titles per source file and LV, kept while the program runs.
    remembered: HashMap<(PathBuf, usize), HashSet<RowId>>,
    source: Option<Source>,
    /// Counts loaded sources (the search cache belongs to one).
    generation: u64,
    error: Option<String>,
    search: String,
    /// Hits of `search` in the shown LV, recomputed when the text, the LV or
    /// the source changes.
    cache: Option<SearchCache>,
    checked: HashSet<RowId>,
    collapsed: HashSet<RowId>,
    preview: Option<RowId>,
    pub options: TransferOptions,
    lv_index: usize,
    open: bool,
    /// Folder of „In allen Projekten unter … suchen“ (kept while the program runs).
    folder: Option<PathBuf>,
    folder_search: Option<FolderSearch>,
    folder_results: Option<FolderResults>,
    /// A hit chosen in the folder results: shown once its project is loaded.
    pending: Option<(PathBuf, usize, RowId)>,
}

/// The search in the shown LV.
struct SearchCache {
    key: (String, usize, u64),
    /// Rows to show (hits and the titles above), LV order.
    rows: Vec<usize>,
    /// The hit rows with the excerpt around the find.
    hits: HashMap<RowId, String>,
}

/// A running search over all projects below a folder.
pub struct FolderSearch {
    folder: PathBuf,
    text: String,
    job: Job<(SearchOutcome, bool)>,
    cancel: Arc<AtomicBool>,
}

struct FolderResults {
    folder: PathBuf,
    text: String,
    outcome: SearchOutcome,
    /// More project files than one run searches.
    more: bool,
    cancelled: bool,
}

struct Source {
    path: PathBuf,
    project: Project,
    library: bool,
    warnings: Vec<String>,
}

pub enum TransferRequest {
    /// Choose a file (project or GAEB).
    PickFile,
    /// Load this source (recent entry or the library).
    Load {
        path: PathBuf,
        library: bool,
    },
    Take(Transfer),
    /// Search every project below this folder; `None` = choose the folder first.
    SearchFolder(Option<PathBuf>),
}

impl Default for TransferView {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferView {
    pub fn new() -> Self {
        Self {
            remembered: HashMap::new(),
            source: None,
            generation: 0,
            error: None,
            search: String::new(),
            cache: None,
            checked: HashSet::new(),
            collapsed: HashSet::new(),
            preview: None,
            options: TransferOptions::default(),
            lv_index: 0,
            open: true,
            folder: None,
            folder_search: None,
            folder_results: None,
            pending: None,
        }
    }

    /// The search text (for „In allen Projekten unter … suchen“).
    pub fn search_text(&self) -> &str {
        &self.search
    }

    /// The folder last searched (start of the folder choice).
    pub fn search_folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    /// A search over the projects below `folder` was started.
    pub fn searching(
        &mut self,
        folder: PathBuf,
        text: String,
        job: Job<(SearchOutcome, bool)>,
        cancel: Arc<AtomicBool>,
    ) {
        self.folder = Some(folder.clone());
        self.folder_results = None;
        self.folder_search = Some(FolderSearch {
            folder,
            text,
            job,
            cancel,
        });
    }

    /// Takes the result of a finished folder search.
    fn poll_folder_search(&mut self, ctx: &egui::Context) {
        let Some(search) = &self.folder_search else {
            return;
        };
        match search.job.poll() {
            Poll::Running => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Poll::Done((outcome, more)) => {
                let search = self.folder_search.take().expect("checked above");
                self.folder_results = Some(FolderResults {
                    cancelled: search.cancel.load(Ordering::Relaxed),
                    folder: search.folder,
                    text: search.text,
                    outcome,
                    more,
                });
            }
            Poll::Lost => {
                self.folder_search = None;
                self.error = Some("Die Suche in den Projekten ist abgebrochen.".to_string());
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn reopen(&mut self) {
        self.open = true;
    }

    fn remember(&mut self) {
        if let Some(source) = &self.source {
            self.remembered
                .insert((source.path.clone(), self.lv_index), self.collapsed.clone());
        }
    }

    fn restore(&mut self) {
        let Some(source) = &self.source else {
            return;
        };
        if let Some(closed) = self.remembered.get(&(source.path.clone(), self.lv_index)) {
            self.collapsed = closed.clone();
        }
    }

    /// Puts a read source in place (or its error).
    pub fn loaded(
        &mut self,
        path: &Path,
        library: bool,
        result: Result<(Project, Vec<String>), String>,
    ) {
        match result {
            Ok((project, warnings)) => {
                self.remember();
                self.collapsed = project
                    .lv()
                    .rows()
                    .iter()
                    .filter(|row| matches!(row, LvRow::Group(group) if group.level() > 0))
                    .map(LvRow::id)
                    .collect();
                self.lv_index = project.active_lv();
                self.source = Some(Source {
                    path: path.to_path_buf(),
                    project,
                    library,
                    warnings,
                });
                self.generation += 1;
                self.checked.clear();
                self.preview = None;
                self.error = None;
                self.restore();
                // A hit chosen in the folder results: its LV, shown below.
                if let Some((file, lv_index, row)) = self.pending.take() {
                    if file == path {
                        if lv_index != self.lv_index {
                            self.remember();
                            self.lv_index = lv_index;
                            self.restore();
                        }
                        self.preview = Some(row);
                    }
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn lv(&self) -> Option<&Lv> {
        let source = self.source.as_ref()?;
        source
            .project
            .lv_at(self.lv_index)
            .or(Some(source.project.lv()))
    }

    /// The chosen rows as a transfer (nothing when none is ticked).
    pub fn transfer(&self) -> Option<Transfer> {
        let source = self.source.as_ref()?;
        let lv = self.lv()?;
        let ids: Vec<RowId> = lv
            .rows()
            .iter()
            .map(LvRow::id)
            .filter(|id| self.checked.contains(id))
            .collect();
        (!ids.is_empty()).then(|| transfer::collect(&source.project, lv, &ids, self.options))
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        recent: &[PathBuf],
        library: &Path,
    ) -> Option<TransferRequest> {
        self.poll_folder_search(ctx);
        let mut request = None;
        let mut open = self.open;
        // Esc closes it right after opening, no click first (Anwender 18.11.).
        if open && crate::widgets::esc_closes_window(ctx) {
            open = false;
        }
        egui::Window::new("Übernehmen aus Projekt, GAEB oder Bibliothek")
            .id(egui::Id::unique("transfer_window"))
            .open(&mut open)
            .default_size([620.0, 720.0])
            .min_size([520.0, 560.0])
            .default_pos(ctx.content_rect().right_top() + egui::vec2(-660.0, 130.0))
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                request = self.content(ui, palette, recent, library);
            });
        self.open = open;
        self.remember();
        request
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        recent: &[PathBuf],
        library: &Path,
    ) -> Option<TransferRequest> {
        let mut request = None;
        ui.set_min_size(egui::vec2(560.0, 560.0));
        crate::widgets::row(ui, |ui| {
            ui.label(RichText::new("Quelle").size(12.5).color(palette.text_dim));
            let current = self
                .source
                .as_ref()
                .map(|source| source_label(&source.path, source.library))
                .unwrap_or_else(|| "– bitte wählen –".to_string());
            egui::ComboBox::from_id_salt("transfer_source")
                .width(360.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(false, "Eigene Bibliothek").clicked() {
                        request = Some(TransferRequest::Load {
                            path: library.to_path_buf(),
                            library: true,
                        });
                    }
                    for path in recent.iter().filter(|path| path.as_path() != library) {
                        if ui
                            .selectable_label(false, source_label(path, false))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            request = Some(TransferRequest::Load {
                                path: path.clone(),
                                library: false,
                            });
                        }
                    }
                });
            if ui.button("Datei wählen …").clicked() {
                request = Some(TransferRequest::PickFile);
            }
        });
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).size(12.5).color(palette.error));
        }
        if let Some(source) = self.source.as_ref() {
            for warning in source.warnings.iter().take(2) {
                ui.label(RichText::new(warning).size(11.5).color(palette.text_dim));
            }
            if source.project.lv_count() > 1 {
                let labels: Vec<String> = source
                    .project
                    .lvs()
                    .map(|lv| {
                        if lv.number().is_empty() {
                            lv.title().to_string()
                        } else {
                            format!("{} – {}", lv.number(), lv.title())
                        }
                    })
                    .collect();
                let mut index = self.lv_index;
                egui::ComboBox::from_id_salt("transfer_lv")
                    .width(360.0)
                    .selected_text(labels.get(index).cloned().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (number, label) in labels.iter().enumerate() {
                            ui.selectable_value(&mut index, number, label);
                        }
                    });
                if index != self.lv_index {
                    self.remember();
                    self.lv_index = index;
                    self.restore();
                    self.checked.clear();
                    self.preview = None;
                }
            }
        }
        self.refresh_search();
        let has_source = self.source.is_some();
        let searching = !self.search.trim().is_empty();
        crate::widgets::row(ui, |ui| {
            let caption = ui.label(RichText::new("Suche").size(12.5).color(palette.text_dim));
            crate::widgets::search_mark(ui, palette);
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Wörter in Kurz- und Langtext, OZ …")
                    .desired_width(240.0),
            )
            .labelled_by(caption.id)
            .on_hover_text(
                "Alle Wörter müssen vorkommen – in Kurztext, Langtext, Bieterangaben, Hinweisen \
                 oder Ausführungsbeschreibungen; Groß/klein und Umlaute egal.",
            );
            let hits = self.cache.as_ref().map_or(0, |cache| cache.hits.len());
            if has_source && searching {
                ui.label(
                    RichText::new(crate::format::count(hits, "Treffer", "Treffer"))
                        .size(12.5)
                        .color(palette.text_dim),
                );
            }
            if ui
                .add_enabled(hits > 0, egui::Button::new("Treffer ankreuzen"))
                .on_hover_text("Kreuzt alle Treffer der Suche an.")
                .clicked()
            {
                if let Some(cache) = &self.cache {
                    self.checked.extend(cache.hits.keys().copied());
                }
            }
            if ui
                .add_enabled(has_source, egui::Button::new("Alles zuklappen"))
                .clicked()
            {
                if let Some(lv) = self.lv() {
                    self.collapsed = lv
                        .rows()
                        .iter()
                        .filter(|row| row.is_group())
                        .map(LvRow::id)
                        .collect();
                }
            }
            if ui
                .add_enabled(has_source, egui::Button::new("Auswahl leeren"))
                .clicked()
            {
                self.checked.clear();
            }
        });
        ui.horizontal(|ui| {
            let label = match &self.folder {
                Some(folder) => format!(
                    "In allen Projekten unter „{}“ suchen",
                    crate::file_dialog::file_name(folder)
                ),
                None => "In allen Projekten unter … suchen".to_string(),
            };
            let hover = match &self.folder {
                Some(folder) => format!(
                    "Durchsucht jedes proAVA-Projekt unter {} (mit Unterordnern, ohne „Alte \
                     Versionen“) nach dem Suchtext.",
                    folder.display()
                ),
                None => "Ordner wählen und jedes proAVA-Projekt darunter (mit Unterordnern, ohne \
                         „Alte Versionen“) nach dem Suchtext durchsuchen."
                    .to_string(),
            };
            if ui
                .add_enabled(
                    searching && self.folder_search.is_none(),
                    egui::Button::new(label),
                )
                .on_hover_text(hover)
                .on_disabled_hover_text("Erst einen Suchtext eingeben.")
                .clicked()
            {
                request = Some(TransferRequest::SearchFolder(self.folder.clone()));
            }
            if self.folder.is_some()
                && ui
                    .add_enabled(
                        searching && self.folder_search.is_none(),
                        egui::Button::new("Anderer Ordner …"),
                    )
                    .clicked()
            {
                request = Some(TransferRequest::SearchFolder(None));
            }
        });
        if let Some(chosen) = self.folder_results_ui(ui, palette) {
            request = Some(chosen);
        }
        let Some(lv) = self.lv().cloned() else {
            ui.add_space(20.0);
            ui.label(
                RichText::new(
                    "Ein proAVA-Projekt, eine GAEB-Datei (X81–X86, D81–D86) oder die eigene \
                     Bibliothek wählen – oder einen Suchtext eingeben und in allen Projekten \
                     unter einem Ordner suchen. Die Quelle wird nur gelesen.",
                )
                .color(palette.text_dim),
            );
            return request;
        };
        ui.separator();
        let hit_excerpts = self
            .cache
            .as_ref()
            .filter(|_| searching)
            .map(|cache| cache.hits.clone());
        let rows: Vec<(usize, usize)> = match &self.cache {
            Some(cache) if searching => {
                let depths = proava_domain::library::depths(&lv);
                cache
                    .rows
                    .iter()
                    .map(|&index| (index, depths[index]))
                    .collect()
            }
            _ => shown_rows(&lv, &self.collapsed, ""),
        };
        let tree_height = (ui.available_height() - 210.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("transfer_tree")
            .max_height(tree_height)
            .auto_shrink([false, false])
            .show_rows(ui, 24.0, rows.len(), |ui, range| {
                for &(index, depth) in &rows[range] {
                    let row = &lv.rows()[index];
                    let id = row.id();
                    // While searching, titles that only lead to a hit are dim.
                    let excerpt = hit_excerpts.as_ref().map(|hits| hits.get(&id));
                    let context_only = matches!(excerpt, Some(None));
                    ui.horizontal(|ui| {
                        ui.set_height(22.0);
                        ui.add_space(depth as f32 * 14.0);
                        if row.is_group() && hit_excerpts.is_none() {
                            let closed = self.collapsed.contains(&id);
                            if ui.small_button(if closed { "+" } else { "–" }).clicked() {
                                if closed {
                                    self.collapsed.remove(&id);
                                } else {
                                    self.collapsed.insert(id);
                                }
                            }
                        } else {
                            ui.add_space(22.0);
                        }
                        let mut ticked =
                            self.checked.contains(&id) || covered(&lv, index, &self.checked);
                        if ui.checkbox(&mut ticked, "").changed() {
                            if ticked {
                                self.checked.insert(id);
                            } else {
                                self.checked.remove(&id);
                            }
                        }
                        let text = match row {
                            LvRow::Note(note) => format!("Hinweis: {}", note.title()),
                            other => other.short_text(),
                        };
                        let color = if context_only {
                            palette.text_dim
                        } else if row.is_group() {
                            palette.group_text
                        } else {
                            palette.text
                        };
                        let label = RichText::new(format!("{}  {}", row.ordinal(), text))
                            .size(13.0)
                            .color(color);
                        let hover = match excerpt.flatten() {
                            Some(found) if !found.is_empty() => format!(
                                "{found}\nKlick: Langtext unten · Ziehen: in die LV-Tabelle"
                            ),
                            _ => "Klick: Langtext unten · Ziehen: in die LV-Tabelle".to_string(),
                        };
                        let response = ui
                            .add(
                                egui::Label::new(label)
                                    .sense(egui::Sense::click_and_drag())
                                    .truncate(),
                            )
                            .on_hover_text(hover);
                        if response.clicked() {
                            self.preview = Some(id);
                        }
                        if response.drag_started() {
                            // Dragging an unticked row takes just this row.
                            if !self.checked.contains(&id) && !covered(&lv, index, &self.checked) {
                                self.checked = [id].into();
                            }
                            egui::DragAndDrop::set_payload(
                                ui.ctx(),
                                TransferDrag {
                                    from_library: false,
                                },
                            );
                        }
                    });
                }
            });
        ui.separator();
        // Vorschau
        let preview = self
            .preview
            .and_then(|id| lv.row_by_id(id))
            .map(|row| match row {
                LvRow::Position(position) => format!(
                    "{} {}\n{} {}\n\n{}",
                    position.ordinal(),
                    position.short_text(),
                    crate::numbers::quantity_text(position.quantity()),
                    position.unit(),
                    position.long_text().to_plain()
                ),
                LvRow::Note(note) => note.text().to_plain(),
                LvRow::Performance(performance) => {
                    format!(
                        "{}\n\n{}",
                        performance.heading(),
                        performance.long_text().to_plain()
                    )
                }
                LvRow::Group(group) => format!("{} {}", group.ordinal(), group.short_text()),
                LvRow::Spacer(_) => "Abstandshalter (Leerzeile)".to_string(),
            });
        egui::ScrollArea::vertical()
            .id_salt("transfer_preview")
            .max_height(110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        preview.unwrap_or_else(|| {
                            "Klick auf einen Text zeigt hier den Langtext.".into()
                        }),
                    )
                    .size(12.5)
                    .color(palette.text_dim),
                );
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.options.quantities, "Mengen übernehmen");
            ui.checkbox(&mut self.options.prices, "Preise übernehmen");
        });
        let transfer = self.transfer();
        let count = transfer.as_ref().map(Transfer::position_count).unwrap_or(0);
        let rows_count = transfer.as_ref().map(|t| t.rows.len()).unwrap_or(0);
        ui.horizontal(|ui| {
            let label = format!(
                "Übernehmen ({}, {})",
                crate::format::count(count, "Position", "Positionen"),
                crate::format::count(rows_count, "Zeile", "Zeilen")
            );
            if ui
                .add_enabled(
                    rows_count > 0,
                    egui::Button::new(RichText::new(label).color(palette.on_accent)).fill(palette.accent),
                )
                .on_hover_text("Fügt hinter der Auswahl im offenen LV ein; Status „offen“, freie Ordnungszahlen.")
                .clicked()
            {
                request = transfer.map(TransferRequest::Take);
            }
        });
        request
    }

    /// Recomputes the hits when the text, the LV or the source changed.
    fn refresh_search(&mut self) {
        let key = (
            self.search.trim().to_string(),
            self.lv_index,
            self.generation,
        );
        if self.cache.as_ref().is_some_and(|cache| cache.key == key) {
            return;
        }
        let Some(lv) = self.lv() else {
            self.cache = None;
            return;
        };
        let (rows, hits) = search_rows(lv, &key.0);
        self.cache = Some(SearchCache { key, rows, hits });
    }

    /// The running folder search or its result list; a click on a hit asks
    /// to load its project.
    fn folder_results_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
    ) -> Option<TransferRequest> {
        if let Some(search) = &self.folder_search {
            let mut cancel = false;
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(
                    RichText::new(format!(
                        "Suche „{}“ in den Projekten unter {} …",
                        search.text,
                        search.folder.display()
                    ))
                    .size(12.5)
                    .color(palette.text_dim),
                );
                cancel = ui.button("Abbrechen").clicked();
            });
            if cancel {
                search.cancel.store(true, Ordering::Relaxed);
            }
            return None;
        }
        let results = self.folder_results.as_ref()?;
        let mut request = None;
        let mut close = false;
        let outcome = &results.outcome;
        let mut summary = format!(
            "„{}“: {} in {} unter {} ({} durchsucht",
            results.text,
            crate::format::count(outcome.hit_count(), "Treffer", "Treffer"),
            crate::format::count(outcome.sources.len(), "Projekt", "Projekten"),
            results.folder.display(),
            crate::format::count(outcome.searched, "Projekt", "Projekte"),
        );
        if !outcome.skipped.is_empty() {
            summary.push_str(&format!(
                ", {} nicht lesbar",
                crate::format::count(outcome.skipped.len(), "Datei", "Dateien")
            ));
        }
        if results.cancelled && outcome.stopped() {
            summary.push_str(&format!(
                ", abgebrochen – {} nicht durchsucht",
                crate::format::count(outcome.not_reached, "Projekt", "Projekte")
            ));
        }
        if results.more {
            summary.push_str(&format!(
                ", nur die ersten {} Projekte",
                proava_application::lv_search::MAX_FILES
            ));
        }
        summary.push(')');
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::Label::new(RichText::new(summary).size(12.5).color(palette.text_dim)).wrap(),
            );
            if !outcome.skipped.is_empty() {
                response.on_hover_text(
                    outcome
                        .skipped
                        .iter()
                        .take(12)
                        .map(|(_, reason)| reason.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("transfer_folder_hits")
            .max_height(170.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for source in &outcome.sources {
                    ui.label(
                        RichText::new(format!(
                            "{} ({})",
                            source.name,
                            crate::format::count(source.count(), "Treffer", "Treffer")
                        ))
                        .size(13.0)
                        .strong()
                        .color(palette.group_text),
                    )
                    .on_hover_text(source.path.display().to_string());
                    for lv in &source.lvs {
                        if source.lvs.len() > 1 {
                            ui.label(
                                RichText::new(format!("LV {}", lv.label()))
                                    .size(12.0)
                                    .color(palette.text_dim),
                            );
                        }
                        for hit in &lv.hits {
                            let text = match hit.kind {
                                lv_search::HitKind::Note => format!("Hinweis: {}", hit.short_text),
                                _ => format!("{}  {}", hit.ordinal, hit.short_text),
                            };
                            let mut hover = hit.path_text();
                            if !hit.excerpt.is_empty() {
                                hover.push('\n');
                                hover.push_str(&hit.excerpt);
                            }
                            hover.push_str("\nKlick: Projekt als Quelle laden");
                            ui.horizontal(|ui| {
                                ui.add_space(14.0);
                                if ui
                                    .add(
                                        egui::Label::new(
                                            RichText::new(text).size(12.5).color(palette.text),
                                        )
                                        .sense(egui::Sense::click())
                                        .truncate(),
                                    )
                                    .on_hover_text(hover)
                                    .clicked()
                                {
                                    request = Some((source.path.clone(), lv.lv_index, hit.id));
                                }
                            });
                        }
                    }
                }
            });
        if ui.button("Liste schließen").clicked() {
            close = true;
        }
        if close {
            self.folder_results = None;
        }
        let (path, lv_index, row) = request?;
        let loaded = self
            .source
            .as_ref()
            .is_some_and(|source| source.path == path && !source.library);
        if loaded {
            // Already the source: switch the LV and show the row.
            if lv_index != self.lv_index {
                self.remember();
                self.lv_index = lv_index;
                self.restore();
                self.checked.clear();
            }
            self.preview = Some(row);
            return None;
        }
        self.pending = Some((path.clone(), lv_index, row));
        Some(TransferRequest::Load {
            path,
            library: false,
        })
    }
}

fn source_label(path: &Path, library: bool) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if library {
        format!("Eigene Bibliothek ({name})")
    } else {
        name
    }
}

/// Whether a ticked title above already covers row `index`.
fn covered(lv: &Lv, index: usize, checked: &HashSet<RowId>) -> bool {
    lv.rows()[..index]
        .iter()
        .enumerate()
        .rev()
        .any(|(above, row)| {
            row.is_group() && checked.contains(&row.id()) && lv.block_range(above).contains(&index)
        })
}

/// Rows of a search: the hits of the LV search (Kurz-/Langtext,
/// Bieterangaben, Hinweise …) and rows whose OZ contains the text, with the
/// titles above them; the hits with their excerpt.
fn search_rows(lv: &Lv, search: &str) -> (Vec<usize>, HashMap<RowId, String>) {
    let search = search.trim();
    if search.is_empty() {
        return (Vec::new(), HashMap::new());
    }
    let mut hits: HashMap<RowId, String> = HashMap::new();
    let mut indices = Vec::new();
    for hit in lv_search::search(lv, &LvQuery::text(search)) {
        indices.push(hit.index);
        hits.insert(hit.id, hit.excerpt);
    }
    // An OZ (digits and dots) finds the rows with that number.
    if search
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
    {
        for (index, row) in lv.rows().iter().enumerate() {
            if row.ordinal().contains(search) && !hits.contains_key(&row.id()) {
                indices.push(index);
                hits.insert(row.id(), String::new());
            }
        }
    }
    (lv_search::rows_with_titles(lv, indices), hits)
}

/// Rows to show with their indent: closed titles hide their block; a
/// search shows the matching rows with the titles above them.
fn shown_rows(lv: &Lv, collapsed: &HashSet<RowId>, search: &str) -> Vec<(usize, usize)> {
    let depth_of = proava_domain::library::depths(lv);
    if !search.trim().is_empty() {
        return search_rows(lv, search)
            .0
            .into_iter()
            .map(|index| (index, depth_of[index]))
            .collect();
    }
    let mut shown = Vec::new();
    let mut index = 0;
    while index < lv.rows().len() {
        shown.push((index, depth_of[index]));
        if lv.rows()[index].is_group() && collapsed.contains(&lv.rows()[index].id()) {
            index = lv.block_range(index).end;
        } else {
            index += 1;
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{Decimal, LvGroup, OzMask, Position, ProjectName};

    fn project() -> Project {
        let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Automation"));
        lv.push_group(LvGroup::new(1, "", "ASP01"));
        lv.push_position(Position::new("", "DDC-Station", Decimal::ONE, "St", None));
        lv.push_group(LvGroup::new(0, "", "Feld"));
        lv.push_position(Position::new("", "Fühler", Decimal::ONE, "St", None));
        lv.renumber();
        Project::new(ProjectName::parse("Q").unwrap(), "2026-11-17", lv)
    }

    #[test]
    fn search_shows_hits_with_their_titles_and_collapsed_titles_hide_blocks() {
        let project = project();
        let lv = project.lv();
        let hits: Vec<usize> = shown_rows(lv, &HashSet::new(), "ddc")
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        assert_eq!(hits, [0, 1, 2]);
        let closed: HashSet<RowId> = [lv.rows()[0].id()].into();
        let rows: Vec<usize> = shown_rows(lv, &closed, "")
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        assert_eq!(rows, [0, 3, 4]);
        assert_eq!(shown_rows(lv, &HashSet::new(), "")[2], (2, 2));
    }

    #[test]
    fn ticked_rows_become_a_transfer() {
        let mut view = TransferView::new();
        assert!(view.transfer().is_none());
        view.loaded(Path::new("/q.proava"), false, Ok((project(), Vec::new())));
        let lv = view.lv().unwrap().clone();
        assert!(
            view.collapsed.contains(&lv.rows()[1].id()),
            "subtitles start closed"
        );
        view.checked.insert(lv.rows()[0].id());
        assert!(covered(&lv, 2, &view.checked));
        assert!(!covered(&lv, 4, &view.checked));
        let transfer = view.transfer().unwrap();
        assert_eq!(transfer.rows.len(), 3);
        view.loaded(Path::new("/x"), false, Err("kaputt".into()));
        assert_eq!(view.error.as_deref(), Some("kaputt"));
        assert!(view.source.is_some(), "the last good source stays");
    }

    #[test]
    fn search_finds_long_texts_and_order_numbers() {
        let mut project = project();
        let index = project
            .lv()
            .rows()
            .iter()
            .position(|row| row.short_text() == "Fühler")
            .unwrap();
        if let Some(LvRow::Position(position)) = project.lv_mut().row_mut(index) {
            position.set_long_text(proava_domain::RichText::plain(
                "Außentemperaturfühler mit Strahlungsschutz",
            ));
        }
        let lv = project.lv();
        let (rows, hits) = search_rows(lv, "strahlungsschutz AUSSEN");
        assert_eq!(rows, [3, 4], "title above and the hit");
        assert!(hits[&lv.rows()[4].id()].contains("Strahlungsschutz"));
        let (rows, hits) = search_rows(lv, "01.01");
        assert_eq!(rows, [0, 1, 2], "OZ");
        assert_eq!(hits.len(), 2);
        assert!(search_rows(lv, "  ").0.is_empty());
    }

    #[test]
    fn a_hit_from_the_folder_search_is_shown_once_its_project_is_loaded() {
        let mut view = TransferView::new();
        let source = project();
        let target = source.lv().rows()[4].id();
        view.pending = Some((PathBuf::from("/q.proava"), 0, target));
        view.loaded(
            Path::new("/andere.proava"),
            false,
            Ok((project(), Vec::new())),
        );
        assert_eq!(view.preview, None, "another file");
        view.pending = Some((PathBuf::from("/q.proava"), 0, target));
        view.loaded(Path::new("/q.proava"), false, Ok((source, Vec::new())));
        assert_eq!(view.preview, Some(target));
        assert!(view.pending.is_none());
    }

    #[test]
    fn closed_titles_are_remembered_per_source() {
        let mut view = TransferView::new();
        view.loaded(Path::new("/a.proava"), false, Ok((project(), Vec::new())));
        let lv = view.lv().unwrap().clone();
        view.collapsed = [lv.rows()[0].id()].into();
        view.remember();
        view.loaded(Path::new("/b.proava"), false, Ok((project(), Vec::new())));
        assert!(
            !view.collapsed.contains(&lv.rows()[0].id()),
            "other source: its own state"
        );
        view.loaded(Path::new("/a.proava"), false, Ok((project(), Vec::new())));
        assert_eq!(view.collapsed, [lv.rows()[0].id()].into(), "back: as left");
    }
}
