//! Parts of the edit panel: Ordnungszahl von Hand, Hinweis-Überschrift und
//! Vorbemerkung, „Weitere Angaben“ einer Position (Textergänzungen,
//! Wiederholung, Menge offen, entfällt, Schwerpunkt, EP-Anteile,
//! Zeitansatz, Unterbeschreibungen).

use eframe::egui::{self, FontId, RichText};
use proava_domain::{ComplementKind, LvRow, Note, Position, Project, TextComplement};

use super::context_menu::{focus_ordinal_key, ordinal_field_id, FocusOrdinal};
use super::fields::{self, caption};
use super::{EditorOutput, LvEditor};
use crate::lv_ops::Command;
use crate::numbers;
use crate::struct_ops;
use crate::theme::Palette;
use crate::undo::EditKey;

impl LvEditor {
    /// OZ field of a heading or position: the fixed part in grey, the own
    /// part editable; checked when the field is left (the block moves along).
    pub(super) fn ordinal_field(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let ctx = ui.ctx().clone();
        let lv = project.lv();
        let row = &lv.rows()[index];
        let id = row.id();
        let (prefix, own) = struct_ops::ordinal_stage(lv, index);
        let hint = match row {
            LvRow::Group(group) => {
                let level = group.level();
                format!(
                    "{} · Stufe {} von {} · {} {}",
                    lv.level_label(level),
                    level + 1,
                    lv.level_count().max(1),
                    lv.oz_mask().level_width(level),
                    if lv.oz_mask().level_is_letters(level) {
                        "Buchstabe(n)"
                    } else {
                        "Stellen"
                    }
                )
            }
            _ => {
                let index_width = lv.oz_mask().index_width();
                format!(
                    "Position · {} Stellen{}",
                    lv.oz_mask().position_width(),
                    if index_width > 0 {
                        ", Index mit Punkt (0010.1)"
                    } else {
                        ""
                    }
                )
            }
        };
        caption(ui, palette, "Ordnungszahl");
        let field = ordinal_field_id(id);
        let mut commit = None;
        crate::widgets::row(ui, |ui| {
            if !prefix.is_empty() {
                ui.label(
                    RichText::new(&prefix)
                        .monospace()
                        .size(13.5)
                        .color(palette.text_dim),
                );
            }
            let checked =
                fields::checked_field(ui, palette, field, &own, 110.0, FontId::monospace(13.5));
            let wanted = ctx.data(|data| data.get_temp::<FocusOrdinal>(focus_ordinal_key()));
            if wanted.is_some_and(|FocusOrdinal(row)| row == id) {
                checked.response.request_focus();
                // Strg+Umschalt+O selects the number: typing „481“ replaces „01“.
                let mut state = egui::text_edit::TextEditState::load(&ctx, checked.response.id)
                    .unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::select_all_str(&own)));
                state.store(&ctx, checked.response.id);
                ctx.data_mut(|data| data.remove::<FocusOrdinal>(focus_ordinal_key()));
            }
            checked
                .response
                .on_hover_text("Eigene Nummer eintragen (z. B. 481); Enter oder Tab übernimmt.");
            commit = checked.commit;
        });
        ui.label(RichText::new(hint).size(11.5).color(palette.text_dim));
        if let Some(text) = commit {
            let before = project.lv().clone();
            let moved = project.lv().block_range(index).len().saturating_sub(1);
            match project.lv_mut().set_row_number(index, &text) {
                Ok(()) => {
                    self.undo.record(&before, None);
                    fields::accept(&ctx, field);
                    output.dirty = true;
                    output.toast = Some(if moved > 0 {
                        format!(
                            "Ordnungszahl {} – {} darunter mitgezogen.",
                            project.lv().rows()[index].ordinal(),
                            crate::format::count(moved, "Zeile", "Zeilen")
                        )
                    } else {
                        format!("Ordnungszahl {}.", project.lv().rows()[index].ordinal())
                    });
                }
                Err(error) => fields::refuse(&ctx, field, error),
            }
        }
        ui.add_space(8.0);
    }

    /// Überschrift, Vorbemerkung and Schlussbemerkung of a hint.
    pub(super) fn note_extra(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        note: &Note,
        output: &mut EditorOutput,
    ) {
        let id = note.id();
        caption(ui, palette, "Überschrift (optional)");
        let mut heading = note.heading().to_string();
        let heading_changed =
            fields::text_field(ui, &mut heading, egui::Id::unique(("note_heading", id)), "");
        let mut preliminary = note.is_preliminary();
        let preliminary_changed = ui
            .checkbox(&mut preliminary, "Vorbemerkung (steht in GAEB vor dem LV)")
            .changed();
        let mut closing = note.is_closing();
        let closing_changed = ui
            .checkbox(
                &mut closing,
                "Schlussbemerkung (steht in GAEB und im Druck nach dem LV)",
            )
            .on_hover_text(
                "Text nach dem LV, z. B. ein Schlussblatt mit Angaben des Bieters – gedruckt nach den Summen auf eigener Seite",
            )
            .changed();
        if heading_changed || preliminary_changed || closing_changed {
            self.undo
                .record(project.lv(), Some(EditKey(format!("note:{}", id.0))));
            if let Some(LvRow::Note(note)) = project.lv_mut().row_mut(index) {
                note.set_heading(heading);
                // Ticking one clears the other (a note is never both).
                if preliminary_changed {
                    note.set_preliminary(preliminary);
                }
                if closing_changed {
                    note.set_closing(closing);
                }
            }
            output.dirty = true;
        }
        if note.is_blank() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Hinweis ohne Text – für eine Leerzeile in der Bearbeitung den Abstandshalter verwenden.")
                        .size(12.0)
                        .color(palette.warn),
                );
                if ui
                    .button(Command::NoteToSpacer.label())
                    .on_hover_text("Leerzeile nur für die Bearbeitung, ohne Warnung – erscheint nicht im Druck (Strg+Z macht es rückgängig)")
                    .clicked()
                {
                    output.merge(self.notes_to_spacers(project, &[id], false));
                }
            });
        }
        ui.add_space(8.0);
    }

    pub(super) fn edit_position(
        &mut self,
        project: &mut Project,
        index: usize,
        field: &str,
        change: impl FnOnce(&mut Position),
    ) {
        let id = project.lv().rows()[index].id();
        self.undo
            .record(project.lv(), Some(EditKey(format!("{field}:{}", id.0))));
        if let Some(position) = project
            .lv_mut()
            .row_mut(index)
            .and_then(LvRow::as_position_mut)
        {
            change(position);
        }
    }

    /// „Weitere Angaben“ of a position, folded by default.
    pub(super) fn position_extra(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let mut summary = struct_ops::row_badges(project.lv(), &position)
            .into_iter()
            .filter(|badge| !proava_domain::is_performance_mark(badge))
            .collect::<Vec<_>>();
        if position.is_not_applicable() {
            summary.insert(0, "entfällt".to_string());
        }
        let title = if summary.is_empty() {
            "Weitere Angaben".to_string()
        } else {
            format!("Weitere Angaben ({})", summary.join(", "))
        };
        if let Some(origin) = position.text_origin() {
            // Textquelle (AUSSCHREIBEN.DE, STLB-Bau …): only shown here, never
            // exported or printed. The library link has its own line above
            // („Aus der Bibliothek …“, library_fields).
            let details = origin_details(origin);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Textquelle:")
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.label(
                    RichText::new(origin.summary())
                        .size(12.0)
                        .color(palette.text),
                )
                .on_hover_text(details);
                if ui
                    .small_button("entfernen")
                    .on_hover_text("Textquelle dieser Position vergessen (Text bleibt).")
                    .clicked()
                {
                    self.edit_position(project, index, "origin", |p| p.set_text_origin(None));
                    output.dirty = true;
                }
            });
            // „in freie Position umgewandelt am …“
            if !origin.note.trim().is_empty() {
                ui.label(
                    RichText::new(origin.note.trim())
                        .size(12.0)
                        .color(palette.text_dim),
                );
            }
            ui.add_space(4.0);
        }
        egui::CollapsingHeader::new(RichText::new(title).size(13.0))
            .id_salt("position_extra")
            .show(ui, |ui| {
                self.position_extra_fields(ui, palette, project, index, output)
            });
        ui.add_space(6.0);
    }

    /// The fields of „Weitere Angaben“ (also the tab „Weiteres“ of the
    /// window „Eigenschaften“): switches, Wiederholung, Zeitansatz,
    /// EP-Anteile, Textergänzungen, Unterbeschreibungen.
    pub(super) fn position_extra_fields(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        output: &mut EditorOutput,
    ) {
        let Some(position) = project.lv().rows()[index].as_position().cloned() else {
            return;
        };
        let id = position.id();
        let ctx = ui.ctx().clone();
        // Schalter
        let mut open = position.is_quantity_open();
        let mut gone = position.is_not_applicable();
        let mut key = position.is_key_item();
        let mut breakdown = position.has_price_breakdown();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut open, "Menge offen")
                .on_hover_text("Der Bieter trägt die Menge ein (GAEB QtyTBD).");
            ui.checkbox(&mut gone, "entfällt")
                .on_hover_text("Bleibt im LV, zählt in keiner Summe.");
            ui.checkbox(&mut key, "Schwerpunktposition");
            ui.checkbox(&mut breakdown, "EP aufgliedern")
                .on_hover_text("Der Bieter gliedert den EP auf (GAEB UPBkdn).");
        });
        if (open, gone, key, breakdown)
            != (
                position.is_quantity_open(),
                position.is_not_applicable(),
                position.is_key_item(),
                position.has_price_breakdown(),
            )
        {
            self.edit_position(project, index, "flags", |p| {
                p.set_quantity_open(open);
                p.set_not_applicable(gone);
                p.set_key_item(key);
                p.set_price_breakdown(breakdown);
            });
            output.dirty = true;
        }
        ui.add_space(8.0);

        // Wiederholung
        caption(ui, palette, "Wiederholung von (Bezugsposition)");
        let choices = struct_ops::repetition_choices(project.lv(), id);
        let shown = position
            .repeats()
            .and_then(|base| choices.iter().find(|(choice, _)| *choice == base))
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| "– keine –".to_string());
        let mut picked = None;
        egui::ComboBox::from_id_salt(("repeats", id))
            .width(ui.available_width().min(320.0))
            .selected_text(shown)
            .height(320.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(position.repeats().is_none(), "– keine –")
                    .clicked()
                {
                    picked = Some(None);
                }
                for (choice, text) in &choices {
                    if ui
                        .selectable_label(position.repeats() == Some(*choice), text.as_str())
                        .clicked()
                    {
                        picked = Some(Some(*choice));
                    }
                }
            });
        if let Some(base) = picked.filter(|base| *base != position.repeats()) {
            let before = project.lv().clone();
            match project.lv_mut().link_repetition(id, base) {
                Ok(()) => {
                    self.undo.record(&before, None);
                    output.dirty = true;
                }
                Err(error) => output.toast = Some(error),
            }
        }
        let repeating = project.lv().positions_repeating(id);
        if !repeating.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Wiederholt von {}:",
                    crate::format::count(repeating.len(), "Position", "Positionen")
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
            let mut jump = None;
            for other in &repeating {
                if let Some(other_position) = project.lv().position_by_id(*other) {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(format!(
                                    "{}  {}",
                                    other_position.ordinal(),
                                    other_position.short_text()
                                ))
                                .size(12.5),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        jump = Some(*other);
                    }
                }
            }
            if let Some(target) = jump {
                self.select(project.lv(), target);
            }
        }
        ui.add_space(8.0);

        // Zeitansatz und EP-Anteile
        let time_label = project.lv().time_label().to_string();
        caption(
            ui,
            palette,
            &if time_label.is_empty() {
                "Zeitansatz je Einheit".to_string()
            } else {
                format!("Zeitansatz je Einheit ({time_label})")
            },
        );
        let field = egui::Id::unique(("time_quantity", id, self.revision));
        let current = position
            .time_quantity()
            .map(numbers::quantity_input)
            .unwrap_or_default();
        let checked =
            fields::checked_field(ui, palette, field, &current, 120.0, FontId::monospace(13.0));
        if let Some(text) = checked.commit {
            match struct_ops::parse_optional_quantity(&text) {
                Ok(value) => {
                    self.edit_position(project, index, "time", |p| p.set_time_quantity(value));
                    fields::accept(&ctx, field);
                    output.dirty = true;
                }
                Err(error) => fields::refuse(&ctx, field, format!("Zeitansatz: {error}")),
            }
        }
        let columns = project.lv().price_components().to_vec();
        if !columns.is_empty() {
            ui.add_space(6.0);
            caption(
                ui,
                palette,
                "EP-Anteile (nur zur Information, es zählt der EP)",
            );
            egui::Grid::new(("price_components", id))
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for (column, component) in columns.iter().enumerate() {
                        ui.label(
                            RichText::new(if component.label.is_empty() {
                                format!("Anteil {}", column + 1)
                            } else {
                                component.label.clone()
                            })
                            .size(12.5),
                        );
                        let field = egui::Id::unique(("component", id, column, self.revision));
                        let current = numbers::price_input(
                            position.price_components().get(column).copied().flatten(),
                        );
                        let checked = fields::checked_field(
                            ui,
                            palette,
                            field,
                            &current,
                            120.0,
                            FontId::monospace(13.0),
                        );
                        if let Some(text) = checked.commit {
                            match numbers::parse_price(&text) {
                                Ok(value) => {
                                    let count = columns.len();
                                    self.edit_position(project, index, "component", |p| {
                                        let mut values = p.price_components().to_vec();
                                        values.resize(count.max(values.len()), None);
                                        values[column] = value;
                                        p.set_price_components(values);
                                    });
                                    fields::accept(&ctx, field);
                                    output.dirty = true;
                                }
                                Err(error) => fields::refuse(&ctx, field, error),
                            }
                        }
                        ui.end_row();
                    }
                });
        }
        ui.add_space(8.0);
        self.complements_editor(ui, palette, project, index, &position, output);
        self.sub_descriptions_editor(ui, palette, project, index, &position, output);
    }

    fn complements_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        position: &Position,
        output: &mut EditorOutput,
    ) {
        let id = position.id();
        caption(ui, palette, "Textergänzungen");
        if position.is_text_locked() {
            // STLB-Bau-Text: kind, Vortext and Nachtext belong to it; the
            // values are filled above the Langtext.
            ui.label(
                RichText::new(
                    "Aus STLB-Bau – Art, Vortext und Nachtext gehören zum Text; die Werte stehen über dem Langtext.",
                )
                .size(12.0)
                .color(palette.text_dim),
            );
            let _ = (project, index, output);
            return;
        }
        let mut list = position.complements().to_vec();
        let mut changed = false;
        let mut remove = None;
        for (number, complement) in list.iter_mut().enumerate() {
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    crate::widgets::row(ui, |ui| {
                        ui.label(RichText::new("Kennung").size(12.0).color(palette.text_dim));
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut complement.marker)
                                    .id(egui::Id::unique(("complement_marker", id, number)))
                                    .desired_width(36.0),
                            )
                            .changed();
                        egui::ComboBox::from_id_salt(("complement_kind", id, number))
                            .width(130.0)
                            .selected_text(kind_label(complement.kind))
                            .show_ui(ui, |ui| {
                                for kind in [ComplementKind::Bidder, ComplementKind::Owner] {
                                    if ui
                                        .selectable_label(complement.kind == kind, kind_label(kind))
                                        .clicked()
                                        && complement.kind != kind
                                    {
                                        complement.kind = kind;
                                        changed = true;
                                    }
                                }
                            });
                        if ui
                            .small_button("Entfernen")
                            .on_hover_text("Textergänzung entfernen")
                            .clicked()
                        {
                            remove = Some(number);
                        }
                    });
                    for (label, value, key, hint) in [
                        (
                            "Vortext",
                            &mut complement.caption,
                            "caption",
                            "z. B. Fabrikat:",
                        ),
                        ("Text", &mut complement.body, "body", ""),
                        ("Nachtext", &mut complement.tail, "tail", ""),
                    ] {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [56.0, 20.0],
                                egui::Label::new(
                                    RichText::new(label).size(12.0).color(palette.text_dim),
                                ),
                            );
                            changed |= fields::text_field(
                                ui,
                                value,
                                egui::Id::unique(("complement", key, id, number)),
                                hint,
                            );
                        });
                    }
                });
            ui.add_space(4.0);
        }
        if let Some(number) = remove {
            list.remove(number);
            changed = true;
        }
        if ui.button("+ Textergänzung").clicked() {
            list.push(TextComplement {
                marker: struct_ops::next_complement_marker(&list),
                kind: ComplementKind::Bidder,
                ..TextComplement::default()
            });
            changed = true;
        }
        for problem in struct_ops::complement_problems(&list) {
            ui.label(RichText::new(problem).size(12.0).color(palette.error));
        }
        if changed {
            self.edit_position(project, index, "complements", |p| p.set_complements(list));
            output.dirty = true;
        }
        ui.add_space(8.0);
    }

    fn sub_descriptions_editor(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        position: &Position,
        output: &mut EditorOutput,
    ) {
        if position.sub_descriptions().is_empty() {
            return;
        }
        let id = position.id();
        let ctx = ui.ctx().clone();
        caption(ui, palette, "Unterbeschreibungen");
        let mut list = position.sub_descriptions().to_vec();
        let mut changed = false;
        for (number, sub) in list.iter_mut().enumerate() {
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, palette.line))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut sub.number)
                                    .id(egui::Id::unique(("sub_number", id, number)))
                                    .desired_width(36.0),
                            )
                            .changed();
                        changed |= fields::text_field(
                            ui,
                            &mut sub.short_text,
                            egui::Id::unique(("sub_short", id, number)),
                            "Kurztext",
                        );
                    });
                    crate::widgets::row(ui, |ui| {
                        ui.label(RichText::new("Menge").size(12.0).color(palette.text_dim));
                        let field = egui::Id::unique(("sub_quantity", id, number, self.revision));
                        let current = sub
                            .quantity
                            .map(numbers::quantity_input)
                            .unwrap_or_default();
                        let checked = fields::checked_field(
                            ui,
                            palette,
                            field,
                            &current,
                            90.0,
                            FontId::monospace(13.0),
                        );
                        if let Some(text) = checked.commit {
                            match struct_ops::parse_optional_quantity(&text) {
                                Ok(value) => {
                                    sub.quantity = value;
                                    changed = true;
                                    fields::accept(&ctx, field);
                                }
                                Err(error) => fields::refuse(&ctx, field, error),
                            }
                        }
                        ui.label(RichText::new("Einheit").size(12.0).color(palette.text_dim));
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut sub.unit)
                                    .id(egui::Id::unique(("sub_unit", id, number)))
                                    .desired_width(50.0),
                            )
                            .changed();
                    });
                    let plain = sub.long_text.to_plain();
                    if !plain.trim().is_empty() {
                        ui.label(
                            RichText::new(plain.trim())
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                    }
                });
            ui.add_space(4.0);
        }
        if changed {
            self.edit_position(project, index, "subs", |p| p.set_sub_descriptions(list));
            output.dirty = true;
        }
    }
}

/// Tooltip of the Herkunft: every part on its own line.
fn origin_details(origin: &proava_domain::TextOrigin) -> String {
    [
        ("Quelle", origin.source.as_str()),
        ("Hersteller/Katalog", origin.supplier.as_str()),
        ("Abgerufen", origin.retrieved.as_str()),
        ("Datei", origin.file.as_str()),
        ("Artikelnummer", origin.reference.as_str()),
        ("Vermerk", origin.note.as_str()),
    ]
    .into_iter()
    .filter(|(_, value)| !value.trim().is_empty())
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n")
}

fn kind_label(kind: ComplementKind) -> &'static str {
    match kind {
        ComplementKind::Bidder => "vom Bieter",
        ComplementKind::Owner => "vom Auftraggeber",
    }
}
