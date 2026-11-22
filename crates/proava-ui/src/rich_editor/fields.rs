//! Textergänzungen im Langtext: Bieterangaben („Fabrikat: …………“, vom
//! Bieter einzutragen) und Angaben des Auftraggebers stehen im Editor als
//! Felder an ihrer Stelle im Text.
//!
//! Im Text steht je Feld ein einziges Zeichen (`complement_text::field_char`,
//! Index in [`RichEditor::fields`]): Pfeiltasten, Markieren, Entf und
//! Rücktaste behandeln es wie einen Buchstaben, versehentlich halb löschen
//! geht nicht. Für die Anzeige ersetzt [`flow_layout_job`] das Zeichen durch
//! die Beschriftung des Felds; [`FieldMap`] rechnet zwischen Zeichen im Text
//! und Zeichen in der Anzeige um. Beim Abgeben trennt
//! `complement_text::extract` Text und Textergänzungen wieder (neue Anker).

use eframe::egui::{self, text::LayoutJob, Color32, Galley, Key, Rect, RichText, Stroke, Vec2};
use proava_domain::complement_text::{field_char, field_index, next_marker};
use proava_domain::{ComplementKind, TextComplement};

use super::model::{DocBlock, Para, Pos, Spot};
use super::{text_format, RichEditor};
use crate::theme::Palette;
use crate::widgets;

/// Menu entry and tooltip of the toolbar button.
pub const BIDDER_MENU: &str = "Bieterangabe einfügen …";
pub const BIDDER_BELOW_MENU: &str = "Bieterangabe unter dem Langtext …";
pub const OWNER_MENU: &str = "Angabe des Auftraggebers einfügen …";

/// Vortexte for the quick choice (Einstellungen ▸ Darstellung); the caller
/// sets them from the settings.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPresets {
    pub bidder: Vec<String>,
    pub owner: Vec<String>,
}

impl FieldPresets {
    /// The lists from the settings.
    pub fn of(launcher: &proava_application::Launcher) -> Self {
        let (bidder, owner) = launcher.complement_captions();
        Self { bidder, owner }
    }
}

impl Default for FieldPresets {
    fn default() -> Self {
        Self {
            bidder: proava_application::default_bidder_captions(),
            owner: proava_application::default_owner_captions(),
        }
    }
}

/// Where a new field goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldSpot {
    /// At the caret (in a table: on its own line after the table).
    #[default]
    Caret,
    /// On a line of its own under the long text.
    End,
}

/// The dialog for a new or an existing field.
pub(super) struct FieldDialog {
    /// None = new field; Some(i) = field `i` is edited.
    editing: Option<usize>,
    complement: TextComplement,
    spot: FieldSpot,
    focus: bool,
}

/// One field in a laid-out paragraph: its char in the text, where its
/// label starts in the galley and how many chars it has there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FieldSpan {
    pub model: usize,
    pub start: usize,
    pub len: usize,
    pub field: usize,
}

/// Fields of one paragraph, in text order.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct FieldMap {
    pub spans: Vec<FieldSpan>,
}

impl FieldMap {
    /// Galley char index of text char index `offset`.
    pub fn to_galley(&self, offset: usize) -> usize {
        let mut extra = 0usize;
        for span in &self.spans {
            if span.model >= offset {
                break;
            }
            extra += span.len.saturating_sub(1);
        }
        offset.saturating_add(extra)
    }

    /// Text char index of galley index `index`; inside a label the nearer
    /// edge of the field.
    pub fn to_model(&self, index: usize) -> usize {
        let mut extra = 0usize;
        for span in &self.spans {
            if index <= span.start {
                break;
            }
            if index < span.start + span.len {
                return if (index - span.start) * 2 < span.len {
                    span.model
                } else {
                    span.model + 1
                };
            }
            extra += span.len.saturating_sub(1);
        }
        index - extra
    }
}

