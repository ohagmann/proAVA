//! Positions added to or removed from an LV that already has bids or an
//! Auftrag (Anwender 19.11.): the bids read in and the Auftrag miss new
//! positions, removed ones stay in them. proAVA asks before such a change –
//! independent of whether the LV was sent from proAVA.

use std::collections::BTreeSet;

use crate::{Lv, Project, RowId};

/// Bids and Auftrag of one LV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BidsOrOrder {
    /// Bids read in (excluded ones too).
    pub bids: usize,
    /// An Auftrag: an awarded bid or a festgeschriebener Auftrag.
    pub order: bool,
}

impl BidsOrOrder {
    pub fn is_empty(&self) -> bool {
        self.bids == 0 && !self.order
    }

    /// „3 Angebote“, „ein Angebot“, „einen Auftrag“, „2 Angebote und einen Auftrag“.
    pub fn label(&self) -> String {
        let bids = match self.bids {
            0 => String::new(),
            1 => "ein Angebot".to_string(),
            many => format!("{many} Angebote"),
        };
        match (bids.is_empty(), self.order) {
            (true, true) => "einen Auftrag".to_string(),
            (false, true) => format!("{bids} und einen Auftrag"),
            _ => bids,
        }
    }
}

/// Positions of a change that the bids or the Auftrag do not match.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UncoveredPositions {
    /// New positions (not part of a Nachtrag): the bids and the Auftrag miss them.
    pub added: Vec<RowId>,
    /// Removed positions that a bid or the Auftrag contains.
    pub removed: Vec<RowId>,
}

impl UncoveredPositions {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// The verb of the question: „einfügen“, „löschen“ or „ändern“ (both).
    pub fn verb(&self) -> &'static str {
        match (self.added.is_empty(), self.removed.is_empty()) {
            (false, true) => "einfügen",
            (true, false) => "löschen",
            _ => "ändern",
        }
    }
}

/// The ids of all positions of an LV (the state before a change).
pub fn position_ids(lv: &Lv) -> BTreeSet<RowId> {
    lv.positions().map(|position| position.id()).collect()
}

impl Project {
    /// Bids and Auftrag of the LV at `index`.
    pub fn bids_or_order(&self, index: usize) -> BidsOrOrder {
        let bids = self.bids_of(index);
        BidsOrOrder {
            bids: bids.len(),
            order: self.contract_of(index).is_some() || bids.iter().any(|bid| bid.is_awarded()),
        }
    }

    /// Whether the LV at `index` has bids read in or an Auftrag – new or
    /// removed positions then leave them incomplete (other than [`Lv`]'s
    /// Versand, this does not depend on an export from proAVA).
    pub fn lv_has_bids_or_order(&self, index: usize) -> bool {
        !self.bids_or_order(index).is_empty()
    }

    /// The positions of the LV at `index` that a change since `before`
    /// (position ids) added or removed and that do not match the bids or the
    /// Auftrag: new positions that are no Nachtragsposition, and removed
    /// positions a bid or the Auftrag contains. Empty without bids and Auftrag.
    pub fn uncovered_positions(
        &self,
        index: usize,
        before: &BTreeSet<RowId>,
    ) -> UncoveredPositions {
        let Some(lv) = self.lv_at(index) else {
            return UncoveredPositions::default();
        };
        if !self.lv_has_bids_or_order(index) {
            return UncoveredPositions::default();
        }
        let now = position_ids(lv);
        let in_change_order = |id: &RowId| {
            self.change_orders_of(index)
                .iter()
                .any(|order| order.items.iter().any(|item| item.position == *id))
        };
        let contained = |id: &RowId| {
            self.bids_of(index)
                .iter()
                .any(|bid| bid.price(*id).is_some())
                || self
                    .contract_of(index)
                    .is_some_and(|contract| contract.item(*id).is_some())
        };
        UncoveredPositions {
            added: now
                .difference(before)
                .filter(|id| !in_change_order(id))
                .copied()
                .collect(),
            removed: before
                .difference(&now)
                .filter(|id| contained(id))
                .copied()
                .collect(),
        }
    }

    /// Why the change of the LV at `index` needs a question, e.g. „Das LV hat
    /// bereits 3 Angebote – 2 neue Positionen fehlen dort.“; with an Auftrag
    /// also the hint on a Nachtrag. None when nothing is uncovered.
    pub fn uncovered_note(&self, index: usize, uncovered: &UncoveredPositions) -> Option<String> {
        self.uncovered_note_for(index, uncovered, "Das LV", "gelöschte")
    }

    /// Like [`Project::uncovered_note`] with another `subject` („„LV 1 –
    /// Elektro“, aus dem die Zeilen kommen,“) and word for the removed
    /// positions („verschobene“).
    pub fn uncovered_note_for(
        &self,
        index: usize,
        uncovered: &UncoveredPositions,
        subject: &str,
        removed_word: &str,
    ) -> Option<String> {
        if uncovered.is_empty() {
            return None;
        }
        let has = self.bids_or_order(index);
        let mut parts = Vec::new();
        match uncovered.added.len() {
            0 => {}
            1 => parts.push("die neue Position fehlt dort".to_string()),
            many => parts.push(format!("{many} neue Positionen fehlen dort")),
        }
        match uncovered.removed.len() {
            0 => {}
            1 => parts.push(format!("die {removed_word} Position ist dort enthalten")),
            many => parts.push(format!(
                "{many} {removed_word} Positionen sind dort enthalten"
            )),
        }
        let mut text = format!(
            "{subject} hat bereits {} – {}.",
            has.label(),
            parts.join(", ")
        );
        if has.order {
            match (uncovered.added.is_empty(), uncovered.removed.is_empty()) {
                (false, true) => {
                    text.push_str(" Zusätzliche Leistungen gehören in einen Nachtrag.")
                }
                (true, false) => text
                    .push_str(" Entfallende Leistungen gehören als Mindermenge in einen Nachtrag."),
                _ => text.push_str(" Geänderte Leistungen gehören in einen Nachtrag."),
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests;
