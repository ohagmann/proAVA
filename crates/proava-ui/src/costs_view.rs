//! Reiter „Kosten“ (Strg+6): Kostenschätzung (DIN 276, 2. Ebene) und
//! Kostenberechnung (3. Ebene) für die KG 480.
//!
//! Ganz oben die Kosten-LVs (Kostenermittlung als Kurztext-LV, der
//! Normalfall für alle Gewerke; bearbeitet im Reiter LV), darunter die
//! Datenpunktrechnung der Gebäudeautomation (Sonderfall):
//!
//! Oben die Wahl der Kostenermittlung und ihr Kopf (Bezeichnung, Stand,
//! Bearbeiter, Kennwert), aufklappbar die DP-Arten mit Preisen. Links die
//! Anlagenliste je Bauteil wie eine Tabellenkalkulation (Klick oder Tippen
//! bearbeitet, Tab/Enter wandern, Summenzeilen fest), rechts das Ergebnis:
//! Aufteilung auf KG 481–489 mit Prüfsumme, Zusammenfassung über alle
//! Bauteile, Kostenkontrolle gegen die LVs und Hinweise.

mod cost_lvs;
mod side;
mod table;

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    format_decimal_de, CostEstimate, CostEstimateId, CostStage, Decimal, NumberKind, PointKind,
    Project,
};

use crate::icons::Icon;
use crate::theme::Palette;
use crate::view::ViewResponse;
use crate::widgets;

pub use table::{CellRef, Col};

/// Below this width the result column moves under the plant list.
pub const NARROW_WIDTH: f32 = 980.0;

/// What only the workspace can do for the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostRequest {
    /// Open the print dialog for this estimate.
    Print(CostEstimateId),
    /// New Kosten-LV of this Stufe (dialog „Neues LV“ with the Art set).
    NewLv(CostStage),
    /// Open the Kosten-LV at this index in the Reiter LV.
    OpenLv(usize),
    /// „In Kosten-LV übernehmen“: this estimate becomes a Kosten-LV with one
    /// Datenpunkt-Abschnitt (GU-Fall).
    ToCostLv(CostEstimateId),
}

/// Result of one frame.
#[derive(Debug, Default)]
pub struct CostsOutput {
    pub response: ViewResponse,
    /// The estimates changed: label of the undo step and whether typing
    /// in a row may join the previous step.
    pub changed: Option<(&'static str, bool)>,
    pub requests: Vec<CostRequest>,
}

impl CostsOutput {
    fn change(&mut self, label: &'static str, join: bool) {
        // A structural change in the same frame wins over typing.
        if self.changed.is_none_or(|(_, joined)| joined) {
            self.changed = Some((label, join));
        }
        self.response.dirty = true;
    }
}

pub struct CostsView {
    selected: Option<CostEstimateId>,
    pub(crate) editing: Option<table::Editing>,
    confirm_delete: Option<CostEstimateId>,
    kinds_open: bool,
    /// Typed text of number fields while they have the focus.
    buffers: HashMap<egui::Id, String>,
    /// Bauteil whose split is open in the side panel.
    open_section: Option<usize>,
    today: String,
    author: String,
    /// Scroll position of the table (for keeping the edited cell visible).
    pub(crate) scroll: table::ScrollState,
    /// Editing the Datenpunkt-Abschnitt of a Kosten-LV (window in the Reiter
    /// LV): Bezeichnung, Stand and Bearbeiter are the LV's, not shown.
    section: bool,
}

impl Default for CostsView {
    fn default() -> Self {
        Self::new()
    }
}

impl CostsView {
    pub fn new() -> Self {
        Self {
            selected: None,
            editing: None,
            confirm_delete: None,
            kinds_open: false,
            buffers: HashMap::new(),
            open_section: Some(0),
            today: String::new(),
            author: String::new(),
            scroll: table::ScrollState::default(),
            section: false,
        }
    }

