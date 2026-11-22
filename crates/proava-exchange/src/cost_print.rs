//! Kostenschätzung / Kostenberechnung → print model.

use proava_domain::{cost_control, CostEstimate, CostEstimateId, CostResult, CostStage, Project};
use proava_print as print;

use crate::to_print::header;

/// The estimate `id` (or the newest) for print; `control` adds the
/// Kostenkontrolle when the project's LVs carry prices.
pub fn cost_estimate_to_print(
    project: &Project,
    id: Option<CostEstimateId>,
    date: &str,
    options: print::PrintCostOptions,
    control: bool,
) -> Result<print::PrintCostEstimate, String> {
    let estimate = match id {
        Some(id) => project.cost_estimate(id),
        None => project.cost_estimates().last(),
    }
    .ok_or("Im Projekt gibt es keine Kostenermittlung – erst unter Kosten eine anlegen.")?;
    Ok(estimate_to_print(project, estimate, date, options, control))
}

pub(crate) fn estimate_to_print(
    project: &Project,
    estimate: &CostEstimate,
    date: &str,
    options: print::PrintCostOptions,
    with_control: bool,
) -> print::PrintCostEstimate {
    let result: CostResult = estimate.result();
    let mut head = header(project, date);
    head.document_kind = Some(estimate.stage.label().to_string());
    // Stand and Bearbeiter of the estimate win over the print date and the planner.
    if !estimate.date.trim().is_empty() {
        head.date = estimate.date.trim().to_string();
    }
    if !estimate.author.trim().is_empty() {
        head.author = estimate.author.trim().to_string();
    }
    head.lv_name = String::new();
    let kind_sum = |sum: &proava_domain::KindSum| print::PrintKindSum {
        count: sum.count,
        unit_price: sum.unit_price,
        amount: sum.amount.to_decimal(),
    };
    let share = |line: &proava_domain::ShareLine| print::PrintShare {
        code: line.code.clone(),
        label: line.label.clone(),
        percent: line.percent,
        amount: line.amount.to_decimal(),
    };
    let control = with_control
        .then(|| cost_control(project, estimate))
        .filter(|control| control.has_planner || control.has_award)
        .map(|control| print::PrintCostControl {
            lines: control
                .lines
                .iter()
                .map(|line| print::PrintControlLine {
                    code: line.code.clone(),
                    label: line.label.clone(),
                    estimate: line.estimate.to_decimal(),
                    planner: line.planner.to_decimal(),
                    awarded: line.awarded.to_decimal(),
                })
                .collect(),
            estimate: control.estimate.to_decimal(),
            planner: control.planner.to_decimal(),
            awarded: control.awarded.to_decimal(),
            has_award: control.has_award,
        });
    let single_rate = estimate.stage == CostStage::Estimate && !estimate.rate_per_kind;
    print::PrintCostEstimate {
        header: head,
        calculation: estimate.stage == CostStage::Calculation,
        stage: estimate.stage.label().to_string(),
        level: estimate.stage.level_label(),
        title: estimate.title.trim().to_string(),
        remark: estimate.remark.trim().to_string(),
        kinds: estimate
            .kinds
            .iter()
            .map(|kind| print::PrintPointKind {
                key: kind.key.clone(),
                label: kind.label.clone(),
                virtual_point: kind.is_virtual(),
            })
            .collect(),
        sections: estimate
            .sections
            .iter()
            .zip(&result.sections)
            .map(|(section, sums)| print::PrintCostSection {
                name: section.name.clone(),
                rows: section
                    .rows
                    .iter()
                    .map(|row| print::PrintPointRow {
                        location: row.location.clone(),
                        trade: row.trade.clone(),
                        plant: row.plant.clone(),
                        remark: row.remark.clone(),
                        counts: row.counts.clone(),
                    })
                    .collect(),
                kinds: sums.kinds.iter().map(kind_sum).collect(),
                points: sums.points,
                total: sums.total.to_decimal(),
                low: sums.low.map(|value| value.to_decimal()),
                high: sums.high.map(|value| value.to_decimal()),
                shares: sums.shares.iter().map(share).collect(),
                percent_sum: sums.percent_sum,
                check: sums.check.to_decimal(),
                own_shares: sums.own_shares,
            })
            .collect(),
        kind_sums: result.kinds.iter().map(kind_sum).collect(),
        points: result.points,
        total: result.total.to_decimal(),
        low: result.low.map(|value| value.to_decimal()),
        high: result.high.map(|value| value.to_decimal()),
        rate: single_rate.then_some(estimate.rate),
        rate_low: estimate.rate_low.filter(|_| estimate.has_span()),
        rate_high: estimate.rate_high.filter(|_| estimate.has_span()),
        groups: result.groups.iter().map(share).collect(),
        check: result.check.to_decimal(),
        shares: estimate
            .shares
            .iter()
            .map(|share| print::PrintShare {
                code: share.code.clone(),
                label: proava_domain::cost_group_label(&share.code).to_string(),
                percent: share.percent,
                amount: Default::default(),
            })
            .collect(),
        control,
        warnings: result.warnings,
        options,
    }
}
