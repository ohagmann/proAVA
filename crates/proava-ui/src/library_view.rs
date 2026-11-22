//! Side window „Bibliothek“ in the LV: the libraries at hand (own one and
//! further ones from the settings), a tree of their titles, full-text search
//! over Kurz- and Langtext with filters (Kostengruppe, Positionsart,
//! Stichwort), a formatted preview with images, and taking entries into the
//! LV – double click, Enter or dragging into the table.
//!
//! The panel only reads the libraries; the workspace inserts (one undo
//! step) and stores ([`LibraryRequest`]).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use eframe::egui::{self, RichText};
use proava_application::{Launcher, LibrarySource};
use proava_domain::library::{self, KindFilter, SearchFilter, SearchIndex};
use proava_domain::transfer::{self, Transfer, TransferOptions};
use proava_domain::{ImageAsset, Lv, LvRow, Project, RowId};

use crate::complement_display::{self, DisplayBlock};
use crate::theme::Palette;
use crate::transfer_view::TransferDrag;
use crate::{form_kit, numbers};

const ROW_HEIGHT: f32 = 22.0;
pub const TITLE: &str = "Bibliothek";

/// One library in the panel.
struct Shelf {
    source: LibrarySource,
    project: Option<Project>,
    index: SearchIndex,
    keywords: Vec<String>,
    error: Option<String>,
}

/// An entry: library (index into the shelves) and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pick {
    pub shelf: usize,
    pub row: RowId,
}

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Line {
    /// Name of a library (when all are shown).
    Header(usize),
    Row {
        shelf: usize,
        index: usize,
        depth: usize,
    },
}

/// What the workspace should do.
pub enum LibraryRequest {
    /// Insert at the selection of the LV (origin already marked).
    Insert(Transfer),
    /// „In Bibliothek übernehmen“ for the rows selected in the LV.
    Store,
    /// A GAEB file into the own library.
    ImportGaeb,
    /// Open a library as project to edit it.
    Edit(PathBuf),
    /// Write these entries as GAEB X81.
    Export { taken: Transfer, label: String },
    /// Hide the panel.
    Close,
    /// Read the libraries again.
    Reload,
    /// Positions of the LV against their library entries.
    Sync,
}

pub struct LibraryPanel {
    shelves: Vec<Shelf>,
    /// None = all libraries together.
    choice: Option<usize>,
    pub filter: SearchFilter,
    focus_search: bool,
    selected: Option<Pick>,
    /// Further chosen entries (Strg+Klick), same library as `selected`.
    marked: HashSet<Pick>,
    collapsed: HashMap<usize, HashSet<RowId>>,
    lines: Vec<Line>,
    lines_dirty: bool,
    /// Take the quantities of the entries (usually 1 – off by default).
    pub quantities: bool,
    /// Take the Erfahrungspreise as Planerpreise.
    pub prices: bool,
    textures: HashMap<String, Option<egui::TextureHandle>>,
    /// Keys go to the list (a row was clicked, the search is focused).
    active: bool,
    scroll_to_selected: bool,
}

impl LibraryPanel {
    pub fn new(launcher: &Launcher) -> Self {
        let mut panel = Self {
            shelves: Vec::new(),
            choice: None,
            filter: SearchFilter::default(),
            focus_search: false,
            selected: None,
            marked: HashSet::new(),
            collapsed: HashMap::new(),
            lines: Vec::new(),
            lines_dirty: true,
            quantities: false,
            prices: true,
            textures: HashMap::new(),
            active: false,
            scroll_to_selected: false,
        };
        panel.reload(launcher);
        panel
    }

