//! Dialogs of the Kostenverfolgung: Budget (Kostenvorgabe) per KG with the
//! take-over from a Kostenermittlung, the choice of the Kostenermittlungen
//! and „Kostenstand festhalten“.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    group_label, parse_cost_group, parse_decimal_de, record_snapshot, stage_parts, BudgetKind,
    CostStage, CostTracking, Money, Project,
};

use super::{TrackingOutput, TrackingView};
use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets;

/// An open dialog of the Kostenverfolgung.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TrackingDialog {
    Budget(BudgetDialog),
    Sources,
    Record { date: String, note: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BudgetDialog {
    /// (KG, amount as typed).
    pub(crate) lines: Vec<(String, String)>,
    pub(crate) kind: BudgetKind,
    pub(crate) date: String,
    pub(crate) note: String,
    pub(crate) threshold: String,
    /// Index into the take-over choices.
    pub(crate) source: usize,
    pub(crate) error: Option<String>,
}

impl TrackingDialog {
    pub(crate) fn budget(project: &Project, today: &str) -> Self {
        let tracking = project.cost_tracking();
        TrackingDialog::Budget(BudgetDialog {
            lines: tracking
                .budget
                .iter()
                .map(|(code, amount)| (code.clone(), amount.plain()))
                .collect(),
            kind: tracking.budget_kind,
            date: if tracking.budget_date.is_empty() {
                today.to_string()
            } else {
                tracking.budget_date.clone()
            },
            note: tracking.budget_note.clone(),
            threshold: proava_domain::format_decimal_de(tracking.threshold_percent, 1)
                .trim_end_matches(",0")
                .to_string(),
            source: 0,
            error: None,
        })
    }

    pub(crate) fn record(today: &str) -> Self {
        TrackingDialog::Record {
            date: today.to_string(),
            note: String::new(),
        }
    }
}

/// What the budget can be taken from: each Kostenermittlung that counts and
/// the sum of each stage.
fn take_over_choices(project: &Project) -> Vec<(String, BTreeMap<String, Money>)> {
    let parts = stage_parts(project);
    let mut choices = Vec::new();
    for stage in [CostStage::Calculation, CostStage::Estimate] {
        let counting: Vec<_> = parts
            .iter()
            .filter(|part| part.stage == stage && part.counts)
            .collect();
        if counting.len() > 1 {
            let mut sums: BTreeMap<String, Money> = BTreeMap::new();
            for part in &counting {
                for (code, amount) in &part.sums {
                    if let Some(code) = code {
                        *sums.entry(code.clone()).or_default() += *amount;
                    }
                }
            }
            let total: Money = sums.values().copied().sum();
            choices.push((
                format!("{} (alle geltenden) – {total}", stage.label()),
                sums,
            ));
        }
        for part in counting {
            let sums: BTreeMap<String, Money> = part
                .sums
                .iter()
                .filter_map(|(code, amount)| code.clone().map(|code| (code, *amount)))
                .collect();
            choices.push((
                format!("{}: {} – {}", stage.label(), part.name, part.total()),
                sums,
            ));
        }
    }
    choices
}

impl TrackingView {
    pub(super) fn dialog_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &mut Project,
        output: &mut TrackingOutput,
    ) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let today = self.today.clone();
        let close = match dialog {
            TrackingDialog::Budget(budget) => {
                budget_ui(ui, palette, project, budget, &today, output)
            }
            TrackingDialog::Sources => sources_ui(ui, palette, project, output),
            TrackingDialog::Record { date, note } => {
                record_ui(ui, palette, project, date, note, output)
            }
        };
        if close {
            self.dialog = None;
        }
    }
}