/// Paragraph layout with the field labels in place of the field chars.
/// Also returns the text for the spelling check (labels as spaces, same
/// length as the galley text).
pub(super) fn flow_layout_job(
    para: &Para,
    wrap: f32,
    zoom: f32,
    color: Color32,
    fields: &[TextComplement],
) -> (LayoutJob, FieldMap, String) {
    if !para
        .text
        .chars()
        .any(|character| field_index(character).is_some())
    {
        return (
            super::layout_job(para, wrap, zoom, color),
            FieldMap::default(),
            para.text.clone(),
        );
    }
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = super::egui_align(para.align);
    job.justify = para.align == proava_domain::Align::Justify;
    let mut map = FieldMap::default();
    let mut spell = String::new();
    let mut galley_len = 0usize;
    let mut piece = String::new();
    let mut current: Option<&super::model::Fmt> = None;
    for (model, (character, fmt)) in para.text.chars().zip(&para.fmts).enumerate() {
        if let Some(field) = field_index(character) {
            if let Some(last) = current.take() {
                job.append(&piece, 0.0, text_format(last, zoom, color));
                piece.clear();
            }
            let label = format!("\u{a0}{}\u{a0}", label_of(fields.get(field)));
            let mut format = text_format(fmt, zoom, color);
            format.underline = Stroke::NONE;
            format.font_id = crate::fonts::rich(
                format.font_id.size,
                proava_domain::font_class(fmt.font.as_deref()),
                false,
                false,
            );
            let len = label.chars().count();
            job.append(&label, 0.0, format);
            map.spans.push(FieldSpan {
                model,
                start: galley_len,
                len,
                field,
            });
            spell.extend(std::iter::repeat_n(' ', len));
            galley_len += len;
            continue;
        }
        if current.is_some_and(|last| last != fmt) {
            job.append(
                &piece,
                0.0,
                text_format(current.unwrap_or(fmt), zoom, color),
            );
            piece.clear();
        }
        current = Some(fmt);
        piece.push(character);
        spell.push(character);
        galley_len += 1;
    }
    if let Some(last) = current {
        job.append(&piece, 0.0, text_format(last, zoom, color));
    }
    (job, map, spell)
}

/// Label of a field in the editor („Fabrikat: ………… (vom Bieter)“).
fn label_of(complement: Option<&TextComplement>) -> String {
    match complement {
        Some(complement) => {
            let label = complement.field_label();
            if label.is_empty() {
                format!("{} ohne Text", complement.kind_name())
            } else {
                label
            }
        }
        None => "Textergänzung fehlt".to_string(),
    }
}

/// Text of a field outside proAVA (copy as plain text, HTML for other
/// programs, editors without fields): its printed form, set off by spaces
/// like in the PDF – without the editor's „(vom Bieter)“.
pub(super) fn outward_of(
    complement: &TextComplement,
    before: Option<char>,
    after: Option<char>,
) -> String {
    let text = complement.print_text();
    proava_domain::complement_text::set_off(before, text.trim_end(), after)
}

/// Screen rows (relative to the galley) of the chars `from..to`.
pub(super) fn range_rects(galley: &Galley, from: usize, to: usize) -> Vec<Rect> {
    let mut rects = Vec::new();
    let mut index = 0usize;
    for row in &galley.rows {
        let count = row.char_count_including_newline().0;
        let row_from = from.max(index);
        let row_to = to.min(index + row.char_count_excluding_newline().0);
        if row_from < row_to {
            let x0 = row
                .glyphs
                .get(row_from - index)
                .map_or(row.rect().max.x, |glyph| row.pos.x + glyph.pos.x);
            let x1 = row
                .glyphs
                .get(row_to - index)
                .map_or(row.rect().max.x, |glyph| row.pos.x + glyph.pos.x);
            rects.push(Rect::from_min_max(
                egui::pos2(x0, row.rect().min.y),
                egui::pos2(x1, row.rect().max.y),
            ));
        }
        index += count;
    }
    rects
}

