//! Kostengruppen in the editor: the field „Kostengruppe“ (titles,
//! positions, several rows) and the window „Kosten nach Kostengruppen“.

use eframe::egui::{self, RichText};
use proava_application::PdfDocument;
use proava_domain::{
    cost_group_label, cost_group_report, parse_cost_group, LvRow, Project, RowId, DIN276_KG400,
};

use super::{Dialog, EditorOutput, LvEditor};
use crate::columns::Column;
use crate::theme::Palette;
use crate::widgets;

/// Text typed into the field „Kostengruppe“: for which rows and starting
/// from which value (a change from elsewhere – Strg+Z, the question below,
/// another program – replaces the text).
pub(super) struct CostGroupInput {
    ids: Vec<RowId>,
    base: Option<Option<String>>,
    text: String,
}

/// A cost group (or Leistungsbereich) change that waits for the answer to
/// „n fertige Positionen erben die neue KG“.
pub(super) struct PendingCostGroup {
    /// [`Column::CostGroup`] or [`Column::WorkCategory`].
    column: Column,
    ids: Vec<RowId>,
    code: Option<String>,
    /// Finished positions whose cost group would change.
    heirs: Vec<RowId>,
    /// Pass in which the question came up: the Enter that left the field
    /// must not answer it.
    asked_in: u64,
}