    /// The view for a Datenpunkt-Abschnitt of a Kosten-LV (GU-Fall): plant
    /// list, DP-Arten and result as in the Reiter Kosten, without the head of
    /// a standalone Kostenermittlung.
    pub fn for_section() -> Self {
        Self {
            section: true,
            ..Self::new()
        }
    }

    /// Today as TT.MM.JJJJ and the Bearbeiter from the settings (for new estimates).
    pub fn set_defaults(&mut self, today_iso: &str, author: &str) {
        self.today = crate::format::iso_to_german_date(today_iso);
        self.author = author.trim().to_string();
    }

    /// The estimate shown (the newest when none was chosen or it is gone).
    pub fn selected(&self, project: &Project) -> Option<CostEstimateId> {
        self.selected
            .filter(|id| project.cost_estimate(*id).is_some())
            .or_else(|| project.cost_estimates().last().map(|estimate| estimate.id))
    }

    pub fn select(&mut self, id: CostEstimateId) {
        if self.selected != Some(id) {
            self.selected = Some(id);
            self.editing = None;
            self.open_section = Some(0);
        }
    }

    /// After undo/redo: drop a cell edit that may point to rows that are gone.
    pub fn reset_editing(&mut self) {
        self.editing = None;
        self.buffers.clear();
    }

    /// Status line text.
    pub fn status_text(&self, project: &Project) -> String {
        match self
            .selected(project)
            .and_then(|id| project.cost_estimate(id))
        {
            Some(estimate) => {
                let result = estimate.result();
                format!(
                    "{} · {} · {} · {}",
                    estimate.stage.label(),
                    crate::format::count(estimate.sections.len(), "Bauteil", "Bauteile"),
                    crate::format::count(result.points as usize, "Datenpunkt", "Datenpunkte"),
                    result.total
                )
            }
            None => crate::format::count(
                project.cost_estimates().len(),
                "Kostenermittlung",
                "Kostenermittlungen",
            ),
        }
    }

    /// Adds a new estimate of `stage` (template values) and shows it.
    pub fn add_new(&mut self, project: &mut Project, stage: CostStage) -> CostEstimateId {
        let mut estimate = CostEstimate::template(stage, &self.today, &self.author);
        let same = project
            .cost_estimates()
            .iter()
            .filter(|other| other.stage == stage)
            .count();
        if same > 0 {
            estimate.title = format!("{} {}", stage.label(), same + 1);
        }
        let id = project.add_cost_estimate(estimate);
        self.select(id);
        id
    }

