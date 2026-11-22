//! Own file dialog (no native dialog): choose a folder, open one or more files
//! with an extension filter, or save under a name.
//!
//! A complete browser in text only: places on the left (personal folders,
//! the caller's folders, recently used folders, drives), „Nach oben“, a path
//! field to type into, clickable path parts, the list with keyboard control
//! (arrows, Enter, Backspace), a name filter, „Alle Dateien“, hidden folders
//! on request and a preview for pictures.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Vec2};
use proava_application::{Launcher, UserFolder};

use crate::icons::{self, Icon};
use crate::replace_question::{ReplaceAnswer, ReplaceQuestion};
use crate::theme::Palette;
use crate::{format, images, strings, widgets};

const PLACES_TITLE: &str = "Orte";
const RECENT_TITLE: &str = "Zuletzt benutzt";
const DRIVES_TITLE: &str = "Laufwerke";
const HOME_LABEL: &str = "Persönlicher Ordner";
const DEFAULT_FOLDER_LABEL: &str = "Standardordner";
const FILTER_HINT: &str = "Name filtern";
const PATH_LABEL: &str = "Pfad";
const SHOW_HIDDEN: &str = "Versteckte Ordner zeigen";
const ALL_FILES: &str = "Alle Dateien";
const FILE_TYPE: &str = "Dateityp";
const PREVIEW_TITLE: &str = "Vorschau";
const PREVIEW_NONE: &str = "Bild in der Liste wählen.";
const FOLDER_KIND: &str = "Ordner";
const CHOOSE: &str = "Wählen";
const KEYS_HINT: &str =
    "Klick wählt, Doppelklick oder Enter öffnet; Pfeiltasten wählen, Rücktaste geht nach oben.";