fn budget_ui(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    dialog: &mut BudgetDialog,
    today: &str,
    output: &mut TrackingOutput,
) -> bool {
    let choices = take_over_choices(project);
    let (result, closed) = widgets::modal(ui, palette, 640.0, |ui| {
        widgets::title(
            ui,
            palette,
            "Kostenvorgabe (Budget)",
            "Budget bzw. Kostenvorgabe des Bauherrn (DIN 276, 4.6), je Kostengruppe netto. Eine Gruppe ohne eigene Kostenvorgabe summiert ihre Untergruppen.",
        );
        egui::Grid::new("budget_head")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Art");
                ui.horizontal(|ui| {
                    for kind in BudgetKind::ALL {
                        ui.radio_value(&mut dialog.kind, kind, kind.label());
                    }
                });
                ui.end_row();
                ui.label("Stand");
                ui.add(egui::TextEdit::singleline(&mut dialog.date).desired_width(120.0));
                ui.end_row();
                ui.label("Grundlage");
                ui.add(egui::TextEdit::singleline(&mut dialog.note).desired_width(420.0));
                ui.end_row();
                ui.label("Markierung");
                ui.horizontal(|ui| {
                    ui.label("über der Vorstufe ab");
                    ui.add(egui::TextEdit::singleline(&mut dialog.threshold).desired_width(50.0));
                    ui.label("%");
                });
                ui.end_row();
            });
        ui.add_space(8.0);
        if !choices.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Übernehmen aus");
                dialog.source = dialog.source.min(choices.len() - 1);
                egui::ComboBox::from_id_salt("budget_source")
                    .selected_text(choices[dialog.source].0.clone())
                    .width(380.0)
                    .show_ui(ui, |ui| {
                        for (index, (label, _)) in choices.iter().enumerate() {
                            ui.selectable_value(&mut dialog.source, index, label);
                        }
                    });
                if widgets::small_button(ui, palette, "übernehmen")
                    .on_hover_text(
                        "Ersetzt die Zeilen unten durch die Summen je KG dieser Kostenermittlung",
                    )
                    .clicked()
                {
                    let (label, sums) = &choices[dialog.source];
                    dialog.lines = sums
                        .iter()
                        .filter(|(_, amount)| !amount.is_zero())
                        .map(|(code, amount)| (code.clone(), amount.plain()))
                        .collect();
                    dialog.note = format!(
                        "aus {}",
                        label
                            .rsplit_once(" – ")
                            .map(|(name, _)| name)
                            .unwrap_or(label)
                    );
                    dialog.date = today.to_string();
                }
            });
            ui.add_space(6.0);
        }
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("budget_lines")
            .max_height(300.0)
            .show(ui, |ui| {
                egui::Grid::new("budget_lines_grid")
                    .num_columns(4)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("KG").strong());
                        ui.label(RichText::new("Bezeichnung").strong());
                        ui.label(RichText::new("Kostenvorgabe netto €").strong());
                        ui.label("");
                        ui.end_row();
                        for (index, (code, amount)) in dialog.lines.iter_mut().enumerate() {
                            ui.add(
                                egui::TextEdit::singleline(code)
                                    .id_salt(("budget_code", index))
                                    .desired_width(48.0),
                            );
                            ui.label(group_label(code.trim()));
                            ui.add(
                                egui::TextEdit::singleline(amount)
                                    .id_salt(("budget_amount", index))
                                    .align(egui::Align2::RIGHT_CENTER)
                                    .desired_width(130.0),
                            );
                            if widgets::tool_button(
                                ui,
                                palette,
                                Icon::Trash,
                                false,
                                "Zeile entfernen",
                            )
                            .clicked()
                            {
                                remove = Some(index);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(index) = remove {
            dialog.lines.remove(index);
        }
        if widgets::icon_label_button(ui, palette, Icon::Plus, "Kostengruppe").clicked() {
            dialog.lines.push((String::new(), String::new()));
        }
        let parsed = parse_budget(dialog);
        match &parsed {
            Ok(tracking) => {
                if let Some(total) = tracking.budget_total() {
                    ui.label(
                        RichText::new(format!("Kostenvorgabe gesamt {total}"))
                            .size(12.5)
                            .strong()
                            .color(palette.text),
                    );
                }
                for conflict in tracking.budget_conflicts() {
                    ui.label(RichText::new(conflict).size(12.0).color(palette.text));
                }
            }
            Err(error) => widgets::error_line(ui, palette, Some(error)),
        }
        ui.add_space(8.0);
        widgets::dialog_buttons(ui, palette, "Übernehmen")
    });
    let (cancel, confirm) = result;
    if confirm {
        match parse_budget(dialog) {
            Ok(parsed) => {
                let tracking = project.cost_tracking_mut();
                if tracking.budget != parsed.budget
                    || tracking.budget_kind != parsed.budget_kind
                    || tracking.budget_date != parsed.budget_date
                    || tracking.budget_note != parsed.budget_note
                    || tracking.threshold_percent != parsed.threshold_percent
                {
                    tracking.budget = parsed.budget;
                    tracking.budget_kind = parsed.budget_kind;
                    tracking.budget_date = parsed.budget_date;
                    tracking.budget_note = parsed.budget_note;
                    tracking.threshold_percent = parsed.threshold_percent;
                    output.change("Kostenvorgabe", false);
                }
                return true;
            }
            Err(error) => {
                dialog.error = Some(error);
                return false;
            }
        }
    }
    cancel || closed
}

