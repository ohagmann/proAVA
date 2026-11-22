//! WYSIWYG editor for long texts, hints and performance descriptions:
//! font, size, bold/italic/underline, alignment, lists, tables and images.
//!
//! Every paragraph (and every table cell) is its own text field whose
//! layout comes from the per-character formats in [`model::Para`]. Enter
//! splits a paragraph, Backspace at its start joins it with the one above,
//! ↑/↓ at the first/last line move into the neighbour block.
//!
//! Use: keep one [`RichEditor`] per place in the UI and call
//! [`RichEditor::ui`] (or [`RichEditor::show`] for image requests) every
//! frame with the text; it reports whether the text changed.

mod fields;
mod flow;
pub mod html;
pub mod model;

pub use fields::{FieldPresets, FieldSpot, BIDDER_BELOW_MENU, BIDDER_MENU, OWNER_MENU};

use std::collections::HashMap;

use eframe::egui::{
    self, text::LayoutJob, Color32, CornerRadius, FontId, Key, Modifiers, Rect, Sense, Stroke,
    TextFormat, Vec2,
};
use proava_domain::{
    font_class, Align, ComplementKind, FontClass, ImageAsset, RichText, TextComplement,
};

use self::model::{Doc, DocBlock, Fmt, FmtChange, Para, ParaKind, Spot, TableModel};
use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets;

/// Screen pixels per point of font size at zoom 1 (10 pt → 13.5 px).
const PX_PER_PT: f32 = 1.35;

fn text_spacing_id() -> egui::Id {
    egui::Id::unique("proava_text_spacing")
}

/// Zeilenabstand and Abstand nach Absatz of all text editors (the shell sets
/// it every frame from the settings; the printouts use the same).
pub fn set_text_spacing(ctx: &egui::Context, spacing: proava_application::TextSpacing) {
    ctx.data_mut(|data| data.insert_temp(text_spacing_id(), spacing));
}

/// The spacing set by the shell; the default (einfach, kein Abstand) when
/// nobody set it.
pub fn text_spacing(ctx: &egui::Context) -> proava_application::TextSpacing {
    ctx.data(|data| data.get_temp(text_spacing_id()))
        .unwrap_or_default()
}

/// Zeilenabstand: every section of the job gets `factor` × the line height
/// of its font (einfach = the font's own line height, as printed).
fn apply_line_spacing(ui: &egui::Ui, job: &mut LayoutJob, factor: f32) {
    if (factor - 1.0).abs() < f32::EPSILON {
        return;
    }
    for section in &mut job.sections {
        let natural = ui.fonts_mut(|fonts| fonts.row_height(&section.format.font_id));
        section.format.line_height = Some((natural * factor).round());
    }
}

/// Abstand nach Absatz in screen pixels for a paragraph in `size_pt`.
fn paragraph_gap_px(spacing: proava_application::TextSpacing, size_pt: f32, zoom: f32) -> f32 {
    (spacing.gap_pt(size_pt) * PX_PER_PT * zoom).round()
}
/// Text width the PDF prints long texts with; image and column widths in
/// mm are shown relative to it.
const TEXT_WIDTH_MM: f32 = 150.0;
pub const DEFAULT_IMAGE_WIDTH_MM: f32 = 80.0;
/// Menu entry and tooltip of the PDF clip.
pub const CLIP_MENU: &str = "Ausschnitt aus PDF einfügen …";
const MAX_IMAGE_WIDTH_MM: f32 = 180.0;
/// Sizes offered in the size list (GAEB allows these and a few more).
pub const SIZES: [f32; 12] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0,
];

/// What happened in one frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RichEditorOutput {
    pub changed: bool,
    /// The user asked for an image file (button, context menu); answer with
    /// [`RichEditor::insert_image`].
    pub pick_image: bool,
    /// „Ausschnitt aus PDF einfügen …“ (only with [`RichEditor::pdf_clips`]);
    /// answer with [`RichEditor::insert_image_sized`].
    pub pick_pdf_clip: bool,
    /// Key for grouping undo steps: the same key in a row is one step.
    pub edit_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Format(FmtChangeKind),
    Align(Align),
    Kind(ParaKind),
    InsertTable(usize, usize),
    Focus {
        spot: Spot,
        caret: usize,
    },
    TableRow {
        block: usize,
        row: usize,
        insert: bool,
    },
    TableColumn {
        block: usize,
        column: usize,
        insert: bool,
    },
    TableHeader(usize),
    /// Strg+Umschalt+B / Strg+Umschalt+G: field dialog at the caret.
    OpenField {
        owner: bool,
    },
    Move {
        block: usize,
        up: bool,
    },
    Remove(usize),
}

/// [`FmtChange`] without data, so that [`Action`] stays `Copy`; font and
/// size travel in [`RichEditor::wanted`].
#[derive(Debug, Clone, Copy, PartialEq)]
enum FmtChangeKind {
    Bold,
    Italic,
    Underline,
    Font,
    Size,
    Clear,
}

pub struct RichEditor {
    salt: egui::Id,
    doc: Doc,
    /// The text the doc was made from or last produced.
    source: Option<RichText>,
    /// Paragraph or cell the caret was in last.
    active: Spot,
    /// Selection in the active paragraph (char indices, ordered).
    selection: (usize, usize),
    /// The caret has been in the text since loading (else images go to the end).
    touched: bool,
    /// Format for the next typed text (toolbar click without selection).
    pending: Option<(Spot, usize, Fmt)>,
    /// Caret/selection to set on the next frame.
    focus: Option<(Spot, usize, usize)>,
    /// Input that came after Tab in a table cell (or before the next cell
    /// had the focus): played into the next cell, never into the old one.
    carry: Vec<egui::Event>,
    /// Pass in which `carry` was filled: the old cell already handled the
    /// keyboard input of that pass.
    carry_pass: Option<u64>,
    /// Caret on the first / last line of a paragraph in the last frame.
    edges: HashMap<Spot, (bool, bool)>,
    textures: HashMap<String, Result<egui::TextureHandle, String>>,
    /// Caret, selection and layout of the running text (paragraphs).
    flow: flow::Flow,
    wanted_font: Option<String>,
    wanted_size: Option<f32>,
    table_size: (usize, usize),
    /// Smallest height of the writing area in px (the big window uses more).
    pub min_height: f32,
    /// Zoom of the text (1 = normal).
    pub zoom: f32,
    /// Offer „Ausschnitt aus PDF einfügen …“ (long texts of the LV).
    pub pdf_clips: bool,
    /// Textergänzungen of the text (see `fields.rs`); field char `i` stands
    /// for `fields[i]`. Kept while the text is edited, so that a cut field
    /// can be pasted back.
    fields: Vec<TextComplement>,
    /// The editor was shown with complements ([`RichEditor::show_with_complements`]).
    fields_on: bool,
    /// The complements the doc was made from or last produced.
    source_fields: Option<Vec<TextComplement>>,
    field_dialog: Option<fields::FieldDialog>,
    /// A field was inserted, edited or removed (one undo step).
    field_changed: bool,
    /// Vortexte of the quick choice (from the settings).
    pub field_presets: FieldPresets,
}

impl Default for RichEditor {
    fn default() -> Self {
        Self::new("rich_editor")
    }
}