/// Pictures larger than this are not decoded for the preview.
const PREVIEW_MAX_BYTES: usize = 40 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Folder,
    Open {
        multiple: bool,
        extensions: Vec<String>,
    },
    Save {
        extension: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    Open,
    Cancelled,
    Chosen(Vec<PathBuf>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaceGroup {
    Places,
    Recent,
    Drives,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    group: PlaceGroup,
    label: String,
    path: PathBuf,
}

/// One line of the list: a folder or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    folder: bool,
}

struct Preview {
    file: PathBuf,
    picture: Result<egui::TextureHandle, String>,
}

/// „Neuer Ordner“ being named (a row at the top of the list).
struct NewFolder {
    name: String,
    error: Option<String>,
    /// Put the caret into the name (all of it chosen) in the next frame.
    focus: bool,
}

/// Name of a new folder before it is changed.
const NEW_FOLDER: &str = "Neuer Ordner";
/// Key for „Neuer Ordner“ (as in the file managers).
pub const NEW_FOLDER_KEY: &str = "Strg+Umschalt+N";

pub struct FileDialog {
    title: String,
    hint: String,
    mode: Mode,
    dir: PathBuf,
    path_text: String,
    folders: Vec<PathBuf>,
    files: Vec<PathBuf>,
    selected: Vec<PathBuf>,
    name: String,
    error: Option<String>,
    /// „Datei ersetzen?“ for the chosen file (Save mode, file exists).
    replace: Option<(ReplaceQuestion, PathBuf)>,
    /// Save mode: ask before an existing file is replaced (off when only a
    /// file is chosen, e.g. the library, which may or may not exist yet).
    ask_overwrite: bool,
    confirm_text: Option<String>,
    focus_name: bool,
    /// One choice shown with a save dialog (e.g. „Bilder mitgeben“).
    option: Option<(String, bool)>,
    places: Vec<Place>,
    show_hidden: bool,
    all_files: bool,
    filter: String,
    /// Keyboard row in the visible list.
    cursor: Option<usize>,
    /// The folder chosen with one click or the arrow keys (as in a file
    /// manager: one click chooses, a double click or Enter opens it).
    marked: Option<PathBuf>,
    new_folder: Option<NewFolder>,
    scroll_to_cursor: bool,
    preview: Option<Preview>,
}

impl FileDialog {
    pub fn folder(launcher: &Launcher, title: &str, start: PathBuf) -> Self {
        Self::new(
            launcher,
            title,
            strings::PICKER_HINT,
            Mode::Folder,
            start,
            String::new(),
        )
    }

    /// `extensions` without dot, compared case-insensitively ("x84", "D84" …).
    pub fn open(
        launcher: &Launcher,
        title: &str,
        hint: &str,
        start: PathBuf,
        extensions: &[&str],
        multiple: bool,
    ) -> Self {
        let mode = Mode::Open {
            multiple,
            extensions: extensions.iter().map(|ext| ext.to_lowercase()).collect(),
        };
        Self::new(launcher, title, hint, mode, start, String::new())
    }

    pub fn save(
        launcher: &Launcher,
        title: &str,
        hint: &str,
        start: PathBuf,
        suggested_name: &str,
        extension: &str,
    ) -> Self {
        let mode = Mode::Save {
            extension: extension.to_string(),
        };
        Self::new(
            launcher,
            title,
            hint,
            mode,
            start,
            suggested_name.to_string(),
        )
    }

    /// A check box under the file name (save dialogs), e.g. an export choice.
    pub fn with_option(mut self, label: &str, value: bool) -> Self {
        self.option = Some((label.to_string(), value));
        self
    }

    /// The state of the check box from [`Self::with_option`].
    pub fn option(&self) -> Option<bool> {
        self.option.as_ref().map(|(_, value)| *value)
    }

    /// A file that may exist or not (e.g. the position library): like
    /// [`FileDialog::save`], but without the question about replacing and
    /// with „Wählen“ on the button.
    pub fn choose_file(
        launcher: &Launcher,
        title: &str,
        hint: &str,
        start: PathBuf,
        suggested_name: &str,
        extension: &str,
    ) -> Self {
        let mut dialog = Self::save(launcher, title, hint, start, suggested_name, extension);
        dialog.ask_overwrite = false;
        dialog.confirm_text = Some(CHOOSE.to_string());
        dialog
    }

    /// Adds a folder of the caller to the places („Projektordner“ …).
    pub fn with_place(mut self, label: &str, folder: &Path) -> Self {
        let place = Place {
            group: PlaceGroup::Places,
            label: label.to_string(),
            path: folder.to_path_buf(),
        };
        if !self.places.iter().any(|known| known.path == place.path) {
            let index = self
                .places
                .iter()
                .position(|known| known.group != PlaceGroup::Places)
                .unwrap_or(self.places.len());
            self.places.insert(index, place);
        }
        self
    }

    /// Shows a message under the list (e.g. why the chosen file was not taken).
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Marks the current file (e.g. the logo chosen before) when it lies in
    /// the shown folder and passes the filter.
    pub fn with_selection(mut self, file: &Path) -> Self {
        if let Some(index) = self
            .entries()
            .iter()
            .position(|entry| !entry.folder && entry.path == file)
        {
            self.pick_file(file, false);
            self.cursor = Some(index);
            self.scroll_to_cursor = true;
        }
        self
    }

    fn new(
        launcher: &Launcher,
        title: &str,
        hint: &str,
        mode: Mode,
        start: PathBuf,
        name: String,
    ) -> Self {
        let start = if launcher.is_folder(&start) {
            start
        } else {
            launcher
                .recent_file_folders()
                .into_iter()
                .next()
                .unwrap_or_else(|| launcher.home_folder())
        };
        let mut dialog = Self {
            title: title.to_string(),
            hint: hint.to_string(),
            mode,
            path_text: start.to_string_lossy().to_string(),
            dir: start,
            folders: Vec::new(),
            files: Vec::new(),
            selected: Vec::new(),
            name,
            error: None,
            replace: None,
            ask_overwrite: true,
            confirm_text: None,
            focus_name: true,
            option: None,
            places: places_of(launcher),
            show_hidden: false,
            all_files: false,
            filter: String::new(),
            cursor: None,
            marked: None,
            new_folder: None,
            scroll_to_cursor: false,
            preview: None,
        };
        dialog.reload(launcher);
        dialog
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn extensions(&self) -> Vec<String> {
        if self.all_files {
            return Vec::new();
        }
        match &self.mode {
            Mode::Folder => Vec::new(),
            Mode::Open { extensions, .. } => extensions.clone(),
            Mode::Save { extension } => vec![extension.to_lowercase()],
        }
    }

    fn has_type_filter(&self) -> bool {
        match &self.mode {
            Mode::Folder => false,
            Mode::Open { extensions, .. } => !extensions.is_empty(),
            Mode::Save { .. } => true,
        }
    }

    fn type_label(&self) -> String {
        match &self.mode {
            Mode::Folder => String::new(),
            Mode::Open { extensions, .. } => type_label(extensions),
            Mode::Save { extension } => type_label(std::slice::from_ref(extension)),
        }
    }

    /// Whether the dialog chooses pictures (then it shows a preview).
    fn shows_pictures(&self) -> bool {
        match &self.mode {
            Mode::Open { extensions, .. } => extensions
                .iter()
                .any(|ext| strings::IMAGE_EXTENSIONS.contains(&ext.as_str())),
            _ => false,
        }
    }

    fn reload(&mut self, launcher: &Launcher) {
        if !launcher.is_folder(&self.dir) {
            self.error = Some(format!("Ordner nicht lesbar: {}", self.dir.display()));
            self.folders.clear();
            self.files.clear();
            return;
        }
        self.error = None;
        self.folders = launcher.subfolders(&self.dir);
        let extensions = self.extensions();
        self.files = match &self.mode {
            Mode::Folder => Vec::new(),
            _ => launcher
                .files(&self.dir)
                .into_iter()
                .filter(|file| matches_extension(file, &extensions))
                .collect(),
        };
        self.selected
            .retain(|file| file.parent() == Some(self.dir.as_path()));
        self.path_text = self.dir.to_string_lossy().to_string();
        self.cursor = None;
    }

    /// Folders first, then files, as shown: hidden ones only on request,
    /// narrowed by the name filter.
    fn entries(&self) -> Vec<Entry> {
        visible_entries(&self.folders, &self.files, &self.filter, self.show_hidden)
    }

    fn enter(&mut self, launcher: &Launcher, dir: PathBuf) {
        self.dir = dir;
        self.selected.clear();
        self.filter.clear();
        self.cursor = None;
        self.marked = None;
        self.new_folder = None;
        self.reload(launcher);
    }

    /// „Neuer Ordner“ where something is saved or a folder is chosen (not
    /// when only opening).
    fn can_make_folder(&self) -> bool {
        !matches!(self.mode, Mode::Open { .. })
    }

    /// A new row „Neuer Ordner“ (free name) with the name chosen for typing over.
    fn start_new_folder(&mut self) {
        let taken = |name: &str| {
            self.folders
                .iter()
                .chain(&self.files)
                .any(|path| file_name(path).to_lowercase() == name.to_lowercase())
        };
        let mut name = NEW_FOLDER.to_string();
        let mut number = 2;
        while taken(&name) {
            name = format!("{NEW_FOLDER} ({number})");
            number += 1;
        }
        self.new_folder = Some(NewFolder {
            name,
            error: None,
            focus: true,
        });
        self.marked = None;
        self.cursor = None;
    }

    /// Makes the folder being named; on success it is chosen, else the
    /// message stays at the field. True when made.
    fn make_folder(&mut self, launcher: &Launcher) -> bool {
        let Some(new) = self.new_folder.as_mut() else {
            return false;
        };
        let name = new.name.trim().to_string();
        let checked = if name.is_empty() {
            Err("Bitte einen Namen für den Ordner eingeben.".to_string())
        } else {
            check_name(&name, "Ordnername")
        };
        let made = checked.and_then(|()| {
            let exists = self
                .folders
                .iter()
                .chain(&self.files)
                .any(|path| file_name(path).to_lowercase() == name.to_lowercase());
            if exists {
                Err(format!("„{name}“ gibt es hier schon."))
            } else {
                launcher.create_folder(&self.dir, &name)
            }
        });
        match made {
            Ok(folder) => {
                self.new_folder = None;
                self.reload(launcher);
                self.cursor = self.entries().iter().position(|entry| entry.path == folder);
                self.scroll_to_cursor = true;
                self.marked = Some(folder);
                true
            }
            Err(error) => {
                new.error = Some(error);
                new.focus = true;
                false
            }
        }
    }

    /// The folder „Ordner wählen“ takes: the chosen subfolder, else the one shown.
    fn folder_choice(&self) -> &Path {
        self.marked.as_deref().unwrap_or(&self.dir)
    }

    fn confirm_label(&self) -> String {
        if let Some(text) = &self.confirm_text {
            return text.clone();
        }
        match self.mode {
            // Names the folder that is taken: the chosen one or the one shown.
            Mode::Folder => {
                let folder = self.folder_choice();
                let name = folder
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder.to_string_lossy().to_string());
                format!("Ordner „{}“ wählen", format::truncate_middle(&name, 32))
            }
            Mode::Open { .. } => strings::OPEN.to_string(),
            // A chosen folder is opened first (as in a file manager).
            Mode::Save { .. } if self.marked.is_some() => strings::OPEN.to_string(),
            Mode::Save { .. } => strings::FD_SAVE.to_string(),
        }
    }

    /// One click on a folder (or the keyboard row on it): chosen, not opened.
    /// In „Datei öffnen“ the files chosen before are let go.
    fn mark_folder(&mut self, folder: &Path) {
        self.marked = Some(folder.to_path_buf());
        if matches!(self.mode, Mode::Open { .. }) {
            self.selected.clear();
        }
    }

    /// Single click on a file (or the keyboard row on it).
    fn pick_file(&mut self, file: &Path, add: bool) {
        match &self.mode {
            Mode::Save { .. } => {
                self.name = file_name(file);
            }
            Mode::Open { multiple: true, .. } if add => {
                if self.selected.iter().any(|known| known == file) {
                    self.selected.retain(|known| known != file);
                } else {
                    self.selected.push(file.to_path_buf());
                }
            }
            _ => self.selected = vec![file.to_path_buf()],
        }
    }

    /// Loads the preview of the one selected picture when it changed.
    fn update_preview(&mut self, ctx: &egui::Context, launcher: &Launcher) {
        if !self.shows_pictures() {
            return;
        }
        let Some(file) = self.selected.last().cloned() else {
            self.preview = None;
            return;
        };
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.file == file)
        {
            return;
        }
        let picture = launcher.read_file(&file).and_then(|bytes| {
            if bytes.len() > PREVIEW_MAX_BYTES {
                return Err("Datei zu groß für die Vorschau.".to_string());
            }
            images::decode_preview(&bytes)
                .map_err(|_| "Kein lesbares PNG- oder JPEG-Bild.".to_string())
        });
        let picture = picture.map(|image| {
            ctx.load_texture(
                format!("file-dialog-preview-{}", file.display()),
                image,
                egui::TextureOptions::LINEAR,
            )
        });
        self.preview = Some(Preview { file, picture });
    }

    /// Draws the dialog as a modal window.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
    ) -> DialogResult {
        let mut enter_dir: Option<PathBuf> = None;
        let mut chosen: Option<Vec<PathBuf>> = None;
        let mut cancelled = false;
        let mut confirmed = false;
        let mut reload = false;
        let ctx = ui.ctx().clone();
        let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
        // Keys for the list only while no text field has the keyboard
        // (state of the last frame, before Enter takes a field's focus away).
        // While „Datei ersetzen?“ is open, its window has the keys.
        let question_open = self.replace.is_some();
        // Esc while naming a new folder drops it (not the whole dialog).
        if self.new_folder.is_some()
            && !question_open
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.new_folder = None;
        }
        let new_folder_key = self.can_make_folder()
            && !question_open
            && ui.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::N,
                )
            });
        let mut start_new_folder = new_folder_key;
        let mut make_folder = false;
        let typing = ctx.text_edit_focused() || question_open;
        let (down, up, backspace, alt_up) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::Backspace),
                input.modifiers.alt && input.key_pressed(egui::Key::ArrowUp),
            )
        });
        let entries = self.entries();
        // Only into folders the folder port shows (tests confine it).
        let parent = self
            .dir
            .parent()
            .map(Path::to_path_buf)
            .filter(|parent| launcher.is_folder(parent));
        if (alt_up && !question_open) || (!typing && backspace) {
            if let Some(parent) = parent.clone() {
                enter_dir = Some(parent);
            }
        } else if !typing && (down || up) && !entries.is_empty() {
            let next = move_cursor(self.cursor, entries.len(), if down { 1 } else { -1 });
            self.cursor = Some(next);
            self.scroll_to_cursor = true;
            match entries.get(next) {
                Some(entry) if entry.folder => self.mark_folder(&entry.path),
                Some(entry) => {
                    self.marked = None;
                    self.pick_file(&entry.path.clone(), false);
                }
                None => {}
            }
        } else if !typing && enter_pressed {
            match self.cursor.and_then(|index| entries.get(index)) {
                Some(entry) if entry.folder => enter_dir = Some(entry.path.clone()),
                Some(entry) => match &self.mode {
                    Mode::Save { .. } => confirmed = true,
                    _ if self.selected.len() > 1 => confirmed = true,
                    _ => chosen = Some(vec![entry.path.clone()]),
                },
                None => confirmed = true,
            }
        }

        let screen = ctx.content_rect();
        let width = (screen.width() - 60.0).clamp(560.0, 980.0);
        let list_height = (screen.height() * 0.42).clamp(200.0, 460.0);
        let pictures = self.shows_pictures();

        let (_, close) = widgets::modal(ui, palette, width, |ui| {
            widgets::title(ui, palette, &self.title, &self.hint);

            ui.horizontal(|ui| {
                let up_button = ui
                    .add_enabled_ui(parent.is_some(), |ui| {
                        widgets::icon_label_button(ui, palette, Icon::Up, strings::PICKER_UP)
                    })
                    .inner;
                if up_button
                    .on_hover_text("Alt+Pfeil hoch oder Rücktaste")
                    .clicked()
                {
                    enter_dir = parent.clone();
                }
                if self.can_make_folder()
                    && widgets::icon_label_button(ui, palette, Icon::NewFolder, NEW_FOLDER)
                        .on_hover_text(format!(
                            "Neuen Ordner im gezeigten Ordner anlegen ({NEW_FOLDER_KEY})"
                        ))
                        .clicked()
                {
                    start_new_folder = true;
                }
                ui.add_space(8.0);
                let caption =
                    ui.label(RichText::new(PATH_LABEL).size(12.0).color(palette.text_dim));
                let width = ui.available_width().max(120.0);
                let response = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.path_text)
                            .hint_text("Pfad eintippen und Enter")
                            .desired_width(width)
                            .margin(egui::Margin::symmetric(8, 7)),
                    )
                    .labelled_by(caption.id);
                if response.lost_focus() && enter_pressed {
                    let typed = typed_path(self.path_text.trim(), &launcher.home_folder());
                    if launcher.is_folder(&typed) {
                        enter_dir = Some(typed);
                    } else if typed
                        .parent()
                        .is_some_and(|parent| launcher.is_folder(parent))
                        && !matches!(self.mode, Mode::Folder)
                    {
                        match &self.mode {
                            Mode::Save { .. } => {
                                if let Some(parent) = typed.parent() {
                                    enter_dir = Some(parent.to_path_buf());
                                }
                                self.name = file_name(&typed);
                                confirmed = true;
                            }
                            _ if launcher.is_file(&typed) => chosen = Some(vec![typed]),
                            _ => {
                                self.error =
                                    Some(format!("Datei nicht gefunden: {}", typed.display()))
                            }
                        }
                    } else {
                        self.error = Some(format!("Ordner nicht gefunden: {}", typed.display()));
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let parts = breadcrumbs(&self.dir);
                let last = parts.len().saturating_sub(1);
                let separator = std::path::MAIN_SEPARATOR.to_string();
                // No extra separator right after a root that is one itself.
                let root_is_separator = parts
                    .first()
                    .is_some_and(|(label, _)| label.ends_with(&separator));
                for (index, (label, path)) in parts.into_iter().enumerate() {
                    if index > 0 && !(index == 1 && root_is_separator) {
                        ui.label(RichText::new(&separator).size(12.0).color(palette.text_dim));
                    }
                    // Parts to click in the accent colour, the shown folder in text colour.
                    let color = if index == last {
                        palette.text
                    } else {
                        palette.accent
                    };
                    let text = RichText::new(label).size(12.0).color(color);
                    if ui
                        .add(egui::Button::new(text).frame(false))
                        .on_hover_text(path.to_string_lossy())
                        .clicked()
                    {
                        enter_dir = Some(path);
                    }
                }
            });
            ui.add_space(6.0);

            ui.horizontal_top(|ui| {
                // Places
                let places_width = 190.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(places_width, list_height + 14.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(places_width);
                        egui::ScrollArea::vertical()
                            .id_salt("file_dialog_places")
                            .max_height(list_height + 14.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut group = None;
                                for place in &self.places {
                                    if group != Some(place.group) {
                                        if group.is_some() {
                                            ui.add_space(8.0);
                                        }
                                        group = Some(place.group);
                                        ui.label(
                                            RichText::new(match place.group {
                                                PlaceGroup::Places => PLACES_TITLE,
                                                PlaceGroup::Recent => RECENT_TITLE,
                                                PlaceGroup::Drives => DRIVES_TITLE,
                                            })
                                            .size(11.0)
                                            .strong()
                                            .color(palette.text_dim),
                                        );
                                        ui.add_space(2.0);
                                    }
                                    let current = place.path == self.dir;
                                    if place_row(ui, palette, &place.label, current)
                                        .on_hover_text(place.path.to_string_lossy())
                                        .clicked()
                                    {
                                        enter_dir = Some(place.path.clone());
                                    }
                                }
                            });
                    },
                );
                ui.add_space(8.0);

                // List (and preview)
                let preview_width = if pictures { 210.0 } else { 0.0 };
                let list_width = (ui.available_width() - preview_width).max(240.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(list_width, list_height + 14.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::new()
                            .fill(palette.panel_alt)
                            .stroke(Stroke::new(1.0, palette.line))
                            .corner_radius(egui::CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(list_width - 14.0);
                                egui::ScrollArea::vertical()
                                    .id_salt(("file_dialog_list", self.dir.clone()))
                                    .max_height(list_height)
                                    .min_scrolled_height(list_height)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        // Right click into the list: Neuer Ordner (the
                                        // rows lie above this background).
                                        let can_make = self.can_make_folder();
                                        let background = ui.interact(
                                            egui::Rect::from_min_size(
                                                ui.cursor().min,
                                                Vec2::new(
                                                    ui.available_width(),
                                                    list_height.max(entries.len() as f32 * 28.0),
                                                ),
                                            ),
                                            ui.scope_id().with("file_dialog_list_background"),
                                            Sense::click(),
                                        );
                                        if can_make {
                                            background.context_menu(|ui| {
                                                if new_folder_entry(ui) {
                                                    start_new_folder = true;
                                                }
                                            });
                                        }
                                        if let Some(new) = self.new_folder.as_mut() {
                                            make_folder |= new_folder_row(ui, palette, new);
                                        }
                                        if entries.is_empty() && self.new_folder.is_none() {
                                            ui.add_space(6.0);
                                            ui.label(
                                                RichText::new(strings::FD_EMPTY)
                                                    .size(12.5)
                                                    .color(palette.text_dim),
                                            );
                                        }
                                        for (index, entry) in entries.iter().enumerate() {
                                            let is_selected = if entry.folder {
                                                self.marked.as_ref() == Some(&entry.path)
                                            } else {
                                                self.selected.contains(&entry.path)
                                            };
                                            let response = entry_row(
                                                ui,
                                                palette,
                                                entry.folder,
                                                &file_name(&entry.path),
                                                is_selected,
                                                self.cursor == Some(index),
                                            );
                                            if self.scroll_to_cursor && self.cursor == Some(index) {
                                                response.scroll_to_me(None);
                                            }
                                            if can_make {
                                                response.context_menu(|ui| {
                                                    if new_folder_entry(ui) {
                                                        start_new_folder = true;
                                                    }
                                                });
                                            }
                                            // One click chooses, a double click opens
                                            // (Anwender 18.11.: as everywhere else).
                                            if entry.folder {
                                                if response.double_clicked() {
                                                    enter_dir = Some(entry.path.clone());
                                                } else if response.clicked() {
                                                    self.cursor = Some(index);
                                                    self.mark_folder(&entry.path);
                                                }
                                                continue;
                                            }
                                            if response.clicked() {
                                                let add = ui.input(|input| input.modifiers.command);
                                                self.cursor = Some(index);
                                                self.marked = None;
                                                self.pick_file(&entry.path, add);
                                            }
                                            if response.double_clicked() {
                                                match &self.mode {
                                                    Mode::Save { .. } => confirmed = true,
                                                    _ => chosen = Some(vec![entry.path.clone()]),
                                                }
                                            }
                                        }
                                    });
                            });
                    },
                );
                if pictures {
                    ui.add_space(8.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(preview_width - 8.0, list_height + 14.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.label(
                                RichText::new(PREVIEW_TITLE)
                                    .size(11.0)
                                    .strong()
                                    .color(palette.text_dim),
                            );
                            ui.add_space(4.0);
                            match self.preview.as_ref().map(|preview| &preview.picture) {
                                Some(Ok(texture)) => {
                                    let size = texture.size_vec2();
                                    ui.add(
                                        egui::Image::from_texture(
                                            egui::load::SizedTexture::from_handle(texture),
                                        )
                                        .max_size(
                                            egui::vec2(preview_width - 12.0, list_height - 30.0),
                                        ),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{} × {} Pixel",
                                            size.x as u32, size.y as u32
                                        ))
                                        .size(11.0)
                                        .color(palette.text_dim),
                                    );
                                }
                                Some(Err(error)) => {
                                    ui.label(RichText::new(error).size(11.5).color(palette.error));
                                }
                                None => {
                                    ui.label(
                                        RichText::new(PREVIEW_NONE)
                                            .size(11.5)
                                            .color(palette.text_dim),
                                    );
                                }
                            }
                        },
                    );
                }
            });
            ui.add_space(6.0);

            // Filter row
            ui.horizontal(|ui| {
                let filter = ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(FILTER_HINT)
                        .desired_width(200.0)
                        .margin(egui::Margin::symmetric(8, 5)),
                );
                if filter.changed() {
                    self.cursor = None;
                    self.marked = None;
                }
                ui.add_space(10.0);
                if self.has_type_filter() {
                    let type_caption =
                        ui.label(RichText::new(FILE_TYPE).size(12.0).color(palette.text_dim));
                    let type_label = self.type_label();
                    let current = if self.all_files {
                        ALL_FILES.to_string()
                    } else {
                        type_label.clone()
                    };
                    let before = self.all_files;
                    egui::ComboBox::from_id_salt("file_dialog_type")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.all_files, false, type_label);
                            ui.selectable_value(&mut self.all_files, true, ALL_FILES);
                        })
                        .response
                        .labelled_by(type_caption.id);
                    if self.all_files != before {
                        reload = true;
                    }
                    ui.add_space(10.0);
                }
                if ui.checkbox(&mut self.show_hidden, SHOW_HIDDEN).changed() {
                    self.cursor = None;
                    self.marked = None;
                }
            });
            ui.add_space(6.0);

            match &self.mode {
                Mode::Save { extension } => {
                    let name_caption = ui.label(
                        RichText::new(strings::FD_FILE_NAME)
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add_space(4.0);
                    let response = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.name)
                                .desired_width(ui.available_width())
                                .margin(egui::Margin::symmetric(8, 7)),
                        )
                        .labelled_by(name_caption.id);
                    if self.focus_name {
                        response.request_focus();
                        self.focus_name = false;
                    }
                    if response.changed() {
                        // Typing a name means the file, not a chosen folder.
                        self.marked = None;
                    }
                    if response.lost_focus() && enter_pressed && !question_open {
                        confirmed = true;
                    }
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("Endung .{extension} wird ergänzt, wenn sie fehlt."))
                            .size(11.5)
                            .color(palette.text_dim),
                    );
                    if let Some((label, value)) = &mut self.option {
                        ui.add_space(6.0);
                        ui.checkbox(value, label.as_str());
                    }
                    ui.add_space(8.0);
                }
                Mode::Open { multiple, .. } => {
                    let text = match self.selected.len() {
                        0 if *multiple => strings::FD_PICK_MANY.to_string(),
                        0 => strings::FD_PICK_ONE.to_string(),
                        1 => file_name(&self.selected[0]),
                        count => format!("{count} Dateien gewählt"),
                    };
                    ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
                    ui.add_space(8.0);
                }
                Mode::Folder => {
                    ui.label(
                        RichText::new(format::truncate_middle(
                            &self.folder_choice().to_string_lossy(),
                            80,
                        ))
                        .size(12.0)
                        .color(palette.text_dim),
                    );
                    ui.add_space(8.0);
                }
            }
            ui.label(RichText::new(KEYS_HINT).size(11.0).color(palette.text_dim));
            ui.add_space(6.0);

            widgets::error_line(ui, palette, self.error.as_deref());
            let (no, yes) = widgets::dialog_buttons(ui, palette, &self.confirm_label());
            cancelled = no;
            confirmed |= yes;
        });
        self.scroll_to_cursor = false;
        if start_new_folder {
            self.start_new_folder();
        }
        // The dialog's button while a new folder is being named: make it
        // first (chosen afterwards), then go on only if that worked.
        if confirmed && self.new_folder.is_some() {
            make_folder = true;
        }
        if make_folder && !self.make_folder(launcher) {
            return DialogResult::Open;
        }
        if make_folder && !matches!(self.mode, Mode::Folder) {
            // Saving: the new folder is chosen; „Speichern“ again opens it.
            confirmed = false;
        }

        if let Some((question, file)) = &mut self.replace {
            match question.show(ui, palette) {
                ReplaceAnswer::Open => {}
                ReplaceAnswer::Cancel => self.replace = None,
                ReplaceAnswer::Replace => {
                    let file = file.clone();
                    self.replace = None;
                    if let Some(folder) = file.parent() {
                        launcher.remember_file_folder(folder);
                    }
                    return DialogResult::Chosen(vec![file]);
                }
            }
            return DialogResult::Open;
        }

        // „Öffnen“/„Speichern“ with a chosen folder opens it first.
        if confirmed && enter_dir.is_none() && !matches!(self.mode, Mode::Folder) {
            if let Some(folder) = self.marked.clone() {
                enter_dir = Some(folder);
                confirmed = false;
            }
        }
        if let Some(dir) = enter_dir {
            self.enter(launcher, dir);
            if !confirmed {
                return DialogResult::Open;
            }
        }
        if reload {
            self.reload(launcher);
        }
        if close || cancelled {
            return DialogResult::Cancelled;
        }
        if let Some(files) = chosen {
            if let Some(folder) = files.first().and_then(|file| file.parent()) {
                launcher.remember_file_folder(folder);
            }
            return DialogResult::Chosen(files);
        }
        if confirmed {
            let result = self.confirm(launcher);
            if let DialogResult::Chosen(paths) = &result {
                let folder = match self.mode {
                    Mode::Folder => paths.first().map(PathBuf::as_path),
                    _ => paths.first().and_then(|file| file.parent()),
                };
                if let Some(folder) = folder {
                    launcher.remember_file_folder(folder);
                }
            }
            return result;
        }
        self.update_preview(&ctx, launcher);
        DialogResult::Open
    }

    fn confirm(&mut self, launcher: &Launcher) -> DialogResult {
        match &self.mode {
            Mode::Folder => DialogResult::Chosen(vec![self.folder_choice().to_path_buf()]),
            Mode::Open { .. } => {
                if self.selected.is_empty() {
                    self.error = Some(strings::FD_PICK_ONE.to_string());
                    DialogResult::Open
                } else {
                    DialogResult::Chosen(self.selected.clone())
                }
            }
            Mode::Save { extension } => {
                let name = self.name.trim();
                if name.is_empty() {
                    self.error = Some(strings::FD_NAME_MISSING.to_string());
                    return DialogResult::Open;
                }
                if let Err(error) = check_file_name(name) {
                    self.error = Some(error);
                    return DialogResult::Open;
                }
                let file = self.dir.join(ensure_extension(name, extension));
                if launcher.is_folder(&file) {
                    self.error = Some(format!("„{}“ ist ein Ordner.", file_name(&file)));
                    return DialogResult::Open;
                }
                if self.ask_overwrite && launcher.is_file(&file) {
                    // One click on „Speichern“, then the question (Befund 6).
                    let question = ReplaceQuestion::new(launcher, vec![file.clone()]);
                    self.replace = Some((question, file));
                    return DialogResult::Open;
                }
                DialogResult::Chosen(vec![file])
            }
        }
    }
}