/// Background and frame of the fields of a paragraph (under the text).
pub(super) fn paint_fields(
    painter: &egui::Painter,
    palette: &Palette,
    galley: &Galley,
    origin: egui::Pos2,
    map: &FieldMap,
    fields: &[TextComplement],
) {
    for span in &map.spans {
        let owner = fields
            .get(span.field)
            .is_some_and(|complement| complement.kind == ComplementKind::Owner);
        let (fill, stroke) = if owner {
            (palette.text_dim.gamma_multiply(0.14), palette.line)
        } else {
            (
                palette.accent.gamma_multiply(0.16),
                palette.accent.gamma_multiply(0.7),
            )
        };
        for rect in range_rects(galley, span.start, span.start + span.len) {
            let rect = rect
                .translate(origin.to_vec2())
                .shrink2(Vec2::new(0.5, 0.0));
            painter.rect_filled(rect, 3.0, fill);
            painter.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.0, stroke),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// The field under a screen point (galley-relative point), if any.
pub(super) fn span_at(galley: &Galley, map: &FieldMap, point: egui::Pos2) -> Option<FieldSpan> {
    map.spans.iter().copied().find(|span| {
        range_rects(galley, span.start, span.start + span.len)
            .iter()
            .any(|rect| rect.contains(point))
    })
}

impl RichEditor {
    /// Whether this editor shows Textergänzungen as fields (long texts of
    /// positions and hints).
    pub fn has_fields(&self) -> bool {
        self.fields_on
    }

    /// Opens the dialog for a new field.
    pub(super) fn open_new_field(&mut self, kind: ComplementKind, spot: FieldSpot, caption: &str) {
        if !self.fields_on {
            return;
        }
        let complement = TextComplement {
            marker: next_marker(&self.fields),
            kind,
            caption: caption.to_string(),
            ..TextComplement::default()
        };
        self.field_dialog = Some(FieldDialog {
            editing: None,
            complement,
            spot,
            focus: true,
        });
    }

    /// Opens the dialog for field `index`.
    pub(super) fn open_field(&mut self, index: usize) {
        if let Some(complement) = self.fields.get(index) {
            self.field_dialog = Some(FieldDialog {
                editing: Some(index),
                complement: complement.clone(),
                spot: FieldSpot::Caret,
                focus: true,
            });
        }
    }

    /// Inserts a field with this Vortext right away (quick choice).
    pub(super) fn quick_field(&mut self, kind: ComplementKind, spot: FieldSpot, caption: &str) {
        let complement = TextComplement {
            marker: next_marker(&self.fields),
            kind,
            caption: caption.to_string(),
            ..TextComplement::default()
        };
        self.insert_field(complement, spot);
    }

    /// Puts `complement` into the text as a field and the caret after it.
    pub(super) fn insert_field(&mut self, mut complement: TextComplement, spot: FieldSpot) {
        if !self.fields_on || self.fields.len() >= proava_domain::complement_text::MAX_FIELDS {
            return;
        }
        self.drop_composition();
        if complement.marker.trim().is_empty()
            || self
                .fields
                .iter()
                .any(|field| same_marker(&field.marker, &complement.marker))
        {
            complement.marker = next_marker(&self.fields);
        }
        complement.anchor = None;
        let complement = complement.with_spaces();
        let index = self.fields.len();
        self.fields.push(complement);
        let text = field_char(index).to_string();
        let caret = if spot == FieldSpot::Caret && self.touched && self.active.cell.is_none() {
            let fmt = self.typing_fmt_for_fields();
            let at = self.doc.delete_range(self.flow.anchor, self.flow.head);
            // A space before the field when the text there has none
            // („Tasten, Fabrikat: …“, not „Tasten,Fabrikat:“).
            let glued = at.offset > 0
                && self.doc.para(Spot::block(at.block)).is_some_and(|para| {
                    para.text
                        .chars()
                        .nth(at.offset - 1)
                        .is_some_and(|before| !before.is_whitespace())
                });
            let text = if glued { format!(" {text}") } else { text };
            self.doc.insert_text(at, &text, &fmt)
        } else {
            // Under the text, or after the table the caret is in.
            let block = match (spot, self.active.cell) {
                (FieldSpot::Caret, Some(_)) if self.touched => self.active.block + 1,
                _ => self.doc.blocks.len(),
            };
            let reuse = block == self.doc.blocks.len()
                && matches!(self.doc.blocks.last(), Some(DocBlock::Para(para)) if para.is_empty() && para.kind == super::model::ParaKind::Text)
                && self.doc.blocks.len() > 1;
            let target = if reuse {
                block - 1
            } else {
                self.doc.blocks.insert(
                    block.min(self.doc.blocks.len()),
                    DocBlock::Para(Para::default()),
                );
                block.min(self.doc.blocks.len() - 1)
            };
            self.doc
                .insert_text(Pos::new(target, 0), &text, &Default::default())
        };
        self.doc.ensure_text_ends();
        self.focus = Some((Spot::block(caret.block), caret.offset, caret.offset));
        self.touched = true;
        self.field_changed = true;
    }

    /// Format of a field typed at the caret: that of the text around it.
    fn typing_fmt_for_fields(&self) -> super::model::Fmt {
        let head = self.flow.head;
        self.doc
            .para(Spot::block(head.block))
            .map(|para| para.fmt_at(head.offset))
            .unwrap_or_default()
    }

    /// Removes the field whose char is at `pos`.
    pub(super) fn remove_field_at(&mut self, pos: Pos) {
        let end = Pos::new(pos.block, pos.offset + 1);
        let caret = self.doc.delete_range(pos, end);
        self.focus = Some((Spot::block(caret.block), caret.offset, caret.offset));
        self.field_changed = true;
    }

    /// Position of the char of field `index` in the running text.
    pub(super) fn field_pos(&self, index: usize) -> Option<Pos> {
        let wanted = field_char(index);
        self.doc
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block, item)| match item {
                DocBlock::Para(para) => para
                    .text
                    .chars()
                    .position(|character| character == wanted)
                    .map(|offset| Pos::new(block, offset)),
                _ => None,
            })
    }

    /// The field dialog, when open.
    pub(super) fn field_dialog_ui(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        let Some(mut dialog) = self.field_dialog.take() else {
            return;
        };
        let presets = match dialog.complement.kind {
            ComplementKind::Bidder => self.field_presets.bidder.clone(),
            ComplementKind::Owner => self.field_presets.owner.clone(),
        };
        let others: Vec<String> = self
            .fields
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != dialog.editing)
            .filter(|(index, _)| self.field_pos(*index).is_some())
            .map(|(_, field)| field.marker.clone())
            .collect();
        let doubled = others
            .iter()
            .any(|marker| same_marker(marker, &dialog.complement.marker));
        let id = self.salt.with("field_dialog");
        let mut remove = false;
        let ((cancelled, confirmed), close) =
            widgets::modal_with_id(ui, palette, id, 560.0, |ui| {
                // Enter confirms – read before the widgets: a focused button
                // takes Enter as a click. Not while a field has two lines.
                let multiline = dialog.complement.caption.contains('\n')
                    || dialog.complement.tail.contains('\n');
                let enter = !multiline
                    && ui.input(|input| input.key_pressed(Key::Enter) && !input.modifiers.shift);
                let bidder = dialog.complement.kind == ComplementKind::Bidder;
                let title = match (dialog.editing.is_some(), bidder) {
                    (false, true) => "Bieterangabe einfügen",
                    (false, false) => "Angabe des Auftraggebers einfügen",
                    (true, true) => "Bieterangabe bearbeiten",
                    (true, false) => "Angabe des Auftraggebers bearbeiten",
                };
                let hint = if bidder {
                    "Der Bieter trägt die Angabe im Angebot ein (GAEB: Textergänzung vom Bieter). \
                 Im Druck steht sie als Lücke mit Punkten im Langtext."
                } else {
                    "Ein Wert, den du vorgibst, z. B. „Nennweite DN 50“ \
                 (GAEB: Textergänzung vom Auftraggeber)."
                };
                widgets::title(ui, palette, title, hint);
                if !presets.is_empty() {
                    ui.label(
                        RichText::new("Schnellauswahl")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        for preset in &presets {
                            let chosen = dialog.complement.caption.trim() == preset.trim();
                            if ui
                                .add(egui::Button::new(preset.as_str()).selected(chosen))
                                .clicked()
                            {
                                dialog.complement.caption = preset.clone();
                                dialog.focus = true;
                            }
                        }
                        if ui
                            .add(egui::Button::new("frei"))
                            .on_hover_text("Vortext selbst schreiben")
                            .clicked()
                        {
                            dialog.complement.caption.clear();
                            dialog.focus = true;
                        }
                    });
                    ui.add_space(10.0);
                }
                egui::Grid::new(id.with("grid"))
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Art");
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut dialog.complement.kind,
                                ComplementKind::Bidder,
                                "vom Bieter",
                            );
                            ui.radio_value(
                                &mut dialog.complement.kind,
                                ComplementKind::Owner,
                                "vom Auftraggeber",
                            );
                        });
                        ui.end_row();
                        let label = ui.label("Vortext");
                        let response =
                            text_line(ui, &mut dialog.complement.caption, "z. B. Fabrikat:")
                                .labelled_by(label.id);
                        if std::mem::take(&mut dialog.focus) {
                            response.request_focus();
                        }
                        ui.end_row();
                        let label = ui.label(if bidder { "Eintrag" } else { "Wert" });
                        text_line(
                            ui,
                            &mut dialog.complement.body,
                            if bidder {
                                "leer lassen – trägt der Bieter ein"
                            } else {
                                "z. B. DN 50"
                            },
                        )
                        .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Nachtext");
                        text_line(
                            ui,
                            &mut dialog.complement.tail,
                            "optional, z. B. oder gleichwertig",
                        )
                        .labelled_by(label.id);
                        ui.end_row();
                        let label = ui.label("Kennung");
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.complement.marker)
                                .desired_width(48.0),
                        )
                        .labelled_by(label.id)
                        .on_hover_text("Nummer der Textergänzung in der Position (GAEB MarkLbl)");
                        ui.end_row();
                        if dialog.editing.is_none() {
                            ui.label("Stelle");
                            ui.horizontal(|ui| {
                                ui.radio_value(
                                    &mut dialog.spot,
                                    FieldSpot::Caret,
                                    "an der Schreibmarke",
                                );
                                ui.radio_value(
                                    &mut dialog.spot,
                                    FieldSpot::End,
                                    "unter dem Langtext",
                                );
                            });
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
                ui.label(RichText::new("Im Druck").size(12.0).color(palette.text_dim));
                ui.label(dialog.complement.print_text());
                ui.add_space(6.0);
                if dialog.complement.caption.trim().is_empty() {
                    ui.label(
                        RichText::new("Ohne Vortext weiß der Bieter nicht, was er eintragen soll.")
                            .size(12.0)
                            .color(palette.warn),
                    );
                }
                if dialog.complement.marker.trim().is_empty() {
                    ui.label(
                        RichText::new("Die Kennung fehlt.")
                            .size(12.0)
                            .color(palette.error),
                    );
                } else if doubled {
                    ui.label(
                        RichText::new(format!(
                            "Die Kennung {} hat schon eine andere Textergänzung dieser Position.",
                            dialog.complement.marker.trim()
                        ))
                        .size(12.0)
                        .color(palette.error),
                    );
                }
                ui.add_space(8.0);
                if dialog.editing.is_some()
                    && ui
                        .button(format!("{} entfernen", dialog.complement.kind_name()))
                        .on_hover_text("Das Feld aus dem Langtext nehmen (wie Entf)")
                        .clicked()
                {
                    remove = true;
                }
                let label = if dialog.editing.is_some() {
                    "Übernehmen"
                } else {
                    "Einfügen"
                };
                let (cancelled, confirmed) = widgets::dialog_buttons(ui, palette, label);
                (cancelled, confirmed || enter)
            });
        if remove {
            if let Some(pos) = dialog.editing.and_then(|index| self.field_pos(index)) {
                self.remove_field_at(pos);
            }
            return;
        }
        let valid = !dialog.complement.marker.trim().is_empty() && !doubled;
        if confirmed && valid {
            dialog.complement.marker = dialog.complement.marker.trim().to_string();
            match dialog.editing {
                Some(index) => {
                    if let Some(field) = self.fields.get_mut(index) {
                        let anchor = field.anchor;
                        if *field != dialog.complement {
                            *field = dialog.complement.with_spaces();
                        }
                        field.anchor = anchor;
                        self.field_changed = true;
                    }
                    if let Some(pos) = self.field_pos(index) {
                        let after = Pos::new(pos.block, pos.offset + 1);
                        self.focus = Some((Spot::block(after.block), after.offset, after.offset));
                    }
                }
                None => {
                    let spot = dialog.spot;
                    self.insert_field(dialog.complement, spot);
                }
            }
            return;
        }
        if cancelled || close {
            self.flow.want_focus = self.touched;
            return;
        }
        self.field_dialog = Some(dialog);
    }

    /// Entries for menus: quick choices for one spot, „andere …“ and the
    /// Auftraggeber dialog. True when something was chosen (menu closes).
    pub(super) fn field_menu_items(&mut self, ui: &mut egui::Ui, spot: FieldSpot) -> bool {
        let mut chosen = false;
        let presets = self.field_presets.bidder.clone();
        for preset in presets {
            if ui.button(preset.as_str()).clicked() {
                self.quick_field(ComplementKind::Bidder, spot, &preset);
                chosen = true;
            }
        }
        if ui.button("Andere Bieterangabe …").clicked() {
            self.open_new_field(ComplementKind::Bidder, spot, "");
            chosen = true;
        }
        ui.separator();
        if ui.button(OWNER_MENU).clicked() {
            self.open_new_field(ComplementKind::Owner, spot, "");
            chosen = true;
        }
        if chosen {
            ui.close();
        }
        chosen
    }
}