impl RichEditor {
    /// `salt` keeps the text fields of two editors apart (e.g. the panel and
    /// the big window).
    pub fn new(salt: &str) -> Self {
        Self {
            salt: egui::Id::unique(salt),
            doc: Doc::default(),
            source: None,
            active: Spot::block(0),
            selection: (0, 0),
            touched: false,
            pending: None,
            focus: None,
            carry: Vec::new(),
            carry_pass: None,
            edges: HashMap::new(),
            textures: HashMap::new(),
            flow: flow::Flow::default(),
            wanted_font: None,
            wanted_size: None,
            table_size: (3, 3),
            min_height: 150.0,
            zoom: 1.0,
            pdf_clips: false,
            fields: Vec::new(),
            fields_on: false,
            source_fields: None,
            field_dialog: None,
            field_changed: false,
            field_presets: FieldPresets::default(),
        }
    }

    /// Loads the text again on the next frame (after undo, another row …).
    pub fn reset(&mut self) {
        self.source = None;
        self.source_fields = None;
        self.field_dialog = None;
        self.active = Spot::block(0);
        self.selection = (0, 0);
        self.touched = false;
        self.pending = None;
        self.carry.clear();
        self.edges.clear();
        self.flow.reset();
    }

    /// Puts the caret at the start of the text on the next frame.
    pub fn request_focus(&mut self) {
        self.focus = Some((Spot::block(0), 0, 0));
    }

    /// Inserts an image at the caret (after the active paragraph) and returns
    /// the new text; the caller stores it.
    pub fn insert_image(&mut self, file: String) -> RichText {
        self.insert_image_sized(file, DEFAULT_IMAGE_WIDTH_MM)
    }

    /// Like [`RichEditor::insert_image`] with a width in mm (PDF clips).
    pub fn insert_image_sized(&mut self, file: String, width_mm: f32) -> RichText {
        self.drop_composition();
        if !self.touched {
            // Never clicked into the text: below everything.
            let end = self.doc.end();
            self.flow_set(end, end);
        }
        let image = DocBlock::Image {
            file,
            width_mm: Some(width_mm.clamp(10.0, MAX_IMAGE_WIDTH_MM).round()),
        };
        let at = if self.active.cell.is_some() {
            self.doc
                .insert_block(Spot::block(self.active.block), usize::MAX, image)
        } else {
            let caret = self.doc.delete_range(self.flow.anchor, self.flow.head);
            self.doc
                .insert_block(Spot::block(caret.block), caret.offset, image)
        };
        let next = (at + 1).min(self.doc.blocks.len() - 1);
        self.focus = Some((Spot::block(next), 0, 0));
        self.produce()
    }