/// Personal folders, the default folder, the recently used folders and the
/// drives, in this order and without doubles.
fn places_of(launcher: &Launcher) -> Vec<Place> {
    let mut places: Vec<Place> = Vec::new();
    let mut add = |group: PlaceGroup, label: String, path: PathBuf| {
        if !places.iter().any(|known| known.path == path) {
            places.push(Place { group, label, path });
        }
    };
    add(
        PlaceGroup::Places,
        HOME_LABEL.to_string(),
        launcher.home_folder(),
    );
    for (kind, folder) in launcher.user_folders() {
        let label = match kind {
            UserFolder::Desktop => "Schreibtisch",
            UserFolder::Documents => "Dokumente",
            UserFolder::Pictures => "Bilder",
            UserFolder::Downloads => "Downloads",
        };
        add(PlaceGroup::Places, label.to_string(), folder);
    }
    if let Some(folder) = launcher
        .default_folder()
        .filter(|folder| launcher.is_folder(folder))
    {
        add(PlaceGroup::Places, DEFAULT_FOLDER_LABEL.to_string(), folder);
    }
    for folder in launcher.recent_file_folders() {
        add(PlaceGroup::Recent, file_name(&folder), folder);
    }
    for drive in launcher.drives() {
        let label = drive_label(&drive);
        add(PlaceGroup::Drives, label, drive);
    }
    places
}

