//! Datenpunkt-Abschnitt eines Kosten-LV (GU-Fall): ein Titel, der statt
//! Positionen die Datenpunktrechnung der Gebäudeautomation trägt (KG 480).
//!
//! Einfügen ▸ „Datenpunkt-Abschnitt einfügen (KG 480)“ legt ihn an; ein
//! Doppelklick auf den Titel öffnet die Anlagenliste in einem Fenster über dem
//! LV – dieselbe Ansicht wie im Reiter Kosten (DP-Arten, Bauteile, Ergebnis,
//! Aufteilung auf 481–489), ohne Bezeichnung und Stand (die des Kosten-LV).
//! Jede Änderung ist ein Schritt im Rückgängig des LV (Strg+Z).

use eframe::egui::{self, RichText};
use proava_domain::{CostEstimate, CostStage, Lv, LvRow, Project, RowId};

use super::{EditorOutput, LvEditor};
use crate::costs_view::{CostsOutput, CostsView};
use crate::theme::Palette;
use crate::undo::EditKey;

/// The open window of one Datenpunkt-Abschnitt.
pub(super) struct PointWindow {
    pub(super) row: RowId,
    view: CostsView,
}

impl PointWindow {
    fn new(row: RowId) -> Self {
        Self {
            row,
            view: CostsView::for_section(),
        }
    }

    /// The LV changed from outside (undo): drop a cell edit.
    pub(super) fn reload(&mut self) {
        self.view.reset_editing();
    }
}