    /// Shows the editor; true when the text changed.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        text: &mut RichText,
        images: &[ImageAsset],
    ) -> bool {
        self.show(ui, palette, text, images).changed
    }

    /// Shows the editor with toolbar; see [`RichEditorOutput`].
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        text: &mut RichText,
        images: &[ImageAsset],
    ) -> RichEditorOutput {
        self.show_inner(ui, palette, text, None, images)
    }

    /// Like [`RichEditor::show`] for a long text with Textergänzungen: they
    /// stand as fields in the text (Bieterangaben, Angaben des
    /// Auftraggebers); `changed` also covers new anchors and fields.
    pub fn show_with_complements(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        text: &mut RichText,
        complements: &mut Vec<TextComplement>,
        images: &[ImageAsset],
    ) -> RichEditorOutput {
        self.show_inner(ui, palette, text, Some(complements), images)
    }

    /// The complements last handed out (after [`RichEditor::insert_image`]
    /// the anchors may have moved); None for editors without fields.
    pub fn complements(&self) -> Option<Vec<TextComplement>> {
        self.fields_on
            .then(|| self.source_fields.clone().unwrap_or_default())
    }

    fn show_inner(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        text: &mut RichText,
        complements: Option<&mut Vec<TextComplement>>,
        images: &[ImageAsset],
    ) -> RichEditorOutput {
        let with_fields = complements.is_some();
        let fields_differ = match complements.as_deref() {
            Some(list) => self.source_fields.as_ref() != Some(list),
            None => self.fields_on,
        };
        if self.source.as_ref() != Some(text) || fields_differ {
            self.fields_on = with_fields;
            self.fields = complements.as_deref().cloned().unwrap_or_default();
            self.source_fields = complements.as_deref().cloned();
            let shown = if with_fields {
                proava_domain::complement_text::embed(text, &self.fields)
            } else {
                text.clone()
            };
            self.doc = Doc::from_rich(&shown);
            self.source = Some(text.clone());
            // Changed from outside (undo …): a pre-edit text is gone with it.
            self.flow.composing = None;
            if self.doc.para(self.active).is_none() {
                self.active = Spot::block(0);
                self.selection = (0, 0);
            }
        }
        let mut output = RichEditorOutput::default();
        let mut actions: Vec<Action> = Vec::new();
        if !self.fields_on {
            self.field_dialog = None;
        }

        self.toolbar(ui, palette, &mut actions, &mut output);
        ui.add_space(4.0);

        egui::Frame::new()
            .fill(palette.input)
            .stroke(Stroke::new(1.0, palette.line))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(self.min_height);
                ui.spacing_mut().item_spacing.y = 2.0;
                self.flow_begin(ui);
                let typed = self.flow_input(ui, &mut actions, &mut output);
                self.flow_layouts_reset();
                let changed =
                    self.blocks_ui(ui, palette, images, &mut actions, &mut output) || typed;
                if changed {
                    output.changed = true;
                }
                // Free space below the text: a click puts the caret at the end.
                let rest = ui.available_size_before_wrap();
                let height = rest.y.max(18.0);
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(rest.x, height), Sense::click());
                if self.doc.is_empty() {
                    ui.painter().text(
                        rect.left_top() + Vec2::new(2.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        "Bild hierher ziehen, mit Strg+V einfügen oder über „Bild einfügen“",
                        FontId::proportional(12.0),
                        palette.text_dim,
                    );
                }
                if response.clicked() {
                    let end = self.doc.end();
                    actions.push(Action::Focus {
                        spot: Spot::block(end.block),
                        caret: end.offset,
                    });
                }
                response.context_menu(|ui| {
                    if ui.button("Bild einfügen …").clicked() {
                        output.pick_image = true;
                        ui.close();
                    }
                    if self.pdf_clips && ui.button(CLIP_MENU).clicked() {
                        output.pick_pdf_clip = true;
                        ui.close();
                    }
                    if self.fields_on {
                        ui.separator();
                        crate::menus::menu(ui, BIDDER_BELOW_MENU, |ui| {
                            self.field_menu_items(ui, FieldSpot::End);
                        });
                    }
                });
                self.flow_end(ui);
            });
        self.field_dialog_ui(ui, palette);

        // Every structural change is an undo step of its own.
        let structural = std::mem::take(&mut self.field_changed)
            || actions
                .iter()
                .any(|action| !matches!(action, Action::Focus { .. } | Action::OpenField { .. }));
        if structural && self.drop_composition() {
            ui.ctx().memory_mut(|memory| memory.interrupt_ime());
        }
        for action in actions {
            self.run(action);
        }
        if structural {
            output.edit_key = None;
        }
        if structural || output.changed || self.field_changed {
            self.field_changed = false;
            let produced = self.produce();
            if produced != *text {
                *text = produced;
                output.changed = true;
            }
            if let (Some(list), Some(produced)) = (complements, &self.source_fields) {
                if list != produced {
                    *list = produced.clone();
                    output.changed = true;
                }
            }
        }
        output
    }

    /// Text from the doc, remembered as the source; with fields the
    /// complements get their new anchors (`source_fields`).
    fn produce(&mut self) -> RichText {
        let rich = self.committed_doc().to_rich();
        let text = if self.fields_on {
            let (text, complements) = proava_domain::complement_text::extract(&rich, &self.fields);
            self.source_fields = Some(complements);
            text
        } else {
            let (text, _) = proava_domain::complement_text::extract(&rich, &[]);
            text
        };
        self.source = Some(text.clone());
        text
    }

    fn id(&self, spot: Spot) -> egui::Id {
        self.salt.with(("para", spot.block, spot.cell))
    }

    // ----- toolbar -----

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) {
        let shown = self.current_fmt();
        let para_align = self
            .doc
            .para(self.active)
            .map(|p| p.align)
            .unwrap_or_default();
        let para_kind = self
            .doc
            .para(self.active)
            .map(|p| p.kind)
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // Font family.
            let class = font_class(shown.font.as_deref());
            egui::ComboBox::from_id_salt(self.salt.with("font"))
                .width(150.0)
                .selected_text(class.label())
                .show_ui(ui, |ui| {
                    for choice in FontClass::ALL {
                        if ui
                            .selectable_label(choice == class, choice.label())
                            .clicked()
                        {
                            self.wanted_font = choice.stored_name().map(str::to_string);
                            actions.push(Action::Format(FmtChangeKind::Font));
                        }
                    }
                });
            // Size: list and free entry.
            let mut size = shown.size_pt.unwrap_or(proava_domain::DEFAULT_SIZE_PT);
            egui::ComboBox::from_id_salt(self.salt.with("size"))
                .width(48.0)
                .selected_text(format_size(size))
                .show_ui(ui, |ui| {
                    for choice in SIZES {
                        if ui
                            .selectable_label((choice - size).abs() < 0.05, format_size(choice))
                            .clicked()
                        {
                            self.wanted_size = Some(choice);
                            actions.push(Action::Format(FmtChangeKind::Size));
                        }
                    }
                });
            let response = ui
                .add(
                    egui::DragValue::new(&mut size)
                        .range(4.0..=72.0)
                        .speed(0.25)
                        .max_decimals(1)
                        .suffix(" pt"),
                )
                .on_hover_text("Schriftgröße in Punkt");
            if response.changed() {
                self.wanted_size = Some(size);
                actions.push(Action::Format(FmtChangeKind::Size));
            }
            ui.separator();
            for (label, hint, kind, on) in [
                ("F", "Fett (Strg+B)", FmtChangeKind::Bold, shown.bold),
                ("K", "Kursiv (Strg+I)", FmtChangeKind::Italic, shown.italic),
                ("U", "Unterstrichen (Strg+U)", FmtChangeKind::Underline, shown.underline),
            ] {
                let mut job = LayoutJob::default();
                job.append(
                    label,
                    0.0,
                    TextFormat {
                        font_id: crate::fonts::styled(
                            14.0,
                            kind == FmtChangeKind::Bold,
                            kind == FmtChangeKind::Italic,
                        ),
                        color: palette.text,
                        underline: if kind == FmtChangeKind::Underline {
                            Stroke::new(1.0, palette.text)
                        } else {
                            Stroke::NONE
                        },
                        ..TextFormat::default()
                    },
                );
                if ui
                    .add(egui::Button::new(job).selected(on).min_size(Vec2::new(28.0, 26.0)))
                    .on_hover_text(hint)
                    .clicked()
                {
                    actions.push(Action::Format(kind));
                }
            }
            if ui
                .add(egui::Button::new("normal").min_size(Vec2::new(0.0, 26.0)))
                .on_hover_text("Zeichenformat entfernen: Standardschrift, 10 pt, nicht fett/kursiv/unterstrichen")
                .clicked()
            {
                actions.push(Action::Format(FmtChangeKind::Clear));
            }
            ui.separator();
            for (align, icon, hint) in [
                (Align::Left, Icon::AlignLeft, "Linksbündig"),
                (Align::Center, Icon::AlignCenter, "Zentriert"),
                (Align::Right, Icon::AlignRight, "Rechtsbündig"),
                (Align::Justify, Icon::AlignJustify, "Blocksatz"),
            ] {
                if widgets::tool_button(ui, palette, icon, para_align == align, hint).clicked() {
                    actions.push(Action::Align(align));
                }
            }
            ui.separator();
            for (icon, hint, kind) in [
                (
                    Icon::Bullets,
                    "Aufzählung (noch einmal: normaler Absatz; Tab/Umschalt+Tab: Unterpunkt ein/aus)",
                    ParaKind::Bullet,
                ),
                (
                    Icon::Numbering,
                    "Nummerierung (noch einmal: normaler Absatz; Tab/Umschalt+Tab: Unterpunkt ein/aus)",
                    ParaKind::Numbered,
                ),
            ] {
                if widgets::tool_button(ui, palette, icon, para_kind == kind, hint).clicked() {
                    actions.push(Action::Kind(kind));
                }
            }
            ui.separator();
            let table_button =
                widgets::tool_button(ui, palette, Icon::Table, false, "Tabelle einfügen …")
                    .on_hover_text("Zeilen × Spalten wählen");
            egui::Popup::menu(&table_button).show(|ui| {
                ui.label("Tabelle einfügen");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.table_size.0).range(1..=50));
                    ui.label("Zeilen ×");
                    ui.add(egui::DragValue::new(&mut self.table_size.1).range(1..=12));
                    ui.label("Spalten");
                });
                if ui.button("Einfügen").clicked() {
                    actions.push(Action::InsertTable(self.table_size.0, self.table_size.1));
                    ui.close();
                }
            });
            if widgets::tool_button(ui, palette, Icon::Image, false, "Bild einfügen …")
                .on_hover_text("PNG/JPEG – auch per Ziehen einer Datei oder Strg+V")
                .clicked()
            {
                output.pick_image = true;
            }
            if self.pdf_clips
                && widgets::tool_button(ui, palette, Icon::Document, false, CLIP_MENU)
                    .on_hover_text("Teil einer PDF-Seite, z. B. der Systemtopologie – im Druck als Vektorgrafik")
                    .clicked()
            {
                output.pick_pdf_clip = true;
            }
            if self.fields_on {
                ui.separator();
                let field_button = widgets::tool_button(
                    ui,
                    palette,
                    Icon::Field,
                    false,
                    "Bieterangabe einfügen … (Strg+Umschalt+B)",
                )
                .on_hover_text("An der Schreibmarke, z. B. „Fabrikat: …………“ – der Bieter trägt sie im Angebot ein");
                egui::Popup::menu(&field_button).show(|ui| {
                    ui.label(egui::RichText::new("An der Schreibmarke").color(palette.text_dim));
                    self.field_menu_items(ui, FieldSpot::Caret);
                });
                let below_button = widgets::tool_button(
                    ui,
                    palette,
                    Icon::FieldBelow,
                    false,
                    BIDDER_BELOW_MENU,
                )
                .on_hover_text("Als eigener Absatz am Ende des Langtexts");
                egui::Popup::menu(&below_button).show(|ui| {
                    ui.label(egui::RichText::new("Unter dem Langtext").color(palette.text_dim));
                    self.field_menu_items(ui, FieldSpot::End);
                });
            }
        });
    }

    /// Format at the caret or of the selection of the active paragraph.
    fn current_fmt(&self) -> Fmt {
        if self.active.cell.is_none() {
            let head = self.flow.head;
            if let Some((spot, caret, fmt)) = &self.pending {
                if *spot == Spot::block(head.block) && *caret == head.offset {
                    return fmt.clone();
                }
            }
            return self.doc.fmt_of_selection(self.flow.anchor, head);
        }
        if let Some((spot, caret, fmt)) = &self.pending {
            if *spot == self.active && *caret == self.selection.1 {
                return fmt.clone();
            }
        }
        self.doc
            .para(self.active)
            .map(|para| para.fmt_of_range(self.selection.0, self.selection.1))
            .unwrap_or_default()
    }

    // ----- actions -----

    fn change_for(&self, kind: FmtChangeKind) -> FmtChange {
        match kind {
            FmtChangeKind::Bold => FmtChange::ToggleBold,
            FmtChangeKind::Italic => FmtChange::ToggleItalic,
            FmtChangeKind::Underline => FmtChange::ToggleUnderline,
            FmtChangeKind::Font => FmtChange::Font(self.wanted_font.clone()),
            FmtChangeKind::Size => FmtChange::Size(
                self.wanted_size
                    .filter(|size| (size - proava_domain::DEFAULT_SIZE_PT).abs() > 0.05),
            ),
            FmtChangeKind::Clear => FmtChange::Clear,
        }
    }

    fn run(&mut self, action: Action) {
        let active = self.active;
        let (start, end) = self.selection;
        if active.cell.is_none() && self.run_flow(action) {
            self.edges.clear();
            return;
        }
        match action {
            Action::Format(kind) => {
                let change = self.change_for(kind);
                if start == end {
                    let base = self.current_fmt();
                    self.pending = Some((active, end, Para::changed_fmt(&base, &change)));
                    // An empty paragraph takes the format right away.
                    if let Some(para) = self.doc.para_mut(active) {
                        if para.is_empty() {
                            para.empty_fmt = Para::changed_fmt(&para.empty_fmt, &change);
                        }
                    }
                } else if let Some(para) = self.doc.para_mut(active) {
                    para.format_range(start, end, &change);
                }
                self.focus = Some((active, start, end));
            }
            Action::Align(align) => {
                if let Some(para) = self.doc.para_mut(active) {
                    para.align = align;
                }
                self.focus = Some((active, start, end));
            }
            Action::Kind(kind) => {
                if active.cell.is_none() {
                    if let Some(para) = self.doc.para_mut(active) {
                        para.kind = if para.kind == kind {
                            ParaKind::Text
                        } else {
                            kind
                        };
                    }
                }
                self.focus = Some((active, start, end));
            }
            Action::InsertTable(rows, columns) => {
                let spot = Spot::block(active.block);
                let caret = if active.cell.is_none() {
                    end
                } else {
                    usize::MAX
                };
                let at = self.doc.insert_block(
                    spot,
                    caret,
                    DocBlock::Table(TableModel::new(rows, columns)),
                );
                self.focus = Some((
                    Spot {
                        block: at,
                        cell: Some((0, 0)),
                    },
                    0,
                    0,
                ));
            }
            Action::Focus { spot, caret } => self.focus = Some((spot, caret, caret)),
            Action::TableRow { block, row, insert } => {
                if let Some(DocBlock::Table(table)) = self.doc.blocks.get_mut(block) {
                    if insert {
                        table.insert_row(row);
                    } else {
                        table.remove_row(row);
                    }
                }
            }
            Action::TableColumn {
                block,
                column,
                insert,
            } => {
                if let Some(DocBlock::Table(table)) = self.doc.blocks.get_mut(block) {
                    if insert {
                        table.insert_column(column);
                    } else {
                        table.remove_column(column);
                    }
                }
            }
            Action::TableHeader(block) => {
                if let Some(DocBlock::Table(table)) = self.doc.blocks.get_mut(block) {
                    table.header_rows = usize::from(table.header_rows == 0);
                }
            }
            Action::OpenField { owner } => {
                let kind = if owner {
                    ComplementKind::Owner
                } else {
                    ComplementKind::Bidder
                };
                self.open_new_field(kind, FieldSpot::Caret, "");
            }
            Action::Move { block, up } => {
                self.doc.move_block(block, up);
            }
            Action::Remove(block) => {
                self.doc.remove_block(block);
                self.active = Spot::block(block.saturating_sub(1).min(self.doc.blocks.len() - 1));
                self.selection = (0, 0);
            }
        }
        // Spots may have moved; drop stale caret edges.
        self.edges.clear();
    }

    /// Toolbar actions on the running text; false = not a text action.
    fn run_flow(&mut self, action: Action) -> bool {
        let (anchor, head) = (self.flow.anchor, self.flow.head);
        match action {
            Action::Format(kind) => {
                let change = self.change_for(kind);
                if anchor == head {
                    let base = self.current_fmt();
                    let fmt = Para::changed_fmt(&base, &change);
                    self.pending = Some((Spot::block(head.block), head.offset, fmt));
                    if let Some(para) = self.doc.para_mut(Spot::block(head.block)) {
                        if para.is_empty() {
                            para.empty_fmt = Para::changed_fmt(&para.empty_fmt, &change);
                        }
                    }
                } else {
                    self.doc.format_selection(anchor, head, &change);
                }
            }
            Action::Align(align) => self.doc.set_align(anchor, head, align),
            Action::Kind(kind) => self.doc.toggle_kind(anchor, head, kind),
            Action::InsertTable(rows, columns) => {
                let caret = self.doc.delete_range(anchor, head);
                let at = self.doc.insert_block(
                    Spot::block(caret.block),
                    caret.offset,
                    DocBlock::Table(TableModel::new(rows, columns)),
                );
                self.focus = Some((
                    Spot {
                        block: at,
                        cell: Some((0, 0)),
                    },
                    0,
                    0,
                ));
                return true;
            }
            _ => return false,
        }
        self.flow.want_focus = true;
        true
    }

    /// Last paragraph position inside `block` (end of text / last cell).
    fn end_of(&self, block: usize) -> Option<(Spot, usize)> {
        match self.doc.blocks.get(block)? {
            DocBlock::Para(para) => Some((Spot::block(block), para.len())),
            DocBlock::Table(table) => {
                let row = table.rows.len().checked_sub(1)?;
                let column = table.rows[row].len().checked_sub(1)?;
                let len = table.rows[row][column].len();
                Some((
                    Spot {
                        block,
                        cell: Some((row, column)),
                    },
                    len,
                ))
            }
            DocBlock::Image { .. } => self.end_of(block.checked_sub(1)?),
        }
    }

    fn start_of(&self, block: usize) -> Option<(Spot, usize)> {
        match self.doc.blocks.get(block)? {
            DocBlock::Para(_) => Some((Spot::block(block), 0)),
            DocBlock::Table(_) => Some((
                Spot {
                    block,
                    cell: Some((0, 0)),
                },
                0,
            )),
            DocBlock::Image { .. } => self.start_of(block + 1),
        }
    }

    // ----- blocks -----

    fn blocks_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        images: &[ImageAsset],
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let mut changed = false;
        let width = ui.available_width();
        let px_per_mm = (width / TEXT_WIDTH_MM).max(0.5);
        for block in 0..self.doc.blocks.len() {
            // Paragraphs leave no gap of their own (see flow_para_ui): a
            // table or image keeps its distance to the text above.
            if block > 0
                && !matches!(self.doc.blocks[block], DocBlock::Para(_))
                && matches!(self.doc.blocks[block - 1], DocBlock::Para(_))
            {
                ui.add_space(ui.spacing().item_spacing.y);
            }
            let top = ui.cursor().top();
            match &self.doc.blocks[block] {
                DocBlock::Para(_) => {
                    self.flow_para_ui(ui, palette, block, width, actions, output);
                    continue;
                }
                DocBlock::Table(_) => {
                    changed |= self.table_ui(ui, palette, block, px_per_mm, actions, output);
                }
                DocBlock::Image { .. } => {
                    changed |=
                        self.image_ui(ui, palette, block, images, px_per_mm, actions, output);
                }
            }
            let rect = Rect::from_min_max(
                egui::pos2(ui.max_rect().left(), top),
                egui::pos2(ui.max_rect().right(), ui.cursor().top()),
            );
            self.flow_mark_block(ui, block, rect);
        }
        changed
    }

    /// One table cell as text field. Returns true on a text change.
    fn para_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        spot: Spot,
        width: f32,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let id = self.id(spot);
        let ctx = ui.ctx().clone();
        let Some(para) = self.doc.para(spot).cloned() else {
            return false;
        };

        // Focus requests from the last actions.
        if let Some((target, start, end)) = self.focus {
            if target == spot {
                let mut state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
                let (start, end) = (start.min(para.len()), end.min(para.len()));
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(start),
                        egui::text::CCursor::new(end),
                    )));
                state.store(&ctx, id);
                ctx.memory_mut(|memory| memory.request_focus(id));
                self.focus = None;
                self.active = spot;
                self.selection = (start, end);
                let same_pass = self.carry_pass.take() == Some(ctx.cumulative_pass_nr());
                if same_pass || !self.carry.is_empty() {
                    let mut carried = std::mem::take(&mut self.carry);
                    ui.input_mut(|input| {
                        if same_pass {
                            // Typed before the Tab: the old cell has it.
                            input.events.retain(|event| !is_keyboard_input(event));
                        }
                        carried.append(&mut input.events);
                        input.events = carried;
                    });
                }
            }
        }

        // Keys the text field must not see.
        let focused = ctx.memory(|memory| memory.has_focus(id));
        if focused {
            let state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
            let (start, end) = state
                .cursor
                .char_range()
                .map(|range| {
                    let (a, b) = (range.primary.index.0, range.secondary.index.0);
                    (a.min(b), a.max(b))
                })
                .unwrap_or((para.len(), para.len()));
            let (top, bottom) = self.edges.get(&spot).copied().unwrap_or((false, false));
            let pass = ctx.cumulative_pass_nr();
            ui.input_mut(|input| {
                for (key, owner) in [(Key::B, false), (Key::G, true)] {
                    if input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, key) {
                        self.active = spot;
                        actions.push(Action::OpenField { owner });
                    }
                }
                for (key, kind) in [
                    (Key::B, FmtChangeKind::Bold),
                    (Key::I, FmtChangeKind::Italic),
                    (Key::U, FmtChangeKind::Underline),
                ] {
                    if input.consume_key(Modifiers::COMMAND, key) {
                        self.active = spot;
                        self.selection = (start, end);
                        actions.push(Action::Format(kind));
                    }
                }
                // Set at once: a block further down takes the focus in this
                // frame already, so that fast typing does not land here.
                if top && start == end && input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    if let Some((target, caret)) = self.neighbour(spot, true) {
                        self.focus = Some((target, caret, caret));
                    }
                }
                if bottom && start == end && input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    if let Some((target, caret)) = self.neighbour(spot, false) {
                        self.focus = Some((target, caret, caret));
                    }
                }
                // Tab/Umschalt+Tab: next/previous cell (whole content
                // selected, as in a word processor); Tab in the last cell
                // adds a row. The text field never leaves the table by Tab,
                // so typing on cannot land in the LV table. What comes after
                // the Tab goes to the next cell.
                let moving_away = self
                    .focus
                    .is_some_and(|(target, _, _)| target != spot && target.cell.is_some());
                if let Some(cell) = spot.cell.filter(|_| !moving_away) {
                    let tab = input.events.iter().position(|event| {
                        matches!(event, egui::Event::Key { key: Key::Tab, pressed: true, modifiers, .. }
                            if !modifiers.command && !modifiers.alt)
                    });
                    if let Some(at) = tab {
                        let back = matches!(&input.events[at], egui::Event::Key { modifiers, .. } if modifiers.shift);
                        let mut rest: Vec<egui::Event> = input.events.drain(at..).collect();
                        rest.remove(0);
                        self.carry.extend(rest);
                        self.carry_pass = Some(pass);
                        if let Some(DocBlock::Table(table)) = self.doc.blocks.get(spot.block) {
                            let target = table.tab_target(cell, back);
                            let rows = table.rows.len();
                            match target {
                                Some(target) => {
                                    let spot = Spot {
                                        block: spot.block,
                                        cell: Some(target),
                                    };
                                    let len = self.doc.para(spot).map_or(0, Para::len);
                                    self.focus = Some((spot, 0, len));
                                }
                                None if !back => {
                                    actions.push(Action::TableRow {
                                        block: spot.block,
                                        row: rows,
                                        insert: true,
                                    });
                                    actions.push(Action::Focus {
                                        spot: Spot {
                                            block: spot.block,
                                            cell: Some((rows, 0)),
                                        },
                                        caret: 0,
                                    });
                                }
                                None => self.carry.clear(),
                            }
                        }
                    }
                } else if moving_away {
                    // The next cell takes the focus later in this or the
                    // next frame: its input waits for it.
                    self.carry.append(&mut input.events);
                    self.carry_pass = Some(pass);
                }
            });
        }

        let pending = self
            .pending
            .as_ref()
            .filter(|(pending_spot, _, _)| *pending_spot == spot)
            .map(|(_, _, fmt)| fmt.clone());
        let zoom = self.zoom;
        let text_color = palette.text;
        let line_factor = text_spacing(&ctx).line.factor();
        let layout_para = para.clone();
        let layout_pending = pending.clone();
        let mut layouter = move |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
            let mut shown = layout_para.clone();
            if buffer.as_str() != shown.text {
                shown.apply_edit(buffer.as_str(), layout_pending.as_ref());
            }
            let mut job = layout_job(&shown, wrap, zoom, text_color);
            apply_line_spacing(ui, &mut job, line_factor);
            with_trailing_spaces(ui.fonts_mut(|fonts| fonts.layout_job(job)))
        };

        let mut buffer = para.text.clone();
        let marker = match para.kind {
            ParaKind::Text => None,
            ParaKind::Bullet | ParaKind::Numbered => self.doc.marker_of(spot.block),
        };
        let field_width = if marker.is_some() {
            width - 22.0
        } else {
            width
        };
        let edit_output = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if let Some(marker) = &marker {
                    let size = para.fmt_at(0).size_pt.unwrap_or(10.0) * PX_PER_PT * zoom;
                    ui.add_sized(
                        [18.0, size * 1.3],
                        egui::Label::new(
                            egui::RichText::new(marker).size(size).color(palette.accent),
                        ),
                    );
                }
                egui::TextEdit::multiline(&mut buffer)
                    .id(id)
                    // Tab is ours in table cells (next cell), see above.
                    .lock_focus(spot.cell.is_some())
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::symmetric(2, 1))
                    .desired_width(field_width)
                    .desired_rows(1)
                    .align(egui::Align2([egui_align(para.align), egui::Align::TOP]))
                    .layouter(&mut layouter)
                    .show(ui)
            })
            .inner;

        let response = &edit_output.response;
        let mut changed = false;
        if response.has_focus() {
            self.active = spot;
            self.touched = true;
            if let Some(range) = edit_output.cursor_range {
                let (a, b) = (range.primary.index.0, range.secondary.index.0);
                let selection = (a.min(b), a.max(b));
                if selection != self.selection
                    && self
                        .pending
                        .as_ref()
                        .is_some_and(|(_, caret, _)| *caret != selection.1)
                {
                    self.pending = None;
                }
                self.selection = selection;
                // First/last line for ↑/↓ into the neighbour block.
                let galley = &edit_output.galley;
                let caret_rect = galley.pos_from_cursor(range.primary);
                let first = galley.rows.first().map(|row| row.rect());
                let last = galley.rows.last().map(|row| row.rect());
                let top = first.is_none_or(|row| caret_rect.center().y <= row.max.y);
                let bottom = last.is_none_or(|row| caret_rect.center().y >= row.min.y);
                self.edges.insert(spot, (top, bottom));
            }
        }
        if response.changed() && buffer != para.text {
            let typed = pending.as_ref();
            if let Some(target) = self.doc.para_mut(spot) {
                target.apply_edit(&buffer, typed);
            }
            changed = true;
            output.edit_key = Some(format!("rt:{}:{:?}", spot.block, spot.cell));
        }
        response.context_menu(|ui| {
            if ui.button("Bild einfügen …").clicked() {
                output.pick_image = true;
                ui.close();
            }
        });
        changed
    }

    /// Block/cell above or below `spot` and the caret there.
    fn neighbour(&self, spot: Spot, up: bool) -> Option<(Spot, usize)> {
        if let Some((row, column)) = spot.cell {
            if let Some(DocBlock::Table(table)) = self.doc.blocks.get(spot.block) {
                if up && row > 0 {
                    let len = table.rows[row - 1].get(column).map_or(0, Para::len);
                    return Some((
                        Spot {
                            block: spot.block,
                            cell: Some((row - 1, column)),
                        },
                        len,
                    ));
                }
                if !up && row + 1 < table.rows.len() {
                    return Some((
                        Spot {
                            block: spot.block,
                            cell: Some((row + 1, column)),
                        },
                        0,
                    ));
                }
            }
        }
        if up {
            let mut block = spot.block.checked_sub(1)?;
            while matches!(self.doc.blocks.get(block), Some(DocBlock::Image { .. })) {
                block = block.checked_sub(1)?;
            }
            self.end_of(block)
        } else {
            self.start_of(spot.block + 1)
        }
    }

    fn table_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        block: usize,
        px_per_mm: f32,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let Some(DocBlock::Table(table)) = self.doc.blocks.get(block) else {
            return false;
        };
        let rows = table.rows.len();
        let columns = table.columns().max(1);
        let header_rows = table.header_rows;
        let available = ui.available_width() - 8.0;
        // Column widths in px: fixed ones from mm, the rest share the remainder.
        let fixed: Vec<Option<f32>> = (0..columns)
            .map(|column| {
                table
                    .widths_mm
                    .get(column)
                    .copied()
                    .flatten()
                    .map(|mm| mm * px_per_mm)
            })
            .collect();
        let used: f32 = fixed.iter().flatten().sum();
        let free = fixed.iter().filter(|width| width.is_none()).count();
        let share = if free > 0 {
            ((available - used) / free as f32).max(40.0)
        } else {
            0.0
        };
        let widths: Vec<f32> = fixed
            .iter()
            .map(|width| width.unwrap_or(share).max(24.0))
            .collect();
        let mut changed = false;
        let active_cell = (self.active.block == block)
            .then_some(self.active.cell)
            .flatten();

        ui.add_space(4.0);
        egui::ScrollArea::horizontal()
            .id_salt(self.salt.with(("table", block)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for row in 0..rows {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (column, width) in widths.iter().enumerate() {
                            let spot = Spot {
                                block,
                                cell: Some((row, column)),
                            };
                            let fill = if row < header_rows {
                                palette.accent_soft
                            } else {
                                palette.panel
                            };
                            let stroke = if active_cell == Some((row, column)) {
                                Stroke::new(1.5, palette.accent)
                            } else {
                                Stroke::new(1.0, palette.line)
                            };
                            let frame = egui::Frame::new()
                                .fill(fill)
                                .stroke(stroke)
                                .inner_margin(egui::Margin::symmetric(4, 3))
                                .show(ui, |ui| {
                                    ui.set_width(width - 9.0);
                                    self.para_ui(ui, palette, spot, width - 9.0, actions, output)
                                });
                            changed |= frame.inner;
                            // Drag handle on the right edge of the head cells.
                            if row == 0 {
                                let rect = frame.response.rect;
                                let handle = Rect::from_min_max(
                                    egui::pos2(rect.right() - 3.0, rect.top()),
                                    egui::pos2(rect.right() + 3.0, rect.bottom()),
                                );
                                let response = ui
                                    .interact(
                                        handle,
                                        self.salt.with(("col", block, column)),
                                        Sense::click_and_drag(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                                    .on_hover_text(
                                        "Spaltenbreite ziehen – Doppelklick: automatisch",
                                    );
                                if response.dragged() {
                                    let delta = response.drag_delta().x / px_per_mm;
                                    let mm = (width / px_per_mm + delta).clamp(5.0, 180.0);
                                    if let Some(DocBlock::Table(table)) =
                                        self.doc.blocks.get_mut(block)
                                    {
                                        table.set_width(column, Some(mm));
                                    }
                                    changed = true;
                                    output.edit_key = Some(format!("rt-col:{block}:{column}"));
                                }
                                if response.double_clicked() {
                                    if let Some(DocBlock::Table(table)) =
                                        self.doc.blocks.get_mut(block)
                                    {
                                        table.set_width(column, None);
                                    }
                                    changed = true;
                                }
                            }
                        }
                    });
                }
            });
        ui.add_space(3.0);
        let (row, column) = active_cell.unwrap_or((rows - 1, columns - 1));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let small = |text: &str| egui::Button::new(egui::RichText::new(text).size(12.0));
            if ui
                .add(small("+ Zeile"))
                .on_hover_text("Zeile unter der aktiven Zelle")
                .clicked()
            {
                actions.push(Action::TableRow {
                    block,
                    row: row + 1,
                    insert: true,
                });
            }
            if ui
                .add(small("− Zeile"))
                .on_hover_text("Aktive Zeile löschen")
                .clicked()
            {
                actions.push(Action::TableRow {
                    block,
                    row,
                    insert: false,
                });
            }
            if ui
                .add(small("+ Spalte"))
                .on_hover_text("Spalte rechts der aktiven Zelle")
                .clicked()
            {
                actions.push(Action::TableColumn {
                    block,
                    column: column + 1,
                    insert: true,
                });
            }
            if ui
                .add(small("− Spalte"))
                .on_hover_text("Aktive Spalte löschen")
                .clicked()
            {
                actions.push(Action::TableColumn {
                    block,
                    column,
                    insert: false,
                });
            }
            if ui
                .add(small("Kopfzeile").selected(header_rows > 0))
                .on_hover_text("Erste Zeile als Tabellenkopf (fett, hinterlegt, wiederholt)")
                .clicked()
            {
                actions.push(Action::TableHeader(block));
            }
            block_buttons(ui, palette, block, actions);
        });
        ui.add_space(4.0);
        changed
    }

    #[allow(clippy::too_many_arguments)]
    fn image_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        block: usize,
        images: &[ImageAsset],
        px_per_mm: f32,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let Some(DocBlock::Image { file, width_mm }) = self.doc.blocks.get(block).cloned() else {
            return false;
        };
        if let Some(asset) = images
            .iter()
            .find(|image| image.file == file && image.is_pdf_clip())
        {
            return self.clip_ui(
                ui, palette, block, asset, width_mm, px_per_mm, actions, output,
            );
        }
        let ctx = ui.ctx().clone();
        // A missing image may come back under the same name (added again).
        if matches!(self.textures.get(&file), Some(Err(error)) if *error == missing_image(&file))
            && images.iter().any(|image| image.file == file)
        {
            self.textures.remove(&file);
        }
        let texture = self
            .textures
            .entry(file.clone())
            .or_insert_with(|| load_texture(&ctx, &file, images))
            .clone();
        let mm = width_mm.unwrap_or(DEFAULT_IMAGE_WIDTH_MM);
        let mut new_mm = mm;
        ui.add_space(4.0);
        match texture {
            Ok(handle) => {
                let size = handle.size_vec2();
                let width = (mm * px_per_mm).clamp(32.0, ui.available_width());
                let height = width * size.y / size.x.max(1.0);
                let response = ui.add(
                    egui::Image::new((handle.id(), Vec2::new(width, height))).sense(Sense::click()),
                );
                let rect = response.rect;
                // Corner grip: drag to change the width.
                let grip = Rect::from_center_size(rect.right_bottom(), Vec2::splat(14.0));
                let grip_response = ui
                    .interact(grip, self.salt.with(("img", block)), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
                    .on_hover_text("Ziehen ändert die Bildbreite");
                ui.painter()
                    .rect_filled(grip.shrink(3.0), 2.0, palette.accent);
                if grip_response.dragged() {
                    new_mm = ((rect.width() + grip_response.drag_delta().x) / px_per_mm)
                        .clamp(10.0, MAX_IMAGE_WIDTH_MM);
                }
                response.context_menu(|ui| {
                    if ui.button("Bild einfügen …").clicked() {
                        output.pick_image = true;
                        ui.close();
                    }
                    if ui.button("Bild entfernen").clicked() {
                        actions.push(Action::Remove(block));
                        ui.close();
                    }
                });
            }
            Err(error) => {
                ui.label(egui::RichText::new(error).size(12.5).color(palette.error));
            }
        }
        crate::widgets::row_wrapped(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                egui::RichText::new(&file)
                    .size(12.0)
                    .color(palette.text_dim),
            );
            ui.label(
                egui::RichText::new("Breite")
                    .size(12.0)
                    .color(palette.text_dim),
            );
            let mut value = new_mm;
            if ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(10.0..=MAX_IMAGE_WIDTH_MM)
                        .speed(1.0)
                        .max_decimals(0)
                        .suffix(" mm"),
                )
                .changed()
            {
                new_mm = value;
            }
            block_buttons(ui, palette, block, actions);
        });
        ui.add_space(4.0);
        if (new_mm - mm).abs() > 0.05 {
            if let Some(DocBlock::Image { width_mm, .. }) = self.doc.blocks.get_mut(block) {
                *width_mm = Some(new_mm.round());
            }
            output.edit_key = Some(format!("rt-img:{block}"));
            return true;
        }
        false
    }
}