impl RichEditor {
    /// `text` with every field char replaced by the field's printed text
    /// (for the clipboard of other programs).
    pub(super) fn fields_as_text(&self, text: &str) -> String {
        let characters: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        for (at, character) in characters.iter().enumerate() {
            match field_index(*character).and_then(|index| self.fields.get(index)) {
                Some(complement) => {
                    let after = characters.get(at + 1).copied();
                    out.push_str(&outward_of(complement, out.chars().last(), after));
                }
                None if field_index(*character).is_some() => {}
                None => out.push(*character),
            }
        }
        out
    }

    /// The Textergänzungen of the field chars in `text` (for a copy).
    pub(super) fn field_snapshot(&self, text: &str) -> Vec<(char, TextComplement)> {
        let mut out: Vec<(char, TextComplement)> = Vec::new();
        for character in text.chars() {
            if let Some(complement) =
                field_index(character).and_then(|index| self.fields.get(index))
            {
                if !out.iter().any(|(known, _)| *known == character) {
                    out.push((character, complement.clone()));
                }
            }
        }
        out
    }

    /// A copied fragment for this editor: its fields become fields here
    /// (new ones get a free Kennung); an editor without fields gets their
    /// text („Fabrikat: ………… (vom Bieter)“).
    pub(super) fn adopt_fields(
        &mut self,
        mut fragment: super::model::Doc,
        snapshot: &[(char, TextComplement)],
    ) -> super::model::Doc {
        // Per copied field char: a field char here, or its text.
        let mut map: Vec<(char, Result<char, TextComplement>)> = Vec::new();
        for (character, complement) in snapshot {
            let index = field_index(*character).unwrap_or(usize::MAX);
            let target = if !self.fields_on
                || self.fields.len() >= proava_domain::complement_text::MAX_FIELDS
            {
                Err(complement.clone())
            } else if self.fields.get(index) == Some(complement) {
                Ok(*character)
            } else {
                let mut complement = complement.clone();
                if self
                    .fields
                    .iter()
                    .any(|field| same_marker(&field.marker, &complement.marker))
                {
                    complement.marker = next_marker(&self.fields);
                }
                complement.anchor = None;
                self.fields.push(complement);
                Ok(field_char(self.fields.len() - 1))
            };
            map.push((*character, target));
        }
        // `cell`: fields belong into the running text – in a table cell
        // their text.
        let convert = |para: &mut Para, cell: bool| {
            if !proava_domain::complement_text::has_fields(&para.text) {
                return;
            }
            let characters: Vec<char> = para.text.chars().collect();
            let mut text = String::with_capacity(para.text.len());
            let mut fmts = Vec::with_capacity(para.fmts.len());
            for (at, (character, fmt)) in characters.iter().zip(para.fmts.iter()).enumerate() {
                let replaced = match field_index(*character) {
                    // A field without its Textergänzung goes.
                    Some(_) => match map.iter().find(|(from, _)| from == character) {
                        Some((_, Ok(field))) if !cell => field.to_string(),
                        Some((_, Ok(_))) => snapshot
                            .iter()
                            .find(|(from, _)| from == character)
                            .map(|(_, complement)| {
                                outward_of(
                                    complement,
                                    text.chars().last(),
                                    characters.get(at + 1).copied(),
                                )
                            })
                            .unwrap_or_default(),
                        Some((_, Err(complement))) => outward_of(
                            complement,
                            text.chars().last(),
                            characters.get(at + 1).copied(),
                        ),
                        None => String::new(),
                    },
                    None => character.to_string(),
                };
                for character in replaced.chars() {
                    text.push(character);
                    fmts.push(fmt.clone());
                }
            }
            para.text = text;
            para.fmts = fmts;
        };
        for block in fragment.blocks.iter_mut() {
            match block {
                DocBlock::Para(para) => convert(para, false),
                DocBlock::Table(table) => {
                    for cell in table.rows.iter_mut().flatten() {
                        convert(cell, true);
                    }
                }
                DocBlock::Image { .. } => {}
            }
        }
        fragment
    }
}

