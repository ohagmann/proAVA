//! „In Bibliothek übernehmen …“ (Rechtsklick auf Zeilen, Menü LV ▸
//! Bibliothek, Seitenfenster, GAEB-Import): in welche Bibliothek, unter
//! welchen Titel, Stichwörter – oder eine neue Bibliothek mit Name und Ort,
//! wenn es noch keine gibt. Das Projekt ändert sich nicht; geschrieben wird
//! die Bibliotheksdatei, die Meldung danach bietet „Rückgängig“ für sie an.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use proava_application::gaeb_import::{self, GaebImport, ImportTarget};
use proava_application::library::{self as library_use, LibrarySource};
use proava_application::{FileDetails, ImportProtocol, LibrarySetting};
use proava_domain::library::EntryInfo;
use proava_domain::transfer::{self, TransferOptions};
use proava_domain::{LvGroup, LvRow, Project, RowId};

use super::WorkspaceScreen;
use crate::file_dialog::{DialogResult, FileDialog};
use crate::{form_kit, strings, widgets};

/// One library in the list of the dialog.
struct Choice {
    source: LibrarySource,
    /// Why nothing can go there (read-only, missing, open elsewhere).
    blocked: Option<&'static str>,
}

/// The dialog „In Bibliothek übernehmen“.
pub(super) struct LibraryForm {
    choices: Vec<Choice>,
    /// Index into `choices`; None = „Neue Bibliothek anlegen“.
    chosen: Option<usize>,
    /// The chosen library as read (None for a new one or when unreadable)
    /// and for which choice it was read.
    library: Option<Project>,
    loaded_for: Option<Option<usize>>,
    load_error: Option<String>,
    new_name: String,
    new_folder: String,
    rows: Vec<RowId>,
    /// Title in the library; None = a new title.
    target: Option<RowId>,
    new_title: String,
    /// Title the rows have in the LV (or the LV of a GAEB file): the
    /// library title of that name is chosen, else it is the new title.
    suggestion: String,
    keywords: String,
    error: Option<String>,
    /// „GAEB ▸ Importieren … ▸ In die Bibliothek“: all rows of this file
    /// instead of the selection of the open LV.
    import: Option<(Box<GaebImport>, bool)>,
    folder_dialog: Option<FileDialog>,
}

/// „Rückgängig“ in the message after storing: the library as it was.
pub(super) struct LibraryUndo {
    path: PathBuf,
    name: String,
    before: Project,
    /// The file as written – a change since (another window) blocks the undo.
    written: Option<FileDetails>,
}

/// What a library is called in the dialog and the messages.
fn library_label(source: &LibrarySource) -> String {
    let name = source.name();
    if source.label == name {
        format!("„{name}“")
    } else {
        format!("{} „{name}“", source.label)
    }
}

impl WorkspaceScreen {
    /// The selected rows of the LV into a library (dialog).
    pub(super) fn open_library_form(&mut self, ctx: &egui::Context) {
        let rows = self.editor.selected_rows(&self.project);
        if rows.is_empty() {
            self.say(
                ctx,
                "Bitte erst Zeilen wählen, die in die Bibliothek sollen.",
            );
            return;
        }
        let lv = self.project.lv();
        // The top title above the first row (a stored title goes under it).
        let suggestion = rows
            .first()
            .and_then(|id| lv.index_of(*id))
            .and_then(|index| crate::lv_ops::ancestors(lv, index).first().copied())
            .and_then(|id| lv.index_of(id))
            .map(|index| lv.rows()[index].short_text().trim().to_string())
            .unwrap_or_default();
        self.library_form = Some(self.new_library_form(rows, suggestion, None));
    }

    /// „In Bibliothek übernehmen“ for a whole GAEB file (Zulieferung).
    pub(super) fn open_library_for_import(
        &mut self,
        _ctx: &egui::Context,
        import: GaebImport,
        prices: bool,
    ) {
        // A delivery usually gets its own title, named like the file's LV.
        let suggestion = gaeb_import::ImportPreview::of(&import).suggested_title();
        let rows = gaeb_import::all_rows(&import);
        self.library_form =
            Some(self.new_library_form(rows, suggestion, Some((Box::new(import), prices))));
    }

    fn new_library_form(
        &self,
        rows: Vec<RowId>,
        suggestion: String,
        import: Option<(Box<GaebImport>, bool)>,
    ) -> LibraryForm {
        let choices = self.library_choices();
        // The first library that takes entries (the own one first); none:
        // „Neue Bibliothek anlegen“ with name and folder of the settings.
        let chosen = choices.iter().position(|choice| choice.blocked.is_none());
        let own = self.launcher.library_file();
        LibraryForm {
            choices,
            chosen,
            library: None,
            loaded_for: None,
            load_error: None,
            new_name: library_use::library_name(&own),
            new_folder: own
                .parent()
                .map(|folder| folder.to_string_lossy().to_string())
                .unwrap_or_default(),
            rows,
            target: None,
            new_title: suggestion.clone(),
            suggestion,
            keywords: String::new(),
            error: None,
            import,
            folder_dialog: None,
        }
    }