impl RichEditor {
    /// A PDF clip: the preview rendered once in the background (or frozen,
    /// or only a placeholder), width grip, preview switches.
    #[allow(clippy::too_many_arguments)]
    fn clip_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        block: usize,
        asset: &ImageAsset,
        width_mm: Option<f32>,
        px_per_mm: f32,
        actions: &mut Vec<Action>,
        output: &mut RichEditorOutput,
    ) -> bool {
        let Some(clip) = asset.clip.as_ref() else {
            return false;
        };
        let previews = crate::pdf_preview::ClipPreviews::of(ui.ctx());
        let all_hidden = previews.all_hidden();
        let hidden = clip.preview_hidden || all_hidden;
        let mm = width_mm.unwrap_or(proava_application::pdf_clip::DEFAULT_CLIP_WIDTH_MM);
        let mut new_mm = mm;
        let width = (mm * px_per_mm).clamp(32.0, ui.available_width().max(32.0));
        let height = width * clip.rect.aspect();
        let freezing = previews.is_freezing(&asset.file);
        ui.add_space(4.0);

        let response = if hidden {
            // Placeholder: name and size, nothing is rendered.
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, height.clamp(40.0, 64.0)), Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, palette.panel);
            painter.rect_stroke(
                rect,
                4.0,
                Stroke::new(1.0, palette.line),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.left_top() + Vec2::new(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                format!("PDF-Ausschnitt {}", asset.file),
                FontId::proportional(13.0),
                palette.text,
            );
            painter.text(
                rect.left_top() + Vec2::new(10.0, 26.0),
                egui::Align2::LEFT_TOP,
                format!(
                    "{} · im Text {:.0} × {:.0} mm · Vorschau ausgeblendet{}",
                    clip.rect.size_text(),
                    mm,
                    mm * clip.rect.aspect(),
                    if all_hidden && !clip.preview_hidden {
                        " (alle)"
                    } else {
                        ""
                    }
                ),
                FontId::proportional(12.0),
                palette.text_dim,
            );
            response
        } else {
            let wanted_px = (width * ui.ctx().pixels_per_point()).ceil() as u32;
            let shown = previews.picture(asset, wanted_px);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::WHITE);
            let picture = match &shown {
                crate::pdf_preview::Shown::Picture(texture) => Some(texture),
                crate::pdf_preview::Shown::Waiting(texture) => texture.as_ref(),
                _ => None,
            };
            if let Some(texture) = picture {
                painter.image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            let note = match &shown {
                crate::pdf_preview::Shown::Waiting(None) => {
                    Some(("Vorschau wird gerechnet …".to_string(), palette.text_dim))
                }
                crate::pdf_preview::Shown::Failed(error) => {
                    Some((format!("Vorschau nicht möglich: {error}"), palette.error))
                }
                crate::pdf_preview::Shown::Unavailable => Some((
                    format!("PDF-Ausschnitt {} · {}", asset.file, clip.rect.size_text()),
                    palette.text_dim,
                )),
                _ => None,
            };
            if let Some((text, color)) = note {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(12.5),
                    color,
                );
            }
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, palette.line),
                egui::StrokeKind::Inside,
            );
            response
        };
        let rect = response.rect;
        let grip = Rect::from_center_size(rect.right_bottom(), Vec2::splat(14.0));
        let grip_response = ui
            .interact(grip, self.salt.with(("img", block)), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeNwSe)
            .on_hover_text("Ziehen ändert die Breite");
        ui.painter()
            .rect_filled(grip.shrink(3.0), 2.0, palette.accent);
        if grip_response.dragged() {
            new_mm = ((rect.width() + grip_response.drag_delta().x) / px_per_mm)
                .clamp(10.0, MAX_IMAGE_WIDTH_MM);
        }
        let mut freeze = clip.preview_frozen;
        let mut hide = clip.preview_hidden;
        let mut hide_all = all_hidden;
        response.context_menu(|ui| {
            if ui.button(CLIP_MENU).clicked() {
                output.pick_pdf_clip = true;
                ui.close();
            }
            ui.separator();
            ui.checkbox(&mut freeze, "Vorschau einfrieren");
            ui.checkbox(&mut hide, "Vorschau ausblenden");
            ui.checkbox(&mut hide_all, "Vorschau aller PDF-Ausschnitte ausblenden");
            ui.separator();
            if ui.button("Ausschnitt entfernen").clicked() {
                actions.push(Action::Remove(block));
                ui.close();
            }
        });
        crate::widgets::row_wrapped(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(
                egui::RichText::new(format!(
                    "PDF-Ausschnitt · {} · {}",
                    clip.source,
                    clip.rect.size_text()
                ))
                .size(12.0)
                .color(palette.text_dim),
            )
            .on_hover_text(format!(
                "{} im Projekt, {}",
                asset.file,
                proava_domain::size_text(asset.bytes.len())
            ));
            ui.label(
                egui::RichText::new("Breite")
                    .size(12.0)
                    .color(palette.text_dim),
            );
            let mut value = new_mm;
            if ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(10.0..=MAX_IMAGE_WIDTH_MM)
                        .speed(1.0)
                        .max_decimals(0)
                        .suffix(" mm"),
                )
                .changed()
            {
                new_mm = value;
            }
            ui.add_space(6.0);
            ui.checkbox(&mut freeze, "Vorschau einfrieren")
                .on_hover_text(
                    "Das gerechnete Vorschaubild bleibt und wird mit dem Projekt gespeichert – \
                 die PDF wird für die Vorschau nicht mehr gerechnet",
                );
            ui.checkbox(&mut hide, "Vorschau ausblenden").on_hover_text(
                "Nur ein Platzhalter mit Name und Größe; der Druck enthält den Ausschnitt",
            );
            if freezing {
                ui.spinner();
            }
            block_buttons(ui, palette, block, actions);
        });
        ui.add_space(4.0);
        if hide_all != all_hidden {
            previews.request_all_hidden(hide_all);
        }
        if freeze != clip.preview_frozen || hide != clip.preview_hidden {
            if freeze && !clip.preview_frozen {
                if hide != clip.preview_hidden {
                    previews.request(crate::pdf_preview::PreviewUpdate {
                        file: asset.file.clone(),
                        frozen: false,
                        hidden: hide,
                        png: None,
                    });
                }
                let mut target = asset.clone();
                if let Some(clip) = target.clip.as_mut() {
                    clip.preview_hidden = hide;
                }
                previews.freeze(&target);
            } else {
                previews.request(crate::pdf_preview::PreviewUpdate {
                    file: asset.file.clone(),
                    frozen: freeze,
                    hidden: hide,
                    png: clip.frozen_png.clone().filter(|_| freeze),
                });
            }
        }
        if (new_mm - mm).abs() > 0.05 {
            if let Some(DocBlock::Image { width_mm, .. }) = self.doc.blocks.get_mut(block) {
                *width_mm = Some(new_mm.round());
            }
            output.edit_key = Some(format!("rt-img:{block}"));
            return true;
        }
        false
    }
}

