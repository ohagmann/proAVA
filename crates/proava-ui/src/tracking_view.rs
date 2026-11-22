//! Reiter „Kostenverfolgung“: Budget → Kostenschätzung → Kostenberechnung →
//! Kostenvoranschlag → Angebot → Auftrag → Prognose → Abrechnung →
//! Kostenfeststellung je Kostengruppe (1.–3. Ebene) oder je Vergabeeinheit
//! (DIN 276, 4.4–4.6). Live aus den Projektdaten berechnet und
//! zwischengespeichert, bis sich das Projekt ändert; gespeichert werden nur
//! Budget, Erläuterungen, die Wahl der Kostenermittlungen und Angebote und
//! festgehaltene Kostenstände.

mod dialogs;
mod table;

use std::collections::HashSet;

use eframe::egui::{self, RichText};
use proava_domain::{
    cost_tracking, format_decimal_de, BidId, CostEstimate, CostTracking, Deviation, Money,
    OfferChoice, Project, RowId, TrackDeviation, TrackOptions, TrackReport, TrackRow, TrackRowKind,
    TrackStage, TrackView,
};

use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets;

pub(crate) use dialogs::TrackingDialog;

/// Whether the project has something to follow: a Kostenermittlung, a
/// Kosten-LV, a budget or a contract (tab, print, menu).
pub fn has_tracking(project: &Project) -> bool {
    !project.cost_estimates().is_empty()
        || !project.cost_lv_indices().is_empty()
        || !project.cost_tracking().is_empty()
        || (0..project.lv_count()).any(|index| project.contract_of(index).is_some())
}

/// What the workspace does for the view.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackingRequest {
    /// Druckdialog „Kostenverfolgung“.
    Print,
    /// Export as CSV (file dialog).
    Csv,
    /// Show this position in the LV (row „ohne Kostengruppe“).
    ShowPosition(usize, RowId),
    /// The bid that counts as „Angebot“ of an LV (undo with the bids).
    ChooseOffer(usize, Option<OfferChoice>),
}

#[derive(Default)]
pub struct TrackingOutput {
    /// The Kostenverfolgung data changed: label of the undo step, join
    /// (typing in a field).
    pub changed: Option<(&'static str, bool)>,
    pub requests: Vec<TrackingRequest>,
}

impl TrackingOutput {
    fn change(&mut self, label: &'static str, join: bool) {
        self.changed = Some((label, join));
    }
}

struct Cache {
    revision: u64,
    estimates: Vec<CostEstimate>,
    tracking: CostTracking,
    options: TrackOptions,
    report: TrackReport,
}

#[derive(Default)]
pub struct TrackingView {
    pub(crate) options: TrackOptions,
    /// Keys of collapsed group and LV rows.
    collapsed: HashSet<String>,
    selected: Option<String>,
    cache: Option<Cache>,
    /// Erläuterung being typed: (KG, text).
    note: Option<(String, String)>,
    /// Reason of the chosen bid being typed: (LV, text).
    reason: Option<(usize, String)>,
    pub(crate) dialog: Option<TrackingDialog>,
    /// TT.MM.JJJJ of today (Kostenstand, Budget-Stand).
    pub(crate) today: String,
}

/// Identity of a row for collapsing and selection.
pub(crate) fn row_key(row: &TrackRow) -> String {
    format!(
        "{:?}|{}|{}",
        row.kind,
        row.lv.map(|lv| lv.to_string()).unwrap_or_default(),
        row.code.as_deref().unwrap_or("")
    )
}

/// „1.234,56“
pub(crate) fn amount(value: Money) -> String {
    value.plain()
}

/// „+1.234,56 € / +12,3 %“
/// „01.10.2026“ of a recorded state's label („01.10.2026 – Anlass“).
pub(crate) fn short_date(label: &str) -> &str {
    label.split(" – ").next().unwrap_or(label)
}

pub(crate) fn deviation_text(deviation: &Deviation) -> String {
    let sign = if deviation.amount.cents() > 0 {
        "+"
    } else {
        ""
    };
    match deviation.percent {
        Some(percent) => format!(
            "{sign}{} € / {sign}{} %",
            deviation.amount.plain(),
            format_decimal_de(percent, 1)
        ),
        None => format!("{sign}{} €", deviation.amount.plain()),
    }
}

impl TrackingView {
    /// After undo/redo: typed texts are read again from the project.
    pub fn reset_editing(&mut self) {
        self.note = None;
        self.reason = None;
    }