/// `/` → „Dateisystem (/)“, `C:\` → „C:“, mounted disks by their path.
fn drive_label(drive: &Path) -> String {
    let text = drive.to_string_lossy();
    if text == "/" {
        return "Dateisystem (/)".to_string();
    }
    let trimmed = text.trim_end_matches(['\\', '/']);
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        return trimmed.to_string();
    }
    text.to_string()
}

/// „Logo (PNG, JPG)“-style text of an extension filter: `PNG, JPG, JPEG`.
fn type_label(extensions: &[String]) -> String {
    extensions
        .iter()
        .map(|ext| ext.to_uppercase())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Folders first, then files; hidden (dot) entries only when wanted; the
/// filter keeps names containing the text (case-insensitive).
fn visible_entries(
    folders: &[PathBuf],
    files: &[PathBuf],
    filter: &str,
    show_hidden: bool,
) -> Vec<Entry> {
    let filter = filter.trim().to_lowercase();
    let keep = |path: &PathBuf| {
        let name = file_name(path);
        (show_hidden || !name.starts_with('.'))
            && (filter.is_empty() || name.to_lowercase().contains(&filter))
    };
    folders
        .iter()
        .filter(|path| keep(path))
        .map(|path| Entry {
            path: path.clone(),
            folder: true,
        })
        .chain(files.iter().filter(|path| keep(path)).map(|path| Entry {
            path: path.clone(),
            folder: false,
        }))
        .collect()
}

/// Next keyboard row: down from nothing = first, up from nothing = last,
/// stays inside the list.
fn move_cursor(cursor: Option<usize>, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    match cursor {
        None if delta >= 0 => 0,
        None => len - 1,
        Some(index) => {
            let next = index as i64 + i64::from(delta);
            next.clamp(0, len as i64 - 1) as usize
        }
    }
}

/// The parts of a path to click on: `/`, `home`, `oh` … with the folder
/// each part stands for. On Windows the drive or share and its root are one
/// part (`C:` → `C:\`, `\\server\share` → `\\server\share\`): a bare
/// `C:` would be the current folder of drive C, not its root.
fn breadcrumbs(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut parts: Vec<(String, PathBuf)> = Vec::new();
    let mut current = PathBuf::new();
    for component in dir.components() {
        current.push(component.as_os_str());
        match component {
            std::path::Component::RootDir => match parts.last_mut() {
                Some(prefix) => prefix.1 = current.clone(),
                None => parts.push((std::path::MAIN_SEPARATOR.to_string(), current.clone())),
            },
            other => parts.push((
                other.as_os_str().to_string_lossy().to_string(),
                current.clone(),
            )),
        }
    }
    parts
}

/// What a typed path means: `~` and `~/…` stand for the personal folder;
/// on Windows a bare drive (`D:`) for its root (`D:\`).
fn typed_path(typed: &str, home: &Path) -> PathBuf {
    bare_drive_to_root(expand_home(typed, home), cfg!(windows))
}

fn bare_drive_to_root(path: PathBuf, windows: bool) -> PathBuf {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if windows && bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return PathBuf::from(format!("{text}\\"));
    }
    path
}

