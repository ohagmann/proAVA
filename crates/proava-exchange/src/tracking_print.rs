//! Kostenverfolgung → print model: the table as the tab shows it (Ansicht,
//! Ebene, netto/brutto, Abweichung, Vergleich), key figures, the chart of the
//! stages per KG (or LV), Erläuterungen, Grundlagen and Annahmen.

use proava_domain::{
    code_level, cost_tracking, group_label, Mark, Money, Project, TrackDeviation, TrackOptions,
    TrackReport, TrackRow, TrackRowKind, TrackStage, TrackView,
};
use proava_print as print;

use crate::to_print::header;

fn decimal(value: Money) -> proava_domain::Decimal {
    value.to_decimal()
}

fn row_kind(kind: TrackRowKind) -> print::PrintTrackRowKind {
    match kind {
        TrackRowKind::Group => print::PrintTrackRowKind::Group,
        TrackRowKind::Leaf => print::PrintTrackRowKind::Leaf,
        TrackRowKind::Lv => print::PrintTrackRowKind::Lv,
        TrackRowKind::NoLv => print::PrintTrackRowKind::NoLv,
        TrackRowKind::Without => print::PrintTrackRowKind::Without,
        TrackRowKind::Total => print::PrintTrackRowKind::Total,
    }
}

fn print_row(report: &TrackReport, row: &TrackRow, stages: &[TrackStage]) -> print::PrintTrackRow {
    let deviation = report.options.deviation;
    print::PrintTrackRow {
        kind: row_kind(row.kind),
        depth: row.depth,
        code: if row.kind == TrackRowKind::Lv {
            String::new()
        } else {
            row.code.clone().unwrap_or_default()
        },
        label: if row.kind == TrackRowKind::Lv {
            row.title()
        } else {
            row.label.clone()
        },
        cells: stages
            .iter()
            .map(|stage| {
                let value = row.value(*stage);
                // A stage gone since the recorded state keeps its change.
                let shown = deviation
                    .of(row, *stage)
                    .filter(|_| value.is_some() || deviation == TrackDeviation::Snapshot);
                print::PrintTrackCell {
                    value: value.map(decimal),
                    deviation: shown.map(|deviation| decimal(deviation.amount)),
                    deviation_percent: shown.and_then(|deviation| deviation.percent),
                    mark: match value.map(|_| report.mark(row, *stage)) {
                        Some(Mark::OverBudget) => print::PrintTrackMark::OverBudget,
                        Some(Mark::AbovePrevious) => print::PrintTrackMark::AbovePrevious,
                        _ => print::PrintTrackMark::None,
                    },
                }
            })
            .collect(),
    }
}

/// „+1.234,56 € / +3,3 %“
fn deviation_text(deviation: &proava_domain::Deviation) -> String {
    let sign = if deviation.amount.cents() > 0 {
        "+"
    } else {
        ""
    };
    match deviation.percent {
        Some(percent) => format!(
            "{sign}{} / {sign}{} %",
            deviation.amount,
            proava_domain::format_decimal_de(percent, 1)
        ),
        None => format!("{sign}{}", deviation.amount),
    }
}

fn figures(report: &TrackReport) -> Vec<print::PrintKeyFigure> {
    let total = &report.total;
    let mut figures = Vec::new();
    let mut figure = |label: &str, value: Money, note: String| {
        figures.push(print::PrintKeyFigure {
            label: label.to_string(),
            value: value.to_string(),
            note,
        })
    };
    if let Some(budget) = total.value(TrackStage::Budget) {
        let mut note = report.budget_kind.label().to_string();
        if !report.budget_date.is_empty() {
            note.push_str(&format!(", Stand {}", report.budget_date));
        }
        figure("Kostenvorgabe", budget, note);
    }
    if let Some(forecast) = total.value(TrackStage::Forecast) {
        let note = total
            .to_budget(TrackStage::Forecast)
            // Amount and percent each stay on one line in the narrow box.
            .map(|deviation| {
                let text = deviation_text(&deviation)
                    .replace(" €", "\u{a0}€")
                    .replace(" %", "\u{a0}%");
                format!("zur Kostenvorgabe {text}")
            })
            .unwrap_or_default();
        figure("Prognose", forecast, note);
    }
    if let Some(contract) = total.value(TrackStage::Contract) {
        figure(
            "Auftrag mit Nachträgen",
            contract,
            format!(
                "{} von {} LVs beauftragt",
                report.contract_lvs,
                report.lvs.len()
            ),
        );
    }
    if let Some(billed) = total.value(TrackStage::Billed) {
        let released = if report.options.gross {
            report.released_gross
        } else {
            report.released_net
        };
        figure(
            "Abrechnung (geprüft)",
            billed,
            format!("Freigaben {released}"),
        );
    }
    if let Some(final_cost) = total.value(TrackStage::Final) {
        figure("Kostenfeststellung", final_cost, report.final_text());
    }
    figures
}