/// Up, down, remove for a table or image.
fn block_buttons(ui: &mut egui::Ui, palette: &Palette, block: usize, actions: &mut Vec<Action>) {
    if widgets::tool_button(ui, palette, Icon::Up, false, "Nach oben").clicked() {
        actions.push(Action::Move { block, up: true });
    }
    if widgets::tool_button(ui, palette, Icon::Down, false, "Nach unten").clicked() {
        actions.push(Action::Move { block, up: false });
    }
    if widgets::tool_button(ui, palette, Icon::Trash, false, "Entfernen").clicked() {
        actions.push(Action::Remove(block));
    }
}

/// Typing, keys and clipboard: what a text field takes.
fn is_keyboard_input(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Text(_)
            | egui::Event::Key { .. }
            | egui::Event::Paste(_)
            | egui::Event::Copy
            | egui::Event::Cut
            | egui::Event::Ime(_)
    )
}

fn format_size(size: f32) -> String {
    if size.fract() == 0.0 {
        format!("{size:.0}")
    } else {
        format!("{size:.1}").replace('.', ",")
    }
}

fn egui_align(align: Align) -> egui::Align {
    match align {
        Align::Left | Align::Justify => egui::Align::LEFT,
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
    }
}

/// Layout of a paragraph with its char formats.
/// Text format of a char format at a zoom.
fn text_format(fmt: &Fmt, zoom: f32, color: Color32) -> TextFormat {
    let size = fmt.size_pt.unwrap_or(proava_domain::DEFAULT_SIZE_PT) * PX_PER_PT * zoom;
    TextFormat {
        font_id: crate::fonts::rich(size, font_class(fmt.font.as_deref()), fmt.bold, fmt.italic),
        color,
        underline: if fmt.underline {
            Stroke::new(1.0, color)
        } else {
            Stroke::NONE
        },
        valign: egui::Align::BOTTOM,
        ..TextFormat::default()
    }
}

