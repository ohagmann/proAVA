//! Rechte Seite der Kosten-Ansicht: Ergebnis, Aufteilung auf KG 481–489
//! (Vorgabe und je Bauteil) mit Prüfsumme, Zusammenfassung über alle
//! Bauteile, Kostenkontrolle gegen die LVs und Hinweise.

use std::collections::HashMap;

use eframe::egui::{self, RichText};
use proava_domain::{
    cost_group_label, format_decimal_de, CostControl, CostEstimate, CostResult, CostShare,
    CostStage, Decimal, Money, NumberKind, ShareLine, DIN276_KG400,
};

use super::{decimal_field, CostsOutput, CostsView};
use crate::icons::Icon;
use crate::theme::Palette;
use crate::widgets;

pub(super) fn show(
    view: &mut CostsView,
    ui: &mut egui::Ui,
    palette: &Palette,
    estimate: &mut CostEstimate,
    control: Option<&CostControl>,
    output: &mut CostsOutput,
) {
    let result = estimate.result();
    summary_box(ui, palette, estimate, &result);
    ui.add_space(10.0);
    if estimate.stage == CostStage::Calculation {
        heading(ui, palette, "Aufteilung auf die Kostengruppen (Vorgabe)");
        hint(
            ui,
            palette,
            "Gilt für alle Bauteile ohne eigene Aufteilung.",
        );
        if share_editor(
            ui,
            palette,
            &mut view.buffers,
            "cost_default_shares",
            &mut estimate.shares,
            None,
        ) {
            output.change("Aufteilung ändern", true);
        }
        ui.add_space(10.0);
        heading(ui, palette, "Bauteile");
        let defaults = estimate.shares.clone();
        for (index, section) in estimate.sections.iter_mut().enumerate() {
            let sums = &result.sections[index];
            let name = if section.name.trim().is_empty() {
                format!("Bauteil {}", index + 1)
            } else {
                section.name.clone()
            };
            let ok = sums.check.is_zero();
            let title = format!(
                "{name} – {}{}",
                sums.total,
                if ok { "" } else { " – Prüfsumme prüfen" }
            );
            let open = view.open_section == Some(index);
            let response =
                egui::CollapsingHeader::new(RichText::new(title).size(12.5).color(if ok {
                    palette.text
                } else {
                    palette.error
                }))
                .id_salt(("cost_section_split", index))
                .open(Some(open))
                .show(ui, |ui| {
                    let mut own = section.shares.is_some();
                    if ui
                        .checkbox(&mut own, "eigene Aufteilung")
                        .on_hover_text("Prozentsätze nur für dieses Bauteil; aus = Vorgabe oben")
                        .changed()
                    {
                        section.shares = own.then(|| defaults.clone());
                        output.change("Aufteilung ändern", false);
                    }
                    match section.shares.as_mut() {
                        Some(shares) => {
                            if share_editor(
                                ui,
                                palette,
                                &mut view.buffers,
                                ("cost_section_shares", index),
                                shares,
                                Some(&sums.shares),
                            ) {
                                output.change("Aufteilung ändern", true);
                            }
                        }
                        None => share_table(ui, palette, &sums.shares),
                    }
                    check_line(ui, palette, sums.percent_sum, sums.check);
                });
            if response.header_response.clicked() {
                view.open_section = if open { None } else { Some(index) };
            }
        }
        ui.add_space(10.0);
        heading(ui, palette, "Zusammenfassung aller Bauteile");
        groups_table(ui, palette, &result);
    } else {
        heading(ui, palette, "Bauteile");
        cells(
            ui,
            palette,
            &[
                ("Bauteil", 0.0, false, false, true),
                ("DP", 44.0, true, false, true),
                ("KG 480", 240.0, true, false, true),
            ],
            0,
        );
        for (section, sums) in estimate.sections.iter().zip(&result.sections) {
            let amount = match (sums.low, sums.high) {
                (Some(low), Some(high)) => format!(
                    "{} ({} – {})",
                    sums.total.plain(),
                    low.plain(),
                    high.plain()
                ),
                _ => sums.total.to_string(),
            };
            cells(
                ui,
                palette,
                &[
                    (&section.name, 0.0, false, false, false),
                    (&sums.points.to_string(), 44.0, true, false, false),
                    (&amount, 240.0, true, false, false),
                ],
                0,
            );
        }
        cells(
            ui,
            palette,
            &[
                ("Gesamt", 0.0, false, true, false),
                (&result.points.to_string(), 44.0, true, true, false),
                (&result.total.to_string(), 240.0, true, true, false),
            ],
            0,
        );
    }
    if let Some(control) = control.filter(|control| control.has_planner || control.has_award) {
        ui.add_space(10.0);
        heading(ui, palette, "Kostenkontrolle");
        hint(
            ui,
            palette,
            &format!(
                "{}: diese {}. KV: Kostenvoranschlag = Planerpreise der Ausschreibungs-LVs je KG (vom Titel geerbt). Auftrag: beauftragte Angebote netto vor Nachlass.",
                if estimate.stage == CostStage::Estimate { "KS" } else { "KB" },
                estimate.stage.label()
            ),
        );
        control_table(ui, palette, estimate.stage, control);
    }
    if !result.warnings.is_empty() {
        ui.add_space(10.0);
        heading(ui, palette, "Hinweise");
        for warning in &result.warnings {
            ui.label(RichText::new(warning).size(12.0).color(palette.warn));
        }
    }
}

