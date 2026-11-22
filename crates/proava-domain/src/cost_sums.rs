//! Sums per Kostengruppe of all Kostenermittlungen of a project – the
//! interface for the Kostenverfolgung (Budget → Kostenschätzung →
//! Kostenberechnung → … → Kostenfeststellung).
//!
//! A project holds two kinds of Kostenermittlung:
//! - Kosten-LVs (Normalfall): an LV with [`LvPurpose::Cost`], Menge ×
//!   Kostenkennwert per position, KG from the position or its title;
//! - GA-Datenpunktrechnungen (Sonderfall): [`CostEstimate`], DP × Preis,
//!   Kostenschätzung = KG 480, Kostenberechnung = 481–489 by Aufteilung.
//!
//! [`Project::cost_sources`] lists every Kostenermittlung with its sums,
//! [`Project::cost_sums`] adds up all of one Stufe – e.g. a Kosten-LV for KG
//! 410–470 and the Datenpunktrechnung for KG 480 make the Kostenberechnung
//! KG 400 of a Generalunternehmer. Callers only see sources and sums: if a
//! Datenpunktrechnung later becomes a part of a Kosten-LV (a title „berechnet
//! über Datenpunkte“), only this module changes.
//!
//! Amounts are net (as entered), without Nachlass; codes are the KG at the
//! wanted level (`"400"`, `"480"`, `"481"`), `""` = ohne Kostengruppe. A
//! source that is coarser than the level keeps its own code (a Schätzung
//! „480“ stays „480“ at level 3).

use std::collections::BTreeMap;

use crate::cost_estimate::{CostEstimateId, CostStage};
use crate::cost_group::cost_group_report;
use crate::lv_purpose::LvPurpose;
use crate::money::Money;
use crate::project::Project;

/// Where the sums of a [`CostSource`] come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostSourceRef {
    /// Kosten-LV at this index of the project's LVs (valid until LVs are
    /// added, removed or moved).
    Lv(usize),
    /// GA-Datenpunktrechnung.
    Estimate(CostEstimateId),
}

/// One Kostenermittlung with its sums per Kostengruppe.
#[derive(Debug, Clone, PartialEq)]
pub struct CostSource {
    pub source: CostSourceRef,
    pub stage: CostStage,
    /// Title of the LV or of the Datenpunktrechnung.
    pub title: String,
    /// Stand (TT.MM.JJJJ) of a Datenpunktrechnung; empty for a Kosten-LV.
    pub date: String,
    /// Net amount per KG at the asked level; `""` = ohne Kostengruppe.
    pub sums: BTreeMap<String, Money>,
}

impl CostSource {
    pub fn total(&self) -> Money {
        self.sums.values().copied().sum()
    }
}

/// The Kostengruppe `code` at `level` (1–3) of DIN 276: „481“ → „400“,
/// „480“, „481“. A code of another shape stays as it is, a coarser one too
/// („480“ at level 3 stays „480“).
pub fn cost_group_at_level(code: &str, level: u8) -> String {
    let code = code.trim();
    let digits: Vec<char> = code.chars().take(3).collect();
    if digits.len() < 3 || !digits.iter().all(char::is_ascii_digit) {
        return code.to_string();
    }
    match level.clamp(1, 3) {
        1 => format!("{}00", digits[0]),
        2 => format!("{}{}0", digits[0], digits[1]),
        _ => digits.iter().collect(),
    }
}

impl Project {
    /// Every Kostenermittlung of the project with its sums per KG at `level`
    /// (1–3): first the Kosten-LVs in their order, then the
    /// Datenpunktrechnungen in theirs.
    pub fn cost_sources(&self, level: u8) -> Vec<CostSource> {
        let mut sources = Vec::new();
        for index in self.cost_lv_indices() {
            let Some(lv) = self.lv_at(index) else {
                continue;
            };
            let LvPurpose::Cost(stage) = lv.purpose() else {
                continue;
            };
            let mut sums = BTreeMap::new();
            for line in cost_group_report(lv, &[])
                .lines
                .iter()
                .filter(|line| !line.subtotal)
            {
                let code = cost_group_at_level(line.code.as_deref().unwrap_or(""), level);
                *sums.entry(code).or_insert(Money::ZERO) += line.planner;
            }
            sources.push(CostSource {
                source: CostSourceRef::Lv(index),
                stage,
                title: lv.title().to_string(),
                date: String::new(),
                sums,
            });
        }
        for estimate in self.cost_estimates() {
            let mut sums = BTreeMap::new();
            for group in estimate.result().groups {
                *sums
                    .entry(cost_group_at_level(&group.code, level))
                    .or_insert(Money::ZERO) += group.amount;
            }
            sources.push(CostSource {
                source: CostSourceRef::Estimate(estimate.id),
                stage: estimate.stage,
                title: estimate.display_name(),
                date: estimate.date.clone(),
                sums,
            });
        }
        sources
    }