    /// Reads all libraries again (after storing, editing, settings).
    pub fn reload(&mut self, launcher: &Launcher) {
        let selected = self.selected.and_then(|pick| {
            self.shelves
                .get(pick.shelf)
                .map(|shelf| (shelf.source.path.clone(), pick.row))
        });
        let sources = launcher.libraries();
        let mut shelves = Vec::new();
        for source in sources {
            let (project, error) = match launcher.read_library(&source) {
                Ok(project) => (Some(project), None),
                Err(error) => (None, Some(error)),
            };
            let index = project
                .as_ref()
                .map(|project| SearchIndex::build(project.lv()))
                .unwrap_or_default();
            let keywords = project
                .as_ref()
                .map(|project| library::keywords(project.lv()))
                .unwrap_or_default();
            // Titles below the first level start closed.
            if let Some(project) = &project {
                let shelf = shelves.len();
                let known = self
                    .shelves
                    .get(shelf)
                    .is_some_and(|old: &Shelf| old.source.path == source.path);
                if !known {
                    self.collapsed.insert(
                        shelf,
                        project
                            .lv()
                            .rows()
                            .iter()
                            .filter(|row| row.as_group().is_some_and(|group| group.level() > 0))
                            .map(LvRow::id)
                            .collect(),
                    );
                }
            }
            shelves.push(Shelf {
                source,
                project,
                index,
                keywords,
                error,
            });
        }
        self.shelves = shelves;
        if self
            .choice
            .is_some_and(|choice| choice >= self.shelves.len())
        {
            self.choice = None;
        }
        self.selected = selected.and_then(|(path, row)| {
            let shelf = self
                .shelves
                .iter()
                .position(|shelf| shelf.source.path == path)?;
            self.shelves[shelf].project.as_ref()?.lv().index_of(row)?;
            Some(Pick { shelf, row })
        });
        self.marked.clear();
        self.textures.clear();
        self.lines_dirty = true;
    }

    /// Puts the keyboard into the search field (F8 when already open).
    pub fn focus_search(&mut self) {
        self.focus_search = true;
    }

    /// The library shown when exactly one is chosen, else the own one.
    fn edit_target(&self) -> Option<&LibrarySource> {
        match self.choice {
            Some(choice) => self.shelves.get(choice).map(|shelf| &shelf.source),
            None => self.shelves.first().map(|shelf| &shelf.source),
        }
    }

    fn lv(&self, shelf: usize) -> Option<&Lv> {
        self.shelves.get(shelf)?.project.as_ref().map(Project::lv)
    }

    /// Lines of the list for the current choice, filter and folding.
    fn build_lines(&mut self) {
        let shelves: Vec<usize> = match self.choice {
            Some(choice) => vec![choice],
            None => (0..self.shelves.len()).collect(),
        };
        let mut lines = Vec::new();
        for shelf in shelves {
            let Some(lv) = self.lv(shelf) else {
                if self.choice.is_none() {
                    lines.push(Line::Header(shelf));
                }
                continue;
            };
            let depths = library::depths(lv);
            let rows: Vec<usize> = if self.filter.is_empty() {
                let closed = self.collapsed.get(&shelf);
                let mut shown = Vec::new();
                let mut index = 0;
                while index < lv.rows().len() {
                    shown.push(index);
                    let row = &lv.rows()[index];
                    if row.is_group() && closed.is_some_and(|closed| closed.contains(&row.id())) {
                        index = lv.block_range(index).end;
                    } else {
                        index += 1;
                    }
                }
                shown
            } else {
                self.shelves[shelf].index.hits(lv, &self.filter)
            };
            if self.choice.is_none() && (self.filter.is_empty() || !rows.is_empty()) {
                lines.push(Line::Header(shelf));
            }
            lines.extend(rows.into_iter().map(|index| Line::Row {
                shelf,
                index,
                depth: depths[index],
            }));
        }
        self.lines = lines;
        self.lines_dirty = false;
    }