/// `~` and `~/…` in a typed path stand for the personal folder.
fn expand_home(typed: &str, home: &Path) -> PathBuf {
    if typed == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = typed
        .strip_prefix("~/")
        .or_else(|| typed.strip_prefix("~\\"))
    {
        return home.join(rest);
    }
    PathBuf::from(typed)
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// Whether the file ends with one of the (lower case) extensions; no filter = all files.
pub fn matches_extension(path: &Path, extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| extensions.contains(&ext))
}

/// A file name that works on Linux and Windows (the app runs on both):
/// no path separators, none of `< > : " | ? *`, no control characters,
/// not ending in a dot or space, no reserved device names (CON, NUL, COM1 …).
pub fn check_file_name(name: &str) -> Result<(), String> {
    check_name(name, "Dateiname")
}

/// Same rules for a new folder („Der Ordnername …“).
pub fn check_folder_name(name: &str) -> Result<(), String> {
    check_name(name, "Ordnername")
}

/// Names allowed on Windows and Linux; `what` = „Dateiname“ or „Ordnername“.
fn check_name(name: &str, what: &str) -> Result<(), String> {
    if let Some(bad) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control()
    }) {
        let shown = if bad.is_control() {
            "Steuerzeichen".to_string()
        } else {
            format!("„{bad}“")
        };
        return Err(format!(
            "Der {what} enthält ein unerlaubtes Zeichen: {shown} (nicht erlaubt: / \\ < > : \" | ? *)."
        ));
    }
    if name == "." || name == ".." || name.ends_with('.') || name.ends_with(' ') {
        return Err(format!(
            "Der {what} darf nicht mit Punkt oder Leerzeichen enden."
        ));
    }
    let stem = name.split('.').next().unwrap_or(name).trim().to_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        return Err(format!("„{stem}“ ist unter Windows als {what} reserviert."));
    }
    if name.chars().count() > 200 {
        return Err(format!("Der {what} ist zu lang (höchstens 200 Zeichen)."));
    }
    Ok(())
}