    /// The libraries of the settings; the own one only when its file exists
    /// (else „Neue Bibliothek anlegen“ creates it).
    fn library_choices(&self) -> Vec<Choice> {
        self.launcher
            .libraries()
            .into_iter()
            .filter_map(|source| {
                let exists = self.launcher.is_file(&source.path);
                if source.own && !exists {
                    return None;
                }
                let blocked = if source.path == self.project_file {
                    Some("hier geöffnet")
                } else if source.read_only {
                    Some("schreibgeschützt")
                } else if !exists {
                    Some("nicht gefunden")
                } else if self
                    .launcher
                    .foreign_lock(&source.path)
                    .is_some_and(|lock| !lock.stale)
                {
                    // Its next save there would drop what goes in here.
                    Some("in einem anderen Fenster offen")
                } else {
                    None
                };
                Some(Choice { source, blocked })
            })
            .collect()
    }

    /// Reads the chosen library once per choice and picks the title.
    fn load_chosen(&self, form: &mut LibraryForm) {
        if form.loaded_for == Some(form.chosen) {
            return;
        }
        form.loaded_for = Some(form.chosen);
        form.load_error = None;
        form.error = None;
        form.library = match form.chosen.and_then(|index| form.choices.get(index)) {
            None => None,
            Some(choice) => match self.launcher.read_source(&choice.source.path) {
                Ok((library, _)) => Some(library),
                Err(error) => {
                    form.load_error = Some(format!("Bibliothek nicht lesbar: {error}"));
                    None
                }
            },
        };
        let wanted = form.suggestion.trim().to_lowercase();
        let groups: Vec<(RowId, String)> = form
            .library
            .iter()
            .flat_map(|library| library.lv().rows())
            .filter(|row| row.is_group())
            .map(|row| (row.id(), row.short_text().trim().to_lowercase()))
            .collect();
        form.target = groups
            .iter()
            .find(|(_, text)| !wanted.is_empty() && *text == wanted)
            .map(|(id, _)| *id)
            .or_else(|| {
                // Without a suggestion the first title, as before.
                wanted
                    .is_empty()
                    .then(|| groups.first().map(|(id, _)| *id))
                    .flatten()
            });
    }

