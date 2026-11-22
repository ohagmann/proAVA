//! Ausführungsbeschreibungen in the edit panel: the description itself
//! (Nummer, Bezeichnung, Kurztext, Langtext, Verwendung, Löschen) and the
//! choice of a description at positions.

use std::collections::HashSet;

use eframe::egui::{self, FontId, RichText};
use proava_domain::{LvRow, PerformanceDescription, Project, RowId};

use super::fields::{self, caption};
use super::{Dialog, EditorOutput, LvEditor};
use crate::struct_ops::{self, PERFORMANCE_LABEL_LIMIT};
use crate::theme::Palette;
use crate::undo::EditKey;

/// Entry „Neue anlegen …“ of the choice at positions.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Keep,
    Set(Option<RowId>),
    New,
}

impl LvEditor {
    /// Number of an Ausführungsbeschreibung: „A0003“, counted automatically
    /// (Befund 7); „Von Hand …“ opens the field on purpose.
    pub(super) fn performance_number(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        performance: &PerformanceDescription,
        output: &mut EditorOutput,
        salt: &str,
    ) {
        let id = performance.id();
        let ctx = ui.ctx().clone();
        caption(ui, palette, "Nummer");
        // Automatic (Befund 7): 1, 2, 3 … by place; by hand only on purpose.
        let manual_id = egui::Id::unique(("performance_number_by_hand", id, salt));
        let mut manual = ui
            .data(|data| data.get_temp::<bool>(manual_id))
            .unwrap_or(false);
        if manual {
            let field = egui::Id::unique(("performance_number", id, salt, self.revision));
            let checked = fields::checked_field(
                ui,
                palette,
                field,
                &performance.shown_number(),
                80.0,
                FontId::monospace(13.0),
            );
            if let Some(text) = checked.commit {
                let before = project.lv().clone();
                let result = struct_ops::check_performance_number(&text)
                    .and_then(|number| project.lv_mut().set_performance_number(id, &number));
                match result {
                    Ok(()) => {
                        self.undo.record(&before, None);
                        fields::accept(&ctx, field);
                        output.dirty = true;
                        manual = false;
                    }
                    Err(error) => fields::refuse(&ctx, field, error),
                }
            }
        } else {
            ui.label(
                egui::RichText::new(performance.mark())
                    .font(FontId::monospace(14.0))
                    .color(palette.text),
            );
        }
        let hint = if project.lv().performances_in_order() {
            "zählt automatisch"
        } else {
            "eigene Nummern – „Neu nummerieren“ zählt wieder A0001, A0002 …"
        };
        ui.label(egui::RichText::new(hint).size(11.0).color(palette.text_dim));
        let toggle = if manual { "Fertig" } else { "Von Hand …" };
        if ui
            .small_button(toggle)
            .on_hover_text("Nummer von Hand vergeben; danach zählt proAVA erst nach „Neu nummerieren“ wieder selbst.")
            .clicked()
        {
            manual = !manual;
        }
        ui.data_mut(|data| data.insert_temp(manual_id, manual));
    }