impl LvEditor {
    /// Field „Kostengruppe“ for the rows `ids` (titles and positions):
    /// free input (three digits) or a choice from DIN 276 KG 400.
    pub(super) fn cost_group_field(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        ids: &[RowId],
        output: &mut EditorOutput,
    ) {
        let lv = project.lv();
        let own: Vec<Option<String>> = ids
            .iter()
            .filter_map(|id| lv.row_by_id(*id))
            .filter_map(|row| match row {
                LvRow::Position(position) => Some(position.cost_group().map(str::to_string)),
                LvRow::Group(group) => Some(group.cost_group().map(str::to_string)),
                _ => None,
            })
            .collect();
        if own.is_empty() {
            return;
        }
        let common = own
            .iter()
            .all(|code| *code == own[0])
            .then(|| own[0].clone());
        // Shown as hint when every row inherits the same group from its title.
        let effective: Vec<Option<(String, bool)>> = ids
            .iter()
            .filter_map(|id| lv.index_of(*id))
            .map(|index| lv.effective_cost_group(index))
            .collect();
        let inherited = match effective.first() {
            Some(Some((code, true)))
                if effective
                    .iter()
                    .all(|other| other.as_ref() == Some(&(code.clone(), true))) =>
            {
                Some(code.clone())
            }
            _ => None,
        };

        let mut key: Vec<RowId> = ids.to_vec();
        key.sort();
        if self
            .cost_group_input
            .as_ref()
            .is_none_or(|input| input.ids != key || input.base != common)
        {
            self.cost_group_input = Some(CostGroupInput {
                ids: key.clone(),
                base: common.clone(),
                text: common.clone().flatten().unwrap_or_default(),
            });
        }
        let mut chosen: Option<Option<String>> = None;
        ui.label(
            RichText::new("Kostengruppe (DIN 276)")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            let Some(CostGroupInput { text, .. }) = self.cost_group_input.as_mut() else {
                return;
            };
            let hint = match (&common, &inherited) {
                (None, _) => "verschieden".to_string(),
                (_, Some(code)) => format!("{code} (vom Titel)"),
                _ => "z. B. 481".to_string(),
            };
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    // Per place: the Eigenschaftenfenster and the
                    // Positionsfenster may show the same rows at once.
                    .id(ui.scope_id().with(("cost_group", key.first().copied())))
                    .hint_text(hint)
                    .desired_width(110.0)
                    .margin(egui::Margin::symmetric(6, 5)),
            );
            if response.lost_focus() {
                let typed = text.trim().to_string();
                if Some(typed.clone()) != common.clone().map(|code| code.unwrap_or_default()) {
                    match parse_cost_group(&typed) {
                        Ok(code) => chosen = Some(code),
                        Err(error) => output.toast = Some(error),
                    }
                }
            }
            let current = common.clone().flatten();
            egui::ComboBox::from_id_salt(("cost_group_list", key.first().copied()))
                .width(ui.available_width().min(260.0))
                .selected_text(match &current {
                    Some(code) => cost_group_label(code).to_string(),
                    None => "Liste …".to_string(),
                })
                .height(360.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current.is_none(), "– keine eigene –")
                        .clicked()
                    {
                        chosen = Some(None);
                    }
                    for (code, label) in DIN276_KG400 {
                        let group = code.ends_with('0');
                        let text = if group {
                            RichText::new(format!("{code}  {label}"))
                                .strong()
                                .color(palette.text)
                        } else {
                            RichText::new(format!("    {code}  {label}"))
                        };
                        if ui
                            .selectable_label(current.as_deref() == Some(*code), text)
                            .clicked()
                        {
                            chosen = Some(Some(code.to_string()));
                        }
                    }
                });
        });
        if let Some(code) = chosen {
            self.cost_group_input = None;
            let pass = ui.ctx().cumulative_pass_nr();
            output.merge(self.set_cost_groups(project, ids, code, Some(pass)));
        }
        ui.add_space(6.0);
    }

    /// Sets the cost group of titles and positions; finished positions are
    /// skipped and counted. `ask` (the current pass): first ask when finished
    /// positions would inherit a title's new group; None = already answered.
    pub(super) fn set_cost_groups(
        &mut self,
        project: &mut Project,
        ids: &[RowId],
        code: Option<String>,
        ask: Option<u64>,
    ) -> EditorOutput {
        self.set_title_values(project, Column::CostGroup, ids, code, ask)
    }

    /// [`LvEditor::set_cost_groups`] for the column `column`: Kostengruppe
    /// or Leistungsbereich (both pass from a title down to its rows).
    pub(super) fn set_title_values(
        &mut self,
        project: &mut Project,
        column: Column,
        ids: &[RowId],
        code: Option<String>,
        ask: Option<u64>,
    ) -> EditorOutput {
        let work = column == Column::WorkCategory;
        if let Some(pass) = ask {
            let lv = project.lv();
            let titles: Vec<usize> = ids.iter().filter_map(|id| lv.index_of(*id)).collect();
            let heirs = if work {
                lv.locked_work_category_heirs(&titles, code.as_deref())
            } else {
                lv.locked_cost_group_heirs(&titles, code.as_deref())
            };
            if !heirs.is_empty() {
                self.dialog = Some(Dialog::ConfirmCostGroup(PendingCostGroup {
                    column,
                    ids: ids.to_vec(),
                    code,
                    heirs,
                    asked_in: pass,
                }));
                return EditorOutput::default();
            }
        }
        let before = project.lv().clone();
        let mut changed = 0;
        let mut locked = 0;
        for id in ids {
            let Some(index) = project.lv().index_of(*id) else {
                continue;
            };
            if project.lv().rows()[index].is_position()
                && project.lv().ensure_editable(&[*id]).is_err()
            {
                locked += 1;
                continue;
            }
            match project.lv_mut().row_mut(index) {
                Some(LvRow::Position(position)) => {
                    if work {
                        position.set_work_category(code.clone());
                    } else {
                        position.set_cost_group(code.clone());
                    }
                    changed += 1;
                }
                Some(LvRow::Group(group)) => {
                    if work {
                        group.set_work_category(code.clone());
                    } else {
                        group.set_cost_group(code.clone());
                    }
                    changed += 1;
                }
                _ => {}
            }
        }
        if changed == 0 {
            return EditorOutput {
                toast: Some(if locked > 0 {
                    format!(
                        "Nichts geändert: als fertig markiert – {}.",
                        proava_domain::LOCKED_HINT
                    )
                } else if work {
                    "Leistungsbereiche gibt es nur an Titeln und Positionen.".to_string()
                } else {
                    "Kostengruppen gibt es nur an Titeln und Positionen.".to_string()
                }),
                ..EditorOutput::default()
            };
        }
        self.undo.record(&before, None);
        let what = match (&code, work) {
            (Some(code), false) => format!("KG {code}"),
            (Some(code), true) => format!("LB {}", proava_domain::work_category_text(code)),
            (None, false) => "keine eigene KG".to_string(),
            (None, true) => "kein eigener LB".to_string(),
        };
        let mut toast = if changed == 1 {
            format!("{what} gesetzt.")
        } else {
            format!("{what} für {changed} Zeilen gesetzt.")
        };
        if locked > 0 {
            toast.push_str(&format!(
                " {} übersprungen.",
                crate::format::count(locked, "fertige Position", "fertige Positionen")
            ));
        }
        EditorOutput {
            dirty: true,
            toast: Some(toast),
            ..EditorOutput::default()
        }
    }

    /// Question „n fertige Positionen erben die neue KG“; true while it is
    /// the open dialog.
    pub(super) fn cost_group_dialog(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) -> bool {
        let Some(Dialog::ConfirmCostGroup(pending)) = &self.dialog else {
            return false;
        };
        let lv = project.lv();
        let work = pending.column == Column::WorkCategory;
        let question = if work {
            proava_domain::work_category_heirs_question(
                pending.heirs.len(),
                pending.code.as_deref(),
            )
        } else {
            proava_domain::cost_group_heirs_question(pending.heirs.len(), pending.code.as_deref())
        };
        let mut names: Vec<String> = pending
            .heirs
            .iter()
            .take(6)
            .filter_map(|id| lv.row_by_id(*id))
            .map(|row| format!("{}  {}", row.ordinal(), row.short_text().trim()))
            .collect();
        if pending.heirs.len() > names.len() {
            names.push(format!(
                "… und {} weitere",
                pending.heirs.len() - names.len()
            ));
        }
        let enter = ui.ctx().cumulative_pass_nr() > pending.asked_in
            && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 500.0, |ui| {
            widgets::title(
                ui,
                palette,
                if work {
                    "Leistungsbereich des Titels ändern?"
                } else {
                    "Kostengruppe des Titels ändern?"
                },
                &format!("{question} Ihr Status bleibt „fertig“. Strg+Z macht es rückgängig."),
            );
            for name in &names {
                ui.label(RichText::new(name).size(12.5).color(palette.text_dim));
            }
            ui.add_space(10.0);
            widgets::dialog_buttons(ui, palette, if work { "LB ändern" } else { "KG ändern" })
        });
        if close || cancelled {
            self.dialog = None;
        } else if confirmed || enter {
            if let Some(Dialog::ConfirmCostGroup(pending)) = self.dialog.take() {
                output.merge(self.set_title_values(
                    project,
                    pending.column,
                    &pending.ids,
                    pending.code,
                    None,
                ));
            }
        }
        true
    }

    /// Window „Kosten nach Kostengruppen“.
    pub(super) fn cost_groups_window(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &Project,
        output: &mut EditorOutput,
    ) {
        if !self.cost_groups_open {
            return;
        }
        let lv = project.lv();
        let report = cost_group_report(lv, project.bids());
        // Esc closes it right after opening (Anwender 18.11.: no click first).
        let mut open = !crate::widgets::esc_closes_window(ctx);
        let mut pdf = false;
        egui::Window::new("Kosten nach Kostengruppen")
            .default_size([620.0, 480.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Katalog {} · Positionen ohne eigene KG zählen zu der ihres Titels · Angebote netto vor Nachlass",
                        lv.cost_catalog().name
                    ))
                    .size(12.0)
                    .color(palette.text_dim),
                );
                if !lv.has_cost_groups() {
                    ui.label(
                        RichText::new(
                            "Noch keine Kostengruppe im LV – im Eigenschaftenfenster an Titeln oder Positionen setzen.",
                        )
                        .size(12.5)
                        .color(palette.warn),
                    );
                }
                ui.add_space(6.0);
                egui::ScrollArea::both().auto_shrink([false, false]).max_height(380.0).show(ui, |ui| {
                    egui::Grid::new("cost_groups_grid")
                        .striped(true)
                        .min_col_width(36.0)
                        .spacing([14.0, 4.0])
                        .show(ui, |ui| {
                            let head = |ui: &mut egui::Ui, text: &str, right: bool| {
                                cell(ui, palette, text, CellStyle::Head, right);
                            };
                            head(ui, "KG", false);
                            head(ui, "Bezeichnung", false);
                            head(ui, "Pos.", true);
                            head(ui, "Planer", true);
                            for bidder in &report.bidders {
                                head(ui, bidder, true);
                            }
                            ui.end_row();
                            for line in &report.lines {
                                let style = if line.subtotal { CellStyle::Sum } else { CellStyle::Plain };
                                let code = line.code.clone().unwrap_or_else(|| "–".to_string());
                                let label = if line.subtotal {
                                    format!("Summe {}", line.label)
                                } else {
                                    line.label.clone()
                                };
                                cell(ui, palette, &code, style, false);
                                cell(ui, palette, &label, style, false);
                                cell(ui, palette, &line.positions.to_string(), style, true);
                                cell(ui, palette, &line.planner.plain(), style, true);
                                for value in &line.bidders {
                                    cell(ui, palette, &value.plain(), style, true);
                                }
                                ui.end_row();
                            }
                            cell(ui, palette, "", CellStyle::Sum, false);
                            cell(ui, palette, "Gesamt", CellStyle::Sum, false);
                            cell(ui, palette, &lv.position_count().to_string(), CellStyle::Sum, true);
                            cell(ui, palette, &report.planner_total.plain(), CellStyle::Sum, true);
                            for value in &report.bidder_totals {
                                cell(ui, palette, &value.plain(), CellStyle::Sum, true);
                            }
                            ui.end_row();
                        });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Drucken …").on_hover_text("Druckdialog mit „Kosten nach Kostengruppen“").clicked() {
                        pdf = true;
                    }
                    if ui.button("Als Text kopieren").clicked() {
                        ctx.copy_text(report_text(&report));
                        output.toast = Some("Kosten nach Kostengruppen kopiert.".into());
                    }
                    if ui.button("Schließen").clicked() {
                        open = false;
                    }
                });
            });
        if pdf {
            output.pdf = Some(PdfDocument::CostGroups);
        }
        if !open {
            self.cost_groups_open = false;
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CellStyle {
    Head,
    Plain,
    Sum,
}

/// One cell of the table „Kosten nach Kostengruppen“; numbers right aligned
/// in the font of the LV table.
fn cell(ui: &mut egui::Ui, palette: &Palette, text: &str, style: CellStyle, right: bool) {
    let font = match (style, right) {
        (CellStyle::Head, _) => crate::fonts::bold(12.0),
        (CellStyle::Sum, _) => crate::fonts::bold(12.5),
        (CellStyle::Plain, false) => egui::FontId::proportional(12.5),
        (CellStyle::Plain, true) => egui::FontId::monospace(12.5),
    };
    let color = if style == CellStyle::Head {
        palette.text_dim
    } else {
        palette.text
    };
    let text = RichText::new(text).font(font).color(color);
    if right {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(text);
        });
    } else {
        ui.label(text);
    }
}

/// Tab-separated text for the clipboard.
fn report_text(report: &proava_domain::CostGroupReport) -> String {
    let mut lines = vec![{
        let mut head = "KG\tBezeichnung\tPos.\tPlaner".to_string();
        for bidder in &report.bidders {
            head.push('\t');
            head.push_str(bidder);
        }
        head
    }];
    for line in &report.lines {
        let mut text = format!(
            "{}\t{}{}\t{}\t{}",
            line.code.clone().unwrap_or_default(),
            if line.subtotal { "Summe " } else { "" },
            line.label,
            line.positions,
            line.planner.plain()
        );
        for value in &line.bidders {
            text.push('\t');
            text.push_str(&value.plain());
        }
        lines.push(text);
    }
    lines.join("\n")
}