    /// Draws the dialog (if open) and stores on „Übernehmen“.
    pub(super) fn library_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut form) = self.library_form.take() else {
            return;
        };
        let palette = self.palette;
        if let Some(dialog) = form.folder_dialog.as_mut() {
            match dialog.show(ui, &palette, &self.launcher) {
                DialogResult::Open => {}
                DialogResult::Cancelled => form.folder_dialog = None,
                DialogResult::Chosen(paths) => {
                    if let Some(folder) = paths.first() {
                        form.new_folder = folder.to_string_lossy().to_string();
                    }
                    form.folder_dialog = None;
                }
            }
            self.library_form = Some(form);
            return;
        }
        self.load_chosen(&mut form);
        let titles: Vec<(RowId, String, usize)> = form
            .library
            .iter()
            .flat_map(|library| library.lv().rows())
            .filter_map(|row| {
                row.as_group().map(|group| {
                    (
                        row.id(),
                        format!("{} {}", group.ordinal(), group.short_text()),
                        group.level(),
                    )
                })
            })
            .collect();
        let what = match &form.import {
            Some((import, _)) => format!(
                "Alle {} aus {}",
                crate::format::count(form.rows.len(), "Zeile", "Zeilen"),
                crate::file_dialog::file_name(&import.file)
            ),
            None => format!(
                "{} mit Inhalt",
                crate::format::count(form.rows.len(), "gewählte Zeile", "gewählte Zeilen")
            ),
        };
        let mut pick_folder = false;
        let ((cancelled, confirmed), close) = widgets::modal(ui, &palette, 600.0, |ui| {
            widgets::title(
                ui,
                &palette,
                strings::LIBRARY_STORE_TITLE,
                &format!("{what} – das Projekt bleibt unverändert."),
            );
            ui.label(
                RichText::new("In welche Bibliothek?")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            for (index, choice) in form.choices.iter().enumerate() {
                let mut text = library_label(&choice.source);
                if let Some(why) = choice.blocked {
                    text.push_str(&format!(" ({why})"));
                }
                let response = ui
                    .add_enabled(
                        choice.blocked.is_none(),
                        egui::RadioButton::new(form.chosen == Some(index), text),
                    )
                    .on_hover_text(choice.source.path.display().to_string())
                    .on_disabled_hover_text(choice.source.path.display().to_string());
                if response.clicked() {
                    form.chosen = Some(index);
                }
            }
            if ui
                .radio(form.chosen.is_none(), strings::LIBRARY_NEW)
                .clicked()
            {
                form.chosen = None;
            }
            if form.chosen.is_none() {
                ui.indent("new_library", |ui| {
                    ui.label(
                        RichText::new("Name (Datei <Name>.proava)")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add_space(3.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut form.new_name)
                            .hint_text("z. B. Bibliothek Gebäudeautomation")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    pick_folder = form_kit::path_field(
                        ui,
                        &palette,
                        "Ort (Ordner)",
                        &mut form.new_folder,
                        "z. B. /home/…/Dokumente",
                        strings::CHOOSE_FOLDER,
                    );
                    ui.label(
                        RichText::new(
                            "Gibt es noch keine eigene Bibliothek, wird diese die eigene; sonst kommt sie zu den weiteren Bibliotheken (Einstellungen).",
                        )
                        .size(11.5)
                        .color(palette.text_dim),
                    );
                });
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("Unter welchen Titel?")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for (id, text, level) in &titles {
                        ui.horizontal(|ui| {
                            ui.add_space(*level as f32 * 14.0);
                            ui.radio_value(&mut form.target, Some(*id), text.as_str());
                        });
                    }
                    ui.radio_value(&mut form.target, None, "Neuer Titel:");
                });
            ui.add_enabled(
                form.target.is_none(),
                egui::TextEdit::singleline(&mut form.new_title).hint_text("z. B. Raumautomation"),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Stichwörter (optional, mit Komma getrennt)")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            ui.add(
                egui::TextEdit::singleline(&mut form.keywords)
                    .hint_text("z. B. KNX, Raumbedienung")
                    .desired_width(f32::INFINITY),
            );
            widgets::error_line(
                ui,
                &palette,
                form.error.as_deref().or(form.load_error.as_deref()),
            );
            widgets::dialog_buttons(ui, &palette, "Übernehmen")
        });
        if pick_folder {
            let start = PathBuf::from(form.new_folder.trim());
            let start = if self.launcher.is_folder(&start) {
                start
            } else {
                self.launcher.home_folder()
            };
            form.folder_dialog = Some(FileDialog::folder(
                &self.launcher,
                "Ordner der neuen Bibliothek",
                start,
            ));
            self.library_form = Some(form);
            return;
        }
        if close || cancelled {
            return;
        }
        if confirmed {
            match self.store_in_library(&mut form) {
                Ok((message, protocol, undo)) => {
                    let ctx = ui.ctx().clone();
                    self.library_undo = Some(undo);
                    self.say_with_undo(&ctx, message);
                    self.reload_library();
                    if let Some(protocol) = protocol {
                        self.dialog = Some(super::Dialog::Import(Box::new(
                            crate::import_dialog::ImportDialog::showing(protocol),
                        )));
                    }
                    return;
                }
                Err(error) => form.error = Some(error),
            }
        }
        self.library_form = Some(form);
    }

    /// The file of a new library from name and folder of the dialog.
    fn new_library_path(&self, form: &LibraryForm) -> Result<PathBuf, String> {
        let name = form.new_name.trim();
        let name = name.strip_suffix(".proava").unwrap_or(name).trim();
        if name.is_empty() {
            return Err("Bitte einen Namen für die neue Bibliothek eintragen.".to_string());
        }
        if name.contains(['/', '\\']) {
            return Err(
                "Der Name darf keinen Schrägstrich enthalten – den Ort bitte unter „Ort“ wählen."
                    .to_string(),
            );
        }
        let folder = PathBuf::from(form.new_folder.trim());
        if form.new_folder.trim().is_empty() || !self.launcher.is_folder(&folder) {
            return Err(format!(
                "Den Ordner „{}“ gibt es nicht.",
                form.new_folder.trim()
            ));
        }
        Ok(folder.join(format!("{name}.proava")))
    }

    /// Writes the rows into the library; returns the message, the import
    /// protocol (GAEB) and what „Rückgängig“ needs.
    fn store_in_library(
        &mut self,
        form: &mut LibraryForm,
    ) -> Result<(String, Option<ImportProtocol>, LibraryUndo), String> {
        let now = self.launcher.now_iso();
        let (path, mut library, created) = match form.chosen {
            Some(index) => {
                let choice = form
                    .choices
                    .get(index)
                    .ok_or("Bitte eine Bibliothek wählen.")?;
                let library = form.library.clone().ok_or_else(|| {
                    form.load_error
                        .clone()
                        .unwrap_or_else(|| "Bibliothek nicht lesbar.".to_string())
                })?;
                (choice.source.path.clone(), library, false)
            }
            None => {
                let path = self.new_library_path(form)?;
                // A library file already there (not in the settings yet) is used.
                let library = if self.launcher.is_file(&path) {
                    self.launcher
                        .read_source(&path)
                        .map(|(library, _)| library)
                        .map_err(|error| format!("Bibliothek nicht lesbar: {error}"))?
                } else {
                    library_use::new_library(&now)
                };
                (path, library, true)
            }
        };
        let before = library.clone();
        let known: HashSet<RowId> = library.lv().rows().iter().map(LvRow::id).collect();
        let target = match form.target {
            Some(id) => id,
            None => {
                let text = form.new_title.trim();
                if text.is_empty() {
                    return Err("Bitte einen Titel wählen oder einen neuen benennen.".to_string());
                }
                let lv = library.lv_mut();
                let end = lv.rows().len();
                lv.insert_numbered(end, LvRow::Group(LvGroup::new(0, "", text)))?
            }
        };
        let index = library
            .lv()
            .index_of(target)
            .ok_or("Den Titel gibt es in der Bibliothek nicht mehr.")?;
        let title = library.lv().rows()[index].short_text().trim().to_string();
        let today = crate::format::iso_to_german_date(&now);
        let (count, protocol) = match &form.import {
            Some((import, prices)) => {
                let count =
                    gaeb_import::into_library(&mut library, import, index, *prices, &today)?;
                let target = ImportTarget::Library {
                    file: path.clone(),
                    title: title.clone(),
                    rows: count,
                };
                (count, Some(gaeb_import::protocol(import, &target, *prices)))
            }
            None => {
                let mut taken = transfer::collect(
                    &self.project,
                    self.project.lv(),
                    &form.rows,
                    TransferOptions {
                        quantities: true,
                        prices: true,
                    },
                );
                proava_domain::library::prepare_for_library(&mut taken.rows, &today);
                (
                    transfer::insert_under(&mut library, index, &taken)?.len(),
                    None,
                )
            }
        };
        // Stichwörter for every new position (added to its own).
        let keywords = EntryInfo::parse_keywords(&form.keywords);
        if !keywords.is_empty() {
            let lv = library.lv_mut();
            for row in 0..lv.rows().len() {
                if known.contains(&lv.rows()[row].id()) {
                    continue;
                }
                if let Some(position) = lv.row_mut(row).and_then(LvRow::as_position_mut) {
                    let mut info = position.library_info().clone();
                    let words = info.keywords_text();
                    info.keywords =
                        EntryInfo::parse_keywords(&format!("{words},{}", keywords.join(",")));
                    position.set_library_info(info);
                }
            }
        }
        // Only now the own library is known to be missing (before the write).
        let own_missing = !self.launcher.is_file(&self.launcher.library_file());
        self.launcher.save_project(&path, &library)?;
        let name = library_use::library_name(&path);
        let mut message = format!(
            "In „{name}“ abgelegt: {} unter „{title}“.",
            crate::format::count(count, "Zeile", "Zeilen"),
        );
        if created {
            if let Err(problems) = self.register_library(&path, own_missing) {
                message.push_str(&format!(
                    " In den Einstellungen nicht eingetragen: {}",
                    problems.join(" ")
                ));
            }
        }
        let undo = LibraryUndo {
            written: self.launcher.file_details(&path),
            path,
            name,
            before,
        };
        Ok((message, protocol, undo))
    }

    /// A new library goes into the settings: the own one when there is
    /// none yet, otherwise one of the further libraries (writable).
    fn register_library(&mut self, path: &Path, own_missing: bool) -> Result<(), Vec<String>> {
        let mut preferences = self.launcher.preferences();
        if own_missing {
            if self.launcher.library_file() == path {
                return Ok(());
            }
            preferences.library_file = Some(path.to_path_buf());
        } else if self.launcher.library_at(path).is_none() {
            preferences.libraries.push(LibrarySetting {
                path: path.to_path_buf(),
                label: String::new(),
                read_only: false,
            });
        } else {
            return Ok(());
        }
        self.launcher.set_preferences(preferences)?;
        self.preferences = self.launcher.preferences();
        Ok(())
    }

    /// „Rückgängig“ in the message: the library file as it was before.
    pub(super) fn undo_library_store(&mut self, ctx: &egui::Context) {
        let Some(undo) = self.library_undo.take() else {
            return;
        };
        if self.launcher.file_details(&undo.path) != undo.written {
            self.say(
                ctx,
                format!(
                    "„{}“ wurde inzwischen geändert – das Ablegen lässt sich nicht mehr zurücknehmen.",
                    undo.name
                ),
            );
            return;
        }
        match self.launcher.save_project(&undo.path, &undo.before) {
            Ok(()) => {
                self.reload_library();
                self.say(ctx, format!("Ablegen in „{}“ zurückgenommen.", undo.name));
            }
            Err(error) => self.say(ctx, format!("Rückgängig ging nicht: {error}")),
        }
    }
}
