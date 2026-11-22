//! Datenpunkt-Abschnitte im Kosten-LV (GU-Fall, Anwender 19.11.: „GU-Fall ja – in
//! einer Kostenermittlung“): a Kostenschätzung or Kostenberechnung as Kosten-LV
//! holds Kurztext positions for KG 410–470 and, under a title „berechnet über
//! Datenpunkte“, the GA-Datenpunktrechnung for KG 480.
//!
//! The section is the same building block as a standalone Datenpunktrechnung
//! ([`CostEstimate`]): DP-Arten with prices, Bauteile with their plant list,
//! Kennwert (Kostenschätzung) or EP je DP-Art and the split on 481–489
//! (Kostenberechnung). It hangs on the title ([`LvGroup::points`]); its total is
//! the title's own amount in every planner's sum of the LV, split per
//! Kostengruppe in the evaluation by KG ([`crate::cost_group_report`]) – so
//! [`Project::cost_sums`] and the Kostenverfolgung count it with the Kosten-LV,
//! once.
//!
//! Its stage follows the LV: [`Lv::set_purpose`] and every insertion of rows
//! call [`Lv::fit_point_sections`]; a tender LV holds no section.

use crate::cost_estimate::{CostEstimate, CostEstimateId, CostStage};
use crate::cost_lv::{cost_lv_note, cost_lv_template, din_level};
use crate::cost_sums::cost_group_at_level;
use crate::lv::{Lv, LvGroup, LvRow, RowId};
use crate::lv_purpose::LvPurpose;
use crate::project::Project;

/// Kostengruppe of a Datenpunkt-Abschnitt (Gebäude- und Anlagenautomation).
pub const POINTS_GROUP: &str = "480";

/// Name of the title a new section gets.
pub const POINTS_TITLE: &str = "Gebäude- und Anlagenautomation";

/// A new, empty section of this stage: the template of the Datenpunktrechnung
/// (DP-Arten, one Bauteil, Kennwert 450 € je DP or EP je DP-Art and the split
/// 40/12/10/30/5/3 %) without title and Stand of its own – those are the LV's.
pub fn new_point_section(stage: CostStage) -> CostEstimate {
    let mut points = CostEstimate::template(stage, "", "");
    points.title = String::new();
    points
}

/// The section converted to `stage`: a Kostenberechnung takes the EP of the
/// kinds and the template split ([`CostEstimate::to_calculation`]); a
/// Kostenschätzung keeps the prices as Kennwert je DP-Art.
pub fn point_section_as(points: &CostEstimate, stage: CostStage) -> CostEstimate {
    if points.stage == stage {
        return points.clone();
    }
    match stage {
        CostStage::Calculation => {
            let mut converted = points.to_calculation(&points.date, &points.author);
            converted.title = points.title.clone();
            converted.remark = points.remark.clone();
            converted
        }
        CostStage::Estimate => {
            let mut converted = points.clone();
            converted.stage = CostStage::Estimate;
            converted.rate_per_kind = true;
            converted
        }
    }
}

/// What a section covers: KS the KG 480 as a whole, KB the KG of its split.
fn covered_codes(points: &CostEstimate) -> Vec<String> {
    match points.stage {
        CostStage::Estimate => vec![POINTS_GROUP.to_string()],
        CostStage::Calculation => {
            let mut codes: Vec<String> = points
                .result()
                .groups
                .into_iter()
                .map(|line| line.code)
                .collect();
            codes.push(POINTS_GROUP.to_string());
            codes
        }
    }
}

/// Whether a position with Kostengruppe `code` is counted twice next to a
/// section that covers `covered`.
fn overlaps(code: &str, points: &CostEstimate, covered: &[String]) -> bool {
    match points.stage {
        // KS: the whole KG 480 (2nd level) is the section's.
        CostStage::Estimate => cost_group_at_level(code, 2) == POINTS_GROUP,
        CostStage::Calculation => covered.iter().any(|covered| covered == code.trim()),
    }
}

impl Lv {
    /// „Datenpunkt-Abschnitt einfügen“ (only in a Kosten-LV): an empty title of
    /// KG 480 – as the template lays it out, with empty subtitles at most –
    /// becomes the section; otherwise a new title „480 Gebäude- und
    /// Anlagenautomation“ comes among the titles of KG (OZ 480 when the mask
    /// numbers the titles with the KG and it is free). Returns its index.
    pub fn insert_point_section(&mut self) -> Result<usize, String> {
        let LvPurpose::Cost(stage) = self.purpose() else {
            return Err(
                "Datenpunkt-Abschnitte gibt es nur in einer Kostenermittlung (Kosten-LV)."
                    .to_string(),
            );
        };
        let empty_480 = (0..self.rows().len()).find(|&index| {
            let LvRow::Group(group) = &self.rows()[index] else {
                return false;
            };
            let is_480 =
                group.cost_group() == Some(POINTS_GROUP) || group.ordinal().trim() == POINTS_GROUP;
            group.level() == 0
                && is_480
                && !group.is_point_section()
                && self.rows()[self.block_range(index)]
                    .iter()
                    .skip(1)
                    .all(LvRow::is_group)
        });
        if let Some(index) = empty_480 {
            let end = self.block_range(index).end;
            for row in (index + 1..end).rev() {
                self.remove_row(row);
            }
            if let Some(LvRow::Group(group)) = self.row_mut(index) {
                if group.cost_group().is_none() {
                    group.set_cost_group(Some(POINTS_GROUP.to_string()));
                }
                group.set_points(Some(new_point_section(stage)));
            }
            return Ok(index);
        }
        let mut group = LvGroup::new(0, "", POINTS_TITLE);
        group.set_cost_group(Some(POINTS_GROUP.to_string()));
        group.set_points(Some(new_point_section(stage)));
        // Among the titles of KG: before the first one above 480.
        let index = (0..self.rows().len())
            .find(|&index| {
                matches!(&self.rows()[index], LvRow::Group(other)
                    if other.level() == 0
                        && other.ordinal().trim().parse::<u32>().is_ok_and(|number| number > 480))
            })
            .unwrap_or(self.rows().len());
        let mask = self.oz_mask();
        let numbered_by_kg =
            mask.level_count() > 0 && mask.level_width(0) >= 3 && !mask.level_is_letters(0);
        let ordinal = mask.format_group(&[480]);
        let taken = self
            .rows()
            .iter()
            .any(|row| row.is_group() && row.ordinal().trim() == ordinal);
        if numbered_by_kg && !taken {
            group.set_ordinal(ordinal);
            let id = self.insert_row(index, LvRow::Group(group));
            return self.index_of(id).ok_or_else(|| "Titel fehlt.".to_string());
        }
        let id = self.insert_numbered(index, LvRow::Group(group))?;
        self.index_of(id).ok_or_else(|| "Titel fehlt.".to_string())
    }

