//! Window „Aufmaß“ of one position: takeoff lines with place and
//! calculation, assigned to an invoice (Zuwachs there) or not yet.

use eframe::egui::{self, RichText};
use proava_domain::{InvoiceId, Project, TakeoffLine};

use super::{qty, BillingView};
use crate::theme::Palette;
use crate::view::ViewResponse;

impl BillingView {
    pub(super) fn takeoff_window(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: InvoiceId,
        response: &mut ViewResponse,
    ) {
        let Some(row) = self.takeoff else {
            return;
        };
        let title = project
            .lv()
            .row_by_id(row)
            .map(|lv_row| format!("Aufmaß – {} {}", lv_row.ordinal(), lv_row.short_text()))
            .unwrap_or_else(|| "Aufmaß".into());
        let invoices: Vec<(InvoiceId, String)> = project
            .billing()
            .invoices()
            .iter()
            .map(|invoice| {
                let label = self
                    .check(invoice.id)
                    .map(|check| check.label.clone())
                    .unwrap_or_else(|| invoice.kind.label().to_string());
                (
                    invoice.id,
                    format!("{label} {}", invoice.number).trim().to_string(),
                )
            })
            .collect();
        let unit = project
            .lv()
            .row_by_id(row)
            .and_then(|lv_row| lv_row.as_position())
            .map(|position| position.unit().to_string())
            .unwrap_or_default();
        let mut close = false;
        let mut changed = false;
        egui::Window::new(title)
            .id(egui::Id::unique("billing_takeoff"))
            .default_size([800.0, 380.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ui.ctx().content_rect().center())
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                ui.label(
                    RichText::new(
                        "Je Zeile Ort oder Bezeichnung und Rechenansatz (z. B. 2*12,5+3). Zeilen der gewählten Rechnung zählen dort als Zuwachs.",
                    )
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(6.0);
                let (sum, errors) = project.billing().takeoff_sum(row, Some(id));
                let (open_sum, _) = project.billing().takeoff_sum(row, None);
                let lines = project.billing_mut().takeoff_mut(row);
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("billing_takeoff_grid")
                            .num_columns(5)
                            .striped(true)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                for text in ["Bezeichnung / Ort", "Rechenansatz", "Ergebnis", "Rechnung", ""] {
                                    ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
                                }
                                ui.end_row();
                                for (index, line) in lines.iter_mut().enumerate() {
                                    changed |= ui
                                        .add_sized([240.0, 22.0], egui::TextEdit::singleline(&mut line.label))
                                        .changed();
                                    changed |= ui
                                        .add_sized(
                                            [200.0, 22.0],
                                            egui::TextEdit::singleline(&mut line.formula)
                                                .font(egui::TextStyle::Monospace),
                                        )
                                        .changed();
                                    match line.result() {
                                        Ok(value) => ui.label(RichText::new(format!("{} {unit}", qty(value))).monospace()),
                                        Err(error) => ui.label(RichText::new("Fehler").color(palette.error)).on_hover_text(error),
                                    };
                                    let shown = match line.invoice {
                                        None => "nicht zugeordnet".to_string(),
                                        Some(target) => invoices
                                            .iter()
                                            .find(|(invoice, _)| *invoice == target)
                                            .map(|(_, label)| label.clone())
                                            .unwrap_or_else(|| "nicht zugeordnet".into()),
                                    };
                                    let mut target = line.invoice;
                                    egui::ComboBox::from_id_salt(("takeoff_invoice", index))
                                        .selected_text(shown)
                                        .width(180.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut target, None, "nicht zugeordnet");
                                            for (invoice, label) in &invoices {
                                                ui.selectable_value(&mut target, Some(*invoice), label);
                                            }
                                        });
                                    if target != line.invoice {
                                        line.invoice = target;
                                        changed = true;
                                    }
                                    if ui.button("Löschen").clicked() {
                                        remove = Some(index);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(index) = remove {
                    lines.remove(index);
                    changed = true;
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Zeile hinzufügen").clicked() {
                        lines.push(TakeoffLine {
                            label: String::new(),
                            formula: String::new(),
                            invoice: Some(id),
                        });
                        changed = true;
                    }
                    ui.label(
                        RichText::new(format!(
                            "Summe dieser Rechnung: {} {unit}{}",
                            qty(sum),
                            if open_sum.is_zero() { String::new() } else { format!(" · nicht zugeordnet: {} {unit}", qty(open_sum)) }
                        ))
                        .font(crate::fonts::bold(13.0))
                        .color(palette.text),
                    );
                    if !errors.is_empty() {
                        ui.label(RichText::new(format!("{} mit Fehler", crate::format::count(errors.len(), "Zeile", "Zeilen"))).color(palette.error));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Schließen").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if changed {
            response.dirty = true;
        }
        if close || ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
            self.takeoff = None;
        }
    }
}