fn heading(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(13.0).strong().color(palette.text));
    ui.add_space(2.0);
}

fn hint(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(palette.text_dim));
}

fn dim(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(palette.text_dim));
}

fn right(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(text);
    });
}

fn percent(value: Decimal) -> String {
    format!(
        "{} %",
        format_decimal_de(
            value.round_dp(2).normalize(),
            value.round_dp(2).normalize().scale()
        )
    )
}

fn summary_box(ui: &mut egui::Ui, palette: &Palette, estimate: &CostEstimate, result: &CostResult) {
    egui::Frame::new()
        .fill(palette.accent_soft)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let label = match estimate.stage {
                CostStage::Estimate => "KG 480 Gebäude- und Anlagenautomation (netto)",
                CostStage::Calculation => "Gesamtkosten KG 481–489 (netto)",
            };
            ui.label(RichText::new(label).size(11.5).color(palette.text_dim));
            ui.label(
                RichText::new(result.total.to_string())
                    .size(22.0)
                    .strong()
                    .color(palette.text),
            );
            let mut facts = vec![crate::format::count(
                result.points as usize,
                "Datenpunkt",
                "Datenpunkte",
            )];
            if let (Some(low), Some(high)) = (result.low, result.high) {
                facts.push(format!("Spanne {} bis {}", low, high));
            }
            if estimate.stage == CostStage::Estimate && !estimate.rate_per_kind {
                facts.push(format!("{} € je DP", format_decimal_de(estimate.rate, 2)));
            }
            if estimate.stage == CostStage::Calculation {
                facts.push(format!("Prüfsumme {}", result.check));
            }
            ui.label(
                RichText::new(facts.join(" · "))
                    .size(12.0)
                    .color(palette.text),
            );
        });
}

/// The KG 48x of DIN 276 (without 480) for the choice in the split.
fn group_codes() -> Vec<(&'static str, &'static str)> {
    DIN276_KG400
        .iter()
        .filter(|(code, _)| code.starts_with("48") && *code != "480")
        .copied()
        .collect()
}