/// A one-line field; text with line breaks (STLB-Bau: „\nvom Bieter
/// einzutragen“) gets two lines so that the break stays.
fn text_line(ui: &mut egui::Ui, value: &mut String, hint: &str) -> egui::Response {
    let edit = if value.contains('\n') {
        egui::TextEdit::multiline(value).desired_rows(2)
    } else {
        egui::TextEdit::singleline(value)
    };
    ui.add(edit.hint_text(hint).desired_width(360.0))
}

/// Kennungen „1“ and „01“ are the same.
fn same_marker(a: &str, b: &str) -> bool {
    let (a, b) = (
        a.trim().trim_start_matches('0'),
        b.trim().trim_start_matches('0'),
    );
    !a.is_empty() && a == b || (a.is_empty() && b.is_empty())
}

#[cfg(test)]
mod editor_tests {
    use eframe::egui;
    use proava_domain::complement_text::field_char;
    use proava_domain::{ComplementKind, RichText, TextAnchor, TextComplement};

    use super::super::model::{Pos, Spot};
    use super::super::RichEditor;
    use super::FieldSpot;

    fn frame(
        ctx: &egui::Context,
        editor: &mut RichEditor,
        text: &mut RichText,
        list: &mut Vec<TextComplement>,
        events: Vec<egui::Event>,
    ) -> bool {
        let palette = crate::theme::Palette::of(proava_application::ThemeChoice::Light);
        let mut changed = false;
        let input = egui::RawInput {
            events,
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            changed = editor
                .show_with_complements(ui, &palette, text, list, &[])
                .changed;
        });
        output.textures_delta.clear();
        changed
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Insert at the caret, select it and copy: outside it is text, pasted
    /// here again a second field with a new Kennung; Rücktaste takes a field
    /// whole.
    #[test]
    fn field_is_inserted_copied_as_field_and_deleted_as_one() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("felder");
        let mut text = RichText::plain("Regler mit Display");
        let mut list = Vec::new();
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        editor.focus = Some((Spot::block(0), 10, 10));
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        editor.quick_field(ComplementKind::Bidder, FieldSpot::Caret, "Fabrikat:");
        assert!(frame(&ctx, &mut editor, &mut text, &mut list, vec![]));
        assert_eq!(
            text.to_plain(),
            "Regler mit  Display",
            "Leerzeichen vor dem Feld"
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].caption, "Fabrikat: ");
        assert_eq!(list[0].marker, "01");
        assert_eq!(
            list[0].anchor,
            Some(TextAnchor {
                block: 0,
                offset: 11,
                own_line: false
            })
        );

        // Select the field (one char) and copy it.
        editor.focus = Some((Spot::block(0), 11, 12));
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            &mut list,
            vec![egui::Event::Copy],
        );
        let copy = super::super::flow::shared_copy(&ctx).expect("kopiert");
        let plain = copy.0.clone();
        assert_eq!(plain, "Fabrikat: …………", "nach außen als gedruckter Text");
        assert_eq!(copy.2[0].0, field_char(0));
        // Paste at the end: a second field.
        let end = editor.doc.end();
        editor.focus = Some((Spot::block(end.block), end.offset, end.offset));
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        frame(
            &ctx,
            &mut editor,
            &mut text,
            &mut list,
            vec![egui::Event::Paste(plain)],
        );
        assert_eq!(list.len(), 2, "{list:?}");
        assert_eq!(list[1].caption, "Fabrikat: ");
        assert_eq!(list[1].marker, "02", "neue Kennung");
        assert_eq!(text.to_plain(), "Regler mit  Display");

        // Rücktaste after the second field: the field goes, the text stays.
        frame(
            &ctx,
            &mut editor,
            &mut text,
            &mut list,
            vec![key(egui::Key::Backspace, egui::Modifiers::NONE)],
        );
        assert_eq!(list.len(), 1);
        assert_eq!(text.to_plain(), "Regler mit  Display");
        assert_eq!(
            editor.flow.head,
            Pos::new(0, 20),
            "Schreibmarke dort, wo das Feld stand"
        );
    }

    /// Copy a field in one editor (Eigenschaftenbereich), paste it in
    /// another (großes Langtextfenster, Positionsfenster – same program,
    /// same egui context): it arrives as a field. A second program gets it
    /// from the clipboard HTML; an editor without fields gets its text.
    #[test]
    fn fields_travel_between_editors_and_programs() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut panel = RichEditor::new("panel");
        let mut text = RichText::plain("Regler");
        let mut list = vec![TextComplement {
            marker: "07".into(),
            caption: "Fabrikat: ".into(),
            tail: " oder gleichwertig".into(),
            anchor: Some(TextAnchor {
                block: 0,
                offset: 6,
                own_line: false,
            }),
            ..TextComplement::default()
        }];
        frame(&ctx, &mut panel, &mut text, &mut list, vec![]);
        panel.focus = Some((Spot::block(0), 0, 7));
        frame(&ctx, &mut panel, &mut text, &mut list, vec![]);
        frame(
            &ctx,
            &mut panel,
            &mut text,
            &mut list,
            vec![egui::Event::Copy],
        );
        let copy = super::super::flow::shared_copy(&ctx).expect("kopiert");
        let plain = copy.0.clone();
        let html = super::super::html::to_html_with_fields(&copy.1.to_rich(), &panel.fields);
        assert!(
            html.contains("data-proava-feld=\"B|07|Fabrikat: |"),
            "{html}"
        );
        assert!(
            html.contains("Regler<span data-proava-feld=\"B|07|Fabrikat: || oder gleichwertig\"> Fabrikat: ………… oder gleichwertig</span>"),
            "{html}"
        );

        // Another editor of the same program (other position).
        let mut window = RichEditor::new("window");
        let mut other = RichText::plain("Fühler");
        let mut other_list = vec![TextComplement {
            marker: "07".into(),
            caption: "Typ: ".into(),
            ..TextComplement::default()
        }];
        frame(&ctx, &mut window, &mut other, &mut other_list, vec![]);
        window.focus = Some((Spot::block(0), 6, 6));
        frame(&ctx, &mut window, &mut other, &mut other_list, vec![]);
        frame(
            &ctx,
            &mut window,
            &mut other,
            &mut other_list,
            vec![egui::Event::Paste(plain.clone())],
        );
        assert_eq!(other.to_plain(), "FühlerRegler", "{other_list:?}");
        assert_eq!(other_list.len(), 2);
        let pasted = other_list
            .iter()
            .find(|field| field.caption == "Fabrikat: ")
            .expect("als Feld");
        assert_eq!(pasted.marker, "08", "07 ist dort belegt");
        assert_eq!(pasted.tail, " oder gleichwertig");
        assert_eq!(
            pasted.anchor,
            Some(TextAnchor {
                block: 0,
                offset: 12,
                own_line: false
            })
        );

        // A second program: own egui context, only the clipboard HTML.
        let second = egui::Context::default();
        crate::fonts::install(&second);
        let mut far = RichEditor::new("far");
        let mut far_text = RichText::default();
        let mut far_list = Vec::new();
        frame(&second, &mut far, &mut far_text, &mut far_list, vec![]);
        far.focus = Some((Spot::block(0), 0, 0));
        frame(&second, &mut far, &mut far_text, &mut far_list, vec![]);
        far.paste_with(&plain, Some(html.clone()), &second);
        far.field_changed = true;
        frame(&second, &mut far, &mut far_text, &mut far_list, vec![]);
        assert_eq!(far_text.to_plain(), "Regler");
        assert_eq!(far_list.len(), 1, "als Feld, nicht als Text");
        assert_eq!(
            (far_list[0].marker.as_str(), far_list[0].caption.as_str()),
            ("07", "Fabrikat: ")
        );

        // An editor without fields (Brief, Ausführungsbeschreibung): the text.
        let mut letter = RichEditor::new("brief");
        let mut body = RichText::default();
        let palette = crate::theme::Palette::of(proava_application::ThemeChoice::Light);
        let run = |events: Vec<egui::Event>, editor: &mut RichEditor, body: &mut RichText| {
            let mut output = second.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, &palette, body, &[]);
                },
            );
            output.textures_delta.clear();
        };
        run(vec![], &mut letter, &mut body);
        letter.focus = Some((Spot::block(0), 0, 0));
        run(vec![], &mut letter, &mut body);
        letter.paste_with(&plain, Some(html), &second);
        letter.field_changed = true;
        run(vec![], &mut letter, &mut body);
        assert_eq!(body.to_plain(), "Regler Fabrikat: ………… oder gleichwertig");
    }

    /// An image inserted at the caret before a field: the field keeps its
    /// place in the text (`complements()` has the new anchors).
    #[test]
    fn image_before_a_field_moves_its_anchor() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let mut editor = RichEditor::new("bild");
        let mut text = RichText::plain("Erster Absatz\nZweiter Absatz");
        let mut list = vec![TextComplement {
            marker: "01".into(),
            caption: "Typ: ".into(),
            anchor: Some(TextAnchor {
                block: 1,
                offset: 7,
                own_line: false,
            }),
            ..TextComplement::default()
        }];
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        editor.focus = Some((Spot::block(0), 13, 13));
        frame(&ctx, &mut editor, &mut text, &mut list, vec![]);
        let new_text = editor.insert_image("schema.png".into());
        let new_list = editor.complements().expect("Felder an");
        let image = new_text
            .blocks()
            .iter()
            .position(|block| matches!(block, proava_domain::TextBlock::Image { .. }))
            .unwrap();
        let second = new_text
            .blocks()
            .iter()
            .position(|block| {
                block
                    .runs()
                    .is_some_and(|runs| runs.iter().any(|run| run.text.contains("Zweiter")))
            })
            .unwrap();
        assert!(image < second);
        assert_eq!(
            new_list[0].anchor,
            Some(TextAnchor {
                block: second,
                offset: 7,
                own_line: false
            })
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{same_marker, FieldMap, FieldSpan};

    #[test]
    fn char_indices_map_both_ways() {
        // "ab[F]c": field at 2, label of 5 chars → galley "ab12345c".
        let map = FieldMap {
            spans: vec![FieldSpan {
                model: 2,
                start: 2,
                len: 5,
                field: 0,
            }],
        };
        assert_eq!(map.to_galley(0), 0);
        assert_eq!(map.to_galley(2), 2, "vor dem Feld");
        assert_eq!(map.to_galley(3), 7, "hinter dem Feld");
        assert_eq!(map.to_galley(4), 8);
        assert_eq!(map.to_galley(usize::MAX), usize::MAX);
        assert_eq!(map.to_model(2), 2);
        assert_eq!(map.to_model(3), 2, "vordere Hälfte → davor");
        assert_eq!(map.to_model(5), 3, "hintere Hälfte → dahinter");
        assert_eq!(map.to_model(7), 3);
        assert_eq!(map.to_model(8), 4);
    }

    #[test]
    fn markers_compare_without_leading_zeros() {
        assert!(same_marker("1", "01"));
        assert!(!same_marker("1", "10"));
        assert!(!same_marker("01", "02"));
    }
}
