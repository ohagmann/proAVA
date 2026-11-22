//! Reiter „Nachträge“ im Arbeitsbereich: Ansicht, Rückgängig und die
//! Dateien der Nachträge (PDF Prüfung/Übersicht, Nachtrags-LV als GAEB,
//! Nachtragsangebot einlesen).

use std::path::PathBuf;

use eframe::egui;
use proava_application::{ChangeOrderExport, ChangeOrderPrint};
use proava_domain::{ChangeOrderId, ChangeOrderOffer};

use super::{Dialog, FilePurpose, JobOutcome, WorkspaceScreen};
use crate::change_orders_view::{ChangeOrderRequest, ChangeOrdersOutput};
use crate::file_dialog::{self, FileDialog};

/// File the Nachträge wait for.
#[derive(Debug, Clone, Copy)]
pub(super) enum ChangeOrderFile {
    Print(ChangeOrderPrint),
    Export(ChangeOrderId, ChangeOrderExport),
    Import(ChangeOrderId),
}

/// Result of the worker thread.
pub(super) enum ChangeOrderDone {
    Pdf {
        label: &'static str,
        file: PathBuf,
        result: Result<usize, String>,
    },
    Gaeb {
        label: &'static str,
        file: PathBuf,
        result: Result<Vec<String>, String>,
    },
    Offer {
        id: ChangeOrderId,
        file: PathBuf,
        result: Result<Box<(ChangeOrderOffer, Vec<String>)>, String>,
    },
}

/// GAEB files a change-order offer may come in.
const OFFER_EXTENSIONS: [&str; 14] = [
    "X84", "X86", "X82", "X83", "X81", "D84", "D86", "D82", "D83", "D81", "P84", "P86", "P82",
    "P83",
];