    pub fn set_today(&mut self, today_iso: &str) {
        self.today = crate::format::iso_to_german_date(today_iso);
    }

    /// The report for the current options, computed again only after a change.
    pub fn report(&mut self, project: &Project) -> &TrackReport {
        let fresh = self.cache.as_ref().is_some_and(|cache| {
            cache.revision == project.revision()
                && cache.options == self.options
                && cache.tracking == *project.cost_tracking()
                && cache.estimates.as_slice() == project.cost_estimates()
        });
        if !fresh {
            self.cache = Some(Cache {
                revision: project.revision(),
                estimates: project.cost_estimates().to_vec(),
                tracking: project.cost_tracking().clone(),
                options: self.options.clone(),
                report: cost_tracking(project, &self.options),
            });
        }
        &self.cache.as_ref().expect("just computed").report
    }

    /// Text of the status line.
    pub fn status_text(&mut self, project: &Project) -> String {
        let report = self.report(project);
        let forecast = report.total.value(TrackStage::Forecast).unwrap_or_default();
        let mut text = format!(
            "Kostenverfolgung {} · Prognose {}",
            report.amounts_text(),
            forecast
        );
        if let Some(budget) = report.total.value(TrackStage::Budget) {
            text.push_str(&format!(" · Kostenvorgabe {budget}"));
        }
        text
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
    ) -> TrackingOutput {
        let mut output = TrackingOutput::default();
        self.dialog_ui(ui, palette, project, &mut output);
        self.toolbar(ui, palette, project, &mut output);
        ui.add_space(6.0);
        let report = self.report(project).clone();
        self.key_figures(ui, palette, &report);
        if !report.warnings.is_empty() {
            ui.add_space(4.0);
            egui::CollapsingHeader::new(
                RichText::new(format!("Hinweise ({})", report.warnings.len()))
                    .size(12.5)
                    .color(palette.text),
            )
            .id_salt("tracking_hints")
            .default_open(false)
            .show(ui, |ui| {
                for warning in &report.warnings {
                    ui.label(RichText::new(warning).size(12.0).color(palette.text));
                }
            });
        }
        ui.add_space(6.0);

        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, egui::Sense::hover());
        let narrow = area.width() < 980.0;
        let gap = 12.0;
        let (main_rect, side_rect) = if narrow {
            let side = (area.height() * 0.38)
                .clamp(120.0, 320.0)
                .min(area.height() - 120.0)
                .max(0.0);
            (
                egui::Rect::from_min_max(
                    area.min,
                    egui::pos2(area.right(), area.bottom() - side - gap),
                ),
                egui::Rect::from_min_max(egui::pos2(area.left(), area.bottom() - side), area.max),
            )
        } else {
            let width = (area.width() * 0.2).clamp(280.0, 340.0);
            (
                egui::Rect::from_min_max(
                    area.min,
                    egui::pos2(area.right() - width - gap, area.bottom()),
                ),
                egui::Rect::from_min_max(egui::pos2(area.right() - width, area.top()), area.max),
            )
        };
        let clip = ui.clip_rect();
        let mut main_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("tracking_main")
                .max_rect(main_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        main_ui.set_clip_rect(main_rect.intersect(clip));
        if let Some(key) = table::show(
            &mut main_ui,
            palette,
            &report,
            self.options.deviation,
            &mut self.collapsed,
            self.selected.as_deref(),
        ) {
            self.selected = Some(key);
        }
        let mut side_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("tracking_side")
                .max_rect(side_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        side_ui.set_clip_rect(side_rect.intersect(clip));
        egui::ScrollArea::vertical()
            .id_salt("tracking_side_scroll")
            .auto_shrink([false, false])
            .show(&mut side_ui, |ui| {
                self.side(ui, palette, project, &report, &mut output);
            });
        output
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut TrackingOutput,
    ) {
        let label = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
        };
        // Several Umsatzsteuersätze: named under „brutto“.
        let details = self.report(project).vat_details();
        let gross_hint = if details.is_empty() {
            "Brutto je LV und Kostenermittlung mit deren Umsatzsteuer".to_string()
        } else {
            format!(
                "Brutto je LV und Kostenermittlung mit deren Umsatzsteuer – verschiedene Sätze:\n{}",
                details.join("\n")
            )
        };
        widgets::row_wrapped(ui, |ui| {
            label(ui, "Ansicht");
            ui.selectable_value(
                &mut self.options.view,
                TrackView::ByGroup,
                "nach Kostengruppen",
            )
            .on_hover_text("Je Kostengruppe nach DIN 276 mit den Summen der 1. und 2. Ebene");
            ui.selectable_value(
                &mut self.options.view,
                TrackView::ByLv,
                "nach Vergabeeinheiten",
            )
            .on_hover_text("Je LV (Vergabeeinheit, DIN 276 4.3.5/4.3.6) mit seinen Kostengruppen");
            ui.add_space(10.0);
            label(ui, "Ebene");
            for level in 1..=3u8 {
                ui.selectable_value(&mut self.options.level, level, level.to_string())
                    .on_hover_text(format!("Kostengruppen bis zur {level}. Ebene (DIN 276)"));
            }
            ui.add_space(10.0);
            ui.selectable_value(&mut self.options.gross, false, "netto");
            ui.selectable_value(&mut self.options.gross, true, "brutto")
                .on_hover_text(gross_hint);
            ui.add_space(10.0);
            label(ui, "Abweichung");
            // „zum Stand“ only while a recorded state is chosen.
            for mode in TrackDeviation::ALL
                .into_iter()
                .filter(|mode| *mode != TrackDeviation::Snapshot || self.options.snapshot.is_some())
            {
                ui.selectable_value(&mut self.options.deviation, mode, mode.label())
                    .on_hover_text(match mode {
                        TrackDeviation::Snapshot => {
                            "Jede Stufe gegen dieselbe Stufe des festgehaltenen Kostenstands"
                        }
                        TrackDeviation::Previous => {
                            "Je Vergabeeinheit gegen die letzte frühere Stufe; Kostenermittlungen je KG"
                        }
                        _ => "",
                    });
            }
            ui.add_space(10.0);
            label(ui, "Vergleich");
            let snapshots = &project.cost_tracking().snapshots;
            if self
                .options
                .snapshot
                .is_some_and(|index| index >= snapshots.len())
            {
                self.options.snapshot = None;
            }
            let chosen = self.options.snapshot;
            let current = self
                .options
                .snapshot
                .and_then(|index| snapshots.get(index))
                .map(|snapshot| format!("Stand {}", snapshot.label()))
                .unwrap_or_else(|| "kein Kostenstand".to_string());
            ui.add_enabled_ui(!snapshots.is_empty(), |ui| {
                egui::ComboBox::from_id_salt("tracking_compare")
                    .selected_text(current)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.options.snapshot, None, "kein Kostenstand");
                        for (index, snapshot) in snapshots.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.options.snapshot,
                                Some(index),
                                format!("Stand {}", snapshot.label()),
                            );
                        }
                    })
                    .response
                    .on_hover_text(
                        "Alle Stufen gegen einen festgehaltenen Kostenstand vergleichen",
                    );
            });
            // Choosing a state shows the changes since then; without one
            // that deviation has nothing to show.
            if self.options.snapshot != chosen {
                if self.options.snapshot.is_some() {
                    self.options.deviation = TrackDeviation::Snapshot;
                } else if self.options.deviation == TrackDeviation::Snapshot {
                    self.options.deviation = TrackDeviation::default();
                }
            }
        });
        ui.add_space(4.0);
        widgets::row_wrapped(ui, |ui| {
            if widgets::icon_label_button(ui, palette, Icon::Field, "Kostenvorgabe (Budget) …")
                .on_hover_text("Budget bzw. Kostenvorgabe des Bauherrn (DIN 276, 4.6) je KG eingeben oder aus einer Kostenermittlung übernehmen")
                .clicked()
            {
                self.dialog = Some(TrackingDialog::budget(project, &self.today));
            }
            if widgets::icon_label_button(ui, palette, Icon::Gear, "Kostenermittlungen …")
                .on_hover_text("Welche Kostenschätzungen und -berechnungen zählen")
                .clicked()
            {
                self.dialog = Some(TrackingDialog::Sources);
            }
            if widgets::icon_label_button(ui, palette, Icon::Calendar, "Kostenstand festhalten …")
                .on_hover_text("Die heutige Kette mit Datum und Anlass festhalten (DIN 276, 4.4.3)")
                .clicked()
            {
                self.dialog = Some(TrackingDialog::record(&self.today));
            }
            if widgets::icon_label_button(ui, palette, Icon::Import, "CSV …")
                .on_hover_text("Die Tabelle wie gezeigt als CSV (Excel, LibreOffice)")
                .clicked()
            {
                output.requests.push(TrackingRequest::Csv);
            }
            if widgets::icon_label_button(ui, palette, Icon::Document, "Drucken …")
                .on_hover_text("PDF der Kostenverfolgung (A4 quer, Druckdialog)")
                .clicked()
            {
                output.requests.push(TrackingRequest::Print);
            }
        });
    }

    fn key_figures(&self, ui: &mut egui::Ui, palette: &Palette, report: &TrackReport) {
        let total = &report.total;
        let mut parts: Vec<String> = Vec::new();
        if let Some(budget) = total.value(TrackStage::Budget) {
            parts.push(format!(
                "Kostenvorgabe {budget} ({}{})",
                report.budget_kind.label(),
                if report.budget_date.is_empty() {
                    String::new()
                } else {
                    format!(", Stand {}", report.budget_date)
                }
            ));
        }
        if let Some(forecast) = total.value(TrackStage::Forecast) {
            parts.push(format!("Prognose {forecast}"));
            if let Some(deviation) = total.to_budget(TrackStage::Forecast) {
                parts.push(format!("zur Kostenvorgabe {}", deviation_text(&deviation)));
            }
        }
        if let Some(contract) = total.value(TrackStage::Contract) {
            parts.push(format!("Auftrag mit Nachträgen {contract}"));
        }
        if let Some(billed) = total.value(TrackStage::Billed) {
            let released = if report.options.gross {
                report.released_gross
            } else {
                report.released_net
            };
            parts.push(format!("abgerechnet {billed}, Freigaben {released}"));
        }
        if report.contract_lvs > 0 {
            parts.push(report.final_text());
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Kostenstand {} · {}",
                    self.today,
                    report.amounts_text()
                ))
                .size(12.5)
                .strong()
                .color(palette.text),
            );
            for part in parts {
                ui.label(RichText::new("·").size(12.5).color(palette.text_dim));
                ui.label(RichText::new(part).size(12.5).color(palette.text));
            }
            if let Some(snapshot) = &report.snapshot {
                ui.label(RichText::new("·").size(12.5).color(palette.text_dim));
                ui.label(
                    RichText::new(format!("Vergleich mit Stand {snapshot}"))
                        .size(12.5)
                        .color(palette.text),
                );
            }
        });
    }

    /// Details of the selected row: all stages with deviations, Prognose,
    /// Erläuterung, Angebot of an LV, positions without KG.
    fn side(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        report: &TrackReport,
        output: &mut TrackingOutput,
    ) {
        let heading = |ui: &mut egui::Ui, text: &str| {
            ui.label(RichText::new(text).size(13.0).strong().color(palette.text));
        };
        let row = self
            .selected
            .as_ref()
            .and_then(|key| report.rows.iter().find(|row| row_key(row) == *key))
            .unwrap_or(&report.total);
        heading(ui, &row.title());
        ui.add_space(4.0);
        // One block per stage: name and amount on a line, the deviations
        // below it (wrapping in the narrow panel).
        let line = |ui: &mut egui::Ui, name: &str, hint: &str, value: String| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).size(12.0).color(palette.text_dim))
                    .on_hover_text(hint);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(value).size(12.0).strong().color(palette.text));
                });
            });
        };
        let detail = |ui: &mut egui::Ui, text: String| {
            ui.indent("tracking_side_detail", |ui| {
                ui.add(egui::Label::new(RichText::new(text).size(11.5).color(palette.text)).wrap());
            });
        };
        for stage in TrackStage::ALL {
            let value = row.value(stage);
            if value.is_none() && row.compared(stage).is_none() {
                continue;
            }
            line(
                ui,
                stage.label(),
                stage.explanation(),
                value.map_or_else(|| "–".to_string(), |value| value.to_string()),
            );
            if let Some(deviation) = row.to_budget(stage) {
                detail(
                    ui,
                    format!("zur Kostenvorgabe {}", deviation_text(&deviation)),
                );
            }
            if let Some(previous) = row.to_previous(stage) {
                detail(
                    ui,
                    format!(
                        "{} {}",
                        previous.label(),
                        deviation_text(&previous.deviation)
                    ),
                );
            }
            if let (Some(snapshot), Some(deviation)) = (&report.snapshot, row.to_snapshot(stage)) {
                let then = row
                    .compared(stage)
                    .map_or_else(|| "–".to_string(), |value| value.to_string());
                detail(
                    ui,
                    format!(
                        "Stand {}: {then}, seitdem {}",
                        short_date(snapshot),
                        deviation_text(&deviation)
                    ),
                );
            }
            ui.add_space(3.0);
        }

        // Vergabeeinheit: Angebot, Prognose.
        if let Some(index) = row.lv.filter(|_| row.kind == TrackRowKind::Lv) {
            if let Some(lv) = report.lvs.iter().find(|lv| lv.index == index) {
                ui.add_space(8.0);
                heading(ui, "Vergabeeinheit");
                if !lv.contractor.is_empty() {
                    ui.label(
                        RichText::new(format!("Auftragnehmer: {}", lv.contractor))
                            .size(12.0)
                            .color(palette.text),
                    );
                }
                if let Some(offer) = &lv.offer {
                    ui.label(
                        RichText::new(format!("Angebot: {offer}"))
                            .size(12.0)
                            .color(palette.text),
                    );
                }
                if !lv.forecast_basis.is_empty() {
                    ui.label(
                        RichText::new(format!("Prognose aus: {}", lv.forecast_basis))
                            .size(12.0)
                            .color(palette.text),
                    );
                }
                if !lv.released_net.is_zero() {
                    ui.label(
                        RichText::new(format!(
                            "Freigaben netto {}, brutto {}",
                            lv.released_net, lv.released_gross
                        ))
                        .size(12.0)
                        .color(palette.text),
                    );
                }
                if !lv.has_contract {
                    self.offer_choice(ui, palette, project, index, output);
                }
            }
        }

        // Erläuterung/Maßnahme of a KG.
        if let Some(code) = row.code.clone().filter(|_| row.kind != TrackRowKind::Lv) {
            ui.add_space(8.0);
            heading(ui, &format!("Erläuterung / Maßnahme KG {code}"));
            ui.label(
                RichText::new("Abweichung erläutern, bewerten und Maßnahmen festhalten (DIN 276, 4.4.3 und 4.5).")
                    .size(11.5)
                    .color(palette.text_dim),
            );
            let stored = project
                .cost_tracking()
                .notes
                .get(&code)
                .cloned()
                .unwrap_or_default();
            if self.note.as_ref().is_none_or(|(known, _)| *known != code) {
                self.note = Some((code.clone(), stored.clone()));
            }
            if let Some((_, text)) = self.note.as_mut() {
                let response = ui.add(
                    egui::TextEdit::multiline(text)
                        .id_salt("tracking_note")
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                if response.changed() && *text != stored {
                    project.cost_tracking_mut().set_note(&code, text);
                    output.change("Erläuterung", true);
                }
            }
        }

        // Positions without KG.
        if row.kind == TrackRowKind::Without
            || (row.kind == TrackRowKind::Total && !report.without_group.is_empty())
        {
            let positions: Vec<_> = report
                .without_group
                .iter()
                .filter(|item| {
                    row.kind == TrackRowKind::Total || row.lv.is_none_or(|lv| lv == item.lv)
                })
                .collect();
            if !positions.is_empty() {
                ui.add_space(8.0);
                heading(ui, &format!("Ohne Kostengruppe ({})", positions.len()));
                for item in positions.into_iter().take(200) {
                    ui.horizontal(|ui| {
                        if widgets::small_button(ui, palette, "zeigen")
                            .on_hover_text("Position im LV zeigen, dort die Kostengruppe eintragen")
                            .clicked()
                        {
                            output
                                .requests
                                .push(TrackingRequest::ShowPosition(item.lv, item.row));
                        }
                        ui.label(
                            RichText::new(format!(
                                "{} · {} {} – {}",
                                item.lv_label, item.ordinal, item.short_text, item.amount
                            ))
                            .size(12.0)
                            .color(palette.text),
                        );
                    });
                }
            }
        }

        if row.kind == TrackRowKind::Total {
            ui.add_space(8.0);
            heading(ui, "Grundlagen");
            for source in &report.sources {
                ui.label(RichText::new(source).size(12.0).color(palette.text));
            }
            if report.sources.is_empty() {
                ui.label(
                    RichText::new("Keine Kostenermittlung im Projekt.")
                        .size(12.0)
                        .color(palette.text_dim),
                );
            }
            if !report.assumptions.is_empty() {
                ui.add_space(6.0);
                heading(ui, "Prognose – Annahmen");
                for text in &report.assumptions {
                    ui.label(RichText::new(text).size(12.0).color(palette.text));
                }
            }
        }
    }

    /// Angebot of an LV without contract: lowest bid or a chosen one with reason.
    fn offer_choice(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        index: usize,
        output: &mut TrackingOutput,
    ) {
        let Some(lv) = project.lv_at(index) else {
            return;
        };
        let bids: Vec<(BidId, String)> = project
            .bids_of(index)
            .iter()
            .filter(|bid| !bid.is_excluded())
            .map(|bid| {
                (
                    bid.id(),
                    format!("{} – {}", bid.name(), bid.total_after_discount(lv)),
                )
            })
            .collect();
        if bids.is_empty() {
            return;
        }
        ui.add_space(6.0);
        let caption = ui.label(
            RichText::new("Angebot für die Kostenverfolgung")
                .size(12.0)
                .strong()
                .color(palette.text),
        );
        let choice = project.offer_choice_of(index).cloned();
        let mut chosen = choice.as_ref().map(|choice| choice.bid);
        let current = chosen
            .and_then(|id| bids.iter().find(|(bid, _)| *bid == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Mindestbieter (automatisch)".to_string());
        egui::ComboBox::from_id_salt(("tracking_offer", index))
            .selected_text(current)
            .width(ui.available_width().min(320.0))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, "Mindestbieter (automatisch)");
                for (id, name) in &bids {
                    ui.selectable_value(&mut chosen, Some(*id), name);
                }
            })
            .response
            .labelled_by(caption.id);
        if chosen != choice.as_ref().map(|choice| choice.bid) {
            let reason = choice
                .as_ref()
                .map(|choice| choice.reason.clone())
                .unwrap_or_default();
            output.requests.push(TrackingRequest::ChooseOffer(
                index,
                chosen.map(|bid| OfferChoice { bid, reason }),
            ));
            self.reason = None;
        }
        if let Some(choice) = choice {
            ui.label(
                RichText::new("Begründung")
                    .size(11.5)
                    .color(palette.text_dim),
            );
            if self
                .reason
                .as_ref()
                .is_none_or(|(known, _)| *known != index)
            {
                self.reason = Some((index, choice.reason.clone()));
            }
            if let Some((_, text)) = self.reason.as_mut() {
                let response = ui.add(
                    egui::TextEdit::multiline(text)
                        .id_salt(("tracking_reason", index))
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if response.lost_focus() && *text != choice.reason {
                    output.requests.push(TrackingRequest::ChooseOffer(
                        index,
                        Some(OfferChoice {
                            bid: choice.bid,
                            reason: text.clone(),
                        }),
                    ));
                }
            }
        }
    }
}