/// Spaces at the end of a justified, centred or right-aligned paragraph:
/// egui leaves them out of the width of the last row, so the caret stood in
/// front of a typed space until the next character came (egui's own
/// text fields keep them, `LayoutJob::keep_trailing_whitespace`). Here only
/// the last row takes them in – the caret at its end goes behind them. Right
/// aligned, the row moves left by them (the caret would stand outside the
/// text area); else the text keeps its place (no shift of justified rows).
pub(super) fn with_trailing_spaces(
    galley: std::sync::Arc<egui::Galley>,
) -> std::sync::Arc<egui::Galley> {
    let end = galley.rows.last().and_then(|placed| {
        let row = &placed.row;
        let last = row.glyphs.last()?;
        let text = row.glyphs.iter().any(|glyph| !glyph.chr.is_whitespace());
        (text && last.chr.is_whitespace())
            .then_some(last.pos.x + last.advance_width)
            .filter(|end| *end > row.size.x)
    });
    let Some(end) = end else {
        return galley;
    };
    let right = galley.job.halign == egui::Align::RIGHT;
    let mut galley = (*galley).clone();
    if let Some(placed) = galley.rows.last_mut() {
        let row = std::sync::Arc::make_mut(&mut placed.row);
        if right {
            placed.pos.x -= end - row.size.x;
        }
        row.size.x = end;
    }
    std::sync::Arc::new(galley)
}