impl WorkspaceScreen {
    /// The tab itself.
    pub(super) fn change_orders_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_contract_baseline();
        let palette = self.palette;
        self.change_orders.set_today(&self.launcher.now_iso());
        let output = egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(palette.background)
                    .inner_margin(egui::Margin::symmetric(14, 12)),
            )
            .show(ui, |ui| {
                self.change_orders.ui(ui, &palette, &mut self.project)
            })
            .inner;
        self.take_change_orders_output(ctx, output);
    }

    fn take_change_orders_output(&mut self, ctx: &egui::Context, output: ChangeOrdersOutput) {
        let label = output.label.unwrap_or("Nachtrag");
        let index = self.project.active_lv();
        let key = self.active_key();
        if let Some((before, join)) = output.before {
            // The LV changed too: the step keeps it.
            let changed = self
                .project
                .contract_snapshot_with_lv(index)
                .is_some_and(|now| now != before);
            if changed {
                self.note_editor();
                let now = ctx.input(|input| input.time);
                self.history.record_contract(key, before, label, now, join);
            }
            self.contract_baseline = self
                .project
                .contract_snapshot(index)
                .map(|snapshot| (key, snapshot));
        } else if output.response.dirty {
            self.contract_changed(ctx, label);
        }
        if output.lv_changed {
            self.editor.reload();
        }
        for request in output.requests {
            let file = match request {
                // The print dialog first (Stil, Blattrahmen), then the file.
                ChangeOrderRequest::PrintCheck(id) => {
                    self.open_change_order_print(ChangeOrderPrint::Check(id));
                    continue;
                }
                ChangeOrderRequest::PrintOverview => {
                    self.open_change_order_print(ChangeOrderPrint::Overview);
                    continue;
                }
                ChangeOrderRequest::Export(id, kind) => ChangeOrderFile::Export(id, kind),
                ChangeOrderRequest::ImportOffer(id) => ChangeOrderFile::Import(id),
            };
            self.open_file_dialog(FilePurpose::ChangeOrder(file));
        }
        self.handle_view_response(ctx, output.response);
    }

    /// File dialog for a Nachtrag file.
    pub(super) fn change_order_dialog(&self, file: ChangeOrderFile) -> FileDialog {
        let folder = self.project_folder();
        let name = self.project.name().as_str().to_string();
        let today = self.launcher.now_iso();
        let label_of = |id: ChangeOrderId| {
            self.project
                .change_order(id)
                .map(|order| order.label())
                .unwrap_or_default()
        };
        match file {
            ChangeOrderFile::Print(print) => {
                let word = match print {
                    ChangeOrderPrint::Check(id) => {
                        format!("{}_{}", print.file_label(), label_of(id))
                    }
                    ChangeOrderPrint::Overview => print.file_label().to_string(),
                };
                FileDialog::save(
                    &self.launcher,
                    print.label(),
                    "PDF zum Drucken oder Versenden.",
                    folder,
                    &file_dialog::suggested_name(&name, &word, &today, "pdf"),
                    "pdf",
                )
            }
            ChangeOrderFile::Export(id, kind) => FileDialog::save(
                &self.launcher,
                kind.label(),
                "Positionen des Nachtrags mit Nachtragsnummer (CONo) und Status (COStatus).",
                folder,
                &file_dialog::suggested_name(
                    &name,
                    &format!("Nachtrag_{}", label_of(id)),
                    &today,
                    kind.extension(),
                ),
                kind.extension(),
            ),
            ChangeOrderFile::Import(id) => FileDialog::open(
                &self.launcher,
                &format!("Nachtragsangebot einlesen ({})", label_of(id)),
                "GAEB-Datei der Firma (X84, X86, X82 …): angebotene Mengen und Preise.",
                folder,
                &OFFER_EXTENSIONS,
                false,
            ),
        }
    }

    pub(super) fn change_order_file_chosen(
        &mut self,
        ctx: &egui::Context,
        purpose: ChangeOrderFile,
        files: Vec<PathBuf>,
    ) {
        let Some(file) = files.first().cloned() else {
            return;
        };
        let exchange = self.launcher.shared_exchange();
        let project = self.project.clone();
        match purpose {
            ChangeOrderFile::Print(print) => {
                let logo = self.print_logo(ctx);
                let frame = self.launcher.preferences().page_frame;
                let look = self.take_context_look().look;
                let label = print.label();
                self.start_job(ctx, format!("{label} wird erzeugt …"), move || {
                    let result = exchange.export_change_order_pdf(
                        &project,
                        print,
                        logo.as_ref(),
                        &look,
                        &frame,
                        &file,
                    );
                    JobOutcome::ChangeOrder(ChangeOrderDone::Pdf {
                        label,
                        file,
                        result,
                    })
                });
            }
            ChangeOrderFile::Export(id, kind) => {
                let label = kind.label();
                self.start_job(ctx, format!("{label} wird erzeugt …"), move || {
                    let result = exchange.export_change_order_gaeb(&project, id, kind, &file);
                    JobOutcome::ChangeOrder(ChangeOrderDone::Gaeb {
                        label,
                        file,
                        result,
                    })
                });
            }
            ChangeOrderFile::Import(id) => {
                self.start_job(
                    ctx,
                    "Nachtragsangebot wird eingelesen …".to_string(),
                    move || {
                        let result = exchange.read_change_order_offer(&file).map(Box::new);
                        JobOutcome::ChangeOrder(ChangeOrderDone::Offer { id, file, result })
                    },
                );
            }
        }
    }

    pub(super) fn finish_change_order_job(&mut self, ctx: &egui::Context, done: ChangeOrderDone) {
        match done {
            ChangeOrderDone::Pdf {
                label,
                file,
                result,
            } => match result {
                Ok(pages) => self.say_with_file(
                    ctx,
                    format!(
                        "PDF gespeichert: {} ({})",
                        file_dialog::file_name(&file),
                        crate::format::count(pages, "Seite", "Seiten")
                    ),
                    Some(file),
                ),
                Err(error) => self.say(ctx, format!("{label}: {error}")),
            },
            ChangeOrderDone::Gaeb {
                label,
                file,
                result,
            } => match result {
                Ok(warnings) => {
                    if !warnings.is_empty() && self.dialog.is_none() {
                        self.dialog = Some(Dialog::hints(label, Some(&file), warnings));
                    }
                    self.say_with_file(
                        ctx,
                        format!("Exportiert: {}", file_dialog::file_name(&file)),
                        Some(file),
                    );
                }
                Err(error) => self.say(ctx, format!("{label}: {error}")),
            },
            ChangeOrderDone::Offer { id, file, result } => {
                let (offer, mut lines) = match result {
                    Ok(read) => *read,
                    Err(error) => {
                        self.say(ctx, format!("Nachtragsangebot: {error}"));
                        return;
                    }
                };
                let index = self.project.active_lv();
                let Some(before) = self.project.contract_snapshot_with_lv(index) else {
                    return;
                };
                let today = crate::format::iso_to_german_date(&self.launcher.now_iso());
                match self.project.take_change_order_offer(id, &offer, &today) {
                    Ok(result) => {
                        self.note_editor();
                        let key = self.active_key();
                        let now = ctx.input(|input| input.time);
                        self.history.record_contract(
                            key,
                            before,
                            "Nachtragsangebot einlesen",
                            now,
                            false,
                        );
                        self.contract_baseline = self
                            .project
                            .contract_snapshot(index)
                            .map(|snapshot| (key, snapshot));
                        self.editor.reload();
                        self.dirty = true;
                        self.change_orders.open(id);
                        self.say(
                            ctx,
                            format!(
                                "{}: {} übernommen, {} neu im Nachtragsbereich.",
                                file_dialog::file_name(&file),
                                crate::format::count(result.updated, "Position", "Positionen"),
                                result.created
                            ),
                        );
                        lines.extend(result.warnings);
                        if !lines.is_empty() && self.dialog.is_none() {
                            self.dialog = Some(Dialog::hints(
                                "Nachtragsangebot einlesen",
                                Some(&file),
                                lines,
                            ));
                        }
                    }
                    Err(error) => {
                        self.project.restore_contract(index, before);
                        self.say(ctx, format!("Nachtragsangebot: {error}"));
                    }
                }
            }
        }
    }
}