/// Editable split: KG, label, percent, amount (if given), remove; true when changed.
fn share_editor(
    ui: &mut egui::Ui,
    palette: &Palette,
    buffers: &mut HashMap<egui::Id, String>,
    salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    shares: &mut Vec<CostShare>,
    amounts: Option<&[ShareLine]>,
) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let codes = group_codes();
    // KG list, percent, amount and the bin take their room; the label the rest.
    let label_width = (ui.available_width()
        - 80.0
        - 84.0
        - if amounts.is_some() { 110.0 } else { 0.0 }
        - 34.0
        - 60.0)
        .max(80.0);
    egui::Grid::new(("cost_share_grid", salt))
        .num_columns(5)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            dim(ui, palette, "KG");
            dim(ui, palette, "Kostengruppe");
            dim(ui, palette, "Anteil");
            dim(ui, palette, if amounts.is_some() { "Betrag" } else { "" });
            ui.label("");
            ui.end_row();
            for (index, share) in shares.iter_mut().enumerate() {
                let mut code = share.code.clone();
                egui::ComboBox::from_id_salt(("cost_share_code", salt, index))
                    .selected_text(code.as_str())
                    .width(58.0)
                    .show_ui(ui, |ui| {
                        for (known, label) in &codes {
                            ui.selectable_value(
                                &mut code,
                                known.to_string(),
                                format!("{known} {label}"),
                            );
                        }
                    });
                if code != share.code {
                    share.code = code;
                    changed = true;
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width, 18.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(label_width);
                        ui.add(
                            egui::Label::new(
                                RichText::new(cost_group_label(&share.code)).size(12.0),
                            )
                            .truncate(),
                        )
                        .on_hover_text(cost_group_label(&share.code));
                    },
                );
                ui.horizontal(|ui| {
                    let id = ui.scope_id().with(("cost_share_percent", salt, index));
                    changed |= decimal_field(
                        ui,
                        buffers,
                        id,
                        &mut share.percent,
                        NumberKind::CostShare,
                        52.0,
                    );
                    ui.label("%");
                });
                match amounts.and_then(|amounts| amounts.get(index)) {
                    Some(line) => right(ui, &line.amount.to_string()),
                    None => {
                        ui.label("");
                    }
                }
                if widgets::tool_button(
                    ui,
                    palette,
                    Icon::Trash,
                    false,
                    "Kostengruppe aus der Aufteilung nehmen",
                )
                .clicked()
                {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });
    if let Some(index) = remove {
        shares.remove(index);
        changed = true;
    }
    let free = codes
        .iter()
        .find(|(code, _)| !shares.iter().any(|share| share.code == *code));
    if let Some((code, _)) = free {
        if widgets::icon_label_button(ui, palette, Icon::Plus, "Kostengruppe")
            .on_hover_text(format!("KG {code} mit 0 % hinzufügen"))
            .clicked()
        {
            shares.push(CostShare::new(code, Decimal::ZERO));
            changed = true;
        }
    }
    let sum = proava_domain::cost_estimate::percent_sum(shares);
    if amounts.is_none() {
        let ok = sum == Decimal::ONE_HUNDRED;
        ui.label(
            RichText::new(format!(
                "Summe {}{}",
                percent(sum),
                if ok { "" } else { " – muss 100 % ergeben" }
            ))
            .size(12.0)
            .strong()
            .color(if ok { palette.text } else { palette.error }),
        );
    }
    changed
}

/// One line of fixed-width cells; the cell at `flexible` takes the rest.
/// Cells: (text, width, right aligned, bold, dim).
fn cells(
    ui: &mut egui::Ui,
    palette: &Palette,
    items: &[(&str, f32, bool, bool, bool)],
    flexible: usize,
) {
    let spacing = 8.0;
    let fixed: f32 = items
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != flexible)
        .map(|(_, item)| item.1)
        .sum();
    // Some air on the right: egui adds frame and indent margins we do not see here.
    let rest =
        (ui.available_width() - 24.0 - fixed - spacing * (items.len() as f32 - 1.0)).max(40.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        for (index, (text, width, right, strong, dim)) in items.iter().enumerate() {
            let width = if index == flexible { rest } else { *width };
            let layout = if *right {
                egui::Layout::right_to_left(egui::Align::Center)
            } else {
                egui::Layout::left_to_right(egui::Align::Center)
            };
            ui.allocate_ui_with_layout(egui::vec2(width, 18.0), layout, |ui| {
                ui.set_width(width);
                let mut text = RichText::new(*text).size(12.0).color(if *dim {
                    palette.text_dim
                } else {
                    palette.text
                });
                if *strong {
                    text = text.font(crate::fonts::bold(12.0));
                }
                ui.add(egui::Label::new(text).truncate());
            });
        }
    });
}