/// The budget as typed; an error names the line.
fn parse_budget(dialog: &BudgetDialog) -> Result<CostTracking, String> {
    let mut tracking = CostTracking {
        budget_kind: dialog.kind,
        budget_date: dialog.date.trim().to_string(),
        budget_note: dialog.note.trim().to_string(),
        ..CostTracking::default()
    };
    if !dialog.threshold.trim().is_empty() {
        tracking.threshold_percent = parse_decimal_de(&dialog.threshold)
            .map_err(|_| format!("Markierung: „{}“ ist keine Zahl.", dialog.threshold.trim()))?;
    }
    for (code, amount) in &dialog.lines {
        if code.trim().is_empty() && amount.trim().is_empty() {
            continue;
        }
        let code = parse_cost_group(code)?
            .ok_or_else(|| "Bitte zu jedem Betrag die Kostengruppe eintragen.".to_string())?;
        let value = if amount.trim().is_empty() {
            Money::ZERO
        } else {
            Money::from_decimal(
                parse_decimal_de(amount)
                    .map_err(|_| format!("KG {code}: „{}“ ist kein Betrag.", amount.trim()))?,
            )
        };
        if tracking.budget.contains_key(&code) {
            return Err(format!("KG {code} steht zweimal in der Liste."));
        }
        tracking.set_budget(&code, value);
    }
    Ok(tracking)
}

fn sources_ui(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    output: &mut TrackingOutput,
) -> bool {
    let parts = stage_parts(project);
    let (close, closed) = widgets::modal(ui, palette, 640.0, |ui| {
        widgets::title(
            ui,
            palette,
            "Kostenermittlungen der Kostenverfolgung",
            "Kosten-LVs der Gewerke zählen zusammen; von den Datenpunktrechnungen zählt je Stufe die neueste. Doppelt erfasste Kostengruppen hier abwählen.",
        );
        if parts.is_empty() {
            ui.label(
                RichText::new(
                    "Im Projekt gibt es noch keine Kostenschätzung oder Kostenberechnung.",
                )
                .color(palette.text_dim),
            );
        }
        for stage in CostStage::ALL {
            let of_stage: Vec<_> = parts.iter().filter(|part| part.stage == stage).collect();
            if of_stage.is_empty() {
                continue;
            }
            ui.label(
                RichText::new(stage.label())
                    .size(13.0)
                    .strong()
                    .color(palette.text),
            );
            for part in of_stage {
                let mut counts = part.counts;
                let date = if part.date.is_empty() {
                    String::new()
                } else {
                    format!(", Stand {}", part.date)
                };
                let label = format!(
                    "{} ({}{date}) – {}{}",
                    part.name,
                    part.method,
                    part.total(),
                    if part.by_default {
                        ""
                    } else {
                        " – nicht in der Vorgabe"
                    }
                );
                if ui.checkbox(&mut counts, label).changed() {
                    let choice = &mut project.cost_tracking_mut().source_choice;
                    if counts == part.by_default {
                        choice.remove(&part.key);
                    } else {
                        choice.insert(part.key.clone(), counts);
                    }
                    output.change("Kostenermittlungen wählen", false);
                }
            }
            ui.add_space(6.0);
        }
        ui.add_space(8.0);
        widgets::button_row(ui, |ui| {
            widgets::button(ui, palette, "Schließen", true, 110.0).clicked()
        })
    });
    close || closed
}

fn record_ui(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    date: &mut String,
    note: &mut String,
    output: &mut TrackingOutput,
) -> bool {
    let mut delete = None;
    let ((cancel, confirm), closed) = widgets::modal(ui, palette, 560.0, |ui| {
        widgets::title(
            ui,
            palette,
            "Kostenstand festhalten",
            "Hält die heutige Kette aller Stufen mit Datum und Anlass fest; später lässt sich die Prognose damit vergleichen (DIN 276, 4.4.3).",
        );
        egui::Grid::new("record_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Datum");
                ui.add(egui::TextEdit::singleline(date).desired_width(120.0));
                ui.end_row();
                ui.label("Anlass");
                ui.add(
                    egui::TextEdit::singleline(note)
                        .hint_text("z. B. Kostenberechnung abgestimmt")
                        .desired_width(380.0),
                );
                ui.end_row();
            });
        let snapshots = &project.cost_tracking().snapshots;
        if !snapshots.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new("Festgehaltene Kostenstände")
                    .strong()
                    .color(palette.text),
            );
            for (index, snapshot) in snapshots.iter().enumerate() {
                ui.horizontal(|ui| {
                    if widgets::tool_button(ui, palette, Icon::Trash, false, "Kostenstand löschen")
                        .clicked()
                    {
                        delete = Some(index);
                    }
                    ui.label(snapshot.label());
                });
            }
        }
        ui.add_space(8.0);
        widgets::dialog_buttons(ui, palette, "Festhalten")
    });
    if let Some(index) = delete {
        project.cost_tracking_mut().snapshots.remove(index);
        output.change("Kostenstand löschen", false);
        return false;
    }
    if confirm {
        let snapshot = record_snapshot(project, date.trim(), note.trim());
        project.cost_tracking_mut().snapshots.push(snapshot);
        output.change("Kostenstand festhalten", false);
        return true;
    }
    cancel || closed
}