    /// Net sums per KG at `level` (1–3) of all Kostenermittlungen of this
    /// Stufe, Kosten-LVs and Datenpunktrechnungen together.
    pub fn cost_sums(&self, stage: CostStage, level: u8) -> BTreeMap<String, Money> {
        let mut sums = BTreeMap::new();
        for source in self
            .cost_sources(level)
            .into_iter()
            .filter(|source| source.stage == stage)
        {
            for (code, amount) in source.sums {
                *sums.entry(code).or_insert(Money::ZERO) += amount;
            }
        }
        sums
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost_estimate::{CostEstimate, PointRow};
    use crate::lv::{Lv, LvGroup};
    use crate::oz_mask::OzMask;
    use crate::position::Position;
    use crate::project_name::ProjectName;
    use rust_decimal::Decimal;

    fn euro(value: i64) -> Money {
        Money::from_decimal(Decimal::from(value))
    }

    /// Kostenberechnung KG 420 as a Kurztext-LV: Rohrleitungen in KG 422
    /// (from the title), a Wärmepumpe with its own KG 421, one position
    /// without KG.
    fn heating(stage: CostStage) -> Lv {
        let mut lv = Lv::new(
            "Kostenberechnung Heizung",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        );
        lv.set_purpose(LvPurpose::Cost(stage));
        let mut group = LvGroup::new(0, "", "Wärmeverteilnetze");
        group.set_cost_group(Some("422".into()));
        lv.push_group(group);
        lv.push_position(Position::new(
            "",
            "Rohrleitung DN 25",
            Decimal::from(120),
            "m",
            Some(Decimal::from(50)),
        ));
        let mut pump = Position::new(
            "",
            "Wärmepumpe",
            Decimal::from(40),
            "kW",
            Some(Decimal::from(1500)),
        );
        pump.set_cost_group(Some("421".into()));
        lv.push_position(pump);
        lv.push_group(LvGroup::new(0, "", "Sonstiges"));
        lv.push_position(Position::new(
            "",
            "Inbetriebnahme",
            Decimal::ONE,
            "psch",
            Some(Decimal::from(2000)),
        ));
        lv
    }

    fn project() -> Project {
        let tender = {
            let mut lv = heating(CostStage::Calculation);
            lv.set_purpose(LvPurpose::Tender);
            lv
        };
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18",
            tender,
        );
        project.add_lv(heating(CostStage::Calculation));
        project.add_lv(heating(CostStage::Estimate));
        // GA-Datenpunktrechnung: 100 DP × 400 € on 481–489 by the template split.
        let mut calculation = CostEstimate::template(CostStage::Calculation, "18.11.2026", "");
        calculation.sections[0].rows[0] = PointRow {
            counts: vec![100, 0, 0, 0, 0, 0],
            ..PointRow::default()
        };
        project.add_cost_estimate(calculation);
        project
    }

    #[test]
    fn levels_of_din_276() {
        assert_eq!(cost_group_at_level("481", 1), "400");
        assert_eq!(cost_group_at_level("481", 2), "480");
        assert_eq!(cost_group_at_level("481", 3), "481");
        assert_eq!(cost_group_at_level(" 422 ", 9), "422");
        assert_eq!(cost_group_at_level("480", 3), "480");
        assert_eq!(cost_group_at_level("", 2), "");
        assert_eq!(cost_group_at_level("A12", 2), "A12");
    }

    #[test]
    fn sources_list_cost_lvs_and_point_calculations_not_tender_lvs() {
        let project = project();
        let sources = project.cost_sources(3);
        let kinds: Vec<(CostSourceRef, CostStage)> = sources
            .iter()
            .map(|source| (source.source, source.stage))
            .collect();
        assert_eq!(
            kinds,
            [
                (CostSourceRef::Lv(1), CostStage::Calculation),
                (CostSourceRef::Lv(2), CostStage::Estimate),
                (
                    CostSourceRef::Estimate(CostEstimateId(1)),
                    CostStage::Calculation
                ),
            ],
            "das Ausschreibungs-LV (0) ist keine Kostenermittlung"
        );
        assert_eq!(sources[0].sums["422"], euro(6000));
        assert_eq!(sources[0].sums["421"], euro(60_000));
        assert_eq!(sources[0].sums[""], euro(2000), "ohne Kostengruppe");
        assert_eq!(sources[0].total(), euro(68_000));
        assert_eq!(sources[2].date, "18.11.2026");
        assert_eq!(sources[2].total(), euro(40_000));
    }

    #[test]
    fn sums_of_a_stage_add_kurztext_and_points() {
        let project = project();
        let level3 = project.cost_sums(CostStage::Calculation, 3);
        assert_eq!(level3["421"], euro(60_000));
        assert_eq!(level3["481"], euro(16_000), "40 % von 40.000 €");
        assert_eq!(level3["484"], euro(12_000));
        let level2 = project.cost_sums(CostStage::Calculation, 2);
        assert_eq!(level2["420"], euro(66_000));
        assert_eq!(level2["480"], euro(40_000));
        assert_eq!(level2[""], euro(2000));
        let level1 = project.cost_sums(CostStage::Calculation, 1);
        assert_eq!(level1["400"], euro(106_000));
        assert_eq!(level1.values().copied().sum::<Money>(), euro(108_000));
        // The Schätzung has only its own Kosten-LV.
        assert_eq!(
            project.cost_sums(CostStage::Estimate, 1)["400"],
            euro(66_000)
        );
    }
}