/// Adds `.ext` unless the name already ends with it (case-insensitive).
pub fn ensure_extension(name: &str, extension: &str) -> String {
    let suffix = format!(".{}", extension.to_lowercase());
    if name.to_lowercase().ends_with(&suffix) {
        name.to_string()
    } else {
        format!("{name}.{extension}")
    }
}

/// `<Projekt>_<Dokument>_<JJJJ-MM-TT>.<ext>`
pub fn suggested_name(project: &str, document: &str, date_iso: &str, extension: &str) -> String {
    let date = date_iso.get(0..10).unwrap_or(date_iso);
    format!("{project}_{document}_{date}.{extension}")
}

/// One line of the places column (text only).
fn place_row(ui: &mut egui::Ui, palette: &Palette, label: &str, current: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::click());
    widgets::name_button(ui, &response, label);
    let background = if current {
        palette.selected
    } else if response.hovered() {
        palette.row_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::CornerRadius::same(5), background);
    painter.text(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        format::truncate_middle(label, 26),
        FontId::proportional(12.5),
        palette.text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The entry „Neuer Ordner“ of a right-click menu; true when chosen.
fn new_folder_entry(ui: &mut egui::Ui) -> bool {
    let chosen = ui
        .add(egui::Button::new(NEW_FOLDER).shortcut_text(NEW_FOLDER_KEY))
        .clicked();
    if chosen {
        ui.close();
    }
    chosen
}

/// The row of a new folder: symbol and name field (Enter makes it, Esc drops
/// it), the message below; true when Enter was pressed in the field.
fn new_folder_row(ui: &mut egui::Ui, palette: &Palette, new: &mut NewFolder) -> bool {
    let mut enter = false;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::hover());
        icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
            Icon::Folder,
            palette.accent,
            palette.panel_alt,
        );
        let id = ui.scope_id().with("file_dialog_new_folder");
        if new.focus {
            // All of the name chosen: typing replaces it.
            ui.memory_mut(|memory| memory.request_focus(id));
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(new.name.chars().count()),
                )));
            state.store(ui.ctx(), id);
            new.focus = false;
        }
        let response = ui.add(
            egui::TextEdit::singleline(&mut new.name)
                .id(id)
                .desired_width(ui.available_width() - 8.0)
                .margin(egui::Margin::symmetric(6, 4)),
        );
        if response.changed() {
            new.error = None;
        }
        enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    });
    if let Some(error) = &new.error {
        ui.label(RichText::new(error).size(11.5).color(palette.error));
    }
    enter
}