    /// Positions found by the search (all chosen libraries).
    fn hit_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| match line {
                Line::Row { shelf, index, .. } => self.lv(*shelf).is_some_and(|lv| {
                    lv.rows()[*index].is_position()
                        && self.shelves[*shelf].index.matches(lv, *index, &self.filter)
                }),
                Line::Header(_) => false,
            })
            .count()
    }

    /// The chosen entries as a transfer for the LV (origin marked).
    pub fn transfer(&self) -> Option<Transfer> {
        let selected = self.selected?;
        let shelf = self.shelves.get(selected.shelf)?;
        let project = shelf.project.as_ref()?;
        let mut ids: Vec<RowId> = self
            .marked
            .iter()
            .filter(|pick| pick.shelf == selected.shelf)
            .map(|pick| pick.row)
            .collect();
        ids.push(selected.row);
        let ids: Vec<RowId> = project
            .lv()
            .rows()
            .iter()
            .map(LvRow::id)
            .filter(|id| ids.contains(id))
            .collect();
        let mut taken = transfer::collect(
            project,
            project.lv(),
            &ids,
            TransferOptions {
                quantities: self.quantities,
                prices: self.prices,
            },
        );
        library::mark_origin(&mut taken.rows, &shelf.source.name());
        (!taken.is_empty()).then_some(taken)
    }

    /// Draws the panel (inside a side panel of the workspace).
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        can_store: bool,
    ) -> Option<LibraryRequest> {
        let mut request = None;
        if self.lines_dirty {
            self.build_lines();
        }
        // Header
        ui.horizontal(|ui| {
            ui.label(RichText::new(TITLE).size(15.0).strong().color(palette.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("Schließen")
                    .on_hover_text("Bibliothek ausblenden (F8)")
                    .clicked()
                {
                    request = Some(LibraryRequest::Close);
                }
            });
        });
        ui.add_space(4.0);
        let labels: Vec<String> = self
            .shelves
            .iter()
            .map(|shelf| {
                let mut label = shelf.source.label.clone();
                if shelf.source.read_only {
                    label.push_str(" (schreibgeschützt)");
                }
                label
            })
            .collect();
        ui.horizontal(|ui| {
            let current = match self.choice {
                Some(choice) => labels.get(choice).cloned().unwrap_or_default(),
                None if self.shelves.len() > 1 => "Alle Bibliotheken".to_string(),
                None => labels.first().cloned().unwrap_or_default(),
            };
            let mut choice = self.choice;
            egui::ComboBox::from_id_salt("library_choice")
                .width((ui.available_width() - 150.0).max(120.0))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if self.shelves.len() > 1 {
                        ui.selectable_value(&mut choice, None, "Alle Bibliotheken");
                    }
                    for (index, label) in labels.iter().enumerate() {
                        ui.selectable_value(&mut choice, Some(index), label)
                            .on_hover_text(self.shelves[index].source.path.display().to_string());
                    }
                });
            if choice != self.choice {
                self.choice = choice;
                self.lines_dirty = true;
            }
            crate::menus::menu(ui, "Bibliothek …", |ui| {
                let target = self.edit_target().cloned();
                let writable = target.as_ref().is_some_and(|source| !source.read_only);
                if ui
                    .add_enabled(can_store, egui::Button::new(crate::strings::LIBRARY_STORE))
                    .on_hover_text("Die im LV gewählten Zeilen in eine Bibliothek übernehmen (auch per Rechtsklick auf die Zeilen)")
                    .clicked()
                {
                    request = Some(LibraryRequest::Store);
                    ui.close();
                }
                if ui.button("GAEB-Datei in die Bibliothek …").clicked() {
                    request = Some(LibraryRequest::ImportGaeb);
                    ui.close();
                }
                ui.separator();
                if let Some(source) = &target {
                    if ui
                        .add_enabled(writable, egui::Button::new(format!("„{}“ bearbeiten …", source.label)))
                        .on_hover_text("Die Bibliothek wie ein LV öffnen: Titel anlegen, umbenennen, verschieben, Stichwörter")
                        .clicked()
                    {
                        request = Some(LibraryRequest::Edit(source.path.clone()));
                        ui.close();
                    }
                }
                let whole = target.as_ref().and_then(|source| {
                    let shelf = self.shelves.iter().position(|shelf| shelf.source.path == source.path)?;
                    let project = self.shelves[shelf].project.as_ref()?;
                    let ids: Vec<RowId> = project.lv().rows().iter().map(LvRow::id).collect();
                    let taken = transfer::collect(
                        project,
                        project.lv(),
                        &ids,
                        TransferOptions {
                            quantities: true,
                            prices: false,
                        },
                    );
                    (!taken.is_empty()).then(|| (taken, source.label.clone()))
                });
                if ui
                    .add_enabled(whole.is_some(), egui::Button::new("Ganze Bibliothek als GAEB X81 …"))
                    .clicked()
                {
                    if let Some((taken, label)) = whole {
                        request = Some(LibraryRequest::Export { taken, label });
                    }
                    ui.close();
                }
                let chosen = self.transfer();
                if ui
                    .add_enabled(chosen.is_some(), egui::Button::new("Auswahl als GAEB X81 …"))
                    .clicked()
                {
                    if let Some(taken) = chosen {
                        let label = self
                            .selected
                            .and_then(|pick| {
                                let lv = self.lv(pick.shelf)?;
                                Some(lv.row_by_id(pick.row)?.short_text())
                            })
                            .unwrap_or_else(|| "Bibliothek".to_string());
                        request = Some(LibraryRequest::Export { taken, label });
                    }
                    ui.close();
                }
                ui.separator();
                if ui
                    .button("Mit Bibliothek abgleichen …")
                    .on_hover_text("Positionen des LV, die aus einer Bibliothek stammen, gegen ihren Eintrag")
                    .clicked()
                {
                    request = Some(LibraryRequest::Sync);
                    ui.close();
                }
                if ui.button("Neu laden").clicked() {
                    request = Some(LibraryRequest::Reload);
                    ui.close();
                }
            });
        });
        ui.add_space(4.0);
        // Search
        let search = ui
            .horizontal(|ui| {
                crate::widgets::search_mark(ui, palette);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter.text)
                        .hint_text("Suchen in Kurz- und Langtext, Stichwörtern, OZ")
                        .desired_width(ui.available_width()),
                )
            })
            .inner;
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        if search.changed() {
            self.lines_dirty = true;
        }
        if search.has_focus() {
            self.active = true;
        }
        ui.add_space(2.0);
        let keywords: Vec<String> = match self.choice {
            Some(choice) => self
                .shelves
                .get(choice)
                .map(|shelf| shelf.keywords.clone())
                .unwrap_or_default(),
            None => {
                let mut all: Vec<String> = self
                    .shelves
                    .iter()
                    .flat_map(|shelf| shelf.keywords.clone())
                    .collect();
                all.sort_by_key(|word| library::fold(word));
                all.dedup_by_key(|word| library::fold(word));
                all
            }
        };
        ui.horizontal(|ui| {
            let group = ui.add(
                egui::TextEdit::singleline(&mut self.filter.cost_group)
                    .hint_text("KG")
                    .desired_width(46.0),
            );
            if group
                .on_hover_text("Kostengruppe, z. B. 481 oder 48")
                .changed()
            {
                self.lines_dirty = true;
            }
            let mut kind = self.filter.kind;
            egui::ComboBox::from_id_salt("library_kind")
                .width(150.0)
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for option in KindFilter::ALL {
                        ui.selectable_value(&mut kind, option, option.label());
                    }
                });
            if kind != self.filter.kind {
                self.filter.kind = kind;
                self.lines_dirty = true;
            }
            let mut keyword = self.filter.keyword.clone();
            egui::ComboBox::from_id_salt("library_keyword")
                .width(ui.available_width().max(90.0) - 8.0)
                .selected_text(if keyword.is_empty() {
                    "alle Stichwörter".to_string()
                } else {
                    keyword.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut keyword, String::new(), "alle Stichwörter");
                    for word in &keywords {
                        ui.selectable_value(&mut keyword, word.clone(), word);
                    }
                });
            if keyword != self.filter.keyword {
                self.filter.keyword = keyword;
                self.lines_dirty = true;
            }
        });
        if self.lines_dirty {
            self.build_lines();
        }
        if !self.filter.is_empty() {
            ui.horizontal(|ui| {
                let hits = self.hit_count();
                ui.label(
                    RichText::new(if hits == 1 {
                        "1 Position gefunden".to_string()
                    } else {
                        format!("{hits} Positionen gefunden")
                    })
                    .size(12.0)
                    .color(palette.text_dim),
                );
                if ui.small_button("Filter leeren").clicked() {
                    self.filter = SearchFilter::default();
                    self.lines_dirty = true;
                }
            });
        }
        ui.separator();

        let keys = self.keys(ui, search.id);
        if let Some(insert) = keys {
            if insert {
                request = self.transfer().map(LibraryRequest::Insert).or(request);
            }
        }

        // Options and „Einfügen“ at the bottom; list and preview share the rest.
        let chosen = self.selected.is_some();
        let count = 1 + self
            .marked
            .iter()
            .filter(|pick| Some(pick.shelf) == self.selected.map(|selected| selected.shelf))
            .count();
        let label = if count > 1 {
            format!("Einfügen ({count} Einträge)")
        } else {
            "Einfügen".to_string()
        };
        let mut insert = false;
        egui::Panel::bottom("library_actions")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.quantities, "Mengen übernehmen");
                    ui.checkbox(&mut self.prices, "Erfahrungspreise als Planerpreise");
                });
                ui.add_space(2.0);
                insert = ui
                    .add_enabled(
                        chosen,
                        egui::Button::new(RichText::new(label).color(palette.on_accent))
                            .fill(palette.accent)
                            .min_size(egui::vec2(120.0, 28.0)),
                    )
                    .on_hover_text("An der Auswahl im LV einfügen (Doppelklick, Enter oder in die Tabelle ziehen)")
                    .clicked();
            });
        if insert {
            request = self.transfer().map(LibraryRequest::Insert);
        }
        let total = ui.available_height();
        let list_height = (total * 0.55).max(120.0);
        let clicked_inside = self.list(ui, palette, list_height, &mut request);
        ui.separator();
        self.preview(ui, palette, ui.available_height().max(60.0));

        // A click elsewhere gives the keys back to the LV.
        let outside = ui.ctx().input(|input| {
            input.pointer.primary_clicked()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !ui.max_rect().contains(pos))
        });
        if outside && !search.has_focus() {
            self.active = false;
        }
        if clicked_inside {
            self.active = true;
        }
        request
    }

    /// ↑/↓ move through the list, Enter inserts (only while the panel has
    /// the keys). Returns Some(true) for Enter.
    fn keys(&mut self, ui: &mut egui::Ui, search: egui::Id) -> Option<bool> {
        if !self.active {
            return None;
        }
        // Another text field (filter, LV cell) keeps its keys.
        let focused = ui.ctx().memory(|memory| memory.focused());
        if ui.ctx().text_edit_focused() && focused != Some(search) {
            return None;
        }
        let (up, down, enter) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        if up || down {
            let rows: Vec<Pick> = self
                .lines
                .iter()
                .filter_map(|line| match line {
                    Line::Row { shelf, index, .. } => self.lv(*shelf).map(|lv| Pick {
                        shelf: *shelf,
                        row: lv.rows()[*index].id(),
                    }),
                    Line::Header(_) => None,
                })
                .collect();
            let at = self
                .selected
                .and_then(|pick| rows.iter().position(|row| *row == pick));
            let next = match (at, down) {
                (None, _) => rows.first().copied(),
                (Some(at), true) => rows.get(at + 1).or(rows.get(at)).copied(),
                (Some(at), false) => rows.get(at.saturating_sub(1)).copied(),
            };
            if next.is_some() {
                self.selected = next;
                self.marked.clear();
                self.scroll_to_selected = true;
            }
        }
        enter.then_some(true)
    }

    /// The list; true when a row was clicked.
    fn list(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        height: f32,
        request: &mut Option<LibraryRequest>,
    ) -> bool {
        let mut clicked = false;
        if self.shelves.is_empty() {
            return false;
        }
        let empty_own = self.choice.unwrap_or(0) == 0
            && self.lv(0).is_some_and(|lv| lv.rows().is_empty())
            && (self.choice.is_some() || self.shelves.len() == 1);
        if empty_own {
            ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                ui.label(
                    RichText::new(
                        "Die eigene Bibliothek ist noch leer. Zeilen im LV wählen und per Rechtsklick \
                         „In Bibliothek übernehmen …“ – oder eine GAEB-Datei mit Herstellertexten einlesen.",
                    )
                    .size(12.5)
                    .color(palette.text_dim),
                );
            });
            return false;
        }
        let lines = self.lines.clone();
        let mut toggle: Option<(usize, RowId)> = None;
        let scroll_to = std::mem::take(&mut self.scroll_to_selected);
        egui::ScrollArea::vertical()
            .id_salt("library_list")
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, lines.len(), |ui, range| {
                for line in &lines[range] {
                    match *line {
                        Line::Header(shelf) => {
                            let source = &self.shelves[shelf];
                            let mut text = source.source.label.clone();
                            if let Some(error) = &source.error {
                                text.push_str(&format!(" – {error}"));
                            }
                            ui.horizontal(|ui| {
                                ui.set_height(ROW_HEIGHT);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(text).size(12.0).strong().color(palette.text_dim),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                        Line::Row { shelf, index, depth } => {
                            let Some(lv) = self.lv(shelf) else { continue };
                            let row = &lv.rows()[index];
                            let pick = Pick { shelf, row: row.id() };
                            let is_selected = self.selected == Some(pick) || self.marked.contains(&pick);
                            let response = ui
                                .horizontal(|ui| {
                                    ui.set_height(ROW_HEIGHT);
                                    ui.add_space(depth as f32 * 12.0);
                                    if row.is_group() && self.filter.is_empty() {
                                        let closed = self
                                            .collapsed
                                            .get(&shelf)
                                            .is_some_and(|closed| closed.contains(&row.id()));
                                        if ui
                                            .small_button(if closed { "+" } else { "–" })
                                            .on_hover_text(if closed { "Aufklappen" } else { "Zuklappen" })
                                            .clicked()
                                        {
                                            toggle = Some((shelf, row.id()));
                                        }
                                    } else {
                                        ui.add_space(4.0);
                                    }
                                    // One line (GAEB-Kurztexte may have several), „…“ at the edge.
                                    let text = match row {
                                        LvRow::Note(note) => format!("Hinweis: {}", note.title()),
                                        other => other.short_text(),
                                    };
                                    let text = crate::format::one_line(&text).into_owned();
                                    let mut label = RichText::new(text).size(13.0).color(if row.is_group() {
                                        palette.group_text
                                    } else {
                                        palette.text
                                    });
                                    if row.is_group() {
                                        label = label.strong();
                                    }
                                    ui.add(
                                        egui::Button::selectable(is_selected, label)
                                            .truncate()
                                            .sense(egui::Sense::click_and_drag()),
                                    )
                                })
                                .inner;
                            let response = response.on_hover_text(match row {
                                LvRow::Position(position) => format!(
                                    "{} {}\nDoppelklick oder Enter: einfügen · Ziehen: in die LV-Tabelle",
                                    position.ordinal(),
                                    position.short_text()
                                ),
                                other => other.short_text(),
                            });
                            if self.selected == Some(pick) && scroll_to {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                clicked = true;
                                let ctrl = ui.input(|input| input.modifiers.command);
                                if ctrl && self.selected.is_some_and(|selected| selected.shelf == shelf) {
                                    if !self.marked.remove(&pick) && self.selected != Some(pick) {
                                        self.marked.insert(pick);
                                    }
                                } else {
                                    self.selected = Some(pick);
                                    self.marked.clear();
                                }
                            }
                            if response.double_clicked() {
                                self.selected = Some(pick);
                                self.marked.clear();
                                *request = self.transfer().map(LibraryRequest::Insert);
                            }
                            if response.drag_started() {
                                clicked = true;
                                if self.selected != Some(pick) && !self.marked.contains(&pick) {
                                    self.selected = Some(pick);
                                    self.marked.clear();
                                }
                                egui::DragAndDrop::set_payload(
                                    ui.ctx(),
                                    TransferDrag { from_library: true },
                                );
                            }
                        }
                    }
                }
            });
        if let Some((shelf, row)) = toggle {
            let closed = self.collapsed.entry(shelf).or_default();
            if !closed.remove(&row) {
                closed.insert(row);
            }
            self.lines_dirty = true;
        }
        clicked
    }

    fn preview(&mut self, ui: &mut egui::Ui, palette: &Palette, height: f32) {
        let Some(pick) = self.selected else {
            ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                form_kit::hint(
                    ui,
                    palette,
                    "Klick auf einen Eintrag zeigt hier seinen Langtext.",
                );
            });
            return;
        };
        let textures = &mut self.textures;
        let Some(shelf) = self.shelves.get(pick.shelf) else {
            return;
        };
        let Some(project) = shelf.project.as_ref() else {
            return;
        };
        let Some(row) = project.lv().row_by_id(pick.row) else {
            return;
        };
        let images = project.images();
        let source_label = &shelf.source.label;
        let shelf_index = pick.shelf;
        egui::ScrollArea::vertical()
            .id_salt("library_preview")
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("{}  {}", row.ordinal(), row.short_text()))
                        .size(13.5)
                        .strong()
                        .color(palette.text),
                );
                match row {
                    LvRow::Position(position) => {
                        let mut facts = vec![format!(
                            "{} {}",
                            numbers::quantity_text(position.quantity()),
                            position.unit()
                        )];
                        if position.kind() != proava_domain::PositionKind::Normal {
                            facts.push(position.kind().label().to_string());
                        }
                        if position.is_lump_sum() {
                            facts.push("Pauschal".into());
                        }
                        if let Some(price) = position.unit_price() {
                            let info = position.library_info();
                            let date = if info.price_date.trim().is_empty() {
                                String::new()
                            } else {
                                format!(" (Stand {})", info.price_date.trim())
                            };
                            facts.push(format!(
                                "Erfahrungspreis {} €{date}",
                                numbers::price_text(Some(price))
                            ));
                        }
                        ui.label(
                            RichText::new(facts.join(" · "))
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                        let info = position.library_info();
                        let mut meta = vec![source_label.clone()];
                        if !info.as_of.trim().is_empty() {
                            meta.push(format!("Stand {}", info.as_of.trim()));
                        }
                        if !info.keywords.is_empty() {
                            meta.push(format!("Stichwörter: {}", info.keywords_text()));
                        }
                        ui.label(
                            RichText::new(meta.join(" · "))
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                        ui.add_space(6.0);
                        let blocks = complement_display::display(
                            position.long_text(),
                            position.complements(),
                        );
                        show_blocks(ui, palette, textures, shelf_index, &blocks, images);
                    }
                    LvRow::Note(note) => {
                        let blocks = complement_display::display(note.text(), &[]);
                        show_blocks(ui, palette, textures, shelf_index, &blocks, images);
                    }
                    LvRow::Performance(performance) => {
                        let blocks = complement_display::display(performance.long_text(), &[]);
                        show_blocks(ui, palette, textures, shelf_index, &blocks, images);
                    }
                    LvRow::Group(_) => {
                        let lv = project.lv();
                        if let Some(index) = lv.index_of(row.id()) {
                            let positions = lv.rows()[lv.block_range(index)]
                                .iter()
                                .filter(|row| row.is_position())
                                .count();
                            ui.label(
                                RichText::new(format!(
                                    "{} in diesem Titel – Einfügen bringt sie alle.",
                                    crate::format::count(positions, "Position", "Positionen")
                                ))
                                .size(12.0)
                                .color(palette.text_dim),
                            );
                        }
                    }
                    LvRow::Spacer(_) => {}
                }
            });
    }
}

