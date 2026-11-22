//! Long text in its own operating-system window (egui viewport), for long
//! texts: big writing area, full toolbar, short text on top. Changes go into
//! the project at once (with undo, like in the edit panel); Esc or the close
//! button of the window closes it. Where a backend cannot open more windows,
//! egui shows the same content as a movable window inside the program.

use eframe::egui;
use proava_domain::{LvRow, Project, RichText, RowId};

use crate::file_dialog::{DialogResult, FileDialog};
use crate::rich_editor::RichEditor;
use crate::theme::Palette;

const DEFAULT_SIZE: (f32, f32) = (1000.0, 760.0);

pub struct BigText {
    pub row: RowId,
    /// Window level: in front of the main window while proAVA has the focus
    /// (`window_level`).
    pub level: egui::WindowLevel,
    /// „Festhalten“: keeps its row when the table selects another.
    pub held: bool,
    /// Another row to show, taken once a focused field let go (its value first).
    pub pending: Option<RowId>,
    /// A field of this window had the keyboard in the last frame.
    field_focused: bool,
    /// The selected row has no long text (title, Abstandshalter): the window
    /// stays on its row and says so.
    pub skipped: Option<String>,
    pub editor: RichEditor,
    /// Bring the window to the front on the next frame.
    pub focus: bool,
    /// Keyboard focus after opening/raising (Esc works without a click
    /// first, Anwender 18.11.).
    focus_request: crate::window_focus::FocusRequest,
    short: String,
    short_loaded: Option<String>,
    size: Option<(f32, f32)>,
    started: bool,
    /// „Bild einfügen“ inside this window.
    dialog: Option<FileDialog>,
    /// Folder of the last picture chosen here.
    image_folder: Option<std::path::PathBuf>,
    /// „Ausschnitt aus PDF einfügen …“ inside this window.
    clip_dialog: Option<crate::pdf_clip_dialog::PdfClipDialog>,
}

/// What the window wants from the edit panel.
#[derive(Default)]
pub struct BigTextResult {
    pub close: bool,
    /// Last inner size, to remember in the settings when closing.
    pub size: Option<(f32, f32)>,
    pub pick_image: bool,
    /// A PDF clip chosen in this window and its width in mm.
    pub clip: Option<(proava_application::PreparedClip, f32)>,
    /// A message for the status line (e.g. a dropped PDF that cannot be read).
    pub toast: Option<String>,
    /// Image files dropped or pasted into the window.
    pub files: Vec<(String, Vec<u8>)>,
    /// New long text, its Textergänzungen (positions, hints) and the undo key.
    pub changed: Option<(
        RichText,
        Option<Vec<proava_domain::TextComplement>>,
        Option<String>,
    )>,
    pub short_text: Option<String>,
    /// Strg+Z / Strg+Y (Strg+Umschalt+Z) in this window: LV undo/redo.
    pub undo: bool,
    pub redo: bool,
    /// F9, F10 … in this window: a new row after the window's row.
    pub new_row: Option<crate::lv_editor::MenuChoice>,
}

impl BigText {
    pub fn new(row: RowId) -> Self {
        let mut editor = RichEditor::new("long_text_window");
        editor.pdf_clips = true;
        editor.min_height = 420.0;
        editor.zoom = 1.1;
        editor.request_focus();
        Self {
            row,
            level: egui::WindowLevel::Normal,
            held: false,
            pending: None,
            field_focused: false,
            skipped: None,
            editor,
            focus: true,
            focus_request: crate::window_focus::FocusRequest::default(),
            short: String::new(),
            short_loaded: None,
            size: None,
            started: false,
            dialog: None,
            image_folder: None,
            clip_dialog: None,
        }
    }

    /// Whether a field of this window had the keyboard in the last frame.
    pub fn field_focused(&self) -> bool {
        self.field_focused
    }

    /// Shows another row (it follows the table); the window stays where it is.
    pub fn show_row(&mut self, row: RowId) {
        if self.row != row {
            self.row = row;
            self.editor.reset();
            self.short_loaded = None;
        }
        self.pending = None;
        self.skipped = None;
    }