/// Rows of the chart: the Kostengruppen of the 2. Ebene (or the 1. when only
/// that is shown), in the view by LV the Vergabeeinheiten.
fn chart_rows(report: &TrackReport) -> Vec<&TrackRow> {
    match report.options.view {
        TrackView::ByLv => report
            .rows
            .iter()
            .filter(|row| matches!(row.kind, TrackRowKind::Lv | TrackRowKind::NoLv))
            .collect(),
        TrackView::ByGroup => {
            let level = report.options.level.min(2);
            report
                .rows
                .iter()
                .filter(|row| matches!(row.kind, TrackRowKind::Group | TrackRowKind::Leaf))
                .filter(|row| {
                    row.code
                        .as_deref()
                        .is_some_and(|code| code_level(code) == level)
                })
                // A KG not divided further at level 3 appears once.
                .fold(Vec::new(), |mut rows: Vec<&TrackRow>, row| {
                    if !rows.iter().any(|other| other.code == row.code) {
                        rows.push(row);
                    }
                    rows
                })
        }
    }
}

/// The Kostenverfolgung of `project` as `options` show it, for print.
pub fn cost_tracking_to_print(
    project: &Project,
    options: &TrackOptions,
    date: &str,
    print_options: print::PrintTrackOptions,
) -> print::PrintCostTracking {
    let report = cost_tracking(project, options);
    let tracking = project.cost_tracking();
    let stages = report.shown_stages();
    let mut head = header(project, date);
    head.document_kind = Some("Kostenverfolgung".to_string());
    head.lv_name = String::new();
    let by_lv = report.options.view == TrackView::ByLv;
    let mut view = format!(
        "Ebene {} · {}",
        report.options.level,
        if by_lv {
            "nach Vergabeeinheiten"
        } else {
            "nach Kostengruppen"
        }
    );
    if let Some(snapshot) = &report.snapshot {
        view.push_str(&format!(" · Vergleich mit Stand {snapshot}"));
    }
    let mut rows: Vec<print::PrintTrackRow> = report
        .rows
        .iter()
        .map(|row| print_row(&report, row, &stages))
        .collect();
    rows.push(print_row(&report, &report.total, &stages));
    let chart = chart_rows(&report)
        .into_iter()
        .map(|row| print::PrintTrackChart {
            label: row.title(),
            values: stages
                .iter()
                .map(|stage| row.value(*stage).map(decimal))
                .collect(),
            over_budget: report.mark(row, TrackStage::Forecast) == Mark::OverBudget,
        })
        .collect();
    let mut notes: Vec<(&String, &String)> = tracking
        .notes
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .collect();
    notes.sort_by_key(|(code, _)| (*code).clone());
    let mut budget = Vec::new();
    if let Some(total) = tracking.budget_total() {
        let mut line = format!("{} {}", report.budget_kind.label(), total);
        if !tracking.budget_date.is_empty() {
            line.push_str(&format!(", Stand {}", tracking.budget_date));
        }
        if !tracking.budget_note.is_empty() {
            line.push_str(&format!(", {}", tracking.budget_note));
        }
        budget.push(line);
        let mut codes: Vec<String> = tracking
            .budget
            .iter()
            .map(|(code, amount)| format!("KG {code} {amount}"))
            .collect();
        codes.sort();
        budget.push(format!("je Kostengruppe: {}", codes.join(", ")));
        budget.extend(tracking.budget_conflicts());
    } else {
        budget.push("Keine Kostenvorgabe eingetragen.".to_string());
    }
    print::PrintCostTracking {
        header: head,
        first_column: if by_lv {
            "Vergabeeinheit / Kostengruppe".to_string()
        } else {
            "Kostengruppe".to_string()
        },
        columns: stages
            .iter()
            .map(|stage| print::PrintTrackColumn {
                label: stage.label().to_string(),
            })
            .collect(),
        rows,
        amounts: report.amounts_text(),
        view,
        deviation: match (report.options.deviation, &report.snapshot) {
            (TrackDeviation::Off, _) => String::new(),
            (TrackDeviation::Snapshot, Some(snapshot)) => format!(
                "zum Stand vom {}",
                snapshot.split(" – ").next().unwrap_or(snapshot)
            ),
            (mode, _) => mode.label().to_string(),
        },
        snapshot: report.snapshot.clone(),
        figures: figures(&report),
        threshold: report.threshold,
        chart_stages: stages
            .iter()
            .map(|stage| stage.label().to_string())
            .collect(),
        chart,
        chart_of: if by_lv {
            "Vergabeeinheit".to_string()
        } else {
            "Kostengruppe".to_string()
        },
        notes: notes
            .into_iter()
            .map(|(code, text)| print::PrintTrackNote {
                code: code.clone(),
                label: group_label(code).to_string(),
                text: text.trim().to_string(),
            })
            .collect(),
        budget,
        sources: report.sources.clone(),
        assumptions: report.assumptions.clone(),
        vat: if report.options.gross {
            report.vat_details()
        } else {
            Vec::new()
        },
        warnings: report.warnings.clone(),
        without_group: report
            .without_group
            .iter()
            .map(|entry| print::PrintTrackWithout {
                lv: entry.lv_label.clone(),
                oz: entry.ordinal.clone(),
                text: entry.short_text.clone(),
                amount: decimal(entry.amount),
            })
            .collect(),
        options: print_options,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_application::{PrintJob, PrintKind};
    use proava_domain::{
        CostStage, Decimal, Lv, LvGroup, LvPurpose, OzMask, Position, ProjectName,
    };

    fn project() -> Project {
        let mut lv = Lv::new("Kostenberechnung", OzMask::parse("11.22.PPPP").unwrap());
        for (code, title, text, euro) in [
            ("421", "Wärme", "Wärmepumpe", 50_000),
            ("431", "Lüftung", "RLT-Gerät", 70_000),
        ] {
            let mut group = LvGroup::new(0, "", title);
            group.set_cost_group(Some(code.into()));
            lv.push_group(group);
            lv.push_position(Position::new(
                "",
                text,
                Decimal::ONE,
                "St",
                Some(Decimal::from(euro)),
            ));
        }
        lv.renumber();
        lv.set_purpose(LvPurpose::Cost(CostStage::Calculation));
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18T12:00:00",
            lv,
        );
        let tracking = project.cost_tracking_mut();
        tracking.set_budget("420", Money::from_cents(4_500_000));
        tracking.set_budget("430", Money::from_cents(8_000_000));
        tracking.budget_date = "01.06.2026".into();
        tracking.set_note("420", "Variante mit Förderung prüfen.");
        project
    }

    #[test]
    fn the_model_follows_the_view_of_the_tab() {
        let project = project();
        let options = TrackOptions {
            level: 3,
            deviation: TrackDeviation::Budget,
            ..TrackOptions::default()
        };
        let model = cost_tracking_to_print(
            &project,
            &options,
            "18.11.2026",
            print::PrintTrackOptions::default(),
        );
        let columns: Vec<&str> = model.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(columns, ["Kostenvorgabe", "Kostenberechnung", "Prognose"]);
        assert_eq!(
            model.rows.last().unwrap().kind,
            print::PrintTrackRowKind::Total
        );
        let heat = model
            .rows
            .iter()
            .find(|row| row.code == "420")
            .expect("Gruppe 420");
        // Prognose 50.000 over the budget 45.000: marked, deviation +5.000 / +11,1 %.
        let forecast = &heat.cells[2];
        assert_eq!(forecast.mark, print::PrintTrackMark::OverBudget);
        assert_eq!(forecast.deviation, Some(Decimal::from(5_000)));
        assert_eq!(forecast.deviation_percent, Some("11.1".parse().unwrap()));
        let figures: Vec<&str> = model.figures.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(figures, ["Kostenvorgabe", "Prognose"]);
        assert_eq!(model.figures[0].note, "Obergrenze, Stand 01.06.2026");
        assert_eq!(model.notes.len(), 1);
        assert_eq!(model.notes[0].label, "Wärmeversorgungsanlagen");
        assert!(model.budget[0].starts_with("Obergrenze 125.000,00 €, Stand 01.06.2026"));
        // Chart: the KG of the 2. Ebene.
        let chart: Vec<&str> = model.chart.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            chart,
            [
                "420 Wärmeversorgungsanlagen",
                "430 Raumlufttechnische Anlagen"
            ]
        );
        assert!(model.chart[0].over_budget);
        assert_eq!(model.deviation, "zur Kostenvorgabe");
        assert_eq!(model.view, "Ebene 3 · nach Kostengruppen");
    }

    #[test]
    fn the_print_job_writes_a_pdf_with_and_without_cover() {
        let project = project();
        let mut job = PrintJob::new(PrintKind::CostTracking);
        assert!(job.settings.landscape);
        job.settings.cover = true;
        let (pdf, outcome) = crate::render_print(&project, &job, "18.11.2026").unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        assert!(
            outcome.pages >= 2,
            "Deckblatt und Tabelle: {}",
            outcome.pages
        );
        job.settings.cover = false;
        let (_, without) = crate::render_print(&project, &job, "18.11.2026").unwrap();
        assert_eq!(without.pages + 1, outcome.pages);
        assert_eq!(
            PrintKind::from_key("kostenverfolgung"),
            Some(PrintKind::CostTracking)
        );
    }
}