/// Long text read-only: formatting via the complement display, tables as a
/// grid, images as pictures.
fn show_blocks(
    ui: &mut egui::Ui,
    palette: &Palette,
    textures: &mut HashMap<String, Option<egui::TextureHandle>>,
    shelf: usize,
    blocks: &[DisplayBlock],
    images: &[ImageAsset],
) {
    {
        for (number, block) in blocks.iter().enumerate() {
            match block {
                DisplayBlock::Image { file } => {
                    let key = format!("{shelf}:{file}");
                    let texture = textures.entry(key.clone()).or_insert_with(|| {
                        let asset = images.iter().find(|image| &image.file == file)?;
                        let image = crate::images::decode_preview(&asset.bytes).ok()?;
                        Some(
                            ui.ctx()
                                .load_texture(key, image, egui::TextureOptions::LINEAR),
                        )
                    });
                    match texture {
                        Some(texture) => {
                            let size = texture.size_vec2();
                            let width = size.x.min(ui.available_width());
                            let scaled = egui::vec2(width, size.y * width / size.x.max(1.0));
                            ui.add(egui::Image::new((texture.id(), scaled)));
                        }
                        None => {
                            ui.label(
                                RichText::new(format!("[Bild {file}]"))
                                    .size(12.0)
                                    .color(palette.text_dim),
                            );
                        }
                    }
                }
                DisplayBlock::Table(rows) => {
                    egui::Grid::new(("library_table", number))
                        .striped(true)
                        .spacing([10.0, 3.0])
                        .show(ui, |ui| {
                            for cells in rows {
                                for cell in cells {
                                    ui.label(RichText::new(cell).size(12.5).color(palette.text));
                                }
                                ui.end_row();
                            }
                        });
                }
                other => complement_display::show(ui, palette, std::slice::from_ref(other)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{Decimal, LvGroup, OzMask, Position, ProjectName};

    fn shelf(project: Project) -> Shelf {
        Shelf {
            source: LibrarySource {
                path: PathBuf::from("/b/Buero-Bibliothek.proava"),
                label: "Büro".into(),
                read_only: true,
                own: false,
            },
            index: SearchIndex::build(project.lv()),
            keywords: library::keywords(project.lv()),
            project: Some(project),
            error: None,
        }
    }

    fn library_project() -> Project {
        let mut lv = Lv::new("Bibliothek", OzMask::parse("11.22.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Raumautomation"));
        lv.push_group(LvGroup::new(1, "", "Bediengeräte"));
        lv.push_position(Position::new(
            "",
            "Raumbediengerät KNX",
            Decimal::ONE,
            "St",
            Some(Decimal::from(180)),
        ));
        lv.push_position(Position::new("", "Präsenzmelder", Decimal::ONE, "St", None));
        lv.renumber();
        Project::new(ProjectName::parse("Bibliothek").unwrap(), "2026-11-17", lv)
    }

    fn panel() -> LibraryPanel {
        LibraryPanel {
            shelves: vec![shelf(library_project())],
            choice: None,
            filter: SearchFilter::default(),
            focus_search: false,
            selected: None,
            marked: HashSet::new(),
            collapsed: HashMap::new(),
            lines: Vec::new(),
            lines_dirty: true,
            quantities: false,
            prices: true,
            textures: HashMap::new(),
            active: false,
            scroll_to_selected: false,
        }
    }

    #[test]
    fn search_lines_and_transfer_with_origin() {
        let mut panel = panel();
        panel.build_lines();
        assert_eq!(panel.lines.len(), 1 + 4, "header and the whole tree");
        panel.filter.text = "knx".into();
        panel.build_lines();
        assert_eq!(panel.hit_count(), 1);
        let Some(Line::Row { shelf, index, .. }) = panel.lines.last().copied() else {
            panic!("row expected")
        };
        let row = panel.lv(shelf).unwrap().rows()[index].id();
        panel.selected = Some(Pick { shelf, row });
        let taken = panel.transfer().unwrap();
        let position = taken.rows[0].as_position().unwrap();
        assert_eq!(position.origin().unwrap().library, "Buero-Bibliothek");
        assert_eq!(
            position.quantity(),
            Decimal::ZERO,
            "quantities off by default"
        );
        assert_eq!(
            position.unit_price(),
            Some(Decimal::from(180)),
            "Erfahrungspreis on"
        );
        panel.prices = false;
        assert_eq!(
            panel.transfer().unwrap().rows[0]
                .as_position()
                .unwrap()
                .unit_price(),
            None
        );
    }
}