    pub(super) fn performance_detail(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        index: usize,
        performance: &PerformanceDescription,
        output: &mut EditorOutput,
    ) {
        let id = performance.id();
        if let Some(kind) = super::detail_heading_with_kind(
            ui,
            palette,
            "Ausführungsbeschreibung",
            &performance.mark(),
            &[],
            (proava_domain::RowKind::Performance, id, true),
        ) {
            // The rest of the panel belongs to the old kind: next frame.
            output.merge(self.request_conversion(project, index, kind));
            return;
        }

        crate::widgets::row(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(130.0);
                self.performance_number(ui, palette, project, performance, output, "panel");
            });
            ui.vertical(|ui| {
                caption(ui, palette, "Bezeichnung");
                let mut label = performance.label().to_string();
                let focus = self.focus.take_if(|focus| *focus == super::Focus::Short);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut label)
                        .id(egui::Id::unique(("performance_label", id)))
                        .hint_text("z. B. Kabeltrassen")
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(14.0))
                        .margin(egui::Margin::symmetric(6, 5)),
                );
                if focus.is_some() {
                    response.request_focus();
                }
                if response.changed() {
                    self.edit_performance(project, index, "label", |p| p.set_label(label));
                    output.dirty = true;
                }
                let length = performance.label().chars().count();
                if length > PERFORMANCE_LABEL_LIMIT {
                    ui.label(
                        RichText::new(format!(
                            "{length}/{PERFORMANCE_LABEL_LIMIT} – GAEB kürzt längere Bezeichnungen"
                        ))
                        .size(11.5)
                        .color(palette.warn),
                    );
                }
            });
        });
        ui.add_space(8.0);
        caption(ui, palette, "Kurztext (optional)");
        let mut short = performance.short_text().to_string();
        if fields::text_field(
            ui,
            &mut short,
            egui::Id::unique(("performance_short", id)),
            "",
        ) {
            self.edit_performance(project, index, "short", |p| p.set_short_text(short));
            output.dirty = true;
        }
        ui.add_space(10.0);

        let users = project.lv().positions_using(id);
        caption(
            ui,
            palette,
            &match users.len() {
                0 => "Noch von keiner Position verwendet".to_string(),
                1 => "Verwendet von 1 Position".to_string(),
                count => format!("Verwendet von {count} Positionen"),
            },
        );
        let mut jump = None;
        egui::ScrollArea::vertical()
            .id_salt(("performance_users", id))
            .max_height(140.0)
            .show(ui, |ui| {
                for user in &users {
                    if let Some(position) = project.lv().position_by_id(*user) {
                        let text = format!("{}  {}", position.ordinal(), position.short_text());
                        if ui
                            .add(egui::Button::new(RichText::new(text).size(12.5)).frame(false))
                            .on_hover_text("Zur Position springen")
                            .clicked()
                        {
                            jump = Some(*user);
                        }
                    }
                }
            });
        if let Some(target) = jump {
            self.select(project.lv(), target);
        }
        ui.add_space(4.0);
        if ui.button("Ausführungsbeschreibung löschen …").clicked() {
            self.dialog = Some(Dialog::RemovePerformance {
                id,
                users: users.len(),
            });
        }
        ui.add_space(12.0);
        caption(ui, palette, "Langtext");
        self.long_text_section(
            ui,
            palette,
            project,
            index,
            performance.long_text().clone(),
            output,
        );
    }

    fn edit_performance(
        &mut self,
        project: &mut Project,
        index: usize,
        field: &str,
        change: impl FnOnce(&mut PerformanceDescription),
    ) {
        let id = project.lv().rows()[index].id();
        self.undo
            .record(project.lv(), Some(EditKey(format!("{field}:{}", id.0))));
        if let Some(LvRow::Performance(performance)) = project.lv_mut().row_mut(index) {
            change(performance);
        }
    }

    /// Choice of the Ausführungsbeschreibung for the positions `ids`
    /// (one position or a multiple selection).
    pub(super) fn performance_choice(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        ids: &HashSet<RowId>,
        output: &mut EditorOutput,
    ) {
        let lv = project.lv();
        let positions: Vec<RowId> = lv
            .positions()
            .filter(|position| ids.contains(&position.id()))
            .map(|position| position.id())
            .collect();
        let Some(first) = positions.first().and_then(|id| lv.position_by_id(*id)) else {
            return;
        };
        let current = first.performance();
        let same = positions.iter().all(|id| {
            lv.position_by_id(*id)
                .is_some_and(|p| p.performance() == current)
        });
        let choices = struct_ops::performance_choices(lv);
        let shown = if !same {
            "verschieden".to_string()
        } else {
            current
                .and_then(|id| choices.iter().find(|(choice, _)| *choice == id))
                .map(|(_, text)| text.clone())
                .unwrap_or_else(|| "– keine –".to_string())
        };
        caption(ui, palette, "Ausführungsbeschreibung");
        let mut pick = Pick::Keep;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("performance_pick", positions.len(), first.id()))
                .width((ui.available_width() - 40.0).clamp(120.0, 320.0))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(same && current.is_none(), "– keine –")
                        .clicked()
                    {
                        pick = Pick::Set(None);
                    }
                    for (id, text) in &choices {
                        if ui
                            .selectable_label(same && current == Some(*id), text.as_str())
                            .clicked()
                        {
                            pick = Pick::Set(Some(*id));
                        }
                    }
                    ui.separator();
                    if ui.selectable_label(false, "Neue anlegen …").clicked() {
                        pick = Pick::New;
                    }
                });
            if let (true, Some(target)) = (same, current) {
                if crate::widgets::tool_button(
                    ui,
                    palette,
                    crate::icons::Icon::Forward,
                    false,
                    "Zur Ausführungsbeschreibung",
                )
                .clicked()
                {
                    self.select(project.lv(), target);
                }
            }
        });
        let ctx = ui.ctx().clone();
        match pick {
            Pick::Keep => {}
            Pick::Set(target) => output.merge(self.run_struct(
                super::context_menu::StructCommand::Link(target),
                project,
                &ctx,
            )),
            Pick::New => output.merge(self.run_struct(
                super::context_menu::StructCommand::LinkNew,
                project,
                &ctx,
            )),
        }
        ui.add_space(8.0);
    }

    /// „Ausführung nach A0001 – Kabeltrassen“ near the OZ of a position
    /// (edit panel and tab „Allgemein“ of the window „Eigenschaften“): the
    /// link is seen at once. „Anzeigen“ selects the description, „Ändern …“
    /// offers the same choice as the field „Ausführungsbeschreibung“,
    /// „Lösen“ removes the link (Strg+Z brings it back). Without a link a
    /// quiet line with „Verknüpfen …“.
    pub(super) fn performance_link_line(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: RowId,
        output: &mut EditorOutput,
    ) {
        let lv = project.lv();
        let Some(position) = lv.position_by_id(id) else {
            return;
        };
        let current = lv.performance_of(position).map(PerformanceDescription::id);
        let choices = struct_ops::performance_choices(lv);
        let linked = current.and_then(|target| {
            choices
                .iter()
                .find(|(choice, _)| *choice == target)
                .map(|(_, text)| (target, text.clone()))
        });
        let mut pick = Pick::Keep;
        let mut show: Option<RowId> = None;
        let (fill, line) = if linked.is_some() {
            (palette.accent_soft, palette.accent.gamma_multiply(0.45))
        } else {
            (egui::Color32::TRANSPARENT, palette.line)
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, line))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match &linked {
                    Some((target, text)) => {
                        let mut job = egui::text::LayoutJob::default();
                        let style = ui.style().clone();
                        for (part, size, color, strong) in [
                            ("Ausführung nach ", 12.5, palette.text_dim, false),
                            (text.as_str(), 13.5, palette.text, true),
                        ] {
                            let mut rich = RichText::new(part).size(size).color(color);
                            if strong {
                                rich = rich.strong();
                            }
                            rich.append_to(
                                &mut job,
                                &style,
                                egui::FontSelection::Default,
                                egui::Align::Center,
                            );
                        }
                        // Text and buttons on one middle line when there is room
                        // (window), else the text above the buttons (narrow panel).
                        let text_width = ui.painter().layout_job(job.clone()).size().x;
                        let buttons: f32 = ["Anzeigen", "Ändern …", "Lösen"]
                            .iter()
                            .map(|label| {
                                ui.painter()
                                    .layout_no_wrap(
                                        (*label).to_string(),
                                        FontId::proportional(12.5),
                                        palette.text,
                                    )
                                    .size()
                                    .x
                                    + 40.0
                                    + ui.spacing().item_spacing.x
                            })
                            .sum();
                        let one_line = text_width + 16.0 + buttons <= ui.available_width();
                        let hover = "Diese Position verweist auf die Ausführungsbeschreibung.";
                        let mut buttons_ui = |ui: &mut egui::Ui| {
                            if crate::widgets::icon_label_button(
                                ui,
                                palette,
                                crate::icons::Icon::Forward,
                                "Anzeigen",
                            )
                            .on_hover_text("Zur Ausführungsbeschreibung in der Tabelle")
                            .clicked()
                            {
                                show = Some(*target);
                            }
                            let change = crate::widgets::icon_label_button(
                                ui,
                                palette,
                                crate::icons::Icon::Link,
                                "Ändern …",
                            )
                            .on_hover_text("Mit einer anderen Ausführungsbeschreibung verknüpfen");
                            link_menu(&change, &choices, current, &mut pick);
                            if crate::widgets::icon_label_button(
                                ui,
                                palette,
                                crate::icons::Icon::Close,
                                "Lösen",
                            )
                            .on_hover_text("Verknüpfung lösen – Strg+Z macht es rückgängig")
                            .clicked()
                            {
                                pick = Pick::Set(None);
                            }
                        };
                        if one_line {
                            crate::widgets::row(ui, |ui| {
                                ui.add(egui::Label::new(job).extend()).on_hover_text(hover);
                                ui.add_space(8.0);
                                buttons_ui(ui);
                            });
                        } else {
                            ui.add(egui::Label::new(job).wrap()).on_hover_text(hover);
                            ui.add_space(2.0);
                            crate::widgets::row_wrapped(ui, buttons_ui);
                        }
                    }
                    None => {
                        crate::widgets::row_wrapped(ui, |ui| {
                            ui.label(
                                RichText::new("keine Ausführungsbeschreibung")
                                    .size(12.0)
                                    .color(palette.text_dim),
                            );
                            let link = crate::widgets::icon_label_button(
                                ui,
                                palette,
                                crate::icons::Icon::Link,
                                "Verknüpfen …",
                            )
                            .on_hover_text("Mit einer Ausführungsbeschreibung verknüpfen");
                            link_menu(&link, &choices, None, &mut pick);
                        });
                    }
                }
            });
        if let Some(target) = show {
            self.select(project.lv(), target);
        }
        let ctx = ui.ctx().clone();
        match pick {
            Pick::Keep => {}
            Pick::Set(target) => {
                // Only this position, whatever else the table has selected.
                let before = project.lv().clone();
                match struct_ops::link_many(project.lv_mut(), &[id], target) {
                    Ok(0) => {}
                    Ok(_) => {
                        self.undo.record(&before, None);
                        output.dirty = true;
                        output.toast = Some(
                            match target {
                                Some(_) => "Verknüpft – Strg+Z macht es rückgängig.",
                                None => "Verknüpfung gelöst – Strg+Z macht es rückgängig.",
                            }
                            .to_string(),
                        );
                    }
                    Err(error) => output.toast = Some(error),
                }
            }
            Pick::New => output.merge(self.run_struct(
                super::context_menu::StructCommand::LinkNew,
                project,
                &ctx,
            )),
        }
        ui.add_space(8.0);
    }
}

/// The choice of „Ändern …“/„Verknüpfen …“: every description of the LV,
/// then „Neue anlegen …“.
fn link_menu(
    button: &egui::Response,
    choices: &[(RowId, String)],
    current: Option<RowId>,
    pick: &mut Pick,
) {
    egui::Popup::menu(button).show(|ui| {
        if choices.is_empty() {
            ui.label(RichText::new("Im LV gibt es noch keine Ausführungsbeschreibung.").size(12.0));
        }
        for (choice, text) in choices {
            if ui
                .selectable_label(current == Some(*choice), text.as_str())
                .clicked()
            {
                *pick = Pick::Set(Some(*choice));
            }
        }
        ui.separator();
        if ui.selectable_label(false, "Neue anlegen …").clicked() {
            *pick = Pick::New;
        }
    });
}
