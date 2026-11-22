//! Kostenverfolgung nach DIN 276 (4.4–4.6): model, JSON for
//! `templates/cost_tracking.typ` and rendering. A4 quer: optional
//! Deckblatt, Kennzahlen, the chain of all stages per Kostengruppe (or per
//! Vergabeeinheit) with the deviations, a bar chart of the stages per KG,
//! Erläuterungen/Maßnahmen, Grundlagen and Annahmen.

use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::format::{breakable, money, number};
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const TEMPLATE: &str = include_str!("../templates/cost_tracking.typ");

/// One stage column (Budget, Kostenschätzung …).
#[derive(Debug, Clone, Default)]
pub struct PrintTrackColumn {
    pub label: String,
}

/// Marking of a cell, printed as a background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintTrackMark {
    #[default]
    None,
    /// More than the threshold above the previous stage.
    AbovePrevious,
    /// Above the budget.
    OverBudget,
}

/// One value of the table with its deviation.
#[derive(Debug, Clone, Default)]
pub struct PrintTrackCell {
    pub value: Option<Decimal>,
    pub deviation: Option<Decimal>,
    pub deviation_percent: Option<Decimal>,
    pub mark: PrintTrackMark,
}

/// Kind of a table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintTrackRowKind {
    Group,
    #[default]
    Leaf,
    Lv,
    NoLv,
    Without,
    Total,
}

#[derive(Debug, Clone, Default)]
pub struct PrintTrackRow {
    pub kind: PrintTrackRowKind,
    pub depth: u8,
    /// KG („421“); empty for LV rows and the total.
    pub code: String,
    pub label: String,
    /// One per column.
    pub cells: Vec<PrintTrackCell>,
}

/// A key figure above the table („Prognose“, „1.234,56 €“, „zum Budget …“).
#[derive(Debug, Clone, Default)]
pub struct PrintKeyFigure {
    pub label: String,
    pub value: String,
    pub note: String,
}

/// One Kostengruppe (or Vergabeeinheit) of the chart: an amount per stage.
#[derive(Debug, Clone, Default)]
pub struct PrintTrackChart {
    pub label: String,
    /// One per chart stage; None = no value.
    pub values: Vec<Option<Decimal>>,
    /// The Prognose is above the budget.
    pub over_budget: bool,
}

/// Erläuterung/Maßnahme of a Kostengruppe.
#[derive(Debug, Clone, Default)]
pub struct PrintTrackNote {
    pub code: String,
    pub label: String,
    pub text: String,
}

/// A position without Kostengruppe.
#[derive(Debug, Clone, Default)]
pub struct PrintTrackWithout {
    pub lv: String,
    pub oz: String,
    pub text: String,
    pub amount: Decimal,
}

/// What the printout shows besides the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintTrackOptions {
    pub cover: bool,
    pub chart: bool,
}

impl Default for PrintTrackOptions {
    fn default() -> Self {
        Self {
            cover: false,
            chart: true,
        }
    }
}

/// The Kostenverfolgung for print.
#[derive(Debug, Clone, Default)]
pub struct PrintCostTracking {
    pub header: PrintHeader,
    /// „Kostengruppe“ or „Vergabeeinheit / Kostengruppe“.
    pub first_column: String,
    pub columns: Vec<PrintTrackColumn>,
    /// Rows with the total last.
    pub rows: Vec<PrintTrackRow>,
    /// „netto“ or „brutto (USt je LV …)“.
    pub amounts: String,
    /// „Ebene 2 · nach Kostengruppen · Abweichung zur Kostenvorgabe“.
    pub view: String,
    /// The deviation under the values; empty = none.
    pub deviation: String,
    /// Label of the recorded state compared with.
    pub snapshot: Option<String>,
    pub figures: Vec<PrintKeyFigure>,
    /// Threshold of the marking „über der Vorstufe“ in percent.
    pub threshold: Decimal,
    pub chart_stages: Vec<String>,
    pub chart: Vec<PrintTrackChart>,
    /// „Kostengruppen der 2. Ebene“ / „Vergabeeinheiten“.
    pub chart_of: String,
    pub notes: Vec<PrintTrackNote>,
    /// Budget: Art, Stand, Grundlage.
    pub budget: Vec<String>,
    pub sources: Vec<String>,
    pub assumptions: Vec<String>,
    /// Brutto with several Umsatzsteuersätze: who carries which.
    pub vat: Vec<String>,
    pub warnings: Vec<String>,
    pub without_group: Vec<PrintTrackWithout>,
    pub options: PrintTrackOptions,
}