/// One line of the list: folder or document symbol, the name, folders with
/// „Ordner“ on the right.
fn entry_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    folder: bool,
    name: &str,
    selected: bool,
    cursor: bool,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let shown = if folder {
        format!("{name}/")
    } else {
        name.to_string()
    };
    widgets::name_button(ui, &response, &shown);
    let background = if selected {
        palette.selected
    } else if response.hovered() {
        palette.row_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(6), background);
    let (icon, color) = if folder {
        (Icon::Folder, palette.accent)
    } else {
        (Icon::Document, palette.text_dim)
    };
    icons::draw(
        painter,
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        icon,
        color,
        background,
    );
    if cursor {
        painter.rect_stroke(
            rect.shrink(0.5),
            egui::CornerRadius::same(6),
            Stroke::new(1.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
    painter.text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        palette.text,
    );
    if folder {
        painter.text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            FOLDER_KIND,
            FontId::proportional(11.5),
            palette.text_dim,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        bare_drive_to_root, breadcrumbs, check_file_name, drive_label, ensure_extension,
        expand_home, matches_extension, move_cursor, suggested_name, type_label, visible_entries,
    };

    #[test]
    fn extension_filter_ignores_case() {
        let filter = vec!["x84".to_string(), "d84".to_string()];
        assert!(matches_extension(Path::new("/a/Bieter.X84"), &filter));
        assert!(matches_extension(Path::new("/a/b.d84"), &filter));
        assert!(!matches_extension(Path::new("/a/b.x83"), &filter));
        assert!(!matches_extension(Path::new("/a/b"), &filter));
        assert!(matches_extension(Path::new("/a/b"), &[]));
    }

    #[test]
    fn file_names_are_checked_for_windows_and_linux() {
        assert!(check_file_name("GA Schule Nord & Süd ß €_Preisspiegel_2026-11-17.pdf").is_ok());
        assert!(check_file_name("Markthalle Nord").is_ok());
        assert!(check_file_name("a/b").is_err());
        assert!(check_file_name("a\\b").is_err());
        assert!(check_file_name("Angebot: Juni").is_err());
        assert!(check_file_name("LV \"neu\"").is_err());
        assert!(check_file_name("was?").is_err());
        assert!(check_file_name("Ende.").is_err());
        assert!(check_file_name("..").is_err());
        assert!(check_file_name("con.pdf").is_err());
        assert!(check_file_name("COM1").is_err());
        assert!(check_file_name("COM").is_ok());
        assert!(check_file_name("Console.pdf").is_ok());
        assert!(check_file_name("Tab\tName").is_err());
        assert!(check_file_name(&"x".repeat(201)).is_err());
    }

    #[test]
    fn names_get_their_extension_once() {
        assert_eq!(ensure_extension("LV", "X83"), "LV.X83");
        assert_eq!(ensure_extension("LV.x83", "X83"), "LV.x83");
        assert_eq!(
            suggested_name(
                "Schule",
                "Angebotsaufforderung",
                "2026-11-17T18:00:00",
                "X83"
            ),
            "Schule_Angebotsaufforderung_2026-11-17.X83"
        );
    }

    #[test]
    fn list_shows_folders_first_and_hides_dot_entries_unless_asked() {
        let folders = vec![PathBuf::from("/p/.git"), PathBuf::from("/p/Logos")];
        let files = vec![
            PathBuf::from("/p/.versteckt.png"),
            PathBuf::from("/p/Firmenlogo.png"),
        ];
        let names = |show_hidden: bool, filter: &str| {
            visible_entries(&folders, &files, filter, show_hidden)
                .into_iter()
                .map(|entry| (super::file_name(&entry.path), entry.folder))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(false, ""),
            vec![
                ("Logos".to_string(), true),
                ("Firmenlogo.png".to_string(), false)
            ]
        );
        assert_eq!(names(true, "").len(), 4);
        assert_eq!(
            names(false, "firmen"),
            vec![("Firmenlogo.png".to_string(), false)]
        );
    }

    #[test]
    fn keyboard_row_stays_in_the_list() {
        assert_eq!(move_cursor(None, 3, 1), 0);
        assert_eq!(move_cursor(None, 3, -1), 2);
        assert_eq!(move_cursor(Some(2), 3, 1), 2);
        assert_eq!(move_cursor(Some(0), 3, -1), 0);
        assert_eq!(move_cursor(Some(1), 3, 1), 2);
    }

    #[cfg(unix)]
    #[test]
    fn path_parts_lead_to_their_folders() {
        let parts = breadcrumbs(Path::new("/home/nutzer/Bilder"));
        assert_eq!(
            parts,
            vec![
                ("/".to_string(), PathBuf::from("/")),
                ("home".to_string(), PathBuf::from("/home")),
                ("nutzer".to_string(), PathBuf::from("/home/nutzer")),
                ("Bilder".to_string(), PathBuf::from("/home/nutzer/Bilder")),
            ]
        );
        assert_eq!(breadcrumbs(Path::new("/")).len(), 1);
    }

    #[test]
    fn a_bare_drive_means_its_root_on_windows() {
        let path = |text: &str| PathBuf::from(text);
        assert_eq!(bare_drive_to_root(path("D:"), true), path("D:\\"));
        assert_eq!(bare_drive_to_root(path("d:"), true), path("d:\\"));
        assert_eq!(
            bare_drive_to_root(path("D:\\Projekte"), true),
            path("D:\\Projekte")
        );
        assert_eq!(
            bare_drive_to_root(path("D:"), false),
            path("D:"),
            "unter Linux ein Name"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_parts_keep_drive_and_share_roots() {
        assert_eq!(
            breadcrumbs(Path::new("C:\\Users\\oh")),
            vec![
                ("C:".to_string(), PathBuf::from("C:\\")),
                ("Users".to_string(), PathBuf::from("C:\\Users")),
                ("oh".to_string(), PathBuf::from("C:\\Users\\oh")),
            ]
        );
        let parts = breadcrumbs(Path::new("\\\\server\\freigabe\\Projekte"));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].1, PathBuf::from("\\\\server\\freigabe\\"));
        assert_eq!(parts[1].1, PathBuf::from("\\\\server\\freigabe\\Projekte"));
    }

    #[test]
    fn typed_paths_and_labels() {
        let home = Path::new("/home/nutzer");
        assert_eq!(expand_home("~", home), PathBuf::from("/home/nutzer"));
        assert_eq!(
            expand_home("~/Bilder", home),
            PathBuf::from("/home/nutzer/Bilder")
        );
        assert_eq!(expand_home("/tmp", home), PathBuf::from("/tmp"));
        assert_eq!(drive_label(Path::new("/")), "Dateisystem (/)");
        assert_eq!(drive_label(Path::new("C:\\")), "C:");
        assert_eq!(drive_label(Path::new("/daten")), "/daten");
        assert_eq!(
            type_label(&["png".to_string(), "jpg".to_string()]),
            "PNG, JPG"
        );
    }
}