    /// Another LV became the active one: show its row `row` right away and
    /// load it fresh (row ids are only unique within one LV), not held.
    pub fn other_lv(&mut self, row: RowId) {
        self.row = row;
        self.editor.reset();
        self.short_loaded = None;
        self.pending = None;
        self.skipped = None;
        self.held = false;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &Project,
        index: usize,
        launcher: &proava_application::Launcher,
    ) -> BigTextResult {
        let mut result = BigTextResult::default();
        let row = &project.lv().rows()[index];
        let (kind, title, text, short, complements) = match row {
            LvRow::Position(position) => (
                "Langtext",
                format!("{} {}", position.ordinal(), position.short_text()),
                position.long_text().clone(),
                Some(position.short_text().to_string()),
                Some(position.complements().to_vec()),
            ),
            LvRow::Note(note) => (
                "Hinweis",
                note.heading().to_string(),
                note.text().clone(),
                None,
                Some(note.complements().to_vec()),
            ),
            LvRow::Performance(performance) => (
                "Ausführungsbeschreibung",
                performance.title(),
                performance.long_text().clone(),
                None,
                Some(performance.complements().to_vec()),
            ),
            LvRow::Group(_) | LvRow::Spacer(_) => {
                result.close = true;
                return result;
            }
        };
        if let Some(short) = &short {
            if self.short_loaded.as_ref() != Some(short) {
                self.short = short.clone();
                self.short_loaded = Some(short.clone());
            }
        }
        // „Ausführung nach A0001 – Kabeltrassen“ of a position (title bar and head).
        let link = row
            .as_position()
            .and_then(|position| project.lv().performance_of(position))
            .map(|performance| {
                let label = performance.title();
                if label.is_empty() {
                    format!("Ausführung nach {}", performance.mark())
                } else {
                    format!("Ausführung nach {} – {label}", performance.mark())
                }
            });
        // STLB-Bau-Text: shown, write-protected (Kurztext and values free).
        let text_locked = row
            .as_position()
            .is_some_and(proava_domain::Position::is_text_locked);
        let mut window_title = if title.trim().is_empty() {
            format!("{kind} – proAVA")
        } else {
            format!("{kind} – {}", title.trim())
        };
        if let Some(link) = &link {
            window_title.push_str(" · ");
            window_title.push_str(link);
        }
        let size = launcher.long_text_window().unwrap_or(DEFAULT_SIZE);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(window_title)
            .with_inner_size([size.0, size.1])
            .with_min_inner_size([520.0, 360.0])
            .with_window_level(self.level);
        if !self.started {
            self.started = true;
        }
        if std::mem::take(&mut self.focus) {
            builder = builder.with_active(true);
            ctx.send_viewport_cmd_to(Self::id(), egui::ViewportCommand::Focus);
            // That request comes before the window is mapped and may be lost;
            // the window asks again from inside until it has the focus.
            self.focus_request.start();
        }

        let images = project.images();
        ctx.show_viewport_immediate(Self::id(), builder, |ui, class| {
            let ctx = ui.ctx().clone();
            if class != egui::ViewportClass::EmbeddedWindow {
                self.focus_request.keep(&ctx);
            }
            // Another row is due: first let the focused field go (it takes
            // its value), the switch follows in the next frame.
            self.field_focused = ctx.text_edit_focused();
            if self.pending.is_some() && self.field_focused {
                ctx.memory_mut(|memory| {
                    if let Some(focused) = memory.focused() {
                        memory.surrender_focus(focused);
                    }
                });
                ctx.request_repaint();
            }
            if ctx.input(|input| input.viewport().close_requested()) {
                result.close = true;
            }
            // The root Ui spans the whole window (inner size in points);
            // maximised or minimised it keeps the size it had before.
            let (rect, whole_screen) = ctx.input(|input| {
                let viewport = input.viewport();
                (
                    viewport.inner_rect.unwrap_or_else(|| ui.max_rect()),
                    viewport.maximized == Some(true)
                        || viewport.minimized == Some(true)
                        || viewport.fullscreen == Some(true),
                )
            });
            if !whole_screen && rect.width() > 100.0 && rect.height() > 100.0 {
                result.size = Some((rect.width(), rect.height()));
                self.size = result.size;
            }
            // Images dropped on or pasted into this window.
            let (dropped, events) =
                ctx.input(|input| (input.raw.dropped_files.clone(), input.events.clone()));
            // New rows (F9 …) after the window's row, as in the table. As a
            // window of its own it gets these keys itself; embedded the
            // table's keyboard has them (after the selected row).
            if class != egui::ViewportClass::EmbeddedWindow
                && self.dialog.is_none()
                && self.clip_dialog.is_none()
            {
                if let Some((choice, keys)) = crate::lv_editor::new_row_keys(&events) {
                    result.new_row = Some(choice);
                    ctx.input_mut(|input| {
                        input.events.retain(|event| {
                            !matches!(event, egui::Event::Key { key, modifiers, .. }
                                if keys.contains(&(*key, *modifiers)))
                        })
                    });
                }
            }
            if text_locked && !dropped.is_empty() {
                result.toast = Some(proava_domain::stlb::STLB_LOCKED.to_string());
            }
            for file in dropped.into_iter().filter(|_| !text_locked) {
                if let Ok(bytes) = file.bytes() {
                    let name = crate::file_dialog::file_name(file.path());
                    if proava_domain::is_pdf(&bytes) {
                        // A PDF: choose the part of a page.
                        if self.clip_dialog.is_none() {
                            match crate::pdf_clip_dialog::PdfClipDialog::with_pdf(
                                launcher, &name, bytes,
                            ) {
                                Ok(dialog) => self.clip_dialog = Some(dialog),
                                Err(error) => result.toast = Some(error),
                            }
                        }
                        continue;
                    }
                    result.files.push((name, bytes));
                }
            }
            for event in events.into_iter().filter(|_| !text_locked) {
                if let egui::Event::PasteImage(image) = event {
                    if let Ok(bytes) = crate::images::encode_png(&image) {
                        result.files.push(("Zwischenablage.png".into(), bytes));
                    }
                }
            }
            if self.dialog.is_none()
                && self.clip_dialog.is_none()
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                result.close = true;
            }
            // Undo/redo of the LV, as in the main window.
            ctx.input_mut(|input| {
                let shift_z = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
                if input.consume_key(shift_z, egui::Key::Z)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                {
                    result.redo = true;
                } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
                    result.undo = true;
                }
            });
            if let Some(dialog) = &mut self.clip_dialog {
                match dialog.show(ui, palette, launcher) {
                    crate::pdf_clip_dialog::ClipDialogResult::Open => {}
                    crate::pdf_clip_dialog::ClipDialogResult::Cancelled => self.clip_dialog = None,
                    crate::pdf_clip_dialog::ClipDialogResult::Chosen(prepared, width) => {
                        result.clip = Some((*prepared, width));
                        self.clip_dialog = None;
                    }
                }
            }
            if let Some(dialog) = &mut self.dialog {
                match dialog.show(ui, palette, launcher) {
                    DialogResult::Open => {}
                    DialogResult::Cancelled => self.dialog = None,
                    DialogResult::Chosen(paths) => {
                        for path in &paths {
                            if let Ok(bytes) = launcher.read_file(path) {
                                result
                                    .files
                                    .push((crate::file_dialog::file_name(path), bytes));
                            }
                        }
                        self.image_folder = paths
                            .first()
                            .and_then(|path| path.parent())
                            .map(std::path::Path::to_path_buf);
                        self.dialog = None;
                    }
                }
            }

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(palette.background)
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ui, |ui| {
                    crate::widgets::row(ui, |ui| {
                        ui.label(
                            egui::RichText::new(kind)
                                .size(15.0)
                                .strong()
                                .color(palette.text),
                        );
                        if let Some(link) = &link {
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(link)
                                    .size(13.0)
                                    .color(palette.text)
                                    .background_color(palette.accent_soft),
                            )
                            .on_hover_text(
                                "Verknüpfung ändern: Fenster „Eigenschaften“ (Alt+Enter) oder Seitenleiste",
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Schließen (Esc)")
                                .on_hover_text("Änderungen sind schon übernommen")
                                .clicked()
                            {
                                result.close = true;
                            }
                            ui.add_space(8.0);
                            ui.checkbox(&mut self.held, crate::strings::HOLD_WINDOW)
                                .on_hover_text(crate::strings::HOLD_WINDOW_HINT);
                            ui.label(
                                egui::RichText::new(
                                    "Änderungen wirken sofort – Strg+Z macht sie rückgängig",
                                )
                                .size(11.5)
                                .color(palette.text_dim),
                            );
                        });
                    });
                    if let Some(skipped) = &self.skipped {
                        ui.label(
                            egui::RichText::new(skipped)
                                .size(12.5)
                                .color(palette.text_dim),
                        );
                    }
                    ui.add_space(6.0);
                    if short.is_some() {
                        crate::widgets::row(ui, |ui| {
                            ui.label(egui::RichText::new("Kurztext").color(palette.text_dim));
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.short)
                                    .desired_width(f32::INFINITY),
                            );
                            if response.changed() {
                                result.short_text = Some(self.short.clone());
                                self.short_loaded = Some(self.short.clone());
                            }
                        });
                    } else if !title.trim().is_empty() {
                        ui.label(egui::RichText::new(title.trim()).color(palette.text_dim));
                    }
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.editor.min_height = (ui.available_height() - 70.0).max(200.0);
                            let mut edited = text.clone();
                            self.editor.field_presets =
                                crate::rich_editor::FieldPresets::of(launcher);
                            let mut list = complements.clone().unwrap_or_default();
                            if text_locked {
                                ui.label(
                                    egui::RichText::new(proava_domain::stlb::STLB_LOCKED)
                                        .size(12.5)
                                        .color(palette.text_dim),
                                );
                                ui.add_space(4.0);
                                let editor = &mut self.editor;
                                let shown = ui.add_enabled_ui(false, |ui| {
                                    editor.show_with_complements(
                                        ui,
                                        palette,
                                        &mut edited,
                                        &mut list,
                                        images,
                                    )
                                });
                                if shown.inner.changed {
                                    self.editor.reset();
                                }
                                if ui
                                    .interact(
                                        shown.response.rect,
                                        egui::Id::unique("long_text_window_locked"),
                                        egui::Sense::click(),
                                    )
                                    .clicked()
                                {
                                    result.toast =
                                        Some(proava_domain::stlb::STLB_LOCKED.to_string());
                                }
                                return;
                            }
                            let output = if complements.is_some() {
                                self.editor.show_with_complements(
                                    ui,
                                    palette,
                                    &mut edited,
                                    &mut list,
                                    images,
                                )
                            } else {
                                self.editor.show(ui, palette, &mut edited, images)
                            };
                            if output.pick_image && self.dialog.is_none() {
                                let start = self
                                    .image_folder
                                    .clone()
                                    .or_else(|| launcher.last_folder())
                                    .unwrap_or_else(|| launcher.home_folder());
                                self.dialog = Some(FileDialog::open(
                                    launcher,
                                    "Bild einfügen",
                                    "PNG oder JPEG – mit Strg+Klick mehrere wählen.",
                                    start,
                                    &crate::strings::IMAGE_EXTENSIONS,
                                    true,
                                ));
                            }
                            if output.pick_pdf_clip && self.clip_dialog.is_none() {
                                let start = self
                                    .image_folder
                                    .clone()
                                    .or_else(|| launcher.last_folder())
                                    .unwrap_or_else(|| launcher.home_folder());
                                self.clip_dialog = Some(
                                    crate::pdf_clip_dialog::PdfClipDialog::open(launcher, start),
                                );
                            }
                            if output.changed {
                                result.changed = Some((
                                    edited,
                                    complements.as_ref().map(|_| list),
                                    output.edit_key,
                                ));
                            }
                        });
                });
        });
        if result.close && result.size.is_none() {
            result.size = self.size;
        }
        result
    }

    pub(crate) fn id() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("proava-long-text-window")
    }
}
