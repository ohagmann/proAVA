//! Kosten-LVs im Arbeitsbereich: anlegen (Dialog „Neues LV“ mit der Art),
//! öffnen, „Kostenberechnung aus Kostenschätzung“ – und die Sperre: ein
//! Kosten-LV geht nie an Bieter, also gibt es dafür keine Bieter, Angebote,
//! Preisspiegel, Aufträge, Nachträge und Abrechnung (Menü, Befehlspalette,
//! Tastenkürzel und die Reiter zeigen das).

use eframe::egui::{self, RichText};
use proava_application::{GaebExport, PrintKind};
use proava_domain::{calculation_from, CostEstimateId, CostStage, LvPurpose};

use super::{Dialog, ShellAction, Tab, WorkspaceScreen};
use crate::history::ProjectChange;
use crate::lv_properties::{self, LvPropertiesForm};
use crate::widgets;

/// Actions that belong to the tender (Vergabe, Abrechnung) and make no sense
/// for a Kosten-LV.
pub(crate) fn tender_only(action: ShellAction) -> bool {
    match action {
        ShellAction::Bidders
        | ShellAction::Letters
        | ShellAction::ImportBids
        | ShellAction::NewBid
        | ShellAction::Award
        | ShellAction::RevokeAward
        | ShellAction::Contract
        | ShellAction::AdoptPrices
        | ShellAction::Export(GaebExport::RequestForQuotation) => true,
        ShellAction::Print(kind) => tender_print(kind),
        _ => false,
    }
}

/// Printouts of the tender: Angebotsaufforderung, Auftrags-LV, Preisspiegel,
/// Bieterpreise, Niederschrift, Aufmaßprüfung, Rechnungsfreigabe.
pub(crate) fn tender_print(kind: PrintKind) -> bool {
    matches!(
        kind,
        PrintKind::RequestForQuotation
            | PrintKind::Contract
            | PrintKind::PriceComparison
            | PrintKind::BidPrices
            | PrintKind::Submission
            | PrintKind::BillingCheck
            | PrintKind::Release
            | PrintKind::ChangeOrderCheck
            | PrintKind::ChangeOrderOverview
    )
}

/// Reiter of the tender (they show a note for a Kosten-LV).
fn tender_tab(tab: Tab) -> bool {
    matches!(
        tab,
        Tab::Bids | Tab::Comparison | Tab::Billing | Tab::ChangeOrders
    )
}

impl WorkspaceScreen {
    /// The active LV is a Kostenermittlung: its Stufe.
    pub(super) fn active_cost_stage(&self) -> Option<CostStage> {
        self.project.lv().purpose().cost_stage()
    }

    /// Why `action` is refused for the active LV – `None` when it may run.
    pub(super) fn cost_lv_block(&self, action: ShellAction) -> Option<String> {
        let stage = self.active_cost_stage()?;
        tender_only(action).then(|| {
            format!(
                "{} ist eine {} (Kosten-LV) – Bieter, Angebote, Preisspiegel, Auftrag, Nachträge \
                 und Abrechnung gibt es nur für Ausschreibungs-LVs.",
                lv_properties::lv_label(self.project.lv()),
                stage.label()
            )
        })
    }

    /// Dialog „Neues LV“ with the Art set to a Kostenermittlung of this Stufe.
    pub(super) fn new_cost_lv(&mut self, stage: CostStage) {
        self.dialog = Some(Dialog::LvProperties(
            Box::new(LvPropertiesForm::new_cost_lv(&self.project, stage)),
            None,
        ));
    }

    /// Opens a Kosten-LV in the Reiter LV.
    pub(super) fn open_cost_lv(&mut self, ctx: &egui::Context, index: usize) {
        if index >= self.project.lv_count() {
            return;
        }
        self.activate_lv(index);
        self.tab = Tab::Lv;
        let label = lv_properties::lv_label(self.project.lv());
        self.say(ctx, format!("{label} geöffnet."));
    }