/// The PDF with its number of pages.
pub fn render_cost_tracking_pdf_counted(
    tracking: &PrintCostTracking,
) -> Result<(Vec<u8>, usize), PrintError> {
    let mut files = Files::default();
    let data = cost_tracking_json(tracking, &mut files);
    let world = crate::build_world(TEMPLATE, files, &data)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub fn render_cost_tracking_pdf(tracking: &PrintCostTracking) -> Result<Vec<u8>, PrintError> {
    render_cost_tracking_pdf_counted(tracking).map(|(bytes, _)| bytes)
}

/// „+1.235“: whole euros for the narrow deviation line under a value.
fn signed_whole(value: Decimal) -> String {
    let text = number(value, 0);
    if value > Decimal::ZERO {
        format!("+{text}")
    } else {
        text
    }
}

fn signed_percent(value: Decimal) -> String {
    let text = number(value, 1);
    if value > Decimal::ZERO {
        format!("+{text} %")
    } else {
        format!("{text} %")
    }
}

/// 0…1 as a number for the bars.
fn ratio(value: Decimal, max: Decimal) -> f64 {
    if max <= Decimal::ZERO || value <= Decimal::ZERO {
        return 0.0;
    }
    (value / max)
        .round_dp(4)
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

pub(crate) fn cost_tracking_json(tracking: &PrintCostTracking, files: &mut Files) -> Value {
    let rows: Vec<Value> = tracking
        .rows
        .iter()
        .map(|row| {
            let kind = match row.kind {
                PrintTrackRowKind::Group => "group",
                PrintTrackRowKind::Leaf => "leaf",
                PrintTrackRowKind::Lv => "lv",
                PrintTrackRowKind::NoLv => "nolv",
                PrintTrackRowKind::Without => "without",
                PrintTrackRowKind::Total => "total",
            };
            let title = if row.code.is_empty() {
                row.label.clone()
            } else {
                format!("{} {}", row.code, row.label)
            };
            json!({
                "kind": kind,
                "depth": row.depth,
                "title": breakable(&title),
                "cells": row.cells.iter().map(|cell| json!({
                    "v": cell.value.map(money),
                    "d": cell.deviation.map(|amount| {
                        match cell.deviation_percent {
                            Some(percent) => format!("{} / {}", signed_whole(amount), signed_percent(percent)),
                            None => signed_whole(amount),
                        }
                    }),
                    "m": match cell.mark {
                        PrintTrackMark::None => "",
                        PrintTrackMark::AbovePrevious => "warn",
                        PrintTrackMark::OverBudget => "high",
                    },
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let max = tracking
        .chart
        .iter()
        .flat_map(|item| item.values.iter().flatten())
        .copied()
        .max()
        .unwrap_or_default();
    let chart: Vec<Value> = tracking
        .chart
        .iter()
        .map(|item| {
            json!({
                "label": breakable(&item.label),
                "over": item.over_budget,
                "bars": item.values.iter().zip(&tracking.chart_stages).map(|(value, stage)| json!({
                    "stage": stage,
                    "amount": value.map(money),
                    "ratio": value.map(|value| ratio(value, max)).unwrap_or(0.0),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "header": view::header(&tracking.header, "Kostenverfolgung", files),
        "first_column": tracking.first_column,
        "columns": tracking.columns.iter().map(|column| column.label.clone()).collect::<Vec<_>>(),
        "rows": rows,
        "amounts": tracking.amounts,
        "view": tracking.view,
        "deviation": tracking.deviation,
        "snapshot": tracking.snapshot,
        "figures": tracking.figures.iter().map(|figure| json!({
            "label": figure.label,
            "value": figure.value,
            "note": figure.note,
        })).collect::<Vec<_>>(),
        "threshold": number(tracking.threshold, 1).trim_end_matches(",0").to_string(),
        "chart_stages": tracking.chart_stages,
        "chart": chart,
        "chart_of": tracking.chart_of,
        "notes": tracking.notes.iter().map(|note| json!({
            "code": note.code,
            "label": breakable(&note.label),
            "text": note.text,
        })).collect::<Vec<_>>(),
        "budget": tracking.budget,
        "sources": tracking.sources,
        "assumptions": tracking.assumptions,
        "vat": tracking.vat,
        "warnings": tracking.warnings,
        "without": tracking.without_group.iter().map(|entry| json!({
            "lv": breakable(&entry.lv),
            "oz": entry.oz,
            "text": breakable(&entry.text),
            "amount": money(entry.amount),
        })).collect::<Vec<_>>(),
        "cover": tracking.options.cover,
        "chart_on": tracking.options.chart && !tracking.chart.is_empty(),
        "cur": tracking.header.currency_sign(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookmarks_without_change, outline_of};
    use crate::{PrintLook, PrintStyle};

    fn d(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn cell(value: &str, deviation: Option<(&str, &str)>, mark: PrintTrackMark) -> PrintTrackCell {
        PrintTrackCell {
            value: Some(d(value)),
            deviation: deviation.map(|(amount, _)| d(amount)),
            deviation_percent: deviation.map(|(_, percent)| d(percent)),
            mark,
        }
    }

    pub(crate) fn sample() -> PrintCostTracking {
        let columns = [
            "Kostenvorgabe",
            "Kostenberechnung",
            "Angebot",
            "Auftrag",
            "Prognose",
        ];
        let row =
            |kind, depth, code: &str, label: &str, values: [&str; 5], over: bool| PrintTrackRow {
                kind,
                depth,
                code: code.into(),
                label: label.into(),
                cells: values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let mark = if over && index == 4 {
                            PrintTrackMark::OverBudget
                        } else if index == 3 {
                            PrintTrackMark::AbovePrevious
                        } else {
                            PrintTrackMark::None
                        };
                        cell(value, (index > 0).then_some(("1200.5", "4.2")), mark)
                    })
                    .collect(),
            };
        PrintCostTracking {
            header: PrintHeader {
                project_name: "Rathaus 3".into(),
                project_title: "Neubau Rathaus Musterstadt".into(),
                date: "18.11.2026".into(),
                ..PrintHeader::default()
            },
            first_column: "Kostengruppe".into(),
            columns: columns
                .iter()
                .map(|label| PrintTrackColumn {
                    label: (*label).into(),
                })
                .collect(),
            rows: vec![
                row(
                    PrintTrackRowKind::Group,
                    0,
                    "400",
                    "Bauwerk – Technische Anlagen",
                    ["300000", "290000", "295000", "296000", "310000"],
                    true,
                ),
                row(
                    PrintTrackRowKind::Leaf,
                    1,
                    "420",
                    "Wärmeversorgungsanlagen",
                    ["100000", "95000", "99000", "99000", "99000"],
                    false,
                ),
                row(
                    PrintTrackRowKind::Leaf,
                    1,
                    "480",
                    "Gebäude- und Anlagenautomation",
                    ["200000", "195000", "196000", "197000", "211000"],
                    true,
                ),
                row(
                    PrintTrackRowKind::Total,
                    0,
                    "",
                    "Gesamt",
                    ["300000", "290000", "295000", "296000", "310000"],
                    true,
                ),
            ],
            amounts: "netto".into(),
            view: "Ebene 2 · nach Kostengruppen".into(),
            deviation: "zur Kostenvorgabe".into(),
            snapshot: None,
            figures: vec![
                PrintKeyFigure {
                    label: "Kostenvorgabe".into(),
                    value: "300.000,00 €".into(),
                    note: "Obergrenze, Stand 01.06.2026".into(),
                },
                PrintKeyFigure {
                    label: "Prognose".into(),
                    value: "310.000,00 €".into(),
                    note: "zur Kostenvorgabe +10.000,00 € / +3,3 %".into(),
                },
            ],
            threshold: d("10"),
            chart_stages: columns.iter().map(|label| (*label).to_string()).collect(),
            chart: vec![
                PrintTrackChart {
                    label: "420 Wärmeversorgungsanlagen".into(),
                    values: vec![
                        Some(d("100000")),
                        Some(d("95000")),
                        Some(d("99000")),
                        None,
                        Some(d("99000")),
                    ],
                    over_budget: false,
                },
                PrintTrackChart {
                    label: "480 Gebäude- und Anlagenautomation".into(),
                    values: vec![
                        Some(d("200000")),
                        Some(d("195000")),
                        Some(d("196000")),
                        Some(d("197000")),
                        Some(d("211000")),
                    ],
                    over_budget: true,
                },
            ],
            chart_of: "Kostengruppe".into(),
            notes: vec![PrintTrackNote {
                code: "480".into(),
                label: "Gebäude- und Anlagenautomation".into(),
                text: "Nachtrag Raumbediengeräte auf Wunsch des Bauherrn.".into(),
            }],
            budget: vec!["Obergrenze, Stand 01.06.2026, Kostenrahmen des Bauherrn".into()],
            sources: vec!["Kostenberechnung: Kostenberechnung LPH 3 (nach Datenpunkten)".into()],
            assumptions: vec!["LV 2 Gebäudeautomation: Auftrag mit Nachträgen".into()],
            vat: vec![
                "LV 1 Heizung: 7 %".into(),
                "LV 2 Gebäudeautomation: 19 %".into(),
            ],
            warnings: Vec::new(),
            without_group: vec![PrintTrackWithout {
                lv: "LV 1 Heizung".into(),
                oz: "01.0010".into(),
                text: "Druckprobe".into(),
                amount: d("900"),
            }],
            options: PrintTrackOptions::default(),
        }
    }

    #[test]
    fn the_printout_has_its_sections_as_bookmarks_in_both_styles() {
        for (cover, style) in [(false, PrintStyle::Classic), (true, PrintStyle::ProAva)] {
            let mut model = sample();
            model.options.cover = cover;
            model.header.look = PrintLook {
                style,
                ..PrintLook::default()
            };
            let outline = bookmarks_without_change(
                || {
                    let mut files = Files::default();
                    let data = cost_tracking_json(&model, &mut files);
                    crate::build_world(TEMPLATE, files, &data).unwrap()
                },
                "Kostenverfolgung",
            );
            let mut expected = vec![
                "1 Kostenverfolgung je Kostengruppe",
                "1 Diagramm der Stufen je Kostengruppe",
                "1 Erläuterungen und Maßnahmen",
                "1 Grundlagen und Annahmen",
                "1 Positionen ohne Kostengruppe",
            ];
            if cover {
                expected.insert(0, "1 Deckblatt");
            }
            let entries: Vec<String> = outline
                .iter()
                .map(|(level, text)| format!("{level} {text}"))
                .collect();
            assert_eq!(entries, expected);
        }
    }

    /// Druckoption Gitter: the tables of the Kostenverfolgung get every
    /// column line only when it is switched on.
    #[test]
    fn the_tables_are_fully_ruled_only_with_the_grid() {
        for grid in [true, false] {
            let mut model = sample();
            model.header.look.table_grid = grid;
            let mut files = Files::default();
            let data = cost_tracking_json(&model, &mut files);
            let document = crate::build_world(TEMPLATE, files, &data)
                .unwrap()
                .layout()
                .unwrap();
            let gridded = crate::tests::ruled_lines(&document)
                .iter()
                .filter(|line| line.vertical() && (line.width - 0.4).abs() < 0.05)
                .count();
            // One line per column boundary through each table.
            assert_eq!(gridded > 10, grid, "{gridded} senkrechte Gitterlinien");
        }
    }

    #[test]
    fn pages_grow_with_cover_and_chart_and_the_texts_are_printed() {
        let mut model = sample();
        model.options.chart = false;
        let (pdf, plain) = render_cost_tracking_pdf_counted(&model).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        model.options.chart = true;
        let (_, with_chart) = render_cost_tracking_pdf_counted(&model).unwrap();
        model.options.cover = true;
        let (pdf, with_cover) = render_cost_tracking_pdf_counted(&model).unwrap();
        // The chart starts a page of its own (the notes may follow on it).
        assert!(plain >= 1 && with_chart >= plain, "{plain} → {with_chart}");
        assert_eq!(with_cover, with_chart + 1, "Deckblatt");
        assert!(outline_of(&pdf).iter().any(|(_, text)| text == "Deckblatt"));
        // Compared with a recorded state: the change in the second line of
        // every stage, a stage gone since then as „–“ with its change.
        let mut compared = sample();
        compared.snapshot = Some("01.10.2026 – Kostenberechnung abgestimmt".into());
        compared.deviation = "zum Stand vom 01.10.2026".into();
        compared.rows[1].cells[3] = PrintTrackCell {
            value: None,
            deviation: Some(d("-99000")),
            deviation_percent: Some(d("-100")),
            mark: PrintTrackMark::None,
        };
        let (_, pages) = render_cost_tracking_pdf_counted(&compared).unwrap();
        assert_eq!(pages, with_chart);
    }
}