    /// After rows came in (paste, take-over) or the LV changed its purpose or
    /// stage: a Datenpunkt-Abschnitt follows the stage of the Kosten-LV; a
    /// tender LV holds none (the title stays, without data points). Returns how
    /// many sections changed.
    pub fn fit_point_sections(&mut self) -> usize {
        let stage = self.purpose().cost_stage();
        let mut changed = 0;
        for index in self.point_sections() {
            let Some(LvRow::Group(group)) = self.row_mut(index) else {
                continue;
            };
            let Some(points) = group.points() else {
                continue;
            };
            match stage {
                None => group.set_points(None),
                Some(stage) if points.stage != stage => {
                    let converted = point_section_as(points, stage);
                    group.set_points(Some(converted));
                }
                Some(_) => continue,
            }
            changed += 1;
        }
        changed
    }

    /// The section of the title with this id.
    pub fn point_section(&self, id: RowId) -> Option<&CostEstimate> {
        self.index_of(id)
            .and_then(|index| self.rows()[index].as_group())
            .and_then(LvGroup::points)
    }
}

/// Findings of „LV prüfen“ about the Datenpunkt-Abschnitte of a Kosten-LV:
/// the notes of each section (split ≠ 100 %, DP-Art without price …) and
/// Kostengruppen counted twice – as Kurztext position and in a section, or in
/// two sections.
pub fn point_section_findings(lv: &Lv) -> Vec<(RowId, String)> {
    let sections = lv.point_sections();
    let mut findings = Vec::new();
    let effective = lv.effective_cost_groups();
    // The Rechengrundlage of Aufteilungszeilen counts nothing itself.
    let bases = lv.split_bases();
    let name_of = |row: &LvRow| match row.ordinal().trim() {
        "" => row.short_text().trim().to_string(),
        ordinal => ordinal.to_string(),
    };
    for (number, &index) in sections.iter().enumerate() {
        let row = &lv.rows()[index];
        let Some(points) = row.as_group().and_then(LvGroup::points) else {
            continue;
        };
        let title = name_of(row);
        for warning in points.warnings() {
            findings.push((row.id(), format!("Datenpunkte {title}: {warning}")));
        }
        if number > 0 {
            findings.push((
                row.id(),
                format!(
                    "Datenpunkte {title}: KG {POINTS_GROUP} ist schon im Datenpunkt-Abschnitt {} erfasst – doppelt gezählt.",
                    name_of(&lv.rows()[sections[0]])
                ),
            ));
        }
        let covered = covered_codes(points);
        for (other, effective) in lv.rows().iter().zip(&effective) {
            let LvRow::Position(position) = other else {
                continue;
            };
            let Some((code, _)) = effective else {
                continue;
            };
            if position.counts_in_total()
                && !bases.contains(&position.id())
                && din_level(code).is_some()
                && overlaps(code, points, &covered)
            {
                findings.push((
                    other.id(),
                    format!(
                        "{}: KG {code} ist auch im Datenpunkt-Abschnitt {title} erfasst – doppelt gezählt.",
                        name_of(other)
                    ),
                ));
            }
        }
    }
    findings
}

impl Project {
    /// „In Kosten-LV übernehmen“: the standalone Datenpunktrechnung `id` becomes
    /// a Kosten-LV of its stage with exactly this section (title 480), number
    /// `number`; the standalone one is removed, so it counts once. Returns the
    /// index of the new LV.
    pub fn estimate_to_cost_lv(
        &mut self,
        id: CostEstimateId,
        number: &str,
    ) -> Result<usize, String> {
        let estimate = self
            .cost_estimate(id)
            .cloned()
            .ok_or_else(|| "Diese Kostenermittlung gibt es nicht.".to_string())?;
        let stage = estimate.stage;
        let title = match estimate.title.trim() {
            "" => format!("{} KG 480", stage.label()),
            title => title.to_string(),
        };
        let mut lv = cost_lv_template(stage, &title, &[POINTS_GROUP], 10);
        lv.set_number(number.trim());
        if let Some(LvRow::Note(note)) = lv.row_mut(0) {
            note.set_text(cost_lv_note(stage, &estimate.date, &estimate.remark));
        }
        let index = lv.insert_point_section()?;
        if let Some(LvRow::Group(group)) = lv.row_mut(index) {
            let mut points = estimate.clone();
            points.id = CostEstimateId(0);
            group.set_points(Some(points));
        }
        self.remove_cost_estimate(id);
        Ok(self.add_lv(lv))
    }
}

#[cfg(test)]
mod tests;
