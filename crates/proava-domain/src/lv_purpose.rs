//! What an LV is for: a Leistungsverzeichnis for the tender (Ausschreibung,
//! the normal case) or a Kostenermittlung nach DIN 276 in the form of a
//! Kurztext-LV (Anwender 18.11.: „für die anderen Gewerke ist es eigentlich
//! immer eine Kurztext-LV-ähnliche Berechnung“ – Menge × Kostenkennwert je
//! Bezugseinheit, gegliedert nach Kostengruppen).
//!
//! A cost LV lives in the project's list of LVs and is edited with the same
//! editor, but it never goes to bidders: no Bieter, Angebote, Preisspiegel,
//! Auftrag, Nachträge or Abrechnung. Everything that means „the LVs of the
//! tender“ uses [`Project::tender_lv_indices`]; the table with all LVs one
//! below the other shows [`Project::lv_indices_shown_with`].
//!
//! The GA-Datenpunktrechnung (Sonderfall) stays a
//! [`crate::cost_estimate::CostEstimate`]; [`crate::cost_sums`] adds both
//! kinds up per Stufe and Kostengruppe.

use crate::cost_estimate::CostStage;
use crate::project::Project;

/// Purpose of an LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LvPurpose {
    /// Leistungsverzeichnis for the tender (Ausschreibung) – the default and
    /// what every LV of an older project file is.
    #[default]
    Tender,
    /// Kostenermittlung nach DIN 276 of this Stufe as a Kurztext-LV.
    Cost(CostStage),
}

impl LvPurpose {
    pub fn is_tender(self) -> bool {
        matches!(self, LvPurpose::Tender)
    }

    /// The Stufe of a Kostenermittlung, `None` for a tender LV.
    pub fn cost_stage(self) -> Option<CostStage> {
        match self {
            LvPurpose::Tender => None,
            LvPurpose::Cost(stage) => Some(stage),
        }
    }

    /// „Ausschreibung“, „Kostenschätzung“, „Kostenberechnung“.
    pub fn label(self) -> &'static str {
        match self {
            LvPurpose::Tender => "Ausschreibung",
            LvPurpose::Cost(stage) => stage.label(),
        }
    }
}

impl Project {
    /// Indices of the LVs for the tender (Ausschreibung), in their order:
    /// the LVs that bids, Preisspiegel, Auftrag, Nachträge, Abrechnung and
    /// the Kostenvoranschlag (planner's prices) are about.
    pub fn tender_lv_indices(&self) -> Vec<usize> {
        self.lv_indices_where(LvPurpose::is_tender)
    }

    /// Indices of the LVs that are a Kostenermittlung, in their order.
    pub fn cost_lv_indices(&self) -> Vec<usize> {
        self.lv_indices_where(|purpose| !purpose.is_tender())
    }

    /// Purpose of the LV at `index` (tender for an index out of range).
    pub fn lv_purpose(&self, index: usize) -> LvPurpose {
        self.lv_at(index).map(|lv| lv.purpose()).unwrap_or_default()
    }

    /// The LVs to show together with the LV at `index` in one table (all LVs
    /// one below the other): with a tender LV all tender LVs, with a cost LV
    /// only that one – tender and cost LVs never mix, and two Kostenermittlungen
    /// (e.g. Schätzung and Berechnung of the same Gewerk) are not added up by eye.
    pub fn lv_indices_shown_with(&self, index: usize) -> Vec<usize> {
        if index >= self.lv_count() {
            return Vec::new();
        }
        if self.lv_purpose(index).is_tender() {
            self.tender_lv_indices()
        } else {
            vec![index]
        }
    }

    /// What of the tender the LV at `index` already has – Bieter, Angebote,
    /// Auftrag, Nachträge, Rechnungen –, e.g. „2 Angebote, Auftrag“; empty
    /// when none. Such an LV cannot become a Kostenermittlung.
    pub fn tender_data_of(&self, index: usize) -> String {
        let mut parts = Vec::new();
        let count = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        if !self.invitees_of(index).is_empty() {
            parts.push(count(self.invitees_of(index).len(), "Bieter", "Bieter"));
        }
        if !self.bids_of(index).is_empty() {
            parts.push(count(self.bids_of(index).len(), "Angebot", "Angebote"));
        }
        if self.contract_of(index).is_some() {
            parts.push("Auftrag".to_string());
        }
        if !self.change_orders_of(index).is_empty() {
            parts.push(count(
                self.change_orders_of(index).len(),
                "Nachtrag",
                "Nachträge",
            ));
        }
        let invoices = self
            .billing_of(index)
            .map(|billing| billing.invoices().len())
            .unwrap_or(0);
        if invoices > 0 {
            parts.push(count(invoices, "Rechnung", "Rechnungen"));
        }
        parts.join(", ")
    }

    fn lv_indices_where(&self, keep: impl Fn(LvPurpose) -> bool) -> Vec<usize> {
        self.lvs()
            .enumerate()
            .filter(|(_, lv)| keep(lv.purpose()))
            .map(|(index, _)| index)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv::Lv;
    use crate::oz_mask::OzMask;
    use crate::project_name::ProjectName;

    fn lv(title: &str, purpose: LvPurpose) -> Lv {
        let mut lv = Lv::new(title, OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.set_purpose(purpose);
        lv
    }

    fn project() -> Project {
        let mut project = Project::new(
            ProjectName::parse("Rathaus 3").unwrap(),
            "2026-11-18T00:00:00",
            lv("Heizung", LvPurpose::Tender),
        );
        project.add_lv(lv("KB Heizung", LvPurpose::Cost(CostStage::Calculation)));
        project.add_lv(lv("Lüftung", LvPurpose::Tender));
        project.add_lv(lv("KS Lüftung", LvPurpose::Cost(CostStage::Estimate)));
        project
    }

    #[test]
    fn a_new_lv_is_for_the_tender() {
        assert_eq!(
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()).purpose(),
            LvPurpose::Tender
        );
        assert_eq!(LvPurpose::default().label(), "Ausschreibung");
        assert_eq!(
            LvPurpose::Cost(CostStage::Calculation).label(),
            "Kostenberechnung"
        );
        assert_eq!(
            LvPurpose::Cost(CostStage::Estimate).cost_stage(),
            Some(CostStage::Estimate)
        );
    }

    #[test]
    fn tender_and_cost_lvs_are_kept_apart() {
        let project = project();
        assert_eq!(project.tender_lv_indices(), [0, 2]);
        assert_eq!(project.cost_lv_indices(), [1, 3]);
        assert_eq!(project.lv_indices_shown_with(0), [0, 2]);
        assert_eq!(project.lv_indices_shown_with(2), [0, 2]);
        assert_eq!(
            project.lv_indices_shown_with(1),
            [1],
            "ein Kosten-LV steht für sich"
        );
        assert_eq!(project.lv_indices_shown_with(3), [3]);
        assert!(project.lv_indices_shown_with(9).is_empty());
        assert_eq!(project.lv_purpose(9), LvPurpose::Tender);
        assert_eq!(project.tender_data_of(0), "");
    }

    #[test]
    fn tender_data_names_bids() {
        let mut project = project();
        project.add_bid_to(
            2,
            crate::bid::Bid::new(crate::bid::Address::named("Muster GmbH"), "a.x84"),
        );
        assert_eq!(project.tender_data_of(2), "1 Angebot");
        assert_eq!(project.tender_data_of(0), "");
    }
}