fn layout_job(para: &Para, wrap: f32, zoom: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = egui_align(para.align);
    job.justify = para.align == Align::Justify;
    let format_of = |fmt: &Fmt| text_format(fmt, zoom, color);
    if para.text.is_empty() {
        // Empty paragraphs keep the height of their font.
        job.append("", 0.0, format_of(&para.empty_fmt));
        return job;
    }
    let mut piece = String::new();
    let mut current: Option<&Fmt> = None;
    for (ch, fmt) in para.text.chars().zip(&para.fmts) {
        if current.is_some_and(|c| c != fmt) {
            job.append(&piece, 0.0, format_of(current.unwrap_or(fmt)));
            piece.clear();
        }
        current = Some(fmt);
        piece.push(ch);
    }
    if let Some(fmt) = current {
        job.append(&piece, 0.0, format_of(fmt));
    }
    job
}

fn missing_image(file: &str) -> String {
    format!("Bild „{file}“ fehlt im Projekt.")
}

fn load_texture(
    ctx: &egui::Context,
    file: &str,
    images: &[ImageAsset],
) -> Result<egui::TextureHandle, String> {
    let asset = images
        .iter()
        .find(|image| image.file == file)
        .ok_or_else(|| missing_image(file))?;
    let image =
        crate::images::decode_preview(&asset.bytes).map_err(|error| format!("{file}: {error}"))?;
    Ok(ctx.load_texture(format!("rt-{file}"), image, egui::TextureOptions::LINEAR))
}