/// Title of the window: „Datenpunkte – 480 Gebäude- und Anlagenautomation“.
fn window_title(lv: &Lv, index: usize) -> String {
    let row = &lv.rows()[index];
    let name = [row.ordinal().trim(), row.short_text().trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    format!("Datenpunkte – {name}")
}

impl LvEditor {
    /// Double click or a jump with „open“: the plant list for a
    /// Datenpunkt-Abschnitt, the window „Eigenschaften“ for everything else.
    pub(super) fn open_row_window(&mut self, lv: &Lv, row: RowId) {
        if lv.point_section(row).is_some() {
            self.open_point_window(row);
        } else {
            self.open_position_window(row);
        }
    }

    /// Opens the plant list of the Datenpunkt-Abschnitt `row`.
    pub(super) fn open_point_window(&mut self, row: RowId) {
        if self
            .point_window
            .as_ref()
            .is_none_or(|window| window.row != row)
        {
            self.point_window = Some(PointWindow::new(row));
        }
    }

    /// The Datenpunkt-Abschnitt shown in the window (for tests).
    pub fn point_window_row(&self) -> Option<RowId> {
        self.point_window.as_ref().map(|window| window.row)
    }

    /// Einfügen ▸ „Datenpunkt-Abschnitt einfügen (KG 480)“: one undo step,
    /// the new title is selected and its plant list opens.
    pub(super) fn new_point_section(&mut self, project: &mut Project) -> EditorOutput {
        let before = project.lv().clone();
        match project.lv_mut().insert_point_section() {
            Ok(index) => {
                self.undo.record(&before, None);
                let id = project.lv().rows()[index].id();
                self.selected = Some(id);
                self.single(id);
                self.scroll_to_selected = true;
                self.open_point_window(id);
                EditorOutput {
                    dirty: true,
                    toast: Some(
                        "Datenpunkt-Abschnitt angelegt – Anlagenliste im Fenster ausfüllen \
                         (Doppelklick auf den Titel öffnet sie wieder)."
                            .into(),
                    ),
                    ..EditorOutput::default()
                }
            }
            Err(error) => {
                *project.lv_mut() = before;
                EditorOutput::toast(error)
            }
        }
    }

    /// In the edit panel and the window „Eigenschaften“ of a title: what the
    /// Datenpunkt-Abschnitt holds, its amount per Kostengruppe and the button
    /// to its plant list. Nothing for other titles.
    pub(super) fn point_section_summary(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        index: usize,
    ) {
        let lv = project.lv();
        let Some(points) = lv
            .rows()
            .get(index)
            .and_then(LvRow::as_group)
            .and_then(|group| group.points())
        else {
            return;
        };
        let id = lv.rows()[index].id();
        let result = points.result();
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!(
                "Berechnet über Datenpunkte ({}, {}) · {} · {}",
                points.stage.label(),
                points.stage.level_label(),
                crate::format::count(points.sections.len(), "Bauteil", "Bauteile"),
                crate::format::count(result.points as usize, "Datenpunkt", "Datenpunkte"),
            ))
            .size(12.5)
            .color(palette.text),
        );
        let currency = lv.currency();
        egui::Grid::new(("point_section_groups", id))
            .num_columns(2)
            .spacing([14.0, 2.0])
            .show(ui, |ui| {
                let lines: Vec<(String, proava_domain::Money)> = match points.stage {
                    CostStage::Estimate => {
                        vec![(format!("KG {}", proava_domain::POINTS_GROUP), result.total)]
                    }
                    CostStage::Calculation => result
                        .groups
                        .iter()
                        .filter(|line| !line.amount.is_zero())
                        .map(|line| (format!("KG {} {}", line.code, line.label), line.amount))
                        .collect(),
                };
                for (label, amount) in lines {
                    ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(proava_domain::price_check::amount(amount, currency))
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                    });
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::small_button(ui, palette, "Anlagenliste öffnen …")
                .on_hover_text(
                    "DP-Arten, Bauteile und Anlagen dieses Abschnitts (auch Doppelklick auf den Titel)",
                )
                .clicked()
            {
                self.open_point_window(id);
            }
            if crate::widgets::small_button(ui, palette, "In Kurztext-Positionen umwandeln")
                .on_hover_text(
                    "Je Bauteil die Datenpunkte als Positionen (Menge aus der Anlagenliste, EP der \
                     DP-Art), darunter die Aufteilung nach KG – dieselbe Summe, ohne die Zeilen der \
                     Anlagenliste",
                )
                .clicked()
            {
                self.wants_point_to_kurztext = Some(id);
            }
        });
    }

    /// Draws the window of the Datenpunkt-Abschnitt (every frame while open).
    pub(super) fn point_window_ui(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &mut Project,
        output: &mut EditorOutput,
    ) {
        let Some(mut window) = self.point_window.take() else {
            return;
        };
        // The title is gone (deleted, undo, another LV) or no longer a section:
        // the window closes.
        let Some(index) = project.lv().index_of(window.row) else {
            return;
        };
        let Some(before) = project.lv().point_section(window.row).cloned() else {
            return;
        };
        let title = window_title(project.lv(), index);
        let screen = ctx.content_rect();
        let size = [
            (screen.width() * 0.9).max(720.0),
            (screen.height() * 0.8).max(420.0),
        ];
        // Esc closes it when no cell is being edited (Anwender 18.11.).
        let mut open = !crate::widgets::esc_closes_window(ctx);
        let mut edited: CostEstimate = before.clone();
        let mut costs = CostsOutput::default();
        egui::Window::new(title)
            .id(egui::Id::unique("lv_point_section_window"))
            .open(&mut open)
            .default_size(size)
            .min_size([640.0, 360.0])
            .default_pos(screen.center() - egui::vec2(size[0], size[1]) / 2.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Bezeichnung, Stand und Bearbeiter sind die des Kosten-LV; die Summe zählt \
                         zum Titel (Rückgängig wie im LV: Strg+Z).",
                    )
                    .size(12.0)
                    .color(palette.text_dim),
                );
                ui.add_space(4.0);
                window.view.edit(ui, palette, &mut edited, None, &mut costs);
            });
        if edited != before {
            let (label, join) = costs.changed.unwrap_or(("Datenpunkte ändern", true));
            let mut before_lv = project.lv().clone();
            if let Some(LvRow::Group(group)) = before_lv.row_mut(index) {
                group.set_points(Some(before));
            }
            let key = join.then(|| EditKey(format!("points:{}:{label}", window.row.0)));
            self.undo.record(&before_lv, key);
            if let Some(LvRow::Group(group)) = project.lv_mut().row_mut(index) {
                group.set_points(Some(edited));
            }
            output.dirty = true;
        }
        if let Some(toast) = costs.response.toast {
            output.toast = Some(toast);
        }
        if open {
            self.point_window = Some(window);
        } else {
            self.undo.seal();
        }
    }
}
