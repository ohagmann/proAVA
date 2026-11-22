//! Reiter „Kostenverfolgung“ im Arbeitsbereich: Ansicht, Rückgängig,
//! Menübefehle, Sprung zur Position ohne Kostengruppe, Wahl des Angebots.

use eframe::egui;

use proava_application::{CsvEncoding, PrintKind};

use super::{FilePurpose, Tab, WorkspaceScreen};
use crate::tracking_view::{TrackingDialog, TrackingOutput, TrackingRequest};

/// Befehle der Kostenverfolgung (Menü Kosten, Befehlssuche).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingCommand {
    Show,
    Budget,
    Sources,
    Record,
    Csv,
}

impl TrackingCommand {
    pub const ALL: [TrackingCommand; 5] = [
        TrackingCommand::Show,
        TrackingCommand::Budget,
        TrackingCommand::Sources,
        TrackingCommand::Record,
        TrackingCommand::Csv,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TrackingCommand::Show => "Kostenverfolgung",
            TrackingCommand::Budget => "Kostenvorgabe (Budget) …",
            TrackingCommand::Sources => "Kostenermittlungen der Kostenverfolgung …",
            TrackingCommand::Record => "Kostenstand festhalten …",
            TrackingCommand::Csv => "Kostenverfolgung als CSV …",
        }
    }
}

impl WorkspaceScreen {
    pub(super) fn tracking_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_tracking_baseline();
        let palette = self.palette;
        self.tracking.set_today(&self.launcher.now_iso());
        let output = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(14, 12)),
            )
            .show(ui, |ui| self.tracking.ui(ui, &palette, &mut self.project))
            .inner;
        self.take_tracking_output(ctx, output);
    }

    pub(super) fn ensure_tracking_baseline(&mut self) {
        if self.tracking_baseline.is_none() {
            self.tracking_baseline = Some(self.project.cost_tracking().clone());
        }
    }

    /// The Kostenverfolgung data may have changed: one undo step with the
    /// state before.
    pub(super) fn tracking_changed(
        &mut self,
        ctx: &egui::Context,
        label: &'static str,
        join: bool,
    ) {
        let current = self.project.cost_tracking().clone();
        if let Some(before) = self.tracking_baseline.take() {
            if before != current {
                self.note_editor();
                let now = ctx.input(|input| input.time);
                self.history.record_tracking(before, label, now, join);
                self.dirty = true;
            }
        }
        self.tracking_baseline = Some(current);
    }

    fn take_tracking_output(&mut self, ctx: &egui::Context, output: TrackingOutput) {
        if let Some((label, join)) = output.changed {
            self.tracking_changed(ctx, label, join);
        }
        for request in output.requests {
            match request {
                TrackingRequest::Print => self.open_print_dialog(PrintKind::CostTracking),
                TrackingRequest::Csv => self.export_tracking_csv(),
                TrackingRequest::ShowPosition(index, row) => {
                    self.activate_lv(index);
                    self.tab = Tab::Lv;
                    self.editor.select(self.project.lv(), row);
                    self.editor.scroll_to_row(self.project.lv(), row);
                }
                TrackingRequest::ChooseOffer(index, choice) => {
                    let (Some(key), Some(before)) = (
                        self.project.lv_key(index),
                        self.project.offers_snapshot(index),
                    ) else {
                        continue;
                    };
                    if self.project.set_offer_choice_of(index, choice)
                        && self.project.offers_snapshot(index).as_ref() != Some(&before)
                    {
                        self.note_editor();
                        let now = ctx.input(|input| input.time);
                        self.history.record_offers(
                            key,
                            before,
                            "Angebot für die Kostenverfolgung",
                            now,
                            false,
                        );
                        self.dirty = true;
                        if index == self.project.active_lv() {
                            self.offers_baseline = None;
                        }
                    }
                }
            }
        }
    }

    /// Menü Kosten ▸ Kostenverfolgung …
    pub(super) fn tracking_command(&mut self, command: TrackingCommand) {
        self.tab = Tab::Tracking;
        self.ensure_tracking_baseline();
        self.tracking.set_today(&self.launcher.now_iso());
        let today = self.tracking.today.clone();
        match command {
            TrackingCommand::Show => {}
            TrackingCommand::Budget => {
                self.tracking.dialog = Some(TrackingDialog::budget(&self.project, &today));
            }
            TrackingCommand::Sources => self.tracking.dialog = Some(TrackingDialog::Sources),
            TrackingCommand::Record => {
                self.tracking.dialog = Some(TrackingDialog::record(&today));
            }
            TrackingCommand::Csv => self.export_tracking_csv(),
        }
    }

    /// Kostenverfolgung ▸ CSV …: the table as shown.
    pub(super) fn export_tracking_csv(&mut self) {
        self.open_file_dialog(FilePurpose::ExportTrackingCsv(CsvEncoding::Utf8Bom));
    }

    /// After an undo/redo step of the Kostenverfolgung.
    pub(super) fn after_tracking_step(&mut self, tracking: bool) {
        self.tracking.reset_editing();
        self.tracking_baseline = None;
        if tracking {
            self.tab = Tab::Tracking;
        }
    }
}