    /// „Kostenberechnung aus Kostenschätzung“ of the estimate `from`.
    pub fn calculation_from(
        &mut self,
        project: &mut Project,
        from: CostEstimateId,
    ) -> Option<CostEstimateId> {
        let source = project.cost_estimate(from)?;
        let calculation = source.to_calculation(&self.today, &self.author);
        let id = project.add_cost_estimate(calculation);
        self.select(id);
        Some(id)
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> CostsOutput {
        let mut output = CostsOutput::default();
        let Some(id) = self.selected(project) else {
            self.empty_state(ui, palette, project, &mut output);
            return output;
        };
        if self.selected != Some(id) {
            self.select(id);
        }
        cost_lvs::strip(ui, palette, project, &mut output);
        ui.add_space(6.0);
        self.toolbar(ui, palette, project, id, &mut output);
        if project.cost_estimate(id).is_none() {
            return output;
        }
        ui.add_space(6.0);
        // The control needs the whole project: compute it before borrowing the estimate.
        let control = project
            .cost_estimate(id)
            .map(|estimate| proava_domain::cost_control(project, estimate));
        let Some(estimate) = project.cost_estimate_mut(id) else {
            return output;
        };
        self.edit(ui, palette, estimate, control.as_ref(), &mut output);
        output
    }

    /// Head, DP-Arten, plant list and result column of `estimate` in the
    /// space left in `ui` (Reiter Kosten and the Datenpunkt-Abschnitt of a
    /// Kosten-LV).
    pub fn edit(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        estimate: &mut CostEstimate,
        control: Option<&proava_domain::CostControl>,
        output: &mut CostsOutput,
    ) {
        // The result column docks right under the button row and takes the
        // full height down to the status line (Anwender 18.11.: „bis oben
        // andocken“), with its own scrolling – whatever the left side holds.
        // In narrow windows it moves under the plant list. Both parts get a
        // fixed rectangle of their own (and are clipped to it), so neither can
        // paint into the other.
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, egui::Sense::hover());
        let narrow = area.width() < NARROW_WIDTH;
        let gap = 12.0;
        let (main_rect, side_rect) = if narrow {
            let side_height = (area.height() * 0.42)
                .max(180.0)
                .min(area.height() - 120.0)
                .max(0.0);
            let split = area.bottom() - side_height;
            (
                egui::Rect::from_min_max(area.min, egui::pos2(area.right(), split - gap)),
                egui::Rect::from_min_max(egui::pos2(area.left(), split), area.max),
            )
        } else {
            // Fixed width: the side tables size their columns to it.
            let side_width = (area.width() * 0.34).clamp(400.0, 560.0);
            let split = area.right() - side_width;
            (
                egui::Rect::from_min_max(area.min, egui::pos2(split - gap, area.bottom())),
                egui::Rect::from_min_max(egui::pos2(split, area.top()), area.max),
            )
        };
        let line = if narrow {
            [
                egui::pos2(area.left(), side_rect.top() - gap / 2.0),
                egui::pos2(area.right(), side_rect.top() - gap / 2.0),
            ]
        } else {
            [
                egui::pos2(side_rect.left() - gap / 2.0, area.top()),
                egui::pos2(side_rect.left() - gap / 2.0, area.bottom()),
            ]
        };
        ui.painter()
            .line_segment(line, egui::Stroke::new(1.0, palette.line));
        let clip = ui.clip_rect();
        let mut side_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("costs_side")
                .max_rect(side_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        side_ui.set_clip_rect(side_rect.intersect(clip));
        egui::ScrollArea::vertical()
            .id_salt("costs_side_scroll")
            .auto_shrink([false, false])
            .show(&mut side_ui, |ui| {
                side::show(self, ui, palette, estimate, control, output);
            });
        let mut main_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("costs_main")
                .max_rect(main_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        main_ui.set_clip_rect(main_rect.intersect(clip));
        let ui = &mut main_ui;
        // Head and DP-Arten take at most half the height (and scroll then);
        // the plant list keeps the rest, also with many DP-Arten open.
        egui::ScrollArea::vertical()
            .id_salt("costs_head_scroll")
            .max_height((main_rect.height() * 0.5).max(120.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.head(ui, palette, estimate, output);
                ui.add_space(4.0);
                self.kinds(ui, palette, estimate, output);
            });
        ui.add_space(6.0);
        table::show(self, ui, palette, estimate, output);
    }

    fn empty_state(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut CostsOutput,
    ) {
        if !project.cost_lv_indices().is_empty() {
            cost_lvs::strip(ui, palette, project, output);
        }
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            let heading = if project.cost_lv_indices().is_empty() {
                "Noch keine Kostenermittlung im Projekt"
            } else {
                "Neue Kostenermittlung"
            };
            ui.label(
                RichText::new(heading)
                    .size(17.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Kostenschätzung nach DIN 276: 2. Ebene der Kostengliederung, \
                     Kostenberechnung: 3. Ebene.",
                )
                .size(12.5)
                .color(palette.text_dim),
            );
        });
        ui.add_space(16.0);
        cost_lvs::choices(ui, palette, output, |stage, output| {
            self.add_new(project, stage);
            output.change("Kostenermittlung anlegen", false);
        });
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: CostEstimateId,
        output: &mut CostsOutput,
    ) {
        let names: Vec<(CostEstimateId, String)> = project
            .cost_estimates()
            .iter()
            .map(|estimate| {
                (
                    estimate.id,
                    format!(
                        "{} – {}",
                        estimate.display_name(),
                        estimate.stage.level_label()
                    ),
                )
            })
            .collect();
        let stage = project.cost_estimate(id).map(|estimate| estimate.stage);
        let narrow = ui.available_width() < NARROW_WIDTH + 200.0;
        crate::widgets::row_wrapped(ui, |ui| {
            ui.label(
                RichText::new("Nach Datenpunkten (GA)")
                    .size(12.0)
                    .color(palette.text_dim),
            );
            let current = names
                .iter()
                .find(|(other, _)| *other == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            let mut chosen = id;
            egui::ComboBox::from_id_salt("cost_estimate_pick")
                .selected_text(current)
                .width(if narrow { 260.0 } else { 360.0 })
                .show_ui(ui, |ui| {
                    for (other, name) in &names {
                        ui.selectable_value(&mut chosen, *other, name);
                    }
                });
            if chosen != id {
                self.select(chosen);
            }
            ui.add_space(8.0);
            for stage in CostStage::ALL {
                if widgets::icon_label_button(ui, palette, Icon::Plus, stage.label())
                    .on_hover_text(format!(
                        "Neue {} nach der Vorlage „{}“",
                        stage.label(),
                        stage.template_name()
                    ))
                    .clicked()
                {
                    self.add_new(project, stage);
                    output.change("Kostenermittlung anlegen", false);
                }
            }
            if widgets::small_button(ui, palette, "In Kosten-LV übernehmen")
                .on_hover_text(
                    "Wird ein Kosten-LV mit diesem Datenpunkt-Abschnitt (KG 480) – dort neben den \
                     Kurztext-Positionen der übrigen Kostengruppen (GU-Fall). Gezählt wird es danach \
                     nur noch dort.",
                )
                .clicked()
            {
                output.requests.push(CostRequest::ToCostLv(id));
            }
            if stage == Some(CostStage::Estimate)
                && widgets::small_button(ui, palette, "Kostenberechnung daraus")
                    .on_hover_text("Neue Kostenberechnung mit derselben Anlagenliste, EP je DP-Art und Aufteilung der Vorlage")
                    .clicked()
            {
                self.calculation_from(project, id);
                output.change("Kostenberechnung aus Kostenschätzung", false);
            }
            if narrow {
                // Narrow window: everything in one flowing row.
                ui.add_space(8.0);
                self.toolbar_print_delete(ui, palette, project, id, output, false);
            } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.toolbar_print_delete(ui, palette, project, id, output, true);
                });
            }
        });
    }

    /// „Drucken …“ and „Löschen …“ (with its question); `right_to_left` =
    /// in the right-aligned part of the button row.
    fn toolbar_print_delete(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        id: CostEstimateId,
        output: &mut CostsOutput,
        right_to_left: bool,
    ) {
        let print = |ui: &mut egui::Ui, output: &mut CostsOutput| {
            if widgets::icon_label_button(ui, palette, Icon::Document, "Drucken …")
                .on_hover_text("PDF der Kostenermittlung (Druckdialog, Strg+P)")
                .clicked()
            {
                output.requests.push(CostRequest::Print(id));
            }
        };
        if !right_to_left {
            print(ui, output);
            ui.add_space(6.0);
        }
        if self.confirm_delete == Some(id) {
            if widgets::danger_button(ui, palette, "Wirklich löschen", 130.0).clicked() {
                project.remove_cost_estimate(id);
                self.confirm_delete = None;
                self.selected = None;
                self.editing = None;
                output.change("Kostenermittlung löschen", false);
                output.response.toast =
                    Some("Kostenermittlung gelöscht (Strg+Z holt sie zurück)".into());
            }
            if widgets::small_button(ui, palette, "Abbrechen").clicked() {
                self.confirm_delete = None;
            }
        } else if widgets::icon_label_button(ui, palette, Icon::Trash, "Löschen …")
            .on_hover_text("Diese Kostenermittlung löschen")
            .clicked()
        {
            self.confirm_delete = Some(id);
        }
        if right_to_left {
            ui.add_space(6.0);
            print(ui, output);
        }
    }

    fn head(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        estimate: &mut CostEstimate,
        output: &mut CostsOutput,
    ) {
        let section = self.section;
        crate::widgets::row_wrapped(ui, |ui| {
            let label = |ui: &mut egui::Ui, text: &str| {
                ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
            };
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    estimate.stage.label(),
                    estimate.stage.level_label()
                ))
                .size(12.5)
                .strong()
                .color(palette.accent),
            );
            ui.add_space(10.0);
            if section {
                // Bezeichnung, Stand, Bearbeiter: those of the Kosten-LV.
                return;
            }
            // Name and field stay together when the row wraps: a pair has a
            // known size, so the wrapping row moves it whole to the next line
            // (a nested `horizontal` would not wrap and widen the page).
            let pair = |ui: &mut egui::Ui, name: &str, field: egui::TextEdit<'_>, width: f32| {
                let name_width = ui
                    .painter()
                    .layout_no_wrap(
                        name.to_string(),
                        egui::FontId::proportional(12.0),
                        palette.text_dim,
                    )
                    .size()
                    .x;
                let spacing = ui.spacing().item_spacing.x;
                ui.allocate_ui_with_layout(
                    egui::vec2(name_width + spacing + width, 24.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        label(ui, name);
                        ui.add(field.desired_width(width)).changed()
                    },
                )
                .inner
            };
            if pair(
                ui,
                "Bezeichnung",
                egui::TextEdit::singleline(&mut estimate.title),
                220.0,
            ) {
                output.change("Kostenermittlung ändern", true);
            }
            if pair(
                ui,
                "Stand",
                egui::TextEdit::singleline(&mut estimate.date).hint_text("TT.MM.JJJJ"),
                84.0,
            ) {
                output.change("Kostenermittlung ändern", true);
            }
            if pair(
                ui,
                "Bearbeiter",
                egui::TextEdit::singleline(&mut estimate.author),
                140.0,
            ) {
                output.change("Kostenermittlung ändern", true);
            }
            if pair(
                ui,
                "Bemerkung",
                egui::TextEdit::singleline(&mut estimate.remark),
                220.0,
            ) {
                output.change("Kostenermittlung ändern", true);
            }
        });
        if estimate.stage == CostStage::Estimate {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!estimate.rate_per_kind, |ui| {
                    ui.label(RichText::new("Kostenkennwert je DP").size(12.0).color(palette.text_dim));
                    let id = ui.scope_id().with("cost_rate");
                    if decimal_field(ui, &mut self.buffers, id, &mut estimate.rate, NumberKind::CostPrice, 90.0) {
                        output.change("Kostenkennwert ändern", true);
                    }
                    ui.label("€");
                    ui.add_space(10.0);
                    ui.label(RichText::new("Spanne von").size(12.0).color(palette.text_dim));
                    let id = ui.scope_id().with("cost_rate_low");
                    if optional_decimal_field(ui, &mut self.buffers, id, &mut estimate.rate_low, NumberKind::CostPrice, 80.0) {
                        output.change("Kostenkennwert ändern", true);
                    }
                    ui.label(RichText::new("bis").size(12.0).color(palette.text_dim));
                    let id = ui.scope_id().with("cost_rate_high");
                    if optional_decimal_field(ui, &mut self.buffers, id, &mut estimate.rate_high, NumberKind::CostPrice, 80.0) {
                        output.change("Kostenkennwert ändern", true);
                    }
                    ui.label("€");
                });
                ui.add_space(16.0);
                if ui
                    .checkbox(&mut estimate.rate_per_kind, "Kennwert je DP-Art")
                    .on_hover_text("Statt eines Preises für alle Datenpunkte gilt der Preis jeder DP-Art (unter „DP-Arten und Preise“)")
                    .changed()
                {
                    if estimate.rate_per_kind {
                        self.kinds_open = true;
                    }
                    output.change("Kostenkennwert ändern", false);
                }
            });
        }
    }

    fn kinds(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        estimate: &mut CostEstimate,
        output: &mut CostsOutput,
    ) {
        let priced = estimate.stage == CostStage::Calculation || estimate.rate_per_kind;
        let summary = estimate
            .kinds
            .iter()
            .map(|kind| {
                if priced {
                    format!("{} {}", kind.key, format_decimal_de(kind.unit_price, 2))
                } else {
                    kind.key.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let header = format!(
            "DP-Arten{} ({}) – {summary}",
            if priced { " und Preise in €" } else { "" },
            estimate.kinds.len()
        );
        let response = egui::CollapsingHeader::new(RichText::new(header).size(12.5).color(palette.text))
            .id_salt("cost_kinds")
            .open(Some(self.kinds_open))
            .show(ui, |ui| {
                let mut action: Option<(usize, KindAction)> = None;
                // Own column widths (no Grid: a Grid sizes its cells after the
                // last frame and cut the Bezeichnung off). Kürzel and Preis stay
                // narrow, the Bezeichnung takes the free width.
                let spacing = 8.0;
                let (key_width, price_width, move_width, bin_width) = (56.0, 90.0, 64.0, 30.0);
                let fixed = key_width + if priced { price_width } else { 0.0 } + move_width + bin_width;
                let gaps = spacing * if priced { 4.0 } else { 3.0 };
                let label_width = (ui.available_width() - fixed - gaps - 4.0).clamp(160.0, 520.0);
                let cell = |ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_width(width);
                            add(ui);
                        },
                    );
                };
                let head = |ui: &mut egui::Ui, text: &str| {
                    ui.label(RichText::new(text).size(11.5).color(palette.text_dim));
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = spacing;
                    cell(ui, key_width, &mut |ui| head(ui, "Kürzel"));
                    cell(ui, label_width, &mut |ui| head(ui, "Bezeichnung"));
                    if priced {
                        cell(ui, price_width, &mut |ui| head(ui, "Preis je DP"));
                    }
                });
                let count = estimate.kinds.len();
                for (index, kind) in estimate.kinds.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = spacing;
                        cell(ui, key_width, &mut |ui| {
                            if ui
                                .add(egui::TextEdit::singleline(&mut kind.key).desired_width(key_width))
                                .on_hover_text("Kürzel; AV, BV und MV gelten als virtuell (1.2.n im Druck), alle übrigen als physikalische Ein-/Ausgänge (1.1.n)")
                                .changed()
                            {
                                output.change("DP-Art ändern", true);
                            }
                        });
                        cell(ui, label_width, &mut |ui| {
                            if ui
                                .add(egui::TextEdit::singleline(&mut kind.label).desired_width(label_width))
                                .changed()
                            {
                                output.change("DP-Art ändern", true);
                            }
                        });
                        if priced {
                            cell(ui, price_width, &mut |ui| {
                                let id = ui.scope_id().with(("cost_kind_price", index));
                                if decimal_field(
                                    ui,
                                    &mut self.buffers,
                                    id,
                                    &mut kind.unit_price,
                                    NumberKind::CostPrice,
                                    price_width,
                                ) {
                                    output.change("Preis der DP-Art ändern", true);
                                }
                            });
                        }
                        cell(ui, move_width, &mut |ui| {
                            ui.add_enabled_ui(index > 0, |ui| {
                                if widgets::tool_button(ui, palette, Icon::Back, false, "Nach links").clicked() {
                                    action = Some((index, KindAction::Left));
                                }
                            });
                            ui.add_enabled_ui(index + 1 < count, |ui| {
                                if widgets::tool_button(ui, palette, Icon::Forward, false, "Nach rechts").clicked() {
                                    action = Some((index, KindAction::Right));
                                }
                            });
                        });
                        cell(ui, bin_width, &mut |ui| {
                            if widgets::tool_button(
                                ui,
                                palette,
                                Icon::Trash,
                                false,
                                "DP-Art entfernen (mit ihren Anzahlen)",
                            )
                            .clicked()
                            {
                                action = Some((index, KindAction::Remove));
                            }
                        });
                    });
                }
                ui.add_space(4.0);
                if widgets::icon_label_button(ui, palette, Icon::Plus, "DP-Art hinzufügen").clicked() {
                    // A new kind is a physical one (Anwender 18.11.), EP like AI/BI/AO/BO.
                    let price = if priced { Decimal::from(400) } else { Decimal::ZERO };
                    estimate.add_kind(PointKind::new("", "", price));
                    output.change("DP-Art hinzufügen", false);
                }
                if let Some((index, action)) = action {
                    self.editing = None;
                    match action {
                        KindAction::Left => {
                            estimate.move_kind(index, true);
                            output.change("DP-Art verschieben", false);
                        }
                        KindAction::Right => {
                            estimate.move_kind(index, false);
                            output.change("DP-Art verschieben", false);
                        }
                        KindAction::Remove => {
                            if let Some(kind) = estimate.remove_kind(index) {
                                output.change("DP-Art entfernen", false);
                                output.response.toast = Some(format!(
                                    "DP-Art „{}“ entfernt (Strg+Z holt sie mit den Anzahlen zurück)",
                                    kind.key
                                ));
                            }
                        }
                    }
                }
            });
        if response.header_response.clicked() {
            self.kinds_open = !self.kinds_open;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum KindAction {
    Left,
    Right,
    Remove,
}

/// A number field in German format: shows `value`, takes over valid input
/// right away (empty = 0), keeps what is typed while it has the focus.
pub(crate) fn decimal_field(
    ui: &mut egui::Ui,
    buffers: &mut HashMap<egui::Id, String>,
    id: egui::Id,
    value: &mut Decimal,
    kind: NumberKind,
    width: f32,
) -> bool {
    let mut option = Some(*value);
    let changed = optional_decimal_field(ui, buffers, id, &mut option, kind, width);
    if changed {
        *value = option.unwrap_or_default();
    }
    changed
}

/// Like [`decimal_field`]; an empty field is None.
pub(crate) fn optional_decimal_field(
    ui: &mut egui::Ui,
    buffers: &mut HashMap<egui::Id, String>,
    id: egui::Id,
    value: &mut Option<Decimal>,
    kind: NumberKind,
    width: f32,
) -> bool {
    // The limits are those of the MCP server too (`input_limits`).
    let shown = value
        .map(|value| format_decimal_de(value, kind.decimals()).replace('.', ""))
        .unwrap_or_default();
    let focused = ui.memory(|memory| memory.has_focus(id));
    let mut text = if focused {
        buffers.get(&id).cloned().unwrap_or_else(|| shown.clone())
    } else {
        shown
    };
    let parsed = if text.trim().is_empty() {
        Ok(None)
    } else {
        kind.parse(&text).map(Some)
    };
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width)
            .align(egui::Align2::RIGHT_CENTER)
            .text_color_opt(parsed.is_err().then_some(ui.visuals().error_fg_color)),
    );
    let mut changed = false;
    if response.changed() {
        let parsed = if text.trim().is_empty() {
            Ok(None)
        } else {
            kind.parse(&text).map(Some).map_err(|_| ())
        };
        if let Ok(parsed) = parsed {
            if parsed != *value {
                *value = parsed;
                changed = true;
            }
        }
    }
    if response.has_focus() {
        buffers.insert(id, text);
    } else {
        buffers.remove(&id);
    }
    changed
}