    /// „Kostenberechnung aus Kostenschätzung“: a copy of the active Kosten-LV
    /// (or the last one) of a Kostenschätzung as Kostenberechnung; its KG are
    /// refined afterwards (LV prüfen names the coarse ones).
    pub(super) fn calculation_from_cost_lv(&mut self, ctx: &egui::Context) {
        let estimate = LvPurpose::Cost(CostStage::Estimate);
        let source = if self.project.lv().purpose() == estimate {
            Some(self.project.active_lv())
        } else {
            self.project
                .cost_lv_indices()
                .into_iter()
                .rev()
                .find(|&index| self.project.lv_purpose(index) == estimate)
        };
        let Some(source) = source else {
            self.say(
                ctx,
                "Im Projekt gibt es keine Kostenschätzung als Kurztext-LV – erst eine anlegen.",
            );
            return;
        };
        let Some(lv) = self.project.lv_at(source) else {
            return;
        };
        let mut calculation = calculation_from(lv);
        calculation.set_number(lv_properties::next_lv_number(&self.project));
        self.note_editor();
        let before = self.project.lv_list_snapshot();
        let index = self.project.add_lv(calculation);
        self.record_project(
            ProjectChange::LvList(Box::new(before)),
            "Kostenberechnung aus Kostenschätzung",
        );
        self.activate_lv(index);
        self.tab = Tab::Lv;
        self.dirty = true;
        let label = lv_properties::lv_label(self.project.lv());
        self.say(
            ctx,
            format!("{label} angelegt – die Kostengruppen bis zur 3. Ebene verfeinern (LV prüfen zeigt, wo)."),
        );
    }

    /// „In Kosten-LV übernehmen“ (Reiter Kosten): the Datenpunktrechnung `id`
    /// becomes a Kosten-LV of its stage with one Datenpunkt-Abschnitt (GU-Fall)
    /// and is shown in the Reiter LV; one undo step for both changes.
    pub(super) fn estimate_to_cost_lv(&mut self, ctx: &egui::Context, id: CostEstimateId) {
        let number = lv_properties::next_lv_number(&self.project);
        self.note_editor();
        let lvs_before = self.project.lv_list_snapshot();
        let costs_before = self.project.cost_estimates().to_vec();
        match self.project.estimate_to_cost_lv(id, &number) {
            Ok(index) => {
                self.record_project(
                    ProjectChange::Several(vec![
                        ProjectChange::LvList(Box::new(lvs_before)),
                        ProjectChange::Costs(costs_before),
                    ]),
                    "In Kosten-LV übernehmen",
                );
                self.costs_baseline = Some(self.project.cost_estimates().to_vec());
                self.costs.reset_editing();
                self.activate_lv(index);
                self.tab = Tab::Lv;
                self.dirty = true;
                let label = lv_properties::lv_label(self.project.lv());
                self.say(
                    ctx,
                    format!(
                        "{label} angelegt mit dem Datenpunkt-Abschnitt 480 – die übrigen Kostengruppen \
                         als Kurztext-Positionen ergänzen (Einfügen › Neuer Titel)."
                    ),
                );
            }
            Err(error) => self.say(ctx, error),
        }
    }

    /// Instead of a Reiter of the tender: a note when the active LV is a
    /// Kosten-LV. True when it was drawn.
    pub(super) fn cost_lv_notice(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(stage) = self.active_cost_stage() else {
            return false;
        };
        if !tender_tab(self.tab) {
            return false;
        }
        let palette = self.palette;
        let label = lv_properties::lv_label(self.project.lv());
        let mut back = false;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(14, 12)),
            )
            .show(ui, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("{label} ist eine {}", stage.label()))
                            .size(17.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Ein Kosten-LV (Kostenermittlung nach DIN 276) geht nicht an Bieter: \
                             Angebote, Preisspiegel, Auftrag, Nachträge und Abrechnung gibt es nur \
                             für Ausschreibungs-LVs. Oben in der LV-Auswahl ein Ausschreibungs-LV wählen.",
                        )
                        .size(12.5)
                        .color(palette.text_dim),
                    );
                    ui.add_space(14.0);
                    back = widgets::button(ui, &palette, "Zum Kosten-LV", true, 220.0).clicked();
                });
            });
        if back {
            self.tab = Tab::Lv;
        }
        true
    }

    /// Line above the LV table of a Kosten-LV: what it is and what it is not.
    pub(super) fn cost_lv_banner(&self, ui: &mut egui::Ui) {
        let Some(stage) = self.active_cost_stage() else {
            return;
        };
        let palette = self.palette;
        egui::Panel::top("cost_lv_banner")
            .frame(
                egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(egui::Stroke::new(1.0, palette.line))
                    .inner_margin(egui::Margin::symmetric(16, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} nach DIN 276 ({}. Ebene) – Kosten-LV, nicht zur Ausschreibung",
                            stage.label(),
                            stage.level()
                        ))
                        .size(12.5)
                        .strong()
                        .color(palette.text),
                    );
                    ui.label(
                        RichText::new(
                            "EP = Kostenkennwert je Bezugseinheit, die Kostengruppe kommt vom Titel \
                             oder steht an der Position; Drucken › Kostenermittlung (Kosten-LV).",
                        )
                        .size(12.0)
                        .color(palette.text_dim),
                    );
                });
            });
    }
}