/// Split of a Bauteil that follows the Vorgabe (read only).
fn share_table(ui: &mut egui::Ui, palette: &Palette, lines: &[ShareLine]) {
    for line in lines {
        cells(
            ui,
            palette,
            &[
                (&line.code, 30.0, false, false, false),
                (&line.label, 0.0, false, false, false),
                (&percent(line.percent), 52.0, true, false, false),
                (&line.amount.to_string(), 104.0, true, false, false),
            ],
            1,
        );
    }
}

fn check_line(ui: &mut egui::Ui, palette: &Palette, percent_sum: Decimal, check: Money) {
    let ok = check.is_zero() && percent_sum == Decimal::ONE_HUNDRED;
    ui.label(
        RichText::new(format!(
            "Summe {} · Prüfsumme {}",
            percent(percent_sum),
            check
        ))
        .size(12.0)
        .strong()
        .color(if ok { palette.done } else { palette.error }),
    );
}

fn groups_table(ui: &mut egui::Ui, palette: &Palette, result: &CostResult) {
    cells(
        ui,
        palette,
        &[
            ("KG", 30.0, false, false, true),
            ("Kostengruppe", 0.0, false, false, true),
            ("Anteil", 52.0, true, false, true),
            ("Betrag", 104.0, true, false, true),
        ],
        1,
    );
    for group in &result.groups {
        cells(
            ui,
            palette,
            &[
                (&group.code, 30.0, false, false, false),
                (&group.label, 0.0, false, false, false),
                (&percent(group.percent), 52.0, true, false, false),
                (&group.amount.to_string(), 104.0, true, false, false),
            ],
            1,
        );
    }
    cells(
        ui,
        palette,
        &[
            ("", 30.0, false, true, false),
            ("Gesamt", 0.0, false, true, false),
            ("100 %", 52.0, true, true, false),
            (&result.total.to_string(), 104.0, true, true, false),
        ],
        1,
    );
    check_line(ui, palette, Decimal::ONE_HUNDRED, result.check);
}

fn control_table(ui: &mut egui::Ui, palette: &Palette, stage: CostStage, control: &CostControl) {
    let difference = |value: Money, base: Money| -> String {
        let delta = value - base;
        if delta.is_zero() {
            "–".to_string()
        } else if base.is_zero() {
            format!(
                "{}{}",
                if delta.cents() > 0 { "+" } else { "" },
                delta.plain()
            )
        } else {
            let percent = delta.to_decimal() * Decimal::ONE_HUNDRED / base.to_decimal();
            format!(
                "{}{} ({}{} %)",
                if delta.cents() > 0 { "+" } else { "" },
                delta.plain(),
                if percent > Decimal::ZERO { "+" } else { "" },
                format_decimal_de(percent, 1)
            )
        }
    };
    let award = control.has_award;
    let line = |ui: &mut egui::Ui, items: Vec<(String, bool)>, strong: bool, dim: bool| {
        let widths = [30.0, 92.0, 92.0, 0.0, 92.0];
        let refs: Vec<(&str, f32, bool, bool, bool)> = items
            .iter()
            .zip(widths)
            .map(|((text, right), width)| (text.as_str(), width, *right, strong, dim))
            .collect();
        cells(ui, palette, &refs, 3);
    };
    let mut head = vec![
        ("KG".to_string(), false),
        (
            match stage {
                CostStage::Estimate => "KS",
                CostStage::Calculation => "KB",
            }
            .to_string(),
            true,
        ),
        ("KV".to_string(), true),
        ("Abweichung".to_string(), true),
    ];
    if award {
        head.push(("Auftrag".to_string(), true));
    }
    line(ui, head, false, true);
    for entry in &control.lines {
        let mut items = vec![
            (
                if entry.code.is_empty() {
                    "–".to_string()
                } else {
                    entry.code.clone()
                },
                false,
            ),
            (entry.estimate.plain(), true),
            (entry.planner.plain(), true),
            (difference(entry.planner, entry.estimate), true),
        ];
        if award {
            items.push((entry.awarded.plain(), true));
        }
        line(ui, items, false, false);
    }
    let mut total = vec![
        ("".to_string(), false),
        (control.estimate.plain(), true),
        (control.planner.plain(), true),
        (difference(control.planner, control.estimate), true),
    ];
    if award {
        total.push((control.awarded.plain(), true));
    }
    line(ui, total, true, false);
}
