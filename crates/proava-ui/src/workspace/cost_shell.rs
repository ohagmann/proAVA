//! Reiter „Kosten“ im Arbeitsbereich: Ansicht, Rückgängig, Menü Kosten und
//! Druck der Kostenermittlung.

use eframe::egui;
use proava_application::PrintKind;
use proava_domain::{CostEstimateId, CostStage};

use super::{Dialog, Tab, WorkspaceScreen};
use crate::costs_view::{CostRequest, CostsOutput};

/// Befehle des Menüs „Kosten“.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostCommand {
    /// Neues Kosten-LV dieser Stufe (Kurztext-LV, Normalfall).
    NewLv(CostStage),
    /// Kostenberechnung aus dem Kosten-LV einer Kostenschätzung (Kopie).
    CalculationFromLv,
    /// Neue Datenpunktrechnung (GA) nach der Vorlage dieser Stufe.
    New(CostStage),
    /// Kostenberechnung aus der gezeigten Kostenschätzung nach Datenpunkten.
    CalculationFromEstimate,
    /// Drucken der gezeigten Kostenermittlung.
    Print,
}

impl CostCommand {
    pub fn label(self) -> &'static str {
        match self {
            CostCommand::NewLv(CostStage::Estimate) => "Neue Kostenschätzung als Kurztext-LV …",
            CostCommand::NewLv(CostStage::Calculation) => "Neue Kostenberechnung als Kurztext-LV …",
            CostCommand::CalculationFromLv => "Kostenberechnung aus Kostenschätzung (Kurztext-LV)",
            CostCommand::New(CostStage::Estimate) => "Neue Kostenschätzung nach Datenpunkten (GA)",
            CostCommand::New(CostStage::Calculation) => {
                "Neue Kostenberechnung nach Datenpunkten (GA)"
            }
            CostCommand::CalculationFromEstimate => {
                "Kostenberechnung aus Kostenschätzung (Datenpunkte)"
            }
            CostCommand::Print => "Kostenermittlung drucken (PDF) …",
        }
    }

    pub const ALL: [CostCommand; 7] = [
        CostCommand::NewLv(CostStage::Estimate),
        CostCommand::NewLv(CostStage::Calculation),
        CostCommand::CalculationFromLv,
        CostCommand::New(CostStage::Estimate),
        CostCommand::New(CostStage::Calculation),
        CostCommand::CalculationFromEstimate,
        CostCommand::Print,
    ];
}

impl WorkspaceScreen {
    /// The tab itself.
    pub(super) fn costs_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_costs_baseline();
        let palette = self.palette;
        let preferences = self.launcher.preferences();
        self.costs
            .set_defaults(&self.launcher.now_iso(), &preferences.print_author);
        let output = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(14, 12)),
            )
            .show(ui, |ui| self.costs.ui(ui, &palette, &mut self.project))
            .inner;
        self.take_costs_output(ctx, output);
    }

    pub(super) fn ensure_costs_baseline(&mut self) {
        if self.costs_baseline.is_none() {
            self.costs_baseline = Some(self.project.cost_estimates().to_vec());
        }
    }

    /// The estimates may have changed: one undo step with the state before.
    pub(super) fn costs_changed(&mut self, ctx: &egui::Context, label: &'static str, join: bool) {
        let current = self.project.cost_estimates().to_vec();
        if let Some(before) = self.costs_baseline.take() {
            if before != current {
                self.note_editor();
                let now = ctx.input(|input| input.time);
                self.history.record_costs(before, label, now, join);
                self.dirty = true;
            }
        }
        self.costs_baseline = Some(current);
    }

    fn take_costs_output(&mut self, ctx: &egui::Context, output: CostsOutput) {
        if let Some((label, join)) = output.changed {
            self.costs_changed(ctx, label, join);
        }
        for request in output.requests {
            match request {
                CostRequest::Print(id) => self.open_cost_print(id),
                CostRequest::NewLv(stage) => self.new_cost_lv(stage),
                CostRequest::OpenLv(index) => self.open_cost_lv(ctx, index),
                CostRequest::ToCostLv(id) => self.estimate_to_cost_lv(ctx, id),
            }
        }
        self.handle_view_response(ctx, output.response);
    }

    /// Menü Kosten.
    pub(super) fn cost_command(&mut self, ctx: &egui::Context, command: CostCommand) {
        self.tab = Tab::Costs;
        self.ensure_costs_baseline();
        let preferences = self.launcher.preferences();
        self.costs
            .set_defaults(&self.launcher.now_iso(), &preferences.print_author);
        let shown = self.costs.selected(&self.project);
        match command {
            CostCommand::NewLv(stage) => self.new_cost_lv(stage),
            CostCommand::CalculationFromLv => self.calculation_from_cost_lv(ctx),
            CostCommand::New(stage) => {
                self.costs.add_new(&mut self.project, stage);
                self.costs_changed(ctx, "Kostenermittlung anlegen", false);
            }
            CostCommand::CalculationFromEstimate => {
                let estimate = shown
                    .filter(|id| {
                        self.project
                            .cost_estimate(*id)
                            .is_some_and(|estimate| estimate.stage == CostStage::Estimate)
                    })
                    .or_else(|| {
                        self.project
                            .cost_estimates()
                            .iter()
                            .rev()
                            .find(|estimate| estimate.stage == CostStage::Estimate)
                            .map(|estimate| estimate.id)
                    });
                match estimate {
                    Some(id) => {
                        self.costs.calculation_from(&mut self.project, id);
                        self.costs_changed(ctx, "Kostenberechnung aus Kostenschätzung", false);
                    }
                    None => self.say(
                        ctx,
                        "Im Projekt gibt es keine Kostenschätzung – erst eine anlegen.",
                    ),
                }
            }
            CostCommand::Print => match shown {
                Some(id) => self.open_cost_print(id),
                None => self.say(
                    ctx,
                    "Erst eine Kostenschätzung oder Kostenberechnung anlegen.",
                ),
            },
        }
    }

    pub(super) fn open_cost_print(&mut self, id: CostEstimateId) {
        self.open_print_dialog(PrintKind::CostEstimate);
        if let Some(Dialog::Print(print)) = &mut self.dialog {
            print.set_cost_estimate(Some(id));
        }
    }
}
